# Benchmarks

What a desktop shell and a compositor cost: memory, CPU, wake-ups and GPU memory, measured side by side with
other shells and compositors on the same machine, under the same conditions.

**Machine:** AMD Ryzen 7 5700X (8 cores / 16 threads), NVIDIA GeForce RTX 4060 (driver 620.70), CachyOS,
kernel 7.3. All numbers below are from 2026-11-09; raw data in [`bench/results`](../bench/results).

## Shells side by side (2026-11-09)

[`bench/shells.sh`](../bench/shells.sh) `3 45 60`: every shell runs **alone** in a fresh headless sway 1.12
rendering with OpenGL ES on the GPU (two 1920x1080 outputs at 60 Hz), with a throwaway `HOME` and
`XDG_RUNTIME_DIR` (so each one starts with the configuration its package ships), its own D-Bus session bus,
and nothing of the running desktop except the PipeWire socket. sway is the host because it is neither
Hyprland nor mist-comp: no shell plays on home ground, and all of them support it. After 45 s to settle,
[`bench/sample.py`](../bench/sample.py) measures the shell's whole process tree (helpers it spawns included)
for two windows of 60 s:

- **idle**: nothing happens;
- **active**: [`bench/activity.sh`](../bench/activity.sh), a workspace switch every 2 s, a window
  (`weston-simple-shm`) opened every 6 s and closed 3 s later, a notification every 10 s.

Measured: **PSS** (shared pages split between the processes that map them), **USS** (private pages: what
stopping the shell frees), **CPU** (run time of every thread from `schedstat`, % of one core), **wake-ups**
(context switches per second: polling shows here even when its CPU rounds to zero) and **GPU memory**
(`nvidia-smi`, per process). A screenshot of both outputs from every run proves the shell drew what it
should. Mist draws its bar, dock, island, desktop clock and widget and a live GLSL wallpaper (`mist-wall`);
Noctalia its bar, a photo wallpaper and its first-run dialog; the others a bar.

The shells are the distribution packages: [`bench/fetch.sh`](../bench/fetch.sh) unpacks the Arch packages
into `~/.local/share/mist-bench` without root, checked against the SHA-256 sums in pacman's sync databases.
Median of the runs, range in brackets:

| Shell | Version | PSS idle | USS idle | GPU memory | CPU idle | wake-ups idle | CPU active | wake-ups active |
|---|---|---|---|---|---|---|---|---|
| **Mist** | v0.3.0 | **132 MB** (129 MB–132 MB) | 107 MB (105 MB–107 MB) | 42 MiB | 0.83 % (0.82 %–0.83 %) | 110/s (107/s–111/s) | 1.35 % (1.35 %–1.42 %) | 151/s (149/s–170/s) |
| Waybar | Waybar v0.15.0 | 32 MB | 26 MB | 0 MiB | 0.03 % (0.02 %–0.04 %) | 2/s (1/s–3/s) | 0.38 % | 28/s (27/s–29/s) |
| Ironbar | ironbar 0.19.1 | 137 MB (137 MB–139 MB) | 103 MB (103 MB–104 MB) | 9 MiB | 0.37 % (0.36 %–0.39 %) | 160/s (160/s–161/s) | 0.43 % (0.42 %–0.43 %) | 168/s (167/s–169/s) |
| nwg-panel | nwg-panel 0.12.3-1 | 56 MB | 47 MB | 0 MiB | 0.10 % | 11/s | 0.22 % | 31/s |
| swaybar | sway version 1.12 | 7 MB | 4 MB | 0 MiB | 0.00 % | 0/s | 0.03 % | 6/s (5/s–6/s) |
| Quickshell, empty PanelWindow | Quickshell 0.3.2 (revision , distributed by Arch Linux) | 142 MB | 115 MB | 6 MiB | 0.00 % | 0/s | 0.00 % | 0/s |
| Noctalia | noctalia 5.4.1-1.1 | 177 MB (176 MB–179 MB) | 152 MB (151 MB–154 MB) | 60 MiB | 0.12 % (0.10 %–0.12 %) | 7/s | 4.46 % (4.45 %–4.55 %) | 119/s (115/s–120/s) |

3 runs each. Per process (idle, median PSS / CPU):

- Mist: mist-wall 93.4 MB / 0.63 %, mist-ui 26.4 MB / 0.16 %, mistd 6.6 MB / 0.03 %, nmcli 2.5 MB / 0.00 %, udevadm 1.0 MB / 0.00 %, pactl 1.0 MB / 0.00 %
- Waybar: waybar 31.6 MB / 0.03 %
- Ironbar: ironbar 137.2 MB / 0.37 %
- nwg-panel: nwg-panel 55.2 MB / 0.10 %, pactl 1.0 MB / 0.00 %
- swaybar: swaybar 6.7 MB / 0.00 %
- Quickshell, empty PanelWindow: quickshell 142.4 MB / 0.00 %
- Noctalia: noctalia 176.9 MB / 0.12 %


