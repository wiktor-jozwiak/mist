<div align="center">

# Mist

**Powłoka pulpitu dla Hyprlanda inspirowana macOS, napisana w Rust i rysowana na CPU.**

Pasek, dock, Dynamic Island, widżety pulpitu, wyszukiwarka w stylu Spotlight, centrum sterowania,
powiadomienia, ekran blokady i agent polkit w jednym procesie ~50 MB, bez kontekstu GPU.

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

![Licencja: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)

![Pulpit Mist](../img/desktop.webp)

<sub>Render poza ekranem (`mist-ui --desk-png`, `--dock-png`, `--png`, `mist-wall-bin --png`) z danymi demo.
W prawdziwej sesji Hyprland rozmywa tło pod szkłem.</sub>

</div>

## Dlaczego

Większość powłok dla Hyprlanda działa na Qt Quick (Quickshell), GTK + GJS (AGS) albo WebView. Każda otwiera
kontekst OpenGL, a na NVIDII każdy kontekst GL mapuje sterownik: około **100–160 MB na proces**, zanim
pojawi się pierwszy piksel. Mist zaczynał jako konfiguracja Quickshella (428 MB, 9,5 % CPU) i został
przepisany powierzchnia po powierzchni:

- **Powierzchnie rysowane na CPU.** `mist-ui` maluje do buforów `wl_shm` przez [tiny-skia] i [swash];
  Hyprland je składa i rozmywa. Przerysowywane są tylko zmienione obszary.
- **Sterowanie zdarzeniami.** Dane płyną z gniazda Hyprlanda, D-Bus, `pactl subscribe`, inotify i stale
  działających backendów `mistd`. Nic nie odpytuje w pętli, gdy ekran stoi; ukryte powierzchnie nie pracują.
- **Jeden proces GPU, tylko gdy trzeba.** `mist-wall` (EGL, GLSL 150) rysuje animowane tapety i niebo
  pogodowe. Przy tapecie ze zdjęcia kończy działanie, a zdjęcie rysuje `mist-ui`.

Wynik na maszynie autora: **~87–106 MB PSS za całą powłokę**, 0,6 % jednego rdzenia przy grającej muzyce.
Szczegóły: [docs/BENCHMARKS.md](../BENCHMARKS.md).

Ryzen 7 5700X + RTX 4060, Hyprland 0.56.2, cała powłoka w spoczynku, [bench/measure.sh](../../bench/measure.sh).

| Powłoka | Technologia | PSS | CPU w spoczynku | Kontekst GL |
|---|---|---|---|---|
| **Mist native** | Rust, CPU, `wl_shm` | **87–106 MB** | 0,6 % | `mist-wall` |
| Mist native, `mist-ui` sam | | 36–50 MB | 0,6 % | – |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0,07 % | ✓ |
| Pusty Quickshell (jeden `PanelWindow`) | Qt Quick | 124 MB | ~0 % | ✓ |
| Mist na Quickshell (stara wersja QML) | Qt Quick + QML | 428 MB | 9,5 % | ✓ |

## Funkcje

| | |
|---|---|
| **Górny pasek** | aktywna aplikacja i tytuł, zasobnik (host StatusNotifierItem + dbusmenu), sieć, Bluetooth, dźwięk, CPU / pamięć, zegar, kalendarz |
| **Dock** | przypięte i uruchomione aplikacje, powiększanie, wybór okna z miniaturami na żywo (`hyprland_toplevel_export_v1`), klik w aktywną aplikację ją minimalizuje |
| **Dynamic Island** | multimedia (MPRIS) z korektorem, minutniki, połączenia (KDE Connect / ADB), powiadomienia, głośność |
| **Pulpit** | zegar, wschód / zachód słońca, szklane karty (pogoda, gotowość Garmin, plan biegowy), arkusz ustawień widżetów |
| **Panele** | wyszukiwarka w stylu Spotlight, Start, przełącznik Cmd+Tab, centrum sterowania, centrum powiadomień, historia schowka, emoji, menu sesji, galeria tapet, pasek zrzutów ekranu |
| **System** | serwer powiadomień, agent polkit, blokada ekranu (`ext-session-lock-v1` + PAM), bezczynność (`ext-idle-notify-v1`, logind), przyciąganie okien jak w Windows 11 |
| **Tapety** | zdjęcia (CPU) albo sceny GLSL na żywo i niebo pogodowe w czasie rzeczywistym (`mist-wall`) |
| **mist-comp** *(eksperymentalny)* | własny kompozytor na Smithay + greeter, do uruchamiania Mist bez Hyprlanda |

### Języki

Interfejs mówi **po polsku, angielsku, niemiecku i hiszpańsku**. Wybór: *Dostosuj pulpit* (prawy klik na
zegarze pulpitu) → Wygląd → Język, albo `MIST_LANG=en`. *Automatycznie* bierze `$LANG`, w razie braku angielski.
Tłumaczenia: jedna tabela [native/assets/i18n.tsv](../../native/assets/i18n.tsv), wiersz na napis, kolumna na
język. Nowy język to jedna kolumna więcej.

Dock, wyspa, pulpit i niebo pogody są na pierwszym monitorze Hyprlanda. Inny: `"primary": "DP-3"` w
`~/.config/mist/settings.json` albo `MIST_PRIMARY=DP-3`. Miejsce pogody: *Dostosuj pulpit* → Ogólne.

### Karty pulpitu

