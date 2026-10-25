#!/bin/sh
# `workspaces vertical`: fingers up bring the next workspace from below and the slide follows them (red window
# half way up at the half); sideways does nothing
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
printf 'workspaces vertical\n' > vert.conf; MIST_COMP_CONFIG=$(pwd)/vert.conf ./nest.sh >/dev/null; S=$(cat sock)
WAYLAND_DISPLAY=$S setsid kitty -o cursor_blink_interval=0 -o background=#ff0000 >/dev/null 2>&1 &
timeout 10 sh -c "until ./cc.sh windows | grep -q 'kitty ws1 2'; do sleep 0.05; done"; timeout 0.5 tail -f /dev/null
px() { WAYLAND_DISPLAY=$S grim -g "640,$1 1x1" - | magick - -format '%[pixel:p{0,0}]' info:; }
echo "start: y200 $(px 200) y700 $(px 700)"
./cc.sh swipe begin >/dev/null; for i in 1 2 3 4 5; do ./cc.sh swipe 0 -30 >/dev/null; done; timeout 0.2 tail -f /dev/null
echo "held at half: ws $(./cc.sh workspaces | tr '\n' ' ')| y200 $(px 200) y700 $(px 700)"
./cc.sh swipe end >/dev/null; timeout 0.5 tail -f /dev/null
echo "finished: ws $(./cc.sh workspaces | tr '\n' ' ')| y200 $(px 200)"
./cc.sh swipe begin >/dev/null; for i in 1 2 3 4 5 6; do ./cc.sh swipe -40 0 >/dev/null; done; ./cc.sh swipe end >/dev/null; timeout 0.5 tail -f /dev/null
echo "sideways: ws $(./cc.sh workspaces | tr '\n' ' ')"
./unnest.sh >/dev/null 2>&1; rm -f vert.conf
