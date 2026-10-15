#!/bin/sh
# XWayland window resized by its frame and by Super + right drag
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
MIST_COMP_XWAYLAND=1 ./nest.sh >/dev/null
timeout 10 sh -c "until grep -q 'xwayland ready' comp.log; do sleep 0.2; done" || { echo "no xwayland"; exit 1; }
DISPLAY=$(grep -m1 '^DISPLAY=' comp.log | cut -d= -f2) WAYLAND_DISPLAY= setsid glxgears >gears.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows 2>/dev/null | grep -q ws; do sleep 0.2; done"
c() { ./cc.sh "$@" >/dev/null; }
drag() { c motion $1 $2; c button $6 press; c motion $3 $4; c button $6 release; timeout 0.3 tail -f /dev/null; echo "$5: $(./cc.sh windows)"; }
echo "start: $(./cc.sh windows)"
drag 795 400 895 400 right left
drag 485 215 455 185 topleft left
c key 125 press; drag 850 500 900 550 super-rmb right; c key 125 release
./unnest.sh >/dev/null 2>&1
