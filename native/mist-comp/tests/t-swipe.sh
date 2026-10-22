#!/bin/sh
# 1:1 workspace swipe: the slide follows the fingers (no frames while they rest), lifting past a third
# finishes it, a short swipe slides the old workspace back (cc.sh swipe = what libinput's gesture drives)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
WAYLAND_DISPLAY=$S setsid kitty -o cursor_blink_interval=0 -o background=#ff0000 >/dev/null 2>&1 &
timeout 10 sh -c "until ./cc.sh windows | grep -q 'kitty ws1 2'; do sleep 0.05; done"; timeout 0.5 tail -f /dev/null
px() { WAYLAND_DISPLAY=$S grim -g "$1,400 1x1" - | magick - -format '%[pixel:p{0,0}]' info:; }
f() { ./cc.sh stats | cut -d' ' -f2; }
echo "start: x300 $(px 300) x900 $(px 900)"
./cc.sh swipe begin >/dev/null; for i in 1 2 3 4 5; do ./cc.sh swipe -30 >/dev/null; done; timeout 0.2 tail -f /dev/null
echo "held at half: ws $(./cc.sh workspaces | tr '\n' ' ')| x300 $(px 300) x900 $(px 900)"
a=$(f); timeout 0.5 tail -f /dev/null; b=$(f); echo "frames while resting: $((b-a))"
./cc.sh swipe end >/dev/null; timeout 0.5 tail -f /dev/null
echo "finished: ws $(./cc.sh workspaces | tr '\n' ' ')| x300 $(px 300)"
./cc.sh action workspace 1 >/dev/null; timeout 0.5 tail -f /dev/null
./cc.sh swipe begin >/dev/null; ./cc.sh swipe -30 >/dev/null; ./cc.sh swipe -30 >/dev/null; ./cc.sh swipe 2 >/dev/null
./cc.sh swipe end >/dev/null; timeout 0.5 tail -f /dev/null
echo "short swipe back: ws $(./cc.sh workspaces | tr '\n' ' ')| x640 $(px 640)"
./unnest.sh >/dev/null 2>&1
