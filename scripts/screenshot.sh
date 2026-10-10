#!/bin/sh
# Screenshots into swash (annotate, copy, save): screenshot.sh region|full
# region = slurp selection, full = the focused monitor.
name="swash-$(date +'%Y-%m-%d_%H:%M:%S').png"
if [ "$1" = full ]; then
    out=$(hyprctl monitors -j | jq -r '.[] | select(.focused) | .name')
    grim -o "$out" - | swash --stdin --name "$name"
else
    g=$(slurp -d -b '#0a0e1866' -c '#8ad6d0' -s '#00000000' -w 2) || exit 0
    grim -g "$g" - | swash --stdin --name "$name"
fi
