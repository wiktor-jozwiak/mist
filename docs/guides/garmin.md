# Garmin cards: sync from Garmin Connect

[Polski](garmin.pl.md)

The **readiness** card (Training Readiness, Body Battery curve, sleep, resting HR, stress, HRV, steps) and
the **running** card (week dots, last run, month / year distance, 5 km prediction, days to the race) read a
local InfluxDB filled by [garmin-grafana]'s `garmin-fetch-data`. Nothing is sent anywhere except the login to
Garmin Connect done by that container.

```
Garmin watch ─ Garmin Connect ─ garmin-fetch-data (docker) ─ InfluxDB 127.0.0.1:8086 ─ mistd data garmin|running ─ cards
```

Without this setup the two cards stay empty; hide them in *Dostosuj pulpit* (right click on the desktop
clock) or keep only the weather card.

## 1. Containers

Needs `docker` with the compose plugin (`sudo pacman -S docker docker-compose`, `sudo systemctl enable --now docker`).

```bash
mkdir -p ~/garmin-mist && cp ~/.config/quickshell/mist/integrations/garmin/{compose.yml,.env.example} ~/garmin-mist/
```

```bash
cd ~/garmin-mist && cp .env.example .env && mkdir garminconnect-tokens && sudo chown -R 1000:1000 garminconnect-tokens
```

Edit `~/garmin-mist/.env`: set `INFLUXDB_PASSWORD` (long and random, e.g. `openssl rand -hex 24`) and `TZ`.

## 2. First login (once, interactive)

```bash
cd ~/garmin-mist && docker compose run --rm garmin-fetch-data
```

It asks for the Garmin Connect e-mail, password and the 2FA code, then stores session tokens in
`garminconnect-tokens/` (your password is not stored). Stop it with `Ctrl+C` once it starts fetching.

## 3. Run in the background

```bash
cd ~/garmin-mist && docker compose up -d
```

The first run back-fills recent days; afterwards it fetches every few minutes. Grafana dashboards are optional:
`docker compose --profile grafana up -d`, then http://127.0.0.1:3000.

## 4. Tell Mist where the data is

`~/.config/mist/influx.json`, readable only by you:

```json
{ "url": "http://127.0.0.1:8086/query", "db": "GarminStats", "user": "mist", "password": "<INFLUXDB_PASSWORD from .env>" }
```

```bash
chmod 600 ~/.config/mist/influx.json
```

## 5. Check

```bash
~/.config/quickshell/mist/scripts/bin/mistd data garmin
```

```bash
~/.config/quickshell/mist/scripts/bin/mistd data running
```

Both print JSON (`ready`, `bb`, `sleepSec`, … / `weekKm`, `last`, …). mist-ui asks for `garmin` every 5 min and
`running` every 10 min, so the cards fill in by themselves. `{"error": ...}` means:

| Error | Fix |
|---|---|
| `brak …/influx.json` | step 4 |
| `connection refused` | `docker compose ps`: InfluxDB not running |
| `brak danych w InfluxDB` | the fetcher has not written anything yet: `docker compose logs garmin-fetch-data` |
| `401` / `authorization failed` | user / password in `influx.json` differ from `.env` |

## Daily suggested workout (optional)

The **Dziś** line on the running card and the readiness footer use the `SuggestedWorkout` measurement
(`title`, `detail`, `race_days`). Garmin does not export the watch's suggestion, so it comes from a small
script (`coach.py`) that rebuilds it from readiness, recovery time, Body Battery, the 4-week load mix and the
race date, every 30 min. Without it the cards just skip that line.

[garmin-grafana]: https://github.com/arpanghosh8453/garmin-grafana
