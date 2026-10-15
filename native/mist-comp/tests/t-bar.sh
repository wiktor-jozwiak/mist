#!/bin/sh
# title bar: hover symbols, green = maximize, double click = restore, drag, yellow = minimize, red = close
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh kitty >/dev/null; S=$(cat sock)
timeout 10 sh -c "until ./cc.sh windows | grep -q kitty; do sleep 0.1; done"
c() { ./cc.sh "$@" >/dev/null; }
w() { ./cc.sh windows | head -1; }
G=$(./cc.sh windows | grep kitty | awk '{print $3}'); X=${G%,*}; Y=${G#*,}; BY=$((Y - 15))
c motion $((X + 39)) $BY; WAYLAND_DISPLAY=$S grim -g "$X,$((Y - 30)) 80x30" hov.png
c motion $((X + 61)) $BY; c button left press; c button left release
timeout 5 sh -c 'until ./cc.sh windows | grep -q " 1264x"; do sleep 0.1; done'; echo "max:  $(w)"
c motion 600 15; c button left press; c button left release; c button left press; c button left release
timeout 5 sh -c "until ./cc.sh windows | grep -q ' $X,$Y '; do sleep 0.1; done"; echo "dbl:  $(w)"
c motion $((X + 300)) $BY; c button left press; c motion $((X + 400)) $((BY + 100)); c button left release; echo "drag: $(w)"
c motion $((X + 139)) $((BY + 100)); c button left press; c button left release; echo "min:  $(w)"
grep -q panicked comp.log && echo PANIC
