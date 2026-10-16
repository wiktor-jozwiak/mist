# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [SemVer](https://semver.org);
nothing is released yet.

## Unreleased

### Added
- Repository prepared for publishing: README in six languages, GPL-3.0 license, benchmarks
  (`docs/BENCHMARKS.md`, `bench/measure.sh`), publishing checklist, contributing guide, CI, issue templates,
  Hyprland example config (`examples/hyprland/mist.lua`), the `mist-ui` guard in `native/dist/`.
- UI font fallback: Inter Variable, then Noto Sans, when SF Pro is not installed (was a panic).
- UI languages: Polski, English, Deutsch, Español (`native/assets/i18n.tsv`, `mist-ui/src/i18n.rs`).
  Chosen in *Dostosuj pulpit* → Wygląd → Język, saved in `~/.config/mist/settings.json`, `MIST_LANG`
  overrides; *auto* follows `$LANG`, English fallback. Applied when text is shaped, so cached strings cost
  nothing; Polish renders are pixel-identical to before.
- Garmin setup guide (`docs/guides/garmin.md`, Polish version) and a compose file for garmin-grafana
  (`integrations/garmin/`).
- Benchmarks table in the README; demo data (`docs/demo`) and `docs/img/hero.sh` for the README
  screenshot, now with the weather card and its sky; an English screenshot.

- Paths: `native/common/paths.rs` finds scripts, shaders and assets through `$MIST_DIR`,
  `~/.local/share/mist/shell`, `/usr/share/mist`; the repo can live anywhere.
- Packaging: `packaging/arch/PKGBUILD` (`mist-shell-git`), release workflow with prebuilt binaries on tags,
  docs site (mkdocs-material), `scripts/demo-reel.sh` for the README GIF.
- Weather place in settings: *Dostosuj pulpit* → Ogólne (language, place search through `mistd geo`,
  wallpaper), saved in `~/.config/mist/location.json`; a welcome line on the first open.

### Changed
- mist-wall reads GLSL 150 from `shaders/glsl`; `qsb` (qt6-shadertools) is no longer needed at run time.
- Live wallpaper presets moved to `native/assets/live-presets.js`; photo wallpapers come from XDG
  Pictures/Wallpapers (`Tapety` in Polish), `$MIST_WALLPAPERS` overrides; a new install starts on the live
  scene.
- Weather has no built-in place: without `location.json` the card stays empty; the timezone follows the place.
- Dock icons are pre-scaled once per size, so a resting dock blits instead of resampling.
- `mistd data garmin` reads the InfluxDB connection from `~/.config/mist/influx.json` instead of constants
  in the source.
- `mist.desktop` no longer contains an absolute home path.

## 2026-10-15
- Native Start, Spotlight, control centre, phone, wallpaper gallery and Windows 11-style snap; Quickshell is
  no longer started.
- `mist-comp`: own Smithay compositor with session and greeter (experimental).
- Performance: shadow fast path, equalizer-only island frames, adaptive slow tick, AVX2 builds.
- Dock: click on the active app minimizes it; window picker with live thumbnails.
- Desktop widgets: hover intent, one settings sheet for layout, places, clock and per-widget data.
- Notification centre limited to 640 × 360 px.

## 2026-10-10
- Rewrite in Rust started: top bar, dock, Dynamic Island, notification server, phone, MPRIS, wallpaper
  (`mist-wall`), desktop cards and weather sky, lock + idle, polkit agent, popups framework, calendar,
  notification centre, Cmd+Tab switcher, clipboard history, screenshot toolbar.
- `mistd`: Rust backends replace the Python scripts.
- Initial import of the Quickshell (QML) version.
