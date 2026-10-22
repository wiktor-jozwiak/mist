#!/bin/sh
# an app that draws its own title bar (app id com.anthropic.Claude): no Mist bar, four rounded corners, the
# window shadow; a barred kitty keeps its bar and rounded bottom corners
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D
./unnest.sh >/dev/null 2>&1
SIZE=1280x800 ./nest.sh >/dev/null
S=$(cat sock)
WAYLAND_DISPLAY=$S setsid kitty --class mist-pattern -o background=#808080 -o cursor_blink_interval=0 sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-pattern; do sleep 0.2; done"
./cc.sh action maximize >/dev/null
WAYLAND_DISPLAY=$S setsid kitty --class com.anthropic.Claude -o background=#ffffff -o cursor_blink_interval=0 sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client3.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q anthropic; do sleep 0.2; done"; timeout 1.5 tail -f /dev/null
./cc.sh windows
WAYLAND_DISPLAY=$S grim round.png
# the window's rect from the list; corner pixel (1,1) inside it must show the grey behind, the centre white
set -- $(./cc.sh windows | grep anthropic | sed 's/.* \([0-9]*\),\([0-9]*\) \([0-9]*\)x\([0-9]*\).*/\1 \2 \3 \4/')
x=$1; y=$2; w=$3; h=$4
px() { magick round.png -format "%[pixel:p{$1,$2}]" info:; }
echo "bar: $(./cc.sh windows | grep -c 'anthropic.*bar')"
echo "corner tl: $(px $((x+1)) $((y+1)))  tr: $(px $((x+w-2)) $((y+1)))  bl: $(px $((x+1)) $((y+h-2)))  centre: $(px $((x+w/2)) $((y+h/2)))"
echo "shadow below: $(px $((x+w/2)) $((y+h+12)))  far: $(px 5 795)"
kill $(cat client3.pid) 2>/dev/null
# a barred kitty: bottom corners rounded, the window under its bar square
WAYLAND_DISPLAY=$S setsid kitty --class plain -o background=#ffffff -o cursor_blink_interval=0 sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client3.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q '^plain'; do sleep 0.2; done"; timeout 1.5 tail -f /dev/null
WAYLAND_DISPLAY=$S grim round.png
set -- $(./cc.sh windows | grep '^plain' | sed 's/.* \([0-9]*\),\([0-9]*\) \([0-9]*\)x\([0-9]*\).*/\1 \2 \3 \4/')
x=$1; y=$2; w=$3; h=$4
echo "barred: $(./cc.sh windows | grep -c '^plain.*bar')  top-left under bar: $(px $((x+1)) $((y+1)))  bottom-left: $(px $((x+1)) $((y+h-2)))"
kill $(cat client2.pid) $(cat client3.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
