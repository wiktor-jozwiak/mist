#!/bin/sh
# Wayland compositors side by side, headless on the GPU, with the same clients, without touching the session.
#   bench/compositors.sh [runs] [window s] [compositor...]      (defaults 3 30, every one below)
#
# Each run: a throwaway HOME and XDG_RUNTIME_DIR, a private D-Bus bus, the compositor's headless backend
# rendering with OpenGL ES on the GPU (the render node, NVIDIA here), 1920x1080 at 60 Hz, XWayland off.
# bench/sample.py measures the compositor's process tree in three windows:
#   idle  no client at all
#   shm   weston-simple-shm: a client drawing on the CPU every frame (what GTK / Qt software and Mist send)
#   egl   weston-simple-egl -f: a fullscreen OpenGL client synced to the compositor's frames; its own fps
#         count gives the compositor's CPU per frame
# One JSON line per measurement to $OUT (default bench/results/compositors-<date>.jsonl); bench/report.py
# makes the table. Hyprland and niri have no headless mode a script can start, so they are not in it.
set -u
here=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$here")
[ -f "$HOME/.local/share/mist-bench/env.sh" ] && . "$HOME/.local/share/mist-bench/env.sh"
runs=${1:-3}; secs=${2:-30}
[ $# -ge 2 ] && shift 2 || shift $#
OUT=${OUT:-$ROOT/bench/results/compositors-$(date +%F).jsonl}
mkdir -p "$(dirname "$OUT")"
MIST_COMP=${MIST_COMP:-$ROOT/native/target/release/mist-comp}
NODE=${NODE:-$(ls /dev/dri/renderD* | head -1)}

version() {
    case "$1" in
        sway) sway --version ;;
        labwc) labwc --version ;;
        weston) LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}${R:+:$R/usr/lib/weston}" weston --version ;;
        mist-comp) echo "mist-comp $(git -C "$ROOT" describe --tags --always 2>/dev/null)" ;;
    esac 2>/dev/null | head -1
}

for comp in sway labwc weston mist-comp; do
    if [ $# -gt 0 ]; then
        hit=0; for w in "$@"; do [ "$w" = "$comp" ] && hit=1; done
        [ $hit = 1 ] || continue
    fi
    case $comp in mist-comp) bin=$MIST_COMP ;; *) bin=$comp ;; esac
    command -v "$bin" >/dev/null 2>&1 || [ -x "$bin" ] || { echo "skip $comp: not found"; continue; }
    ver=$(version "$comp")
    r=1
    while [ "$r" -le "$runs" ]; do
        RT=$(mktemp -d /tmp/mcb.XXXX)
        mkdir -p "$RT/home"
        chmod 700 "$RT"
        q=0; until sh "$here/quiet.sh" 5 2>/dev/null; do   # a burst (a browser tab) passes: wait up to 10 min
            q=$((q+1)); [ "$q" -ge ${BENCH_WAIT:-40} ] && break; sleep 12
        done
        [ "$q" -gt 0 ] && echo "   (waited for a quiet machine: $q checks)"
        sh "$here/quiet.sh" 5 || { echo "bench: stopped before this run; quiet the machine and start again (BENCH_BUSY_OK=1 to measure anyway)" >&2; exit 1; }
        echo "== $comp ($ver), run $r/$runs"
        env -i HOME="$RT/home" XDG_RUNTIME_DIR="$RT" PATH="$PATH" LANG=C.UTF-8 USER="$USER" \
            LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}${R:+:$R/usr/lib/weston}" XDG_DATA_DIRS="${XDG_DATA_DIRS:-/usr/share}" \
            WESTON_MODULE_MAP="${R:+headless-backend.so=$R/usr/lib/libweston-15/headless-backend.so;gl-renderer.so=$R/usr/lib/libweston-15/gl-renderer.so;kiosk-shell.so=$R/usr/lib/weston/kiosk-shell.so}" \
            NODE="$NODE" MIST_COMP="$MIST_COMP" \
            dbus-run-session -- sh "$here/compositors-run.sh" "$comp" "$ver" "$r" "$secs" >>"$OUT" 2>"$RT/run.err" ||
            echo "   failed: $(tail -5 "$RT/run.err")"
        tail -3 "$OUT" | python3 -c 'import json,sys
for l in sys.stdin:
    d=json.loads(l); t=d["total"]
    if d.get("comp") != sys.argv[1] or d.get("run") != int(sys.argv[2]): continue
    print("   %-5s pss %6.1f MB  cpu %5.2f %%  wake %6.1f/s  gpu %4d MiB  %s" % (d["scenario"], t["pss"], t["cpu"], t["wake"], t["gpu"], ("client %.1f fps" % d["fps"]) if d.get("fps") else ""))' "$comp" "$r" 2>/dev/null
        if [ -n "${KEEP:-}" ]; then echo "   kept $RT"; else rm -rf "${RT:?}"; fi
        r=$((r + 1))
    done
done
