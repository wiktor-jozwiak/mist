#!/bin/sh
# nested: a kitty test window maps (GPU via dmabuf?)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg
$D/unnest.sh
$D/nest.sh kitty --class mist-comp-test -e sleep 60
i=0; until $D/cc.sh windows | grep -q '[1-9][0-9]*x[1-9]' || [ $i -gt 50 ]; do i=$((i+1)); sleep 0.2; done
echo "windows: $($D/cc.sh windows)"; echo "focus: $($D/cc.sh focus)"
grep -v '^$' $D/client.log | tail -3
