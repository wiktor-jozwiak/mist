#!/bin/sh
# Measures other shells on this machine with bench/measure.sh, one at a time, next to the running Mist:
# each one is started alone, left 3 minutes to settle (caches warm, start-up animations done), measured for
# 30 s, then stopped. Only the shells that are installed run; the TOTAL lines go to stdout, for
# docs/BENCHMARKS.md ("Results: measured on this machine"). Do not move the pointer while it runs.
#   bench/others.sh [settle seconds]
# Installing them (Arch): pacman -S waybar; AUR: ags-hyprpanel-git (HyprPanel), dms-shell-git
# (DankMaterialShell), ironbar; end-4's illogical-impulse comes with its own dotfiles installer.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
settle=${1:-180}
# name | command to start it | process names to measure
while IFS='|' read -r name cmd procs; do
    first=${cmd%% *}
    command -v "$first" >/dev/null 2>&1 || { echo "skip $name: $first not installed"; continue; }
    echo "== $name: $cmd"
    setsid sh -c "exec $cmd" >/dev/null 2>&1 &
    pid=$!
    timeout "$settle" tail -f /dev/null || true
    # shellcheck disable=SC2086
    "$here/measure.sh" 30 $procs | tail -1
    kill -TERM -"$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    timeout 5 sh -c "while kill -0 $pid 2>/dev/null; do sleep 0.2; done" || kill -KILL -"$pid" 2>/dev/null || true
done <<'EOF'
Waybar|waybar|waybar
HyprPanel|hyprpanel|gjs hyprpanel
DankMaterialShell|dms run|quickshell qs dms
Ironbar|ironbar|ironbar
Noctalia v5|noctalia|noctalia
EOF
