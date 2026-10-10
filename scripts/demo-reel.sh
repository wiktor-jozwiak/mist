#!/usr/bin/env bash
# Mist showcase reel: drives panels over IPC while wf-recorder captures one output,
# then ffmpeg mixes in a song. Run detached: setsid scripts/demo-reel.sh &
# Env: OUT (monitor, default DP-3), SONG, SONG_START (s), DEST (final mp4).
set -u
OUT=${OUT:-DP-3}
SONG=${SONG:-$(ls ~/Pobrane/*Purple_Rain*.mp3 | head -1)}
SONG_START=${SONG_START:-357}
DEST=${DEST:-$HOME/Wideo/mist-showcase.mp4}
TMP=$(mktemp -d)
QS="$HOME/.local/bin/mist-qs -p $HOME/.config/quickshell/mist ipc call"

ipc() { $QS "$@" >/dev/null 2>&1; }
panel() { ipc mist toggle "$1"; sleep "$2"; ipc mist close; sleep 0.8; }
island() { ipc island demo "$1"; sleep "$2"; }

hyprctl eval "hl.dispatch(hl.dsp.focus({ monitor = \"$OUT\" }))" >/dev/null
ipc mist close
ipc island dismiss
sleep 1

wf-recorder -o "$OUT" -r 60 -c libx264 -p preset=veryfast -p crf=16 -f "$TMP/raw.mp4" >"$TMP/rec.log" 2>&1 &
REC=$!
sleep 3                                   # live wallpaper intro

island track 3.5
island volume 2.5
island notif 3.5
island timer 3
island phone-call 3.5
island bt 2.5
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
ffmpeg -y -loglevel error -i "$TMP/raw.mp4" -ss "$SONG_START" -i "$SONG" \
  -filter_complex "[0:v]fps=60,format=yuv420p,fade=t=in:st=0:d=1.2,fade=t=out:st=$FO:d=3[v];[1:a]afade=t=in:st=0:d=2.5,afade=t=out:st=$FO:d=3[a]" \
  -map "[v]" -map "[a]" -t "$LEN" -c:v libx264 -preset slow -crf 18 -c:a aac -b:a 192k -movflags +faststart "$DEST"
echo "$DEST" > "$TMP/done"
notify-send "Mist showcase" "Gotowe: $DEST" 2>/dev/null
echo "$TMP"
