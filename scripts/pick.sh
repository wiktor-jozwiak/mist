#!/bin/bash
# Colour picker: hyprpicker freezes the screen, the colour goes to the clipboard as #rrggbb and the island
# shows it with a swatch.
c=$(hyprpicker -a -n -f hex 2>/dev/null | tail -1)
[ -n "$c" ] || exit 0
mist-ui-bin msg island color "$c" >/dev/null 2>&1
