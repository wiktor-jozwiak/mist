# Configuration

Everything lives in `~/.config/mist/`. Every file is optional: Mist starts with sane defaults and writes most of
them itself when you change something in the UI (*Customise desktop*, the wallpaper gallery, the dock). Edit
them by hand only for what the UI does not offer; `mist-ui` re-reads them on its own or after a restart
(`pkill -x mist-ui-bin`, the guard `mist-ui` starts it again).

## Files

### `settings.json`

```json
{ "lang": "auto", "primary": "DP-3" }
```

| Key | Values | |
|---|---|---|
| `lang` | `auto`, `pl`, `en`, `de`, `es` | UI language; `auto` follows `$LANG`, falls back to English. Set in *Customise desktop* → Appearance |
| `primary` | a Hyprland monitor name | where the dock, island, desktop and weather sky go; default: Hyprland's first monitor |
| `contrast` | `high` | accessibility: secondary text near full opacity, glass panels opaque (read at start; `MIST_CONTRAST=high` overrides) |

### `widgets.json`: the desktop

Written by *Customise desktop* (right click on the desktop clock).

| Key | Values |
|---|---|
| `order` | the three cards, left to right: `["garmin", "weather", "running"]` |
| `desk.cards` | how many places the row shows, `1` to `3` |
| `desk.clock` | `big`, `small`, `hide` |
| `desk.date`, `desk.sun` | `show`, `hide` (the date, the sunrise / sunset path) |
| `desk.glass` | card background: `light`, `normal`, `dark` |
| `weather.hero` | the big number: `temp`, `feels`, `rain`, `wind` |
| `weather.metric` | the hourly chart: `temp`, `rain`, `wind` |
| `weather.days` | 4-day forecast: `show`, `hide` |
| `garmin.hero` | `ready`, `bb` (Body Battery), `sleep`, `hrv` |
| `garmin.vitals` | up to 4 of `sleep`, `hrv`, `rhr`, `stress`, `steps`, `bb`, `ready` |
| `garmin.foot` | `insight`, `hide` (only on hover) |
| `running.hero` | `race` (days to the race), `week`, `year` |
| `running.show` | any of `strip`, `workout`, `p5` (5 km prediction) |
| `running.foot` | `last` (the last run), `week`, `year`, `hide` |

The Garmin and running cards need their data source (below); without it they are not shown and the other
cards close up.

### `location.json`: the weather place

```json
{ "city": "Kraków", "lat": 50.06, "lon": 19.94 }
```

Chosen in *Customise desktop* → General (search through Open-Meteo geocoding). Without it the weather card
asks for a place and nothing is fetched.

### `wallpaper.json`

| Key | Values |
|---|---|
| `image` | a photo (absolute path); drawn by `mist-ui` on the CPU |
| `live` | a live preset id from [native/assets/live-presets.js](../native/assets/live-presets.js) (`dunes`, `day`, `aurora`, ...); empty = the photo. Starts `mist-wall` |
| `tempo` | animation speed: `calm`, `normal`, `fast` |
| `music` | `true`: presets that react to music follow the playing audio |

Photos for the gallery: `$MIST_WALLPAPERS`, else `Pictures/Wallpapers` (`Pictures/Tapety` with a Polish
`$LANG`), from `~/.config/user-dirs.dirs`.

### `apps.json`: dock and Start

```json
{ "dock": ["mist-files", "firefox", "kitty"], "groups": [{ "name": "Internet", "apps": ["firefox"] }] }
```

Desktop entry ids (the `.desktop` file name without the extension). The dock writes `dock` when you pin,
unpin or drag an app; `groups` are the pinned groups of Start (12 apps at most).

### `idle.json`

```json
{ "lock": 600, "off": 900 }
```

Seconds without input before the session locks and before the monitors turn off; `0` = never. Apps that
inhibit idle hold both off, through Wayland idle-inhibit (Firefox, mpv, the Mist player) or D-Bus
`org.freedesktop.ScreenSaver` / `PowerManagement.Inhibit` (Chromium, VLC, Spotify, Flatpak apps; Mist serves
these, as hypridle did). A fullscreen window while a player plays counts too. When the hold ends, both timers
start over. `mist-ui-bin --inhibit` keeps the screen on by hand until stopped (Ctrl+C).

### `feeds.json`: calendars

A list of iCal URLs (`.ics`, e.g. the *secret address* of a Google calendar). Shown in the calendar sheet and as
the island's next event. Treat this file as a secret.

### `influx.json`: Garmin and running (optional)

```json
{ "url": "http://127.0.0.1:8086/query", "db": "GarminStats", "user": "...", "password": "..." }
```

