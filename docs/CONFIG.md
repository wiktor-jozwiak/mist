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
inhibit idle (video players, browsers playing a video, games) hold both off.

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

The experimental compositor's key bindings, autostart and outputs; syntax at the top of
[native/mist-comp/src/config.rs](../native/mist-comp/src/config.rs). `outputs`: one `NAME X Y` per line.
Hyprland users do not need them.

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
`MIST_WALL_FPS` (mist-wall), `MIST_COMP_*` (mist-comp).
