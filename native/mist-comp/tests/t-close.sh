#!/bin/sh
# close / minimize animations: the window's picture is still on screen right after it is gone
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
f() { ./cc.sh stats | cut -d' ' -f2; }
px() { WAYLAND_DISPLAY=$S grim -g "630,400 1x1" - | magick - -format '%[pixel:p{0,0}]' info:; }
for what in close minimize; do
  WAYLAND_DISPLAY=$S setsid kitty -o cursor_blink_interval=0 -o background=#ff0000 >/dev/null 2>&1 &
  timeout 10 sh -c "until ./cc.sh windows | grep -q 'kitty ws1 2'; do sleep 0.05; done"; timeout 0.5 tail -f /dev/null
  echo "$what before: $(px)"
  if [ $what = close ]; then ./cc.sh action close >/dev/null; timeout 5 sh -c "while ./cc.sh windows | grep -q kitty; do sleep 0.01; done"
  else HYPRLAND_INSTANCE_SIGNATURE=$(cat sig) hyprctl eval 'MistMinimize()' >/dev/null; fi
  a=$(f); echo "$what just after: $(px)"; timeout 0.6 tail -f /dev/null; b=$(f); echo "$what later: $(px)  frames $((b-a))"
done
./cc.sh windows; ./unnest.sh >/dev/null 2>&1
