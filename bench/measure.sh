#!/bin/sh
# Idle cost of a running desktop shell: PSS (proportional set size, shared pages split between users),
# RSS, threads and CPU over a window, summed over every process whose name matches.
#   bench/measure.sh [seconds] <process-name>...
#   bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd
#   bench/measure.sh 30 waybar          bench/measure.sh 30 quickshell qs
# CPU comes from /proc/<pid>/task/*/schedstat (ns run time; CLK_TCK ticks are too coarse for idle shells).
# GPU: a process with an EGL/GL context on NVIDIA maps the driver (~100+ MB); reported as "gl=yes".
set -e
export LC_ALL=C
secs=30
case "$1" in ''|*[!0-9]*) ;; *) secs=$1; shift ;; esac
[ $# -gt 0 ] || { echo "usage: $0 [seconds] <process-name>..." >&2; exit 2; }

mb() { awk -v k="$1" 'BEGIN {print k / 1024}'; }
pids() { for n in "$@"; do pgrep -x "$n"; done | sort -u; }
cpu_ns() { # total run time of all threads of the given pids
    t=0
    for p in $1; do
        for s in /proc/$p/task/*/schedstat; do
            [ -r "$s" ] && t=$((t + $(cut -d' ' -f1 "$s")))
        done
    done
    echo $t
}

P=$(pids "$@")
[ -n "$P" ] || { echo "no process named: $*" >&2; exit 1; }
c0=$(cpu_ns "$P"); t0=$(date +%s%N)
sleep "$secs"
c1=$(cpu_ns "$P"); t1=$(date +%s%N)

pss=0; rss=0; thr=0; gl=no
printf '%-8s %-18s %9s %9s %5s %4s\n' PID NAME PSS_MB RSS_MB THR GL
for p in $P; do
    [ -r /proc/$p/smaps_rollup ] || continue
    ps_k=$(awk '/^Pss:/ {print $2}' /proc/$p/smaps_rollup)
    rs_k=$(awk '/^Rss:/ {print $2}' /proc/$p/smaps_rollup)
    th=$(ls /proc/$p/task | wc -l)
    g=no; grep -qE 'libnvidia-eglcore|libnvidia-glcore|libGLX_|dri/.*_dri\.so|libgallium' /proc/$p/maps 2>/dev/null && g=yes && gl=yes
    printf '%-8s %-18s %9.1f %9.1f %5s %4s\n' "$p" "$(cat /proc/$p/comm)" "$(mb $ps_k)" "$(mb $rs_k)" "$th" "$g"
    pss=$((pss + ps_k)); rss=$((rss + rs_k)); thr=$((thr + th))
done
cpu=$(awk -v a=$c0 -v b=$c1 -v s=$t0 -v e=$t1 'BEGIN {print 100 * (b - a) / (e - s)}')
printf 'TOTAL  pss=%.1fMB rss=%.1fMB threads=%s cpu=%.2f%% (one core, %ss) gl=%s\n' \
    "$(mb $pss)" "$(mb $rss)" "$thr" "$cpu" "$secs" "$gl"
