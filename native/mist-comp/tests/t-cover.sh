#!/bin/sh
# mistcover events: a maximized window covers the desktop layers (wallpaper, card), restoring it uncovers them
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
M=$XDG_RUNTIME_DIR/t-cover; mkdir -p $M/mist; chmod 700 $M
OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 setsid ${MIST_UI_BIN:-$PWD/../../target/release/mist-ui} >$M/ui.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-desktop-card; do sleep 0.2; done"
socat -u UNIX:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock - > $M/ev.txt & echo $! > fake.pid
sleep 0.5
WAYLAND_DISPLAY=$S setsid kitty --class mist-cover sh -c 'sleep 60' >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-cover; do sleep 0.2; done"; sleep 0.3
echo "-- maximize"; ./cc.sh action maximize >/dev/null; sleep 0.5
echo "-- restore"; ./cc.sh action maximize >/dev/null; sleep 0.5
kill $(cat fake.pid client.pid client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
grep mistcover $M/ev.txt
grep -q "mistcover>>DP-3,mist-desktop-card,1,1" $M/ev.txt && grep -q "mistcover>>DP-3,mist-desktop-card,0,0" $M/ev.txt && echo "result: ok" || echo "result: FAIL"
