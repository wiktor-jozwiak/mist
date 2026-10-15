#!/bin/sh
# nested: Super+drag moves a window, Super+Q closes it (synthetic input over IPC)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 50 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
$D/nest.sh kitty --class mist-in -e sleep 60
wait_for "$D/cc.sh windows | grep -q 'mist-in ws1 213,148 853'"
$D/cc.sh key 125 press >/dev/null; $D/cc.sh motion 400 300 >/dev/null; $D/cc.sh button left press >/dev/null
$D/cc.sh motion 500 350 >/dev/null; $D/cc.sh button left release >/dev/null; $D/cc.sh key 125 release >/dev/null
echo "moved: $($D/cc.sh windows)"
$D/cc.sh key 125 press >/dev/null; $D/cc.sh key 16 press >/dev/null; $D/cc.sh key 16 release >/dev/null; $D/cc.sh key 125 release >/dev/null
wait_for "! $D/cc.sh windows | grep -q mist-in" && echo "closed by Super+Q"
