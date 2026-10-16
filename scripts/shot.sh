#!/bin/bash
# Mist screenshots: shot.sh screen|window|region [delay s] [copy 0|1]
mode=${1:-screen}; delay=${2:-0}; copy=${3:-1}
exec 2>>"${XDG_CACHE_HOME:-$HOME/.cache}/mist/shot.log"
echo "$(date +%T) $mode delay=$delay copy=$copy" >&2
# $MIST_SHOT_DIR, else Pictures/Screenshots (XDG, "Zrzuty ekranu" in Polish)
pics=$(xdg-user-dir PICTURES 2>/dev/null); [ -n "$pics" ] && [ "$pics" != "$HOME" ] || pics="$HOME/Pictures"
case "${LANG:-}" in pl*) sub="Zrzuty ekranu" ;; *) sub="Screenshots" ;; esac
dir="${MIST_SHOT_DIR:-$pics/$sub}"
mkdir -p "$dir"

sel() { slurp -d -b '#0b0f1860' -c '#a9c2eeff' -s '#a9c2ee22' -w 1 -B '#a9c2ee18' "$@"; }
sleep 0.25 # schowanie interfejsu paska

[ "$delay" -gt 0 ] 2>/dev/null && sleep "$delay"

ts=$(date +'%Y-%m-%d o %H.%M.%S')
f="$dir/Zrzut ekranu ${ts}.png"

if [ "$mode" = "screen" ]; then
    g=$(hyprctl monitors -j | python3 -c 'import json,sys;m=[m for m in json.load(sys.stdin) if m["focused"]][0];print("%d,%d %dx%d"%(m["x"],m["y"],m["width"],m["height"]))')
    grim -g "$g" "$f" || exit 1
    [ "$copy" = 1 ] && wl-copy --type image/png < "$f"
    echo "$f"
    exit 0
fi

# Tryb region lub window z zamrożeniem
tmp_screen="/tmp/screen_freeze_${UID}_$$.png"
grim "$tmp_screen" || exit 1

hyprpicker -r -z >/dev/null 2>&1 &
PICKER_PID=$!
sleep 0.05

case $mode in
    region)
        g=$(sel </dev/null 2>/dev/null)
        ;;
    window)
        g=$(hyprctl monitors -j | python3 -c '
import json, subprocess, sys
ws = {m["activeWorkspace"]["id"] for m in json.load(sys.stdin)}
for c in json.loads(subprocess.run(["hyprctl", "clients", "-j"], capture_output=True, text=True).stdout):
    if c["mapped"] and not c["hidden"] and c["workspace"]["id"] in ws:
        print("%d,%d %dx%d" % (*c["at"], *c["size"]))' | sel -r 2>/dev/null)
        ;;
esac

kill "$PICKER_PID" 2>/dev/null
wait "$PICKER_PID" 2>/dev/null

if [ -z "$g" ]; then
    rm -f "$tmp_screen"
    exit 1
fi

crop_geom=$(python3 -c "
import sys
g = sys.argv[1]
xy, wh = g.split(' ')
x, y = xy.split(',')
w, h = wh.split('x')
print(f'{w}x{h}+{x}+{y}')
" "$g" 2>/dev/null)

if [ -n "$crop_geom" ]; then
    magick "$tmp_screen" -crop "$crop_geom" +repage "$f"
fi
rm -f "$tmp_screen"

if [ ! -s "$f" ]; then
    exit 1
fi

[ "$copy" = 1 ] && wl-copy --type image/png < "$f"
echo "$f"
