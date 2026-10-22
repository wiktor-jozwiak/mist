#!/bin/sh
# mist-ui full-monitor pane cost: spotlight open, 12 letters typed; paint ms and commit µs per pane frame
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
M=$XDG_RUNTIME_DIR/t-pane; mkdir -p $M/mist; chmod 700 $M; BIN=${MIST_UI_BIN:-$PWD/../../target/release/mist-ui}
OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 setsid $BIN >$M/ui.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"; sleep 1
st() { MIST_UI_SOCK=$M/ui.sock $BIN msg ui stats | tr -d '{}"' | tr ',' '\n' | grep -E "^ ?(pane|pane_ms|commit_us|commits|swap_px):" | tr -d ' ' | tr '\n' ' '; }
MIST_UI_SOCK=$M/ui.sock $BIN msg mist toggle spotlight >/dev/null; sleep 1.5
a=$(st)
for k in 30 31 32 33 34 35 36 37 38 30 31 32; do ./cc.sh key $k press >/dev/null; ./cc.sh key $k release >/dev/null; sleep 0.25; done; sleep 1
b=$(st); echo "before: $a"; echo "after:  $b"
WAYLAND_DISPLAY=$S grim ${SHOT:-/dev/null} 2>/dev/null; c0=$(./cc.sh stats | awk '{print $2}'); echo "comp frames: $c0"
kill $(cat client.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
