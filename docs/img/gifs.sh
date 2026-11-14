#!/bin/sh
# Rebuilds the README's animations from offscreen renders and the demo data in docs/demo (no live session,
# no personal data): docs/img/wallpaper.webp (a live wallpaper), weather.webp (the weather card in the rain),
# island.webp (Dynamic Island). Needs mist-ui-bin / mist-wall-bin (or $MIST_UI / $MIST_WALL), ImageMagick, ffmpeg.
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

# frames to a looping animated WebP (GitHub plays it in an <img>; alpha for the rounded corners, a tenth of
# a GIF's size)
anim() { # <frames pattern> <fps> <out>; once more if ffmpeg crashes (it did once, at random)
    for _ in 1 2; do
        ffmpeg -loglevel error -y -framerate "$2" -i "$1" -c:v libwebp_anim -pix_fmt yuva420p -quality 82 \
            -compression_level 6 -loop 0 "$3" && return
    done
    return 1
}
# the README's 16:10 tile (800 x 500, as the stills of docs/img/panels.sh at 1280 x 800), rounded corners
magick -size 800x500 xc:black -fill white -draw "roundrectangle 0,0 799,499 18,18" "$w/tile.png"
round() { magick "$1" "$w/tile.png" -alpha off -compose CopyOpacity -composite "$2"; }

# 1. the live wallpaper: 4 s of dunes, 12 fps
HOME=$w/home MIST_WALL_FRAMES=48 MIST_WALL_FPS=12 "$wall" --png "$w/f/wall" dunes 800 500 >/dev/null 2>&1
for f in "$w"/f/wall.*.ppm; do n=${f##*/wall.}; round "$f" "$w/f/wallr.${n%.ppm}.png"; done
anim "$w/f/wallr.%03d.png" 12 "$out/wallpaper.webp"

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
# the tile around the card, inside the monitor
tx=$((cx + cw / 2 - 400)); ty=$((cy + ch / 2 - 250))
[ $tx -lt 0 ] && tx=0; [ $ty -lt 0 ] && ty=0; [ $tx -gt 1120 ] && tx=1120; [ $ty -gt 580 ] && ty=580
magick "$w/bg.ppm" -crop "800x500+$tx+$ty" +repage "$w/cardbg.png"
for f in "$w"/f/sky.*.ppm; do
    n=${f##*/sky.}
    magick "$w/cardbg.png" \( "$f" "$w/mask.png" -alpha off -compose CopyOpacity -composite \) \
        -compose over -geometry +$((sx - tx))+$((sy - ty)) -composite \
        "$w/desk/card.png" -geometry +$((cx - tx))+$((cy - ty)) -composite \
        "$w/tile.png" -alpha off -compose CopyOpacity -composite "$w/f/card.${n%.ppm}.png"
done
anim "$w/f/card.%03d.png" 12 "$out/weather.webp"

# 3. Dynamic Island: a notification, the volume, rain soon, the next event, on a wide strip of the top of the
# wallpaper (1120 x 140, the island's 560 x 240 canvas in the middle, where the shell puts it)
magick "$w/bg.ppm" -crop 1120x140+400+0 +repage "$w/islbg.png"
magick -size 1120x140 xc:black -fill white -draw "roundrectangle 0,0 1119,139 18,18" "$w/strip.png"
i=0
for k in notif volume rain cal; do
    "$ui" --island-frames "$k" "$w/f/isl-$k" 30 20 >/dev/null 2>&1
    for f in "$w"/f/isl-"$k".*.png; do
        magick "$w/islbg.png" "$f" -geometry +280+0 -composite \
            "$w/strip.png" -alpha off -compose CopyOpacity -composite "$(printf '%s/f/island.%03d.png' "$w" "$i")"
        i=$((i + 1))
    done
done
anim "$w/f/island.%03d.png" 20 "$out/island.webp"

ls -la "$out/wallpaper.webp" "$out/weather.webp" "$out/island.webp"