- **Pogoda**: [Open-Meteo](https://open-meteo.com), bez konta. Krzywa godzinowa, 4 dni, nowcast opadów, okno na
  bieg, podpowiedź ubioru, słońce i księżyc; niebo za kartą odwzorowuje aktualną pogodę.
- **Gotowość Garmin** i **bieganie**: Twoje dane z Garmin Connect przez lokalne InfluxDB. Konfiguracja:
  [docs/guides/garmin.pl.md](../guides/garmin.pl.md).

## Architektura

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust, CPU (tiny-skia + swash), wl_shm, jeden proces na wszystkie powierzchnie
                           │      ├── mistd stats | phone | calls | island | clipstore   (backendy zdarzeń)
                           │      └── IPC: mist-ui-bin msg <cel> <funkcja> [argumenty]
                           └── mist-wall    Rust, EGL / GLSL 150, tylko tapety na żywo i niebo
```

| Ścieżka | Co |
|---|---|
| `native/mist-ui` | wszystkie powierzchnie powłoki ([native/README.md](../../native/README.md)) |
| `native/mist-wall` | proces GPU tapety |
| `native/mist-comp` | eksperymentalny kompozytor, sesja i greeter |
| `native/common` | kod wspólny mist-ui i mist-wall (astronomia, presety) |
| `scripts/mistd` | backendy: statystyki, telefon (KDE Connect, ADB), połączenia, schowek, D-Bus menedżera plików |
| `shaders/` | shadery tapet i nieba (źródła `.frag`, GLSL 150 w `shaders/glsl` czytany przez mist-wall) |
| `*.qml`, `components/` | pierwotna wersja na Quickshellu, zostawiona jako wzorzec; już nieuruchamiana |

## Instalacja

Mist to codzienne środowisko jednej osoby: Arch / CachyOS, Hyprland 0.55+ z konfiguracją w Lua, GPU
NVIDIA lub Mesa. Licz się z czytaniem kodu.

**Do budowania:** `rust` (stable), `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa` (nagłówki EGL);
dla `mist-comp` także `libinput`, `libseat`, `libdrm`, `mesa` (gbm), `systemd-libs` (udev).

**W działaniu:** `hyprland`, `pipewire` + `pipewire-pulse` (`pactl`, `pw-play`, `parec`), `wireplumber`
(`wpctl`), `networkmanager` (`nmcli`), `wl-clipboard`, `grim`, `slurp`, `polkit`, `noto-fonts`, `noto-fonts-emoji`. Opcjonalnie: `ddcutil` (jasność przez DDC/CI),
`kdeconnect` / `android-tools` (telefon), `wf-recorder` (nagrywanie ekranu).

**Font:** interfejs jest projektowany pod SF Pro (zmienny `SF-Pro.ttf` w `~/.local/share/fonts/SF-Pro/`);
Apple licencjonuje go tylko na swoje platformy i Mist go nie dołącza. Bez niego Mist używa Inter Variable,
potem Noto Sans.

**Arch (paczka w stylu AUR):** [packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) buduje `mist-shell-git`
(binarki w `/usr/bin`, pliki w `/usr/share/mist`).

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

**Z kopii repo** (w dowolnym miejscu; `native/install.sh` linkuje do niej `~/.local/share/mist/shell`):

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

`native/install.sh` instaluje do `~/.local/bin` `mist-ui-bin`, `mist-wall-bin`, `mist-comp`, a za pierwszym
razem także strażnika `mist-ui`. Wydania z tagiem mają gotowe binarki.

Potem dodaj [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) do konfiguracji Hyprlanda
(autostart, skróty, reguły rozmycia) i usuń inne paski, demony powiadomień i agentów polkit.

## Obsługa

| Klawisz | Akcja |
|---|---|
| `Super` | Start |
| `Super + Spacja` | Spotlight |
| `Super + Tab` | przełącznik aplikacji |
| `Super + I` | centrum sterowania |
| `Super + A` | centrum powiadomień |
| `Super + V` | historia schowka |
| `Super + .` | emoji |
| `Super + L` | blokada |
| `Print`, `Super + Shift + S` | zrzut ekranu (obszar) |

Wszystko da się oskryptować: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`,
`mist-ui-bin msg dock click firefox`. Log: `~/.cache/mist/ui.log`. Ustawienia: `~/.config/mist/`.

## Rozwój

```bash
cd native && cargo build --release && cargo test --release
```

Sprawdzanie bez ruszania działającej sesji:

| Polecenie | Wynik |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | górny pasek jako PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | scena pulpitu + karty / dock |
| `mist-ui --test` | drugi pasek nad prawdziwym na HDMI-A-1 |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | klatka tapety na żywo |
| `mist-ui-bin msg ui stats` | wybudzenia, malowania i czas malowania na powierzchnię |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | PSS / CPU działającej powłoki w spoczynku |

Zostaw `lto = false` (rustc z Archa z systemowym LLVM wywala się przy LTO). Zob.
[CONTRIBUTING.md](../../CONTRIBUTING.md).

## Stan

Codzienne środowisko jednej osoby, dwa monitory, NVIDIA. Wszystko z tabeli funkcji jest natywne i używane
na co dzień; `mist-comp` jest eksperymentalny. Czego brakuje przed publicznym
wydaniem (konfigurowalne ścieżki, bez zasobów Apple): [docs/PUBLISHING.md](../PUBLISHING.md).

## Podziękowania

[Hyprland], [Smithay] i [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus], [Tabler Icons]
(MIT). Pierwotna wersja QML powstała na [Quickshell]; [Noctalia] służyła za rezerwę i wzorzec.

*Mist nie jest powiązany z Apple. macOS, Dynamic Island, Spotlight i SF Pro są znakami towarowymi Apple Inc.;
padają tu tylko jako opis inspiracji.*

## Licencja

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
