# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [SemVer](https://semver.org).

## [Unreleased]

### Fixed
- Weather card: an hour or a day picked with a click goes back to "now" 2.5 s after the pointer leaves the
  card, as a hovered one does; it stayed 20 s and looked stuck. The Sun's and the Moon's details still stay 20 s.
- Arch package: the description names the shell and the compositor, Hyprland is optional (mist-comp and
  `mist-try` run without it), the version reads `0.4.0.r<commits>.g<hash>` instead of `r<commits>`.

## [0.4.0] - 2026-11-14

### Added
- README redesigned: a header with the logo and the numbers that matter (idle memory, OpenGL contexts,
  wake-ups), a tour of even 16:10 pictures (`docs/img/panels.sh` frames every panel and app window the same
  way, `docs/img/gifs.sh` writes animated WebP instead of GIF, a tenth of the size), the seven themes side by
  side, the app icons, the benchmark tables folded, star history. The translations follow the same layout;
  `bench/i18n/gen.py` takes the badges from the English README.
- Icons of their own for Mist Files, Mist Player and Settings (`native/assets/icons/make.py` writes the SVGs:
  the macOS icon grid, Mist's dusk colours, a glass rim). `native/install.sh` and the Arch and Fedora packages
  install them with launchers for all three apps; Settings had none and did not show in Start.
- Desktop: three new widgets for the bento row. Notatka shows `~/.config/mist/note.txt` (`# ` a heading, `- ` a
  point, `- [ ]` and `- [x]` a to-do list with the count of what is left; a click opens the file in the system's
  editor; small, standard or large text, glass or paper). Czas shows how much of the day, the week, the month
  and the year has passed and counts down to the next calendar event, the weekend or the New Year. Słońce i
  Księżyc draws the day's arc with the Sun on it and the Moon's disc and phase (both, the Sun alone or the Moon
  alone). None of them starts a thread or polls: they are drawn with the stage's paint.
- Desktop: two more widgets. Zdjęcie is a picture frame: a photo of the wallpaper folders, another one every day
  or every hour, or the file `~/.config/mist/photo.jpg` (.jpeg, .png); fill or fit, an optional caption. Decoded
  once per pick. Urządzenia shows the batteries the bar already knows (laptop, phone, a Bluetooth device) and
  redraws only when one of them changes.
- Desktop clock: 12 or 24 hours, a short date ("pt 13.11"), three opacities, blue and pink. Zegary: face (ticks,
  numbers, plain), colour of the accents, 12 or 24 hours.
- Lock screen: Ustawienia > Ekran blokady > Zegar sets the clock's size (big, small, hidden), weight, colour, the
  date and 12 or 24 hours. `mist-ui --lock-clock-png file` renders it for checks.
- Settings > Pulpit is simpler: one row picks the widgets (a click adds or takes one off, three at most), the ones
  shown follow as rows that open their settings; Liczba kart and the three Miejsce rows are gone. The rest is
  four short cards: Wygląd kart, Zegar, Wygląd zegara.
- Weather sky: rain no longer stutters. A streak fell 1.5 to 2.5 card heights a second, its whole length between
  two frames at 20 fps; it now falls 0.85 to 1.45, is fainter and sparser, and the sky draws 30 fps while rain or
  snow falls and 60 fps while the shown moment travels. Pogoda > Deszcz i śnieg: off, soft (the default), full.
- Settings > Pogoda in three short cards (Karta, Niebo, Stopka), every row saying what it changes.
- Lock screen: date format, the clock at the top or nearer the middle, wallpaper blur (after waking, always,
  never), dimming, the user picture shown or hidden.
- Screenshots: a region is picked over the frozen screen. The monitor is captured first and shown still under
  the selection (a video or an open menu holds while it is framed), the box shows its size, Esc cancels; mist-ui
  draws the picker itself (slurp and hyprpicker, which mist-comp cannot serve, are no longer used for regions).
- Weather card, Sun view: the day's arc is gone; the next five days stand in columns with sunrise, sunset and the
  length of the day, as the Moon's view shows its phases.
- Translations: seven cities of Zegary (Londyn, Nowy Jork, Tokio…) were Polish in every language.
- Desktop settings: Poświata kart (how strongly the cards glow in their colour: off, soft, standard, strong);
  Zegary knows eight more cities (Warszawa, Chicago, São Paulo, Reykjavik, Moskwa, Bangkok, Singapur, Seul); the
  weather card's Sun and Moon chips can be hidden.
- Weather sky, new scenes for weather that had none: blizzard (gusts of white, snow running over the ground),
  freezing rain (icicles on the glass, the plain turned to ice), gale (twigs and dust torn along, trees
  thrashing), deep frost under a clear sky (diamond dust, sun dogs, a pillar of light), puddles mirroring the sky
  after rain, ground fog over the fields on calm damp nights and dawns.
- Weather card: five landscapes under the sky, picked in the card's settings (Landscape): fields, a lake that
  mirrors the sky and the far shore and freezes over under snow, mountains with snow caps and alpenglow, a city
  skyline with lit windows and their light on the water, the sea with swell, foam and a lighthouse on a headland.
  Water carries a path of glitter under the Sun or the Moon.
- Weather card: the sky follows the hour or the day pointed at on the timeline (the Sun and the Moon where they
  will stand, that hour's weather), gliding there in 0.7 s and back when the pointer leaves.
- `sky-scenes.py <out> lands` draws every landscape through one day (`lands.png`); `MIST_SKY_LAND=0..4` picks the
  landscape for the weather cases.
- `native/mist-wall/tests/sky-scenes.py` draws the sky of 31 weather cases offscreen into a contact sheet;
  `MIST_SKY_NOW` and `MIST_SKY_T` set the moment and the sky's clock of `mist-wall --png out.ppm sky`.
- After a panic the restarted shell says so on the island, with the thread and the source line; every Mist
  binary appends its panics to `~/.cache/mist/crash.log` (time, binary, version, thread, place, message).
- A file in `~/.config/mist` that is not valid JSON is named on the island with the line and column, at start
  and whenever it is saved. Before, its readers fell back to defaults without a word.
- `settings.json` with a theme, accent, language or contrast that Mist does not know (a typo in a theme's
  name, say) is named on the island with the field and the value.
- The island's notice about a broken configuration file goes away by itself when the file is saved correct.
- mist-comp sends `mistlock>>CAPS,NUM` on its event socket when Caps Lock or Num Lock changes; the island's
  indicator follows it instead of reading the keyboard LEDs four times a second.
- `bench/quiet.sh`: a benchmark series waits for a quiet machine before every run; `bench/sample.py` records
  `pss_anon`, the part of PSS that does not depend on what else is running.
- `native/mist-ui/tests/i18n-miss.sh` draws every pane, Settings page, Mist Player view, Mist Files and the bar
  offscreen and lists the strings that reached the screen without a translation.
- CI fails on unused code (`-D unused`) next to the clippy groups it already denied; the workspace builds
  without a compiler warning.

### Changed
- Weather card: pointing at the Sun or the Moon (in the sky or on its chip) no longer opens a popup: the card
  itself turns to that body, in the weather's layout, and back when the pointer leaves (a click keeps it, a click
  anywhere else or 20 s away brings the weather back). The Sun's card leads with the next sunset or sunrise and
  the time left, draws the day's arc over the horizon with the Sun on it (under the line at night) and says how
  long the day is and how tomorrow's compares; the Moon's shows the disc, the phase, how much is lit, the Moon of
  the next ten days and its distance. Both give rise, set and altitude, for the hour pinned on the timeline.
- Weather card: the Sun and the Moon cross the sky through the day and the night. The view now faces the equator
  and holds still, so the Sun rises on the left, stands in the middle at noon and sets on the right (mirrored in
  the southern hemisphere); before, the view turned with the Sun, which stayed in the middle of the card.
- Weather card: a row of the moment's numbers under the headline (rain chance and millimetres, wind with its
  direction, humidity, UV; cloud cover for an hour, hours of sun for a day) is always shown and replaces the
  hover-only chips. Its first three are the timeline's switch: click one or turn the wheel over the row. The
  timeline marks sunrise, sunset and "now", the next days show their temperature span on a common scale, the
  wheel over the footer walks the insights, a click on the big number picks what the card leads with.
