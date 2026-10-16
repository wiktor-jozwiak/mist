#!/bin/sh
# Rebuilds the README's panel shots (docs/img/panels/*.webp): each panel rendered offscreen on a 1920 x 1080
# monitor (mist-ui --pane-png), cut to what it draws plus a margin, over the dunes wallpaper. Demo data and an
# English session only (no notifications, clipboard or account names). Needs mist-ui-bin / mist-wall-bin
# (or $MIST_UI / $MIST_WALL), ImageMagick.
#   docs/img/panels.sh [outdir]
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$repo/docs/img/panels}
ui=${MIST_UI:-mist-ui-bin}
wall=${MIST_WALL:-mist-wall-bin}
w=$(mktemp -d)
trap 'rm -rf "$w"' EXIT
mkdir -p "$out" "$w/home/.cache/mist" "$w/home/.config/mist" "$w/home/.config/quickshell" "$w/home/.local/share/fonts"
cp "$repo"/docs/demo/weather.json "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$w/home/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$w/home/.config/mist/"
ln -s "$repo" "$w/home/.config/quickshell/mist"
[ -d "$HOME/.local/share/fonts/SF-Pro" ] && ln -s "$HOME/.local/share/fonts/SF-Pro" "$w/home/.local/share/fonts/SF-Pro"
export LANG=en_US.UTF-8 MIST_LANG=en

HOME=$w/home "$wall" --png "$w/bg.ppm" dunes 1920 1080 >/dev/null 2>&1
# <panel> [query]
shot() {
    HOME=$w/home "$ui" --pane-png "$1" 1920 1080 "$w/$1.png" ${2:+"$2"} >/dev/null 2>&1
    # what it draws (alpha > 3 %), 48 px around it, inside the monitor
    g=$(magick "$w/$1.png" -alpha extract -threshold 3% -format '%@' info:)
    set -- "$1" $(echo "$g" | sed -E 's/([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)/\1 \2 \3 \4/')
    x=$(($4 > 48 ? $4 - 48 : 0)); y=$(($5 > 48 ? $5 - 48 : 0))
    cw=$(($2 + 96)); ch=$(($3 + 96))
    [ $((x + cw)) -gt 1920 ] && cw=$((1920 - x))
    [ $((y + ch)) -gt 1080 ] && ch=$((1080 - y))
    # Hyprland's blur under the glass (layer rule ignore_alpha 0.25; blur size 5, 3 passes ~ a 18 px gaussian,
    # its default brightness 0.82 / contrast 0.89), then the panel over it
    magick "$w/bg.ppm" \( +clone -blur 0x18 -brightness-contrast -8x-6 \) \
        \( "$w/$1.png" -alpha extract -threshold 25% \) -composite "$w/$1.bg.png"
    magick "$w/$1.bg.png" "$w/$1.png" -composite -crop "${cw}x${ch}+${x}+${y}" +repage \
        \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "roundrectangle 0,0 $((cw - 1)),$((ch - 1)) 18,18" \) \
        -alpha off -compose CopyOpacity -composite -strip -quality 92 "$out/$1.webp"
    echo "$out/$1.webp"
}
shot spotlight te
shot control
shot calendar
shot emoji
