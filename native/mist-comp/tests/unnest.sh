#!/bin/sh
D=$(dirname "$0")
kill $(cat $D/client.pid $D/client2.pid $D/lock.pid 2>/dev/null) 2>/dev/null
kill $(cat $D/comp.pid 2>/dev/null) 2>/dev/null
timeout 5 sh -c "while kill -0 $(cat $D/comp.pid 2>/dev/null) 2>/dev/null; do sleep 0.2; done"
rm -f $D/client.pid $D/client2.pid $D/lock.pid
