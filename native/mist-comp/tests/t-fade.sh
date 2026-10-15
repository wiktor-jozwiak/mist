#!/bin/sh
# layer fade-in (Hyprland layersIn): Control Centre fades over ~200 ms (frames tick, then idle); Spotlight (no_anim) does not
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
U=$XDG_RUNTIME_DIR/mist/ui-test.sock
OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh ~/.local/bin/mist-ui-bin >/dev/null
timeout 10 sh -c "until ./cc.sh layers | grep -q mist-dock; do sleep 0.2; done"; timeout 1 tail -f /dev/null
f() { ./cc.sh stats | cut -d' ' -f2; }
for p in control spotlight; do
  a=$(f); MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg mist toggle $p >/dev/null
  timeout 5 sh -c "until ./cc.sh layers | grep -q mist-$p; do sleep 0.02; done"
  timeout 0.6 tail -f /dev/null; b=$(f); timeout 1 tail -f /dev/null; c=$(f)
  echo "$p: frames during open $((b-a)), idle after $((c-b))"
  MIST_UI_SOCK=$U ~/.local/bin/mist-ui-bin msg mist toggle $p >/dev/null; timeout 1 tail -f /dev/null
done
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
