#!/bin/sh
# hit.rs: a subsurface placed below its parent (Mist Player's video plane) never takes the pointer from the
# parent drawn over it (smithay 0.7 kept the lowest hit); expect `enter main`
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
./nest.sh >/dev/null
WAYLAND_DISPLAY=$(cat sock) BUSY_ID=subinput BUSY_SIZE=400x300 BUSY_MS=100 BUSY_SUB=1 setsid "$B" >client.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q subinput; do sleep 0.1; done"
P=$(./cc.sh windows | awk '/subinput/ {print $3}'); X=${P%,*}; Y=${P#*,}
./cc.sh motion $((X + 200)) $((Y + 150)) >/dev/null; sleep 0.3
if grep -q 'enter main' client.log && ! grep -q 'enter sub' client.log; then echo "ok   pointer on the parent"; R=0; else echo "FAIL pointer: $(grep enter client.log | tr '\n' ' ')"; R=1; fi
grep -q panicked comp.log && { echo PANIC; R=1; }
./unnest.sh >/dev/null 2>&1
exit $R
