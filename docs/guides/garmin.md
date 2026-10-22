# Garmin cards

[Polski](garmin.pl.md)

The **readiness** card (Training Readiness, Body Battery curve, sleep, resting HR, stress, HRV, steps) and the
**running** card (week dots, last run, month and year distance, 5 km prediction, days to the race) read a local
SQLite file that `mistd garmin sync` fills from Garmin Connect.

```
watch ─ Garmin Connect ─ mistd garmin sync (every 20 min) ─ ~/.local/share/mist/garmin.db ─ mistd data garmin|running ─ cards
```

No server, no container, no password stored. The only thing Mist cannot do itself is the first login with 2FA,
so that step borrows the login of [garmin-grafana] once. Without any of this the two cards are simply not shown.

## 1. Log in once

Needs `docker` with the compose plugin, only for this step.

```bash
mkdir -p ~/garmin-mist && cp ~/.config/quickshell/mist/integrations/garmin/compose.yml ~/garmin-mist/
```

```bash
cd ~/garmin-mist && mkdir -p garminconnect-tokens && sudo chown 1000:1000 garminconnect-tokens && docker compose run --rm garmin-fetch-data
```

Enter the Garmin Connect e-mail, password and 2FA code. When it starts fetching, the tokens are in
`~/garmin-mist/garminconnect-tokens/garmin_tokens.json`; stop it with `Ctrl+C` and clean up:

```bash
cd ~/garmin-mist && docker compose down -v
```

Your password is not stored. mistd refreshes the tokens by itself from now on.

## 2. Sync in the background

```bash
cp ~/.config/quickshell/mist/integrations/garmin/mist-garmin-sync.{service,timer} ~/.config/systemd/user/
```

```bash
systemctl --user daemon-reload && systemctl --user enable --now mist-garmin-sync.timer
```

The first run writes today's data; every run after that adds to it. Sync now by hand:

```bash
~/.config/quickshell/mist/scripts/bin/mistd garmin sync
```

It prints `{"status": "ok", "rows": …, "coach": {…}}`.

## 3. Check

```bash
~/.config/quickshell/mist/scripts/bin/mistd data garmin
```

```bash
~/.config/quickshell/mist/scripts/bin/mistd data running
```

mist-ui asks for `garmin` every 5 minutes and `running` every 10, so the cards fill in by themselves.

| Output | Fix |
|---|---|
| `"status": "disabled"` | no token file: step 1 |
| `cannot read …garmin_tokens.json` | the token file is not readable by you: `sudo chown $USER` it |
| `GET …: http status: 401` on sync | tokens revoked (password change, logout everywhere): step 1 again |
| cards empty, sync ok | the watch has not synced to Garmin Connect today yet |

## Suggested workout

The **Dziś** line on the running card comes from a small rule set inside `mistd garmin sync`: readiness,
recovery time, Body Battery, the last weeks of runs and the race date pick rest, recovery, base, tempo or intervals.
The race is set in `~/.config/mist/coach.json`: `{"race_date": "2027-04-12", "race_name":
"Marathon"}`; without it the countdown is hidden.

## Data

`~/.local/share/mist/garmin.db`, mode 0600. Plain SQLite: `sqlite3 ~/.local/share/mist/garmin.db .tables`.
Delete it to start over; the next sync writes it again.

[garmin-grafana]: https://github.com/arpanghosh8453/garmin-grafana
