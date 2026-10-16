<div align="center">

# Mist

**Eine von macOS inspirierte Desktop-Shell für Hyprland, geschrieben in Rust und auf der CPU gezeichnet.**

Leiste, Dock, Dynamic Island, Desktop-Widgets, Spotlight-artige Suche, Kontrollzentrum, Benachrichtigungen,
Sperrbildschirm und Polkit-Agent in einem Prozess von ~50 MB, ohne GPU-Kontext.

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

![Lizenz: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)

![Mist-Desktop](../img/desktop.webp)

<sub>Offscreen-Render (`mist-ui --desk-png`, `--dock-png`, `--png`, `mist-wall-bin --png`) mit Demodaten.
In einer echten Sitzung macht Hyprland den Hintergrund unter dem Glas unscharf.</sub>

</div>

## Warum

Die meisten Hyprland-Shells laufen auf Qt Quick (Quickshell), GTK + GJS (AGS) oder einer WebView. Jede
öffnet einen OpenGL-Kontext, und unter NVIDIA blendet jeder GL-Kontext den Treiber ein: etwa **100 bis 160 MB
pro Prozess**, bevor ein Pixel erscheint. Mist begann als Quickshell-Konfiguration (428 MB, 9,5 % CPU) und
wurde Oberfläche für Oberfläche neu geschrieben:

- **Auf der CPU gezeichnete Oberflächen.** `mist-ui` malt mit [tiny-skia] und [swash] in `wl_shm`-Puffer;
  Hyprland setzt sie zusammen und macht sie unscharf. Neu gezeichnet werden nur geänderte Bereiche.
- **Ereignisgesteuert.** Daten kommen über den Hyprland-Socket, D-Bus, `pactl subscribe`, inotify und
  dauerhaft laufende `mistd`-Backends. Bei ruhendem Bildschirm fragt nichts per Timer ab; versteckte
  Oberflächen arbeiten nicht.
- **Ein GPU-Prozess, nur bei Bedarf.** `mist-wall` (EGL, GLSL 150) zeichnet animierte Hintergründe und den
  Wetterhimmel. Bei einem Foto beendet er sich, und `mist-ui` zeichnet das Foto selbst.

Ergebnis auf dem Rechner des Autors: **~87 bis 106 MB PSS für die ganze Shell**, 0,6 % eines Kerns bei
laufender Musik. Siehe [docs/BENCHMARKS.md](../BENCHMARKS.md) (Englisch).

## Funktionen

| | |
|---|---|
| **Obere Leiste** | aktive App und Titel, Systray (StatusNotifierItem-Host + dbusmenu), Netzwerk, Bluetooth, Audio, CPU / Speicher, Uhr, Kalender |
| **Dock** | angeheftete und laufende Apps, Vergrößerung, Fensterauswahl mit Live-Vorschau (`hyprland_toplevel_export_v1`), Klick auf die aktive App minimiert sie |
| **Dynamic Island** | Medien (MPRIS) mit Equalizer, Timer, Anrufe (KDE Connect / ADB), Benachrichtigungen, Lautstärke |
| **Desktop** | Uhr, Sonnenauf- / -untergang, Glaskarten (Wetter, Garmin-Bereitschaft, Laufplan), Einstellungsblatt pro Widget |
| **Panels** | Spotlight-artige Suche, Start, Cmd+Tab-Umschalter, Kontrollzentrum, Mitteilungszentrale, Zwischenablage-Verlauf, Emoji, Sitzungsmenü, Hintergrundgalerie, Screenshot-Leiste |
| **System** | Benachrichtigungsserver, Polkit-Agent, Sperrbildschirm (`ext-session-lock-v1` + PAM), Leerlauf (`ext-idle-notify-v1`, logind), Fenster-Einrasten wie in Windows 11 |
| **Hintergründe** | Fotos (CPU) oder Live-GLSL-Szenen und ein Echtzeit-Wetterhimmel (`mist-wall`) |
| **mist-comp** *(experimentell)* | eigener Smithay-Compositor + Greeter, um Mist ohne Hyprland zu starten |

## Architektur

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust, CPU (tiny-skia + swash), wl_shm, ein Prozess für alle Oberflächen
                           │      ├── mistd stats | phone | calls | island | clipstore   (Ereignis-Backends)
                           │      └── IPC: mist-ui-bin msg <Ziel> <Funktion> [Argumente]
                           └── mist-wall    Rust, EGL / GLSL 150, nur Live-Hintergründe und Himmel
