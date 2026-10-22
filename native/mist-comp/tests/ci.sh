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
export XDG_RUNTIME_DIR=$RT HOME=$H MIST_DIR=$ROOT MIST_LANG=en LANG=C.UTF-8 MIST_PRIMARY=DP-3
export MIST_UI_NO_SERVICES=1 MIST_UI_SOCK=$RT/ui.sock MIST_COMP_SOFTWARE=1
unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
# the Mist session may hide Mesa from EGL (start-mist on NVIDIA): llvmpipe needs it
unset __EGL_VENDOR_LIBRARY_FILENAMES
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
SIG=$(grep -m1 '^HYPRLAND_INSTANCE_SIGNATURE=' "$OUT/comp.log" | cut -d= -f2)
socat -u "UNIX-CONNECT:$RT/hypr/$SIG/.socket2.sock" - >"$OUT/events.log" 2>/dev/null &
pids="$pids $!"
check "compositor up, 2 outputs" "J j/monitors | grep -o '\"model\"' | wc -l | grep -qx 2"

# ------------------------------------------------------------------ protocols a desktop needs
WAYLAND_DISPLAY=$S "$BIN/mist-ui" --wayland-globals >"$OUT/globals.txt" 2>/dev/null
for g in xdg_wm_base zwlr_layer_shell_v1 ext_session_lock_manager_v1 zwp_linux_dmabuf_v1 wp_viewporter wp_fractional_scale_manager_v1 \
         wp_presentation xdg_activation_v1 zxdg_decoration_manager_v1 zwp_pointer_constraints_v1 zwp_relative_pointer_manager_v1 \
         zwp_text_input_manager_v3 zwp_input_method_manager_v2 zwp_virtual_keyboard_manager_v1 zwp_tablet_manager_v2 \
         ext_foreign_toplevel_list_v1 ext_data_control_manager_v1 zwlr_data_control_manager_v1 zxdg_exporter_v2 zxdg_importer_v2 \
         xdg_toplevel_tag_manager_v1 wp_content_type_manager_v1 wp_alpha_modifier_v1 xdg_system_bell_v1 \
         wp_security_context_manager_v1 wp_cursor_shape_manager_v1 zwlr_screencopy_manager_v1 ext_idle_notifier_v1 zwp_idle_inhibit_manager_v1 \
         zwlr_foreign_toplevel_manager_v1 zwlr_output_power_manager_v1 zwlr_virtual_pointer_manager_v1 zwlr_gamma_control_manager_v1 zwlr_output_manager_v1; do
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
# a window already at the very top before the bar exists: the bar pushes it down
spawn "$BIN/mist-ui" files "$H" >/dev/null
wait_for "C windows | grep -q '^mist-files ws'" 100
A0=$(J j/clients | tr ',' '\n' | sed -n 's/.*"address":"\(0x[0-9a-f]*\)".*/\1/p' | head -1)
J "eval hl.dispatch(hl.dsp.window.move({ x = 100, y = 0, window = \"address:$A0\" }))" >/dev/null
Y0=$(C windows | awk '/^mist-files ws/ {print $3; exit}'); Y0=${Y0#*,}
spawn "$BIN/mist-ui" >/dev/null
check "shell: bar on DP-3" "wait_for \"C layers | grep -q '^DP-3 mist-bar'\" 100"
check "shell: dock on DP-3" "C layers | grep -q '^DP-3 mist-dock'"
check "shell: island on DP-3" "C layers | grep -q '^DP-3 mist-island'"
check "shell: bar on HDMI-A-1" "C layers | grep -q '^HDMI-A-1 mist-bar'"
TOP=$(J j/monitors | tr -d ' \n' | sed -n 's/.*"reserved":\[\([0-9]*\),\([0-9]*\),.*/\2/p')
check "shell: the bar reserves the top" "[ \"\${TOP:-0}\" -gt 0 ]" "reserved top ${TOP:-?}"
Y1=$(C windows | awk '/^mist-files ws/ {print $3; exit}'); Y1=${Y1#*,}
check "window placed before the bar is pushed below it" "[ \"$Y0\" -lt \"${TOP:-0}\" ] && [ \"$Y1\" -ge \"${TOP:-0}\" ]" "y $Y0 -> $Y1, top ${TOP:-?}"
J "eval hl.dispatch(hl.dsp.window.close({ window = \"address:$A0\" }))" >/dev/null
wait_for "! C windows | grep -q '^mist-files'" 50

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
check "window drag: the cursor streamed to mist-ui (mistdrag move)" "grep -q '^mistdrag>>move,' '$OUT/events.log'"
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

# ------------------------------------------------------------------ typing reaches the window (wev, if installed)
if command -v wev >/dev/null; then
    WAYLAND_DISPLAY=$S stdbuf -oL wev >"$OUT/wev.log" 2>&1 &
    pids="$pids $!"
    wait_for "C windows | grep -q '^wev '" 100
    WG=$(C windows | awk '/^wev / {print $3; exit}'); WX=${WG%,*}; WY=${WG#*,}
    C motion $((WX + 60)) $((WY + 60)) >/dev/null; C button left press >/dev/null; C button left release >/dev/null
    for k in 30 48; do C key $k press >/dev/null; C key $k release >/dev/null; done
    check "typing: keys reach the focused window" "wait_for \"grep -q \\\"utf8: 'b'\\\" '$OUT/wev.log'\" 30"
    check "typing: comp.log says where the keys go" "grep -q 'keyboard focus: window wev' '$OUT/comp.log'"
    # another window takes the focus; a click on wev brings the keys back
    NF=$(C windows | grep -c '^mist-files')
    spawn "$BIN/mist-ui" files "$H" >/dev/null
    wait_for "[ \$(C windows | grep -c '^mist-files') -gt $NF ]" 100
    sleep 0.5
    # every mist-files window (one is left from the checks above) off wev, onto the other monitor
    for MF in $(J j/clients | tr '{' '\n' | grep '"class":"mist-files"' | sed -n 's/.*"address":"\(0x[0-9a-f]*\)".*/\1/p'); do
        J "eval hl.dispatch(hl.dsp.window.move({ x = 1400, y = 100, window = \"address:$MF\" }))" >/dev/null
    done
    sleep 0.5
    check "typing: the new window took the keys" "tail -3 '$OUT/comp.log' | grep -q 'keyboard focus: window mist-files'"
    C motion $((WX + 60)) $((WY + 60)) >/dev/null; C button left press >/dev/null; C button left release >/dev/null
    C key 46 press >/dev/null; C key 46 release >/dev/null
    check "typing: a click gives the keys back" "wait_for \"grep -q \\\"utf8: 'c'\\\" '$OUT/wev.log'\" 30"
    pkill -x wev; C action close >/dev/null
    wait_for "! C windows | grep -q '^mist-files'" 50
else
    echo "skip typing checks (no wev)"
fi

# ------------------------------------------------------------------ screencopy (grim, portals)
if command -v grim >/dev/null; then
    check "screencopy: grim works" "WAYLAND_DISPLAY=$S timeout 10 grim -o DP-3 -t ppm '$OUT/grim-dp3.ppm'"
    # on a still screen (island / window animations over: the compositor's shot the same before and after);
    # llvmpipe on a CI runner takes a while to settle
    same=1
    for _ in $(seq 30); do
        C shot "$OUT/shot-a.ppm" DP-3 >/dev/null
        WAYLAND_DISPLAY=$S timeout 10 grim -o DP-3 -t ppm "$OUT/grim-dp3.ppm"
        C shot "$OUT/shot-b.ppm" DP-3 >/dev/null
        cmp -s "$OUT/shot-a.ppm" "$OUT/shot-b.ppm" || { sleep 0.5; continue; }
        cmp -s "$OUT/shot-a.ppm" "$OUT/grim-dp3.ppm" && same=0
        break
    done
    check "screencopy: the same pixels as the compositor's own shot" "[ $same = 0 ]"
    check "screencopy: region" "WAYLAND_DISPLAY=$S timeout 10 grim -g '10,10 200x100' -t ppm '$OUT/grim-region.ppm' && head -c 20 '$OUT/grim-region.ppm' | grep -q '200 100'"
fi

# ------------------------------------------------------------------ wlroots tools (task bars, DPMS, remote input)
if command -v wlrctl >/dev/null; then
    W() { WAYLAND_DISPLAY=$S timeout 5 wlrctl "$@" 2>/dev/null; }
    check "foreign-toplevel: wlrctl lists the window" "W toplevel list | grep -q '^mist-files:'"
    W toplevel minimize app_id:mist-files
    check "foreign-toplevel: minimize" "wait_for \"C windows | grep -q '^mist-files ws4294967295 hidden'\" 30"
    W toplevel focus app_id:mist-files
    check "foreign-toplevel: activate restores it" "wait_for \"C windows | grep -q '^mist-files ws1 [0-9]'\" 30"
    C motion 100 100 >/dev/null; W pointer move 40 20
    check "virtual-pointer: relative motion" "J j/cursorpos | grep -q '\"x\":140,\"y\":120'"
fi
if command -v wlopm >/dev/null; then
    WAYLAND_DISPLAY=$S timeout 5 wlopm --off DP-3 >/dev/null 2>&1
    check "output-power: off" "WAYLAND_DISPLAY=$S timeout 5 wlopm | grep -q 'DP-3 off'"
    WAYLAND_DISPLAY=$S timeout 5 wlopm --on '*' >/dev/null 2>&1
    check "output-power: on" "WAYLAND_DISPLAY=$S timeout 5 wlopm | grep -q 'DP-3 on'"
fi

if command -v wlr-randr >/dev/null; then
    R() { WAYLAND_DISPLAY=$S timeout 5 wlr-randr "$@" 2>/dev/null; }
    check "output-management: wlr-randr lists both outputs" "R | grep -c '^[A-Z].*\"' | grep -qx 2"
    R --output HDMI-A-1 --pos 1300,0 --scale 1.25
    check "output-management: position and scale applied" "J j/monitors | tr -d ' ' | grep -q '\"scale\":1.25' && R | grep -q 'Position: 1300,0'"
    R --output HDMI-A-1 --pos 1280,0 --scale 1
    check "output-management: turning an output off is refused" "! R --output HDMI-A-1 --off"
fi

# ------------------------------------------------------------------ workspaces
C action workspace 2 >/dev/null
check "workspaces: switch to 2" "wait_for \"C workspaces | grep -q ' 2$'\" 30"
check "workspaces: the window is hidden on 1" "wait_for \"C windows | grep -q '^mist-files ws1 hidden'\" 30"
C action workspace 1 >/dev/null
check "workspaces: back on 1, the window shown" "wait_for \"C windows | grep -q '^mist-files ws1 [0-9]'\" 30"

# ------------------------------------------------------------------ Hyprland compatibility (mist-ui, scripts)
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
WAYLAND_DISPLAY=$S "$BIN/mist-ui" --wincap "$A" "$OUT/wincap.ppm" >"$OUT/wincap.txt" 2>&1
check "toplevel export: the window at its size (dock picker thumbnails)" "grep -qx '900x600' '$OUT/wincap.txt'" "$(cat "$OUT/wincap.txt")"
check "toplevel export: real pixels, not blank" "[ \$(tail -c 100000 '$OUT/wincap.ppm' | od -An -tu1 -v | tr -s ' ' '\n' | sort -u | wc -l) -gt 8 ]"
check "toplevel export: an unknown window fails" "! WAYLAND_DISPLAY=$S '$BIN/mist-ui' --wincap 0xdead '$OUT/none.ppm' >/dev/null 2>&1"
# wp_presentation: every frame gets its feedback (a client that waits for it before drawing on never hangs)
WAYLAND_DISPLAY=$S timeout 20 "$BIN/mist-ui" --present 30 >"$OUT/present.txt" 2>&1
check "presentation feedback: 30 frames, all presented" "grep -q '^presented 30 ' '$OUT/present.txt'" "$(cat "$OUT/present.txt")"
C motion 1900 400 >/dev/null; C motion 300 400 >/dev/null
check "event focusedmonv2 when the pointer changes output" "wait_for \"grep -q '^focusedmonv2>>HDMI-A-1' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.fullscreen({ window = \"address:$A\" }))" >/dev/null
check "eval window.fullscreen + event" "wait_for \"grep -q '^fullscreen>>1' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.fullscreen({ window = \"address:$A\" }))" >/dev/null
check "fullscreen off + event" "wait_for \"grep -q '^fullscreen>>0' '$OUT/events.log'\" 30"
J "eval hl.dispatch(hl.dsp.window.close({ window = \"address:$A\" }))" >/dev/null
check "eval window.close of a given window" "wait_for \"! C windows | grep -q '^mist-files'\" 50"
check "event closewindow" "grep -q '^closewindow>>' '$OUT/events.log'"
# global shortcuts (portal / Discord push-to-talk): a `global` bind presses and releases what the app registered
printf 'bind SUPER+F9 global mist.test:ptt\n' >"$H/.config/mist/comp.conf"
C action reload >/dev/null
WAYLAND_DISPLAY=$S "$BIN/mist-ui" --global-shortcut mist.test ptt >"$OUT/gs.txt" 2>&1 &
GS=$!; pids="$pids $GS"
wait_for "grep -q registered '$OUT/gs.txt'" 50
check "global shortcuts: hyprctl globalshortcuts lists it" "wait_for \"J globalshortcuts | grep -q '^mist.test:ptt'\" 30"
C key 125 press >/dev/null; C key 67 press >/dev/null
check "global shortcuts: pressed on the key press" "wait_for \"grep -q '^pressed' '$OUT/gs.txt'\" 30"
check "global shortcuts: not released while held" "! grep -q '^released' '$OUT/gs.txt'"
C key 125 release >/dev/null; C key 67 release >/dev/null
check "global shortcuts: released with the key" "wait_for \"grep -q '^released' '$OUT/gs.txt'\" 30"
kill $GS 2>/dev/null; sleep 0.3
check "global shortcuts: gone with its client" "! J globalshortcuts | grep -q mist.test"
printf 'output HDMI-A-1 1290 0 scale 1.5\n' >"$H/.config/mist/comp.conf"
C action reload >/dev/null
check "config: output line applied on reload (position, scale)" "J j/monitors | tr -d ' ' | grep -q '\"scale\":1.5' && J j/monitors | tr -d ' ' | grep -q '\"x\":1290'"
rm -f "$H/.config/mist/comp.conf"
check "event configreloaded" "wait_for \"grep -q '^configreloaded>>' '$OUT/events.log'\" 30"

# ------------------------------------------------------------------ idle cost
# not across a minute: the island animates its clock then (~250 ms of frames, real work)
sec=$(date +%-S); [ "$sec" -ge 53 ] && sleep $((62 - sec))
C motion 640 400 >/dev/null; sleep 2
S0=$(C stats); w0=$(echo "$S0" | awk '{print $10}'); sleep 3; S1=$(C stats); w1=$(echo "$S1" | awk '{print $10}')
# the shell shares the session bus: music playing on this machine animates the island's equalizer
playing=$(busctl --user list 2>/dev/null | awk '/org.mpris.MediaPlayer2/ {print $1}' | while read -r n; do
    busctl --user get-property "$n" /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlaybackStatus 2>/dev/null; done | grep -c Playing)
if [ "${playing:-0}" -gt 0 ]; then
    echo "skip idle: a media player is playing on this machine (the island animates); $((w1 - w0)) wakeups in 3 s"
else
    check "idle: the compositor sleeps (< 10 wakeups/s with the shell up)" "[ \$((w1 - w0)) -lt 30 ]" "$((w1 - w0)) wakeups in 3 s"
fi
echo "     idle wakeups in 3 s: $((w1 - w0)) ($S0 -> $S1)"

# ------------------------------------------------------------------ health
check "no panic in the compositor" "! grep -q panicked '$OUT/comp.log'"
check "no panic in the clients" "! grep -q panicked '$OUT/clients.log'"
check "compositor still answers" "C stats | grep -q frames"
echo "$(C stats | tr -d '\n') | out: $OUT"
[ $fails -eq 0 ] && echo "all ok" || { echo "$fails failed"; exit 1; }
