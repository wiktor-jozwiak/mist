#!/bin/sh
# Builds the native Mist parts and installs them as ~/.local/bin/mist-ui-bin and mist-wall-bin (atomic
# replace; the guard ~/.local/bin/mist-ui restarts the running mist-ui, which starts mist-wall) and
# ~/.local/bin/mist-comp (the compositor; session/start-mist runs it, a running one is not touched).
set -e
cd "$(dirname "$0")"
# Mist finds its scripts, shaders and assets through ~/.local/share/mist/shell (common/paths.rs): point it at
# this checkout, wherever it lives (an existing link or directory is left alone)
data=${XDG_DATA_HOME:-$HOME/.local/share}
[ -e "$data/mist/shell" ] || { mkdir -p "$data/mist"; ln -s "$(cd .. && pwd)" "$data/mist/shell"; }
cargo build --release
for b in mist-ui mist-wall; do
    cp target/release/$b "$HOME/.local/bin/.$b-bin.new"
    mv -f "$HOME/.local/bin/.$b-bin.new" "$HOME/.local/bin/$b-bin"
    ls -l "$HOME/.local/bin/$b-bin"
done
# the guard (dist/mist-ui) only on a first install: a local copy may carry its own fallback
[ -e "$HOME/.local/bin/mist-ui" ] || install -m 755 dist/mist-ui "$HOME/.local/bin/mist-ui"
cp target/release/mist-comp "$HOME/.local/bin/.mist-comp.new"
mv -f "$HOME/.local/bin/.mist-comp.new" "$HOME/.local/bin/mist-comp"
ls -l "$HOME/.local/bin/mist-comp"
