#!/bin/sh
# Rebuilds docs/img/desktop.webp from offscreen renders and the demo data in docs/demo (no live session,
# no personal data). Needs installed mist-ui-bin / mist-wall-bin, ImageMagick, SF Pro (or Inter).
#   docs/img/hero.sh            Polish UI
#   MIST_LANG=en docs/img/hero.sh docs/img/desktop-en.webp
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$repo/docs/img/desktop.webp}
w=$(mktemp -d)
trap 'rm -rf "$w"' EXIT

# a throwaway HOME: demo caches + settings, the repo and the fonts linked in
mkdir -p "$w/home/.cache/mist" "$w/home/.config/mist" "$w/home/.config/quickshell" "$w/home/.local/share/fonts" "$w/desk"
cp "$repo"/docs/demo/weather.json "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$w/home/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$w/home/.config/mist/"
ln -s "$repo" "$w/home/.config/quickshell/mist"
[ -d "$HOME/.local/share/fonts/SF-Pro" ] && ln -s "$HOME/.local/share/fonts/SF-Pro" "$w/home/.local/share/fonts/SF-Pro"

HOME=$w/home mist-wall-bin --png "$w/wall.ppm" dunes 1920 1080 >/dev/null 2>&1
HOME=$w/home mist-wall-bin --png "$w/sky.ppm" sky 546 310 >/dev/null 2>&1
HOME=$w/home mist-ui-bin --desk-png "$w/desk" >/dev/null 2>&1
mist-ui-bin --dock-png "$w/dock.png" >/dev/null 2>&1
mist-ui-bin --png "$w/bar.png" '{"app":"Firefox","title":"Mist README"}' >/dev/null 2>&1

# stage, weather card and the sky under it where the shell puts them (--desk-png writes layout.json);
# the sky window has rounded corners
read -r gx gy cx cy sx sy <<EOF
$(sed -E 's/.*"card":\[([0-9-]+),([0-9-]+)\].*"sky":\[([0-9-]+),([0-9-]+),[0-9]+,[0-9]+\].*"stage":\[([0-9-]+),([0-9-]+)\].*/\5 \6 \1 \2 \3 \4/' "$w/desk/layout.json")
EOF
magick "$w/sky.ppm" \( -size 546x310 xc:black -fill white -draw "roundrectangle 0,0 545,309 25,25" \) \
    -alpha off -compose CopyOpacity -composite "$w/sky.png"
magick "$w/wall.ppm" "$w/desk/stage.png" -geometry +$gx+$gy -composite \
    "$w/sky.png" -geometry +$sx+$sy -composite \
    "$w/desk/card.png" -geometry +$cx+$cy -composite \
    "$w/dock.png" -gravity south -geometry +0+8 -composite \
    "$w/bar.png" -gravity north -geometry +0+0 -composite \
    -resize 1600x -strip -quality 92 "$out"
echo "$out"
