#!/bin/sh
# a fullscreen window on top hides the shell's chrome (bar, dock, island; any top layer) on its output:
# distinct colours in the bar strip and the dock strip, normal (chrome shown) vs fullscreen (one colour) vs back
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
BIN=${MIST_UI_BIN:-$PWD/../../target/release/mist-ui}
M=$XDG_RUNTIME_DIR/t-fullchrome; mkdir -p $M/mist; chmod 700 $M
OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 setsid $BIN >$M/ui.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"
WAYLAND_DISPLAY=$S setsid kitty --class mist-full sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-full; do sleep 0.2; done"
cols() { WAYLAND_DISPLAY=$S grim -g "$1" - | magick - -format '%k' info:; }
shot() { timeout 1.5 tail -f /dev/null; WAYLAND_DISPLAY=$S grim $M/$1.png; echo "$1: $(./cc.sh windows | grep mist-full | cut -c1-80)
   bar $(cols '0,0 1920x30') dock $(cols '660,1020 600x60') colours"; }
shot normal
./cc.sh action fullscreen >/dev/null; shot fullscreen
./cc.sh layers | grep -c mist- | sed 's/^/layers still mapped: /'
./cc.sh action fullscreen >/dev/null; shot restored
grep -q panicked comp.log && echo PANIC
kill $(cat client.pid client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
