#!/bin/sh
# Rebuilds the README's gallery (docs/img/panels/*.webp) and the theme strip (docs/img/themes.webp). Every shot
# is rendered offscreen on a 1920 x 1080 monitor over the dunes wallpaper and cut to the same 16:10 frame
# (1280 x 800) around what it shows, so the README's grid lines up. Demo data, an English session and a demo
# home (a made-up user, demo files): no notifications, clipboard, account or host names.
# Needs mist-ui-bin / mist-wall-bin (or $MIST_UI / $MIST_WALL), ImageMagick.
#   docs/img/panels.sh [outdir]          the gallery; the theme strip goes to outdir/../themes.webp
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$repo/docs/img/panels}
ui=${MIST_UI:-mist-ui-bin}
wall=${MIST_WALL:-mist-wall-bin}
w=$(mktemp -d)
trap 'rm -rf "$w"' EXIT
h=$w/home
mkdir -p "$out" "$h/.cache/mist/thumbs" "$h/.config/mist" "$h/.config/quickshell" "$h/.local/share/fonts" \
    "$h/.local/share/applications" "$h/Desktop" "$h/Documents" "$h/Downloads" "$h/Music" "$h/Pictures" "$h/Videos"
cp "$repo"/docs/demo/weather.json "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$h/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$h/.config/mist/"
echo '{"welcomed": true}' >"$h/.config/mist/settings.json" # no first-start welcome in the Settings shot
ln -s "$repo" "$h/.config/quickshell/mist"
[ -d "$HOME/.local/share/fonts/SF-Pro" ] && ln -s "$HOME/.local/share/fonts/SF-Pro" "$h/.local/share/fonts/SF-Pro"
# Mist's own apps and their icons (installed by native/install.sh), demo favourites in Start
for e in mist-files mist-player mist-settings; do
    [ -f "$HOME/.local/share/applications/$e.desktop" ] && ln -sf "$HOME/.local/share/applications/$e.desktop" "$h/.local/share/applications/"
done
[ -d "$HOME/.local/share/icons" ] && ln -sfn "$HOME/.local/share/icons" "$h/.local/share/icons"
echo '{"pinned": ["firefox", "mist-files", "kitty", "spotify", "libreoffice-writer", "libreoffice-calc", "org.gnome.Calculator", "mist-player", "mist-settings", "com.github.wwmm.easyeffects", "mpv", "Alacritty"]}' >"$h/.config/mist/start.json"
for f in "Q3 report.pdf:180000" "Budget 2026.ods:42000" "Notes.md:2100" "Slides.odp:950000" "Invoice 0412.pdf:61000" \
    "Reading list.txt:900" "Logo.svg:12000" "Contract draft.odt:31000"; do
    head -c "${f##*:}" /dev/zero >"$h/Documents/${f%:*}"
done
export LANG=en_US.UTF-8 MIST_LANG=en USER=alex HOME="$h"

"$wall" --png "$w/bg.ppm" dunes 1920 1080 >/dev/null 2>&1
"$wall" --png "$w/thumb.ppm" dunes 640 360 >/dev/null 2>&1
magick "$w/thumb.ppm" "$h/.cache/mist/thumbs/live_dunes.png"
echo '{"live": "dunes"}' >"$h/.config/mist/wallpaper.json"
# Hyprland's blur under the glass (layer rule ignore_alpha 0.25; blur size 5, 3 passes ~ a 18 px gaussian, its
# default brightness 0.82 / contrast 0.89)
magick "$w/bg.ppm" -blur 0x18 -brightness-contrast -8x-6 "$w/bgblur.png"

# <full 1920x1080 png> <WxH+X+Y of what it shows> <out.webp>: a 16:10 frame around it ($PAD times what it
# needs, a quarter more by default, inside the monitor), 1280 x 800, rounded corners
frame() {
    set -- "$1" $(echo "$2" | sed -E 's/([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)/\1 \2 \3 \4/') "$3"
    geo=$(awk -v w="$2" -v h="$3" -v x="$4" -v y="$5" -v p="${PAD:-1.25}" 'BEGIN {
        fw = w * p; if (h * p * 1.6 > fw) fw = h * p * 1.6
        if (fw < 760) fw = 760; if (fw > 1728) fw = 1728; fh = fw / 1.6
        fx = x + w / 2 - fw / 2; fy = y + h / 2 - fh / 2
        if (fx < 0) fx = 0; if (fy < 0) fy = 0; if (fx + fw > 1920) fx = 1920 - fw; if (fy + fh > 1080) fy = 1080 - fh
        printf "%dx%d+%d+%d", fw, fh, fx, fy }')
    magick "$1" -crop "$geo" +repage -resize 1280x800! \
        \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "roundrectangle 0,0 1279,799 22,22" \) \
        -alpha off -compose CopyOpacity -composite -strip -quality 90 "$6"
    echo "$6"
}

