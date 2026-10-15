#!/bin/sh
# nested: kitty, then mist-ui-bin lock: desktop hidden, keyboard on the lock surface
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 80 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
OUT=DP-3 $D/nest.sh kitty --class mist-lk -e sleep 60
wait_for "$D/cc.sh windows | grep -q 'mist-lk ws1 [0-9]*,[0-9]* [1-9]'"
echo "before: $($D/cc.sh locked) focus=$($D/cc.sh focus)"
mkdir -p -m 700 $XDG_RUNTIME_DIR/mrt; ln -sfn $XDG_RUNTIME_DIR/$(cat $D/sock) $XDG_RUNTIME_DIR/mrt/$(cat $D/sock)
XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR/mrt WAYLAND_DISPLAY=$(cat $D/sock) setsid ~/.local/bin/mist-ui-bin lock >$D/lock.log 2>&1 & echo $! > $D/lock.pid
wait_for "$D/cc.sh locked | grep -q 'true surfaces=1'"
sleep 1
echo "locked: $($D/cc.sh locked) focus=$($D/cc.sh focus)"
$D/cc.sh shot $D/lock.ppm >/dev/null; magick $D/lock.ppm -resize 25% $D/lock.png
kill $(cat $D/lock.pid); tail -3 $D/lock.log
