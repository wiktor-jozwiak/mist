#!/bin/sh
# Strings that reach the screen without a translation: renders every pane, Settings page, Mist Player view,
# Mist Files and the bar offscreen in English with MIST_I18N_MISS and prints what was drawn untranslated.
# Read the list by eye: app names, file names and wrapped fragments of translated text are expected, Polish
# is not. Lines with Polish letters come first.
#   native/mist-ui/tests/i18n-miss.sh [lang]
set -u
here=$(cd "$(dirname "$0")" && pwd); repo=$(cd "$here/../../.." && pwd)
ui=${MIST_UI:-$repo/native/target/release/mist-ui}; lang=${1:-en}
w=$(mktemp -d); trap 'rm -rf "$w"' EXIT
mkdir -p "$w/home/.cache/mist" "$w/home/.config/mist" "$w/home/.config/quickshell"
cp "$repo"/docs/demo/weather.json "$repo"/docs/demo/garmin.json "$repo"/docs/demo/running.json "$w/home/.cache/mist/"
cp "$repo/docs/demo/widgets.json" "$w/home/.config/mist/"
echo '{"welcomed": true}' >"$w/home/.config/mist/settings.json"
ln -s "$repo" "$w/home/.config/quickshell/mist"
run() { HOME=$w/home XDG_RUNTIME_DIR=$w LANG=C.UTF-8 MIST_LANG=$lang MIST_I18N_MISS=$w/miss MIST_DIR=$repo timeout 20 "$@" >/dev/null 2>&1; }
for p in session keys calendar notifications clipboard spotlight emoji wallpaper control phone start start:all switcher; do
    run "$ui" --pane-png $p 1920 1080 "$w/p.png"
done
run "$ui" --pane-png spotlight 1920 1080 "$w/p.png" te
for p in about accounts apps bluetooth colors default-apps desktop devices display home installed language lockscreen mouse \
    multitasking network notifications personalization power sound storage system time wallpaper widgets; do
    run "$ui" settings --png "$w/s.png" $p
done
for p in empty video audio noart plain queue tracks help info chapters seek pip resume; do run "$ui" player --png "$w/pl.png" $p; done
run "$ui" files --png "$w/f.png" "$w/home"
run "$ui" --png "$w/bar.png" '{}'
[ -f "$w/miss" ] || { echo "nothing was logged: is $ui built?" >&2; exit 1; }
sort -u "$w/miss" | grep -v '…' >"$w/all"
grep -E '[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]' "$w/all"
echo "--- without Polish letters ($(grep -cvE '[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]' "$w/all") lines)"
grep -vE '[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]' "$w/all"
