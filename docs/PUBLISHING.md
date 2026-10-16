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
- [x] Nothing personal committed since (checked 2026-10-16): the keyword grep over `git log -p --all` only
      finds placeholders (`.env.example`, docs) and code; the real InfluxDB password, user and the iCal
      URLs of `feeds.json` occur in no commit. Run it again right before the switch:
      `git log -p --all | grep -iE 'password|token|secret|api[_-]?key'`.

## Should fix

- [x] **Personal defaults.** Done: the weather place is `~/.config/mist/location.json`, chosen in
      *Dostosuj pulpit* → Ogólne (search through `mistd geo`, Open-Meteo geocoding); the sky fallbacks are
      Greenwich, the rain demo says Kraków. Left: the old home town and its coordinates are still in git
      history (`scripts/mistd/src/data.rs` before 66a342f, legacy `Island.qml`, `Desktop.qml`, `wxcard.rs`) and
      the phone's KDE Connect id (`phone.rs` before 2839b54). A rewritten copy (`git filter-branch`: Warszawa,
      its coordinates, a dummy id; the tree of `main` unchanged) is ready; force-pushing it is the last step.
- [x] **Fixed install path.** Done: `native/common/paths.rs` (`$MIST_DIR`, `~/.local/share/mist/shell`
      linked by `native/install.sh`, `/usr/share/mist`, then the old `~/.config/quickshell/mist`); scripts
      use XDG Pictures. Left: the checkout itself still lives in `~/.config/quickshell` until QML is gone.
- [x] **Polish UI strings.** Done: `native/assets/i18n.tsv` (pl, en, de, es), auto follows `$LANG` with an
      English fallback; `MIST_I18N_MISS=file` logs every string without a row (only the place name Kraków left
      in the demo renders); week initials, wrapped text and wallpaper presets are translated; app names
      reload when the language changes.
- [x] **Personal integrations.** Garmin and running cards only with `~/.config/mist/influx.json` or a cache
      (no `mistd` call without one, the cards are not offered); the KDE Connect test walks every paired
      device; ADB call detection documented as optional ([CONFIG.md](CONFIG.md)).
- [x] **Legacy QML.** Root `*.qml` and `components/` are off `main`, kept under the tag `legacy-qml`. Done: `mist-wall` reads GLSL 150 from
      `shaders/glsl` (`regen.sh`), `.qsb` only as a fallback, so `qsb` is no longer a runtime dependency.
- [x] **Screenshots.** `docs/img/hero.sh` and `panels.sh` render offscreen from a throwaway HOME (demo data,
      no windows, notifications or accounts of the running session) with Hyprland's blur emulated under the
      glass. Left: real grabs of a nested session need the monitors on.

## Nice to have

- [x] Packaging: `packaging/arch/PKGBUILD` (`mist-shell-git`, not on the AUR yet) and a release workflow
      with prebuilt binaries on `v*` tags.
- [x] Config docs: [CONFIG.md](CONFIG.md).
- [x] CI: build, tests, shell syntax, `cargo clippy` (correctness, suspicious, perf fail) and `cargo deny`
      (licences, sources; advisories reported, not blocking).
- [ ] Benchmarks of HyprPanel, DankMaterialShell, Ironbar, Noctalia and Waybar on the same machine: the
      script is `bench/others.sh`, none of them is installed here yet ([BENCHMARKS.md](BENCHMARKS.md)).
- [ ] Trademark care: the name "Mist" is fine; avoid Apple marks in names of binaries and in the logo.
      The README already carries a non-affiliation note.

## Release steps

1. Blockers done, CI green on `main`.
2. `CHANGELOG.md`: move "Unreleased" to `0.1.0` with the date; `git tag -a v0.1.0`.
3. GitHub: description, topics (`hyprland`, `wayland`, `rust`, `desktop-shell`, `macos`), social preview
   image (`docs/img/social-preview.png`, 1280×640, uploaded by hand), enable Issues and Discussions, disable Wiki.
4. Settings → General → Danger zone → Change visibility → Public.
5. Announce: r/hyprland, Hyprland Discord `#showcase`, r/unixporn (screenshot + numbers).
