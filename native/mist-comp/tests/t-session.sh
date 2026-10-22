#!/bin/sh
# session restore: kitty on ws3 is written to the session file, a new compositor with MIST_COMP_RESTORE=1 starts
# it again on ws3 at the same place; the live ~/.cache/mist/session.json is never touched
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
export MIST_COMP_SESSION_FILE=$XDG_RUNTIME_DIR/mist-test-session.json; rm -f $MIST_COMP_SESSION_FILE*
./nest.sh >/dev/null; S=$(cat sock)
WAYLAND_DISPLAY=$S setsid kitty --class mist-sess -e sleep 120 >/dev/null 2>&1 & k=$!
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-sess ws1 '; do sleep 0.1; done"
./cc.sh action movetoworkspacesilent 3 >/dev/null
before=$(./cc.sh windows | grep mist-sess); echo "before: $before"
timeout 5 sh -c "until grep -q mist-sess $MIST_COMP_SESSION_FILE 2>/dev/null; do sleep 0.2; done"
echo "file: $(cat $MIST_COMP_SESSION_FILE)"
kill $k; ./unnest.sh >/dev/null 2>&1
MIST_COMP_RESTORE=1 ./nest.sh >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q 'mist-sess ws3 '; do sleep 0.1; done"
after=$(./cc.sh windows | grep mist-sess); echo "after: $after"
grep 'session restore' comp.log
[ -f $MIST_COMP_SESSION_FILE.restored ] || echo "FAIL: file not renamed"
[ "${before#* ws}" = "${after#* ws}" ] && echo "result: ok" || echo "result: FAIL (place differs)"
./unnest.sh >/dev/null 2>&1; rm -f $MIST_COMP_SESSION_FILE*
