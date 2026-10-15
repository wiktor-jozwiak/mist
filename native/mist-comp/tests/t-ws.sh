#!/bin/sh
# nested: two kitty windows on workspaces 1 and 2, switch back and forth, maximize
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 50 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
$D/nest.sh kitty --class mist-ws-a -e sleep 60
wait_for "$D/cc.sh windows | grep -q 'mist-ws-a ws1 [0-9]'"
$D/cc.sh action workspace 2 >/dev/null
WAYLAND_DISPLAY=$(cat $D/sock) setsid kitty --class mist-ws-b -e sleep 60 >/dev/null 2>&1 & echo $! > $D/client2.pid
wait_for "$D/cc.sh windows | grep -q 'mist-ws-b ws2 [0-9]'"
echo "--- on ws2"; $D/cc.sh windows; $D/cc.sh workspaces
$D/cc.sh action workspace 1 >/dev/null
echo "--- back on ws1"; $D/cc.sh windows; echo "focus: $($D/cc.sh focus)"
$D/cc.sh action maximize >/dev/null
wait_for "$D/cc.sh windows | grep -q 'mist-ws-a ws1 0,30 1280x770'"
echo "--- maximized"; $D/cc.sh windows | head -1
$D/cc.sh action maximize >/dev/null
wait_for "$D/cc.sh windows | grep -q 'mist-ws-a ws1 213,148'"
echo "--- restored"; $D/cc.sh windows | head -1
kill $(cat $D/client2.pid)
