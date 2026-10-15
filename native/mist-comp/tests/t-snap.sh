#!/bin/sh
# Windows 11 snapping in mist-comp: Win+arrows, green = maximize into the work area, drag events, MistSnapApply
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh kitty >/dev/null
timeout 10 sh -c "until ./cc.sh windows | grep -q kitty; do sleep 0.2; done"
export HYPRLAND_INSTANCE_SIGNATURE=$(cat sig)
socat -u UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock - > ev.txt & EV=$!
w() { timeout 3 sh -c "until ./cc.sh windows | grep -q '$1'; do sleep 0.05; done" || echo "  timeout waiting for $1"; ./cc.sh windows | head -1; }
# key SUPER (125) + arrow: evdev Left 105, Right 106, Up 103, Down 108
k() { ./cc.sh key 125 press >/dev/null; ./cc.sh key $1 press >/dev/null; ./cc.sh key $1 release >/dev/null; ./cc.sh key 125 release >/dev/null; }
k 105; echo "Win+Left:   $(w ' 628x')"
k 103; echo "Win+Up:     $(w ' 628x3')"
k 108; echo "Win+Down:   $(w ' 628x7')"
k 106; echo "Win+Right:  $(w '243,127')"
k 103; echo "Win+Up max: $(w ' 1264x')"
k 108; echo "Win+Down:   $(w '243,127')"
# green button = maximize into the work area; drag from the max bar restores the size under the cursor
./cc.sh motion 304 112 >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh button left release >/dev/null
echo "green:      $(w ' 1264x')"
./cc.sh motion 640 23 >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh motion 650 60 >/dev/null; ./cc.sh button left release >/dev/null
echo "drag:       $(w ' 793x')"
A=$(hyprctl clients -j | jq -r '.[0].address')
hyprctl eval "MistSnapApply(\"$A\", 0.5, 0, 0.5, 0.5, 100, 100, 600, 400)" >/dev/null
echo "apply q:    $(w ' 628x3')"
kill $EV; echo "--- events"; grep -E "^mist" ev.txt
./unnest.sh >/dev/null 2>&1
