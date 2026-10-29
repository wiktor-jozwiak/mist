#!/bin/sh
# IPC shot-win <out.ppm> <app_id>: one window alone, with what covers it left out (a second window on top)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
./nest.sh >/dev/null
WAYLAND_DISPLAY=$(cat sock) BUSY_ID=under BUSY_SIZE=300x200 BUSY_MS=100 setsid "$B" >/dev/null 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q under; do sleep 0.1; done"
WAYLAND_DISPLAY=$(cat sock) BUSY_ID=over BUSY_SIZE=600x500 BUSY_FRAME=1 BUSY_PAINT=1 setsid "$B" >/dev/null 2>&1 & echo $! > client3.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q over; do sleep 0.1; done"; sleep 0.3
R=0; ./cc.sh windows
./cc.sh shot-win $PWD/shotwin.ppm under | grep -q ok || { echo "FAIL shot-win reply"; R=1; }
# the busy client fills its buffer with 0x60: every pixel of the shot must be that grey
python3 - "$PWD/shotwin.ppm" <<'P' || R=1
import sys
d = open(sys.argv[1], 'rb').read()
hdr = d.split(b'\n', 3); w, h = map(int, hdr[1].split()); px = hdr[3]
grey = sum(1 for i in range(0, len(px), 3) if px[i:i+3] == b'\x60\x60\x60')
print(("ok  " if grey == w * h else "FAIL") + f" shot-win {w}x{h}, {grey} of {w*h} pixels the window's own")
sys.exit(0 if grey == w * h else 1)
P
./cc.sh shot-win $PWD/shotwin.ppm nosuch | grep -q 'error no window' && echo "ok   unknown app_id answers" || { echo "FAIL unknown app_id"; R=1; }
grep -q panicked comp.log && { echo PANIC; R=1; }
kill $(cat client3.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
exit $R
