#!/bin/sh
# nested: spotlight popup takes the keyboard (exclusive layer), gives it back on close
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; U=$XDG_RUNTIME_DIR/mist/ui-test.sock
wait_for() { i=0; until eval "$1"; do i=$((i+1)); [ $i -gt 50 ] && { echo "timeout: $1"; return 1; }; sleep 0.1; done; }
$D/unnest.sh
OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 $D/nest.sh ~/.local/bin/mist-ui-bin
wait_for "grep -q mist-dock $D/comp.log"
echo "before: $($D/cc.sh focus)"
MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg mist toggle spotlight
wait_for "$D/cc.sh focus | grep -q spotlight"
echo "open: $($D/cc.sh focus)"
MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg mist toggle spotlight
wait_for "$D/cc.sh focus | grep -q none"
echo "closed: $($D/cc.sh focus)"
