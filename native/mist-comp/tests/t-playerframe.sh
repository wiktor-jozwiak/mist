#!/bin/sh
# Mist Player shows the picture, not only counts frames: a colour test video in the nested compositor, then a
# shot-win of the player window must hold its saturated colours (the UI is grey; a black video plane has none).
# The player runs with its own XDG_RUNTIME_DIR and HOME: never forwards to the real instance, never touches player.json.
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
UI=$(cd ../../target/release && pwd)/mist-ui
command -v ffmpeg >/dev/null || { echo "skip: no ffmpeg"; exit 0; }
T=$(mktemp -d); chmod 700 $T; mkdir -p $T/home
ffmpeg -loglevel error -f lavfi -i testsrc2=size=640x360:rate=30 -t 20 -pix_fmt yuv420p $T/colour.mkv || { echo "FAIL ffmpeg"; exit 1; }
./nest.sh >/dev/null
W=$XDG_RUNTIME_DIR/$(cat sock)
XDG_RUNTIME_DIR=$T HOME=$T/home WAYLAND_DISPLAY=$W setsid "$UI" player $T/colour.mkv >player.log 2>&1 & echo $! > client.pid
timeout 15 sh -c "until ./cc.sh windows | grep -q mist-player; do sleep 0.2; done" || { echo "FAIL no player window"; cat player.log; }
sleep 3
R=0
./cc.sh shot-win $PWD/playerframe.ppm mist-player | grep -q ok || { echo "FAIL shot-win reply"; R=1; }
python3 - "$PWD/playerframe.ppm" <<'P' || R=1
import sys
d = open(sys.argv[1], 'rb').read()
hdr = d.split(b'\n', 3); w, h = map(int, hdr[1].split()); px = hdr[3]
sat = sum(1 for i in range(0, len(px), 3) if max(px[i:i+3]) - min(px[i:i+3]) > 80)
share = sat / (w * h)
ok = share > 0.05
print(("ok  " if ok else "FAIL") + f" player frame {w}x{h}: {share:.0%} saturated pixels (the video plane)")
sys.exit(0 if ok else 1)
P
grep -q panicked comp.log player.log && { echo PANIC; R=1; }
kill $(cat client.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1; rm -rf $T
exit $R
