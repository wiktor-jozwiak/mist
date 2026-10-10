# Plan: przepisanie skryptów Pythona Mist na Rust

**Zrobione 2026-10-09.** Wszystkie skrypty Pythona (i inline Python z `Clipboard.qml`, i `clipstore.sh`) działają teraz jako `scripts/bin/mistd` (źródła: `scripts/mistd`, budowanie: `scripts/mistd/install.sh`). Stare pliki: `scripts/old-python/`. Zostaje tylko `shot.sh` (sklejka grim/slurp). Poniżej pierwotny plan.

## Stan obecny

| Skrypt | Linie | Tryb | RSS | Co robi |
|---|---|---|---|---|
| `phonewatch.py` | ~900 | stały proces | ~18 MB | KDE Connect przez `busctl monitor` + D-Bus, SMS, kontakty, ADB, połączenia z `adb logcat` |
| `callwatch.py` | 402 | stały proces | ~15 MB | HFP: `org.pipewire.Telephony`, dźwięk rozmowy (`pipewire -c`), dzwonek |
| `islandwatch.py` | 314 | stały proces | ~18 MB | schowek, USB (udevadm), montowania, kamera, LED |
| `notifywatch.py` | 41 | stały proces | ~13 MB | podsłuch `Notify` przez `busctl monitor` |
| `mistdata.py` | 332 | uruchamiany okresowo | – | pogoda, Garmin/Influx, bieganie, kalendarz ICS, sys |
| `docktiles.py` | 257 | na żądanie (venv + Pillow) | – | kafelki ikon docka |
| `notifimg.py` | 19 | na żądanie (venv + Pillow) | – | WebP → PNG |
| `clipstore.sh`, `shot.sh` | 45 | na żądanie | – | zostają w shellu (to tylko sklejanie narzędzi) |

Cztery stałe procesy Pythona zajmują razem ~65 MB RSS. Do tego dochodzą procesy-dzieci `busctl monitor` (3 sztuki) i interpreter, który startuje ~150 ms przy każdym uruchomieniu `mistdata.py`.

## Decyzja: Rust, jeden demon `mistd`

- **Rust** zamiast C++/Go: mały RSS (2–5 MB), `zbus` daje natywny D-Bus z match rules (znika `busctl monitor`), `tokio` obsługuje wiele strumieni w jednym procesie, a jeden statyczny plik binarny nie zależy od wersji Qt.
- **Odrzucone: wtyczka C++ do QML.** Mist-qs ma własne Qt (`~/.local/opt/mist-qs/usr` oraz `usr.qt611`), a systemowe to 6.12. Wtyczkę trzeba by przebudowywać przy każdej zmianie Qt, a jej awaria wywraca całą powłokę.
- **Odrzucone: Go.** Większy RSS (GC, ~10 MB) i słabszy ekosystem D-Bus.
- Wymaga instalacji: `sudo pacman -S rust` (obecnie brak `cargo`/`rustc`; jest tylko `g++`).

## Architektura

```
scripts/mistd/            crate (cargo workspace)
  src/main.rs             tokio runtime, wybór trybu: `mistd phone|calls|island|notify`, `mistd data <src>`, `mistd tiles ...`
  src/proto.rs            wspólne: JSON line na stdout, komendy JSON na stdin
  src/phone/              kdeconnect.rs (zbus proxy + sygnały), sms.rs, contacts.rs (vCard), adb.rs, calls.rs (logcat)
  src/calls/              pipewire_telephony.rs, audio.rs (generowany config + `pipewire -c`), ringer.rs
  src/island/             clip.rs, usb.rs (crate `udev` / netlink), mounts.rs (/proc/self/mountinfo + poll), cam.rs
  src/notify.rs           zbus: BecomeMonitor na Notify
  src/data/               weather.rs, garmin.rs, running.rs, calendar.rs (crate `icalendar` + rrule), sys.rs
  src/tiles.rs            crate `image` + `resvg` (SVG ikon), zastępuje Pillow i venv
```

