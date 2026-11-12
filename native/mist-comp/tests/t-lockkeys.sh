#!/bin/sh
# nested: Caps / Num Lock reach the shell as a `mistlock>>CAPS,NUM` event on socket2 (the state at connect,
# then one per change), so nothing has to poll the keyboard LEDs
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 50 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
$D/nest.sh kitty --class mist-lk1 -e sleep 60
export HYPRLAND_INSTANCE_SIGNATURE=$(cat $D/sig)
wait_for "$D/cc.sh windows | grep -q 'mist-lk1 ws1 [0-9]*,[0-9]* [1-9]'"
timeout 4 socat -u UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock - > $D/ev.txt &
sleep 0.5
tap() { $D/cc.sh key $1 press >/dev/null; $D/cc.sh key $1 release >/dev/null; sleep 0.2; }
tap 58; tap 30; tap 58; tap 69; tap 69
wait
echo "lock events: $(grep '^mistlock' $D/ev.txt | tr '\n' ' ')"
