#!/usr/bin/env python3
# bench/player.py BIN FILE SCENARIO [secs]  -> one JSON line (Mist Player: start, RSS, CPU, GPU, renders)
# scenarios: play | pause | min (minimized) | chrome (controls held visible)
# OLDAPI=1: the player before 2026-10-29 (no --test minimize/poke hooks). Restores ~/.config/mist/player.json.
import json, os, subprocess, sys, time, socket, shutil

BIN, FILE, SCEN = sys.argv[1], sys.argv[2], sys.argv[3]
SECS = float(sys.argv[4]) if len(sys.argv) > 4 else 10.0
RT = os.environ["XDG_RUNTIME_DIR"]
STATE = f"{RT}/mist/player-state.json"
CFG = os.path.expanduser("~/.config/mist/player.json")
HZ = os.sysconf("SC_CLK_TCK")


def test(*a):
    subprocess.run([BIN, "player", "--test", *a], timeout=5)


def dump():
    try:
        os.remove(STATE)
    except FileNotFoundError:
        pass
    test("dump")
    for _ in range(100):
        try:
            return json.load(open(STATE))
        except Exception:
            time.sleep(0.005)
    return {}


def ticks(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return int(f[11]) + int(f[12])


def rss(pid):
    for l in open(f"/proc/{pid}/status"):
        if l.startswith("VmRSS"):
            return int(l.split()[1]) // 1024
    return 0


def comp(cmd):
    s = socket.socket(socket.AF_UNIX)
    s.connect(f"{RT}/mist-comp/{os.environ['WAYLAND_DISPLAY']}.sock")
    s.sendall((cmd + "\n").encode())
    out = b""
    while True:
        b = s.recv(4096)
        if not b:
            break
        out += b
    return out.decode()


def comp_frames():
    return int(comp("stats").split()[1])


bak = CFG + ".measure-bak"
if os.path.exists(CFG) and not os.path.exists(bak):
    shutil.copy(CFG, bak)
cpid = int(subprocess.check_output(["pgrep", "-x", "mist-comp"]).split()[0])
subprocess.run(["pkill", "-f", "mist-ui-bin player"])
time.sleep(0.5)
t0 = time.monotonic()
p = subprocess.Popen([BIN, "player", FILE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
start = None
audio = FILE.endswith(".mp3")
while time.monotonic() - t0 < 10:
    time.sleep(0.01)
    d = dump()
    st = d.get("stat") or [0, 0, 0]
    if (audio and d.get("pos", 0) > 0) or (not audio and st[1] >= 1 and d.get("video")):
        start = (time.monotonic() - t0) * 1000
        break
time.sleep(4)
if SCEN == "pause":
    test("key", "space")
    time.sleep(1.5)
elif SCEN == "min":
    if not os.environ.get("OLDAPI"):
        test("minimize")
    else:
        test("key", "x")
        time.sleep(0.3)
        test("click", "45", "24")
    time.sleep(1.5)
d0 = dump()
c0, k0, f0, w0 = ticks(p.pid), ticks(cpid), comp_frames(), time.monotonic()
gpu = []
gpu_dec = []
pm = subprocess.Popen(["nvidia-smi", "pmon", "-c", str(int(SECS)), "-d", "1", "-s", "u"], stdout=subprocess.PIPE, text=True)
end = w0 + SECS
while time.monotonic() < end:
    if SCEN == "chrome":
        test("poke") if not os.environ.get("OLDAPI") else test("key", "x")
    time.sleep(0.5)
el = time.monotonic() - w0
for l in pm.communicate(timeout=20)[0].splitlines():
    f = l.split()
    if len(f) > 4 and f[1] == str(p.pid):
        gpu.append(0 if f[3] == "-" else int(f[3]))
        dec = f[6] if len(f) > 6 else "-"
        gpu_dec.append(0 if dec == "-" else int(dec))
d1 = dump()
c1, k1, f1 = ticks(p.pid), ticks(cpid), comp_frames()
s0, s1 = d0.get("stat", [0, 0, 0]), d1.get("stat", [0, 0, 0])
r = {
    "file": os.path.basename(FILE), "scen": SCEN, "start_ms": round(start) if start else None,
    "rss_mb": rss(p.pid), "cpu_pct": round((c1 - c0) / HZ / el * 100, 1), "comp_cpu_pct": round((k1 - k0) / HZ / el * 100, 1),
    "comp_fps": round((f1 - f0) / el, 1), "gpu_sm": round(sum(gpu) / max(len(gpu), 1), 1), "gpu_dec": round(sum(gpu_dec) / max(len(gpu_dec), 1), 1),
    "render_s": round((s1[0] - s0[0]) / el, 1), "frames_s": round((s1[1] - s0[1]) / el, 1), "ui_s": round((s1[2] - s0[2]) / el, 2),
    "hwdec": d1.get("hwdec"), "paused": d1.get("paused"), "cb_s": round((d1.get("callbacks", 0) - d0.get("callbacks", 0)) / el, 1), "first_ms": d1.get("first_frame_ms"), "gl": d1.get("gl"), "err": d1.get("err"),
}
print(json.dumps(r))
p.terminate()
try:
    p.wait(3)
except Exception:
    p.kill()
if os.path.exists(bak):
    shutil.copy(bak, CFG)