- **Jeden plik binarny, osobne procesy na tryb** (jak teraz, jeden `Process` w QML na tryb). Awaria jednego modułu nie zabija innych, a QML zmienia tylko `command:`. Scalenie w jeden proces to ewentualny etap 2, gdy wszystko będzie stabilne.
- **Protokół bez zmian.** Te same linie JSON na stdout i te same komendy na stdin, więc pliki QML zmieniają tylko linię `command:`. Jedyny wyjątek: `notifywatch` wysyła linie TSV, zostawić je bez zmian.
- `setpriv --pdeathsig` zastąpić wywołaniem `prctl(PR_SET_PDEATHSIG)` w samym `mistd` (crate `nix`). Dzieci (`adb`, `pipewire`) uruchamiać z `pre_exec` prctl.

## Kolejność (od najprostszego, każdy krok osobno testowalny)

1. **`notify`** (41 linii): rozgrzewka, ustawia szkielet crate'a, `proto.rs` i zbus monitor.
   Test: uruchomić obok starego (`python3 notifywatch.py > a`, `mistd notify > b`), wysłać `notify-send`, porównać pliki `diff a b`.
2. **`island`**: schowek (nadal `wl-paste --watch`), udev, mountinfo, kamera (`/proc/*/fd` → `/dev/video*`).
   Test: demo wyspy + podpięcie pendrive'a, porównanie z wyjściem starego skryptu.
3. **`data`** (oneshot): weather/garmin/running/calendar/sys. Cache `~/.cache/mist/<src>.json` w identycznym formacie, w razie błędu wraca `stale: true`.
   Test: `diff <(python3 mistdata.py X | jq -S .) <(mistd data X | jq -S .)` dla każdego źródła.
4. **`calls`** (HFP/PipeWire): ostrożnie, z aktywnym telefonem po Bluetooth. Dema wyspy `call-bt`, `call-talk`.
5. **`phone`** (największy): kolejno stan urządzeń → zdarzenia → SMS/kontakty → ADB → połączenia z logcat.
   Test: panel telefonu, SMS w obie strony, prawdziwe połączenie przychodzące i wychodzące (dema `call-adb*`).
6. **`tiles` + `notifimg`**: usuwa zależność od `~/.local/share/mist/venv`. Porównanie PNG (`compare -metric AE` z ImageMagick).
7. Sprzątanie: usunąć stare `.py` (najpierw zrobić kopie `.before-rust`), zaktualizować komentarze w nagłówkach QML i pamięć.

## Przy okazji do naprawy

- **Wyciek `wl-paste --watch`.** Po każdym przeładowaniu Quickshella zostaje osierocony proces (teraz jest ich 8+). `Clipboard.qml:23` uruchamia go bez `setpriv --pdeathsig`. Naprawić od razu, niezależnie od przepisywania: dodać `["setpriv", "--pdeathsig", "TERM", "wl-paste", ...]`.
- `phonewatch`: wątek `call_watch` czeka do 20 s na pierwsze IP po starcie. W Rust zrobić to zdarzeniowo (start strumienia logcat po zdarzeniu `linked`).

## Ryzyka

- **SMS przez KDE Connect** (`a(v)`, warianty z `av`): typy D-Bus są nieoczywiste. Najpierw zrzucić prawdziwe wiadomości przez `busctl --json` jako fixtures do testów jednostkowych.
- **Dźwięk rozmowy w `callwatch`** jest kruchy (profil karty bluez, echo cancel). Zostawić generowanie configu i uruchamianie `pipewire -c` bez zmian, przepisać tylko logikę.
- **Kalendarz ICS z RRULE**: porównać rozwinięte wystąpienia na ±60 dni ze starą implementacją.

## Szacunek

1–2 sesje na kroki 1–3, osobna sesja na `calls` i `phone`. Zysk: ~65 MB → ~10 MB RSS, brak interpretera przy `mistdata`, brak venv z Pillow.
