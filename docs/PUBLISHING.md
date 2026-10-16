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

- [x] **Personal defaults.** Done: the weather place is `~/.config/mist/location.json`, chosen in
      *Dostosuj pulpit* → Ogólne (search through `mistd geo`, Open-Meteo geocoding); the sky fallbacks are
      Greenwich, the rain demo says Kraków. Left: the old home town and its coordinates are still in git
      history (`scripts/mistd/src/data.rs` before 66a342f). Rewrite history or accept it before going public.
- [x] **Fixed install path.** Done: `native/common/paths.rs` (`$MIST_DIR`, `~/.local/share/mist/shell`
      linked by `native/install.sh`, `/usr/share/mist`, then the old `~/.config/quickshell/mist`); scripts
      use XDG Pictures. Left: the checkout itself still lives in `~/.config/quickshell` until QML is gone.
- [x] **Polish UI strings.** Done: `native/assets/i18n.tsv` (pl, en, de, es), auto follows `$LANG` with an
      English fallback. Left: text built from fragments at run time that no row matches stays Polish
      (run `MIST_LANG=en` and look); the single-letter week initials on the running card.
- [ ] **Personal integrations.** Garmin (InfluxDB of garmin-grafana), running plan, KDE Connect device id in
      a test (`phone.rs`, `runcommand_is_stable`), HyperOS-specific ADB call detection. Keep them, but behind
      config and documented as optional; cards without data should hide.
- [ ] **Legacy QML.** Root `*.qml`, `components/`, `shaders/*.qsb` consumers: Quickshell no longer starts.
      Move to a `legacy/` branch or tag and delete from `main`. Done: `mist-wall` reads GLSL 150 from
      `shaders/glsl` (`regen.sh`), `.qsb` only as a fallback, so `qsb` is no longer a runtime dependency.
- [ ] **Screenshots.** `docs/img/desktop.webp` is an offscreen composite without blur. Add real screenshots
      of a clean session (no personal windows, demo data in `~/.cache/mist/*.json`).

## Nice to have

- [x] Packaging: `packaging/arch/PKGBUILD` (`mist-shell-git`, not on the AUR yet) and a release workflow
      with prebuilt binaries on `v*` tags.
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
