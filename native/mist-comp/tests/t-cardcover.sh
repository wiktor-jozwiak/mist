#!/bin/sh
# the desktop weather card stops its carousel under a maximized window: card draws in 20 s, bare and covered
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
BIN=${MIST_UI_BIN:-$PWD/../../target/release/mist-ui}
M=$XDG_RUNTIME_DIR/t-cardcover; mkdir -p $M/mist; chmod 700 $M
OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
# its own runtime dir: no single-instance socket of the live shell; the Hyprland IPC of the nested comp
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 setsid $BIN >$M/ui.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-desktop-card; do sleep 0.2; done"; timeout 3 tail -f /dev/null
cards() { MIST_UI_SOCK=$M/ui.sock $BIN msg ui stats | grep -o '"card": [0-9]*' | grep -o '[0-9]*$'; }
a=$(cards); timeout 20 tail -f /dev/null; b=$(cards); echo "bare: card draws $((b - a)) / 20 s"
WAYLAND_DISPLAY=$S setsid kitty --class mist-cover sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-cover; do sleep 0.2; done"; ./cc.sh action maximize >/dev/null; timeout 3 tail -f /dev/null
a=$(cards); timeout 20 tail -f /dev/null; b=$(cards); echo "covered: card draws $((b - a)) / 20 s"
kill $(cat client.pid client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
