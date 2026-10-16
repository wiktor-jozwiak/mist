# Karty Garmin: synchronizacja z Garmin Connect

[English](garmin.md)

Karta **gotowości** (Training Readiness, krzywa Body Battery, sen, tętno spoczynkowe, stres, HRV, kroki) i
karta **biegania** (kropki tygodnia, ostatni bieg, kilometry w miesiącu / roku, prognoza 5 km, dni do startu)
czytają lokalną bazę InfluxDB, którą wypełnia `garmin-fetch-data` z [garmin-grafana]. Poza logowaniem tego
kontenera do Garmin Connect nic nie wychodzi z komputera.

```
zegarek Garmin ─ Garmin Connect ─ garmin-fetch-data (docker) ─ InfluxDB 127.0.0.1:8086 ─ mistd data garmin|running ─ karty
```

Bez tej konfiguracji obie karty są puste; schowaj je w *Dostosuj pulpit* (prawy klik na zegarze pulpitu)
albo zostaw samą pogodę.

## 1. Kontenery

Potrzebny `docker` z wtyczką compose (`sudo pacman -S docker docker-compose`, `sudo systemctl enable --now docker`).

```bash
mkdir -p ~/garmin-mist && cp ~/.config/quickshell/mist/integrations/garmin/{compose.yml,.env.example} ~/garmin-mist/
```

```bash
cd ~/garmin-mist && cp .env.example .env && mkdir garminconnect-tokens && sudo chown -R 1000:1000 garminconnect-tokens
```

W `~/garmin-mist/.env` ustaw `INFLUXDB_PASSWORD` (długie i losowe, np. `openssl rand -hex 24`) oraz `TZ`.

## 2. Pierwsze logowanie (raz, interaktywnie)

```bash
cd ~/garmin-mist && docker compose run --rm garmin-fetch-data
```

Zapyta o e-mail i hasło Garmin Connect oraz kod 2FA, potem zapisze tokeny sesji w `garminconnect-tokens/`
(hasło nie jest zapisywane). Gdy zacznie pobierać dane, przerwij `Ctrl+C`.

## 3. Uruchom w tle

```bash
cd ~/garmin-mist && docker compose up -d
```

Pierwsze uruchomienie dociąga ostatnie dni, potem pobiera co kilka minut. Grafana jest opcjonalna:
`docker compose --profile grafana up -d`, potem http://127.0.0.1:3000.

## 4. Powiedz Mist, gdzie są dane

`~/.config/mist/influx.json`, do odczytu tylko przez Ciebie:

```json
{ "url": "http://127.0.0.1:8086/query", "db": "GarminStats", "user": "mist", "password": "<INFLUXDB_PASSWORD z .env>" }
```

```bash
chmod 600 ~/.config/mist/influx.json
```

## 5. Sprawdź

```bash
~/.config/quickshell/mist/scripts/bin/mistd data garmin
```

```bash
~/.config/quickshell/mist/scripts/bin/mistd data running
```

Oba wypisują JSON (`ready`, `bb`, `sleepSec`, … / `weekKm`, `last`, …). mist-ui pyta o `garmin` co 5 min, o
`running` co 10 min, więc karty wypełnią się same. `{"error": ...}` oznacza:

| Błąd | Rozwiązanie |
|---|---|
| `brak …/influx.json` | krok 4 |
| `connection refused` | `docker compose ps`: InfluxDB nie działa |
| `brak danych w InfluxDB` | pobieracz jeszcze nic nie zapisał: `docker compose logs garmin-fetch-data` |
| `401` / `authorization failed` | użytkownik / hasło w `influx.json` inne niż w `.env` |

## Trening na dziś (opcjonalnie)

Linia **Dziś** na karcie biegania i stopka karty gotowości korzystają z pomiaru `SuggestedWorkout` (`title`,
`detail`, `race_days`). Garmin nie udostępnia propozycji z zegarka, więc wylicza ją mały skrypt (`coach.py`)
z gotowości, czasu regeneracji, Body Battery, rozkładu obciążeń z 4 tygodni i daty startu, co 30 min. Bez
niego karty po prostu pomijają tę linię.

[garmin-grafana]: https://github.com/arpanghosh8453/garmin-grafana
