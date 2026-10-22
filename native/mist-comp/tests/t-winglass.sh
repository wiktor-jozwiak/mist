#!/bin/sh
# window glass: colour pattern (maximized) behind a translucent window with app id "zen"; grim with and without MIST_COMP_BLUR
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D
run() {
  ./unnest.sh >/dev/null 2>&1
  SIZE=1920x1080 OUT=DP-3 ./nest.sh >/dev/null
  S=$(cat sock)
  WAYLAND_DISPLAY=$S setsid kitty --class mist-pattern -o font_size=40 -o background=#808080 -o cursor_blink_interval=0 $PWD/pattern.sh >/dev/null 2>&1 & echo $! > client2.pid
  timeout 10 sh -c "until ./cc.sh windows | grep -q mist-pattern; do sleep 0.2; done"
  ./cc.sh action maximize >/dev/null
  WAYLAND_DISPLAY=$S setsid kitty --class zen -o background_opacity=0.3 -o hide_window_decorations=yes -o cursor_blink_interval=0 sh -c 'sleep 30' >/dev/null 2>&1 & echo $! > client3.pid
  timeout 10 sh -c "until ./cc.sh windows | grep -q ' zen\|zen '; do sleep 0.2; done"; timeout 1.5 tail -f /dev/null
  ./cc.sh windows
  WAYLAND_DISPLAY=$S grim winglass-$1.png
  a=$(./cc.sh stats); timeout 2 tail -f /dev/null; echo "$1 idle: $a -> $(./cc.sh stats)  rss/cpu: $(ps -o rss=,pcpu= -p $(cat comp.pid))"
  kill $(cat client2.pid) $(cat client3.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
}
MIST_COMP_BLUR=0 run off; run on
echo "AE: $(magick compare -metric AE -fuzz 2% winglass-off.png winglass-on.png null: 2>&1)"
# the shadow: a strip 8-40 px under the window (365,151 1190x777) is darker with glass on
for m in off on; do echo "under $m: $(magick winglass-$m.png -crop 1100x32+410+936 +repage -format '%[fx:mean]' info:)"; done
