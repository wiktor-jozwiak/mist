#!/bin/sh
# Is the machine quiet enough to measure? Samples /proc/stat for a few seconds.
#   quiet.sh [limit-percent] [seconds]     exit 0: quiet; exit 1: busy (prints who)
# A series run beside a build or a dev server gives numbers that are up to twice too high and scattered
# (2026-11-12: a Next.js server at 3.4 cores turned sway's 33.5 % into anything from 16 to 35 %).
# BENCH_BUSY_OK=1 skips the check.
[ "${BENCH_BUSY_OK:-0}" = 1 ] && exit 0
limit=${1:-3}; secs=${2:-3}
snap() { awk '/^cpu /{print $2+$3+$4+$7+$8+$9, $2+$3+$4+$5+$6+$7+$8+$9}' /proc/stat; }
a=$(snap); sleep "$secs"; b=$(snap)
busy=$(echo "$a $b" | awk '{printf "%.1f", ($4-$2) ? 100*($3-$1)/($4-$2) : 0}')
if awk -v b="$busy" -v l="$limit" 'BEGIN{exit !(b>l)}'; then
    echo "bench: machine is busy ($busy % of all cores over ${secs}s, limit $limit %); the busiest processes:" >&2
    top -b -n 2 -d 1 -o %CPU 2>/dev/null | awk '/^ *PID/{n++; next} n==2 && c<4 {printf "   %5s %%  %s\n", $9, $12; c++}' >&2
    exit 1
fi
exit 0
