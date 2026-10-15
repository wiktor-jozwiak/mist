#!/bin/sh
# Native Mist greeter for greetd: mist-comp on the TTY runs `mist-ui greeter` (the lock screen, logging in
# through greetd IPC) and quits with it; greetd then starts the chosen session.
# Replaces the old lock/install-greeter.sh (Quickshell inside noctalia-greeter-compositor; in git history).
# Run: sudo ./install-greeter.sh [hyprland|mist]   (session after login, default hyprland)
#      sudo ./install-greeter.sh --revert          (the greetd config from before, kept as config.toml.before-mist-native)
# Build first as the user: cargo build --release -p mist-comp -p mist-ui (in native/).
set -eu
SRC=$(cd "$(dirname "$0")/../.." && pwd)            # native/
U=${SUDO_USER:-wiktor}
H=$(getent passwd "$U" | cut -d: -f6)
D=/usr/share/mist-greeter
CONF=/etc/greetd/config.toml

[ "$(id -u)" = 0 ] || { echo "uruchom przez sudo"; exit 1; }

if [ "${1:-}" = "--revert" ]; then
    [ -f "$CONF.before-mist-native" ] && cp "$CONF.before-mist-native" "$CONF" && echo "przywrócono $CONF"
    exit 0
fi
for b in mist-comp mist-ui; do
    [ -x "$SRC/target/release/$b" ] || { echo "brak $SRC/target/release/$b: najpierw cargo build --release"; exit 1; }
done

# binaries readable by the greeter user (the user's ~/.local is behind a 700 home)
install -m755 "$SRC/target/release/mist-comp" /usr/local/bin/mist-greeter-comp
install -m755 "$SRC/target/release/mist-ui" /usr/local/bin/mist-greeter-ui

mkdir -p "$D"
install -m644 "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf" "$D/SF-Pro.ttf"
install -m644 "$SRC/assets/tabler-icons.ttf" "$D/tabler-icons.ttf"
# Mist's photo wallpaper (~/.config/mist/wallpaper.json "image")
WALL=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1])).get('image',''))" "$H/.config/mist/wallpaper.json" 2>/dev/null || true)
WALLF=""
[ -f "$WALL" ] && install -m644 "$WALL" "$D/wallpaper.${WALL##*.}" && WALLF="wallpaper.${WALL##*.}"
AV=""
[ -f "$H/.face" ] && install -m644 "$H/.face" "$D/avatar" && AV="avatar"
NAME=$(getent passwd "$U" | cut -d: -f5 | cut -d, -f1)

case "${1:-hyprland}" in
    mist) SESSION="\"$SRC/mist-comp/session/start-mist\""; DESK=mist ;;
    *) SESSION='"start-hyprland"'; DESK=Hyprland ;;
esac
cat > "$D/greeter.json" <<JSON
{
  "user": "$U",
  "name": "${NAME:-$U}",
  "avatar": "$AV",
  "wallpaper": "$WALLF",
  "session": [$SESSION],
  "env": ["XDG_SESSION_TYPE=wayland", "XDG_CURRENT_DESKTOP=$DESK", "XDG_SESSION_DESKTOP=$DESK"]
}
JSON

# the greeter's compositor: no binds (nothing to launch from the login screen), keyboard, monitor layout
{
    echo "# mist-greeter-comp (install-greeter.sh): no binds, no autostart"
    echo "keyboard pl"
    echo "numlock on"
    echo "pointer flat"
    grep -E '^[A-Za-z]' "$H/.config/mist/outputs" 2>/dev/null | sed 's/^/output /' || true
} > "$D/comp.conf"
chmod 644 "$D"/*

cat > "$D/run" <<SH
#!/bin/sh
# started by greetd as the greeter user
export MIST_COMP_CONFIG=$D/comp.conf MIST_FONT_DIR=$D XDG_SESSION_TYPE=wayland
exec /usr/local/bin/mist-greeter-comp --tty --session "/usr/local/bin/mist-greeter-ui greeter $D/greeter.json"
SH
chmod 755 "$D/run"

[ -f "$CONF.before-mist-native" ] || cp "$CONF" "$CONF.before-mist-native"
cat > "$CONF" <<TOML
[terminal]
vt = 1

[default_session]
command = "$D/run"
user = "greeter"
TOML
echo "gotowe: $D, $CONF (kopia: $CONF.before-mist-native). Sesja po zalogowaniu: ${1:-hyprland}. Zadziała przy następnym logowaniu."
echo "Odwrót: sudo $0 --revert. Test na sucho (bez greetd): native/mist-comp/tests/t-greeter.sh"
