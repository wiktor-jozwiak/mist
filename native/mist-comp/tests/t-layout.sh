#!/bin/sh
# layout save / load: open windows go back to their saved workspace and place, a closed one is started again
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
export MIST_COMP_LAYOUT_DIR=$(mktemp -d)
./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
for c in lay-a lay-b; do
  WAYLAND_DISPLAY=$S setsid kitty --class $c >/dev/null 2>&1 &
  timeout 10 sh -c "until ./cc.sh windows | grep -q '^$c '; do sleep 0.05; done"
done
timeout 0.5 tail -f /dev/null
fw() { a=$(hyprctl clients -j | jq -r ".[] | select(.class==\"$1\") | .address"); hyprctl eval "hl.dispatch(hl.dsp.focus({ window = \"address:$a\" }))" >/dev/null; }
st() { hyprctl clients -j | jq -c '[.[] | select(.class|startswith("lay-")) | [.class, .workspace.id, .at, .size]] | sort'; }
fw lay-b; ./cc.sh action movetoworkspacesilent 2 >/dev/null
timeout 0.3 tail -f /dev/null
before=$(st); echo "saved: $before"
hyprctl dispatch layout save Test >/dev/null; echo "file: $(jq -c '[.[].app] | sort' $MIST_COMP_LAYOUT_DIR/Test.json)"
# shuffle: a to ws 3, b closed
fw lay-a; ./cc.sh action movetoworkspacesilent 3 >/dev/null
./cc.sh action workspace 2 >/dev/null; fw lay-b; ./cc.sh action close >/dev/null
timeout 0.5 tail -f /dev/null
echo "shuffled: $(st)"
./cc.sh action workspace 1 >/dev/null
hyprctl dispatch layout load Test >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q '^lay-b '; do sleep 0.05; done"
timeout 0.8 tail -f /dev/null
after=$(st); echo "loaded: $after"
[ "$before" = "$after" ] && echo "same as saved" || echo "DIFFERENT"
./unnest.sh >/dev/null 2>&1; rm -rf "$MIST_COMP_LAYOUT_DIR"
