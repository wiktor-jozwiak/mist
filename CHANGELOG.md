# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [SemVer](https://semver.org).

## [0.1.1] - 2026-10-18

### Fixed
- UI translation fixes across language changes: support for uppercase template headings (e.g. `WEEK 42`), recursive sub-template translation (e.g. `2 days ago` in `Last run`), lowercase word lookups (e.g. weather conditions), and middle-dot (` · `) segment boundary isolation.
- Missing translations added: Garmin readiness levels (*prime*, *moderate*, etc.), suggested workouts (*Rest*, *Rest day or 20–30 min walk*, etc.), previous run metrics, and weather comparison deltas.
- UI surfaces (bars, dock, island) marked dirty and refreshed immediately on language change in desktop settings.

### Security
- IPC: 2 s read/write timeout and 64 KiB request cap so one stuck client cannot block the event loop; no `/tmp` socket fallback without `XDG_RUNTIME_DIR`.
- Tray: `IconPixmap` size math in `usize` with icons capped at 1024×1024 px.
- Lock screen: typed password buffer zeroed with volatile writes after PAM/greetd verification.
- Phone backend: screenshot moved from `/tmp` to `$XDG_RUNTIME_DIR`.

### Changed & Performance
- `mist-wall`: live wallpaper pauses while monitors are off or session is locked.
- `mist-ui`: capped memory arena allocations at 2 per process.
- Audio: `pactl` events batched in bursts to eliminate duplicate wakeups.
- Window title updates: fast-path state patch instead of full client parsing.
- View count badge switched to `hits.sh`.
- Star History chart added to READMEs.

## [0.1.0] - 2026-10-16

