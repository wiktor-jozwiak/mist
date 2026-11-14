<div align="center">

<img src="../img/logo.svg" alt="" width="104">

# Mist

**Eine Desktop-Shell und ein Wayland-Compositor für Linux, in Rust geschrieben und auf der CPU gezeichnet.**

[![CI](https://img.shields.io/github/actions/workflow/status/wiktor-jozwiak/mist/ci.yml?branch=main&style=flat-square&label=CI)](../../.github/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wiktor-jozwiak/mist?style=flat-square&color=3b82f6)](https://github.com/wiktor-jozwiak/mist/releases)
[![Stars](https://img.shields.io/github/stars/wiktor-jozwiak/mist?style=flat-square&color=e2a03f)](https://github.com/wiktor-jozwiak/mist/stargazers)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-8d8590?style=flat-square)](../../LICENSE)
[![Discord](https://img.shields.io/badge/discord-chat-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/4rRabwQTPE)

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/desktop-en.webp" alt="Der Mist-Desktop: obere Leiste, Uhr, Wetter-, Bereitschafts- und Laufkarten, Dock" width="100%">

<table>
  <tr>
    <td align="center" width="33%"><h3>44 MB</h3><sub>Speicher der ganzen Shell im Leerlauf</sub></td>
    <td align="center" width="33%"><h3>0</h3><sub>OpenGL-Kontexte im Shell-Prozess</sub></td>
    <td align="center" width="33%"><h3>8 / s</h3><sub>Aufwachvorgänge im Leerlauf, ohne Polling</sub></td>
  </tr>
</table>

**[Rundgang](#rundgang) · [Was du bekommst](#was-du-bekommst) · [Warum es leicht ist](#warum-es-leicht-ist) · [mist-comp](#mist-comp) ·
[Installation](#installation) · [Tasten](#tasten-und-skripte) · [Entwicklung](#entwicklung)**

</div>

Mist besteht aus zwei Programmen, und jedes funktioniert ohne das andere:

| | Was es ist | Wo es läuft |
|---|---|---|
| **Die Shell** (`mist-ui`, `mist-wall`) | Leiste, Dock, Dynamic Island, Desktop-Widgets, Suche, Kontrollzentrum, Benachrichtigungen, Sperrbildschirm, Polkit-Agent, ein Dateimanager und ein Videoplayer, in einem Prozess | Hyprland, mist-comp oder [ein Fenster](#2-in-einem-fenster-ausprobieren) auf dem Desktop, den du schon benutzt |
| **Der Compositor** ([`mist-comp`](#mist-comp)) | ein Wayland-Compositor auf Smithay, mit eigenem Anmeldebildschirm | eine eigene Sitzung, neben Hyprland |

Das Aussehen lehnt sich bewusst an macOS an; darunter ist es ein Linux-Desktop.

> [!NOTE]
> Mist ist der tägliche Desktop einer Person (Arch / CachyOS, zwei Monitore, NVIDIA). Es ist öffentlich, weil
> es funktioniert, nicht weil es fertig ist: Rechne damit, Code zu lesen.

## Rundgang

<div align="center">

<img src="../img/island.webp" alt="Dynamic Island: eine Benachrichtigung, die Lautstärke, gleich Regen, der nächste Termin" width="100%">

<sub>Dynamic Island: Benachrichtigungen, Lautstärke, Regen in der nächsten Stunde, der nächste Termin</sub>

<br><br>

<table>
  <tr>
    <td width="50%" align="center"><img src="../img/weather.webp" alt="Die Wetterkarte im Regen über ihrem Live-Himmel" width="100%"><br><sub>Wetterkarte über einem Live-Himmel, der dem Wetter und der Uhrzeit folgt</sub></td>
    <td width="50%" align="center"><img src="../img/wallpaper.webp" alt="Die Dünen, ein Live-Hintergrund in GLSL" width="100%"><br><sub>Live-Hintergründe in GLSL, Fotos oder der Himmel des aktuellen Wetters</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/start.webp" alt="Start: Suchfeld, Lieblings-Apps, der heutige Kalender, das Telefon" width="100%"><br><sub>Start: Apps, Dateien, Befehle, der heutige Tag und das Telefon</sub></td>
    <td align="center"><img src="../img/panels/spotlight.webp" alt="Spotlight beantwortet 128*42" width="100%"><br><sub>Spotlight: Apps, Dateien, Befehle, Rechnen, Einheiten, Zeitzonen, Emoji, Web</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/files.webp" alt="Mist Files: ein Ordner in der Detailansicht" width="100%"><br><sub>Mist Files, auch der Dateidialog jeder App</sub></td>
    <td align="center"><img src="../img/panels/player.webp" alt="Mist Player: die Bedienelemente über einem Video" width="100%"><br><sub>Mist Player auf libmpv: Kapitel, Spuren, Bild im Bild</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/settings.webp" alt="Einstellungen, die Seite Personalisierung" width="100%"><br><sub>Einstellungen: jede Option in einem Fenster, sofort angewendet</sub></td>
    <td align="center"><img src="../img/panels/control.webp" alt="Kontrollzentrum" width="100%"><br><sub>Kontrollzentrum: Netzwerk, Bluetooth, Telefon, Ton, Nachtlicht</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/calendar.webp" alt="Kalender" width="100%"><br><sub>Kalender: Monat, Agenda, neue Termine</sub></td>
    <td align="center"><img src="../img/panels/emoji.webp" alt="Die Emoji-Auswahl" width="100%"><br><sub>Emoji-Auswahl mit Suche und Kategorien</sub></td>
  </tr>
</table>

<img src="../img/themes.webp" alt="Das Kontrollzentrum in den sieben eingebauten Themes" width="100%">

<sub>Mist, Catppuccin, Nord, Gruvbox, Tokyo Night, Rosé Pine, Dracula oder eine eigene JSON-Datei. Glas, Fenster und Akzent wechseln live.</sub>

<br><br>

<table>
  <tr>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-files.svg" alt="" width="88"><br><b>Mist Files</b><br><sub>Tabs, Vorschau, schlaue Suche, die Dateiauswahl des Systems</sub></td>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-player.svg" alt="" width="88"><br><b>Mist Player</b><br><sub>libmpv, MPRIS, Bild-in-Bild, gleichmäßige Bildfolge</sub></td>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-settings.svg" alt="" width="88"><br><b>Einstellungen</b><br><sub>Anzeige, Ton, Netzwerk, Desktop, Widgets, Sprache</sub></td>
  </tr>
</table>

<br>

<a href="https://youtu.be/mwisDtz0Yd0"><img src="../img/demo-poster.webp" alt="Mist demo video (40 s)" width="720"></a>

<sub>40-s-Demo, aufgenommen in einer headless <code>mist-comp</code>-Sitzung mit Demodaten. Musik: <i>Trap Beat</i> von JonasBlakewood (Pixabay) · <a href="https://github.com/user-attachments/assets/ed8137a5-daf5-488e-8515-686aa54e6e07">MP4</a></sub>

</div>

## Was du bekommst

**Die Shell**

- **Obere Leiste:** aktive App, Systemleiste (StatusNotifierItem + dbusmenu), Netzwerk, Bluetooth, Ton,
  CPU und Speicher, Uhr.
- **Dock:** angeheftete und laufende Apps, Vergrößerung, eine Fensterauswahl mit Live-Vorschau.
- **Dynamic Island:** Medien mit Equalizer, Timer, Anrufe, Benachrichtigungen, Lautstärke, Caps Lock und ein
  Hinweis, wenn eine Konfigurationsdatei einen Fehler hat (mit Zeile und Spalte).
- **Panels:** Start, Spotlight, App-Umschalter, Kontrollzentrum, Benachrichtigungszentrale, Kalender,
  Zwischenablage-Verlauf, Emoji, Sitzungsmenü. Suchfelder verhalten sich wie jedes Textfeld: Cursor, Pfeile,
  Auswahl mit Shift und Strg+A.
- **Bildschirmfotos:** ein Bereich, gewählt über dem eingefrorenen Bildschirm, eine Werkzeugleiste, Text vom
  Bildschirm (OCR), eine Farbpipette.
- **Fenster einrasten** wie in Windows 11: an einen Rand oder eine Ecke ziehen, Snap Layouts am grünen Knopf
  oder `Super + Z`, Snap Assist.
- **Sperrbildschirm** auf PAM, mit eigenem Uhrstil, Unschärfe und Abdunklung.

**Der Desktop**

- **Uhr und Glaskarten:** Wetter, Garmin-Bereitschaft, Laufen, Musik, Kalender, System, Weltuhren, eine Notiz
  mit Aufgaben, der Fortschritt des Tages, Sonne und Mond, ein Bilderrahmen, Gerätebatterien. Die Garmin-Karten
  zeigen deine eigenen Garmin-Connect-Daten, synchronisiert in eine lokale SQLite-Datei, und bleiben verborgen,
  bis du dich einmal anmeldest ([Anleitung](../guides/garmin.md)).
- **Wetter** von [Open-Meteo](https://open-meteo.com), ohne Konto: Stundenkurve, vier Tage, Regen in der
  nächsten Stunde, der Tag von Sonne und Mond. Der Himmel der Karte wird live für das Wetter und die Stunde
  gezeichnet, auf die du zeigst, über einer von fünf Landschaften (Felder, ein See, Berge, eine Stadt, das Meer).
- **Hintergründe:** Fotos, Live-GLSL-Szenen oder der Himmel des aktuellen Wetters.
- **Themes:** Mist, Catppuccin, Nord, Gruvbox, Tokyo Night, Rosé Pine, Dracula oder eine eigene JSON-Datei.
  Glas, Fenster und Akzent wechseln live in der Shell und in Files, Player und Einstellungen.

**Was es ersetzt**

- **Dienste:** der Benachrichtigungsserver, der Polkit-Agent, der Sperrbildschirm, Leerlauf und Ruhezustand.
- **Apps:** Mist Files, Mist Player und Einstellungen (Anzeige, Ton, Bluetooth, Netzwerk, Energie, Apps,
  Hintergrund, Desktop und Widgets, Sperrbildschirm, Benachrichtigungen, Sprache).
- **Telefon** über KDE Connect: Akku, Benachrichtigungen, SMS und Kontakte in einem Panel, Anrufe in der Insel
  (ADB als Rückfall für Anrufe).

Die Oberfläche spricht **Deutsch, Englisch, Polnisch, Spanisch und Französisch**, nach `$LANG` oder der Wahl in
den Einstellungen. Eine Sprache ist eine JSON-Datei in [native/assets/i18n](../../native/assets/i18n).

## Warum es leicht ist

Angefangen hat Mist als Quickshell-Konfiguration mit 428 MB und 9,5 % CPU im Leerlauf. Es wurde Fläche für
Fläche nach drei Regeln neu geschrieben:

- **Auf der CPU zeichnen.** `mist-ui` malt `wl_shm`-Puffer mit [tiny-skia] und [swash] und zeichnet nur neu,
  was sich geändert hat. Auf NVIDIA kostet jeder OpenGL-Kontext 100 bis 160 MB vor dem ersten Pixel; `mist-ui`
  hat keinen.
- **Auf Ereignisse warten.** Der Zustand kommt aus dem Compositor-Socket, D-Bus, `pactl subscribe` und
  inotify, und verdeckte Flächen arbeiten nicht. Die Shell wacht im Leerlauf 8-mal pro Sekunde auf.
- **Ein GPU-Prozess, nur solange er etwas zeigt.** `mist-wall` zeichnet Live-Hintergründe, den Himmel der
  Wetterkarte und die Hintergrundgalerie. Mit einem Foto als Hintergrund und ohne diese beendet er sich, und
  `mist-ui` zeichnet das Foto.

<p align="center"><img src="../img/memory.svg" alt="Speicher im Leerlauf (PSS), jede Shell allein, von swaybar mit 6 MB bis Noctalia mit 192 MB: Mist mit Foto-Hintergrund 44 MB, Mist mit Live-Hintergrund 144 MB" width="100%"></p>

<details>
<summary><b>Die Zahlen</b>: Shells und Compositoren, gleich gemessen</summary>

<br>

**Shells.** Jede allein in einem frischen headless sway auf der GPU (RTX 4060, zwei 1080p-Ausgänge), mit der
Konfiguration aus ihrem Paket; 3 Läufe, Median ([bench/shells.sh](../../bench/shells.sh)). *Leerlauf*: eine
Minute passiert nichts. *Aktiv*: alle 2 s ein Arbeitsflächenwechsel, alle 6 s ein neues Fenster, alle 10 s
eine Benachrichtigung. CPU in % eines Kerns.

| Shell | Was sie zeichnet | Speicher (PSS) | GPU-Speicher | CPU Leerlauf | CPU aktiv |
|---|---|---|---|---|---|
| swaybar (sway 1.12) | Leiste | 6 MB | – | 0 % | 0,03 % |
| Waybar 0.15 | Leiste | 34 MB | – | 0,03 % | 0,38 % |
| **Mist, Foto-Hintergrund** | Leiste, Dock, Island, Desktop-Widgets, Benachrichtigungen, Hintergrund | **44 MB** | – | 0,14 % | 0,66 % |
| nwg-panel 0.12 | Leiste | 59 MB | – | 0,09 % | 0,23 % |
| **Mist, Live-Hintergrund** | all das, der Hintergrund eine Live-Szene in GLSL | 144 MB | 50 MiB | 0,79 % | 1,34 % |
| Ironbar 0.19 | Leiste | 158 MB | 9 MiB | 0,35 % | 0,42 % |
| Quickshell 0.3, ein leeres `PanelWindow` | eine leere Leiste | 161 MB | 6 MiB | 0 % | 0 % |
| Noctalia 5.4 | Leiste, Foto-Hintergrund (und ihr Einrichtungsdialog) | 192 MB | 60 MiB | 0,10 % | 4,01 % |

**Compositoren.** Headless auf derselben GPU, 1920×1080 60 Hz, 5 Läufe
([bench/compositors.sh](../../bench/compositors.sh)). Ohne etwas auf dem Bildschirm brauchen alle 0 % CPU.

| Compositor | Speicher (PSS) | CPU, ein Client, der jedes Bild auf der CPU zeichnet | CPU, ein OpenGL-Client im Vollbild | CPU pro OpenGL-Bild |
|---|---|---|---|---|
| **mist-comp** | 128 MB | **2,2 %** | 1,5 % | 0,25 ms |
| weston 15 | 129 MB | 3,3 % | **0,8 %** | **0,13 ms** |
| labwc 0.20 | 137 MB | 8,1 % | 8,9 % | 1,5 ms |
| sway 1.12 | 129 MB | 33,6 % | 8,3 % | 1,4 ms |

Hyprland und niri haben keinen headless-Modus, den ein Skript starten kann, und fehlen deshalb in der Tabelle.
Methode, jeder Lauf, Aufwachvorgänge, Zahlen pro Prozess und die früheren Messungen in der echten Sitzung:
[docs/BENCHMARKS.md](../BENCHMARKS.md).

</details>

## mist-comp

Mists eigener Wayland-Compositor, rund 14 000 Zeilen Rust auf [Smithay], mit Anmeldebildschirm. Er startet
dieselbe Shell und beantwortet genug von Hyprlands IPC, dass `hyprctl` und Mists Skripte unverändert
funktionieren. Er ist experimentell und neben Hyprland die tägliche Sitzung des Autors.

- **Fenster:** Mist-Titelleisten oder die der App, runde Ecken und Schatten, Glasunschärfe unter
  durchscheinenden Fenstern und Panels, Overview (`Super + O`), eine Lupe, Arbeitsflächenwechsel, der den
  Fingern folgt, Win + D zeigt den Desktop.
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

### 1. Holen

Arch: [packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) baut `mist-shell-git`.

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

Überall sonst aus einem Klon. Das erste Skript baut `mistd` (die Backends), das zweite legt `mist-ui-bin`,
`mist-wall-bin`, `mist-comp` und `mist-try` nach `~/.local/bin`:

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

Fertige Binärdateien hängen an jedem [Release](https://github.com/wiktor-jozwiak/mist/releases).

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

### 2. In einem Fenster ausprobieren

```bash
mist-try
```

Der ganze Desktop (sein Compositor, Leiste, Dock, Island, Widgets) in einem Fenster unter Hyprland, Sway,
GNOME oder KDE. Er hat ein eigenes Laufzeitverzeichnis und einen eigenen D-Bus-Sitzungsbus, deine
Benachrichtigungen, Systemablage und dein Polkit-Agent bleiben also, wo sie sind; Fenster zu, und er ist weg.
Optionen: `MIST_TRY_SIZE=1600x1000`, `MIST_TRY_KEYBOARD=de`, `MIST_THEME=nord`.

### 3. Zu deinem Desktop machen

- **Unter Hyprland:** [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) in die eigene Konfiguration
  laden und andere Leisten, Benachrichtigungsdienste und Polkit-Agenten entfernen.
- **Unter mist-comp:** `native/mist-comp/session/install-session.sh --greeter` fügt dem Anmeldebildschirm eine
  Mist-Sitzung hinzu; Hyprland bleibt.

## Tasten und Skripte

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

Die Bilder auf dieser Seite stammen aus denselben Offscreen-Renderings, mit Demodaten: `docs/img/hero.sh`,
`docs/img/panels.sh`, `docs/img/gifs.sh`.

| Pfad | Inhalt |
|---|---|
| `native/mist-ui` | alle Flächen der Shell |
| `native/mist-wall` | der GPU-Prozess für Hintergründe |
| `native/mist-comp` | Compositor, Sitzung und Anmeldebildschirm |
| `scripts/mistd` | Backends: Systemwerte, Telefon, Anrufe, Zwischenablage, Garmin-Synchronisierung |
| `shaders/` | Shader für Hintergründe und Himmel |
| `bench/` | die Messwerkzeuge und ihre Ergebnisse |
| Tag `legacy-qml` | die ursprüngliche Quickshell-Version |

## Sterne-Verlauf

<a href="https://star-history.com/#wiktor-jozwiak/mist&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date&theme=dark">
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date">
    <img alt="Sterne-Verlauf von wiktor-jozwiak/mist" src="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date" width="100%">
  </picture>
</a>

## Dank

Gebaut auf [Smithay], [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus] und [Tabler Icons].
Die erste Version lief auf [Quickshell], mit [Noctalia] als Vorbild.
KI-Programmierassistenten helfen bei der Entwicklung.

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
