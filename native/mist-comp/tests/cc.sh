#!/bin/sh
# send one command to the nested mist-comp IPC
D=$(dirname "$0"); echo "$*" | socat -t3 - UNIX:$XDG_RUNTIME_DIR/mist-comp/$(cat $D/sock).sock
