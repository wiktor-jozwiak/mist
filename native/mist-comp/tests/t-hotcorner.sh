#!/bin/sh
# hot corners: `corner tl overview` opens the overview once per visit, leaving re-arms it, a corner without a
# line does nothing, a drag into the corner does nothing
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
printf 'corner tl overview\ncorner br overview\ncorner br off\n' > hotcorner.conf
export KITTY_CONFIG_DIRECTORY=$PWD/kittycfg
MIST_COMP_CONFIG=$PWD/hotcorner.conf ./nest.sh kitty --class mist-corner >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-corner; do sleep 0.1; done"
c() { ./cc.sh "$@" >/dev/null; }
ov() { timeout 0.5 tail -f /dev/null; ./cc.sh overview | grep -q closed && echo closed || echo open; }
c motion 600 400; c motion 0 0; timeout 0.3 tail -f /dev/null; echo "into top-left: $(ov)"
c motion 1 1; c motion 0 0; echo "still there: $(ov)"
c motion 300 300; c motion 0 0; timeout 0.3 tail -f /dev/null; echo "out and back: $(ov)"
c motion 1279 799; echo "bottom-right (off): $(ov)"
c motion 600 400; c button left press; c motion 0 0; c button left release; echo "drag into top-left: $(ov)"
./unnest.sh >/dev/null 2>&1; rm -f hotcorner.conf