# <panel> [query]: a popup of the shell (alpha over $TH % counts as drawn, 3 by default)
shot() {
    f=$w/${1%%:*} # the file without the page (ImageMagick reads "settings:desk.png" as a format prefix)
    "$ui" --pane-png "$1" 1920 1080 "$f.png" ${2:+"$2"} >/dev/null 2>&1
    g=$(magick "$f.png" -alpha extract -threshold "${TH:-3}%" -format '%@' info:)
    magick "$w/bg.ppm" "$w/bgblur.png" \( "$f.png" -alpha extract -threshold 25% \) -composite "$f.png" -composite "$f.full.png"
    frame "$f.full.png" "$g" "$out/${1%%:*}.webp"
}

# <name> <window png>: an app window in the middle of the monitor, with a soft shadow
win() {
    ww=$(magick identify -format %w "$2"); wh=$(magick identify -format %h "$2")
    magick "$w/bg.ppm" \( "$2" \( +clone -background black -shadow 45x18+0+10 \) +swap -background none -layers merge +repage \) \
        -gravity center -compose over -composite "$w/$1.full.png"
    PAD=1.1 frame "$w/$1.full.png" "${ww}x${wh}+$(((1920 - ww) / 2))+$(((1080 - wh) / 2))" "$out/$1.webp"
}

shot spotlight "128*42"
shot control
shot calendar
shot emoji
TH=50 shot start # over its own dimmed screen

"$ui" settings --png "$w/settings.png" personalization 1100x690 >/dev/null 2>&1
win settings "$w/settings.png"
"$ui" files --png "$w/files.png" "$h/Documents" 1100x690 >/dev/null 2>&1
win files "$w/files.png"
# the player's controls over a frame of the aurora wallpaper (the video plane is under the window)
"$ui" player --png "$w/player.png" video 1100x690 >/dev/null 2>&1
"$wall" --png "$w/frame.ppm" aurora 1100 690 >/dev/null 2>&1
magick "$w/frame.ppm" "$w/player.png" -composite \
    \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "roundrectangle 0,0 1099,689 12,12" \) \
    -alpha off -compose CopyOpacity -composite "$w/player-f.png"
win player "$w/player-f.png"

# the theme strip: the control centre in every theme, side by side on one strip of wallpaper
i=0
for t in mist catppuccin nord gruvbox tokyonight rosepine dracula; do
    MIST_THEME=$t "$ui" --pane-png control 1920 1080 "$w/t.png" >/dev/null 2>&1
    g=$(magick "$w/t.png" -alpha extract -threshold 3% -format '%@' info:)
    magick "$w/bgblur.png" "$w/t.png" -composite -crop "$g" +repage -resize 400x "$w/th-$i.png"
    i=$((i + 1))
done
th=$(magick identify -format %h "$w/th-0.png")
magick "$w/bg.ppm" -resize 3000x -gravity center -crop "3000x$((th + 120))+0+0" +repage "$w/strip.png"
font=$(fc-match -f '%{file}' 'SF Pro Text:style=Medium')
x=40
k=0
for t in Mist Catppuccin Nord Gruvbox "Tokyo Night" "Rosé Pine" Dracula; do
    magick "$w/strip.png" "$w/th-$k.png" -geometry +$x+40 -composite \
        -font "$font" -pointsize 26 -fill '#ffffffd8' -gravity northwest -annotate +$((x + 4))+$((th + 58)) "$t" "$w/strip.png"
    x=$((x + 420))
    k=$((k + 1))
done
magick "$w/strip.png" -crop "$((x + 20))x$((th + 120))+0+0" +repage -resize 1600x \
    \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "roundrectangle 0,0 %[fx:w-1],%[fx:h-1] 22,22" \) \
    -alpha off -compose CopyOpacity -composite -strip -quality 90 "$out/../themes.webp"
echo "$out/../themes.webp"
