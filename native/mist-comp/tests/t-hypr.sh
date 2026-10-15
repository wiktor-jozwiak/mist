#!/bin/sh
# nested: Hyprland-compatible IPC (hyprctl queries, eval subset, socket2 events)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 50 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
$D/nest.sh kitty --class mist-h1 -e sleep 60
export HYPRLAND_INSTANCE_SIGNATURE=$(cat $D/sig)
wait_for "$D/cc.sh windows | grep -q 'mist-h1 ws1 [0-9]*,[0-9]* [1-9]'"
timeout 6 socat -u UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock - > $D/ev.txt &
A=$(hyprctl clients -j | jq -r '.[0].address'); echo "clients: $(hyprctl clients -j | jq -c '[.[] | {address,class,workspace:.workspace.id,at,size}]')"
echo "monitors: $(hyprctl monitors -j | jq -c '[.[] | {name,width,height,activeWorkspace:.activeWorkspace.id,reserved}]')"
echo "activewindow: $(hyprctl activewindow -j | jq -c '{class,address}')"
hyprctl eval "MistMinimize('$A')" >/dev/null
echo "after minimize: $(hyprctl clients -j | jq -c '[.[] | {class,ws:.workspace.name,hidden}]') focus=$($D/cc.sh focus)"
hyprctl eval "MistRestore('$A', 1)" >/dev/null
echo "after restore: $(hyprctl clients -j | jq -c '[.[] | {class,ws:.workspace.name,hidden}]') focus=$($D/cc.sh focus)"
hyprctl eval 'hl.dispatch(hl.dsp.focus({ workspace = "m+1" }))' >/dev/null
echo "activeworkspace: $(hyprctl activeworkspace -j | jq -c .)"
hyprctl eval "hl.dispatch(hl.dsp.focus({ window = \"address:$A\" }))" >/dev/null
echo "focus by address switched back: $(hyprctl activeworkspace -j | jq -c .id)"
hyprctl eval "hl.dispatch(hl.dsp.focus({ window = \"address:$A\" })); hl.dispatch(hl.dsp.window.close())" >/dev/null
wait_for "[ \"\$(hyprctl clients -j | jq length)\" = 0 ]" && echo "closed via eval"
sleep 0.3; echo "events:"; cat $D/ev.txt
