#!/bin/sh
# glass: colour pattern window (maximized) behind mist-ui Spotlight; grim with and without MIST_COMP_BLUR
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D
run() {
  ./unnest.sh >/dev/null 2>&1
  U=$XDG_RUNTIME_DIR/mist/ui-test.sock
  SIZE=1920x1080 OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh ~/.local/bin/mist-ui-bin >/dev/null
  S=$(cat sock)
  WAYLAND_DISPLAY=$S setsid kitty --class mist-pattern -o font_size=40 -o cursor_blink_interval=0 -o initial_window_width=1904 -o initial_window_height=1000 $PWD/pattern.sh >/dev/null 2>&1 & echo $! > client2.pid
  timeout 10 sh -c "until ./cc.sh windows | grep -q mist-pattern && ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"
  ./cc.sh action maximize >/dev/null
  MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg mist toggle spotlight >/dev/null
  timeout 5 sh -c "until ./cc.sh layers | grep -q mist-spotlight; do sleep 0.1; done"; timeout 1 tail -f /dev/null
  ./cc.sh layers | grep spotlight
  WAYLAND_DISPLAY=$S grim glass-$1.png
  a=$(./cc.sh stats); timeout 2 tail -f /dev/null; echo "$1 idle: $a -> $(./cc.sh stats)  rss/cpu: $(ps -o rss=,pcpu= -p $(cat comp.pid))"
  kill $(cat client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
}
MIST_COMP_BLUR=0 run off; run on
echo "AE: $(magick compare -metric AE -fuzz 2% glass-off.png glass-on.png null: 2>&1)"
