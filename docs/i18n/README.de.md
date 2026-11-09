<div align="center">

<img src="../img/banner.svg" alt="Mist" width="100%">

**Eine von macOS inspirierte Desktop-Shell für Wayland, in Rust geschrieben und auf der CPU gezeichnet.**

[![CI](https://img.shields.io/github/actions/workflow/status/wiktor-jozwiak/mist/ci.yml?branch=main&style=flat-square&label=CI)](../../.github/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wiktor-jozwiak/mist?style=flat-square&color=3b82f6)](https://github.com/wiktor-jozwiak/mist/releases)
[![Lizenz: GPL-3.0](https://img.shields.io/badge/lizenz-GPL--3.0-8d8590?style=flat-square)](../../LICENSE)
[![Discord](https://img.shields.io/badge/discord-chat-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/4rRabwQTPE)

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/desktop-en.webp" alt="Der Mist-Desktop: obere Leiste, Uhr, Wetter-, Bereitschafts- und Laufkarten, Dock" width="100%">

<table>
  <tr>
    <td width="50%"><img src="../img/island.gif" alt="Dynamic Island: eine Benachrichtigung, die Lautstärke, gleich Regen, der nächste Termin" width="100%"></td>
    <td width="50%"><img src="../img/weather.gif" alt="Die Wetterkarte im Regen über ihrem Live-Himmel" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Dynamic Island: Benachrichtigungen, Lautstärke, Regen in der nächsten Stunde, nächster Termin</sub></td>
    <td align="center"><sub>Wetterkarte über einem in Echtzeit gezeichneten GLSL-Himmel</sub></td>
  </tr>
</table>

<a href="https://youtu.be/mwisDtz0Yd0"><img src="../img/demo-poster.webp" alt="Mist demo video (40 s)" width="720"></a>

<sub>40-s-Demo, aufgenommen in einer Headless-<code>mist-comp</code>-Sitzung mit Demodaten. Musik: <i>Trap Beat</i> von JonasBlakewood (Pixabay) · <a href="https://github.com/user-attachments/assets/ed8137a5-daf5-488e-8515-686aa54e6e07">MP4</a></sub>

</div>

Mist ist Leiste, Dock, Dynamic Island, Desktop-Widgets, Suche, Kontrollzentrum, Benachrichtigungen,
Sperrbildschirm und Polkit-Agent eines Desktops, in einem einzigen Prozess mit ~50 MB, der nie einen
GPU-Kontext öffnet. Es läuft auf Hyprland oder auf seinem eigenen Compositor, `mist-comp`.

Angefangen hat es als Quickshell-Konfiguration mit 428 MB und 9,5 % CPU im Leerlauf. Seitdem wurde es Fläche
für Fläche nach drei Regeln neu geschrieben:

- **Auf der CPU zeichnen.** `mist-ui` malt `wl_shm`-Puffer mit [tiny-skia] und [swash] und zeichnet nur neu,
  was sich geändert hat. Auf NVIDIA kostet jeder OpenGL-Kontext 100 bis 160 MB vor dem ersten Pixel; Mist hat
  keinen.
- **Auf Ereignisse warten.** Alles kommt aus dem Compositor-Socket, D-Bus, `pactl subscribe` und inotify.
  Nichts fragt im Leerlauf zyklisch ab, und verdeckte Flächen arbeiten nicht.
- **Ein GPU-Prozess, nur wenn er etwas zeigt.** `mist-wall` zeichnet Live-Hintergründe und den Wetterhimmel;
  bei einem Foto beendet er sich und `mist-ui` zeichnet das Foto.

> [!NOTE]
> Mist ist der tägliche Desktop einer Person (Arch / CachyOS, zwei Monitore, NVIDIA). Es ist öffentlich, weil
> es funktioniert, nicht weil es fertig ist: Rechne damit, Code zu lesen.

## Funktionen

<table>
  <tr>
    <td width="50%"><img src="../img/panels/spotlight.webp" alt="Suche im Stil von Spotlight" width="100%"></td>
    <td width="50%"><img src="../img/panels/settings.webp" alt="Einstellungsfenster" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Spotlight: Apps, Dateien, Befehle, Emoji, Web</sub></td>
    <td align="center"><sub>Einstellungen: alles in einem Fenster, sofort übernommen</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/control.webp" alt="Kontrollzentrum" width="100%"></td>
    <td width="50%"><img src="../img/panels/calendar.webp" alt="Kalender" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Kontrollzentrum: Netzwerk, Bluetooth, Telefon, Ton, Nachtlicht</sub></td>
    <td align="center"><sub>Kalender: Monat, Agenda, neue Termine</sub></td>
  </tr>
</table>

| | |
|---|---|
| **Obere Leiste** | aktive App, Systemablage (StatusNotifierItem + dbusmenu), Netzwerk, Bluetooth, Audio, CPU und Speicher, Uhr |
| **Dock** | angeheftete und laufende Apps, Vergrößerung, Fensterauswahl mit Live-Vorschau |
| **Dynamic Island** | Medien mit Equalizer, Timer, Anrufe (KDE Connect oder ADB), Benachrichtigungen, Lautstärke |
| **Desktop** | Uhr und Glaskarten: Wetter, Garmin-Bereitschaft, Laufen, Musik, Kalender, System, Weltuhren |
| **Panels** | Spotlight, Start, App-Umschalter, Kontrollzentrum, Mitteilungszentrale, Zwischenablage, Emoji, Sitzungsmenü, Bildschirmfotos |
| **Schnelle Antworten** | in Spotlight beim Tippen: Rechnen (`128*42`), Einheiten (`212 f to c`, `2 gb to mb`), Zeitzonen (`tokyo time`) |
| **Telefon** | KDE Connect: Akku, Benachrichtigungen, SMS und Kontakte in einem Panel, Anrufe in der Island |
| **Einstellungen** | Sprache, Wetterort, Hintergrund, Desktop-Layout, Widgets, Nachtlicht, Sperre und Ruhezustand, Benachrichtigungen |
| **System** | Benachrichtigungsserver, Polkit-Agent, Sperrbildschirm (PAM), Leerlauf und Ruhezustand |
| **Apps** | Mist Files (auch der Dateiauswahldialog jeder App), Mist Player (libmpv), Systemeinstellungen |
| **Andocken** | wie Windows 11: an Rand oder Ecke ziehen, Snap Layouts am grünen Knopf oder mit `Super + Z`, Snap Assist |
| **Hintergründe** | Fotos, Live-Szenen in GLSL oder ein Himmel, der dem aktuellen Wetter folgt |

Das **Wetter** kommt ohne Konto von [Open-Meteo](https://open-meteo.com): Stundenkurve, vier Tage, Regen in der
nächsten Stunde, Sonne und Mond. **Garmin-Bereitschaft** und **Laufen** zeigen deine eigenen Garmin-Connect-Daten,
die `mistd` in eine lokale SQLite-Datei synchronisiert; die Karten bleiben verborgen, bis du dich einmal
anmeldest ([Anleitung](../guides/garmin.md)).

Die Oberfläche spricht **Deutsch, Englisch, Polnisch, Spanisch und Französisch**, nach `$LANG` oder der Wahl in
den Einstellungen. Eine Sprache ist eine JSON-Datei in [native/assets/i18n](../../native/assets/i18n).

## Messungen

<p align="center"><img src="../img/memory.svg" alt="Speicher im Leerlauf (PSS), jede Shell allein: swaybar 7 MB, Waybar 32 MB, Mist ohne mist-wall 38 MB, nwg-panel 56 MB, Mist mit Live-Hintergrund 132 MB, Ironbar 137 MB, leeres Quickshell 142 MB, Noctalia 177 MB" width="100%"></p>

Jede Shell allein in einem frischen headless sway auf der GPU (RTX 4060, zwei 1080p-Ausgänge), mit der
Konfiguration aus ihrem Paket, 3 Läufe, Median ([bench/shells.sh](../../bench/shells.sh)). *Leerlauf*: eine Minute passiert nichts.
*Aktiv*: alle 2 s ein Arbeitsflächenwechsel, alle 6 s ein Fenster, alle 10 s eine Mitteilung. CPU in % eines Kerns.

| Shell | Was sie zeichnet | Speicher (PSS) | GPU-Speicher | CPU Leerlauf | CPU aktiv |
|---|---|---|---|---|---|
| swaybar (sway 1.12) | Leiste | 7 MB | – | 0 % | 0,03 % |
| Waybar 0.15 | Leiste | 32 MB | – | 0,03 % | 0,38 % |
| **Mist ohne `mist-wall`** | Leiste, Dock, Island, Desktop-Widgets, Mitteilungen | **38 MB** | – | 0,19 % | 0,72 % |
| nwg-panel 0.12 | Leiste | 56 MB | – | 0,10 % | 0,22 % |
| **Mist** | all das + ein Live-GLSL-Hintergrund | 132 MB | 42 MiB | 0,83 % | 1,35 % |
| Ironbar 0.19 | Leiste | 137 MB | 9 MiB | 0,37 % | 0,43 % |
| Quickshell 0.3, ein leeres `PanelWindow` | eine leere Leiste | 142 MB | 6 MiB | 0 % | 0 % |
| Noctalia 5.4 | Leiste, Foto-Hintergrund (und ihr Einrichtungsdialog) | 177 MB | 60 MiB | 0,12 % | 4,50 % |

Compositoren, headless auf derselben GPU, 1920×1080 60 Hz, 5 Läufe ([bench/compositors.sh](../../bench/compositors.sh)).
Ohne etwas auf dem Bildschirm brauchen alle 0 % CPU:

| Compositor | Speicher (PSS) | CPU, ein Client, der jedes Bild auf der CPU zeichnet | CPU, ein OpenGL-Client im Vollbild | CPU pro OpenGL-Bild |
|---|---|---|---|---|
| **mist-comp** | 100 MB | **1,7 %** | 1,3 % | 0,22 ms |
| weston 15 | 101 MB | 2,7 % | **0,6 %** | **0,11 ms** |
| labwc 0.20 | 109 MB | 6,1 % | 6,0 % | 1,0 ms |
| sway 1.12 | 103 MB | 33,5 % | 6,6 % | 1,1 ms |

Hyprland und niri haben keinen headless-Modus, den ein Skript starten kann, und fehlen deshalb. Methode, jeder
Lauf, Zahlen pro Prozess und die früheren Messungen in der echten Sitzung: [docs/BENCHMARKS.md](../BENCHMARKS.md).

## mist-comp

Mists eigener Wayland-Compositor, rund 14 000 Zeilen Rust auf [Smithay], mit Anmeldebildschirm. Er startet
dieselbe Shell und beantwortet genug von Hyprlands IPC, dass `hyprctl` und Mists Skripte unverändert
funktionieren. Er ist experimentell und neben Hyprland die tägliche Sitzung des Autors.

- **Fenster:** Mist-Titelleisten oder die der App, runde Ecken und Schatten, Glasunschärfe unter
  durchscheinenden Fenstern und Panels, Andocken wie in Windows 11, Overview (`Super + O`), eine Lupe,
  Arbeitsflächenwechsel, der den Fingern folgt, Win + D zeigt den Desktop.
- **Spart sich, was er auslassen kann:** Ein Monitor zeichnet nur neu, wenn sich darauf etwas geändert hat;
  das Glas wird nur neu weichgezeichnet, wenn sich dahinter etwas bewegt hat; ein verdecktes Fenster bekommt
  ein Bild pro Sekunde statt sechzig.
- **Sitzung:** DRM / KMS über logind, Hotplug, VRR, Explicit Sync, XWayland, Bildschirmfreigabe über das Portal,
  etwa 45 Protokolle. Nach einem Absturz kommen die Fenster auf ihre Arbeitsflächen zurück;
  `mist-session restart` lädt eine neue Version ohne Abmelden.
- **Getestet:** 66 verschachtelte Tests mit Headless-Backend, in der CI auf dem Software-Renderer.

Noch nicht da: Kacheln, Tearing-Steuerung, geprüftes Direct Scan-out auf NVIDIA. Konfiguration:
[docs/CONFIG.md](../CONFIG.md#compconf-outputs-mist-comp-only); `mist-comp migrate` übernimmt eine Hyprland-Konfiguration.

## Installation

Arch: [packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) baut `mist-shell-git`.

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

Aus einem Klon, an beliebiger Stelle:

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

Das erste Skript baut `mistd`, das zweite legt `mist-ui-bin`, `mist-wall-bin` und `mist-comp` nach
`~/.local/bin`. Fertige Binärdateien hängen an jedem [Release](https://github.com/wiktor-jozwiak/mist/releases).

- **Unter Hyprland:** [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) in die eigene Konfiguration
  laden und andere Leisten, Benachrichtigungsdienste und Polkit-Agenten entfernen.
- **Unter mist-comp:** `native/mist-comp/session/install-session.sh --greeter` fügt dem Anmeldebildschirm eine
  Mist-Sitzung hinzu; Hyprland bleibt.

<details>
<summary>Abhängigkeiten</summary>

<br>

**Bauen:** `rust`, `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa`; für `mist-comp` zusätzlich `libinput`,
`libseat`, `libdrm`, `systemd-libs`.

**Laufzeit:** `pipewire-pulse`, `wireplumber`, `networkmanager`, `wl-clipboard`, `grim`, `slurp`, `polkit`,
`noto-fonts`, `noto-fonts-emoji`. Optional: `ddcutil` (Monitorhelligkeit), `kdeconnect` oder `android-tools`
(Telefon). Was jedes davon einschaltet: [docs/CONFIG.md](../CONFIG.md#optional-integrations).

**Schrift:** Die Oberfläche ist für SF Pro gezeichnet, die Mist nicht mitliefern darf. Ohne sie nimmt Mist
Inter, dann Noto Sans.

</details>

## Bedienung

| Taste | Aktion | | Taste | Aktion |
|---|---|---|---|---|
| `Super` | Start | | `Super + A` | Mitteilungszentrale |
| `Super + Space` | Spotlight | | `Super + V` | Zwischenablage |
| `Super + Tab` | App-Umschalter | | `Super + .` | Emoji |
| `Super + I` | Kontrollzentrum | | `Super + L` | Sperren |
| `Super + Shift + S` | Bildschirmfoto | | Rechtsklick auf den Desktop | Einstellungen |

Alles ist skriptbar: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`. Die Einstellungen
liegen in `~/.config/mist/`, jede Datei ist in [docs/CONFIG.md](../CONFIG.md) beschrieben; das Log ist
`~/.cache/mist/ui.log`.

## Entwicklung

```bash
cd native && cargo build --release && cargo test --release
```

Jede Fläche lässt sich offscreen rendern, Änderungen sind also prüfbar, ohne die laufende Sitzung anzufassen:
`mist-ui-bin --pane-png spotlight 1920 1080 out.png`, `mist-ui-bin --desk-png dir`,
`mist-wall-bin --png out.ppm dunes 1920 1080`. `native/mist-comp/tests/run-all.sh` führt die Compositor-Tests
in einem verschachtelten Fenster aus. Mehr in [native/README.md](../../native/README.md) und
[CONTRIBUTING.md](../../CONTRIBUTING.md).

| Pfad | Inhalt |
|---|---|
| `native/mist-ui` | alle Flächen der Shell |
| `native/mist-wall` | der GPU-Prozess für Hintergründe |
| `native/mist-comp` | Compositor, Sitzung und Anmeldebildschirm |
| `scripts/mistd` | Backends: Systemwerte, Telefon, Anrufe, Zwischenablage, Garmin-Synchronisierung |
| `shaders/` | Shader für Hintergründe und Himmel |
| Tag `legacy-qml` | die ursprüngliche Quickshell-Version |

## Dank

Gebaut auf [Smithay], [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus] und [Tabler Icons].
Die erste Version lief auf [Quickshell], mit [Noctalia] als Vorbild.

<sub>Mist steht in keiner Verbindung zu Apple. macOS, Dynamic Island, Spotlight und SF Pro sind Marken von Apple Inc.
und werden hier nur genannt, um die Inspiration zu beschreiben.</sub>

[GPL-3.0](../../LICENSE)

[tiny-skia]: https://github.com/linebender/tiny-skia
[swash]: https://github.com/dfrg/swash
[resvg]: https://github.com/linebender/resvg
[zbus]: https://github.com/dbus2/zbus
[Smithay]: https://github.com/Smithay/smithay
[smithay-client-toolkit]: https://github.com/Smithay/client-toolkit
[Tabler Icons]: https://tabler.io/icons
[Quickshell]: https://quickshell.org
[Noctalia]: https://github.com/noctalia-dev/noctalia
