#!/bin/sh
# Mist Player's extras in the nested compositor: chapters read from the file (and their menu), the folder queue
# kept in player.json, a seek-bar thumbnail made by ffmpeg, and Picture-in-Picture: a second, pinned window takes
# the video plane (its pixels hold the colours, the main window then has none but the corner PiP covers), and
# gives it back.
# The player runs with its own XDG_RUNTIME_DIR and HOME: never forwards to the real instance, never touches player.json.
D=$(dirname "$0"); cd $D; ./unnest.sh >/dev/null 2>&1
UI=$(cd ../../target/release && pwd)/mist-ui
command -v ffmpeg >/dev/null || { echo "skip: no ffmpeg"; exit 0; }
T=$(mktemp -d); chmod 700 $T; mkdir -p $T/home $T/v
printf ';FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1\nSTART=0\nEND=10\ntitle=Intro\n[CHAPTER]\nTIMEBASE=1/1\nSTART=10\nEND=20\ntitle=Middle\n[CHAPTER]\nTIMEBASE=1/1\nSTART=20\nEND=30\ntitle=End\n' >$T/meta.txt
ffmpeg -loglevel error -f lavfi -i testsrc2=size=640x360:rate=30 -i $T/meta.txt -map 0 -map_chapters 1 -t 30 -g 30 -pix_fmt yuv420p $T/v/a.mkv || { echo "FAIL ffmpeg"; exit 1; }
cp $T/v/a.mkv $T/v/b.mkv
SIZE=1920x1080 ./nest.sh >/dev/null
W=$XDG_RUNTIME_DIR/$(cat sock)
P() { XDG_RUNTIME_DIR=$T HOME=$T/home "$UI" player --test "$@"; }
dump() { rm -f $T/mist/player-state.json; P dump; sleep 0.3; cat $T/mist/player-state.json 2>/dev/null; }
field() { python3 -c "import json,sys; print(json.load(open('$T/mist/player-state.json')).get('$1'))"; }
XDG_RUNTIME_DIR=$T HOME=$T/home WAYLAND_DISPLAY=$W setsid "$UI" player $T/v/a.mkv >player.log 2>&1 & echo $! > client.pid
timeout 15 sh -c "until ./cc.sh windows | grep -q mist-player; do sleep 0.2; done" || { echo "FAIL no player window"; cat player.log; }
sleep 3
R=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; R=1; }
# saturated share of a window's rectangle in a full-output shot (its geometry from `windows`, pinned or not)
sat() {
    ./cc.sh shot $PWD/playerpip.ppm >/dev/null
    python3 - "$PWD/playerpip.ppm" "$1" "$(./cc.sh windows)" <<'PY'
import sys
d = open(sys.argv[1], 'rb').read(); want = sys.argv[2]
hdr = d.split(b'\n', 3); w, h = map(int, hdr[1].split()); px = hdr[3]
rows = [l.split() for l in sys.argv[3].splitlines() if l.startswith('mist-player')]
r = [l for l in rows if ('pinned' in l) == (want == 'pip')]
if not r: print(0.0); sys.exit()
x, y = map(int, r[0][2].split(',')); ww, hh = map(int, r[0][3].split('x'))
n = s = 0
for yy in range(max(y, 0), min(y + hh, h), 3):
    for xx in range(max(x, 0), min(x + ww, w), 3):
        i = (yy * w + xx) * 3; p = px[i:i+3]; n += 1
        s += max(p) - min(p) > 80
print(round(s / max(n, 1), 3))
PY
}

dump >/dev/null
[ "$(field chapters)" = 3 ] && ok "3 chapters read" || bad "chapters: $(field chapters)"
P chapters; sleep 0.3; dump >/dev/null
[ "$(field panel)" = Chapters ] && ok "chapter menu open" || bad "panel: $(field panel)"
P key Escape
q=$(python3 -c "import json; print(len(json.load(open('$T/home/.config/mist/player.json')).get('queue', [])))" 2>/dev/null)
[ "$q" = 2 ] && ok "queue of 2 kept in player.json" || bad "saved queue: $q"

# a thumbnail at the middle of the bar
P poke; sleep 0.3; P hover-seek 0.5
timeout 6 sh -c "until [ \"\$(XDG_RUNTIME_DIR=$T HOME=$T/home $UI player --test dump; sleep 0.3; python3 -c \"import json; print(json.load(open('$T/mist/player-state.json'))['thumbs'])\")\" -ge 1 ] 2>/dev/null; do sleep 0.3; done"
dump >/dev/null
[ "$(field thumbs)" -ge 1 ] 2>/dev/null && ok "seek thumbnail made ($(field thumbs_bytes) B kept)" || bad "thumbnails: $(field thumbs) off=$(field thumbs_off)"

main0=$(sat main)
P pip
timeout 6 sh -c "until ./cc.sh windows | grep mist-player | grep -q pinned; do sleep 0.2; done" || bad "no pinned PiP window"
sleep 2
dump >/dev/null
[ "$(field video_win)" = pip ] && ok "the plane is in the PiP window" || bad "video_win: $(field video_win)"
pip=$(sat pip); main1=$(sat main)
python3 -c "import sys; sys.exit(0 if $pip > 0.05 and $main1 < 0.05 else 1)" && ok "PiP shows the video ($pip saturated), main window none (the PiP corner over it: $main1, was $main0)" || bad "PiP pixels $pip, main $main1 (was $main0)"
P pip
timeout 6 sh -c "while ./cc.sh windows | grep mist-player | grep -q pinned; do sleep 0.2; done" || bad "PiP window stays"
sleep 2
dump >/dev/null
main2=$(sat main)
[ "$(field video_win)" = main ] && [ "$(field pip)" = False ] && ok "back in the main window" || bad "video_win: $(field video_win) pip: $(field pip)"
python3 -c "import sys; sys.exit(0 if $main2 > 0.05 else 1)" && ok "main window shows the video again ($main2)" || bad "main pixels after PiP: $main2"
[ "$(./cc.sh windows | grep -c mist-player)" = 1 ] && ok "one window left" || bad "windows: $(./cc.sh windows | grep -c mist-player)"
grep -q panicked comp.log player.log && { echo PANIC; R=1; }
kill $(cat client.pid) 2>/dev/null; ./unnest.sh >/dev/null 2>&1; rm -rf $T
exit $R
