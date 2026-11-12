#!/bin/sh
# Desktop shells side by side, under identical conditions, without touching the running session.
#   bench/shells.sh [runs] [settle s] [window s] [shell...]      (defaults 3 60 60, every shell below)
#
# Each run of each shell: a fresh throwaway HOME and XDG_RUNTIME_DIR (so every shell starts with the
# configuration its package ships), a private D-Bus session bus, and a headless sway on the GPU (NVIDIA
# GLES renderer, two 1920x1080 outputs at 60 Hz: wlroots, not Hyprland or mist-comp, so no shell plays on
# home ground). The shell starts alone, settles, then bench/sample.py measures its whole process tree twice:
#   idle    nothing happens for the window
#   active  bench/activity.sh: a workspace switch every 2 s, a window opened every 6 s and closed 3 s later,
#           a notification every 10 s
# One JSON line per measurement goes to $OUT (default bench/results/shells-<date>.jsonl); bench/report.py
# turns the file into the table of docs/BENCHMARKS.md (median and range over the runs).
#
# The shells come from their distribution packages. Without root, bench/fetch.sh unpacks the Arch packages
# into ~/.local/share/mist-bench (hashes checked against pacman's sync database) and this script uses them.
set -u
here=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$here")
[ -f "$HOME/.local/share/mist-bench/env.sh" ] && . "$HOME/.local/share/mist-bench/env.sh"
runs=${1:-3}; settle=${2:-60}; secs=${3:-60}
[ $# -ge 3 ] && shift 3 || shift $#
OUT=${OUT:-$ROOT/bench/results/shells-$(date +%F).jsonl}
mkdir -p "$(dirname "$OUT")"
MIST_BIN=${MIST_BIN:-$ROOT/native/target/release/mist-ui}
XDGC=${R:+$R/etc/xdg:}/etc/xdg
# Waybar reads only /etc/xdg (not XDG_CONFIG_DIRS): its packaged default config, wherever it was unpacked
WB=/etc/xdg/waybar; [ -n "${R:-}" ] && [ -f "$R/etc/xdg/waybar/config.jsonc" ] && WB=$R/etc/xdg/waybar

PHOTO=/usr/share/backgrounds/sway/Sway_Wallpaper_Blue_1920x1080.png
[ -n "${R:-}" ] && [ -f "$R$PHOTO" ] && PHOTO=$R$PHOTO

# name | command (empty: swaybar, which sway itself starts from a `bar` block)
shells() {
    cat <<EOF
Mist|$MIST_BIN
Mist, photo wallpaper|$MIST_BIN
Waybar|waybar -c $WB/config.jsonc -s $WB/style.css
Ironbar|ironbar
nwg-panel|nwg-panel
swaybar|
Quickshell, empty PanelWindow|quickshell -p $here/empty-shell.qml
Noctalia|noctalia
EOF
}

version() {
    case "$1" in
        Mist*) git -C "$ROOT" describe --tags --always 2>/dev/null ;;
        Waybar) waybar --version 2>/dev/null ;;
        Ironbar) ironbar --version 2>/dev/null ;;
        nwg-panel) pacman -Q nwg-panel 2>/dev/null || ls "$HOME"/.local/share/mist-bench/pkg/nwg-panel-*.pkg.tar.zst 2>/dev/null | sed 's|.*/nwg-panel-\([^-]*-[^-]*\)-.*|nwg-panel \1|' ;;
        swaybar) sway --version 2>/dev/null ;;
        Quickshell*) quickshell --version 2>/dev/null | head -1 ;;
        Noctalia) pacman -Q noctalia 2>/dev/null ;;
    esac | head -1 | tr -d '"'
}

