# Benchmarks

What a desktop shell and a compositor cost: memory, CPU, wake-ups and GPU memory, measured side by side with
other shells and compositors on the same machine, under the same conditions.

**Machine:** AMD Ryzen 7 5700X (8 cores / 16 threads), NVIDIA GeForce RTX 4060 (driver 620.70), CachyOS,
kernel 7.3. The two side-by-side sections are one series from 2026-11-12; raw data in
[`bench/results`](../bench/results). Mist is v0.3.0 plus the changes listed under
[What changed since 2026-11-09](#what-changed-since-2026-11-09).

## Shells side by side (2026-11-12)

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
should. Mist draws its bar, dock, island, desktop clock and widget, and is measured twice: with a live GLSL
wallpaper (`mist-wall`) and with a photo wallpaper, which `mist-ui` draws itself. Noctalia draws its bar, a
photo wallpaper and its first-run dialog; the others a bar.

The shells are the distribution packages: [`bench/fetch.sh`](../bench/fetch.sh) unpacks the Arch packages
into `~/.local/share/mist-bench` without root, checked against the SHA-256 sums in pacman's sync databases.
Median of the runs, range in brackets:

| Shell | Version | PSS idle | USS idle | GPU memory | CPU idle | wake-ups idle | CPU active | wake-ups active |
|---|---|---|---|---|---|---|---|---|
| **Mist** | v0.3.0-3-g7b147ec | **144 MB** (143 MB–147 MB) | 111 MB (110 MB–112 MB) | 50 MiB | 0.79 % (0.78 %–0.82 %) | 38/s (35/s–49/s) | 1.34 % (1.32 %–1.39 %) | 64/s (63/s–75/s) |
| **Mist, photo wallpaper** | v0.3.0-3-g7b147ec | **44 MB** (43 MB–44 MB) | 33 MB (33 MB–34 MB) | 0 MiB | 0.14 % (0.14 %–0.15 %) | 8/s (8/s–10/s) | 0.66 % (0.65 %–0.67 %) | 39/s (38/s–40/s) |
| Waybar | Waybar v0.15.0 | 34 MB | 30 MB | 0 MiB | 0.03 % | 2/s (2/s–3/s) | 0.38 % (0.37 %–0.38 %) | 27/s (27/s–29/s) |
| Ironbar | ironbar 0.19.1 | 158 MB (150 MB–159 MB) | 124 MB (114 MB–126 MB) | 9 MiB | 0.35 % (0.34 %–0.35 %) | 165/s (164/s–165/s) | 0.42 % (0.41 %–0.42 %) | 171/s |
| nwg-panel | nwg-panel 0.12.3-1 | 59 MB | 52 MB | 0 MiB | 0.09 % | 12/s (11/s–13/s) | 0.23 % (0.22 %–0.23 %) | 33/s (32/s–33/s) |
| swaybar | sway version 1.12 | 6 MB | 4 MB | 0 MiB | 0.00 % | 0/s | 0.03 % | 5/s |
| Quickshell, empty PanelWindow | Quickshell 0.3.2 (revision , distributed by Arch Linux) | 161 MB (159 MB–161 MB) | 128 MB (126 MB–128 MB) | 6 MiB | 0.00 % | 0/s | 0.00 % | 0/s |
| Noctalia | noctalia 5.4.1-1.1 | 192 MB (192 MB–193 MB) | 158 MB (157 MB–158 MB) | 60 MiB | 0.10 % (0.10 %–0.11 %) | 8/s | 4.01 % (3.98 %–4.15 %) | 123/s (121/s–125/s) |

3 runs each. Per process (idle, median PSS / CPU):

- Mist: mist-wall 104.8 MB / 0.64 %, mist-ui 25.5 MB / 0.12 %, mistd 6.9 MB / 0.02 %, nmcli 3.7 MB / 0.00 %, udevadm 1.3 MB / 0.00 %, pactl 1.0 MB / 0.00 %
- Mist, photo wallpaper: mist-ui 30.2 MB / 0.12 %, mistd 6.9 MB / 0.02 %, nmcli 3.7 MB / 0.00 %, udevadm 1.3 MB / 0.00 %, pactl 1.0 MB / 0.00 %
- Waybar: waybar 34.3 MB / 0.03 %
- Ironbar: ironbar 157.8 MB / 0.35 %
- nwg-panel: nwg-panel 57.9 MB / 0.09 %, pactl 0.9 MB / 0.00 %
- swaybar: swaybar 5.9 MB / 0.00 %
- Quickshell, empty PanelWindow: quickshell 160.9 MB / 0.00 %
- Noctalia: noctalia 191.7 MB / 0.10 %

What it says: the live wallpaper is most of Mist's cost (`mist-wall`: 105 MB, 0.64 % CPU, all of the GPU
memory). With a photo wallpaper there is no `mist-wall` process and no GPU memory at all: the whole shell
(bar, dock, island, desktop widgets, notifications, wallpaper, `mistd`) is 44 MB, between Waybar and
nwg-panel, which draw a bar only. `mist-wall` still starts on demand for the weather card's live sky and the
wallpaper gallery, and exits when they close. Every toolkit that renders with OpenGL (GTK 4 in Ironbar,
Qt Quick in Quickshell and Noctalia) starts above 150 MB before it draws anything useful: an empty Quickshell
bar costs 161 MB.

