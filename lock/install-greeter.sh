#!/bin/sh
# Installs the macOS-style Mist greeter for greetd. Run: sudo ./install-greeter.sh   (revert: sudo ./install-greeter.sh --revert)
set -eu
SRC=$(cd "$(dirname "$0")" && pwd)
U=${SUDO_USER:-wiktor}
H=$(getent passwd "$U" | cut -d: -f6)
D=/usr/share/mist-greeter
CONF=/etc/greetd/config.toml

[ "$(id -u)" = 0 ] || { echo "uruchom przez sudo"; exit 1; }

if [ "${1:-}" = "--revert" ]; then
    [ -f "$CONF.before-mist" ] && cp "$CONF.before-mist" "$CONF" && echo "przywrócono $CONF (noctalia-greeter)"
    exit 0
fi

# Quickshell runtime readable by the greeter user (the copy in ~/.local is behind a 700 home)
mkdir -p /opt/mist-qs
rm -rf /opt/mist-qs/usr.new
cp -a "$H/.local/opt/mist-qs/usr" /opt/mist-qs/usr.new
rm -rf /opt/mist-qs/usr && mv /opt/mist-qs/usr.new /opt/mist-qs/usr
chown -R root:root /opt/mist-qs

# greeter files, font, wallpaper, avatar
mkdir -p "$D"
install -m644 "$SRC/MacLock.qml" "$SRC/greeter.qml" "$SRC/glasstext.frag.qsb" "$D/"
install -m755 "$SRC/run-greeter.sh" "$D/"
install -m644 "$H/.local/share/fonts/SegUIVar.ttf" "$D/SegUIVar.ttf"
# Mist's photo wallpaper (~/.config/mist/wallpaper.json "image"), else Noctalia's old state
WALL=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1])).get('image',''))" "$H/.config/mist/wallpaper.json" 2>/dev/null || true)
[ -f "$WALL" ] || WALL=$(sed -n '/^\s*\[wallpaper\.monitors\.DP-3\]/{n;s/.*path *= *"\(.*\)".*/\1/p}' "$H/.local/state/noctalia/settings.toml" 2>/dev/null || true)
[ -f "$WALL" ] && install -m644 "$WALL" "$D/wallpaper.${WALL##*.}" && WALLF="wallpaper.${WALL##*.}" || WALLF=""
AV=""
[ -f "$H/.face" ] && install -m644 "$H/.face" "$D/avatar" && AV="avatar"
NAME=$(getent passwd "$U" | cut -d: -f5 | cut -d, -f1 | python3 -c "import sys;print(' '.join(w[:1].upper()+w[1:].lower() for w in sys.stdin.read().split()))")
cat > "$D/greeter.json" <<JSON
{
  "user": "$U",
  "name": "${NAME:-$U}",
  "avatar": "$AV",
  "wallpaper": "$WALLF",
  "session": ["start-hyprland"],
  "env": ["XDG_SESSION_TYPE=wayland", "XDG_CURRENT_DESKTOP=Hyprland", "XDG_SESSION_DESKTOP=Hyprland"]
}
JSON

# greetd: keep the noctalia compositor, swap the greeter binary
[ -f "$CONF.before-mist" ] || cp "$CONF" "$CONF.before-mist"
cat > "$CONF" <<TOML
[terminal]
vt = 1

[default_session]
command = "env NOCTALIA_GREETER_BIN=$D/run-greeter.sh /usr/bin/noctalia-greeter-session"
user = "greeter"
TOML
echo "gotowe: $D, $CONF (kopia: $CONF.before-mist). Zadziała przy następnym logowaniu."
