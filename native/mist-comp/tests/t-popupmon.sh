#!/bin/sh
# a menu opened at the right edge of a window on the second monitor stays on that monitor (flipped left)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" ./nest.sh env GDK_BACKEND=wayland python3 $PWD/popup.py >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q popup.py; do sleep 0.1; done"
./cc.sh action movetomonitor 1 >/dev/null; sleep 0.3
w=$(./cc.sh windows | grep popup.py); echo "window: $w"
g=$(echo $w | awk '{print $3}'); s=$(echo $w | awk '{print $4}')
# right-click 5 px inside the window's right edge, near its bottom: the menu cannot open right / down
x=$(( ${g%,*} + ${s%x*} - 5 )); y=$(( ${g#*,} + ${s#*x} - 20 ))
./cc.sh motion $x $y >/dev/null; ./cc.sh button right press >/dev/null; ./cc.sh button right release >/dev/null
timeout 5 sh -c "until grep -q 'menu shown' client.log; do sleep 0.1; done" || echo "FAIL: no menu"
sleep 0.3; p=$(./cc.sh popups | head -1); echo "menu at click $x,$y: $p"
pg=$(echo $p | awk '{print $2}'); ps=$(echo $p | awk '{print $3}')
px=${pg%,*}; py=${pg#*,}; pw=${ps%x*}; ph=${ps#*x}
if [ -n "$p" ] && [ $px -ge 1280 ] && [ $((px + pw)) -le 2560 ] && [ $py -ge 0 ] && [ $((py + ph)) -le 800 ]; then
  echo "result: menu inside HDMI-A-1 (1280,0 1280x800)"
else echo "FAIL: menu outside HDMI-A-1: $p"; fi
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
