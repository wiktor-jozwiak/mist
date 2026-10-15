#!/bin/sh
# two monitors: workspaces 1-3 left, 4-6 right (workspaces.lua), SUPER+N follows, m+1 stays on the monitor
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" ./nest.sh kitty --class mist-mon >/dev/null
export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-mon; do sleep 0.1; done"
echo "start: $(./cc.sh workspaces | tr '\n' ' ') | $(./cc.sh windows)"
./cc.sh action workspace 5 >/dev/null
echo "ws5:   $(./cc.sh workspaces | tr '\n' ' ') | cursor $(hyprctl cursorpos)"
./cc.sh action workspace +1 >/dev/null; ./cc.sh action workspace +1 >/dev/null
echo "+1+1:  $(./cc.sh workspaces | tr '\n' ' ')"
./cc.sh action workspace 2 >/dev/null
echo "ws2:   $(./cc.sh workspaces | tr '\n' ' ') | cursor $(hyprctl cursorpos)"
./cc.sh action workspace 1 >/dev/null; ./cc.sh focus >/dev/null
./cc.sh action movetomonitor 1 >/dev/null
echo "moved: $(./cc.sh windows)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
