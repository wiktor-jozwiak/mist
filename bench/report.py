#!/usr/bin/env python3
"""Markdown tables from bench/shells.sh and bench/compositors.sh results: median over the runs, range in
brackets.   report.py bench/results/shells-<date>.jsonl [bench/results/compositors-<date>.jsonl]"""
import json, statistics, sys
from collections import defaultdict


def med(xs):
    return statistics.median(xs)


def cell(xs, fmt, bold=False):
    m = fmt % med(xs)
    if bold:
        m = f"**{m}**"
    lo, hi = fmt % min(xs), fmt % max(xs)
    return m if lo == hi else f"{m} ({lo}–{hi})"


def shells(rows):
    by = defaultdict(list)
    order = []
    for d in rows:
        k = (d["shell"], d["scenario"])
        if d["shell"] not in order:
            order.append(d["shell"])
        by[k].append(d)
    print("| Shell | Version | PSS idle | USS idle | GPU memory | CPU idle | wake-ups idle | CPU active | wake-ups active |")
    print("|---|---|---|---|---|---|---|---|---|")
    for s in order:
        i, a = by[(s, "idle")], by[(s, "active")]
        if not i or not a:
            continue
        t = lambda ds, k: [d["total"][k] for d in ds]
        b = s == "Mist"
        print(f"| {'**Mist**' if b else s} | {i[0]['version']} | {cell(t(i, 'pss'), '%.0f MB', b)} | {cell(t(i, 'uss'), '%.0f MB')} "
              f"| {cell(t(i, 'gpu'), '%.0f MiB')} | {cell(t(i, 'cpu'), '%.2f %%')} | {cell(t(i, 'wake'), '%.0f/s')} "
              f"| {cell(t(a, 'cpu'), '%.2f %%')} | {cell(t(a, 'wake'), '%.0f/s')} |")
    print(f"\n{len(by[(order[0], 'idle')])} runs each. Per process (idle, median PSS / CPU):\n")
    for s in order:
        per = defaultdict(lambda: ([], []))
        for d in by[(s, "idle")]:
            seen = defaultdict(lambda: [0.0, 0.0])
            for p in d["procs"]:
                seen[p["comm"]][0] += p["pss"]
                seen[p["comm"]][1] += p["cpu"]
            for c, (pss, cpu) in seen.items():
                per[c][0].append(pss)
                per[c][1].append(cpu)
        parts = [f"{c} {med(v[0]):.1f} MB / {med(v[1]):.2f} %" for c, v in sorted(per.items(), key=lambda x: -med(x[1][0])) if med(v[0]) >= 0.5]
        print(f"- {s}: " + ", ".join(parts))


def compositors(rows):
    by = defaultdict(list)
    order = []
    for d in rows:
        if d["comp"] not in order:
            order.append(d["comp"])
        by[(d["comp"], d["scenario"])].append(d)
    print("| Compositor | Version | PSS idle | GPU memory | CPU idle | CPU, shm client | CPU, GL client | CPU per GL frame | GL client fps |")
    print("|---|---|---|---|---|---|---|---|---|")
    for c in order:
        i, s, e = by[(c, "idle")], by[(c, "shm")], by[(c, "egl")]
        if not (i and s and e):
            continue
        t = lambda ds, k: [d["total"][k] for d in ds]
        per = [10 * d["total"]["cpu"] / d["fps"] for d in e if d.get("fps")]  # ms of CPU per frame
        b = c == "mist-comp"
        print(f"| {'**mist-comp**' if b else c} | {i[0]['version']} | {cell(t(i, 'pss'), '%.0f MB', b)} | {cell(t(e, 'gpu'), '%.0f MiB')} "
              f"| {cell(t(i, 'cpu'), '%.2f %%')} | {cell(t(s, 'cpu'), '%.2f %%')} | {cell(t(e, 'cpu'), '%.2f %%')} "
              f"| {cell(per, '%.3f ms') if per else '–'} | {cell([d['fps'] for d in e if d.get('fps')], '%.0f') if per else '–'} |")
    print(f"\n{len(by[(order[0], 'idle')])} runs each.")


for path in sys.argv[1:]:
    rows = [json.loads(l) for l in open(path) if l.strip()]
    (shells if "shell" in rows[0] else compositors)(rows)
    print()
