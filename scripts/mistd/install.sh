#!/bin/sh
# Builds mistd and puts it where the QML looks for it (scripts/bin/mistd). The new binary is moved over the
# old one, so the running watchers keep their (unlinked) copy until Quickshell restarts them.
set -e
cd "$(dirname "$0")"
# bundled SQLite is C: no GCC LTO bytecode (a -flto CFLAGS from the environment leaves its symbols undefined)
CFLAGS="${CFLAGS:-} -fno-lto" cargo build --release
mkdir -p ../bin
cp target/release/mistd ../bin/.mistd.new
mv -f ../bin/.mistd.new ../bin/mistd
ls -l ../bin/mistd
