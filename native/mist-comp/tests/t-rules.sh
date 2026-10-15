#!/bin/sh
# window rules: PiP pinned quarter-size bottom right, mist-files without a bar, main apps 62x72 %
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
k() { WAYLAND_DISPLAY=$S setsid kitty "$@" >/dev/null 2>&1 & }
k --title Picture-in-Picture --class pip-test; k --class mist-files; k
timeout 10 sh -c "until [ \$(./cc.sh windows | wc -l) -ge 3 ]; do sleep 0.1; done"; timeout 0.5 tail -f /dev/null
./cc.sh windows
./cc.sh action workspace 2 >/dev/null; echo "--- ws2"; ./cc.sh windows | grep -v hidden
./unnest.sh >/dev/null 2>&1
