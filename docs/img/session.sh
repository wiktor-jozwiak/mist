#!/bin/sh
# Real screenshots of a clean Mist session (Hyprland's blur, live wallpaper, the real surfaces), without touching
# the running one: a nested Hyprland, a window of the current session made fullscreen on its monitor for the
# shots (so its output has the monitor's size; ~20 s, do not touch the pointer meanwhile),
# a throwaway HOME with the demo data of docs/demo, a private D-Bus session (no notifications, players, phone
# or tray of yours) and its own XDG_RUNTIME_DIR (its own mist-ui socket and mist-wall lock). Needs Hyprland,
# grim, ImageMagick, mist-ui / mist-ui-bin / mist-wall-bin installed (native/install.sh), dbus-run-session.
#   docs/img/session.sh [outdir]          -> session-desktop.webp, session-control.webp
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$repo/docs/img}
case "$out" in /*) ;; *) out=$PWD/$out ;; esac
w=$(mktemp -d)
trap 'rm -rf "$w"' EXIT
[ -n "${WAYLAND_DISPLAY:-}" ] || { echo "run it inside a Wayland session" >&2; exit 1; }
case "$WAYLAND_DISPLAY" in /*) parent=$WAYLAND_DISPLAY ;; *) parent=$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY ;; esac

H=$w/home R=$w/run
mkdir -p "$H/.cache/mist" "$H/.config/mist" "$H/.config/quickshell" "$H/.config/hypr" "$H/.local/share/fonts" \
    "$H/.local/share/icons" "$R"
chmod 700 "$R"
cp "$repo"/docs/demo/weather.json "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$H/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$H/.config/mist/"
ln -s "$repo" "$H/.config/quickshell/mist"
# the installed binaries, where the guard and mist-ui look for them ($HOME/.local/bin)
mkdir -p "$H/.local/bin"
for b in mist-ui mist-ui-bin mist-wall-bin; do
    p=$(command -v "$b" || echo "$HOME/.local/bin/$b")
    [ -x "$p" ] || { echo "$b is not installed (native/install.sh)" >&2; exit 1; }
    ln -s "$p" "$H/.local/bin/$b"
done
[ -d "$HOME/.local/share/fonts/SF-Pro" ] && ln -s "$HOME/.local/share/fonts/SF-Pro" "$H/.local/share/fonts/SF-Pro"
[ -d "$HOME/.local/share/icons" ] && rmdir "$H/.local/share/icons" && ln -s "$HOME/.local/share/icons" "$H/.local/share/icons"
# the dock's desktop entries and squircle tiles (mistd tiles), where this machine has them
mkdir -p "$H/.local/share/applications" "$H/.cache/mist/tiles"
for e in mist-files mist-player; do
    [ -f "$HOME/.local/share/applications/$e.desktop" ] && ln -s "$HOME/.local/share/applications/$e.desktop" "$H/.local/share/applications/"
done
for t in @start @trash mist-files firefox kitty spotify org.gnome.Calculator btop; do
    [ -f "$HOME/.cache/mist/tiles/$t.png" ] && cp "$HOME/.cache/mist/tiles/$t.png" "$H/.cache/mist/tiles/"
done
# the adb server is shared by every session: an adb that sees no device keeps your phone out of the shots
printf '#!/bin/sh\nexit 1\n' >"$H/.local/bin/adb" && chmod +x "$H/.local/bin/adb"
[ -f "$HOME/.config/qt6ct/qt6ct.conf" ] && mkdir -p "$H/.config/qt6ct" && cp "$HOME/.config/qt6ct/qt6ct.conf" "$H/.config/qt6ct/"
# the demo place (the cached weather is Kraków's), English, the headless output as the main monitor
echo '{"city": "Kraków", "lat": 50.06, "lon": 19.94}' >"$H/.config/mist/location.json"
echo '{"lang": "en", "primary": "WAYLAND-1", "welcomed": true}' >"$H/.config/mist/settings.json"
echo '{"live": "dunes", "image": "", "tempo": "normal", "music": false}' >"$H/.config/mist/wallpaper.json"
echo '{"dock": ["mist-files", "firefox", "kitty", "spotify", "org.gnome.Calculator", "btop"], "groups": []}' >"$H/.config/mist/apps.json"

cat >"$H/.config/hypr/hyprland.lua" <<'EOF'
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
hl.config({
    general = { gaps_in = 3, gaps_out = 8, border_size = 0 },
    decoration = { rounding = 10, blur = { enabled = true, size = 5, passes = 3, popups = true } },
    animations = { enabled = false },
    misc = { disable_hyprland_logo = true, disable_splash_rendering = true, disable_watchdog_warning = true },
    cursor = { invisible = true },
})
EOF
# the glass rules of the example config
sed -n '/^hl.layer_rule/,$p' "$repo/examples/hyprland/mist.lua" >>"$H/.config/hypr/hyprland.lua"

# the nested compositor: its own runtime dir and D-Bus, the parent's display
# the parent's environment minus what ties it to the running session (a stripped one breaks GBM on NVIDIA)
clean() {
    unset HYPRLAND_INSTANCE_SIGNATURE DISPLAY DBUS_SESSION_BUS_ADDRESS XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME \
        XDG_STATE_HOME MIST_UI_SOCK MIST_DIR MIST_PRIMARY MIST_WALLPAPERS
    export HOME="$H" XDG_RUNTIME_DIR="$R" LANG=en_US.UTF-8 MIST_LANG=en
}
# a bus without service directories: nothing is D-Bus activated (kdeconnectd would find your phone on the LAN)
cat >"$w/bus.conf" <<'EOF'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
(clean; export WAYLAND_DISPLAY="$parent"
    dbus-run-session --config-file="$w/bus.conf" -- sh -c 'echo "$DBUS_SESSION_BUS_ADDRESS" >"$XDG_RUNTIME_DIR/bus.addr"; exec Hyprland -c "$HOME/.config/hypr/hyprland.lua"' \
    >"$w/hypr.log" 2>&1) &
hpid=$!
# everything of the nested session (Hyprland, mist-ui, mist-wall, mistd) has XDG_RUNTIME_DIR=$R
cleanup() {
    for e in /proc/[0-9]*/environ; do
        grep -qz "^XDG_RUNTIME_DIR=$R\$" "$e" 2>/dev/null && kill "$(basename "$(dirname "$e")")" 2>/dev/null || true
    done
    [ -n "${KEEP:-}" ] && echo "kept $w" >&2 || rm -rf "$w"
}
trap cleanup EXIT
wait_for() { # <seconds> <command...>: until the command succeeds
    n=$(($1 * 5)); shift
    until "$@" >/dev/null 2>&1; do
        n=$((n - 1)); [ $n -gt 0 ] || { echo "timed out: $*" >&2; cat "$w/ui.log" "$H/.cache/mist/ui.log" 2>/dev/null | tail -20 >&2; exit 1; }
        sleep 0.2
    done
}
wait_for 20 sh -c "ls '$R'/hypr/*/.socket.sock && test -s '$R/bus.addr'"
sig=$(basename "$(dirname "$(ls "$R"/hypr/*/.socket.sock | head -1)")")
nested() { (clean; export PATH="$H/.local/bin:$PATH" HYPRLAND_INSTANCE_SIGNATURE="$sig" WAYLAND_DISPLAY=wayland-1 \
    DBUS_SESSION_BUS_ADDRESS="$(cat "$R/bus.addr")"; exec "$@"); }
# its window in the running session, fullscreen: the nested output takes the monitor's size
hp=$(pgrep -f "Hyprland -c $H/.config/hypr/hyprland.lua" | head -1)
wait_for 10 sh -c "hyprctl clients -j | jq -e '.[] | select(.pid == $hp)'"
addr=$(hyprctl clients -j | jq -r ".[] | select(.pid == $hp) | .address")
hyprctl eval "hl.dispatch(hl.dsp.focus({ window = 'address:$addr' }))" >/dev/null
hyprctl eval "hl.dispatch(hl.dsp.window.fullscreen({ mode = 0 }))" >/dev/null
size=$(hyprctl clients -j | jq -r ".[] | select(.pid == $hp) | .size | map(tostring) | join(\"x\")")
wait_for 10 sh -c "XDG_RUNTIME_DIR='$R' HYPRLAND_INSTANCE_SIGNATURE='$sig' hyprctl monitors | grep -q '$size@'"
# mist-ui-bin itself: the guard would see the running session's mist-ui-bin and quit
nested setsid mist-ui-bin >"$w/ui.log" 2>&1 &
wait_for 30 test -S "$R/mist/ui.sock"
settle() { timeout "$1" tail -f /dev/null || true; } # animations and the first wallpaper frames
settle 4
shot() { # <name>
    nested grim -o WAYLAND-1 "$w/$1.png"
    magick "$w/$1.png" -strip -quality 90 "$out/session-$1.webp"
    echo "$out/session-$1.webp"
}
[ -n "${PAUSE:-}" ] && { echo "paused: R=$R sig=$sig H=$H" >&2; timeout "$PAUSE" tail -f /dev/null || true; }
shot desktop
nested mist-ui-bin msg mist toggle control >/dev/null; settle 3
shot control
