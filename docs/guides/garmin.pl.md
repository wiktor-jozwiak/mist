# Karty Garmin

[English](garmin.md)

Karta **gotowości** (Training Readiness, krzywa Body Battery, sen, tętno spoczynkowe, stres, HRV, kroki) i karta
**biegania** (kropki tygodnia, ostatni bieg, kilometry w miesiącu i roku, prognoza 5 km, dni do startu) czytają
lokalny plik SQLite, który `mistd garmin sync` wypełnia z Garmin Connect.

```
zegarek ─ Garmin Connect ─ mistd garmin sync (co 20 min) ─ ~/.local/share/mist/garmin.db ─ mistd data garmin|running ─ karty
```

Bez serwera, bez kontenera, bez zapisanego hasła. Jedyne, czego Mist nie zrobi sam, to pierwsze logowanie z 2FA,
więc ten krok raz pożycza logowanie z [garmin-grafana]. Bez tej konfiguracji obie karty po prostu się nie pokazują.

## 1. Zaloguj się raz

Potrzebny `docker` z wtyczką compose, tylko do tego kroku.

```bash
mkdir -p ~/garmin-mist && cp ~/.config/quickshell/mist/integrations/garmin/compose.yml ~/garmin-mist/
```

```bash
cd ~/garmin-mist && mkdir -p garminconnect-tokens && sudo chown 1000:1000 garminconnect-tokens && docker compose run --rm garmin-fetch-data
```

Podaj e-mail, hasło i kod 2FA z Garmin Connect. Gdy zacznie pobierać dane, tokeny leżą w
`~/garmin-mist/garminconnect-tokens/garmin_tokens.json`; przerwij `Ctrl+C` i posprzątaj:

```bash
cd ~/garmin-mist && docker compose down -v
```

Hasło nie jest zapisywane. Od teraz mistd sam odświeża tokeny.

## 2. Synchronizacja w tle

```bash
cp ~/.config/quickshell/mist/integrations/garmin/mist-garmin-sync.{service,timer} ~/.config/systemd/user/
```

```bash
systemctl --user daemon-reload && systemctl --user enable --now mist-garmin-sync.timer
```

Pierwsze uruchomienie zapisuje dzisiejsze dane, każde kolejne dopisuje. Ręcznie:

```bash
~/.config/quickshell/mist/scripts/bin/mistd garmin sync
```

Wypisuje `{"status": "ok", "rows": …, "coach": {…}}`.

## 3. Sprawdź

```bash
~/.config/quickshell/mist/scripts/bin/mistd data garmin
```

```bash
~/.config/quickshell/mist/scripts/bin/mistd data running
```

mist-ui pyta o `garmin` co 5 minut i o `running` co 10, więc karty wypełniają się same.

| Wynik | Co zrobić |
|---|---|
| `"status": "disabled"` | brak pliku z tokenami: krok 1 |
| `cannot read …garmin_tokens.json` | plik nie należy do Ciebie: `sudo chown $USER` |
| `GET …: http status: 401` przy sync | tokeny unieważnione (zmiana hasła, wylogowanie wszędzie): znowu krok 1 |
| karty puste, sync ok | zegarek nie zsynchronizował się dziś z Garmin Connect |

## Proponowany trening

Linia **Dziś** na karcie biegania pochodzi z kilku reguł w `mistd garmin sync`: gotowość, czas regeneracji,
Body Battery, ostatnie tygodnie biegów i data startu wybierają odpoczynek, regenerację, bieg bazowy, tempo albo interwały. Start
ustawiasz w `~/.config/mist/coach.json`: `{"race_date": "2027-04-12", "race_name": "Maraton"}`; bez niego
odliczanie jest ukryte.

## Dane

`~/.local/share/mist/garmin.db`, prawa 0600. Zwykły SQLite: `sqlite3 ~/.local/share/mist/garmin.db .tables`.
Usuń go, żeby zacząć od nowa; następny sync zapisze go ponownie.

[garmin-grafana]: https://github.com/arpanghosh8453/garmin-grafana
