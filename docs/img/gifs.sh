#!/bin/sh
# Rebuilds the README's animations from offscreen renders and the demo data in docs/demo (no live session,
# no personal data): docs/img/wallpaper.gif (a live wallpaper), weather.gif (the weather card in the rain),
# island.gif (Dynamic Island). Needs mist-ui-bin / mist-wall-bin (or $MIST_UI / $MIST_WALL), ImageMagick, ffmpeg.
#   MIST_LANG=en docs/img/gifs.sh [outdir]
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$repo/docs/img}
ui=${MIST_UI:-mist-ui-bin}
wall=${MIST_WALL:-mist-wall-bin}
w=$(mktemp -d)
trap 'rm -rf "$w"' EXIT

# a throwaway HOME (as hero.sh): demo caches + settings, the repo and the fonts linked in
mkdir -p "$w/home/.cache/mist" "$w/home/.config/mist" "$w/home/.config/quickshell" "$w/home/.local/share/fonts" \
    "$w/desk" "$w/f"
cp "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$w/home/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$w/home/.config/mist/"
ln -s "$repo" "$w/home/.config/quickshell/mist"
[ -d "$HOME/.local/share/fonts/SF-Pro" ] && ln -s "$HOME/.local/share/fonts/SF-Pro" "$w/home/.local/share/fonts/SF-Pro"
# the demo weather turned to rain: overcast, moderate rain, 30 min of it ahead
sed -e 's/"code": *[0-9]*/"code": 63/' -e 's/"cloud": *[0-9]*/"cloud": 96/' -e 's/"precip": *[0-9.]*/"precip": 2.4/' \
    -e 's/"desc": *"[^"]*"/"desc": "Rain"/' "$repo/docs/demo/weather.json" >"$w/home/.cache/mist/weather.json"

# frames to a looping gif (one palette for all frames, no dithering noise on the gradients)
gif() { # <frames pattern> <fps> <out>; once more if ffmpeg crashes (it did once, at random)
    set -- "$1" "$2" "$3" "split[a][b];[a]palettegen=max_colors=192:stats_mode=full[p];[b][p]paletteuse=dither=bayer:bayer_scale=4"
    ffmpeg -loglevel error -y -framerate "$2" -i "$1" -vf "$4" -loop 0 "$3" ||
        ffmpeg -loglevel error -y -framerate "$2" -i "$1" -vf "$4" -loop 0 "$3"
}

# 1. the live wallpaper: 4 s of dunes, 12 fps, 960 x 540
HOME=$w/home MIST_WALL_FRAMES=48 MIST_WALL_FPS=12 "$wall" --png "$w/f/wall" dunes 960 540 >/dev/null 2>&1
gif "$w/f/wall.%03d.ppm" 12 "$out/wallpaper.gif"

# 2. the weather card over its sky in the rain, where the shell puts them (layout.json from --desk-png)
HOME=$w/home "$ui" --desk-png "$w/desk" >/dev/null 2>&1
HOME=$w/home "$wall" --png "$w/bg.ppm" dunes 1920 1080 >/dev/null 2>&1
read -r cx cy sx sy sw sh <<EOF
$(sed -E 's/.*"card":\[([0-9-]+),([0-9-]+)\].*"sky":\[([0-9-]+),([0-9-]+),([0-9]+),([0-9]+)\].*/\1 \2 \3 \4 \5 \6/' "$w/desk/layout.json")
EOF
cw=$(magick identify -format %w "$w/desk/card.png")
ch=$(magick identify -format %h "$w/desk/card.png")
HOME=$w/home MIST_WALL_FRAMES=48 MIST_WALL_FPS=12 "$wall" --png "$w/f/sky" sky "$sw" "$sh" >/dev/null 2>&1
magick -size "${sw}x${sh}" xc:black -fill white -draw "roundrectangle 0,0 $((sw - 1)),$((sh - 1)) 25,25" "$w/mask.png"
magick "$w/bg.ppm" -crop "${cw}x${ch}+${cx}+${cy}" +repage "$w/cardbg.png"
for f in "$w"/f/sky.*.ppm; do
    n=${f##*/sky.}
    magick "$w/cardbg.png" \( "$f" "$w/mask.png" -alpha off -compose CopyOpacity -composite \) \
        -compose over -geometry +$((sx - cx))+$((sy - cy)) -composite "$w/desk/card.png" -composite \
        "$w/f/card.${n%.ppm}.png"
done
gif "$w/f/card.%03d.png" 12 "$out/weather.gif"

# 3. Dynamic Island: a notification, the volume, rain soon, the next event, on the top of the wallpaper
magick "$w/bg.ppm" -crop 560x240+680+0 +repage "$w/islbg.png"
i=0
for k in notif volume rain cal; do
    "$ui" --island-frames "$k" "$w/f/isl-$k" 30 20 >/dev/null 2>&1
    for f in "$w"/f/isl-"$k".*.png; do
        magick "$w/islbg.png" "$f" -composite "$(printf '%s/f/island.%03d.png' "$w" "$i")"
        i=$((i + 1))
    done
done
gif "$w/f/island.%03d.png" 20 "$out/island.gif"

ls -la "$out/wallpaper.gif" "$out/weather.gif" "$out/island.gif"
