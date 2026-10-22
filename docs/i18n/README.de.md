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

<p align="center"><img src="../img/memory.svg" alt="Speicher im Leerlauf: mist-ui allein 36 bis 50 MB, Mist mit mist-wall 87 bis 106 MB, Noctalia v5 122 MB, leeres Quickshell 124 MB, Mist auf Quickshell 428 MB" width="100%"></p>

Ganze Shell im Leerlauf, PSS, Ryzen 7 5700X + RTX 4060, Hyprland 0.56, zwei 1080p-Monitore,
[bench/measure.sh](../../bench/measure.sh):

| Shell | Technik | Speicher (PSS) | CPU im Leerlauf | GL-Kontext |
|---|---|---|---|---|
| **Mist** | Rust, CPU, `wl_shm` | **87 bis 106 MB** | 0,6 % (Musik läuft) | nur `mist-wall` |
| Mist, nur `mist-ui` | | 36 bis 50 MB | 0,6 % | nein |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0,07 % | ja |
| Leeres Quickshell (ein `PanelWindow`) | Qt Quick | 124 MB | ~0 % | ja |
| Mist auf Quickshell (alte QML-Version) | Qt Quick | 428 MB | 9,5 % | ja |

Auf `mist-comp` liegen Compositor und ganze Shell zusammen bei **0,04 % CPU und 6 Aufwachvorgängen pro Sekunde**
(headless, Software-Renderer). Methode, die Zahlen des Compositors und eine echte, ausgelastete Sitzung:
[docs/BENCHMARKS.md](../BENCHMARKS.md).

## mist-comp

Mists eigener Wayland-Compositor, rund 12 500 Zeilen Rust auf [Smithay], mit Anmeldebildschirm. Er startet
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
- **Getestet:** 50 verschachtelte Tests mit Headless-Backend, in der CI auf dem Software-Renderer.

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
