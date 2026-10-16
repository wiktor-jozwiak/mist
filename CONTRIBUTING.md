# Contributing

Mist is one person's daily driver, so changes are judged by one question first: what does it cost while
nobody is looking at the screen?

## Performance rules

1. **CPU-drawn `wl_shm` surfaces** in `mist-ui`. No new process with an EGL / GL context: on NVIDIA each costs
   100+ MB. `mist-wall` is the only GPU process and exits when it has nothing to animate.
2. **Event-driven.** React to the Hyprland socket, D-Bus signals, `pactl subscribe`, inotify, `mistd` streams.
   No polling timers for data that has an event; when a timer is unavoidable, make it adaptive (slow tick).
3. **No work for hidden surfaces.** Occluded or closed surfaces do not paint, animate or wake the loop.
4. **Repaint only damage.** Mark the region that changed, not the whole surface.

## Workflow

```bash
cd native && cargo build --release && cargo test --release
cd scripts/mistd && cargo test --release
native/install.sh                     # installs and restarts the running mist-ui through its guard
```

- Prefer offscreen checks over screenshots: `mist-ui --png`, `--desk-png`, `--dock-png`, `mist-wall-bin --png`,
  compared with `magick compare -metric AE` against the previous binary. A refactor must render 0 px different.
- Every change to a running surface states its cost in the PR: PSS (`bench/measure.sh`), CPU, paints
  (`mist-ui-bin msg ui stats`).
- Keep `lto = false` in the release profiles (Arch rustc with system LLVM segfaults with LTO).
- Do not commit secrets or personal data: credentials and tokens live in `~/.config/mist/*.json`, never in
  source. Test data must be made up.
- Match the surrounding code: small focused comments that explain why, no dead code, no `*.before-*` backups
  in commits (they are gitignored).

## Commits

One logical change per commit, subject line says what changed for the user ("Dock: clicking the active app
minimizes its windows"). By contributing you agree that your contribution is licensed under GPL-3.0.
