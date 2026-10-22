#!/bin/sh
# Super + right drag from the top-left quadrant of a CSD window (GTK, shadows around its geometry): the
# bottom-right corner stays put, the top-left follows the pointer exactly
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh env GDK_BACKEND=wayland python3 $PWD/popup.py >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q popup.py; do sleep 0.1; done"; sleep 0.3
w0=$(./cc.sh windows | grep popup.py); echo "before: $w0"
g=$(echo $w0 | awk '{print $3}'); s=$(echo $w0 | awk '{print $4}'); x=${g%,*}; y=${g#*,}; w=${s%x*}; h=${s#*x}
./cc.sh motion $((x + 20)) $((y + 20)) >/dev/null
./cc.sh key 125 press >/dev/null; ./cc.sh button right press >/dev/null
for i in 1 2 3 4 5; do ./cc.sh motion $((x + 20 - i * 10)) $((y + 20 - i * 10)) >/dev/null; sleep 0.05; done
./cc.sh button right release >/dev/null; ./cc.sh key 125 release >/dev/null; sleep 0.4
w1=$(./cc.sh windows | grep popup.py); echo "after:  $w1"
g=$(echo $w1 | awk '{print $3}'); s=$(echo $w1 | awk '{print $4}'); x1=${g%,*}; y1=${g#*,}; w1=${s%x*}; h1=${s#*x}
echo "corner: top-left $((x1 - x)),$((y1 - y)) (want -50,-50), bottom-right $((x1 + w1 - x - w)),$((y1 + h1 - y - h)) (want 0,0)"
[ $((x1 + w1)) -eq $((x + w)) ] && [ $((y1 + h1)) -eq $((y + h)) ] && echo "result: ok" || echo "FAIL: the window moved"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