### Added
- mist-comp: Overview (Mission Control): `overview` action, SUPER+O, three fingers up. The monitor's windows side by side over the dimmed desktop, a click brings one forward, Escape goes back; drawn from the windows' own surfaces, no frames while it stands still.
- mist-comp: the workspace swipe follows the fingers (1:1): the next workspace slides in as they move, lifting past a third (or still moving on) finishes it, a short swipe slides back.
- mist-comp: magnifier (`zoom in|out|off`, SUPER+= / SUPER+-): the screen scaled around the pointer up to 8x; the point under the pointer stays put, so clicks need no mapping.
- mist-comp: windows come back after a compositor crash: start-mist restarts it with `MIST_COMP_RESTORE=1`, which starts each app again and puts its window on its old workspace, place and size (`~/.cache/mist/session.json`).
- mist-comp: watchdog in the log for clients that draw on hidden workspaces (over 20 frames/s) or busy-loop (over 300 commits/s).
- mist-comp: ext-workspace-v1 for other bars (waybar `ext/workspaces`, ironbar): one group per monitor, activate switches.
- mist-ui: high contrast (`"contrast": "high"` in settings.json): secondary text near full opacity, glass panels opaque.
- Packaging: the Arch package installs the Mist session (login manager entry, start-mist finds the packaged mist-comp); Fedora spec in `packaging/fedora` (COPR).
- CI runs every nested mist-comp test on the software renderer and prints compositor and panel benchmarks in the job summary (docs/BENCHMARKS.md has this machine's numbers).

### Changed
- The Zen Browser theme is no longer part of the repository (removed from its history too); a local copy in `integrations/zen/` is ignored by git.
- mist-ui: the Snap Layouts flyout redrawn: the last row centred, cards without frames, zones with even 4 px gaps, the hovered card lifts, the pointer hits a zone anywhere up to the middle of a gap.
- One window shape everywhere: corner radius 12 (mist-comp bars, rounded windows and the window shadow, mist-ui app windows, Zen) from `native/common/tokens.rs`, which also holds the window and bar colours; a test keeps a local Zen theme's tokens.css equal. Windows with a Mist bar get the window shadow too.
- mist-comp: `direct_scanout game` is the default: a fullscreen game's frames go to the primary plane without composition (one frame less latency, no GPU pass); other windows stay composited.
- start-mist: on an NVIDIA-only machine EGL skips Mesa's vendor (gallium + LLVM, ~20 MB in every GL process).

### Performance
- mist-comp announces which desktop layers are hidden under windows (`mistcover` events); mist-wall and mist-ui pause what nobody sees from them, without window queries or the 8 s check (resizes included).
- mist-comp: a window's surfaces share one blur (Zen: half the blur passes); `stats` counts blur runs, IPC `blurs` per surface, `top` commits per client.
- mist-ui: full-monitor panels reuse two buffers and send only the rows that changed (typing in Spotlight: ~6 % of the screen per frame instead of all 8 MB); the shared buffer pool shrinks when a panel closes.
- mist-ui: the weather card's carousel waits while windows cover the card.
- mist-comp: a client's commit redraws only the monitors its window is on; the other monitor stays idle (two monitors with a browser animating on one: half the render passes). Window, cursor and workspace bookkeeping runs at most 10 times a second while only clients commit, not on every commit.
- mist-comp: glass (bar, dock, island, Zen) re-blurs only when pixels behind it change, not whenever a window behind it commits anywhere; a glass window's own redraws damage only what the client drew, not the whole window. A chat streaming text under the dock: 362 → 0 blur runs per 3 s.
- mist-comp: a window fully covered by opaque windows gets a frame callback once a second instead of every refresh: a browser or chat app behind a maximized window stops drawing (its CPU and GPU, and our frames) and answers again as soon as it shows. Glass windows (Zen, Mist Files) and X11 windows never cover others.

### Fixed
- Snapping: a zone's edges are rounded from its fractions instead of origin + floored width, in mist-comp and in the preview alike: 2/3 + 1/3, thirds and 1/4 + 1/2 + 1/4 no longer stop 1 px short of the screen edge, neighbours keep exactly 8 px between them.
- mist-comp: Zen (and every glass window) drew a square grey frame around its rounded corners: Firefox leaves a 24 px margin around the window for GTK's own frame and shadow, which mist-comp showed under its rounded shadow. A glass window's own surfaces are now cut at its geometry (menus and popups are not), and the glass under its antialiased corners fades with the surface's alpha instead of a hard edge.
- mist-comp leaked memory: the GL textures and EGL images of every destroyed client buffer stayed until the session ended (smithay 0.7 frees them only on `cleanup_texture_cache`, which was never called), and a closed panel's blur textures stayed for 600 frames. A live session grew about 4 MB a minute (520 MB after two hours); opening and closing panels now levels off at ~80 MB (test t-texleak).
- Screen sharing and screenshots through the portal in the Mist session: xdg-desktop-portal found no ScreenCast backend for `XDG_CURRENT_DESKTOP=mist` (xdph only lists Hyprland and wlroots desktops); start-mist now installs `mist-portals.conf`.
- Translations: the Start menu agenda ("3 events ahead", "next …"), copy / cut notices, "resume at" in the player, weather insights and "no device" were Polish in every language; a test now fails for any `tr("…")` without a row.
- The screen no longer locks or turns off during a video in apps that inhibit idle over D-Bus (Chromium, VLC, Spotify, Flatpak apps): mist-ui serves `org.freedesktop.ScreenSaver` and `org.freedesktop.PowerManagement.Inhibit` as hypridle did, dropping an app's inhibit when it exits; a fullscreen window while a player plays also holds idle off. `mist-ui-bin --inhibit` keeps the screen on by hand.
- mist-comp: an idle inhibitor whose window died without removing it no longer keeps the screen on forever.
- mist-comp: wp_presentation feedback is sent (presented at the frame's vblank, discarded when nothing changed). It was advertised and never answered, so a client that paces its frames on it stopped drawing after a few frames.
- mist-comp: a frame with nothing new on screen sends its frame callbacks at the next vblank instead of at once, so a browser that commits every frame runs at the refresh rate instead of a busy loop.
- mist-comp: client buffers are no longer put straight on a scanout plane unless `direct_scanout on` (Hyprland's default; on NVIDIA it showed torn or stale windows); the cursor plane stays.
- mist-comp: the log names the NVIDIA driver and egl-wayland versions, an output whose vblank never comes, and a window whose frame waits over a second for its GPU fence; `mist-session log` prints that part of the log.
- mist-ui exits cleanly when the compositor goes away instead of aborting.
- mist-comp: a page flip the kernel refuses (the login screen still held the GPU when the session started) no longer leaves stale, garbled windows: the frame was lost while the damage tracking counted it as shown, so later frames only redrew what changed over it. Now everything is redrawn on the next refresh.
- mist-comp: no 5 s black screen at every login. The DRM master check asked the kernel to make the fd master, which it always refuses for a fd logind opened, so it waited out its timeout and warned "no drm master" while the fd was master all along. It now asks the way libdrm's drmIsMaster does, and while the login screen still holds the GPU it hands the fd back to logind and takes it again (up to 5 s) instead of waiting on a fd that nobody makes master.
- mist-comp: right-click menus and menu bars (xdg popup grabs): the keys go to the open menu and a click outside closes it; they stayed open and the window lost its focus.
- mist-comp: a menu of a window on the second monitor opens next to the pointer; it was kept inside the first monitor, so it showed up on the other screen. Menus of panels (layer-shell popups) are kept on their monitor too.
- mist-comp (real session): windows are told which monitor they are on (wl_surface.enter / leave: their scale, refresh rate and menu placement) and closed menus are dropped; only the nested and headless backends did that.
- mist-comp: drag and drop shows the dragged thing under the pointer (the client's drag icon was never drawn).
- mist-comp: a game that asks for a pointer lock or confinement gets it once the pointer is over its window, and again after Alt+Tab; it worked only when the pointer was already there when the game asked.
- mist-comp: X11 windows with their own frame (Steam, Discord) resize from their edges and move by their title area (_NET_WM_MOVERESIZE); window drags end when the button that started them is released.
- mist-comp: Super + right drag on a window with client-side shadows (GTK) no longer makes it jump by the shadow's size.
- mist-comp: a window that asks to be maximized or fullscreen before its first frame (mpv --fs, a browser that was closed maximized) opens that way.
- mist-comp: the keys go back to the top window (not an X11 menu or tooltip) when a panel, the lock screen or a menu closes, and the window shows as focused while its menu is open.
- mist-comp: no crash on an xdg resize request from a window that is not on screen, or on tablet input with no monitor.
- mist-ui and mist-wall exit with 0 when the compositor is gone, so the guard (dist/mist-ui) no longer restarts them five times at logout and then starts Noctalia with a "Mist crashed" notification.

### mist-comp
- Explicit sync (linux-drm-syncobj-v1) where the GPU supports it; commits wait for the client's GPU work (acquire point, else the dmabuf fence).
- Protocols: text-input-v3 + input-method-v2, virtual-keyboard, tablet-v2, ext-foreign-toplevel-list, ext-data-control, xdg-foreign, xdg-toplevel-icon / -tag, content-type, alpha-modifier, xdg-system-bell, security-context (sandboxed clients get no privileged globals), XWayland keyboard grabs; wlroots foreign-toplevel-management, output-management (wlr-randr, kanshi), output-power-management, virtual-pointer, gamma-control.
- wp-fifo opt-in (`fifo on`): FIFO barriers are released on a real frame (vblank, headless tick, nested redraw), so Vulkan vsync clients run at the refresh rate.
- hyprland-toplevel-export-v1: single-window capture (the dock's window picker thumbnails, window sharing through xdg-desktop-portal-hyprland), also for windows on hidden workspaces.
- hyprland-global-shortcuts-v1 and the `global APP_ID:ID` bind action (press and release); `hyprctl globalshortcuts`.
- wp-commit-timing (with `fifo on`) and xdg-dialog; dialogs open centred on their parent window.
- A bar that appears or grows after windows were placed pushes them below it.
- `stats` counts event-loop wakeups; ci.sh checks the compositor sleeps when idle.
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
