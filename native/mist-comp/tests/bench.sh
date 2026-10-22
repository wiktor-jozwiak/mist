#!/bin/sh
# compositor cost per frame: a busy client (kitty printing nonstop) over a maximized pattern; CPU ticks of
# mist-comp per rendered frame, with the app id given (zen = glass + shadow, kitty = server-side bar)
# usage: bench.sh [app-id] [seconds]
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D
APP=${1:-zen}; T=${2:-8}
./unnest.sh >/dev/null 2>&1
SIZE=1920x1080 OUT=DP-3 ./nest.sh >/dev/null
S=$(cat sock)
WAYLAND_DISPLAY=$S setsid kitty --class mist-pattern -o font_size=40 -o background=#808080 -o cursor_blink_interval=0 $PWD/pattern.sh >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-pattern; do sleep 0.2; done"
./cc.sh action maximize >/dev/null
WAYLAND_DISPLAY=$S setsid kitty --class $APP -o background_opacity=0.6 -o cursor_blink_interval=0 sh -c 'yes "mist bench $(date)"' >/dev/null 2>&1 & echo $! > client3.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q \"^$APP \"; do sleep 0.2; done"; timeout 2 tail -f /dev/null
P=$(cat comp.pid); c0=$(awk '{print $14+$15}' /proc/$P/stat); f0=$(./cc.sh stats | awk '{print $2}')
timeout $T tail -f /dev/null
c1=$(awk '{print $14+$15}' /proc/$P/stat); f1=$(./cc.sh stats | awk '{print $2}')
echo "$APP: frames $((f1-f0)) in ${T}s, cpu $((c1-c0)) ticks, $(awk "BEGIN{printf \"%.3f\", ($c1-$c0)*10/($f1-$f0)}") ms/frame, rss $(ps -o rss= -p $P) kB"
kill $(cat client2.pid) $(cat client3.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
