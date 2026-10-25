#!/bin/bash
# Screen recording: record.sh [region|screen] [audio] starts wf-recorder, a second call stops it.
# The island shows a red dot while it records (msg island rec 1|0) and the file when it stops.
# Saved to $MIST_REC_DIR, else Videos/Screen recordings (XDG, "Nagrania ekranu" in Polish).
pidf="${XDG_RUNTIME_DIR:-/tmp}/mist-record.pid"
say() { mist-ui-bin msg island "$@" >/dev/null 2>&1; }

if [ -f "$pidf" ] && kill -0 "$(cat "$pidf")" 2>/dev/null; then
    pid=$(cat "$pidf")
    kill -INT "$pid"
    # wf-recorder finishes the file on SIGINT
    while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
    say rec 0
    exit 0
fi
rm -f "$pidf"
command -v wf-recorder >/dev/null || { say show alert "Brak wf-recorder" "zainstaluj pakiet wf-recorder"; exit 1; }

vids=$(xdg-user-dir VIDEOS 2>/dev/null); [ -n "$vids" ] && [ "$vids" != "$HOME" ] || vids="$HOME/Videos"
case "${LANG:-}" in pl*) sub="Nagrania ekranu" word="Nagranie ekranu" at="o" ;; *) sub="Screen recordings" word="Screen recording" at="at" ;; esac
dir="${MIST_REC_DIR:-$vids/$sub}"
mkdir -p "$dir"
ts=$(date +'%Y-%m-%d_%H.%M.%S')
out="$dir/$word ${ts%%_*} $at ${ts#*_}.mp4"

args=()
if [ "${1:-region}" = "region" ]; then
    g=$(slurp -d -b '#00000040' -c '#ff453a' -s '#ffffff10' -w 1 2>/dev/null) || exit 0
    [ -n "$g" ] || exit 0
    args+=(-g "$g")
else
    args+=(-o "$(hyprctl monitors -j | jq -r '.[] | select(.focused) | .name')")
fi
[ "$2" = "audio" ] && args+=(-a)

# setsid: the recorder outlives this script; the second call stops it through the pid file
setsid wf-recorder "${args[@]}" -f "$out" >/dev/null 2>&1 &
echo $! > "$pidf"
sleep 0.3
if kill -0 "$(cat "$pidf")" 2>/dev/null; then
    say rec 1 "$out"
else
    rm -f "$pidf"
    say show alert "Nagrywanie nie ruszyło" "wf-recorder zakończył się błędem"
fi
