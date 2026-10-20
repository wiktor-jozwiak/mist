#!/bin/sh
# Headless scenarios for CI and before every push: mist-comp on Mesa's software renderer (no GPU needed),
# two outputs, mist-ui as the client. Throwaway HOME and XDG_RUNTIME_DIR; never touches the real session.
#   native/mist-comp/tests/ci.sh            (after cargo build --release -p mist-comp -p mist-ui)
# env: BIN (target/release), OUT (shots and logs, default a temp dir), MIST_CI_SLOW=1 (waits for a minute
# to turn: the lock clock on the second monitor)
# Needs: socat. One line per check, exit 1 if any failed.
set -u
D=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$D/../../.." && pwd)
BIN=${BIN:-$ROOT/native/target/release}
OUT=${OUT:-$(mktemp -d)}
RT=$(mktemp -d /tmp/mci.XXXX) # short: unix socket paths are limited to 108 bytes
H=$OUT/home
mkdir -p "$OUT" "$H/.config/mist" "$H/.cache/mist" "$H/.local/share/fonts/SF-Pro"
chmod 700 "$RT"
export XDG_RUNTIME_DIR=$RT HOME=$H MIST_DIR=$ROOT MIST_LANG=en LANG=en_US.UTF-8 MIST_PRIMARY=DP-3
export MIST_UI_NO_SERVICES=1 MIST_UI_SOCK=$RT/ui.sock MIST_COMP_SOFTWARE=1
unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
cp "$ROOT"/docs/demo/weather.json "$ROOT"/docs/demo/garmin.json "$ROOT"/docs/demo/running.json "$H/.cache/mist/" 2>/dev/null
# a UI font where mist-ui looks first (SF Pro is not shipped; Inter / Noto from the system)
for f in /usr/share/fonts/inter/InterVariable.ttf /usr/share/fonts/TTF/InterVariable.ttf /usr/share/fonts/opentype/inter/Inter-Regular.otf \
         /usr/share/fonts/noto/NotoSans-Regular.ttf /usr/share/fonts/truetype/noto/NotoSans-Regular.ttf; do
    [ -f "$f" ] && { ln -sf "$f" "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf"; break; }
done

fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1${2:+: $2}"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1" "${3:-}"; fi; }
wait_for() { i=0; until eval "$1"; do i=$((i + 1)); [ $i -gt "${2:-100}" ] && return 1; sleep 0.1; done; }
C() { echo "$*" | socat -t3 - "UNIX:$RT/mist-comp/$S.sock" 2>/dev/null; }
J() { echo "$*" | socat -t3 - "UNIX:$RT/hypr/mist-comp-$S/.socket.sock" 2>/dev/null; }
pids=""
spawn() { WAYLAND_DISPLAY=$S "$@" >>"$OUT/clients.log" 2>&1 & pids="$pids $!"; echo $!; }
cleanup() {
    for p in $pids; do kill "$p" 2>/dev/null; done
    [ -n "${CP:-}" ] && kill "$CP" 2>/dev/null
    sleep 0.3; rm -rf "$RT"
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------------ compositor
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" RUST_LOG=${RUST_LOG:-warn,mist_comp=info} \
    "$BIN/mist-comp" --headless >"$OUT/comp.log" 2>&1 &
CP=$!
wait_for "grep -q '^WAYLAND_DISPLAY=' '$OUT/comp.log'" 100 || { echo "FAIL compositor did not start"; cat "$OUT/comp.log"; exit 1; }
S=$(grep -m1 '^WAYLAND_DISPLAY=' "$OUT/comp.log" | cut -d= -f2)
wait_for "C stats | grep -q frames" 50
check "compositor up, 2 outputs" "J j/monitors | grep -o '\"model\"' | wc -l | grep -qx 2"

# ------------------------------------------------------------------ protocols a desktop needs
WAYLAND_DISPLAY=$S "$BIN/mist-ui" --wayland-globals >"$OUT/globals.txt" 2>/dev/null
for g in xdg_wm_base zwlr_layer_shell_v1 ext_session_lock_manager_v1 zwp_linux_dmabuf_v1 wp_viewporter wp_fractional_scale_manager_v1 \
         wp_presentation xdg_activation_v1 zxdg_decoration_manager_v1 zwp_pointer_constraints_v1 zwp_relative_pointer_manager_v1 \
         zwp_text_input_manager_v3 zwp_input_method_manager_v2 zwp_virtual_keyboard_manager_v1 zwp_tablet_manager_v2 \
         ext_foreign_toplevel_list_v1 ext_data_control_manager_v1 zwlr_data_control_manager_v1 zxdg_exporter_v2 zxdg_importer_v2 \
         xdg_toplevel_icon_manager_v1 xdg_toplevel_tag_manager_v1 wp_content_type_manager_v1 wp_alpha_modifier_v1 xdg_system_bell_v1 \
         wp_security_context_manager_v1 wp_cursor_shape_manager_v1 zwlr_screencopy_manager_v1 ext_idle_notifier_v1 zwp_idle_inhibit_manager_v1; do
    grep -q "^$g " "$OUT/globals.txt" || bad "global $g"
done
ok "globals ($(wc -l <"$OUT/globals.txt") advertised)"
if command -v wl-copy >/dev/null && command -v wl-paste >/dev/null; then
    echo -n mist-clip-test | WAYLAND_DISPLAY=$S timeout 5 wl-copy
    check "clipboard: wl-copy then wl-paste (data-control)" "[ \"\$(WAYLAND_DISPLAY=$S timeout 5 wl-paste -n)\" = mist-clip-test ]"
    WAYLAND_DISPLAY=$S timeout 5 wl-copy --clear 2>/dev/null
fi
if command -v wtype >/dev/null; then
    check "virtual keyboard: wtype" "WAYLAND_DISPLAY=$S timeout 5 wtype abc"
fi

# ------------------------------------------------------------------ lock screen
L=$(spawn "$BIN/mist-ui" lock)
check "lock: a lock surface per output" "wait_for \"C locked | grep -q 'true surfaces=2'\" 80"
C motion 1900 400 >/dev/null; C motion 200 400 >/dev/null
check "lock: pointer goes to the second output and back" "J j/cursorpos | grep -q '\"x\":200'"
check "lock: shot DP-3" "C shot '$OUT/lock-dp3.ppm' DP-3 | grep -q ok"
check "lock: shot HDMI-A-1" "C shot '$OUT/lock-hdmi.ppm' HDMI-A-1 | grep -q ok"
if [ "${MIST_CI_SLOW:-0}" = 1 ]; then
    sleep $((61 - $(date +%S)))
    C shot "$OUT/lock-hdmi-2.ppm" HDMI-A-1 >/dev/null
    check "lock: the second output's clock turns with the minute" "! cmp -s '$OUT/lock-hdmi.ppm' '$OUT/lock-hdmi-2.ppm'"
fi
kill "$L" 2>/dev/null; sleep 0.5
check "lock: a dead locker leaves the session locked" "C locked | grep -q '^true'"
spawn "$BIN/mist-ui" lock preview 2 >/dev/null
check "lock: a new locker takes over and unlocks" "wait_for \"C locked | grep -q '^false'\" 80"

# ------------------------------------------------------------------ shell surfaces
spawn "$BIN/mist-ui" >/dev/null
check "shell: bar on DP-3" "wait_for \"C layers | grep -q '^DP-3 mist-bar'\" 100"
check "shell: dock on DP-3" "C layers | grep -q '^DP-3 mist-dock'"
check "shell: island on DP-3" "C layers | grep -q '^DP-3 mist-island'"
check "shell: bar on HDMI-A-1" "C layers | grep -q '^HDMI-A-1 mist-bar'"
TOP=$(J j/monitors | tr -d ' \n' | sed -n 's/.*"reserved":\[\([0-9]*\),\([0-9]*\),.*/\2/p')
check "shell: the bar reserves the top" "[ \"\${TOP:-0}\" -gt 0 ]" "reserved top ${TOP:-?}"

# ------------------------------------------------------------------ a window: kept below the bar
spawn "$BIN/mist-ui" files "$H" >/dev/null
check "window: mist-files mapped" "wait_for \"C windows | grep -q '^mist-files ws'\" 100"
geo() { C windows | awk '/^mist-files ws/ {print $3; exit}'; }
size() { C windows | awk '/^mist-files ws/ {print $4; exit}'; }
G=$(geo); X=${G%,*}; Y=${G#*,}; SZ=$(size); W=${SZ%x*}
check "window: placed below the bar" "[ \"$Y\" -ge \"${TOP:-0}\" ]" "y=$Y top=${TOP:-?}"
# Super + left drag up to the very top
C motion $((X + W / 2)) $((Y + 60)) >/dev/null
C key 125 press >/dev/null; C button left press >/dev/null
C motion $((X + W / 2)) 0 >/dev/null; C motion $((X + W / 2)) 1 >/dev/null
C button left release >/dev/null; C key 125 release >/dev/null
sleep 0.3; G=$(geo); Y2=${G#*,}
check "window: dragged up, stops below the bar" "[ \"$Y2\" -lt \"$Y\" ] && [ \"$Y2\" -ge \"${TOP:-0}\" ]" "y $Y -> $Y2, top ${TOP:-?}"
# grab the top edge and pull it to the top
C motion $((X + W / 2)) $((Y2 + 400)) >/dev/null
C key 125 press >/dev/null; C button left press >/dev/null; C motion $((X + W / 2)) $((Y2 + 500)) >/dev/null
C button left release >/dev/null; C key 125 release >/dev/null
wait_for "[ \"\$(geo)\" != \"$G\" ]" 30
G=$(geo); X=${G%,*}; Y=${G#*,}; SZ=$(size); H0=${SZ#*x}
C motion $((X + W / 2)) $((Y - 2)) >/dev/null
C button left press >/dev/null; C motion $((X + W / 2)) 0 >/dev/null; C motion $((X + W / 2)) 1 >/dev/null
C button left release >/dev/null
wait_for "[ \"\$(size)\" != \"$SZ\" ] && [ \"\$(geo)\" != \"$G\" ]" 50 # the client's new size, the top edge moved
G=$(geo); Y3=${G#*,}; SZ=$(size); H1=${SZ#*x}
check "window: top edge stretched up, stops below the bar" "[ \"$H1\" -gt \"$H0\" ] && [ \"$Y3\" -ge \"${TOP:-0}\" ]" "y $Y -> $Y3, h $H0 -> $H1, top ${TOP:-?}"

# ------------------------------------------------------------------ screencopy (grim, portals)
if command -v grim >/dev/null; then
    C shot "$OUT/shot-dp3.ppm" DP-3 >/dev/null
    check "screencopy: grim works" "WAYLAND_DISPLAY=$S timeout 10 grim -o DP-3 -t ppm '$OUT/grim-dp3.ppm'"
    check "screencopy: the same pixels as the compositor's own shot" "cmp -s '$OUT/shot-dp3.ppm' '$OUT/grim-dp3.ppm'"
    check "screencopy: region" "WAYLAND_DISPLAY=$S timeout 10 grim -g '10,10 200x100' -t ppm '$OUT/grim-region.ppm' && head -c 20 '$OUT/grim-region.ppm' | grep -q '200 100'"
fi

# ------------------------------------------------------------------ workspaces
C action workspace 2 >/dev/null
check "workspaces: switch to 2" "C workspaces | grep -q ' 2$'"
check "workspaces: the window is hidden on 1" "C windows | grep -q '^mist-files ws1 hidden'"
C action workspace 1 >/dev/null
check "workspaces: back on 1, the window shown" "C windows | grep -q '^mist-files ws1 [0-9]'"

# ------------------------------------------------------------------ Hyprland compatibility (mist-ui, scripts)
SIG=$(grep -m1 '^HYPRLAND_INSTANCE_SIGNATURE=' "$OUT/comp.log" | cut -d= -f2)
socat -u "UNIX-CONNECT:$RT/hypr/$SIG/.socket2.sock" - >"$OUT/events.log" 2>/dev/null &
pids="$pids $!"
sleep 0.3
ln -sf "$BIN/mist-comp" "$RT/hyprctl"
HY() { HYPRLAND_INSTANCE_SIGNATURE=$SIG "$RT/hyprctl" "$@" 2>/dev/null; }
check "hyprctl shim: monitors -j" "HY -j monitors | grep -q '\"name\":\"HDMI-A-1\"'"
check "hyprctl shim: getoption hyprbars height" "HY -j getoption plugin:hyprbars:bar_height | grep -q '\"int\":30'"
check "hyprctl shim: decorations" "HY -j decorations address:0x1 | grep -qx '\[\]'"
check "hyprctl shim: unknown request fails" "! HY bogus >/dev/null"
A=$(J j/clients | tr ',' '\n' | sed -n 's/.*"address":"\(0x[0-9a-f]*\)".*/\1/p' | head -1)
J "eval hl.dispatch(hl.dsp.window.move({ x = 300, y = 0, window = \"address:$A\" }))" >/dev/null
check "eval window.move: placed, kept below the bar" "C windows | grep -q '^mist-files ws1 300,'" "$(C windows | head -1)"
G=$(geo); Y4=${G#*,}
check "eval window.move: y not over the bar" "[ \"$Y4\" -ge \"${TOP:-0}\" ]" "y=$Y4"
J "eval hl.dispatch(hl.dsp.window.resize({ x = 900, y = 600, window = \"address:$A\" }))" >/dev/null
check "eval window.resize" "wait_for \"C windows | grep -q '^mist-files ws1 [0-9,]* 900x600'\" 50" "$(C windows | head -1)"
C motion 1900 400 >/dev/null; C motion 300 400 >/dev/null
check "event focusedmonv2 when the pointer changes output" "wait_for \"grep -q '^focusedmonv2>>HDMI-A-1' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.fullscreen({ window = \"address:$A\" }))" >/dev/null
check "eval window.fullscreen + event" "wait_for \"grep -q '^fullscreen>>1' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.fullscreen({ window = \"address:$A\" }))" >/dev/null
check "fullscreen off + event" "wait_for \"grep -q '^fullscreen>>0' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.close({ window = \"address:$A\" }))" >/dev/null
check "eval window.close of a given window" "wait_for \"! C windows | grep -q '^mist-files'\" 50"
check "event closewindow" "grep -q '^closewindow>>' '$OUT/events.log'"
C action reload >/dev/null
check "event configreloaded" "wait_for \"grep -q '^configreloaded>>' '$OUT/events.log'\" 30"

# ------------------------------------------------------------------ health
check "no panic in the compositor" "! grep -q panicked '$OUT/comp.log'"
check "no panic in the clients" "! grep -q panicked '$OUT/clients.log'"
check "compositor still answers" "C stats | grep -q frames"
echo "$(C stats | tr -d '\n') | out: $OUT"
[ $fails -eq 0 ] && echo "all ok" || { echo "$fails failed"; exit 1; }
