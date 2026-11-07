#!/bin/sh
# Mist Player's audio view in the nested compositor: the album art embedded in the file is found (one ffmpeg on
# a worker thread), shown, and its colours tint the backdrop (the window holds them); a track without art falls
# back to the placeholder and the default backdrop. Paint time of the audio view stays small.
# Own XDG_RUNTIME_DIR and HOME: never the real instance or its player.json.
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
UI=${MIST_UI_BIN:-$(cd ../../target/release && pwd)/mist-ui}
command -v ffmpeg >/dev/null || { echo "skip: no ffmpeg"; exit 0; }
T=$(mktemp -d); chmod 700 $T; mkdir -p $T/home $T/a
# a red-orange cover, embedded as attached picture; a second track without art
ffmpeg -loglevel error -f lavfi -i color=c=0xE0401A:s=500x500 -frames:v 1 $T/cover.png || { echo "FAIL ffmpeg"; exit 1; }
ffmpeg -loglevel error -f lavfi -i sine=frequency=440:duration=20 -i $T/cover.png -map 0 -map 1 -c:a flac -c:v png -disposition:v attached_pic -metadata artist=Artysta -metadata title=Utwór $T/a/01.flac || { echo "FAIL ffmpeg"; exit 1; }
ffmpeg -loglevel error -f lavfi -i sine=frequency=550:duration=20 -c:a flac $T/a/02.flac
SIZE=1600x1000 ./nest.sh >/dev/null
W=$XDG_RUNTIME_DIR/$(cat sock)
P() { XDG_RUNTIME_DIR=$T HOME=$T/home "$UI" player --test "$@"; }
dump() { rm -f $T/mist/player-state.json; P dump; sleep 0.3; }
paint_total() { python3 -c "import json; d=json.load(open('$T/mist/player-state.json')); print(d['paint_us'] * d['stat'][2], d['stat'][2])"; }
field() { python3 -c "import json; print(json.load(open('$T/mist/player-state.json')).get('$1'))"; }
# share of warm (red > blue + 40) pixels in the player window
warm() {
    ./cc.sh shot-win $PWD/playercover.ppm mist-player >/dev/null
    python3 - "$PWD/playercover.ppm" <<'PY'
import sys
d = open(sys.argv[1], 'rb').read()
hdr = d.split(b'\n', 3); px = hdr[3]
n = s = 0
for i in range(0, len(px) - 2, 3 * 7):
    r, g, b = px[i], px[i+1], px[i+2]; n += 1; s += r > b + 40
print(round(s / max(n, 1), 3))
PY
}
XDG_RUNTIME_DIR=$T HOME=$T/home WAYLAND_DISPLAY=$W setsid "$UI" player $T/a/01.flac $T/a/02.flac >player.log 2>&1 & echo $! > client.pid
timeout 15 sh -c "until ./cc.sh windows | grep -q mist-player; do sleep 0.2; done" || { echo "FAIL no player window"; cat player.log; }
R=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; R=1; }
sleep 1
timeout 10 sh -c "until [ \"\$(XDG_RUNTIME_DIR=$T HOME=$T/home $UI player --test dump; sleep 0.3; python3 -c \"import json; print(json.load(open('$T/mist/player-state.json')).get('cover_cols'))\")\" = True ]; do sleep 0.3; done"
dump
[ "$(field cover)" = "[500, 500]" ] && ok "embedded art found ($(field cover))" || bad "cover: $(field cover)"
sleep 0.5
a=$(warm)
P poke; sleep 1.2; dump; t1=$(paint_total)
P key n; sleep 1.5; dump
[ "$(field cover)" = None ] && [ "$(field cover_cols)" = False ] && ok "next track without art: none kept" || bad "cover after next: $(field cover) $(field cover_cols)"
b=$(warm)
P poke; sleep 1.2; dump; t2=$(paint_total)
python3 -c "import sys; sys.exit(0 if $a > 0.25 and $a > $b + 0.15 else 1)" && ok "art tints the window ($a warm, $b without)" || bad "warm $a vs $b"
set -- $t1; us1=$(( $1 / ($2 > 0 ? $2 : 1) )); set -- $t2 $t1; us2=$(( ($1 - $3) / ($2 - $4 > 0 ? $2 - $4 : 1) ))
[ "$us1" -lt 12000 ] && ok "paint with art ${us1} µs, without ${us2} µs (average per paint)" || bad "paint ${us1} µs (without art ${us2})"
grep -q panicked comp.log player.log && { echo PANIC; R=1; }
kill $(cat client.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1; rm -rf $T
exit $R
