#!/bin/sh
# occlusion: a client animating on frame callbacks draws at the refresh rate while visible, about once a
# second while a maximized opaque window covers it, and at the full rate again once uncovered
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example busy)
BUSY_FRAME=1 ./nest.sh "$B" >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-busy ws1 '; do sleep 0.1; done"
rate() { ./cc.sh top 2 | awk '/mist-busy|busy/ {print int($1); f=1} END {if (!f) print 0}' | head -1; }
v=$(rate); echo "visible: $v/s"
WAYLAND_DISPLAY=$(cat sock) BUSY_FRAME=1 BUSY_ID=mist-cover BUSY_SIZE=1280x800 setsid "$B" >/dev/null 2>&1 &
cp=$!
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-cover ws1 '; do sleep 0.1; done"
./cc.sh action fullscreen >/dev/null; sleep 0.5
./cc.sh windows
c=$(./cc.sh top 2); echo "$c"
cv=$(echo "$c" | awk '!/mist-cover/ && /busy/ {print int($1)}' | head -1)
kill $cp; sleep 0.5
u=$(rate); echo "covered: ${cv:-?}/s  uncovered: $u/s"
if [ "$v" -ge 20 ] && [ "${cv:-99}" -le 3 ] && [ "$u" -ge 20 ]; then echo "result: ok"; else echo "result: FAIL"; fi
./unnest.sh >/dev/null 2>&1
