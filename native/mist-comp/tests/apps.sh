#!/bin/sh
# The apps of a working day on mist-comp, end to end, as a session started by greetd sees them: the system
# PATH only, the default comp.conf, the shell from its autostart, XWayland on. Each check types real keys:
#   - the Super key alone opens the Start menu (the bind runs mist-ui-bin from ~/.local/bin)
#   - every terminal found (kitty, foot, alacritty, xterm through XWayland) opened by a bind, clicked, then a
#     command typed into its shell runs (writes a file); an X11 window (xev) maps at once
#   - Claude Code starts in a terminal (`claude --version` typed)
#   - a Firefox-family browser (Zen: ZEN=/path/to/zen, else zen-browser / firefox in PATH) takes typing
# Apps that are not installed are skipped. Software rendering, headless, two monitors.
#   native/mist-comp/tests/apps.sh            (after cargo build --release)
set -u
D=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$D/../../.." && pwd)
BIN=${BIN:-$ROOT/native/target/release}
OUT=${OUT:-$(mktemp -d)}
# lower case and digits only: the path is typed into terminals key by key
RT=/tmp/mistapps$$
H=$RT/home
mkdir -p "$RT" "$H/.config/mist" "$H/.cache/mist" "$H/.local/bin" "$H/.local/share/fonts/SF-Pro"
chmod 700 "$RT"
for f in /usr/share/fonts/inter/InterVariable.ttf /usr/share/fonts/TTF/InterVariable.ttf /usr/share/fonts/opentype/inter/Inter-Regular.otf \
         /usr/share/fonts/noto/NotoSans-Regular.ttf /usr/share/fonts/truetype/noto/NotoSans-Regular.ttf; do
    [ -f "$f" ] && { ln -sf "$f" "$H/.local/share/fonts/SF-Pro/SF-Pro.ttf"; break; }
