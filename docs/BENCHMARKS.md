# Benchmarks

What a desktop shell costs while you are not looking at it: memory, CPU at idle, GPU context.

## Method

[`bench/measure.sh`](../bench/measure.sh) sums every process of a shell:

- **PSS** (proportional set size, from `/proc/<pid>/smaps_rollup`): shared libraries are split between the
  processes that map them, so the numbers of several processes add up honestly. RSS counts every shared page
  in full and overstates multi-process shells.
- **CPU**: run time of all threads from `/proc/<pid>/task/*/schedstat` (nanoseconds) over the window, as % of
  one core. `top` ticks (10 ms) are too coarse for idle shells.
- **GL**: whether the process maps a GL driver. On NVIDIA every process with an EGL / GLX context maps
  `libnvidia-glcore` and friends: ~100 to 160 MB RSS before the first frame (measured with `es2gears`).

```bash
bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd     # Mist
bench/measure.sh 30 noctalia                            # Noctalia v5
bench/measure.sh 30 quickshell qs                       # any Quickshell config
bench/measure.sh 30 waybar                              # Waybar
bench/measure.sh 30 gjs hyprpanel                       # HyprPanel / AGS
```

Rules: same session, shell running for at least 3 minutes (caches warm, start-up animations done), no
pointer movement during the window, note whether media is playing (the Dynamic Island equalizer and
`pactl` events wake the shell).

**Machine:** AMD Ryzen 7 5700X (8 cores / 16 threads), NVIDIA GeForce RTX 4060 (driver 615.71), two 1920×1080 monitors (165 Hz +
75 Hz), Hyprland 0.56.2, CachyOS, kernel 7.2.

## Results: measured on this machine

| Shell | Toolkit | Processes | PSS | CPU idle | GL context |
|---|---|---|---|---|---|
| **Mist native** (2026-10-15) | Rust, CPU, `wl_shm` | mist-ui, mist-wall, mistd ×4 | **106 MB** | **0.6 %** (music on) | mist-wall only |
| **Mist native** (2026-10-16, live wallpaper "dunes") | | same | 87 to 103 MB | 0.6 % mist-ui, 0.8 % mist-wall | mist-wall only |
| Mist native, `mist-ui` alone | | 1 | 36 to 50 MB | 0.6 % | no |
| Noctalia v5 | C++, cairo + EGL | 1 | 122 MB | 0.07 % | yes |
| Noctalia v5 (2026-10-16, `bench/others.sh`: 3 min settle, 30 s) | | 1 | 105 MB (RSS 182 MB) | 0.11 % | yes |
| Empty Quickshell (`shell.qml` with one `PanelWindow`) | Qt Quick | 1 | 124 MB | ~0 % | yes |
| Mist on Quickshell (old QML version) | Qt Quick + QML | 1 + helpers | 428 MB | 9.5 % | yes |

With a photo wallpaper `mist-wall` exits and `mist-ui` draws the photo itself; the whole shell is then expected at about
**60 MB with no GPU context** (estimate from the per-process numbers, not measured yet).

Where the memory of a GPU-based shell goes: an empty Quickshell window already costs 124 MB PSS on NVIDIA,
before any widget. Mist spends its memory on caches it controls (glyph atlas, app icons, the wallpaper at
screen size).

## Results: reported elsewhere

Other machines, other configs, mostly RSS; useful for the order of magnitude, not for a ranking.

