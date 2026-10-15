#!/bin/sh
# windowrules.lua: an X11 game (steam_app*) opens fullscreen without a bar on the primary monitor (DP-3)
# even when the pointer is on the other one; its splash (no title) stays a normal window
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
MIST_COMP_XWAYLAND=1 MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" ./nest.sh >/dev/null
export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
timeout 10 sh -c "until grep -q 'xwayland ready' comp.log; do sleep 0.2; done" || { echo "no xwayland"; exit 1; }
./cc.sh motion 1900 400 >/dev/null
echo "pointer: $(hyprctl cursorpos)"
DISPLAY=$(grep -m1 '^DISPLAY=' comp.log | cut -d= -f2) WAYLAND_DISPLAY= setsid kitty --class steam_app_42 -T "Some Game" >client.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until hyprctl clients -j | grep -q steam_app_42; do sleep 0.2; done"
timeout 3 sh -c "until hyprctl clients -j | jq -e '.[0].fullscreen != 0' >/dev/null; do sleep 0.1; done"
echo "game: $(hyprctl clients -j | jq -c '.[0] | {class, at, size, fullscreen, monitor}')"
echo "vrr: $(hyprctl monitors -j | jq -c "[.[] | {name, vrr}]")"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
