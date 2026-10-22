#!/bin/sh
# partial re-blur matches a full one: a 64x64 square changes every frame under the Spotlight glass (partial
# runs, after the first, full one saw it grey), then the client stops; shot A. Spotlight closed and opened
# again (a new chain: a full run); shot B. A and B must match (REACH too small: a stale grey fringe).
# usage: t-blurpartial.sh    env: CI=1 / SOFT=1 (Mesa llvmpipe)
T=$(cd "$(dirname "$0")/../../target/release" && pwd)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
[ -n "$CI$SOFT" ] && { unset __EGL_VENDOR_LIBRARY_FILENAMES; export MIST_COMP_SOFTWARE=1; }
B=$T/examples/busy; O=${OUTDIR:-/tmp}
M=$XDG_RUNTIME_DIR/t-blurp; mkdir -p $M/mist; chmod 700 $M; BIN=${MIST_UI_BIN:-$T/mist-ui}
OUT=DP-3 SIZE=1920x1080 ./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
mkdir -p $M/hypr; ln -sfn $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE $M/hypr/
R=$XDG_RUNTIME_DIR; XDG_RUNTIME_DIR=$M WAYLAND_DISPLAY=$R/$S MIST_UI_SOCK=$M/ui.sock MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 setsid $BIN >$M/ui.log 2>&1 & echo $! > client.pid
WAYLAND_DISPLAY=$S BUSY_FRAME=1 BUSY_PAINT=1 BUSY_PAINT_DELAY=2500 BUSY_SIZE=1920x1080 BUSY_DAMAGE=64x64 BUSY_DAMAGE_AT=600,180 setsid "$B" >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-busy && ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"
./cc.sh action maximize >/dev/null; sleep 1
ui() { MIST_UI_SOCK=$M/ui.sock $BIN msg mist toggle spotlight >/dev/null; }
ui; sleep 3
kill -STOP $(cat client2.pid); sleep 0.5
[ -n "$DEBUG" ] && ./cc.sh blurs; p=$(./cc.sh blurs | awk '/partial/ && $3 > n {n = $3} END {print n + 0}')
WAYLAND_DISPLAY=$S grim $O/blurp-a.png
./cc.sh blur-reset >/dev/null; ui; sleep 1; ui; sleep 2
WAYLAND_DISPLAY=$S grim $O/blurp-b.png; [ -n "$DEBUG" ] && ./cc.sh blurs
kill -CONT $(cat client2.pid); kill $(cat client.pid) $(cat client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
# around the square (the bar's clock may tick between the shots)
for s in a b; do magick $O/blurp-$s.png -crop 640x320+600+220 $O/blurp-$s-crop.png; done
d=$(compare -metric AE -fuzz 0.5% $O/blurp-a-crop.png $O/blurp-b-crop.png null: 2>&1 | cut -d' ' -f1)
echo "partial runs $p, pixels differing (>0.5%) $d"
[ "$p" -gt 10 ] && [ "${d%.*}" -lt 20 ] && echo PASS || { echo FAIL; exit 1; }
