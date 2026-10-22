#!/bin/sh
# a window dragged by its bar onto the other monitor joins the workspace shown there (ws1 left, ws4 right)
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" ./nest.sh kitty --class mist-drag >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-drag; do sleep 0.1; done"
echo "start: $(./cc.sh workspaces | tr '\n' ' ')| $(./cc.sh windows)"
G=$(./cc.sh windows | grep mist-drag | awk '{print $3}'); X=${G%,*}; Y=${G#*,}
./cc.sh motion $((X + 200)) $((Y - 15)) >/dev/null; ./cc.sh button left press >/dev/null
for x in 500 900 1300 1700 1900; do ./cc.sh motion $x 300 >/dev/null; sleep 0.05; done
./cc.sh button left release >/dev/null; sleep 0.3
echo "after: $(./cc.sh windows)"
./cc.sh windows | grep -q "mist-drag ws4 " && echo "result: ok" || echo "result: FAIL (expected ws4)"
./unnest.sh >/dev/null 2>&1