The InfluxDB 1.x of [garmin-grafana](https://github.com/arpanghosh8453/garmin-grafana). Setup:
[guides/garmin.md](guides/garmin.md). Without this file the Garmin and running cards are off and nothing is
asked.

### Written by Mist

`usage.json` (app launch counts, for the Spotlight and Start ranking), `files.json` (Mist Files view), `player.json` (Mist
Player recents and volume). Safe to delete.

### `comp.conf`, `outputs`: mist-comp only

The experimental compositor's key bindings, autostart, monitors and switches; full syntax at the top of
[native/mist-comp/src/config.rs](../native/mist-comp/src/config.rs). Hyprland users do not need them.

| Line | |
|---|---|
| `bind` / `bindr` / `bindl COMBO ACTION` | key binding / on release / also while locked |
| `bind COMBO global APP_ID:ID` | a shortcut an app registered through the portal (Discord push-to-talk): pressed and released with the key; `hyprctl globalshortcuts` lists them |
| `output NAME X Y [scale S] [mode WxH[@Hz]] [transform T]` | a monitor (also in `outputs`, where comp.conf wins); applied at hotplug and on `reload` |
| `exec CMD`, `env K V` | autostart, environment |
| `keyboard LAYOUT[,LAYOUT2] [VARIANT] [OPTIONS]`, `numlock on`, `pointer flat` | input |
| `swipe 3\|4\|off` | touchpad: N fingers (default 3) sideways move to the next / previous workspace, which follows the fingers (lifting past a third finishes it, a short swipe goes back); up opens the overview, down closes it |
| `vrr fullscreen\|off`, `swallow on`, `middle_click_paste off` | as Hyprland's misc options |
| `explicit_sync off` | no linux-drm-syncobj (default: on where the GPU supports it); try it when a window freezes and the log says a frame waits for its GPU fence |
| `direct_scanout game\|on\|off` | a client's buffer goes straight to the screen (Hyprland `render:direct_scanout`). `game` (default): a fullscreen game's, primary plane only; `on`: any window, overlay planes too (on NVIDIA that showed torn or stale windows); `off`: everything composited |
| `screencopy_dmabuf on` | screen casting into the client's GPU buffer (opt-in) |
| `fifo on` | wp-fifo + wp-commit-timing: Vulkan FIFO (vsync) and Proton-paced clients are paced by the display refresh instead of frame callbacks (opt-in; off in safe mode) |

Actions worth a bind besides Hyprland's: `overview` (Mission Control, SUPER+O by default: the monitor's windows
side by side, a click picks one, Escape goes back), `zoom in|out|off` (the magnifier around the pointer,
SUPER+= / SUPER+-, 1.25x a step up to 8x; screenshots and screen sharing stay unmagnified).

`mist-comp migrate [~/.config/hypr/hyprland.lua]` prints comp.conf lines for the binds (exec and common
dispatchers), monitors, `hl.env` and autostart it understands; the rest is marked `# not converted:`.
Monitors can also be set at run time with wlr-randr / kanshi (wlr-output-management).

`start-mist` restarts a crashed compositor in safe mode (`MIST_COMP_SAFE=1`: no blur, animations, VRR,
explicit sync or dmabuf casting), at most three times; the log is `~/.cache/mist/comp.log`. Without Hyprland
installed it links `hyprctl` to mist-comp, which answers the requests scripts make. After a crash it starts
the windows that were open again (`~/.cache/mist/session.json`, written 2 s after windows change; the apps
start fresh, each window back on its workspace, place and size). It also copies `mist-portals.conf` to
`~/.config/xdg-desktop-portal/`, so screen sharing and screenshots in browsers go through
xdg-desktop-portal-hyprland, and file dialogs are Mist Files windows.

mist-comp also speaks ext-workspace-v1 (waybar `ext/workspaces` and other bars) and logs
`watchdog: <app> draws N frames/s while hidden` when a client keeps drawing on a hidden workspace.

## Optional integrations

| What | Needs | Off when |
|---|---|---|
| Garmin readiness + running cards | `influx.json` and garmin-grafana | the file is missing |
| Phone (calls, SMS, battery, find my phone) | KDE Connect paired with the phone | no paired device |
| Calls through ADB (phones where KDE Connect telephony stays silent, e.g. HyperOS) | `adb` installed, the phone authorised (USB, or Wi-Fi after `adb tcpip`) | `adb` missing or no authorised device |
| Brightness keys on external monitors (`scripts/bright.sh`) | `ddcutil`, DDC/CI on | `ddcutil` missing |

## Environment

| Variable | |
|---|---|
| `MIST_LANG` | UI language, overrides `settings.json` |
| `MIST_PRIMARY` | main monitor, overrides `settings.json` |
| `MIST_DIR` | where the Mist files are (default: `~/.local/share/mist/shell`, `/usr/share/mist`) |
| `MIST_WALLPAPERS` | the photo folder of the gallery |
| `MIST_FONT_DIR` | a folder with `SF-Pro.ttf` / `tabler-icons.ttf` (the greeter, which cannot read your home) |
| `MIST_UI_NO_BAR`, `_NO_DOCK`, `_NO_ISLAND`, `_NO_DESK`, `_NO_WALL` | leave that surface out |
| `MIST_UI_NO_SERVICES` | no clipboard watcher and other background helpers (tests) |
| `MIST_UI_SOCK` | the IPC socket of `mist-ui-bin msg` (a second instance for tests) |

Development only: `MIST_I18N_MISS=file` (appends every string drawn without a translation),
`MIST_EDIT`, `MIST_SCROLL`, `MIST_BENCH` (offscreen renders), `MIST_WALL_BENCH`, `MIST_WALL_FRAMES`,
`MIST_WALL_FPS` (mist-wall), `MIST_COMP_*` (mist-comp: `MIST_COMP_WATCHDOG_SECS` (60), `MIST_COMP_NO_SESSION`,
`MIST_COMP_SESSION_FILE`, `MIST_COMP_RESTORE=1` (start what the session file lists), `MIST_COMP_ANIM=0`).
