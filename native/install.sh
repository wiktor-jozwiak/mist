#!/bin/sh
# Builds the native Mist surfaces and installs them as ~/.local/bin/mist-ui-bin (atomic replace; the
# guard ~/.local/bin/mist-ui restarts the running one).
set -e
cd "$(dirname "$0")"
cargo build --release
cp target/release/mist-ui "$HOME/.local/bin/.mist-ui-bin.new"
mv -f "$HOME/.local/bin/.mist-ui-bin.new" "$HOME/.local/bin/mist-ui-bin"
ls -l "$HOME/.local/bin/mist-ui-bin"
