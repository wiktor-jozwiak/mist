#!/bin/sh
# animations: frames while a window opens / a workspace slides, none when idle
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
f() { ./cc.sh stats | cut -d' ' -f2; }
WAYLAND_DISPLAY=$S setsid kitty -o cursor_blink_interval=0 -o background=#ff0000 >/dev/null 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q kitty; do sleep 0.02; done"; a=$(f)
WAYLAND_DISPLAY=$S grim -g "500,400 4x4" mid.png
timeout 1 tail -f /dev/null; b=$(f); timeout 1 tail -f /dev/null; c=$(f)
echo "open: $((b-a)) frames in 1s, then idle $((c-b))"; magick mid.png -format 'mid-open px %[pixel:p{1,1}]\n' info:
./cc.sh action "workspace 2" >/dev/null; timeout 1 tail -f /dev/null; d=$(f); timeout 1 tail -f /dev/null; e=$(f)
echo "ws: $((d-c)) frames, then idle $((e-d))"
WAYLAND_DISPLAY=$S grim -g "500,400 4x4" end.png; magick end.png -format 'ws2 px %[pixel:p{1,1}]\n' info:
./unnest.sh >/dev/null 2>&1
