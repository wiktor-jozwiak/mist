#!/bin/sh
# snapswipe.rs: a 4-finger swipe snaps the focused window like Super+arrows; a short one does nothing
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
./nest.sh >/dev/null
WAYLAND_DISPLAY=$(cat sock) BUSY_ID=snapme BUSY_SIZE=400x300 BUSY_MS=100 setsid "$B" >/dev/null 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q snapme; do sleep 0.1; done"
geo() { ./cc.sh windows | awk '/snapme/ {print $3, $4}'; }
swipe() { ./cc.sh swipe4 begin >/dev/null; for i in 1 2 3 4; do ./cc.sh swipe4 $(($1 / 4)) $(($2 / 4)) >/dev/null; done; ./cc.sh swipe4 end >/dev/null; sleep 0.6; }
R=0; check() { if [ "$2" = "$3" ]; then echo "ok   $1: $2"; else echo "FAIL $1: $2 (want $3)"; R=1; fi; }
P0=$(geo); ./cc.sh motion 640 400 >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh button left release >/dev/null
swipe 20 0; check "short swipe" "$(geo)" "$P0"
# Super+arrow semantics (snap.rs): left half; right from there restores; right again the right half; left restores, then up maximizes
# (busy keeps its 400x300 buffer: the position tells)
swipe -200 0; L=$(geo); swipe 300 0; M=$(geo); swipe 300 0; RG=$(geo); swipe -300 0; swipe 0 -200; U=$(geo)
echo "left: $L | right: $M | right: $RG | up: $U"
check "left half" "${L%% *}" "8,8"
check "right restores" "$M" "$P0"
[ "${RG%%,*}" -gt 600 ] && echo "ok   right half: $RG" || { echo "FAIL right half: $RG"; R=1; }
[ "$U" != "$P0" ] && echo "ok   up from restored: $U" || { echo "FAIL up from restored: $U"; R=1; }
grep -q panicked comp.log && { echo PANIC; R=1; }
./unnest.sh >/dev/null 2>&1
exit $R
