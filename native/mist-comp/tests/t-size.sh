#!/bin/sh
# persistent_size: an app without a size rule reopens at the size it had when it closed
D=$(cd $(dirname "$0") && pwd); cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null
S=$(cat sock)
open() { WAYLAND_DISPLAY=$S QT_QPA_PLATFORM=wayland setsid qmlscene6 $D/child.qml >/dev/null 2>&1 & echo $! > client.pid; timeout 10 sh -c "until ./cc.sh windows | grep -q qmlscene; do sleep 0.1; done"; }
w() { ./cc.sh windows | grep qmlscene; }
open; echo "first:    $(w)"
G=$(w | awk '{print $3}'); X=${G%,*}; Y=${G#*,}
c() { ./cc.sh "$@" >/dev/null; }
c key 125 press; c motion $((X + 280)) $((Y + 180)); c button right press; c motion $((X + 480)) $((Y + 330)); c button right release; c key 125 release
timeout 3 sh -c "until ./cc.sh windows | grep qmlscene | grep -qv ' 300x200'; do sleep 0.1; done"; echo "resized:  $(w)"
kill $(cat client.pid); timeout 5 sh -c "while ./cc.sh windows | grep -q qmlscene; do sleep 0.1; done"
open; timeout 2 sh -c "until ./cc.sh windows | grep qmlscene | grep -qv ' 300x200'; do sleep 0.1; done"; echo "reopened: $(w)"
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
