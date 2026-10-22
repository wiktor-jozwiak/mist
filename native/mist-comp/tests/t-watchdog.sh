#!/bin/sh
# watchdog: a client committing ~200 frames/s with its window on a hidden workspace is named in the log
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
B=$(cd ../../target/release/examples && pwd)/busy
[ -x "$B" ] || (cd ../.. && cargo build -q --release -p mist-comp --example busy)
MIST_COMP_WATCHDOG_SECS=3 ./nest.sh "$B" >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-busy ws1 '; do sleep 0.1; done"
./cc.sh action movetoworkspacesilent 4 >/dev/null; ./cc.sh windows | grep mist-busy
timeout 10 sh -c "until grep -q 'watchdog:' comp.log; do sleep 0.2; done"
w=$(grep -m1 'watchdog:' comp.log | sed 's/.*watchdog: /watchdog: /'); echo "$w"
case "$w" in *mist-busy*hidden*) echo "result: ok" ;; *) echo "result: FAIL (no hidden warning)" ;; esac
./unnest.sh >/dev/null 2>&1
