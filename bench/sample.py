#!/usr/bin/env python3
"""Cost of a process tree over a time window: memory (PSS, USS, RSS), CPU, wake-ups, GPU memory.

    sample.py SECS ROOT_PID [ROOT_PID...]      -> one JSON line on stdout
    sample.py --tree ROOT_PID...               -> the pids of the trees (for stopping them)

Every descendant of the root pids counts (helpers a shell spawns: mistd, playerctld, python workers).
- PSS: shared pages split between the processes that map them (smaps_rollup), so trees add up honestly.
- USS: private pages only (Private_Clean + Private_Dirty): what killing the tree would free.
- CPU: run time of every thread (/proc/<pid>/task/*/schedstat, ns) over the window, % of one core.
- wake-ups: context switches (voluntary + involuntary) of every thread per second; an idle program that
  polls shows here even when its CPU time rounds to zero.
- GPU memory: what nvidia-smi attributes to the pids (graphics + compute), MiB; 0 without NVIDIA.
Processes that start or end inside the window count from zero / until they ended.
"""
import json, os, subprocess, sys, time


def children():
    kids = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/stat") as f:
                s = f.read()
            ppid = int(s[s.rindex(")") + 2:].split()[1])
            kids.setdefault(ppid, []).append(int(d))
        except (OSError, ValueError):
            pass
    return kids


def tree(roots):
    kids, out, todo = children(), set(), list(roots)
    while todo:
        p = todo.pop()
        if p in out or not os.path.exists(f"/proc/{p}"):
            continue
        out.add(p)
        todo += kids.get(p, [])
    return out


def counters(pids):
    """per pid: (cpu ns, context switches) summed over its threads"""
    res = {}
    for p in pids:
        ns = cs = 0
        try:
            for t in os.listdir(f"/proc/{p}/task"):
                try:
                    with open(f"/proc/{p}/task/{t}/schedstat") as f:
                        ns += int(f.read().split()[0])
                    with open(f"/proc/{p}/task/{t}/status") as f:
                        for line in f:
                            if line.startswith(("voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")):
                                cs += int(line.split()[1])
                except (OSError, ValueError, IndexError):
                    pass
            res[p] = (ns, cs)
        except OSError:
            pass
    return res


def gpu_mib():
    """pid -> MiB from nvidia-smi's process table (it lists graphics clients, the CSV query only compute)"""
    try:
        out = subprocess.run(["nvidia-smi"], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.TimeoutExpired):
        return {}
    res = {}
    for line in out.splitlines():
        f = line.strip("| ").split()
        if len(f) >= 6 and f[-1].endswith("MiB") and f[3].isdigit():
            res[int(f[3])] = res.get(int(f[3]), 0) + int(f[-1][:-3])
    return res


def mem(p):
    m = {}
    try:
        with open(f"/proc/{p}/smaps_rollup") as f:
            for line in f:
                k, _, v = line.partition(":")
                if v.strip().endswith("kB"):
                    m[k] = int(v.split()[0])
    except OSError:
        return None
    return m


def main():
    if sys.argv[1] == "--tree":
        print(" ".join(map(str, sorted(tree([int(x) for x in sys.argv[2:]])))))
        return
    secs = float(sys.argv[1])
    roots = [int(x) for x in sys.argv[2:]]
    p0 = tree(roots)
    c0, t0 = counters(p0), time.monotonic()
    time.sleep(secs)
    p1 = tree(roots)
    c1, t1 = counters(p1), time.monotonic()
    dt = t1 - t0
    gpu = gpu_mib()
    procs, tot = [], {"pss": 0, "uss": 0, "rss": 0, "cpu": 0.0, "wake": 0.0, "gpu": 0, "threads": 0}
    for p in sorted(p0 | p1):
        a, b = c0.get(p, (0, 0)), c1.get(p, c0.get(p, (0, 0)))
        m = mem(p) if p in p1 else None
        try:
            with open(f"/proc/{p}/comm") as f:
                comm = f.read().strip()
            thr = len(os.listdir(f"/proc/{p}/task"))
        except OSError:
            comm, thr = "?", 0
        r = {
            "pid": p, "comm": comm, "threads": thr,
            "pss": (m or {}).get("Pss", 0) / 1024, "rss": (m or {}).get("Rss", 0) / 1024,
            "uss": ((m or {}).get("Private_Clean", 0) + (m or {}).get("Private_Dirty", 0)) / 1024,
            "cpu": 100 * (b[0] - a[0]) / 1e9 / dt, "wake": (b[1] - a[1]) / dt, "gpu": gpu.get(p, 0),
        }
        procs.append(r)
        for k in tot:
            tot[k] += r[k]
    print(json.dumps({"secs": round(dt, 2), "total": {k: round(v, 2) for k, v in tot.items()},
                      "procs": [{k: (round(v, 2) if isinstance(v, float) else v) for k, v in r.items()} for r in procs]}))


main()