| Shell | Number | Source |
|---|---|---|
| Waybar (C++, GTK3) | flat at 50 to 66 MB in normal sessions; a leak in the taskbar / icon theme reload grew one session to 32 GB in 22 h (fixed in PR #5342) | [Alexays/Waybar#5186](https://github.com/Alexays/waybar/issues/5186) |
| Caelestia (Quickshell) | ~650 MB after a restart; a launcher bug grew it to 900 MB RSS in 20 min at ~6 % CPU | [caelestia-dots/shell#1927](https://github.com/caelestia-dots/shell/issues/1927) |
| Noctalia v4.6 (Quickshell) | release notes: system stats polled at 1 Hz, animations off when idle, "0 % CPU on idle" | [v4.6.0 release](https://newreleases.io/project/github/noctalia-dev/noctalia-shell/release/v4.6.0) |

Not measured yet: Waybar, HyprPanel (AGS / GJS), DankMaterialShell, end-4 illogical-impulse, Ironbar (none of
them is installed on this machine). [`bench/others.sh`](../bench/others.sh) measures every one that is: each is
started alone next to the running Mist, settles for 3 minutes, is measured for 30 s and stopped; add its
`TOTAL` line with the date to the table above.

## Mist internals

`mist-ui-bin msg ui stats` prints counters since start: loop wake-ups (`wake`), paints per surface (`bar`,
`dock`, `island`, `stage`, `card`) and their time in µs. Between two calls 10 s apart, with music playing:
122 wake-ups, 33 ms of drawing (0.33 % of a core). Offline frame benchmarks: `mist-ui --island-bench <demo|eq>`,
`MIST_BENCH=n mist-ui --dock-png out.png [slot]`.

## mist-comp (the compositor)

Measured headless and nested (`native/mist-comp/tests`, a 1920×1080 output, no real monitor), 2026-10-22,
on the machine above. CI runs the same scripts on Mesa's software renderer and prints them in the job
summary (`.github/workflows/ci.yml`, step "benchmarks"); those numbers compare runs with each other, not
with this table.

| What | Script | Result |
|---|---|---|
| Compositor CPU per frame, a client redrawing nonstop under a Mist bar (kitty) | `tests/bench.sh kitty 8` | 0.25 ms/frame (480 frames in 8 s), RSS 105 MB |
| Same, a translucent window with glass + shadow (Zen rule) | `tests/bench.sh zen 8` | 0.25 ms/frame, RSS 109 MB |
| mist-ui full-monitor pane (Spotlight, 12 letters typed) | `tests/bench-pane.sh` | 31 frames: 4.2 ms paint, 1.4 ms commit, 125 k px uploaded per frame (6 % of the screen) |
| Blur passes per frame, Zen (several surfaces per window) | IPC `stats` (`blur runs/calls`) | 1.92 → 0.96 (one blur per window since 2026-10-22) |
| Nested mist-comp RSS, NVIDIA EGL only vs. glvnd loading Mesa too | `ps` | 91 → 67 MB |
| Weather card redraws while covered by windows | `tests/t-cardcover.sh` | 28 → 0 per 20 s |
| mist-wall CPU under a maximized window | `tests/t-wall.sh` | 0 ticks / 2 s |
| Overview (Mission Control) open and idle | `tests/t-overview.sh` | 0 frames / s |
| Magnifier on, pointer still | `tests/t-zoom.sh` | 0 frames / s |
| Workspace swipe held still | `tests/t-swipe.sh` | 1 frame / 0.5 s (the screenshot's) |
| Render passes on an idle monitor while a client animates on the other one | `tests/t-perout.sh` | 180 → 0 per 3 s |
| Frame callbacks to an animating client covered by an opaque window | `tests/t-occlude.sh` | 60 → 1 per s (60 again once uncovered) |
| Memory while panels open and close (2 × 30 s) | `tests/t-texleak.sh` | levels off: 110 → 132 → 139 MB on llvmpipe, ~80 MB on NVIDIA (grew ~4 MB/min before 2026-10-22) |

Watching it live: `hyprctl` is mist-comp in its session, and its IPC socket answers `stats` (frames,
commits, wake-ups, blur runs), `passes` (render passes per monitor, and how many changed pixels), `top 5` (commits per client over 5 s: who keeps the screen busy) and `blurs`.
The watchdog logs `watchdog: <app> draws N frames/s while hidden` to `~/.cache/mist/comp.log` when a client
keeps drawing on a hidden workspace (`MIST_COMP_WATCHDOG_SECS`, default 60).
