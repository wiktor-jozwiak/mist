#!/bin/sh
# glass blur cost: a maximized window redraws a 64x64 square in its middle every frame (BUSY_DAMAGE) under
# mist-ui's full-monitor Spotlight glass, so the glass behind it changes every frame. Prints blur runs, the
# prof stage "blur" (MIST_COMP_BLUR_SYNC=1 waits for the GPU around each run: GPU time) and compositor CPU.
# usage: bench-blur.sh [seconds]    env: SYNC=0 (CPU side only), FULL=1 (no partial re-blur), CI=1 / SOFT=1 (Mesa llvmpipe)
T=$(cd "$(dirname "$0")/../../target/release" && pwd)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
[ -n "$CI$SOFT" ] && { unset __EGL_VENDOR_LIBRARY_FILENAMES; export MIST_COMP_SOFTWARE=1; }
B=$T/examples/busy
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example busy)
SECS=${1:-5}
M=$XDG_RUNTIME_DIR/t-blur; mkdir -p $M/mist; chmod 700 $M; BIN=${MIST_UI_BIN:-$T/mist-ui}
MIST_COMP_BLUR_FULL=${FULL:-0} MIST_COMP_BLUR_SYNC=${SYNC:-1} OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 setsid $BIN >$M/ui.log 2>&1 & echo $! > client.pid
WAYLAND_DISPLAY=$S BUSY_FRAME=1 BUSY_PAINT=1 BUSY_SIZE=1920x1080 BUSY_DAMAGE=64x64 setsid "$B" >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-busy && ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"
./cc.sh action maximize >/dev/null; sleep 1
MIST_UI_SOCK=$M/ui.sock $BIN msg mist toggle spotlight >/dev/null; sleep 1.5
runs() { ./cc.sh stats | sed 's/.*blur \([0-9]*\)\/.*/\1/'; }
fr() { ./cc.sh stats | awk '{print $2}'; }
./cc.sh prof >/dev/null
c0=$(awk "{print \$14+\$15}" /proc/$(cat comp.pid)/stat); a=$(runs); f=$(fr); sleep $SECS
b=$(runs); g=$(fr); c1=$(awk "{print \$14+\$15}" /proc/$(cat comp.pid)/stat)
p=$(./cc.sh prof | grep '^blur')
echo "${SECS} s: frames $((g - f)), blur runs $((b - a)), compositor CPU $((c1 - c0)) ticks"
echo "prof: $p"; ./cc.sh blurs
[ -n "$SHOT" ] && WAYLAND_DISPLAY=$S grim "$SHOT"
kill $(cat client.pid) $(cat client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
