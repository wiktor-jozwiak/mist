#!/bin/sh
# session restore waits for the old process of a command (a crashed browser still holding its profile lock),
# skips it when a new one started meanwhile, starts at once when there is none. Prints the restore log lines.
N=$(mktemp -d); BIN=${BIN:-$(cd "$(dirname "$0")/../../target/release" && pwd)/mist-comp}; cd $N; cp /usr/bin/sleep $N/zzsl
mk() { printf '[{"app":"sl","cmd":"%s","ws":1,"at":[0,0],"size":[100,100],"max":false}]' "'$N/zzsl' '7'" > s.json; }
run() { MIST_COMP_SESSION_FILE=$N/s.json MIST_COMP_RESTORE=1 MIST_COMP_OUTPUT=DP-3 RUST_LOG=warn,mist_comp=info setsid $BIN --headless -c true > $1 2>&1 & CP=$!; }
echo "case1 old dies after 4 s (start $(date +%T))"; mk; $N/zzsl 4 & run r1.log; sleep 6; grep "session restore" r1.log | cut -c12-200; kill -9 $CP 2>/dev/null; pkill -x zzsl; sleep 0.3
echo "case2 new one started after 1 s"; mk; $N/zzsl 3 & run r2.log; sleep 1; $N/zzsl 9 & sleep 4; grep "session restore" r2.log | cut -c12-200; kill -9 $CP 2>/dev/null; pkill -x zzsl; sleep 0.3
echo "case3 nothing old (start $(date +%T))"; mk; run r3.log; sleep 1; grep "session restore" r3.log | cut -c12-200; kill -9 $CP 2>/dev/null; pkill -x zzsl; true
rm -rf $N
