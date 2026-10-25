#!/bin/sh
# pin (SUPER+CTRL+T): the window stays above the one focused after it and comes along to other workspaces
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
for c in pin-a pin-b; do
  WAYLAND_DISPLAY=$S setsid kitty --class $c >/dev/null 2>&1 &
  timeout 10 sh -c "until ./cc.sh windows | grep -q '^$c '; do sleep 0.05; done"
done
timeout 0.5 tail -f /dev/null
fw() { a=$(hyprctl clients -j | jq -r ".[] | select(.class==\"$1\") | .address"); hyprctl eval "hl.dispatch(hl.dsp.focus({ window = \"address:$a\" }))" >/dev/null; }
top() { ./cc.sh windows | grep -v hidden | tail -1 | cut -d' ' -f1; }
fw pin-a; echo "focused a, top: $(top)"
./cc.sh action pin >/dev/null; fw pin-b; echo "pinned a, focused b, top: $(top), hyprctl pinned: $(hyprctl clients -j | jq -c '[.[] | select(.pinned) | .class]')"
./cc.sh action workspace 2 >/dev/null; echo "ws2: $(./cc.sh windows | grep -v hidden | cut -d' ' -f1,2,5 | tr '\n' ' ')"
./cc.sh action workspace 1 >/dev/null; fw pin-a; ./cc.sh action pin >/dev/null; fw pin-b; echo "unpinned, focused b, top: $(top)"
./unnest.sh >/dev/null 2>&1
