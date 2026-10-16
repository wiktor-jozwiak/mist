# Before going public

The repository is private. This is what has to change before it can be made public, ordered by how bad it
would be to skip it.

## Blockers

- [x] **Old InfluxDB credentials in git history.** They were constants in `scripts/mistd/src/data.rs`;
      the connection now lives in `~/.config/mist/influx.json`, and history was rewritten (2026-10-16)
      before the first push, so no commit on GitHub contains them.
- [x] **Apple sounds.** `sounds/*.wav` (macOS system sounds) were removed from every commit in the same
      rewrite and are gitignored; `notif.rs` falls back to `/usr/share/sounds/freedesktop`. Add own or CC0
      sounds (e.g. freesound.org CC0) if a custom sound is wanted.
- [ ] Before going public, check again that nothing personal was committed since:
      `git log -p | grep -iE 'password|token|secret|api[_-]?key'`.

## Should fix

- [ ] **Personal defaults.** Home town and coordinates are fallbacks in `scripts/mistd/src/data.rs`
      (`LAT`, `LON`, `CITY`), `native/common/sky.rs`, `native/mist-wall/src/main.rs`,
      `native/mist-ui/src/wxcard.rs`, `island.rs` (rain demo). Move to `~/.config/mist/location.json` with a
      neutral default or geolocation.
- [ ] **Fixed install path.** About 15 places build `~/.config/quickshell/mist/...` (mistd binary, scripts,
      shaders, emoji, sounds). One helper `mist_dir()` reading `$MIST_DIR`, default
      `$XDG_DATA_HOME/mist`, and an `install.sh` that copies assets there would let the repo live anywhere.
      The repo should also leave `~/.config/quickshell` once QML is gone.
- [ ] **Polish UI strings.** All labels are Polish literals. Minimum: English default + Polish via a small
      key → string table chosen from `$LANG`.
- [ ] **Personal integrations.** Garmin (InfluxDB of garmin-grafana), running plan, KDE Connect device id in
      a test (`phone.rs`, `runcommand_is_stable`), HyperOS-specific ADB call detection. Keep them, but behind
      config and documented as optional; cards without data should hide.
- [ ] **Legacy QML.** Root `*.qml`, `components/`, `shaders/*.qsb` consumers: Quickshell no longer starts.
      Move to a `legacy/` branch or tag and delete from `main`. Then `mist-wall` should read GLSL from
      `shaders/*.frag` directly (it currently extracts it from `.qsb` with `qsb`, a Qt runtime dependency).
- [ ] **Screenshots.** `docs/img/desktop.webp` is an offscreen composite without blur. Add real screenshots
      of a clean session (no personal windows, demo data in `~/.cache/mist/*.json`).

## Nice to have

- [ ] Packaging: `PKGBUILD` (AUR `mist-shell-git`) building the workspace and installing to `/usr`.
- [ ] Config docs: `~/.config/mist/*.json` keys (widgets, feeds, influx, wallpaper).
- [ ] CI is in `.github/workflows/ci.yml` (build + tests + shell syntax); add `cargo clippy` and `cargo deny` (licenses of
      dependencies).
- [ ] Benchmarks of HyprPanel, DankMaterialShell, end-4 and Waybar on the same machine
      ([BENCHMARKS.md](BENCHMARKS.md)).
- [ ] Trademark care: the name "Mist" is fine; avoid Apple marks in names of binaries and in the logo.
      The README already carries a non-affiliation note.

## Release steps

1. Blockers done, CI green on `main`.
2. `CHANGELOG.md`: move "Unreleased" to `0.1.0` with the date; `git tag -a v0.1.0`.
3. GitHub: description, topics (`hyprland`, `wayland`, `rust`, `desktop-shell`, `macos`), social preview
   image (`docs/img/desktop.webp` as PNG 1280×640), enable Issues and Discussions, disable Wiki.
4. Settings → General → Danger zone → Change visibility → Public.
5. Announce: r/hyprland, Hyprland Discord `#showcase`, r/unixporn (screenshot + numbers).
