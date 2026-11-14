#!/bin/sh
# Builds the native Mist parts and installs them as ~/.local/bin/mist-ui-bin and mist-wall-bin (atomic
# replace; the guard ~/.local/bin/mist-ui restarts the running mist-ui, which starts mist-wall) and
# ~/.local/bin/mist-comp (the compositor; session/start-mist runs it, a running one is not touched).
# The binaries they replace stay as ~/.local/bin/.<name>.prev:  install.sh --rollback  puts them back.
set -e
cd "$(dirname "$0")"
bin=$HOME/.local/bin
if [ "$1" = --rollback ]; then
    n=0
    for b in mist-ui-bin mist-wall-bin mist-comp; do
        [ -e "$bin/.$b.prev" ] || continue
        mv -f "$bin/.$b.prev" "$bin/$b"; n=$((n + 1)); ls -l "$bin/$b"
    done
    [ $n -gt 0 ] || { echo "install.sh: nothing to roll back" >&2; exit 1; }
    exit 0
fi
# Mist finds its scripts, shaders and assets through ~/.local/share/mist/shell (common/paths.rs): point it at
# this checkout, wherever it lives (an existing link or directory is left alone)
data=${XDG_DATA_HOME:-$HOME/.local/share}
[ -e "$data/mist/shell" ] || { mkdir -p "$data/mist"; ln -s "$(cd .. && pwd)" "$data/mist/shell"; }
cargo build --release
# a fresh account may have no ~/.local/bin yet
mkdir -p "$HOME/.local/bin"
for b in mist-ui mist-wall; do
    cp target/release/$b "$HOME/.local/bin/.$b-bin.new"
    [ -e "$HOME/.local/bin/$b-bin" ] && cp -p "$HOME/.local/bin/$b-bin" "$HOME/.local/bin/.$b-bin.prev"
    mv -f "$HOME/.local/bin/.$b-bin.new" "$HOME/.local/bin/$b-bin"
    ls -l "$HOME/.local/bin/$b-bin"
done
# the guard (dist/mist-ui) only on a first install: a local copy may carry its own fallback
[ -e "$HOME/.local/bin/mist-ui" ] || install -m 755 dist/mist-ui "$HOME/.local/bin/mist-ui"
cp target/release/mist-comp "$HOME/.local/bin/.mist-comp.new"
[ -e "$HOME/.local/bin/mist-comp" ] && cp -p "$HOME/.local/bin/mist-comp" "$HOME/.local/bin/.mist-comp.prev"
mv -f "$HOME/.local/bin/.mist-comp.new" "$HOME/.local/bin/mist-comp"
ls -l "$HOME/.local/bin/mist-comp"
# mist-try: the whole desktop in a window on the current one
install -m 755 mist-comp/session/mist-try "$HOME/.local/bin/mist-try"
# the apps' launchers and icons (Files, Player, Settings): Exec points at this install; the dock's old tiles
# of them are dropped so it draws the new icons
apps=${XDG_DATA_HOME:-$HOME/.local/share}
mkdir -p "$apps/applications" "$apps/icons/hicolor/scalable/apps"
cp assets/icons/hicolor/scalable/apps/mist-*.svg "$apps/icons/hicolor/scalable/apps/"
for f in dist/applications/*.desktop; do
    sed "s|^Exec=mist-ui-bin|Exec=$HOME/.local/bin/mist-ui-bin|" "$f" >"$apps/applications/${f##*/}"
done
rm -f "$HOME"/.cache/mist/tiles/mist-*.png
[ -f "$apps/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null &&
    gtk-update-icon-cache -q -f -t "$apps/icons/hicolor" || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps/applications" || true
