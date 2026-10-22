<div align="center">

<img src="../img/banner.svg" alt="Mist" width="100%">

**Powłoka pulpitu dla Waylanda inspirowana macOS, napisana w Rust i rysowana na CPU.**

[![CI](https://img.shields.io/github/actions/workflow/status/wiktor-jozwiak/mist/ci.yml?branch=main&style=flat-square&label=CI)](../../.github/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wiktor-jozwiak/mist?style=flat-square&color=3b82f6)](https://github.com/wiktor-jozwiak/mist/releases)
[![Licencja: GPL-3.0](https://img.shields.io/badge/licencja-GPL--3.0-8d8590?style=flat-square)](../../LICENSE)
[![Discord](https://img.shields.io/badge/discord-czat-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/4rRabwQTPE)

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/desktop.webp" alt="Pulpit Mist: górny pasek, zegar, karty pogody, gotowości i biegania, dock" width="100%">

<table>
  <tr>
    <td width="50%"><img src="../img/island.gif" alt="Dynamic Island: powiadomienie, głośność, zaraz deszcz, następne wydarzenie" width="100%"></td>
    <td width="50%"><img src="../img/weather.gif" alt="Karta pogody w deszczu, nad żywym niebem" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Dynamic Island: powiadomienia, głośność, deszcz w najbliższej godzinie, następne wydarzenie</sub></td>
    <td align="center"><sub>Karta pogody nad niebem rysowanym na żywo w GLSL</sub></td>
  </tr>
</table>

</div>

Mist to pasek, dock, Dynamic Island, widżety pulpitu, wyszukiwarka, centrum sterowania, powiadomienia, ekran
blokady i agent polkit jednego pulpitu, w jednym procesie ~50 MB, który nigdy nie otwiera kontekstu GPU.
Działa na Hyprlandzie albo na własnym kompozytorze, `mist-comp`.

Zaczął jako konfiguracja Quickshell, która na bezczynnym pulpicie zajmowała 428 MB i 9,5 % CPU, i został
przepisany powierzchnia po powierzchni według trzech zasad:

- **Rysuj na CPU.** `mist-ui` maluje bufory `wl_shm` przez [tiny-skia] i [swash] i odświeża tylko to, co się
  zmieniło. Na NVIDII każdy kontekst OpenGL kosztuje 100 do 160 MB, zanim pojawi się pierwszy piksel; Mist nie
  ma żadnego.
- **Czekaj na zdarzenia.** Wszystko przychodzi z gniazda kompozytora, D-Bus, `pactl subscribe` i inotify. Nic
  nie odpytuje w kółko, gdy ekran stoi, a ukryte powierzchnie nie pracują.
- **Jeden proces GPU i tylko wtedy, gdy coś pokazuje.** `mist-wall` rysuje żywe tapety i niebo pogody; przy
  tapecie ze zdjęcia kończy działanie, a zdjęcie rysuje `mist-ui`.

> [!NOTE]
> Mist to codzienny pulpit jednej osoby (Arch / CachyOS, dwa monitory, NVIDIA). Jest publiczny, bo działa,
> a nie dlatego, że jest skończony: licz się z czytaniem kodu.

## Funkcje

<table>
  <tr>
    <td width="50%"><img src="../img/panels/spotlight.webp" alt="Wyszukiwarka w stylu Spotlight" width="100%"></td>
    <td width="50%"><img src="../img/panels/settings.webp" alt="Okno ustawień" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Spotlight: aplikacje, pliki, polecenia, emoji, sieć</sub></td>
    <td align="center"><sub>Ustawienia: wszystko w jednym oknie, zmiany od razu</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/control.webp" alt="Centrum sterowania" width="100%"></td>
    <td width="50%"><img src="../img/panels/calendar.webp" alt="Kalendarz" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Centrum sterowania: sieć, Bluetooth, telefon, dźwięk, światło nocne</sub></td>
    <td align="center"><sub>Kalendarz: miesiąc, agenda, nowe wydarzenia</sub></td>
  </tr>
</table>

| | |
|---|---|
| **Górny pasek** | aktywna aplikacja, zasobnik systemowy (StatusNotifierItem + dbusmenu), sieć, Bluetooth, dźwięk, CPU i pamięć, zegar |
| **Dock** | przypięte i uruchomione aplikacje, powiększanie, wybór okna z podglądem na żywo |
| **Dynamic Island** | multimedia z equalizerem, minutniki, połączenia (KDE Connect lub ADB), powiadomienia, głośność |
| **Pulpit** | zegar i szklane karty: pogoda, gotowość Garmin, bieganie, muzyka, kalendarz, system, zegary świata |
| **Panele** | Spotlight, Start, przełącznik aplikacji, centrum sterowania, centrum powiadomień, historia schowka, emoji, menu sesji, zrzuty ekranu |
| **Ustawienia** | język, miejsce pogody, tapeta, układ pulpitu, widżety, światło nocne, blokada i uśpienie, powiadomienia |
| **System** | serwer powiadomień, agent polkit, ekran blokady (PAM), bezczynność i uśpienie |
| **Aplikacje** | Mist Files (także okno wyboru plików w każdej aplikacji), Mist Player (libmpv), ustawienia systemu |
| **Przyciąganie okien** | jak w Windows 11: przeciągnij do krawędzi lub rogu, Snap Layouts na zielonym przycisku albo `Super + Z`, Snap Assist |
| **Tapety** | zdjęcia, żywe sceny GLSL albo niebo zgodne z aktualną pogodą |

**Pogoda** pochodzi z [Open-Meteo](https://open-meteo.com), bez konta: krzywa godzinowa, cztery dni, deszcz
w najbliższej godzinie, słońce i księżyc. **Gotowość Garmin** i **bieganie** pokazują Twoje dane z Garmin
Connect, które `mistd` synchronizuje do lokalnego pliku SQLite; karty są ukryte, dopóki raz się nie zalogujesz
([instrukcja](../guides/garmin.pl.md)).

Interfejs mówi po **polsku, angielsku, niemiecku, hiszpańsku i francusku**, zgodnie z `$LANG` albo wyborem
w Ustawieniach. Język to jeden plik JSON w [native/assets/i18n](../../native/assets/i18n).

## Pomiary

<p align="center"><img src="../img/memory.svg" alt="Pamięć bez obciążenia: samo mist-ui 36 do 50 MB, Mist z mist-wall 87 do 106 MB, Noctalia v5 122 MB, pusty Quickshell 124 MB, Mist na Quickshell 428 MB" width="100%"></p>

Cała powłoka bez obciążenia, PSS, Ryzen 7 5700X + RTX 4060, Hyprland 0.56, dwa monitory 1080p,
[bench/measure.sh](../../bench/measure.sh):

| Powłoka | Technologia | Pamięć (PSS) | CPU bez obciążenia | Kontekst GL |
|---|---|---|---|---|
| **Mist** | Rust, CPU, `wl_shm` | **87 do 106 MB** | 0,6 % (gra muzyka) | tylko `mist-wall` |
| Mist, samo `mist-ui` | | 36 do 50 MB | 0,6 % | nie |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0,07 % | tak |
| Pusty Quickshell (jedno `PanelWindow`) | Qt Quick | 124 MB | ~0 % | tak |
| Mist na Quickshell (stara wersja QML) | Qt Quick | 428 MB | 9,5 % | tak |

Na `mist-comp` kompozytor razem z całą powłoką stoi na **0,04 % CPU i 6 wybudzeniach na sekundę** (bez
monitora, renderer programowy). Metoda, liczby kompozytora i prawdziwa, obciążona sesja:
[docs/BENCHMARKS.md](../BENCHMARKS.md).

## mist-comp

Własny kompozytor Waylanda dla Mist, około 12 500 linii Rust na [Smithay], z ekranem logowania. Uruchamia tę
samą powłokę i odpowiada na tyle IPC Hyprlanda, że `hyprctl` i skrypty Mist działają bez zmian. Jest
eksperymentalny i jest codzienną sesją autora obok Hyprlanda.

- **Okna:** paski tytułu Mist albo własne aplikacji, zaokrąglone rogi i cienie, rozmycie szkła pod
  półprzezroczystymi oknami i panelami, przyciąganie jak w Windows 11, Overview (`Super + O`), lupa,
  przełączanie obszarów roboczych podążające za palcami, Win + D pokazuje pulpit.
- **Nie robi pracy, którą może pominąć:** monitor przerysowuje się tylko, gdy coś na nim się zmieniło; szkło
  rozmywa się ponownie tylko, gdy za nim coś drgnęło; okno zasłonięte innymi dostaje jedną klatkę na sekundę
  zamiast sześćdziesięciu.
- **Sesja:** DRM / KMS przez logind, podłączanie monitorów w locie, VRR, explicit sync, XWayland, udostępnianie
  ekranu przez portal, około 45 protokołów. Po awarii okna wracają na swoje obszary robocze;
  `mist-session restart` ładuje nową wersję bez wylogowania.
- **Testy:** 50 testów w zagnieżdżonym kompozytorze bez monitora, uruchamianych w CI na rendererze programowym.

Jeszcze nie ma: kafelkowania, kontroli tearingu, sprawdzonego direct scan-out na NVIDII. Konfiguracja:
[docs/CONFIG.md](../CONFIG.md#compconf-outputs-mist-comp-only); `mist-comp migrate` przenosi konfigurację Hyprlanda.

## Instalacja

Arch: [packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) buduje `mist-shell-git`.

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

Z klonu repozytorium, w dowolnym miejscu:

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

Pierwszy skrypt buduje `mistd`, drugi kładzie `mist-ui-bin`, `mist-wall-bin` i `mist-comp` w `~/.local/bin`.
Gotowe binarki są dołączone do każdego [wydania](https://github.com/wiktor-jozwiak/mist/releases).

- **Na Hyprlandzie:** wczytaj [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) w swojej
  konfiguracji i usuń inne paski, demony powiadomień i agentów polkit.
- **Na mist-comp:** `native/mist-comp/session/install-session.sh --greeter` dodaje sesję Mist do ekranu
  logowania; Hyprland zostaje.

<details>
<summary>Zależności</summary>

<br>

**Budowanie:** `rust`, `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa`; dla `mist-comp` także `libinput`,
`libseat`, `libdrm`, `systemd-libs`.

**Uruchamianie:** `pipewire-pulse`, `wireplumber`, `networkmanager`, `wl-clipboard`, `grim`, `slurp`,
`polkit`, `noto-fonts`, `noto-fonts-emoji`. Opcjonalnie: `ddcutil` (jasność monitora), `kdeconnect` albo
`android-tools` (telefon). Co włącza każda z nich: [docs/CONFIG.md](../CONFIG.md#optional-integrations).

**Czcionka:** interfejs jest projektowany pod SF Pro, której Mist nie może dołączyć. Bez niej używa Inter,
potem Noto Sans.

</details>

## Użycie

| Klawisz | Akcja | | Klawisz | Akcja |
|---|---|---|---|---|
| `Super` | Start | | `Super + A` | centrum powiadomień |
| `Super + Space` | Spotlight | | `Super + V` | historia schowka |
| `Super + Tab` | przełącznik aplikacji | | `Super + .` | emoji |
| `Super + I` | centrum sterowania | | `Super + L` | blokada |
| `Super + Shift + S` | zrzut ekranu | | prawy klik na pulpicie | ustawienia |

Wszystko da się oskryptować: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`. Ustawienia
leżą w `~/.config/mist/`, każdy plik opisuje [docs/CONFIG.md](../CONFIG.md); log to `~/.cache/mist/ui.log`.

## Rozwój

```bash
cd native && cargo build --release && cargo test --release
```

Każda powierzchnia renderuje się poza ekranem, więc zmiany sprawdzisz bez ruszania działającej sesji:
`mist-ui-bin --pane-png spotlight 1920 1080 out.png`, `mist-ui-bin --desk-png dir`,
`mist-wall-bin --png out.ppm dunes 1920 1080`. `native/mist-comp/tests/run-all.sh` uruchamia testy kompozytora
w zagnieżdżonym oknie. Więcej w [native/README.md](../../native/README.md) i [CONTRIBUTING.md](../../CONTRIBUTING.md).

| Ścieżka | Co |
|---|---|
| `native/mist-ui` | wszystkie powierzchnie powłoki |
| `native/mist-wall` | proces tapet na GPU |
| `native/mist-comp` | kompozytor, sesja i ekran logowania |
| `scripts/mistd` | backendy: statystyki systemu, telefon, połączenia, schowek, synchronizacja Garmin |
| `shaders/` | shadery tapet i nieba |
| tag `legacy-qml` | pierwotna wersja na Quickshell |

## Podziękowania

Zbudowane na [Smithay], [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus] i [Tabler Icons].
Pierwsza wersja działała na [Quickshell], z [Noctalia] jako punktem odniesienia.

<sub>Mist nie jest powiązany z Apple. macOS, Dynamic Island, Spotlight i SF Pro są znakami towarowymi Apple Inc.,
wymienionymi tu tylko, by opisać inspirację projektu.</sub>

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
