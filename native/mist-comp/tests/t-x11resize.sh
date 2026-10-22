#!/bin/sh
# an X11 window resizing / moving itself from its own frame (_NET_WM_MOVERESIZE: Steam, Discord)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
MIST_COMP_XWAYLAND=1 ./nest.sh >/dev/null
timeout 10 sh -c "until grep -q 'xwayland ready' comp.log; do sleep 0.2; done" || { echo "no xwayland"; exit 1; }
DISPLAY=$(grep -m1 '^DISPLAY=' comp.log | cut -d= -f2) WAYLAND_DISPLAY= GDK_BACKEND=x11 setsid python3 $PWD/xresize.py >client.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows 2>/dev/null | grep -q ws; do sleep 0.2; done"; sleep 0.5
c() { ./cc.sh "$@" >/dev/null; }
w0=$(./cc.sh windows); echo "start:  $w0"
g=$(echo $w0 | grep -oE "[-0-9]+,[-0-9]+" | head -1); x=${g%,*}; y=${g#*,}
c motion $((x + 200)) $((y + 150)); c button left press; sleep 0.3
for i in 1 2 3 4; do c motion $((x + 200 + i * 15)) $((y + 150 + i * 10)); sleep 0.05; done
c button left release; sleep 0.4
w1=$(./cc.sh windows); echo "resize: $w1"
[ "$(echo $w1 | grep -oE "[0-9]+x[0-9]+" | head -1)" = "460x340" ] && echo "resize ok" || echo "FAIL: resize (want 460x340)"
g=$(echo $w1 | grep -oE "[-0-9]+,[-0-9]+" | head -1); x=${g%,*}; y=${g#*,}
c motion $((x + 100)) $((y + 100)); c button middle press; sleep 0.3
for i in 1 2 3 4 5; do c motion $((x + 100 + i * 10)) $((y + 100 + i * 10)); sleep 0.05; done
c button middle release; sleep 0.3; c motion $((x + 300)) $((y + 300)); sleep 0.2
w2=$(./cc.sh windows); echo "move:   $w2"
g2=$(echo $w2 | grep -oE "[-0-9]+,[-0-9]+" | head -1)
[ "$g2" = "$((x + 50)),$((y + 50))" ] && echo "result: ok" || echo "FAIL: move (want $((x + 50)),$((y + 50)), the drag must end on release)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
