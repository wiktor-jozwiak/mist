#!/bin/sh
# mist-ui Snap UI on mist-comp: drag a window's bar to the left edge -> mist-ui snaps it into the left half
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
U=$XDG_RUNTIME_DIR/mist/ui-test.sock
OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh ~/.local/bin/mist-ui-bin >/dev/null
timeout 10 sh -c "until grep -q mist-dock comp.log; do sleep 0.2; done"
S=$(cat sock); export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
WAYLAND_DISPLAY=$S setsid kitty --class mist-snap-test >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-snap-test; do sleep 0.2; done"
echo "start: $(./cc.sh windows | grep mist-snap-test)"
G=$(./cc.sh windows | grep mist-snap-test | awk '{print $3}'); X=${G%,*}; Y=${G#*,}
BX=$((X + 300)); BY=$((Y - 15))
./cc.sh motion $BX $BY >/dev/null; ./cc.sh button left press >/dev/null
for x in 250 150 60 20 2 0; do ./cc.sh motion $x 300 >/dev/null; sleep 0.05; done
timeout 2 sh -c "until MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg snap state 2>/dev/null | grep -qi edge; do sleep 0.05; done"
echo "ui: $(MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg snap state 2>/dev/null | head -c 200)"
./cc.sh button left release >/dev/null
timeout 3 sh -c "until ./cc.sh windows | grep mist-snap-test | grep -q ' 0,\| 8,'; do sleep 0.1; done"
echo "after: $(./cc.sh windows | grep mist-snap-test)"
kill $(cat client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
