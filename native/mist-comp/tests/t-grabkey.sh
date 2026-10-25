#!/bin/sh
# IPC grabkey (Mist Settings recording a shortcut): the next combination comes back by name and reaches no
# bind (Super+Return would open a terminal), Escape cancels, nothing pressed times out
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
sock=$XDG_RUNTIME_DIR/mist-comp/$S.sock
grab() { echo "grabkey $1" | socat -t6 - UNIX:$sock > grab.out & sleep 0.3; }
key() { ./cc.sh key $1 press >/dev/null; }
up() { ./cc.sh key $1 release >/dev/null; }
grab 5; key 125; key 42; key 48; up 48; up 42; up 125; wait; echo "Super+Shift+b: $(cat grab.out)"
grab 5; key 125; key 28; up 28; up 125; wait; echo "Super+Return: $(cat grab.out) | windows $(./cc.sh windows | grep -c .)"
grab 5; key 1; up 1; wait; echo "Escape: $(cat grab.out)"
grab 1; wait; echo "nothing: $(cat grab.out)"
rm -f grab.out; ./unnest.sh >/dev/null 2>&1
