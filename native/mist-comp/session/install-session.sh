#!/bin/sh
# One command for a workstation: the newest mist-comp + Mist shell from this checkout, and a "Mist" session on
# the login screen next to Hyprland (which stays as it is).
#   native/mist-comp/session/install-session.sh [BRANCH] [--greeter]
#     BRANCH      git branch to update to (default: dev); local changes stop the script, nothing is lost
#     --greeter   also make the Mist greeter (greetd) start the Mist session after login
#                 (back: sudo native/mist-comp/session/install-greeter.sh hyprland, or --revert)
# Run as your user; sudo is asked for the session entry (and the greeter).
set -eu
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
BRANCH=dev
GREETER=0
for a in "$@"; do
    case $a in
        --greeter) GREETER=1 ;;
        -*) echo "unknown option $a"; exit 1 ;;
        *) BRANCH=$a ;;
    esac
done
[ "$(id -u)" != 0 ] || { echo "run as your user (sudo is asked when needed)"; exit 1; }
cd "$ROOT"

echo "== 1/4 git: $BRANCH"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "local changes in $ROOT: commit or stash them first"; git status --short --untracked-files=no; exit 1
fi
git fetch origin "$BRANCH"
git checkout "$BRANCH"
git merge --ff-only "origin/$BRANCH"
git log -1 --format='   %h %s (%cr)'

echo "== 2/4 build + install (mistd, mist-ui, mist-wall, mist-comp)"
scripts/mistd/install.sh
native/install.sh # replaces ~/.local/bin/*; the guard restarts a running mist-ui on the new binary

echo "== 3/4 \"Mist\" session for the login screen"
SESS=/usr/share/wayland-sessions/mist.desktop
if ! cmp -s native/mist-comp/session/mist.desktop "$SESS" 2>/dev/null; then
    sudo install -Dm644 native/mist-comp/session/mist.desktop "$SESS"
fi
echo "   $SESS"
# `mist-session hyprland|mist|update|status` from any console
sudo ln -sf "$ROOT/native/mist-comp/session/mist-session" /usr/local/bin/mist-session
echo "   /usr/local/bin/mist-session"

echo "== 4/4 greeter"
if [ $GREETER = 1 ]; then
    sudo native/mist-comp/session/install-greeter.sh mist
else
    echo "   unchanged (pick \"Mist\" in your login manager, or rerun with --greeter)"
fi

echo
echo "done: log out and choose the Mist session. Hyprland stays installed and selectable."
echo "log: ~/.cache/mist/comp.log (previous session: comp.log.old); Ctrl+Alt+BackSpace leaves mist-comp."
