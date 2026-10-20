# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [SemVer](https://semver.org);
nothing is released yet.

## Unreleased

### mist-comp
- Explicit sync (linux-drm-syncobj-v1) where the GPU supports it; commits wait for the client's GPU work (acquire point, else the dmabuf fence).
- Protocols: text-input-v3 + input-method-v2, virtual-keyboard, tablet-v2, ext-foreign-toplevel-list, ext-data-control, xdg-foreign, xdg-toplevel-icon / -tag, content-type, alpha-modifier, xdg-system-bell, security-context (sandboxed clients get no privileged globals), XWayland keyboard grabs; wlroots foreign-toplevel-management, output-management (wlr-randr, kanshi), output-power-management, virtual-pointer, gamma-control.
- wp-fifo opt-in (`fifo on`): FIFO barriers are released on a real frame (vblank, headless tick, nested redraw), so Vulkan vsync clients run at the refresh rate.
- A missing UI font no longer kills the compositor (window bars are drawn without titles); Debian/Fedora font paths added to the fallbacks.
- Screen casting: `copy_with_damage` waits for a new frame (a still screen costs nothing), cursor overlay, BGRA readback without a per-pixel swap, dmabuf rendering opt-in (`screencopy_dmabuf on`), `screencast` events for the island.
- Hyprland compatibility for mist-ui and scripts: `decorations`, `getoption`, window move / resize / close / fullscreen and monitor focus in `eval`, events fullscreen / urgent / focusedmon / activelayout / configreloaded / monitoradded / removed; `mist-comp hyprctl` (linked as `hyprctl` by start-mist when Hyprland is not installed); window drags stream the cursor instead of mist-ui polling it.
- Monitors: comp.conf `output` lines with scale, mode and transform (they were parsed and never used); each surface gets its own output's scale; touchpad gestures (workspace swipe, the rest forwarded to clients).
- Windows stay below the bar when moved or stretched; stretching the top edge grows upward (it grew downwards for some clients).
- Session: crash restart in safe mode, plain log lines, synthetic / remote input counts as activity for the idle lock; `mist-comp migrate` converts a Hyprland Lua config.
- Tests: `native/mist-comp/tests/ci.sh` (headless on Mesa's software renderer, run in CI) and `tests/soak.sh`; `bench/comp.sh`.

### Changed & Performance
- mist-ui sends no frame when a surface's pixels did not change (idle compositor wakeups with the shell: 68.8/s → 0.8/s), talks to the compositor socket instead of starting `hyprctl` processes, and the lock screen redraws only the password band (idle lock CPU 31 % → 3.3 % of a core).

### Fixed
- Lock screen: the clock on every monitor turns with the minute, not only on the primary one.
- Lock screen: sets the arrow cursor on each lock surface, so a cursor an app had hidden (a video, a game) does not stay hidden over the lock screen; `mist-comp` resets it when the session locks.
- Lock screen: avatar and password field fall back to the first monitor when the configured primary is not connected; the arrow button only reacts on the monitor that shows it.
- Windows stay below the top bar: stretching the top edge or dragging a window no longer puts it over the bar or under the Dynamic Island. `mist-comp` stops the move / resize at the usable area; on Hyprland (which reports no resize) `mist-ui` puts the active floating window back below the bar when the pointer comes onto the bar or the island.
- `mist-comp`: the pointer is clamped to the nearest monitor instead of the monitors' bounding box, so it cannot get lost in a corner no monitor shows.
- Phone run commands use `mist-ui-bin` from `$PATH` (packaged `/usr/bin`) when `~/.local/bin/mist-ui-bin` is missing; command ids unchanged.
- Screenshot file names follow `$LANG` like their folder (`Screenshot <date> at <time>.png`).
- Text built from fragments that stayed Polish in other languages: weather insights (gusts, sunrise/sunset, rain), Start's day summary, copy/cut toasts, player shortcuts, phone tabs, island subtitles.

### Added
- French UI (`fr` column in `native/assets/i18n.tsv`, *Français* in the language list).
- Weekday initials on the running card listed per language (`i18n::day_initial`; Spanish writes Wednesday as *X*).
- Repository prepared for publishing: README in six languages, GPL-3.0 license, benchmarks
  (`docs/BENCHMARKS.md`, `bench/measure.sh`), publishing checklist, contributing guide, CI, issue templates,
  Hyprland example config (`examples/hyprland/mist.lua`), the `mist-ui` guard in `native/dist/`.
- UI font fallback: Inter Variable, then Noto Sans, when SF Pro is not installed (was a panic).
- UI languages: Polski, English, Deutsch, Español (`native/assets/i18n.tsv`, `mist-ui/src/i18n.rs`).
  Chosen in *Dostosuj pulpit* → Wygląd → Język, saved in `~/.config/mist/settings.json`, `MIST_LANG`
  overrides; *auto* follows `$LANG`, English fallback. Applied when text is shaped, so cached strings cost
  nothing; Polish renders are pixel-identical to before.
- Zen Browser theme (`integrations/zen/`): Mist glass sidebar, menus, popups, panels, dialogs, urlbar and
  about: pages, SF Pro / Inter fonts, prefs in `user.js`, `install.sh` per profile; guide in
  `docs/guides/zen.md` (Polish version).
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
