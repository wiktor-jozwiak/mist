#!/bin/sh
# Windows taskbar parity: Win+D minimizes every shown window and brings them back (top one focused again);
# MistRestore(addr, shown ws) of a window on a hidden workspace moves it here instead of switching (dock click)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
./nest.sh >/dev/null
export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
for id in sd-a sd-b; do WAYLAND_DISPLAY=$(cat sock) BUSY_ID=$id BUSY_SIZE=300x200 BUSY_MS=200 setsid "$B" >/dev/null 2>&1 & echo $! >> client.pid; sleep 0.5; done
timeout 10 sh -c "until ./cc.sh windows | grep -q sd-b; do sleep 0.1; done"
echo "start: $(./cc.sh windows | tr '\n' ' ') | active $(hyprctl activewindow -j | jq -r .class)"
./cc.sh key 125 press >/dev/null; ./cc.sh key 32 press >/dev/null; ./cc.sh key 32 release >/dev/null; ./cc.sh key 125 release >/dev/null
echo "win+d: $(./cc.sh windows | tr '\n' ' ')"
./cc.sh action showdesktop >/dev/null
echo "again: $(./cc.sh windows | tr '\n' ' ') | active $(hyprctl activewindow -j | jq -r .class)"
# sd-a to workspace 2 (silent), then the dock's restore brings it back onto 1
A=$(hyprctl clients -j | jq -r '.[] | select(.class=="sd-a") | .address')
hyprctl eval "hl.dispatch(hl.dsp.focus({ window = \"address:$A\" }))" >/dev/null
./cc.sh action movetoworkspacesilent 2 >/dev/null
echo "sd-a on 2: $(./cc.sh windows | tr '\n' ' ') | ws $(./cc.sh workspaces)"
hyprctl eval "MistRestore('$A', 1)" >/dev/null
echo "dock restore: $(./cc.sh windows | tr '\n' ' ') | ws $(./cc.sh workspaces) | active $(hyprctl activewindow -j | jq -r .class)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
