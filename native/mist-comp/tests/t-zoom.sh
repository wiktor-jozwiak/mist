#!/bin/sh
# magnifier: `zoom in` scales the view around the pointer (the point under it stays), follows the pointer,
# `zoom off` is the plain desktop again (shots through IPC: the output as drawn; grim gets the unzoomed one)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock); T=$XDG_RUNTIME_DIR/mist-zoom.ppm
WAYLAND_DISPLAY=$S setsid kitty -o cursor_blink_interval=0 -o background=#ff0000 >/dev/null 2>&1 &
timeout 10 sh -c "until ./cc.sh windows | grep -q 'kitty ws1 2'; do sleep 0.05; done"; timeout 0.5 tail -f /dev/null
px() { ./cc.sh shot $T >/dev/null; magick $T -format "%[pixel:p{$1,$2}]" info:; }
./cc.sh motion 640 400 >/dev/null; timeout 0.2 tail -f /dev/null
echo "plain: below the window (640,760) $(px 640 760)"
./cc.sh action zoom in >/dev/null; ./cc.sh action zoom in >/dev/null; ./cc.sh action zoom in >/dev/null; timeout 0.2 tail -f /dev/null
echo "zoom 1.95: (640,760) $(px 640 760) | under pointer (640,400) $(px 640 400)"
# pointer 10 px inside the window's left edge: zoomed, the edge moves out to about 10 px outside it
x0=$(./cc.sh windows | grep kitty | sed 's/.* \([0-9]*\),[0-9]* .*/\1/')
./cc.sh motion $((x0 + 10)) 400 >/dev/null; timeout 0.2 tail -f /dev/null
echo "pointer at the left edge $x0: 5 px outside $(px $((x0 - 5)) 400) 15 px outside $(px $((x0 - 15)) 400)"
a=$(./cc.sh stats | cut -d' ' -f2); timeout 0.5 tail -f /dev/null; b=$(./cc.sh stats | cut -d' ' -f2); echo "frames zoomed + idle: $((b-a))"
./cc.sh action zoom off >/dev/null; timeout 0.2 tail -f /dev/null
echo "off: (640,760) $(px 640 760)"; grep -c 'zoom ' comp.log
rm -f $T; ./unnest.sh >/dev/null 2>&1
