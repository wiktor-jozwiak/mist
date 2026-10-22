#!/bin/sh
# drag and drop: the client's drag icon follows the pointer, and goes away at the drop
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1; S=$(mktemp -d)
./nest.sh env GDK_BACKEND=wayland python3 $PWD/dnd.py >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q dnd.py; do sleep 0.1; done"; sleep 0.4
g=$(./cc.sh windows | grep dnd.py | grep -oE "[-0-9]+,[-0-9]+" | head -1); s=$(./cc.sh windows | grep dnd.py | grep -oE "[0-9]+x[0-9]+" | head -1)
cx=$(( ${g%,*} + ${s%x*} / 2 )); cy=$(( ${g#*,} + ${s#*x} / 2 ))
c() { ./cc.sh "$@" >/dev/null; }
c motion $cx $cy; c button left press
for i in 1 2 3 4 5 6 7 8; do c motion $((cx + i * 20)) $((cy + i * 10)); sleep 0.05; done
x=$((cx + 160)); y=$((cy + 80))
timeout 3 sh -c "until grep -q 'drag begin' client.log; do sleep 0.1; done" || echo "FAIL: no drag started"
sleep 0.3; c shot $S/drag.ppm
px() { magick $1 -format "%[pixel:p{$2,$3}]" info:; }
echo "during drag at +5,+5 of the pointer: $(px $S/drag.ppm $((x + 5)) $((y + 5)))"
[ "$(px $S/drag.ppm $((x + 5)) $((y + 5)))" = "srgb(255,0,0)" ] || echo "FAIL: no drag icon under the pointer"
c button left release; sleep 0.4; c motion $((x + 100)) $((y + 100)); sleep 0.2; c shot $S/after.ppm
echo "after the drop: $(px $S/after.ppm $((x + 5)) $((y + 5)))"
[ "$(px $S/after.ppm $((x + 5)) $((y + 5)))" = "srgb(255,0,0)" ] && echo "FAIL: icon left behind"
grep -q panicked comp.log && echo PANIC
echo "result: done"; rm -rf $S
./unnest.sh >/dev/null 2>&1
