# Mist native

A cargo workspace with every process of the shell:

| Crate | What |
|---|---|
| `mist-ui` | all shell surfaces, drawn on the CPU into `wl_shm` buffers ([tiny-skia], [swash]): no GPU context in the process, so no NVIDIA driver mapping (~100 to 160 MB per process) and no Qt Quick |
| `mist-wall` | the only GPU process: live GLSL wallpapers and the weather sky (EGL, GL 3.3 core); exits with a photo wallpaper |
| `mist-comp` | experimental Smithay compositor, session and greeter |
| `common` | astronomy, sky and preset code shared by `mist-ui` and `mist-wall`; `paths.rs` finds the Mist files |

Data comes from event sources only: the Hyprland socket, D-Bus, `pactl subscribe`, inotify and the
`mistd` backends in `scripts/mistd`. Hidden surfaces do no work.

## mist-ui modules

| Area | Files |
|---|---|
| Surfaces | `bar.rs`, `dock.rs`, `island.rs`, `desktop.rs` + `wxcard.rs` + `desksettings.rs`, `wall.rs` (wallpaper: draws a photo, starts `mist-wall` for live ones) |
| Panels | `pane.rs` (one popup at a time), `spotlight.rs`, `start.rs`, `switcher.rs`, `control.rs`, `notifcenter.rs`, `clipboard.rs`, `calendar.rs`, `session.rs`, `wallpapers.rs`, `shot.rs`, `phonepanel.rs`, `winpick.rs` + `wincap.rs` (dock window thumbnails) |
| System | `notif.rs` (notification server), `polkit.rs`, `lock.rs` (`ext-session-lock-v1` + PAM), `idle.rs`, `tray.rs` (SNI host + dbusmenu), `snap.rs`, `sys.rs` (audio, network, Bluetooth, power), `media.rs` (MPRIS), `phone.rs` |
| Apps | `files/` (Mist Files + FileChooser portal backend), `player/` (libmpv on an EGL subsurface), `appwin.rs` + `edit.rs` (their windows, text fields) |
| Drawing | `gfx.rs`, `glass.rs`, `text.rs`, `img.rs`, `icons.rs`, `theme.rs`, `anim.rs` |
| Glue | `app.rs` (event loop), `ipc.rs` (`mist-ui-bin msg`), `apps.rs` (.desktop entries, icon themes), `i18n.rs` (`assets/i18n.tsv`), `data.rs` |

## Build, install, check

`./install.sh` installs `mist-ui-bin`, `mist-wall-bin`, `mist-comp` and the guard `mist-ui` (restarts
`mist-ui-bin`) into `~/.local/bin`. Log: `~/.cache/mist/ui.log`. Keep `lto = false`.

Offscreen checks, without touching the running session:

- `mist-ui --png out.png '{json}'`, `mist-ui-bin --desk-png dir`, `--dock-png out.png`,
  `--pane-png <panel> w h out.png [query]`, `--island-frames <demo> out [n] [fps]`
- `mist-ui --test` (a second bar over the real one on HDMI-A-1), `mist-ui --tray-dump`
- `mist-wall-bin --png out.ppm <preset|photo|sky> w h`, `MIST_WALL_BENCH=N` (ms per frame)
- `mist-ui-bin msg ui stats` (wake-ups, paints, paint time per surface)

The Quickshell (QML) version Mist started as is kept under the git tag `legacy-qml`; comments that name a
`.qml` file point there.

[tiny-skia]: https://github.com/linebender/tiny-skia
[swash]: https://github.com/dfrg/swash
