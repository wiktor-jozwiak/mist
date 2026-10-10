#!/bin/sh
# Monitor brightness over DDC/CI, every monitor at once: bright.sh up|down [step %]
# (bound to XF86MonBrightnessUp/Down; key repeat while one change runs is dropped)
exec 9>"${XDG_RUNTIME_DIR:-/tmp}/mist-bright.lock"
flock -n 9 || exit 0
cache="${XDG_RUNTIME_DIR:-/tmp}/mist-ddc-buses"
[ -s "$cache" ] || ddcutil detect --brief 2>/dev/null | sed -n 's|.*I2C bus: */dev/i2c-||p' > "$cache"
op=+; [ "$1" = down ] && op=-
for b in $(cat "$cache"); do ddcutil --bus "$b" --noverify --sleep-multiplier .3 setvcp 10 "$op" "${2:-10}" & done
wait