What it says: the live wallpaper is most of Mist's cost (`mist-wall`: 93 MB, 0.63 % CPU, all of its GPU
memory). The rest of Mist (bar, dock, island, desktop widgets, notifications, `mistd`) is 38 MB with no GPU
memory, between Waybar and nwg-panel, which draw a bar only. Every toolkit that renders with OpenGL (GTK 4 in
Ironbar, Qt Quick in Quickshell and Noctalia) starts above 130 MB before it draws anything useful: an empty
Quickshell bar costs 142 MB.

## Compositors side by side (2026-11-09)

[`bench/compositors.sh`](../bench/compositors.sh) `5 30`: each compositor headless on the GPU (OpenGL ES,
one 1920x1080 output at 60 Hz, XWayland off, default configuration), a throwaway `HOME` and runtime dir, and
the same clients from weston's demo set, 30 s each:

- **idle**: no client;
- **shm**: `weston-simple-shm`, a window drawn on the CPU every frame, the path GTK / Qt software rendering
  and Mist use;
- **GL**: `weston-simple-egl -f`, a fullscreen OpenGL client synced to the compositor's frames; its own
  frame count gives the compositor's CPU time per frame.

| Compositor | Version | PSS idle | GPU memory | CPU idle | CPU, shm client | CPU, GL client | CPU per GL frame | GL client fps |
|---|---|---|---|---|---|---|---|---|
| sway | sway version 1.12 | 103 MB (102 MB–104 MB) | 12 MiB | 0.00 % | 33.53 % (33.41 %–33.89 %) | 6.62 % (6.36 %–6.89 %) | 1.120 ms (1.071 ms–1.170 ms) | 59 |
| labwc | labwc 0.20.2 (+xwayland +nls +rsvg +libsfdo) wlroots-0.20.2 | 109 MB (108 MB–109 MB) | 2 MiB | 0.00 % | 6.14 % (6.09 %–6.18 %) | 5.98 % (5.82 %–6.09 %) | 1.008 ms (0.978 ms–1.027 ms) | 59 (59–60) |
| weston | weston 15.0.1 | 101 MB (101 MB–102 MB) | 14 MiB | 0.00 % | 2.74 % (2.53 %–2.81 %) | 0.63 % (0.60 %–0.67 %) | 0.105 ms (0.100 ms–0.111 ms) | 60 |
| **mist-comp** | mist-comp v0.3.0 | **100 MB** (100 MB–101 MB) | 15 MiB | 0.00 % | 1.72 % (1.59 %–2.00 %) | 1.30 % (1.27 %–1.33 %) | 0.217 ms (0.212 ms–0.222 ms) | 60 |

5 runs each.

