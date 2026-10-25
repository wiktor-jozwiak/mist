#!/bin/sh
# dropdown (SUPER+CTRL+Return): the first press starts the terminal at the top centre, pinned; then it hides
# when focused and comes back (on any workspace) when not
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
dd() { hyprctl clients -j | jq -c '[.[] | select(.class=="mist-dropdown") | {ws: .workspace.name, at: .at, size: .size, pinned}]'; }
./cc.sh action dropdown >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q '^mist-dropdown '; do sleep 0.05; done"
timeout 0.5 tail -f /dev/null
echo "started: $(dd)"
./cc.sh action dropdown >/dev/null; echo "hidden: $(dd | jq -c '.[0].ws')"
./cc.sh action workspace 2 >/dev/null; ./cc.sh action dropdown >/dev/null; echo "back on ws2: $(dd | jq -c '.[0].ws')"
echo "count: $(hyprctl clients -j | jq '[.[] | select(.class=="mist-dropdown")] | length')"
./unnest.sh >/dev/null 2>&1
