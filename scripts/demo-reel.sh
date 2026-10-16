#!/usr/bin/env bash
# Mist showcase reel: drives the native shell over IPC (mist-ui-bin msg) while wf-recorder captures one
# output; writes an MP4 and a short README GIF. Run detached: setsid scripts/demo-reel.sh &
# It records the REAL screen: Do not disturb is on while it runs, but close private windows first.
# Env: OUT (monitor, default DP-3), DEST (mp4), GIF (gif, empty = none), GIF_LEN (s, default 18),
#      SONG + SONG_START (s): optional music under the mp4.
set -u
OUT=${OUT:-DP-3}
VIDEOS=$(xdg-user-dir VIDEOS 2>/dev/null || echo "$HOME/Videos")
DEST=${DEST:-$VIDEOS/mist-showcase.mp4}
GIF=${GIF-$VIDEOS/mist-showcase.gif}
GIF_LEN=${GIF_LEN:-18}
SONG=${SONG:-}
SONG_START=${SONG_START:-0}
TMP=$(mktemp -d)
BIN=$(command -v mist-ui-bin || echo "$HOME/.local/bin/mist-ui-bin")

ipc() { "$BIN" msg "$@" >/dev/null 2>&1; }
panel() { ipc mist toggle "$1"; sleep "$2"; ipc mist close; sleep 0.8; }
island() { ipc island demo "$1"; sleep "$2"; }

hyprctl eval "hl.dispatch(hl.dsp.focus({ monitor = \"$OUT\" }))" >/dev/null
ipc mist close
ipc island dismiss
ipc notif dnd 1
trap 'ipc notif dnd 0' EXIT
sleep 1

wf-recorder -o "$OUT" -r 60 -c libx264 -p preset=veryfast -p crf=16 -f "$TMP/raw.mp4" >"$TMP/rec.log" 2>&1 &
REC=$!
sleep 3                                   # live wallpaper intro

island track 3.5
island volume 2.5
island notif 3.5
island timer 3
island phone 3.5
island bt 2.5
island rain 3
ipc island dismiss; sleep 1

panel spotlight 4
panel start 4
ipc mist control "";          sleep 3.5
ipc mist control sound;       sleep 3
ipc mist control bluetooth;   sleep 3
ipc mist close;               sleep 1
panel notifications 4
panel calendar 3.5
panel clipboard 3.5
panel wallpaper 4.5
panel session 3

ipc lock preview 7; sleep 8
sleep 2                                   # outro on desktop

kill -INT $REC; wait $REC

LEN=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$TMP/raw.mp4")
FO=$(awk "BEGIN{print $LEN-3}")
mkdir -p "$(dirname "$DEST")"
VF="fps=60,format=yuv420p,fade=t=in:st=0:d=1.2,fade=t=out:st=$FO:d=3"
if [ -n "$SONG" ]; then
    ffmpeg -y -loglevel error -i "$TMP/raw.mp4" -ss "$SONG_START" -i "$SONG" \
      -filter_complex "[0:v]$VF[v];[1:a]afade=t=in:st=0:d=2.5,afade=t=out:st=$FO:d=3[a]" \
      -map "[v]" -map "[a]" -t "$LEN" -c:v libx264 -preset slow -crf 18 -c:a aac -b:a 192k -movflags +faststart "$DEST"
else
    ffmpeg -y -loglevel error -i "$TMP/raw.mp4" -vf "$VF" -an -c:v libx264 -preset slow -crf 18 -movflags +faststart "$DEST"
fi
# the README GIF: the island part, 960 px wide, 20 fps, one palette for the whole clip
if [ -n "$GIF" ]; then
    ffmpeg -y -loglevel error -ss 2 -t "$GIF_LEN" -i "$TMP/raw.mp4" \
      -vf "fps=20,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" "$GIF"
fi
notify-send "Mist showcase" "$DEST ${GIF:+· $GIF}" 2>/dev/null
rm -rf "$TMP"
echo "$DEST ${GIF}"
