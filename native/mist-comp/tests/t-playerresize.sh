#!/bin/sh
# Mist Player resized by its bottom-right corner at mouse pace (a motion every 2 ms, a configure each): the
# window keeps up with the pointer, ends at the pointer's size within 0.5 s, and the video plane is fitted to it
# (a draw per configure fell seconds behind: the window stood still during the drag).
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
UI=$(cd ../../target/release && pwd)/mist-ui
command -v ffmpeg >/dev/null || { echo "skip: no ffmpeg"; exit 0; }
T=$(mktemp -d); chmod 700 $T; mkdir -p $T/home
ffmpeg -loglevel error -f lavfi -i testsrc2=size=640x360:rate=30 -t 30 -pix_fmt yuv420p $T/colour.mkv || { echo "FAIL ffmpeg"; exit 1; }
SIZE=1920x1080 ./nest.sh >/dev/null
W=$XDG_RUNTIME_DIR/$(cat sock)
XDG_RUNTIME_DIR=$T HOME=$T/home WAYLAND_DISPLAY=$W setsid "$UI" player $T/colour.mkv >player.log 2>&1 & echo $! > client.pid
timeout 15 sh -c "until ./cc.sh windows | grep -q mist-player; do sleep 0.2; done" || { echo "FAIL no player window"; cat player.log; }
sleep 2
g=$(./cc.sh windows | grep mist-player | awk '{print $3, $4}' | tr ',x' '  ')
set -- $g; x=$1; y=$2; w=$3; h=$4
R=0
python3 - $((x + w - 3)) $((y + h - 3)) $w $h "$PWD/playerresize.ppm" <<'P' || R=1
import socket, os, sys, time
ex, ey, w0, h0 = map(int, sys.argv[1:5]); shot = sys.argv[5]
path = os.environ['XDG_RUNTIME_DIR'] + '/mist-comp/' + open('sock').read().strip() + '.sock'
def cc(c):
    s = socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect(path); s.sendall((c + '\n').encode()); o = b''
    try:
        while (b := s.recv(4096)): o += b
    except Exception: pass
    s.close(); return o.decode()
def size():
    l = [l for l in cc('windows').splitlines() if 'mist-player' in l][0]
    return tuple(map(int, l.split()[3].split('x')))
cc(f'motion {ex} {ey}'); time.sleep(0.1); cc('button left press')
behind = 0
for i in range(1, 401):
    cc(f'motion {ex + i} {ey - i // 2}'); time.sleep(0.002)
    if i % 50 == 0:
        behind = max(behind, w0 + i - size()[0])
cc('button left release')
want = (w0 + 400, h0 - 200); t = time.time()
while size() != want and time.time() - t < 3: time.sleep(0.005)
late = (time.time() - t) * 1000
ok = size() == want and late < 500 and behind < 120
print(("ok  " if ok else "FAIL") + f" resize: at most {behind}px behind the pointer, {size()[0]}x{size()[1]} (want {want[0]}x{want[1]}) {late:.0f} ms after the release")
time.sleep(0.3); cc(f'shot-win {shot} mist-player')
d = open(shot, 'rb').read(); hdr = d.split(b'\n', 3); W, H = map(int, hdr[1].split()); px = hdr[3]
sat = lambda x, y: (lambda p: max(p) - min(p) > 80)(px[(y * W + x) * 3:(y * W + x) * 3 + 3])
xs = [x for x in range(W) if sat(x, H // 2)]
fit = H * 16 / 9 if W / H > 16 / 9 else W
got = xs[-1] - xs[0] + 1 if xs else 0
vok = abs(got - fit) <= 3
print(("ok  " if vok else "FAIL") + f" video plane {got}px wide in {W}x{H} (want {fit:.0f}: 16:9 fitted)")
sys.exit(0 if ok and vok else 1)
P
grep -q panicked comp.log player.log && { echo PANIC; R=1; }
kill $(cat client.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1; rm -rf $T
exit $R