- Weather card: the sun and the moon have chips at the end of the numbers' row (the next sunrise or sunset, how
  much of the Moon is lit). Hovering one opens the same details as the body in the sky, which cloud or fog often
  hides; a click on a chip or a body keeps them open. While the card is hovered a faint ring marks both bodies.
- Weather sky, every scene: far hills behind the treeline, fields in perspective with strips of crops, the land
  lit by the night sky and the Moon instead of black, village lights at night, warm light of a low sun, a pink
  belt opposite it, the Milky Way, a larger sun and moon, the sun as a pale coin through fog and overcast, a
  taller two-curtain aurora, a branching bolt and sheet lightning in a darker storm deck, dithered gradients.
- Weather card: a hover repaint costs 4.3 ms instead of 10.7 ms (the glass is painted once and copied, the
  text's shadow is blurred at half resolution); the insights are no longer worked out every second.
- A photo wallpaper is drawn by `mist-ui`; `mist-wall` (the only OpenGL process) starts on demand for a live
  wallpaper, the weather card's sky and the wallpaper gallery, and exits when none is shown.
- The idle shell wakes the CPU 38 times a second with a live wallpaper (110 before) and 8 times with a photo.
- The system statistics sampler sleeps while the screens are off.
- Spotlight's time-zone answers start one `date` process instead of three.
- `blocking` is pinned to 1.6.2: in 1.7.0 idle pool threads never exit and wake twice a second.

### Fixed
- Spotlight and Start: the search field has a caret. Left and Right move it (Ctrl a word), Home and End, Shift
  selects, Ctrl+A selects everything and typing replaces it; before, the arrows did nothing and Ctrl+A only
  marked the whole text.
- Mist Files: "zaznaczono N" in the status bar was Polish in every language.
- `MIST_LANG` is kept when the language setting is read again (it fell back to `settings.json`).
- `docs/img/panels.sh`: Settings is a window now (`settings --png`); Start, Files and Player shots added, the
  app windows rendered from a demo home.
- Start's recent files were looked for in `~/Pobrane`, `~/Obrazy`, `~/Dokumenty`, `~/Pulpit` and `~/Wideo`
  only; the folders now come from the system's user directories.
- Mist Player saved screenshots to `~/Obrazy` whatever the system's Pictures folder was called, and preferred
  Polish audio and subtitle tracks in every language; both now follow the system and the UI language.
- 21 strings of Mist Player's help and statistics, the desktop cards' headers and Settings' shortcut names
  were shown in Polish in every language.

## [0.3.0] - 2026-11-09

### Added
- Theme, accent and contrast changes repaint Mist Files, Mist Player and Mist Settings live, without reopening them.
- The seven newest live wallpapers have names and descriptions in every language.
- mist-comp: hot corners (`corner tl overview`, Settings → Multitasking), `pin` (always on top, on every
  workspace, Super+Ctrl+T), a dropdown terminal (`dropdown`, Super+Ctrl+Return), saved window layouts
  (`layout save|load NAME`, Spotlight "layout …"); `hyprctl dispatch` takes any comp.conf action.
- Shortcut cheat sheet (Super+/): every key binding in words, grouped, numbered ones folded.
- Battery: level in the bar, island notices (charger, full, low at 20 / 10 / 5 %), charge limit 80 % in
  Settings → Power.
- Do Not Disturb by itself: quiet hours, over a fullscreen window, while the screen is shared or recorded.
- Screen recording (Super+Shift+R, wf-recorder, a red dot on the island), text from the screen (Super+Shift+T,
  tesseract), the colour picker shows the colour on the island.
- Spotlight: units, currencies (ECB rates, fetched once a day) and time zones ("22:00 tokio"), open windows.
- Workspace names in the bar; the accent colour can follow the wallpaper photo.
- Send files (Mist Files menu) and the clipboard (Spotlight) to the phone through KDE Connect.

### Changed
- Settings: one window instead of two. The Ustawienia panel is gone; Mist Settings (`mist-ui-bin settings`)
  now has Personalization → Desktop (live preview, cards, clock), Widgets with a page per widget, and the
  weather place search. Right click on the desktop, Start, Spotlight and the control centre open it.
- Mist Settings: the window is translucent over mist-comp's blur (opaque with high contrast), a scroll bar
  that can be dragged or clicked, 60 px per mouse wheel notch.

- Clipboard history: Enter pastes the item straight into the window below (mist-comp types Ctrl+V, Ctrl+Shift+V
  in terminals), Ctrl+Enter only copies, Ctrl+P pins an item (pinned ones stay above the newest 80); a
  password manager's copy is skipped without an extra `wl-paste` call on wl-clipboard 2.2+.

### Fixed
- English UI: the weather card's day label, the phone panel's quick actions, the wallpaper category, time zone
  and ECB rate answers and the notification count were left in Polish. Numbers use a decimal point in English
  and a comma in the other languages; unit answers show two decimals instead of six.
- Settings: the main monitor can be chosen, the lock screen page sets when the screen turns off, restarting
  the shell no longer closes Mist Settings and Mist Files.

## [0.2.0] - 2026-10-23

### Added
- Settings: one window for language, weather place, wallpaper, desktop layout (a live preview, click a card to
  swap it), widgets, night light, high contrast, lock and sleep, notifications. Right click on the desktop,
  Start, Spotlight or the control centre open it; it opens by itself on the first start.
- System settings app (`mist-ui-bin settings`): display, sound, Bluetooth, power, mouse and touchpad, apps,
  accounts, date and time, language and keyboard, accent colour; CPU drawn, nothing polled while idle.
- Desktop cards: music (the player the island follows), calendar, system (CPU, memory, disk, network) and world
  clocks, next to weather, Garmin readiness and running.
- Live wallpapers: aurora over a mountain lake, liquid chrome and gold, neon city in the rain, ringed planet,
  layered waves.
- mist-comp: its own session (`install-session.sh`, greeter, `mist-session`), restart in safe mode after a
  crash with the windows restored, `mist-session restart` without logging out (`session/restart-check` guards it).
- mist-comp: ext-background-effect-v1, so a full-screen panel blurs only under its cards; glass re-blurs only
  the damaged part; Win + D shows the desktop; Spotify and other apps that cannot move themselves are dragged
  by their top strip.
- mist-comp: Windows 11 snapping with exact zone edges, Overview, magnifier, 1:1 workspace swipe,
  wlr-output-management, screencopy, foreign-toplevel, explicit sync, input methods, Hyprland toplevel export
  and global shortcuts. Its default config is `native/assets/comp-default.conf`.
- Lock screen on every monitor; French UI.

### Changed
- Garmin: `mistd garmin sync` talks to Garmin Connect itself and keeps the data in SQLite
  (`~/.local/share/mist/garmin.db`, 0600) instead of InfluxDB. No container, server or password stays behind;
  garmin-grafana is needed once, for the first login. A database from the first builds in `~/.config/mist`
  is moved on first use. Setup: [docs/guides/garmin.md](docs/guides/garmin.md).
- Translations moved from `native/assets/i18n.tsv` to one JSON file per language in `native/assets/i18n/`,
  with user overrides in `~/.config/mist/i18n/<lang>.json`.
- The dock keeps each app where it was when focus or minimizing reorders the compositor's window list.
- READMEs rewritten in every language: shorter, with the Settings window.

### Fixed
- Garmin steps: Connect's 15-minute intervals carry no time zone and were all stored at noon, overwriting
  each other (today's steps read 0).
- mist-comp: a window moved onto another monitor with `hl.dsp.window.move` joins the workspace shown there,
  as after a drag; it kept the old monitor's workspace and ignored both monitors' switches.
- mist-comp tests: the stale `xdg_toplevel_icon_manager_v1` check (the global is off on purpose: Smithay 0.7
  kills Electron clients that drop the icon buffer) and waits for workspace switches on slow CI runners.

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
