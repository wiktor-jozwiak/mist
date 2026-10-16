#!/bin/bash
# Screenshots: region | full
# Zamraża ekran, wycina czysty obszar bez ramek, zapisuje na dysk i natychmiast kopiuje do schowka Wayland

# $MIST_SHOT_DIR, else Pictures/Screenshots (XDG, "Zrzuty ekranu" in Polish)
pics=$(xdg-user-dir PICTURES 2>/dev/null); [ -n "$pics" ] && [ "$pics" != "$HOME" ] || pics="$HOME/Pictures"
case "${LANG:-}" in pl*) sub="Zrzuty ekranu" ;; *) sub="Screenshots" ;; esac
dir="${MIST_SHOT_DIR:-$pics/$sub}"
mkdir -p "$dir"
ts=$(date +'%Y-%m-%d_%H:%M:%S')
out_file="$dir/Zrzut ekranu ${ts//_/\ o\ }.png"
name="swash-${ts}.png"

if [ "$1" = "full" ]; then
    out=$(hyprctl monitors -j | jq -r '.[] | select(.focused) | .name')
    grim -o "$out" "$out_file" || exit 1
    wl-copy --type image/png < "$out_file"
    swash "$out_file" --name "$name" >/dev/null 2>&1 &
    exit 0
fi

# 1. Błyskawiczny zrzut całego pulpitu do pamięci /tmp (RAM)
tmp_screen="/tmp/screen_freeze_${UID}_$$.png"
grim "$tmp_screen" || exit 1

# 2. Zamrożenie ekranu w tle na czas rysowania zaznaczenia
hyprpicker -r -z >/dev/null 2>&1 &
PICKER_PID=$!
sleep 0.05

# 3. Zaznaczenie obszaru przez slurp
g=$(slurp -d -b '#00000040' -c '#8ad6d0' -s '#ffffff15' -w 1 2>/dev/null)
SLURP_STATUS=$?

# Odmrożenie ekranu
kill "$PICKER_PID" 2>/dev/null
wait "$PICKER_PID" 2>/dev/null

if [ $SLURP_STATUS -ne 0 ] || [ -z "$g" ]; then
    rm -f "$tmp_screen"
    exit 0
fi

# 4. Konwersja geometrii ze slurpa ("X,Y WxH") na format ImageMagick ("WxH+X+Y")
crop_geom=$(python3 -c "
import sys
g = sys.argv[1]
xy, wh = g.split(' ')
x, y = xy.split(',')
w, h = wh.split('x')
print(f'{w}x{h}+{x}+{y}')
" "$g" 2>/dev/null)

if [ -n "$crop_geom" ]; then
    magick "$tmp_screen" -crop "$crop_geom" +repage "$out_file"
fi

rm -f "$tmp_screen"

# Sprawdzenie czy plik wynikowy istnieje i nie jest pusty
if [ ! -s "$out_file" ]; then
    exit 1
fi

# 5. Natychmiastowe wrzucenie do schowka Wayland (Ctrl+V zadziała od razu)
wl-copy --type image/png < "$out_file"

# 6. Otwarcie edytora Swash w tle
swash "$out_file" --name "$name" >/dev/null 2>&1 &
