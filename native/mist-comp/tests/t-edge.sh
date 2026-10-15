#!/bin/sh
# resize by the window frame: right edge, bottom-left corner, top edge (above the bar)
D=$(dirname "$0"); cd $D; export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg; ./unnest.sh >/dev/null 2>&1
./nest.sh kitty --class mist-edge >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-edge; do sleep 0.1; done"; timeout 0.4 tail -f /dev/null
c() { ./cc.sh "$@" >/dev/null; }
drag() { c motion $1 $2; c button left press; c motion $3 $4; c button left release; timeout 0.4 tail -f /dev/null; echo "$5: $(./cc.sh windows | grep mist-edge)"; }
echo "start: $(./cc.sh windows | grep mist-edge)"
drag 1071 400 1171 400 right
drag 208 686 158 736 bottomleft
drag 600 113 600 83 top
c motion 600 400; c button left press; c button left release; echo "inside click: $(./cc.sh windows | grep mist-edge)"
./unnest.sh >/dev/null 2>&1
