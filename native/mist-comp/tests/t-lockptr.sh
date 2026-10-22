#!/bin/sh
# pointer lock (games): asked for while the pointer is elsewhere, it takes effect when the pointer comes
# over the window, and again after the pointer left and came back (Alt+Tab)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
[ -x ../../target/release/examples/lockptr ] || (cd ../.. && cargo build -q --release -p mist-comp --example lockptr)
./nest.sh ../../target/release/examples/lockptr >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q lockptr; do sleep 0.1; done"
c() { ./cc.sh "$@" >/dev/null; }
c motion 5 5; sleep 0.3
timeout 3 sh -c "until grep -q 'lock requested' client.log; do sleep 0.1; done"
echo "pointer outside: $(grep -c '^locked' client.log) locks"
g=$(./cc.sh windows | grep lockptr | grep -oE "[-0-9]+,[-0-9]+" | head -1)
c motion $(( ${g%,*} + 150 )) $(( ${g#*,} + 100 )); sleep 0.3
echo "pointer over it: $(grep -c '^locked' client.log) locks"
[ "$(grep -c '^locked' client.log)" -ge 1 ] || echo "FAIL: the lock never took effect"
c motion 5 5; sleep 0.2; c motion $(( ${g%,*} + 150 )) $(( ${g#*,} + 100 )); sleep 0.3
echo "back again: $(tr '\n' ' ' < client.log)"
[ "$(grep -c '^locked' client.log)" -ge 2 ] || echo "FAIL: the lock did not come back"
grep -q panicked comp.log && echo PANIC
echo "result: done"
./unnest.sh >/dev/null 2>&1
