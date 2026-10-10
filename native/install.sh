#!/bin/sh
# Builds the native Mist parts and installs them as ~/.local/bin/mist-ui-bin and mist-wall-bin (atomic
# replace; the guard ~/.local/bin/mist-ui restarts the running mist-ui, which starts mist-wall).
set -e
cd "$(dirname "$0")"
cargo build --release
for b in mist-ui mist-wall; do
    cp target/release/$b "$HOME/.local/bin/.$b-bin.new"
    mv -f "$HOME/.local/bin/.$b-bin.new" "$HOME/.local/bin/$b-bin"
    ls -l "$HOME/.local/bin/$b-bin"
done
