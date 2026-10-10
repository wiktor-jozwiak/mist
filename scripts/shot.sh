#!/bin/sh
# Mist screenshots (Shot.qml): shot.sh screen|window|region [delay s] [copy 0|1] -> prints the saved file.
# Saved to ~/Obrazy/Zrzuty ekranu like macOS ("Zrzut ekranu 2026-10-09 o 05.12.03.png"), optionally copied.
mode=${1:-screen}; delay=${2:-0}; copy=${3:-1}
exec 2>>"${XDG_CACHE_HOME:-$HOME/.cache}/mist/shot.log"
echo "$(date +%T) $mode delay=$delay copy=$copy" >&2
dir="$HOME/Obrazy/Zrzuty ekranu"
mkdir -p "$dir"
sel() { slurp -d -b '#0b0f1860' -c '#a9c2eeff' -s '#a9c2ee22' -w 2 -B '#a9c2ee18' "$@"; }
sleep 0.3                                   # let the toolbar layer disappear
case $mode in
    region) g=$(sel </dev/null) || exit 1 ;;   # slurp reads boxes from a non-tty stdin: give it EOF
    window) g=$(hyprctl monitors -j | python3 -c '
import json, subprocess, sys
ws = {m["activeWorkspace"]["id"] for m in json.load(sys.stdin)}
for c in json.loads(subprocess.run(["hyprctl", "clients", "-j"], capture_output=True, text=True).stdout):
    if c["mapped"] and not c["hidden"] and c["workspace"]["id"] in ws:
        print("%d,%d %dx%d" % (*c["at"], *c["size"]))' | sel -r) || exit 1 ;;
    *) g=$(hyprctl monitors -j | python3 -c 'import json,sys;m=[m for m in json.load(sys.stdin) if m["focused"]][0];print("%d,%d %dx%d"%(m["x"],m["y"],m["width"],m["height"]))') ;;
esac
[ "$delay" -gt 0 ] 2>/dev/null && sleep "$delay"
f="$dir/Zrzut ekranu $(date +'%Y-%m-%d o %H.%M.%S').png"
echo "geometry: $g" >&2
grim -g "$g" "$f" || exit 1
[ "$copy" = 1 ] && wl-copy --type image/png <"$f"
echo "$f"