**PSS depends on what else is running.** Every process that uses OpenGL (and the compositors below) reads 10
to 27 MB higher here than in the 2026-11-09 series, with the same packages and driver: Quickshell 142 MB then,
161 MB now; sway 103 MB then, 129 MB now. The processes did not grow: their resident size is the same (sway
172 MB both times). What changed is how many other processes on the machine map the same GPU libraries.
Mesa's `libLLVM` (26 MB resident in a headless sway), `libnvidia-gpucomp` (35 MB) and `libnvidia-eglcore`
(13 MB) are file pages, and PSS splits each page between the processes that map it. On 2026-11-12 the desktop around
the benchmark was mist-comp with `mist-wall`, which load no Mesa, so the shell under test carried the Mesa
pages alone; the 2026-11-09 figures match a desktop where one more process had them mapped (that desktop's
process list was not recorded). Measured with a headless sway:

| | PSS | of that, anonymous | of that, files |
|---|---|---|---|
| sway alone | 130 MB | 70 MB | 61 MB |
| with a second sway running | 106 MB | 70 MB | 36 MB |
| with three running | 96 MB | 70 MB | 27 MB |

So the 2026-11-12 figures are close to what each shell costs as the only OpenGL user on the machine, and the
2026-11-09 ones were lowered by the desktop they ran beside. Processes without OpenGL did not move (swaybar,
Waybar, nwg-panel, Mist with a photo wallpaper). Compare within one series, not across them;
[`bench/sample.py`](../bench/sample.py) now also records `pss_anon`, the part that does not depend on the
neighbours.

### What changed since 2026-11-09

| Mist, live wallpaper | PSS idle | GPU memory | CPU idle | wake-ups idle | CPU active | wake-ups active |
|---|---|---|---|---|---|---|
| 2026-11-09 | 132 MB | 42 MiB | 0.83 % | 110/s | 1.35 % | 151/s |
| 2026-11-10 | 131 MB | 50 MiB | 0.84 % | 96/s | 1.36 % | 142/s |
| 2026-11-12 | 144 MB | 50 MiB | 0.79 % | 38/s | 1.34 % | 64/s |

(The 13 MB between the last two rows is the series-wide shift described above, not a change in Mist.)

- **`blocking` pinned to 1.6.2.** In 1.7.0 an idle pool thread never exits and wakes twice a second for the
  life of the process; zbus starts a few of them while connecting. Three Mist processes use zbus.
- **`mist-wall` frame timing.** Calm scenes (the default "dunes" among them) run at 24 fps instead of 30 (12
  instead of 15 on other monitors), the weather card's sky at 20 fps, and each live wallpaper renders at its
  preset's `res` scale (half resolution for most, then scaled up; this is the extra 8 MiB of GPU memory).
  Surfaces due within 8 ms of each other are drawn on one tick, so the monitors and the sky share wake-ups
  instead of each keeping its own schedule.
- **Photo wallpaper without the OpenGL process.** `mist-ui` decodes and draws the photo into a `wl_shm`
  buffer once; `mist-wall` is not started. This is the "Mist, photo wallpaper" row.
