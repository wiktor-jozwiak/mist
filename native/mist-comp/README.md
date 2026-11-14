# mist-comp

Mist's Wayland compositor, written in Rust on [Smithay]. It runs the Mist shell (or any other) in a login
session of its own, next to Hyprland, and answers enough of Hyprland's IPC that `hyprctl` and the shell's
scripts work unchanged. It is experimental: the author's daily session, not a finished product.

## What it does

- **Windows:** floating windows with title bars or the app's own, rounded corners, shadows, glass blur under
  translucent windows and panels, Windows 11 style snapping, Overview (`Super + O`), a magnifier, workspace
  swipe that follows the fingers, `Super + D` to show the desktop.
- **No work it can skip:** a monitor redraws only when something on it changed, glass re-blurs only when what
  is behind it moved, and a window covered by others gets one frame a second instead of sixty.
- **Session:** DRM / KMS through logind, monitor hotplug, VRR, explicit sync, XWayland, screen sharing through
  xdg-desktop-portal, about 45 protocols. After a crash the windows come back on their workspaces;
  `mist-session restart` loads a new build without logging out.
- **Greeter:** on greetd, mist-comp runs `mist-ui greeter`, drawn by the same code as the lock screen.

Not there yet: tiling, tearing control, verified direct scan-out on NVIDIA.

## Run it

| How | Command |
|---|---|
| In a window on your current desktop, with the Mist shell | `mist-try` |
| Nested, one client, for testing | `mist-comp -c kitty` |
| As a login session (asks for sudo for the session entry) | `session/install-session.sh` |
| The same, and the Mist greeter on greetd | `session/install-session.sh --greeter` |
| Convert a Hyprland config to `comp.conf` | `mist-comp migrate [hyprland.lua] > comp.conf` |

From a TTY it takes the GPU through logind; inside another compositor it opens a window. Configuration
(`~/.config/mist/comp.conf`, `outputs`): [docs/CONFIG.md](../../docs/CONFIG.md#compconf-outputs-mist-comp-only).
Log in a real session: `~/.cache/mist/comp.log`.

## Code

| Area | Files |
|---|---|
| Start, backends | `main.rs`, `udev.rs` (DRM / KMS), `winit.rs` (nested), `headless.rs`, `session.rs`, `watchdog.rs` |
| Windows | `wm.rs`, `focus.rs`, `rules.rs`, `layouts.rs`, `snap.rs`, `snapswipe.rs`, `swallow.rs`, `grabs/` |
| Drawing | `scene.rs`, `blur.rs`, `rounded.rs`, `shadow.rs`, `decor.rs`, `cover.rs`, `overview.rs`, `anim.rs` |
| Protocols | `handlers/`, `wlr.rs`, `extws.rs`, `screencopy.rs`, `bgeffect.rs`, `lock.rs`, `xwayland.rs`, `outman.rs` |
| Hyprland compatibility | `hyprcompat.rs`, `hyprproto.rs`, `ipc.rs`, `migrate.rs` |
| Input | `input.rs`, `cursor.rs`, `hit.rs`, `hotcorner.rs` |

## Tests

`tests/run-all.sh` starts the compositor nested on a headless backend and runs every `tests/t-*.sh` against
it: focus, drag and drop, glass damage, snapping, workspaces, XWayland, the lock and more. CI runs them on the
software renderer (`tests/ci.sh`).

[Smithay]: https://github.com/Smithay/smithay
