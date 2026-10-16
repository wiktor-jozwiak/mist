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
# a demo dock (pinned apps only: no running windows of this session) with the icon theme of this machine
echo '{"dock": ["mist-files", "firefox", "kitty", "spotify", "org.gnome.Calculator", "btop"], "groups": []}' >"$w/home/.config/mist/apps.json"
mkdir -p "$w/home/.local/share/applications" "$w/home/.config/qt6ct"
for e in mist-files mist-player; do
    [ -f "$HOME/.local/share/applications/$e.desktop" ] && ln -s "$HOME/.local/share/applications/$e.desktop" "$w/home/.local/share/applications/"
done
[ -f "$HOME/.config/qt6ct/qt6ct.conf" ] && cp "$HOME/.config/qt6ct/qt6ct.conf" "$w/home/.config/qt6ct/"
[ -d "$HOME/.local/share/icons" ] && ln -s "$HOME/.local/share/icons" "$w/home/.local/share/icons"
# the dock's squircle tiles (mistd tiles), where this machine has them
mkdir -p "$w/home/.cache/mist/tiles"
for t in @start @trash mist-files firefox kitty spotify org.gnome.Calculator btop; do
    [ -f "$HOME/.cache/mist/tiles/$t.png" ] && cp "$HOME/.cache/mist/tiles/$t.png" "$w/home/.cache/mist/tiles/"
done

HOME=$w/home mist-wall-bin --png "$w/wall.ppm" dunes 1920 1080 >/dev/null 2>&1
HOME=$w/home mist-wall-bin --png "$w/sky.ppm" sky 546 310 >/dev/null 2>&1
HOME=$w/home mist-ui-bin --desk-png "$w/desk" >/dev/null 2>&1
HOME=$w/home env -u HYPRLAND_INSTANCE_SIGNATURE mist-ui-bin --dock-png "$w/dock.png" >/dev/null 2>&1
HOME=$w/home env -u HYPRLAND_INSTANCE_SIGNATURE mist-ui-bin --png "$w/bar.png" '{"app":"Firefox","title":"Mist README"}' >/dev/null 2>&1

# stage, weather card and the sky under it where the shell puts them (--desk-png writes layout.json);
# the sky window has rounded corners
read -r gx gy cx cy sx sy <<EOF
$(sed -E 's/.*"card":\[([0-9-]+),([0-9-]+)\].*"sky":\[([0-9-]+),([0-9-]+),[0-9]+,[0-9]+\].*"stage":\[([0-9-]+),([0-9-]+)\].*/\5 \6 \1 \2 \3 \4/' "$w/desk/layout.json")
EOF
magick "$w/sky.ppm" \( -size 546x310 xc:black -fill white -draw "roundrectangle 0,0 545,309 25,25" \) \
    -alpha off -compose CopyOpacity -composite "$w/sky.png"
# Hyprland's blur under the glass layers (stage, dock, bar: the layer rule of examples/hyprland/mist.lua, ignore_alpha
# 0.25; blur size 5, 3 passes ~ a 18 px gaussian, its default brightness / contrast)
dw=$(magick identify -format %w "$w/dock.png"); dh=$(magick identify -format %h "$w/dock.png")
bw=$(magick identify -format %w "$w/bar.png")
m() { magick "$1" -alpha extract -threshold 25% "$2"; }
m "$w/desk/stage.png" "$w/m-stage.png"; m "$w/dock.png" "$w/m-dock.png"; m "$w/bar.png" "$w/m-bar.png"
magick -size 1920x1080 xc:black \
    "$w/m-stage.png" -geometry +$gx+$gy -compose lighten -composite \
    "$w/m-dock.png" -geometry +$(((1920 - dw) / 2))+$((1080 - 8 - dh)) -compose lighten -composite \
    "$w/m-bar.png" -geometry +$(((1920 - bw) / 2))+0 -compose lighten -composite "$w/mask.png"
magick "$w/wall.ppm" \( +clone -blur 0x18 -brightness-contrast -8x-6 \) "$w/mask.png" -composite "$w/wallb.png"
magick "$w/wallb.png" "$w/desk/stage.png" -geometry +$gx+$gy -composite \
    "$w/sky.png" -geometry +$sx+$sy -composite \
    "$w/desk/card.png" -geometry +$cx+$cy -composite \
    "$w/dock.png" -gravity south -geometry +0+8 -composite \
    "$w/bar.png" -gravity north -geometry +0+0 -composite \
    -resize 1600x -strip -quality 92 "$out"
echo "$out"