All four idle at 0 % CPU and 0 wake-ups. With the CPU-drawn client mist-comp is the cheapest; with the
OpenGL client weston is (half of mist-comp's time per frame), and both are 5 to 10 times cheaper than
the wlroots compositors. sway with the shm client used 33 % of a core in every run; labwc, on the same
wlroots, 6 %; the cause was not investigated. Hyprland and niri have no headless mode a script can start,
so they are not in the table; nested in a window they would depend on the host compositor's frames.

## Live session measurements (Hyprland, October 2026)

The numbers below come from the earlier method: [`bench/measure.sh`](../bench/measure.sh) on the running
desktop, one shell next to another. They are kept for the history (the QML version, the rewrite); the
tables above replace them for comparisons.

### Method

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

### Results, live session

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

### Results reported elsewhere

Other machines, other configs, mostly RSS; useful for the order of magnitude, not for a ranking.

| Shell | Number | Source |
|---|---|---|
| Waybar (C++, GTK3) | flat at 50 to 66 MB in normal sessions; a leak in the taskbar / icon theme reload grew one session to 32 GB in 22 h (fixed in PR #5342) | [Alexays/Waybar#5186](https://github.com/Alexays/waybar/issues/5186) |
| Caelestia (Quickshell) | ~650 MB after a restart; a launcher bug grew it to 900 MB RSS in 20 min at ~6 % CPU | [caelestia-dots/shell#1927](https://github.com/caelestia-dots/shell/issues/1927) |
| Noctalia v4.6 (Quickshell) | release notes: system stats polled at 1 Hz, animations off when idle, "0 % CPU on idle" | [v4.6.0 release](https://newreleases.io/project/github/noctalia-dev/noctalia-shell/release/v4.6.0) |

Not measured: HyprPanel (AGS / GJS), DankMaterialShell, Caelestia and end-4 illogical-impulse, which are
not in the distribution's repositories.

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
| Blur runs while a maximized window redraws a small square under the glass bar and dock | `tests/t-glassdamage.sh` | 362 → 0 per 3 s; compositor CPU 8 → 4 ticks / 3 s (NVIDIA) |
| Memory while panels open and close (2 × 30 s) | `tests/t-texleak.sh` | levels off: 110 → 132 → 139 MB on llvmpipe, ~80 MB on NVIDIA (grew ~4 MB/min before 2026-10-22) |

### Idle and live, 2026-10-22

`env -u __EGL_VENDOR_LIBRARY_FILENAMES SECS=15 sh bench/comp.sh` (headless, llvmpipe, two 1920x1080 outputs;
inside a Mist session the variable hides Mesa and the run fails with "software requested"):

```
mist-comp alone        pss= 125.3MB threads=34 cpu= 0.17% wakeups=   5.7/s frames=  0.1/s
mist-comp + shell      pss= 173.4MB threads=34 cpu= 0.04% wakeups=   6.3/s frames=  3.0/s
mist-ui (shell)        pss=  31.5MB threads=29 cpu= 0.33% wakeups=  22.5/s frames=  0.0/s
mist-comp, pointer     pss= 174.5MB threads=34 cpu= 0.83% wakeups= 162.8/s frames= 60.0/s
```

The live session (NVIDIA, 165 + 75 Hz, 35 min after login, binary from 17:44), `bench/measure.sh 30 mist-comp`
and `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd`, while Zen showed a page and the Claude desktop app
streamed text (IPC `top 5`: zen 272 commits/s, Claude 107/s, mist-live 15/s, kitty 3/s, Mist layers < 2/s):

```
mist-comp      pss=231.2MB rss=283.3MB threads=7  cpu=14.28% gl=yes   (bench/comp.sh live: 15.96 %, 995 wake-ups/s)
mist-wall-bin  pss= 51.4MB            mist-ui-bin pss=64.5MB (41 threads)   mistd x3 pss=9.7MB
shell TOTAL    pss=125.6MB rss=192.0MB threads=65 cpu=5.10% gl=yes
```

`stats` since login: 327 826 frames, 613 164 commits, blur 260 464 runs of 934 072 calls (72 % of the glass
draws reused a cached blur). Hyprland on the same session, same apps: not measured yet.

Watching it live: `hyprctl` is mist-comp in its session, and its IPC socket answers `stats` (frames,
commits, wake-ups, blur runs), `passes` (render passes per monitor, and how many changed pixels), `top 5` (commits per client over 5 s: who keeps the screen busy) and `blurs`.
The watchdog logs `watchdog: <app> draws N frames/s while hidden` to `~/.cache/mist/comp.log` when a client
keeps drawing on a hidden workspace (`MIST_COMP_WATCHDOG_SECS`, default 60).

## Mist Player, before and after the rewrite (2026-10-29)

[`bench/player.py`](../bench/player.py) `BIN FILE play|chrome|pause|min [secs]` starts `mist-ui-bin player FILE`,
waits for the first frame, then measures for 8 s: RSS, CPU (% of one core, `/proc/<pid>/stat`), GPU
(`nvidia-smi pmon`: shader / decoder %), video renders per second and UI repaints per second (the player's
`--test dump` counters). Files: 1080p H.264 8 Mb/s 30 fps, 2160p HEVC 25 Mb/s 30 fps, MP3. `hwdec=auto-safe`
(nvdec on this card) in both versions. Start = launch to first video frame shown (audio: first position).

| file | scenario | RSS before → after | CPU before → after | renders/s before → after | UI repaints/s before → after | start before → after |
|---|---|---|---|---|---|---|
| empty window | – | 313 → **25 MB** | – | – | – | – |
| 1080p | play | 409 → 398 MB | 2.3 → 1.7 % | 30 → 30 | 0.1 → 0.1 | 594 → **340 ms** |
| 1080p | controls shown | 409 → 403 MB | 3.7 → 2.1 % | 30 → 30 | 6.6 → **1.1** | – |
| 1080p | paused | 405 → 396 MB | 0.2 → 0.1 % | 0 → 0 | – → 0 | – |
| 1080p | minimized | 407 → 402 MB | 2.1 → **0.9 %** | 30 → **0** | – → 0.1 | – |
| 2160p | play | 643 → 628 MB | 2.7 → 2.2 % | 30 → 30 | – → 0.1 | 644 → 564 ms |
| 2160p | controls shown | 587 → 574 MB | 4.3 → 2.5 % | – → 30 | 6.7 → **1.1** | – |
| 2160p | minimized | 582 → 571 MB | 2.8 → **1.2 %** | 30 → **0** | – → 0.1 | – |
| MP3 | play | 332 → **126 MB** | 2.0 → 0.5 % | – | 6.2 → 0.9 | 532 → **124 ms** |
| MP3 | minimized | 334 → 126 MB | 0.5 → 0.2 % | – | – → 0 | – |

Start is the median of 5 runs; the first run after another video is ~550 ms (cold CUDA context) in both.
What changed and what did not:

- **Memory with video does not move.** ~400 MB at 1080p is the NVIDIA GL driver plus the CUDA context of
  nvdec (83 MB of `/dev/nvidiactl` mappings alone); standalone `mpv --vo=gpu --hwdec=nvdec` is 439 MB RSS.
  `vo=dmabuf-wayland` (which would skip GL) fails on this driver (`vaExportSurfaceHandle() failed`,
  "no video"). GLES instead of GL costs more (456 MB).
- **Memory without video does.** libmpv is loaded at the first file and GL only for files with a picture:
  the empty window is plain wl_shm, audio files never create an EGL context.
- **Hidden costs nothing per frame.** Video is paced by `wl_surface.frame` callbacks; a minimized window gets
  none, so nothing is rendered (mpv drops the frames after its 200 ms wait, ~5 wake-ups/s; the decoder keeps
  running, GPU decoder ~2 %). Before, it rendered 30 frames/s into a hidden surface.
- **Controls repaint once a second**, when the clock text changes, not on every position update.
  `time-pos` is observed only while the controls are visible.
- **Start**: the old player read properties synchronously in mpv's event handler; with mpv's core locked
  waiting for the first render that stalled both until mpv's 200 ms timeout. The track list and metadata are
  now observed properties and the position an asynchronous read.
- The UI drawing code (the controls, popovers, panels) was kept: it costs 0.1 to 1 repaint/s, rewriting it
  saves nothing measurable.

## Mist Files, before and after the engine pass (2026-10-29)

Offscreen: `MIST_FILES_PROF=1 mist-ui-bin files --png out.png DIR WxH` prints the time of each step (list,
type sniffing, sort, first paint, then a sort by type, a filter and a second paint). Live: a nested
`mist-comp` (1920×1080), the window maximized on a folder of 50 000 empty files, 200 pointer moves over the
rows (one every 25 ms, `files --test move`); CPU of the files process from `/proc/<pid>/stat`, commits per
second from the compositor's `top` (30/s in both versions: the same frames are drawn).

| measurement | before | after |
|---|---|---|
| repaint of a 1920×1080 window (any folder) | 21.9 ms | **1.2 to 2.7 ms** |
| repaint of a 1280×800 window | 11.6 ms | **< 1 ms** |
| CPU for 200 row hovers, 5.4 s (same 30 commits/s) | 4620 ms (85 % of a core) | **930 ms (17 %)** |
| `/usr/bin` (3475 entries) until the first frame, warm cache | 15.2 ms | **5.8 ms** (+12.9 ms type sniffing after it) |
| `/usr/bin` cold cache (first visit after boot) | 551 ms | not measured (no root here to drop caches): the 0.5 s was `open()` + read of every file without a known extension, now done after the first frame |
| sort of 50 000 names by type | 14.5 ms | 11.9 ms |
| RSS, 50 000-file folder, live | 72 MB | 68 MB |

The screenshots of six layouts (list, icons, preview, selection, empty, error) are byte-identical before and
after. What changed:

- **The backdrop is drawn once.** The window's gradient, the sidebar's glass and the list panel's fill only
  change with the size or layout; they were painted every frame (10.3 + 4.6 + 2.0 ms at 1080p). They are now
  painted once by the same tiny-skia calls and kept run-length encoded per row (~60 KB, not an 8 MB copy).
- **The list blit skips empty pixels.** The rows are drawn into a scratch canvas the panel's size and composited
  over the window: tiny-skia's `draw_pixmap` (4.3 ms) is replaced by a source-over with the same float
  arithmetic that skips transparent pixels and copies opaque ones.
- **Only changed rows reach the compositor.** Every Mist app window (Files, Player) now reuses two buffers and
  copies and damages only the rows that differ (the `Swap` of the panels); before, each frame was a fresh
  buffer, all of it converted and damaged.
- **Types of unknown files are read after the first frame**, in the listing thread, and the list re-sorted.
- `natural_cmp` compares digit runs as slices (no allocation per comparison); a sort by type computes each
  entry's type name once.
- Not changed: listing 50 000 entries is ~72 ms, almost all of it one `statx` per entry in a background
  thread; the UI code (layout, menus, dialogs, portal) is kept, it was not where the time went.
