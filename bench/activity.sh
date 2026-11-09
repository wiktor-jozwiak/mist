#!/bin/sh
# The same things happening for every shell in bench/shells.sh, on the sway in WAYLAND_DISPLAY / SWAYSOCK:
# a workspace switch every 2 s (1 2 3 1 ...), a window opened every 6 s and closed 3 s later
# (weston-simple-shm, an animating client: titles, focus and window lists change), a notification every 10 s.
#   activity.sh SECS
secs=${1:-60}
i=0
while [ $((i * 2)) -lt "$secs" ]; do
    swaymsg -q workspace number $((i % 3 + 1))
    if [ $((i % 3)) = 0 ]; then
        weston-simple-shm >/dev/null 2>&1 &
        w=$!
        (sleep 3; kill $w 2>/dev/null) &
    fi
    [ $((i % 5)) = 0 ] && notify-send -t 4000 "Benchmark" "Notification $i" 2>/dev/null &
    sleep 2
    i=$((i + 1))
done
wait
