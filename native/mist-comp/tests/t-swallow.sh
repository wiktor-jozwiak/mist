#!/bin/sh
# swallow: a GUI window started from kitty hides kitty; closing it brings kitty back where it was
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh kitty sh -c "sleep 1.5; QT_QPA_PLATFORM=wayland qmlscene6 $PWD/child.qml & echo \$! > $PWD/child.pid; wait; sleep 100" >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q kitty; do sleep 0.1; done"
echo "before: $(./cc.sh windows | tr '\n' ';')"
timeout 10 sh -c "until ./cc.sh windows | grep -q qmlscene; do sleep 0.1; done"
echo "child:  $(./cc.sh windows | tr '\n' ';')"
kill $(cat child.pid)
timeout 5 sh -c "until ./cc.sh windows | grep -q 'kitty ws1'; do sleep 0.1; done"
echo "closed: $(./cc.sh windows | tr '\n' ';') focus $(./cc.sh focus)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
