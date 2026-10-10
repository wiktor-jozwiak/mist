# Mist native surfaces

`mist-ui` (Rust) draws Mist's surfaces on the CPU into wl_shm buffers: no GPU context in the process, so
no NVIDIA driver (~160 MB RSS per process) and no Qt Quick. Data comes from event sources only (Hyprland
socket, D-Bus, `pactl subscribe`, inotify, `mistd stats`).

Migration from Quickshell, one surface at a time (Quickshell keeps the rest and the popups; the native
surfaces open them over `mist-qs ... ipc call mist toggleOn|controlOn ...`):

| Surface | State |
|---|---|
| Top bar (TopBar.qml) | native (bar.rs), tray host + dbusmenu menus (tray.rs) |
| Dock, Island, Desktop cards, popups | QML |
| Live wallpaper + weather sky | QML (to move into a small GPU process at the end) |

Build / install: `./install.sh` (-> ~/.local/bin/mist-ui-bin; guard ~/.local/bin/mist-ui restarts it).
Checks: `mist-ui --test` (one bar on HDMI-A-1 over the QML one), `mist-ui --png out.png '{json}'`,
`mist-ui --tray-dump` (tray items + menus, writes /tmp/mist-tray-menu.png).
