#!/bin/sh
# decor.rs DRAG_TOP: Spotify never moves itself; a drag in its top strip moves the window, a click there or a
# drag lower in the window does not, and motion after the release does nothing
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
./nest.sh >/dev/null
WAYLAND_DISPLAY=$(cat sock) BUSY_ID=spotify BUSY_SIZE=600x400 BUSY_MS=200 setsid "$B" >/dev/null 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q spotify; do sleep 0.1; done"
pos() { ./cc.sh windows | awk '/spotify/ {print $3}'; }
drag() { # x y dx dy
    ./cc.sh motion $1 $2 >/dev/null; ./cc.sh button left press >/dev/null
    for i in 1 2 3 4 5; do ./cc.sh motion $(($1 + $3 * i / 5)) $(($2 + $4 * i / 5)) >/dev/null; done
    ./cc.sh button left release >/dev/null
}
P=$(pos); X=${P%,*}; Y=${P#*,}
echo "start: $P"
./cc.sh motion $((X + 300)) $((Y + 20)) >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh button left release >/dev/null
./cc.sh motion $((X + 350)) $((Y + 70)) >/dev/null
echo "click in strip, then motion (same): $(pos)"
drag $((X + 300)) $((Y + 200)) 50 50
echo "drag lower (same): $(pos)"
drag $((X + 300)) $((Y + 20)) 50 40
echo "drag in strip (+50,+40): $(pos)"
./cc.sh motion 100 100 >/dev/null
echo "motion after release (same): $(pos)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
