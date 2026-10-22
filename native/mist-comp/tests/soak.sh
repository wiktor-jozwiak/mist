#!/bin/sh
# Soak test: mist-comp headless (software rendering) + mist-ui, random window traffic for a while; fails on a
# panic, a compositor that stops answering, or PSS that keeps growing (a leak).
#   native/mist-comp/tests/soak.sh [seconds]      (default 300; after cargo build --release)
set -u
D=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$D/../../.." && pwd)
BIN=${BIN:-$ROOT/native/target/release}
SECS=${1:-300}
OUT=${OUT:-$(mktemp -d)}
RT=$(mktemp -d /tmp/msk.XXXX)
H=$OUT/home
mkdir -p "$H/.config/mist" "$H/.cache/mist" "$H/.local/share/fonts/SF-Pro"
chmod 700 "$RT"
for f in /usr/share/fonts/inter/InterVariable.ttf /usr/share/fonts/TTF/InterVariable.ttf /usr/share/fonts/opentype/inter/Inter-Regular.otf \
         /usr/share/fonts/noto/NotoSans-Regular.ttf /usr/share/fonts/truetype/noto/NotoSans-Regular.ttf; do
    [ -f "$f" ] && { ln -sf "$f" "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf"; break; }
done
export XDG_RUNTIME_DIR=$RT HOME=$H MIST_DIR=$ROOT MIST_LANG=en LANG=C.UTF-8 MIST_PRIMARY=DP-3
export MIST_UI_NO_SERVICES=1 MIST_UI_SOCK=$RT/ui.sock MIST_COMP_SOFTWARE=1
unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" RUST_LOG=warn "$BIN/mist-comp" --headless >"$OUT/comp.log" 2>&1 &
CP=$!
trap 'kill $CP $(jobs -p) 2>/dev/null; sleep 0.3; rm -rf "$RT"' EXIT INT TERM
i=0; until grep -q '^WAYLAND_DISPLAY=' "$OUT/comp.log"; do i=$((i + 1)); [ $i -gt 100 ] && { echo "FAIL no compositor"; exit 1; }; sleep 0.1; done
S=$(grep -m1 '^WAYLAND_DISPLAY=' "$OUT/comp.log" | cut -d= -f2)
C() { echo "$*" | socat -t3 - "UNIX:$RT/mist-comp/$S.sock" 2>/dev/null; }
pss() { awk '/^Pss:/ {s += $2} END {print int(s / 1024)}' /proc/"$1"/smaps_rollup 2>/dev/null; }
WAYLAND_DISPLAY=$S "$BIN/mist-ui" >"$OUT/ui.log" 2>&1 &
sleep 5
# random number 0..n-1 (POSIX sh has no $RANDOM)
rnd() { od -An -N2 -tu2 /dev/urandom | awk -v n="$1" '{print $1 % n}'; }
start=$(date +%s); n=0; first=""; samples=""
while [ $(($(date +%s) - start)) -lt "$SECS" ]; do
    case $(rnd 10) in
        0 | 1) WAYLAND_DISPLAY=$S "$BIN/mist-ui" files "$H" >/dev/null 2>&1 & ;;
        2) C action close >/dev/null ;;
        3) C action workspace $(($(rnd 4) + 1)) >/dev/null ;;
        4) C action movetoworkspace $(($(rnd 4) + 1)) >/dev/null ;;
        5) C motion $(rnd 2560) $(rnd 800) >/dev/null ;;
        6) # Super+drag somewhere
           C motion $(($(rnd 1000) + 100)) $(($(rnd 500) + 100)) >/dev/null; C key 125 press >/dev/null; C button left press >/dev/null
           C motion $(rnd 2560) $(rnd 800) >/dev/null; C button left release >/dev/null; C key 125 release >/dev/null ;;
        7) C action maximize >/dev/null ;;
        8) C action fullscreen >/dev/null ;;
        9) WAYLAND_DISPLAY=$S timeout 6 "$BIN/mist-ui" lock preview 2 >/dev/null 2>&1 & ;;
    esac
    n=$((n + 1))
    # never more than 8 windows: close the extras
    k=0
    while [ "$(C windows | grep -c '^mist-files')" -gt 8 ] && [ $k -lt 10 ]; do C action close >/dev/null; sleep 0.1; k=$((k + 1)); done
    if [ $((n % 50)) = 0 ]; then
        C stats | grep -q frames || { echo "FAIL compositor stopped answering after $n actions"; exit 1; }
        p=$(pss $CP); samples="$samples $p"; [ -z "$first" ] && first=$p
    fi
    sleep 0.05
done
# let everything settle and close, then compare memory with the first sample
for _ in 1 2 3 4 5 6 7 8 9 10; do C action close >/dev/null; done
sleep 3
last=$(pss $CP)
echo "actions $n, compositor PSS MB:$samples -> $last (first ${first:-?})"
fails=0
grep -q panicked "$OUT/comp.log" && { echo "FAIL compositor panicked"; grep -A5 panicked "$OUT/comp.log" | head -20; fails=1; }
grep -q panicked "$OUT/ui.log" && { echo "FAIL mist-ui panicked"; fails=1; }
C stats | grep -q frames || { echo "FAIL compositor gone"; fails=1; }
# growth beyond 25 % + 20 MB over the run (software rendering keeps some texture memory around)
[ -n "$first" ] && [ "$last" -gt $((first + first / 4 + 20)) ] && { echo "FAIL PSS grew $first -> $last MB"; fails=1; }
[ $fails = 0 ] && echo "soak ok" || exit 1
