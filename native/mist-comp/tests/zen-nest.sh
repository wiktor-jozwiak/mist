#!/bin/sh
# Zen with the Mist theme in a nested mist-comp (wallpaper + mist-ui bar), throwaway profile; for looking at it.
#   zen-nest.sh up     compositor + wallpaper + shell     zen-nest.sh zen [url]   (re)start Zen
#   zen-nest.sh shot <png>                                 zen-nest.sh down
T=$(cd "$(dirname "$0")/../../target/release" && pwd)
D=$(cd $(dirname "$0") && pwd); cd $D
PROF=${ZEN_PROF:-$XDG_RUNTIME_DIR/mist-zen-test}
case $1 in
up)
  ./unnest.sh >/dev/null 2>&1
  U=$XDG_RUNTIME_DIR/mist/ui-test.sock
  SIZE=1920x1080 OUT=DP-3 MIST_UI_SOCK=$U MIST_UI_NO_SERVICES=1 MIST_UI_NO_WALL=1 MIST_UI_NO_DESK=1 ./nest.sh ${MIST_UI_BIN:-$T/mist-ui} >/dev/null
  S=$(cat sock)
  mkdir -p -m 700 $XDG_RUNTIME_DIR/mrt; ln -sfn $XDG_RUNTIME_DIR/$S $XDG_RUNTIME_DIR/mrt/$S; ln -sfn $XDG_RUNTIME_DIR/hypr $XDG_RUNTIME_DIR/mrt/hypr
  HYPRLAND_INSTANCE_SIGNATURE=$(cat sig) XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR/mrt WAYLAND_DISPLAY=$S setsid ${MIST_WALL_BIN:-$T/mist-wall} >wall.log 2>&1 & echo $! > wall.pid
  timeout 10 sh -c "until ./cc.sh layers | grep -qi wall && ./cc.sh layers | grep -q mist-bar; do sleep 0.2; done"; ./cc.sh layers ;;
zen)
  [ -f zen.pid ] && kill $(cat zen.pid) 2>/dev/null && sleep 1
  mkdir -p "$PROF"; # the Mist theme for Zen is local only (integrations/zen, not in the repo): without it Zen runs stock
  [ -n "$NOINST" ] || [ ! -x ../../../integrations/zen/install.sh ] || ../../../integrations/zen/install.sh "$PROF" >/dev/null
  # ZEN_FROM=<profile>: start from a copy of its prefs + window state, else a clean first run
  if [ -n "$ZEN_FROM" ]; then cp "$ZEN_FROM/prefs.js" "$ZEN_FROM/xulstore.json" "$PROF/"; else rm -f "$PROF/xulstore.json"; fi
  [ -n "$ZEN_HOME" ] && echo "user_pref(\"browser.startup.homepage\", \"$ZEN_HOME\");" >> "$PROF/user.js"
  [ -n "$ZEN_PREFS" ] && printf '%s\n' "$ZEN_PREFS" >> "$PROF/user.js"
  grep -q max_resumed "$PROF/user.js" || echo 'user_pref("toolkit.startup.max_resumed_crashes", -1);' >> "$PROF/user.js"
  grep -q welcome-screen "$PROF/user.js" || echo 'user_pref("zen.welcome-screen.seen", true);' >> "$PROF/user.js"
  WAYLAND_DISPLAY=$(cat sock) MOZ_ENABLE_WAYLAND=1 DISPLAY= ${ZEN_ENV:-} setsid /opt/zen-browser-bin/zen-bin --no-remote --profile "$PROF" ${2:-} >zen.log 2>&1 & echo $! > zen.pid
  timeout 20 sh -c "until ./cc.sh windows | grep -q '^zen'; do sleep 0.3; done"; sleep ${WAIT:-4}; ./cc.sh windows ;;
shot) WAYLAND_DISPLAY=$(cat sock) grim "$2" ;;
down) kill $(cat zen.pid) $(cat wall.pid) 2>/dev/null; rm -f zen.pid; ./unnest.sh >/dev/null 2>&1 ;;
esac