- **Caps Lock / Num Lock as an event.** mist-comp sends `mistlock>>CAPS,NUM` on its event socket when the
  state changes; `mistd island` no longer reads the keyboard LEDs four times a second under mist-comp
  (0.1 wake-ups/s, measured alone).
- **Large allocations returned to the system** (`M_MMAP_THRESHOLD`): decoded images and frame buffers are
  unmapped when freed instead of staying in the heap.

CPU did not move: it is the shader and the composite of the live wallpaper, which these changes do not
touch. What dropped is the number of times the idle shell wakes the CPU, by two thirds.

A series is only valid on a quiet machine. A first attempt on 2026-11-12 ran beside a dev server using 3.4
cores and gave sway anything from 16 to 35 % with the shm client (33.1 to 34.0 % in the series above), so
[`bench/quiet.sh`](../bench/quiet.sh) now samples `/proc/stat` before every run and waits, then stops the
series, while other processes use more than 5 % of all cores.

## Compositors side by side (2026-11-12)

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
| sway | sway version 1.12 | 129 MB (128 MB–130 MB) | 12 MiB | 0.00 % | 33.62 % (33.12 %–34.03 %) | 8.26 % (8.13 %–8.46 %) | 1.410 ms (1.387 ms–1.444 ms) | 59 (58–59) |
| labwc | labwc 0.20.2 (+xwayland +nls +rsvg +libsfdo) wlroots-0.20.2 | 137 MB (135 MB–137 MB) | 2 MiB | 0.00 % | 8.09 % (6.88 %–8.26 %) | 8.89 % (7.00 %–9.90 %) | 1.502 ms (1.173 ms–1.678 ms) | 59 (59–60) |
| weston | weston 15.0.1 | 129 MB | 14 MiB | 0.00 % | 3.33 % (2.33 %–3.39 %) | 0.76 % (0.55 %–0.92 %) | 0.126 ms (0.091 ms–0.153 ms) | 60 |
| **mist-comp** | mist-comp v0.3.0-3-g7b147ec | **128 MB** (127 MB–128 MB) | 15 MiB | 0.00 % | 2.19 % (2.17 %–2.20 %) | 1.47 % (1.41 %–1.49 %) | 0.245 ms (0.235 ms–0.249 ms) | 60 |

5 runs each.

All four idle at 0 % CPU and 0 wake-ups. With the CPU-drawn client mist-comp is the cheapest; with the
OpenGL client weston is (half of mist-comp's time per frame), and both are 6 to 12 times cheaper per frame
than the wlroots compositors. sway with the shm client used 33 % of a core in every run; labwc, on the same
wlroots, 8 %; the cause was not investigated. Hyprland and niri have no headless mode a script can start,
so they are not in the table; nested in a window they would depend on the host compositor's frames.

CPU is 13 to 35 % higher for every compositor than in the 2026-11-09 series (weston, the same binary:
2.74 % then, 3.33 % now with the shm client), and the first run of labwc and of weston came out at the old
level (weston 2.33 %, then 3.14 to 3.39 %). Three causes were tested and ruled out: other load on the machine
(a repeat with the browser closed gave the same 3.2 % and 0.8 % for weston, 2.15 % and 1.4 % for mist-comp);
the desktop's own live wallpaper sharing the GPU (paused with `SIGSTOP`: no change); a settings change
(governor, boost and frequency driver as before; the one system package updated between the series,
`cachyos-settings`, changed nothing that affects CPU time). The cause is not known. The order of the four
and the ratios between them are the same in both series.

Where mist-comp's time per OpenGL frame goes (its `prof` IPC counters, same client, a separate run at
0.197 ms per frame):

| Stage | Time per frame |
|---|---|
| render (smithay's damage renderer and the NVIDIA driver) | 81 µs |
| client requests (two dispatches per frame) | 28 µs |
| building the scene (render elements) | 14 µs |
| event loop turn | 9 µs |
| texture cache clean-up | 7 µs |
| frame callbacks and presentation feedback | 5 µs |

No stage of Mist's own code stands out, so nothing was changed: the gap to weston is in the renderer and the
driver calls under it.

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
