# Mist

macOS-style Hyprland shell. Quickshell QML (root `*.qml`, `components/`) being replaced stage by stage by native Rust (`native/`). Details per area live in auto-memory; read the matching memory before digging through code.

## Layout
- `native/` cargo workspace: `mist-ui` (CPU-drawn surfaces: bar, dock, island, desktop, panes), `mist-wall` (GL wallpaper), `common/`. See `native/README.md`.
- `scripts/mistd/` Rust backends, one binary `mistd <mode>`.
- `*.before-*` files are old backups (gitignored) — ignore them.

## Build / install
- mist-ui + mist-wall: `native/install.sh` -> `~/.local/bin/mist-ui-bin`, `mist-wall-bin` (guard `~/.local/bin/mist-ui` restarts it). Log `~/.cache/mist/ui.log`.
- mistd: `scripts/mistd/install.sh` -> `scripts/bin/mistd`. Tests: `cargo test --release` in `scripts/mistd`.
- Keep `lto = false` (rustc + system LLVM segfaults with LTO).
- QML: Quickshell reloads on save; IPC via `mist-qs ipc call mist ...`.

## Checks (prefer these over screenshots)
- `mist-ui --test` (overlay bar on HDMI-A-1), `mist-ui --png out.png '{json}'`, `mist-ui --desk-png dir`, `mist-ui --tray-dump`
- `mist-ui-bin msg ui stats|popup`, `mist-wall-bin --png out.ppm <preset|photo|sky> w h`
- Hyprland dispatch: `hyprctl eval 'hl.dispatch(...)'`

## Rules
- Never write through symlinks in test dirs (unlink first).
- Git commits: `git -c user.name=Wiktor -c user.email=wjozwiak92@gmail.com commit ...`
- Build output: pipe through `tail -30` / `grep -E 'error|warning: unused'` instead of dumping full cargo logs.
- Screenshots cost the most tokens: first verify with `--png` renders + `magick compare -metric AE` numbers; when an image must be viewed, crop to the region (`grim -g "x,y wxh"`) and downscale (`grim -s 0.5`), never full dual-monitor shots.
- No fixed `sleep N` waits: poll for the condition (`until pgrep ...; do sleep 0.2; done`, `timeout 10`).
- Edit files with the Edit tool (small diffs), not python scripts that rewrite whole files.
