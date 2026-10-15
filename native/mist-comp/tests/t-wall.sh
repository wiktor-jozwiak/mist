#!/bin/sh
# mist-wall (GL, layer background) under mist-comp: maps, renders, idle frame rate, paused under a maximized window
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
SIZE=1920x1080 OUT=DP-3 ./nest.sh >/dev/null
S=$(cat sock)
mkdir -p -m 700 $XDG_RUNTIME_DIR/mrt; ln -sfn $XDG_RUNTIME_DIR/$S $XDG_RUNTIME_DIR/mrt/$S; ln -sfn $XDG_RUNTIME_DIR/hypr $XDG_RUNTIME_DIR/mrt/hypr
HYPRLAND_INSTANCE_SIGNATURE=$(cat sig) XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR/mrt WAYLAND_DISPLAY=$S setsid ~/.local/bin/mist-wall-bin >wall.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh layers | grep -qi wall; do sleep 0.2; done"; ./cc.sh layers
WAYLAND_DISPLAY=$S grim -s 0.125 wall.png && magick wall.png -format '%[fx:mean]\n' info:
fps() { a=$(./cc.sh stats | cut -d' ' -f2); timeout 2 tail -f /dev/null; b=$(./cc.sh stats | cut -d' ' -f2); echo $(( (b-a)/2 )); }
echo "wall alone: $(fps) fps"
WAYLAND_DISPLAY=$S setsid kitty >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q kitty; do sleep 0.2; done"
./cc.sh action maximize >/dev/null
timeout 5 sh -c "until ./cc.sh windows | grep -q ' 1920x'; do sleep 0.1; done"
echo "under maximized kitty: $(fps) fps (anim + kitty cursor blink)"
P=$(cat client.pid); t() { awk '{print $14+$15}' /proc/$P/stat; }; a=$(t); timeout 2 tail -f /dev/null; echo "mist-wall cpu ticks/2s under kitty: $(( $(t)-a ))"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
