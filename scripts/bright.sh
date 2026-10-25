#!/bin/sh
# Brightness: bright.sh up|down [step %] (bound to XF86MonBrightnessUp/Down; key repeat while one change runs
# is dropped). A laptop panel (/sys/class/backlight, set through logind: no root needed), else every monitor
# over DDC/CI. The island shows the new level after each step.
exec 9>"${XDG_RUNTIME_DIR:-/tmp}/mist-bright.lock"
flock -n 9 || exit 0
step="${2:-10}"
# the backlight the kernel says to prefer: firmware, then platform, then raw (acpi_video beside amdgpu_bl1)
bl=
for t in firmware platform raw; do
    for d in "${MIST_BACKLIGHT:-/sys/class/backlight}"/*; do
        [ -z "$bl" ] && [ "$(cat "$d/type" 2>/dev/null)" = "$t" ] && bl=$d
    done
done
if [ -n "$bl" ]; then
    max=$(cat "$bl/max_brightness"); cur=$(cat "$bl/brightness")
    d=$((max * step / 100)); [ "$d" -lt 1 ] && d=1
    if [ "$1" = down ]; then new=$((cur - d)); else new=$((cur + d)); fi
    # never fully dark: 1% at the bottom
    min=$((max / 100)); [ "$min" -lt 1 ] && min=1
    [ "$new" -lt "$min" ] && new=$min
    [ "$new" -gt "$max" ] && new=$max
    busctl call org.freedesktop.login1 /org/freedesktop/login1/session/auto org.freedesktop.login1.Session \
        SetBrightness ssu backlight "${bl##*/}" "$new" 2>/dev/null || brightnessctl -q set "$new" 2>/dev/null
    pct=$((new * 100 / max))
else
    cache="${XDG_RUNTIME_DIR:-/tmp}/mist-ddc-buses"
    [ -s "$cache" ] || ddcutil detect --brief 2>/dev/null | sed -n 's|.*I2C bus: */dev/i2c-||p' > "$cache"
    op=+; [ "$1" = down ] && op=-
    for b in $(cat "$cache"); do ddcutil --bus "$b" --noverify --sleep-multiplier .3 setvcp 10 "$op" "$step" & done
    wait
    # the level of the first monitor ("VCP 10 C <cur> <max>")
    b=$(head -n1 "$cache")
    [ -n "$b" ] && pct=$(ddcutil --bus "$b" --sleep-multiplier .3 -t getvcp 10 2>/dev/null | awk '{ if ($5 > 0) print int($4 * 100 / $5) }')
fi
[ -n "$pct" ] && mist-ui-bin msg island brightness "$pct" >/dev/null 2>&1
exit 0
