#!/bin/sh
# per-output frames: a client animating on one monitor redraws that monitor only; the other one, with nothing
# changing on it, gets no render passes at all
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example busy)
MIST_COMP_OUTPUTS=HDMI-A-1:1280x800,DP-3:1280x800 BUSY_FRAME=1 ./nest.sh "$B" >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-busy ws1 '; do sleep 0.1; done"
sleep 1; a=$(./cc.sh passes); sleep 3; b=$(./cc.sh passes)
echo "$a" | sed 's/^/before: /'; echo "$b" | sed 's/^/after:  /'
p() { echo "$1" | awk -v o=$2 '$1 == o {print $2}'; }
busy=$(( $(p "$b" HDMI-A-1) - $(p "$a" HDMI-A-1) )); idle=$(( $(p "$b" DP-3) - $(p "$a" DP-3) ))
echo "3 s: HDMI-A-1 (busy) $busy passes, DP-3 (idle) $idle passes"
if [ $busy -ge 100 ] && [ $idle -le 3 ]; then echo "result: ok"; else echo "result: FAIL"; fi
./unnest.sh >/dev/null 2>&1
