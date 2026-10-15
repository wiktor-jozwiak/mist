#!/bin/sh
# XWayland: glxgears maps as a managed window with a bar, focused, frames keep coming
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
MIST_COMP_XWAYLAND=1 ./nest.sh >/dev/null
timeout 10 sh -c "until grep -q 'xwayland ready' comp.log; do sleep 0.2; done" || { echo "no xwayland"; exit 1; }
DISPLAY=$(grep -m1 '^DISPLAY=' comp.log | cut -d= -f2) WAYLAND_DISPLAY= setsid glxgears >gears.log 2>&1 & echo $! > client.pid
timeout 10 sh -c "until ./cc.sh windows 2>/dev/null | grep -q ws; do sleep 0.2; done"
echo "windows: $(./cc.sh windows)  focus: $(./cc.sh focus)"
S=$(cat sock); WAYLAND_DISPLAY=$S grim -g "490,250 300x300" x1.png
timeout 5 sh -c "until WAYLAND_DISPLAY=$S grim -g '490,250 300x300' x2.png && ! magick compare -metric AE x1.png x2.png null: 2>/dev/null; do :; done" && echo "frames: moving"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