```

| Pfad | Inhalt |
|---|---|
| `native/mist-ui` | alle Oberflächen der Shell ([native/README.md](../../native/README.md)) |
| `native/mist-wall` | GPU-Prozess für den Hintergrund |
| `native/mist-comp` | experimenteller Compositor, Sitzung und Greeter |
| `native/common` | gemeinsamer Code von mist-ui und mist-wall (Astronomie, Presets) |
| `scripts/mistd` | Backends: Systemwerte, Telefon (KDE Connect, ADB), Anrufe, Zwischenablage, Dateimanager-D-Bus |
| `shaders/` | Shader für Hintergründe und Himmel (`.frag`-Quellen, `.qsb`-Pakete für mist-wall) |
| `*.qml`, `components/` | die ursprüngliche Quickshell-Version, als Referenz behalten; wird nicht mehr gestartet |

## Installation

Mist ist der Alltags-Desktop einer Person: Arch / CachyOS, Hyprland 0.55+ mit Lua-Konfiguration,
NVIDIA- oder Mesa-GPU. Rechne damit, Code zu lesen.

**Build:** `rust` (stable), `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa` (EGL-Header); für
`mist-comp` zusätzlich `libinput`, `libseat`, `libdrm`, `mesa` (gbm), `systemd-libs` (udev).

**Laufzeit:** `hyprland`, `pipewire` + `pipewire-pulse` (`pactl`, `pw-play`, `parec`), `wireplumber`
(`wpctl`), `networkmanager` (`nmcli`), `wl-clipboard`, `grim`, `slurp`, `polkit`, `qt6-shadertools`
(`qsb`, nur Live-Hintergründe), `noto-fonts`, `noto-fonts-emoji`. Optional: `ddcutil` (Helligkeit über
DDC/CI), `kdeconnect` / `android-tools` (Telefon), `wf-recorder` (Bildschirmaufnahme).

**Schrift:** Die Oberfläche ist für SF Pro gestaltet (variables `SF-Pro.ttf` in
`~/.local/share/fonts/SF-Pro/`). Apple lizenziert sie nur für eigene Plattformen, Mist liefert sie nicht mit.
Ohne sie nutzt Mist Inter Variable, dann Noto Sans.

```bash
git clone <dieses Repo> ~/.config/quickshell/mist
```

```bash
~/.config/quickshell/mist/scripts/mistd/install.sh
```

```bash
~/.config/quickshell/mist/native/install.sh
```

Der Klon muss in `~/.config/quickshell/mist` liegen (Pfade sind noch fest, siehe
[docs/PUBLISHING.md](../PUBLISHING.md)). `native/install.sh` installiert `mist-ui-bin`, `mist-wall-bin`,
`mist-comp` und beim ersten Mal den Wächter `~/.local/bin/mist-ui` nach `~/.local/bin`.

Danach [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) in die Hyprland-Konfiguration
aufnehmen (Autostart, Tastenkürzel, Blur-Regeln) und andere Leisten, Benachrichtigungsdienste und
Polkit-Agenten entfernen.

## Bedienung

| Taste | Aktion |
|---|---|
| `Super` | Start |
| `Super + Leertaste` | Spotlight |
| `Super + Tab` | App-Umschalter |
| `Super + I` | Kontrollzentrum |
| `Super + A` | Mitteilungszentrale |
| `Super + V` | Zwischenablage-Verlauf |
| `Super + .` | Emoji |
| `Super + L` | Sperren |
| `Print`, `Super + Shift + S` | Screenshot (Bereich) |

Alles ist skriptbar: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`,
`mist-ui-bin msg dock click firefox`. Log: `~/.cache/mist/ui.log`. Einstellungen: `~/.config/mist/`.

## Entwicklung

```bash
cd native && cargo build --release && cargo test --release
```

Prüfungen, ohne die laufende Sitzung anzufassen:

| Befehl | Ergebnis |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | obere Leiste als PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | Desktop-Bühne + Karten / Dock |
| `mist-ui --test` | zweite Leiste über der echten auf HDMI-A-1 |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | ein Bild des Live-Hintergrunds |
| `mist-ui-bin msg ui stats` | Aufwachvorgänge, Zeichnungen und Zeichenzeit pro Oberfläche |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | PSS / CPU der laufenden Shell im Leerlauf |

`lto = false` beibehalten (Arch-rustc mit System-LLVM stürzt mit LTO ab). Siehe
[CONTRIBUTING.md](../../CONTRIBUTING.md).

## Stand

Alltags-Desktop einer Person, zwei Monitore, NVIDIA. Alles aus der Funktionstabelle ist nativ und täglich
in Gebrauch; `mist-comp` ist experimentell. Die Texte der Oberfläche sind vorerst polnisch (was vor einer
öffentlichen Veröffentlichung fehlt: [docs/PUBLISHING.md](../PUBLISHING.md)).

## Dank

[Hyprland], [Smithay] und [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus], [Tabler Icons]
(MIT). Die ursprüngliche QML-Version entstand mit [Quickshell]; [Noctalia] diente als Rückfallebene und
Vorbild.

*Mist steht in keiner Verbindung zu Apple. macOS, Dynamic Island, Spotlight und SF Pro sind Marken von
Apple Inc. und werden hier nur zur Beschreibung der Inspiration genannt.*

## Lizenz

[GPL-3.0](../../LICENSE).

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
