#!/bin/sh
# mist-comp's cost: PSS, threads, CPU, wakeups (context switches) and frames, idle and while the pointer moves.
#   bench/comp.sh                 headless mist-comp (software rendering) + mist-ui, throwaway HOME
#   bench/comp.sh live            the running session (mist-comp started by start-mist), idle only
# env: BIN (native/target/release), SECS (10), OUTPUTS (two 1920x1080)
set -u
export LC_ALL=C
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${BIN:-$ROOT/native/target/release}
SECS=${SECS:-10}

pss() { awk '/^Pss:/ {s += $2} END {printf "%.1f", s / 1024}' /proc/"$1"/smaps_rollup 2>/dev/null; }
thr() { ls /proc/"$1"/task 2>/dev/null | wc -l; }
run_ns() { t=0; for s in /proc/"$1"/task/*/schedstat; do [ -r "$s" ] && t=$((t + $(cut -d' ' -f1 "$s"))); done; echo $t; }
ctx() { t=0; for s in /proc/"$1"/task/*/status; do v=$(awk '/^(non)?voluntary_ctxt_switches/ {n += $2} END {print n + 0}' "$s" 2>/dev/null); t=$((t + ${v:-0})); done; echo $t; }
# <label> <pid> <seconds> [frames-cmd]: one line
measure() {
    l=$1 p=$2 s=$3
    c0=$(run_ns "$p"); x0=$(ctx "$p"); f0=$(${4:-true} | awk '{print $2 + 0}')
    t0=$(date +%s%N)
    ${MOVE:-sleep} "$s"
    t1=$(date +%s%N)
    c1=$(run_ns "$p"); x1=$(ctx "$p"); f1=$(${4:-true} | awk '{print $2 + 0}')
    awk -v l="$l" -v pss="$(pss "$p")" -v thr="$(thr "$p")" -v c=$((c1 - c0)) -v x=$((x1 - x0)) -v f=$((${f1:-0} - ${f0:-0})) -v t=$((t1 - t0)) \
        'BEGIN { s = t / 1e9; printf "%-22s pss=%6.1fMB threads=%2d cpu=%5.2f%% wakeups=%6.1f/s frames=%5.1f/s\n", l, pss, thr, c / t * 100, x / s, f / s }'
}

if [ "${1:-}" = live ]; then
    for n in mist-comp mist-ui mist-wall; do
        p=$(pgrep -x "$n" | head -1)
        [ -n "$p" ] && measure "$n (idle)" "$p" "$SECS"
    done
    exit 0
fi

RT=$(mktemp -d /tmp/mcb.XXXX); chmod 700 "$RT"
H=$RT/home; mkdir -p "$H/.local/share/fonts/SF-Pro" "$H/.cache/mist" "$H/.config/mist"
for f in /usr/share/fonts/inter/InterVariable.ttf /usr/share/fonts/TTF/InterVariable.ttf /usr/share/fonts/opentype/inter/Inter-Regular.otf \
         /usr/share/fonts/noto/NotoSans-Regular.ttf /usr/share/fonts/truetype/noto/NotoSans-Regular.ttf "$HOME/.local/share/fonts/SF-Pro/SF-Pro.ttf"; do
    [ -f "$f" ] && { ln -sf "$f" "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf"; break; }
done
cp "$ROOT"/docs/demo/*.json "$H/.cache/mist/" 2>/dev/null
export XDG_RUNTIME_DIR=$RT HOME=$H MIST_DIR=$ROOT MIST_UI_NO_SERVICES=1 MIST_UI_SOCK=$RT/ui.sock MIST_PRIMARY=DP-3
export MIST_COMP_SOFTWARE=${MIST_COMP_SOFTWARE:-1}
unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
MIST_COMP_OUTPUTS=${OUTPUTS:-DP-3:1920x1080,HDMI-A-1:1920x1080} RUST_LOG=warn "$BIN/mist-comp" --headless >"$RT/comp.log" 2>&1 &
CP=$!
trap 'kill $CP ${UP:-} 2>/dev/null; sleep 0.3; rm -rf "$RT"' EXIT INT TERM
i=0; until grep -q '^WAYLAND_DISPLAY=' "$RT/comp.log"; do i=$((i + 1)); [ $i -gt 100 ] && { cat "$RT/comp.log"; exit 1; }; sleep 0.1; done
S=$(grep -m1 '^WAYLAND_DISPLAY=' "$RT/comp.log" | cut -d= -f2)
C() { echo "$*" | socat -t3 - "UNIX:$RT/mist-comp/$S.sock" 2>/dev/null; }
stats() { C stats; }
measure "mist-comp alone" "$CP" "$SECS" stats
WAYLAND_DISPLAY=$S "$BIN/mist-ui" >"$RT/ui.log" 2>&1 &
UP=$!
sleep 5 # surfaces up, start-up animations over
measure "mist-comp + shell" "$CP" "$SECS" stats
measure "mist-ui (shell)" "$UP" "$SECS"
# the pointer sweeping the desktop of the first output at ~120 Hz
sweep() { n=$(($1 * 120)); k=0; while [ $k -lt $n ]; do C motion $((200 + k % 1400)) $((300 + k % 400)) >/dev/null; k=$((k + 1)); sleep 0.008; done; }
MOVE=sweep measure "mist-comp, pointer" "$CP" "$SECS" stats
