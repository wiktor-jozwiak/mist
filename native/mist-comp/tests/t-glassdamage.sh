#!/bin/sh
# glass re-blurs only for pixels that change behind it: a maximized window redrawing a small square in its
# middle, under mist-ui's glass bar and dock, re-blurs neither (blur runs per second from `stats`)
T=$(cd "$(dirname "$0")/../../target/release" && pwd)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$T/examples/busy
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example busy)
U=$XDG_RUNTIME_DIR/mist/ui-test.sock
SIZE=1920x1080 OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh ${MIST_UI_BIN:-$T/mist-ui} >/dev/null
WAYLAND_DISPLAY=$(cat sock) BUSY_FRAME=1 BUSY_SIZE=1920x1080 BUSY_DAMAGE=64x64 setsid "$B" >/dev/null 2>&1 & echo $! > client2.pid
timeout 10 sh -c "until ./cc.sh windows | grep -q mist-busy && ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"
./cc.sh action maximize >/dev/null; sleep 1
runs() { ./cc.sh stats | sed 's/.*blur \([0-9]*\)\/.*/\1/'; }
fr() { ./cc.sh stats | awk '{print $2}'; }
c0=$(awk "{print \$14+\$15}" /proc/$(cat comp.pid)/stat); a=$(runs); f=$(fr); sleep 3; b=$(runs); g=$(fr); c1=$(awk "{print \$14+\$15}" /proc/$(cat comp.pid)/stat)
echo "3 s: frames $((g - f)), blur runs $((b - a)), compositor CPU $((c1 - c0)) ticks"
if [ $((g - f)) -ge 100 ] && [ $((b - a)) -le 3 ]; then echo "result: ok"; else echo "result: FAIL"; fi
kill $(cat client2.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1
