#!/bin/sh
# popup grab: a right-click menu takes the keys (Down + Return picks an item) and a click outside closes it
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh env GDK_BACKEND=wayland python3 $PWD/popup.py >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q popup.py; do sleep 0.1; done"
g=$(./cc.sh windows | grep popup.py | awk '{print $3}'); x=$(( ${g%,*} + 100 )); y=$(( ${g#*,} + 100 ))
echo "window: $(./cc.sh windows | grep popup.py)"
menu() { ./cc.sh motion $x $y >/dev/null; ./cc.sh button right press >/dev/null; ./cc.sh button right release >/dev/null
  timeout 5 sh -c "until [ \$(grep -c 'menu shown' client.log) -ge $1 ]; do sleep 0.1; done" || echo "FAIL: menu $1 not shown"; sleep 0.3; }
menu 1
echo "focus with menu open: $(./cc.sh focus)"
for k in 108 28; do ./cc.sh key $k press >/dev/null; ./cc.sh key $k release >/dev/null; sleep 0.15; done   # Down, Return
timeout 3 sh -c "until grep -q activate client.log; do sleep 0.1; done" || echo "FAIL: keys never reached the menu"
echo "keys: $(grep activate client.log | tr '\n' ' ')"
menu 2
./cc.sh motion 1200 760 >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh button left release >/dev/null
timeout 3 sh -c "until [ \$(grep -c 'menu closed' client.log) -ge 2 ]; do sleep 0.1; done" || echo "FAIL: click outside left the menu open"
echo "outside click: closed $(grep -c 'menu closed' client.log)x, focus $(./cc.sh focus)"
grep -q panicked comp.log && echo PANIC
echo "result: $(grep -q FAIL /dev/null; grep -c 'menu closed' client.log) menus closed, $(grep -c activate client.log) activated"
./unnest.sh >/dev/null 2>&1
