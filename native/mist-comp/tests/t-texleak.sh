#!/bin/sh
# GL memory of closed client buffers is freed (cleanup_texture_cache each frame): mist-ui panels opened and
# closed for 60 s; mist-comp's memory must level off (before the fix: +37 MB a minute, without end)
T=$(cd "$(dirname "$0")/../../target/release" && pwd)
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
U=$XDG_RUNTIME_DIR/mist/ui-test.sock; UI=${MIST_UI_BIN:-$T/mist-ui}
OUT=DP-3 SIZE=1920x1080 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh $UI >/dev/null
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"; timeout 1 tail -f /dev/null
P=$(cat comp.pid)
pss() { awk '/^Pss:/{s+=$2}END{printf "%d", s/1024}' /proc/$P/smaps_rollup; }
churn() { end=$(( $(date +%s) + $1 )); while [ $(date +%s) -lt $end ]; do for p in control spotlight; do
  MIST_UI_SOCK=$U $UI msg mist toggle $p >/dev/null; timeout 0.4 tail -f /dev/null; MIST_UI_SOCK=$U $UI msg mist toggle $p >/dev/null; timeout 0.3 tail -f /dev/null; done; done; }
a=$(pss); churn 30; b=$(pss); churn 30; c=$(pss)
echo "mist-comp pss: start $a MB, 30 s $b MB, 60 s $c MB"
[ $((c - b)) -le 12 ] && echo "result: ok" || echo "result: FAIL (still growing)"
./unnest.sh >/dev/null 2>&1