done
# what native/install.sh puts there
ln -sf "$BIN/mist-ui" "$H/.local/bin/mist-ui-bin"
ln -sf "$BIN/mist-ui" "$H/.local/bin/mist-ui"
ln -sf "$BIN/mist-comp" "$H/.local/bin/mist-comp"
CLAUDE=$(command -v claude || true)
ZEN=${ZEN:-$(command -v zen-browser || command -v zen || command -v firefox || true)}
export XDG_RUNTIME_DIR=$RT HOME=$H MIST_DIR=$ROOT MIST_LANG=en LANG=C.UTF-8 MIST_PRIMARY=DP-3
export MIST_UI_NO_SERVICES=1 MIST_UI_SOCK=$RT/ui.sock MIST_COMP_SOFTWARE=1 MIST_COMP_AUTOSTART=1 MIST_COMP_XWAYLAND=1
# greetd's PATH: no ~/.local/bin
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:/bin
unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1${2:+: $2}"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1" "${3:-}"; fi; }
wait_for() { i=0; until eval "$1"; do i=$((i + 1)); [ $i -gt "${2:-100}" ] && return 1; sleep 0.1; done; }
MIST_COMP_OUTPUTS="DP-3:1280x800,HDMI-A-1:1280x800" RUST_LOG=warn,mist_comp=info "$BIN/mist-comp" --headless >"$OUT/comp.log" 2>&1 &
CP=$!
trap 'kill $CP 2>/dev/null; pkill -P $CP 2>/dev/null; sleep 0.5; rm -rf "$RT"' EXIT INT TERM
wait_for "grep -q '^WAYLAND_DISPLAY=' '$OUT/comp.log'" 100 || { echo "FAIL no compositor"; exit 1; }
S=$(grep -m1 '^WAYLAND_DISPLAY=' "$OUT/comp.log" | cut -d= -f2)
C() { echo "$*" | socat -t3 - "UNIX:$RT/mist-comp/$S.sock" 2>/dev/null; }
# one key, with Shift when asked
key() { [ -n "${2:-}" ] && C key 42 press >/dev/null; C key "$1" press >/dev/null; C key "$1" release >/dev/null; [ -n "${2:-}" ] && C key 42 release >/dev/null; }
# type a line (lower case, digits, space . / - _ > :) and Enter
type_line() {
    printf '%s\n' "$1" | fold -w1 | while IFS= read -r ch; do
        case $ch in
            a) key 30 ;; b) key 48 ;; c) key 46 ;; d) key 32 ;; e) key 18 ;; f) key 33 ;; g) key 34 ;; h) key 35 ;;
            i) key 23 ;; j) key 36 ;; k) key 37 ;; l) key 38 ;; m) key 50 ;; n) key 49 ;; o) key 24 ;; p) key 25 ;;
            q) key 16 ;; r) key 19 ;; s) key 31 ;; t) key 20 ;; u) key 22 ;; v) key 47 ;; w) key 17 ;; x) key 45 ;;
            y) key 21 ;; z) key 44 ;; 0) key 11 ;; [1-9]) key $((ch + 1)) ;; .) key 52 ;; /) key 53 ;; -) key 12 ;;
            _) key 12 s ;; '>') key 52 s ;; :) key 39 s ;; '') key 57 ;; *) key 57 ;;
        esac
    done
    key 28
}
win() { C windows | awk -v a="$1" 'tolower($1) ~ a {print; exit}'; }
click_win() {
    G=$(win "$1" | awk '{print $3}'); X=${G%,*}; Y=${G#*,}
    C motion $((X + 80)) $((Y + 80)) >/dev/null; C button left press >/dev/null; C button left release >/dev/null
    sleep 0.3
}

# the shell, started by the config's autostart with this PATH
check "autostart: the Mist shell is up (bar)" "wait_for \"C layers | grep -q 'mist-bar'\" 150"
sleep 2
# Super alone: the Start menu (bindr SUPER+Super_L exec mist-ui-bin …)
before=$(C layers | wc -l)
key 125
check "Super: the Start menu opens" "wait_for \"[ \\\$(C layers | wc -l) -gt $before ]\" 50" "$(C layers | tr '\n' ' ')"
C layers | grep -v -e mist-bar -e mist-dock -e mist-island -e mist-desktop -e mist-live | sed 's/^/     layer: /'
key 125
check "Super again: it closes" "wait_for \"[ \\\$(C layers | wc -l) -le $before ]\" 50"

# terminals: opened through a bind's exec, a command typed into the shell
for t in kitty foot alacritty xterm; do
    command -v $t >/dev/null || { echo "skip $t (not installed)"; continue; }
    C action exec "$t" >/dev/null
    if ! wait_for "[ -n \"\$(win $t)\" ]" 900; then bad "$t: a window" "none in 90 s"; continue; fi
    sleep 3
    click_win $t
    # a first line the shell may mangle while it starts (kitty's shell integration takes the typeahead and
    # gives it back at the end of the line), then the real one
    type_line "true"
    sleep 3
    type_line "echo ok >$RT/t$t"
    check "$t: typed command ran" "wait_for \"grep -q ok $RT/t$t 2>/dev/null\" 50"
    [ -n "${SHOTS:-}" ] && C shot "$OUT/$t.ppm" DP-3 >/dev/null
    [ "$t" = foot ] && FOOT=1
    C action close >/dev/null
    wait_for "[ -z \"\$(win $t)\" ]" 50
done

# X11 through XWayland: a plain window maps at once (xterm alone can take a minute without its bitmap fonts)
if command -v xev >/dev/null; then
    n0=$(C windows | wc -l)
    C action exec xev >/dev/null
    check "XWayland: an X11 window maps" "wait_for \"[ \\\$(C windows | wc -l) -gt $n0 ]\" 100"
    pkill -x xev
fi

# Claude Code in a terminal
if [ -n "$CLAUDE" ] && command -v foot >/dev/null; then
    C action exec "foot" >/dev/null
    wait_for "[ -n \"\$(win foot)\" ]" 200 && sleep 2 && click_win foot
    type_line "$CLAUDE --version >$RT/claude"
    check "claude: starts from a terminal" "wait_for \"grep -qi 'claude' $RT/claude 2>/dev/null\" 150" "$(cat $RT/claude 2>/dev/null)"
    C action close >/dev/null
else
    echo "skip claude (no claude or foot)"
fi

# a Firefox-family browser: a page whose title shows what was typed into its field
if [ -n "$ZEN" ]; then
    mkdir -p "$RT/zp"
    # no first-run welcome over the page
    printf 'user_pref("zen.welcome-screen.seen", true);\nuser_pref("browser.aboutwelcome.enabled", false);\nuser_pref("browser.startup.homepage_override.mstone", "ignore");\n' >"$RT/zp/user.js"
    # the field fills the page: a click anywhere on it lands in the field
    printf '<title>empty</title><textarea autofocus style="position:fixed;inset:0;width:100%%;height:100%%" oninput="document.title=%s+this.value"></textarea>\n' "'typed-'" >"$RT/t.html"
    C action exec "MOZ_ENABLE_WAYLAND=1 $ZEN --no-remote --profile $RT/zp file://$RT/t.html" >/dev/null
    if wait_for "[ -n \"\$(win 'zen|firefox|navigator')\" ]" 600; then
        sleep 20
        B=$(win 'zen|firefox|navigator'); G=$(echo "$B" | awk '{print $3}'); SZ=$(echo "$B" | awk '{print $4}')
        C motion $((${G%,*} + ${SZ%x*} / 2)) $((${G#*,} + ${SZ#*x} * 2 / 3)) >/dev/null
        C button left press >/dev/null; C button left release >/dev/null; sleep 0.5
        printf 'abc\n' | fold -w1 | while IFS= read -r ch; do case $ch in a) key 30 ;; b) key 48 ;; c) key 46 ;; esac; done
        sleep 2
        [ -n "${SHOTS:-}" ] && C shot "$OUT/browser.ppm" DP-3 >/dev/null
        title() { echo "j/clients" | socat -t3 - "UNIX:$RT/hypr/mist-comp-$S/.socket.sock" 2>/dev/null; }
        check "browser: typing reaches the page" "wait_for \"title | grep -q 'typed-abc'\" 100" "$(title | tr ',' '\n' | grep '\"title\"' | head -3 | tr '\n' ' ')"
    else
        bad "browser: a window" "none in 60 s"
    fi
else
    echo "skip browser (ZEN=/path/to/zen)"
fi

grep -q panicked "$OUT/comp.log" && { bad "no panic in the compositor"; grep -A5 panicked "$OUT/comp.log" | head; }
C stats | grep -q frames || bad "compositor still answers"
[ $fails = 0 ] && echo "all ok" || { echo "$fails failed | out: $OUT"; exit 1; }
