#!/bin/sh
# One run of bench/shells.sh, inside its private D-Bus session and throwaway runtime dir:
# headless sway, the shell, settle, idle window, active window. JSON lines on stdout.
#   shells-run.sh NAME VERSION RUN SETTLE SECS CMD
set -u
here=$(cd "$(dirname "$0")" && pwd)
name=$1 ver=$2 run=$3 settle=$4 secs=$5 cmd=$6
sway -c "$XDG_RUNTIME_DIR/sway.conf" >"$XDG_RUNTIME_DIR/sway.log" 2>&1 &
sp=$!
i=0; until [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && ls "$XDG_RUNTIME_DIR"/sway-ipc.* >/dev/null 2>&1; do
    i=$((i + 1)); [ $i -gt 100 ] && { echo "sway did not start: $(tail -3 "$XDG_RUNTIME_DIR/sway.log")" >&2; kill $sp; exit 1; }; sleep 0.1
done
export WAYLAND_DISPLAY=wayland-1 SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.* | head -1)
if [ -n "$cmd" ]; then
    setsid sh -c "exec $cmd" >"$XDG_RUNTIME_DIR/shell.log" 2>&1 &
    root=$!
else
    sleep 2; root=$(pgrep -P $sp -x swaybar | head -1)
fi
sleep "$settle"
kill -0 "$root" 2>/dev/null || { echo "$name exited during settle: $(tail -5 "$XDG_RUNTIME_DIR/shell.log")" >&2; kill $sp; exit 1; }
tag() { python3 -c 'import json,sys; d=json.loads(sys.stdin.read()); d.update(shell=sys.argv[1], version=sys.argv[2], run=int(sys.argv[3]), scenario=sys.argv[4], host="sway headless"); print(json.dumps(d))' "$name" "$ver" "$run" "$1"; }
# proof that the shell drew something: both outputs, kept next to the results when SHOTS is set
[ -n "${SHOTS:-}" ] && grim "$SHOTS/$(echo "$name" | tr -c 'A-Za-z0-9' _)$run.png" 2>/dev/null
python3 "$here/sample.py" "$secs" "$root" | tag idle
sh "$here/activity.sh" "$secs" &
ap=$!
python3 "$here/sample.py" "$secs" "$root" | tag active
wait $ap
# the shell's whole tree (helpers outside its process group too), then sway
pids=$(python3 "$here/sample.py" --tree "$root")
kill -TERM $pids 2>/dev/null
swaymsg -q exit 2>/dev/null
i=0; while kill -0 $sp 2>/dev/null && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
kill -KILL $sp $(python3 "$here/sample.py" --tree "$root" $sp) 2>/dev/null
exit 0
