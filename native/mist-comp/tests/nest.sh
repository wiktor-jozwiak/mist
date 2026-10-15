#!/bin/sh
# headless mist-comp (offscreen, never touches the real session) + optional client; usage: nest.sh [client cmd...]
# env: OUT (output name, HDMI-A-1), SIZE (WxH, 1280x800), MIST_COMP_OUTPUTS (several outputs), RUST_LOG
D=$(dirname "$0")
# mist-ui binds (exec mist-ui-bin msg ...) go to a test socket, never the real shell
export MIST_UI_SOCK=$XDG_RUNTIME_DIR/mist/ui-test.sock
MIST_COMP_OUTPUT=${OUT:-HDMI-A-1} MIST_COMP_SIZE=${SIZE:-1280x800} RUST_LOG=${RUST_LOG:-warn,mist_comp=info} \
  setsid ~/.config/quickshell/mist/native/target/release/mist-comp --headless -c true >$D/comp.log 2>&1 &
echo $! > $D/comp.pid
timeout 10 sh -c "until grep -q '^WAYLAND_DISPLAY=' $D/comp.log && grep -q '^HYPRLAND_INSTANCE_SIGNATURE=' $D/comp.log; do sleep 0.1; done"
sock=$(grep -m1 '^WAYLAND_DISPLAY=' $D/comp.log | cut -d= -f2); echo "sock=$sock"; echo $sock > $D/sock
grep -m1 '^HYPRLAND_INSTANCE_SIGNATURE=' $D/comp.log | cut -d= -f2 > $D/sig
[ $# -gt 0 ] && { WAYLAND_DISPLAY=$sock HYPRLAND_INSTANCE_SIGNATURE=$(cat $D/sig) setsid "$@" >$D/client.log 2>&1 & echo $! > $D/client.pid; }
# without this a run with no client returns 1 and `nest.sh && export ...` skips the export (hyprctl hits the real Hyprland)
exit 0
