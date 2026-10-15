#!/bin/sh
# Hyprland bind parity: SUPER+wheel, workspace empty, movetoworkspace follows, CTRL+SHIFT wheel moves, special workspace
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
./nest.sh kitty --class mist-binds >/dev/null
export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-binds; do sleep 0.1; done"
ws() { ./cc.sh workspaces | tr '\n' ' '; }
echo "start: $(ws)"
./cc.sh key 125 press >/dev/null; ./cc.sh scroll 1 >/dev/null; ./cc.sh key 125 release >/dev/null
echo "super+wheel down (m-1, clamped): $(ws)"
./cc.sh key 125 press >/dev/null; ./cc.sh scroll -1 >/dev/null; ./cc.sh key 125 release >/dev/null
echo "super+wheel up (m+1): $(ws)"
./cc.sh scroll 1 >/dev/null
echo "plain wheel: $(ws)"
./cc.sh action workspace empty >/dev/null
echo "empty: $(ws)"
./cc.sh action workspace 1 >/dev/null; ./cc.sh focus >/dev/null
./cc.sh action movetoworkspace 3 >/dev/null
echo "move 3: $(ws) | $(./cc.sh windows)"
for k in 125 29 42; do ./cc.sh key $k press >/dev/null; done; ./cc.sh scroll 1 >/dev/null; ./cc.sh scroll -1 >/dev/null; ./cc.sh scroll -1 >/dev/null; for k in 42 29 125; do ./cc.sh key $k release >/dev/null; done
echo "super+ctrl+shift wheel (3 4 3 2): $(ws) | $(./cc.sh stats)"
./cc.sh action movetoworkspacesilent special >/dev/null
echo "silent special: $(ws) | stats $(./cc.sh stats)"
./cc.sh action togglespecial >/dev/null; timeout 0.5 tail -f /dev/null
echo "show special: stats $(./cc.sh stats)"
./cc.sh action togglespecial >/dev/null; timeout 0.5 tail -f /dev/null
echo "hide special: stats $(./cc.sh stats)"
grep -iE "cannot parse|unknown" comp.log | head -5
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
