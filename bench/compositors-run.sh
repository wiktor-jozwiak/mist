#!/bin/sh
# One run of bench/compositors.sh inside its private D-Bus session: start COMP headless, then the idle,
# shm and egl windows. JSON lines on stdout.
#   compositors-run.sh COMP VERSION RUN SECS
set -u
here=$(cd "$(dirname "$0")" && pwd)
comp=$1 ver=$2 run=$3 secs=$4
RT=$XDG_RUNTIME_DIR
wlr="WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=$NODE WLR_LIBINPUT_NO_DEVICES=1"
case $comp in
    sway)
        printf 'output HEADLESS-1 mode 1920x1080@60Hz\nxwayland disable\n' >"$RT/sway.conf"
        env $wlr sway -c "$RT/sway.conf" >"$RT/comp.log" 2>&1 & ;;
    labwc)
        # default configuration; XWayland is started lazily (only for an X client), so it never runs here
        env $wlr labwc >"$RT/comp.log" 2>&1 & ;;
    weston)
        weston --backend=headless --renderer=gl --shell=kiosk --width=1920 --height=1080 --refresh-rate=60000 \
            --socket=wayland-1 >"$RT/comp.log" 2>&1 & ;;
    mist-comp)
        MIST_COMP_OUTPUTS="HEADLESS-1:1920x1080" MIST_COMP_RENDER_NODE=$NODE RUST_LOG=warn \
            "$MIST_COMP" --headless -c true >"$RT/comp.log" 2>&1 & ;;
esac
cp=$!
i=0; until ls "$RT"/wayland-* 2>/dev/null | grep -qv lock; do
    i=$((i + 1)); [ $i -gt 150 ] && { echo "$comp did not start: $(tail -5 "$RT/comp.log")" >&2; kill $cp; exit 1; }; sleep 0.1
done
export WAYLAND_DISPLAY=$(basename "$(ls "$RT"/wayland-* | grep -v lock | head -1)")
sleep 5
tag() { python3 -c 'import json,sys; d=json.loads(sys.stdin.readline()); d.update(comp=sys.argv[1], version=sys.argv[2], run=int(sys.argv[3]), scenario=sys.argv[4])
if len(sys.argv) > 5 and sys.argv[5]: d["fps"]=float(sys.argv[5])
print(json.dumps(d))' "$comp" "$ver" "$run" "$@"; }

python3 "$here/sample.py" "$secs" $cp | tag idle

weston-simple-shm >/dev/null 2>&1 &
c=$!; sleep 3
python3 "$here/sample.py" "$secs" $cp | tag shm
kill $c; wait $c 2>/dev/null; sleep 2

# simple-egl prints "N frames in 5 seconds: F fps" every 5 s; the windows inside ours are averaged
stdbuf -oL weston-simple-egl -f -o >"$RT/egl.log" 2>&1 &
c=$!; sleep 3
n0=$(grep -c 'fps' "$RT/egl.log")
s=$(python3 "$here/sample.py" "$secs" $cp)
fps=$(grep 'fps' "$RT/egl.log" | tail -n +$((n0 + 1)) | awk '{s += $(NF - 1); n++} END {if (n) printf "%.1f", s / n}')
echo "$s" | tag egl "$fps"
kill $c; wait $c 2>/dev/null

kill -TERM $(python3 "$here/sample.py" --tree $cp) 2>/dev/null
i=0; while kill -0 $cp 2>/dev/null && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
kill -KILL $cp 2>/dev/null
exit 0