shells | while IFS='|' read -r name cmd; do
    if [ $# -gt 0 ]; then
        hit=0; for w in "$@"; do [ "$w" = "$name" ] && hit=1; done
        [ $hit = 1 ] || continue
    fi
    [ -z "$cmd" ] || command -v "${cmd%% *}" >/dev/null 2>&1 || { echo "skip $name: ${cmd%% *} not found"; continue; }
    ver=$(version "$name")
    r=1
    while [ $r -le "$runs" ]; do
        RT=$(mktemp -d /tmp/msb.XXXX) # short: unix socket paths are limited to 108 bytes
        H=$RT/home
        mkdir -p "$H/.cache/mist" "$H/.local/share/fonts/SF-Pro"
        chmod 700 "$RT"
        # audio of the real session (volume and media modules), nothing else of it
        [ -e "$XDG_RUNTIME_DIR/pipewire-0" ] && ln -s "$XDG_RUNTIME_DIR/pipewire-0" "$RT/"
        [ -e "$XDG_RUNTIME_DIR/pulse/native" ] && mkdir "$RT/pulse" && ln -s "$XDG_RUNTIME_DIR/pulse/native" "$RT/pulse/"
        # Mist: demo weather / fitness data (no network, no personal data) and a UI font where it looks first
        cp "$ROOT"/docs/demo/*.json "$H/.cache/mist/" 2>/dev/null
        for f in /usr/share/fonts/inter/InterVariable.ttf /usr/share/fonts/TTF/InterVariable.ttf /usr/share/fonts/noto/NotoSans-Regular.ttf; do
            [ -f "$f" ] && { ln -sf "$f" "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf"; break; }
        done
        # "Mist, photo wallpaper": sway's packaged picture instead of the live scene (what the other shells that
        # draw a wallpaper show: a still image), so mist-wall has nothing to animate
        case "$name" in "Mist, photo"*)
            mkdir -p "$H/.config/mist"
            printf '{"live":"","image":"%s"}\n' "$PHOTO" >"$H/.config/mist/wallpaper.json" ;;
        esac
        printf 'output HEADLESS-1 mode 1920x1080@60Hz pos 0 0\noutput HEADLESS-2 mode 1920x1080@60Hz pos 1920 0\n' >"$RT/sway.conf"
        [ -z "$cmd" ] && printf 'bar {\n    position top\n}\n' >>"$RT/sway.conf"
        q=0; until sh "$here/quiet.sh" 5 2>/dev/null; do   # a burst (a browser tab) passes: wait up to 10 min
            q=$((q+1)); [ "$q" -ge ${BENCH_WAIT:-40} ] && break; sleep 12
        done
        [ "$q" -gt 0 ] && echo "   (waited for a quiet machine: $q checks)"
        sh "$here/quiet.sh" 5 || { echo "bench: stopped before this run; quiet the machine and start again (BENCH_BUSY_OK=1 to measure anyway)" >&2; exit 1; }
        echo "== $name ($ver), run $r/$runs"
        env -i HOME="$H" XDG_RUNTIME_DIR="$RT" PATH="$(dirname "$MIST_BIN"):$PATH" LANG=C.UTF-8 USER="$USER" \
            LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" QT_PLUGIN_PATH="${QT_PLUGIN_PATH:-}" QML_IMPORT_PATH="${QML_IMPORT_PATH:-}" \
            QML2_IMPORT_PATH="${QML2_IMPORT_PATH:-}" XDG_DATA_DIRS="${XDG_DATA_DIRS:-/usr/share}" XDG_CONFIG_DIRS="$XDGC" \
            PYTHONPATH="${PYTHONPATH:-}" GI_TYPELIB_PATH="${GI_TYPELIB_PATH:-}" SHOTS="${SHOTS:-}" MIST_DIR="$ROOT" MIST_LANG=en MIST_PRIMARY=HEADLESS-1 \
            WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=2 WLR_RENDERER=gles2 WLR_LIBINPUT_NO_DEVICES=1 \
            WLR_RENDER_DRM_DEVICE="${WLR_RENDER_DRM_DEVICE:-$(ls /dev/dri/renderD* | head -1)}" \
            dbus-run-session -- sh "$here/shells-run.sh" "$name" "$ver" "$r" "$settle" "$secs" "$cmd" >>"$OUT" 2>"$RT/run.err" ||
            { echo "   failed: $(tail -12 "$RT/run.err")"; }
        tail -2 "$OUT" | python3 -c 'import json,sys
for l in sys.stdin:
    d=json.loads(l); t=d["total"]
    print("   %-6s pss %6.1f MB  uss %6.1f MB  cpu %5.2f %%  wake %6.1f/s  gpu %4d MiB  procs %d" % (d["scenario"], t["pss"], t["uss"], t["cpu"], t["wake"], t["gpu"], len(d["procs"])))' 2>/dev/null
        if [ -n "${KEEP:-}" ]; then echo "   kept $RT"; else rm -rf "${RT:?}"; fi
        r=$((r + 1))
    done
done
