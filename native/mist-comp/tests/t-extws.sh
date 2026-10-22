#!/bin/sh
# ext-workspace-v1: a client sees the workspaces (one group per monitor, the shown one active), hears a
# switch, and its activate + commit switches
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples 2>/dev/null && pwd)/extws
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example extws)
B=$(cd ../../target/release/examples && pwd)/extws
./nest.sh kitty >/dev/null; S=$(cat sock)
timeout 10 sh -c "until ./cc.sh windows | grep -q 'kitty ws1 '; do sleep 0.05; done"
O=$XDG_RUNTIME_DIR/extws.out
WAYLAND_DISPLAY=$S setsid "$B" > $O 2>&1 & P=$!
timeout 3 sh -c "until grep -q done $O; do sleep 0.05; done"; echo "bound: $(tail -1 $O)"
./cc.sh action movetoworkspacesilent 4 >/dev/null; ./cc.sh action workspace 2 >/dev/null; timeout 0.3 tail -f /dev/null
echo "switched: $(tail -1 $O)"
kill $P
WAYLAND_DISPLAY=$S timeout 2 "$B" 4 > $O 2>&1; timeout 0.3 tail -f /dev/null
echo "client asked for 4: $(grep activate $O) | now $(./cc.sh workspaces | tr '\n' ' ')"
rm -f $O; ./unnest.sh >/dev/null 2>&1
