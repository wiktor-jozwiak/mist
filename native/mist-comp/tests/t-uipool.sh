#!/bin/sh
# mist-ui shm pool: a full-monitor pane (session) grows the shared pool; closing it gives the memory back
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
U=$XDG_RUNTIME_DIR/mist/ui-test.sock; BIN=${MIST_UI_BIN:-$PWD/../../target/release/mist-ui}
OUT=DP-3 SIZE=1920x1080 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh $BIN >/dev/null
timeout 10 sh -c "until grep -q mist-dock comp.log; do sleep 0.2; done"; timeout 1 tail -f /dev/null
P=$(cat client.pid)
memfd() { awk '/^[0-9a-f]/{f=$6} /^Rss/{if(f ~ /memfd:smithay/) s+=$2} END{print s+0}' /proc/$P/smaps; }
echo "start: memfd $(memfd) kB"
for i in 1 2; do
  MIST_UI_SOCK=$U $BIN msg mist toggle session >/dev/null; timeout 1.5 tail -f /dev/null
  echo "open $i: memfd $(memfd) kB, layers: $(./cc.sh layers | awk '{print $2}' | tr '\n' ' ')"
  MIST_UI_SOCK=$U $BIN msg mist close >/dev/null; timeout 1.5 tail -f /dev/null
  echo "closed $i: memfd $(memfd) kB, rss $(ps -o rss= -p $P) kB"
done
./unnest.sh >/dev/null 2>&1
