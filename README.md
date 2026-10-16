<div align="center">

# Mist

**A macOS-inspired desktop shell for Hyprland, written in Rust and drawn on the CPU.**

Bar, dock, Dynamic Island, desktop widgets, Spotlight-style launcher, control centre, notifications,
lock screen and polkit agent in one ~50 MB process, without a GPU context.

[English](README.md) · [Polski](docs/i18n/README.pl.md) · [Deutsch](docs/i18n/README.de.md) ·
[Español](docs/i18n/README.es.md) · [简体中文](docs/i18n/README.zh-CN.md) · [日本語](docs/i18n/README.ja.md)

![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)
![Status](https://img.shields.io/badge/status-personal%20daily%20driver-lightgrey)

![Mist desktop](docs/img/desktop-en.webp)

<img src="docs/img/island.gif" width="49%" alt="Dynamic Island: a notification, the volume, rain soon, the next event"> <img src="docs/img/weather.gif" width="49%" alt="The weather card in the rain">
<img src="docs/img/wallpaper.gif" width="98.5%" alt="A live wallpaper (dunes)">

<sub>Offscreen render (`mist-ui --desk-png`, `--dock-png`, `--png`, `mist-wall-bin --png`) with the demo data in `docs/demo`, rebuilt by [docs/img/hero.sh](docs/img/hero.sh) and [docs/img/gifs.sh](docs/img/gifs.sh).
On a real session Hyprland blurs under the glass.</sub>

</div>

## Why

Most Hyprland shells run on Qt Quick (Quickshell), GTK + GJS (AGS) or a WebView. Each of them opens an
OpenGL context, and on NVIDIA every GL context maps the driver: about **100 to 160 MB per process** before the
first pixel. Mist started as a Quickshell config (428 MB, 9.5 % CPU) and was rewritten surface by surface:

- **CPU-drawn surfaces.** `mist-ui` paints into `wl_shm` buffers with [tiny-skia] and [swash]; Hyprland
  composites and blurs them. Only damaged regions are redrawn.
- **Event-driven.** Data comes from the Hyprland socket, D-Bus, `pactl subscribe`, inotify and long-lived
  `mistd` backends. Nothing polls on a timer while the screen is idle; hidden surfaces do no work.
- **One GPU process, only when needed.** `mist-wall` (EGL, GLSL 150) draws animated wallpapers and the
  weather sky. With a photo wallpaper it exits and `mist-ui` draws the photo itself.

## Benchmarks

Idle, whole shell, proportional set size (shared pages split between processes), Ryzen 7 5700X + RTX 4060
(driver 615.71), Hyprland 0.56.2, 2560x1440 + 1920x1080. Measured with [bench/measure.sh](bench/measure.sh).

| Shell | Toolkit | PSS | CPU idle | GL context |
|---|---|---|---|---|
| **Mist native** | Rust, CPU, `wl_shm` | **87 to 106 MB** | 0.6 % (music on) | `mist-wall` only |
| Mist native, `mist-ui` alone | | 36 to 50 MB | 0.6 % | no |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0.07 % | yes |
| Empty Quickshell (one `PanelWindow`) | Qt Quick | 124 MB | ~0 % | yes |
| Mist on Quickshell (old QML version) | Qt Quick + QML | 428 MB | 9.5 % | yes |

Numbers reported by others (RSS, different machines, not comparable 1:1): Waybar 50 to 66 MB
([Waybar#5186](https://github.com/Alexays/waybar/issues/5186)), Caelestia ~650 MB after a restart
([caelestia-dots/shell#1927](https://github.com/caelestia-dots/shell/issues/1927)). Method, raw output and
caveats: [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Features

| | |
|---|---|
| **Top bar** | active app and title, system tray (StatusNotifierItem host + dbusmenu), network, Bluetooth, audio, CPU / memory meters, clock, calendar popup |
| **Dock** | pinned + running apps, magnification, window picker with live thumbnails (`hyprland_toplevel_export_v1`), click on the active app minimises it |
| **Dynamic Island** | media (MPRIS) with equalizer, timers, calls (KDE Connect / ADB), notifications, volume |
| **Desktop** | clock, sunrise / sunset, glass cards (weather, Garmin readiness, running plan), per-widget settings sheet |
| **Panels** | Spotlight-style search, Start, Cmd+Tab switcher, control centre (paged), notification centre, clipboard history, emoji picker, session menu, wallpaper gallery, screenshot toolbar |
| **System** | notification server, polkit agent, lock screen (`ext-session-lock-v1` + PAM), idle (`ext-idle-notify-v1`, logind), Windows 11-style snap assist |
| **Wallpapers** | photos (CPU) or live GLSL scenes and a real-time weather sky (`mist-wall`) |
| **mist-comp** *(experimental)* | own Smithay compositor + greeter, for running Mist without Hyprland |

### Languages

The interface speaks **Polski, English, Deutsch and Español**. Pick one in *Customise desktop* (right click on
the desktop clock) → Appearance → Language, or set `MIST_LANG=en`. *Automatic* follows `$LANG` and falls back
to English. Translations live in one table, [native/assets/i18n.tsv](native/assets/i18n.tsv): one row per
string, one column per language. A new language is one more column.

The dock, island, desktop and weather sky go on Hyprland's first monitor. Another one: `"primary": "DP-3"` in
`~/.config/mist/settings.json`, or `MIST_PRIMARY=DP-3`. The weather place is set in *Customise desktop* → General.

### Desktop cards

- **Weather**: [Open-Meteo](https://open-meteo.com), no account. Hourly curve, 4 days, rain nowcast, running
  window, clothing tip, sun and moon; the live sky behind the card follows the current conditions.
- **Garmin readiness** and **running**: your own Garmin Connect data through a local InfluxDB.
  Setup: [docs/guides/garmin.md](docs/guides/garmin.md) ([Polski](docs/guides/garmin.pl.md)).

## Architecture

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust, CPU (tiny-skia + swash), wl_shm, one process for every surface
                           │      ├── mistd stats | phone | calls | island | clipstore   (event backends)
                           │      └── IPC: mist-ui-bin msg <target> <function> [args]
                           └── mist-wall    Rust, EGL / GLSL 150, only for live wallpapers and the sky
```

| Path | What |
|---|---|
| `native/mist-ui` | all shell surfaces ([native/README.md](native/README.md)) |
| `native/mist-wall` | GPU wallpaper process |
| `native/mist-comp` | experimental Smithay compositor, session and greeter |
| `native/common` | code shared by mist-ui and mist-wall (astronomy, presets) |
| `scripts/mistd` | backends: system stats, phone (KDE Connect, ADB), calls, clipboard store, file manager D-Bus |
| `shaders/` | wallpaper and sky shaders (`.frag` sources, GLSL 150 in `shaders/glsl` read by mist-wall) |
| `*.qml`, `components/` | the original Quickshell version, kept as reference; no longer started |

## Install

Mist is a personal daily driver: it targets Arch / CachyOS, Hyprland 0.55+ with the Lua config, and an
NVIDIA or Mesa GPU. Expect to read code.

**Build dependencies:** `rust` (stable), `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa` (EGL headers);
for `mist-comp` also `libinput`, `libseat`, `libdrm`, `mesa` (gbm), `systemd-libs` (udev).

**Runtime:** `hyprland`, `pipewire` + `pipewire-pulse` (`pactl`, `pw-play`, `parec`), `wireplumber` (`wpctl`),
`networkmanager` (`nmcli`), `wl-clipboard`, `grim`, `slurp`, `polkit`, `noto-fonts`, `noto-fonts-emoji`. Optional: `ddcutil` (brightness over DDC/CI),
`kdeconnect` / `android-tools` (phone), `wf-recorder` (screen recording).

**Font:** the UI is designed for SF Pro (variable `SF-Pro.ttf` in `~/.local/share/fonts/SF-Pro/`), which
Apple licenses for its platforms only and Mist does not ship. Without it Mist uses Inter Variable, then
Noto Sans.

**Arch (AUR-style package):** [packaging/arch/PKGBUILD](packaging/arch/PKGBUILD) builds `mist-shell-git`
(binaries in `/usr/bin`, files in `/usr/share/mist`).

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

**From a checkout** (anywhere; `native/install.sh` links `~/.local/share/mist/shell` to it):

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

`native/install.sh` installs `mist-ui-bin`, `mist-wall-bin`, `mist-comp` and, on the first run, the guard
`mist-ui` into `~/.local/bin`. Tagged releases also carry prebuilt binaries.

Then add [examples/hyprland/mist.lua](examples/hyprland/mist.lua) to your Hyprland config (autostart,
key bindings, blur rules) and remove other bars, notification daemons and polkit agents.

## Use

| Key | Action |
|---|---|
| `Super` | Start |
| `Super + Space` | Spotlight |
| `Super + Tab` | app switcher |
| `Super + I` | control centre |
| `Super + A` | notification centre |
| `Super + V` | clipboard history |
| `Super + .` | emoji |
| `Super + L` | lock |
| `Print`, `Super + Shift + S` | screenshot (region) |

Everything is scriptable: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`,
`mist-ui-bin msg dock click firefox`. Log: `~/.cache/mist/ui.log`. Settings: `~/.config/mist/`.

## Develop

```bash
cd native && cargo build --release && cargo test --release
```

Checks without touching the running session:

| Command | Output |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | the top bar as PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | desktop stage + cards / dock |
| `mist-ui --test` | a second bar over the real one on HDMI-A-1 |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | a live wallpaper frame |
| `mist-ui-bin msg ui stats` | wake-ups, paints and paint time per surface |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | idle PSS / CPU of the running shell |

Keep `lto = false` (Arch rustc with system LLVM crashes with LTO). See [CONTRIBUTING.md](CONTRIBUTING.md).

## Status

Daily driver of one person, two monitors, NVIDIA. Everything in the table above is native and used every
day; `mist-comp` is experimental. See [docs/PUBLISHING.md](docs/PUBLISHING.md) for what is missing before a
public release (configurable paths, no Apple assets).

## Credits

[Hyprland], [Smithay] and [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus], [Tabler Icons]
(MIT). The original QML version was built on [Quickshell]; [Noctalia] served as fallback and reference.

*Mist is not affiliated with Apple. macOS, Dynamic Island, Spotlight and SF Pro are trademarks of Apple Inc.;
they are named here only to describe the design inspiration.*

## License

[GPL-3.0](LICENSE).

[tiny-skia]: https://github.com/linebender/tiny-skia
[swash]: https://github.com/dfrg/swash
[resvg]: https://github.com/linebender/resvg
[zbus]: https://github.com/dbus2/zbus
[Hyprland]: https://hyprland.org
[Smithay]: https://github.com/Smithay/smithay
[smithay-client-toolkit]: https://github.com/Smithay/client-toolkit
[Tabler Icons]: https://tabler.io/icons
[Quickshell]: https://quickshell.org
[Noctalia]: https://github.com/noctalia-dev/noctalia
