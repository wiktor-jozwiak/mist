// Local store for the Garmin and running cards: one SQLite file written by `mistd garmin sync` and read by
// `mistd data garmin|running`. Replaces the InfluxDB of garmin-grafana (no container, no password, no port).
// File: ~/.local/share/mist/garmin.db (0600, health data). Timestamps are unix seconds, UTC.
use crate::common::home;
use crate::data::{local_midnight, r0, r1};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use rusqlite::{Connection, params};
use serde_json::{Value as J, json};
use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS device_sync (timestamp INTEGER PRIMARY KEY, device_name TEXT);
    CREATE TABLE IF NOT EXISTS training_readiness (
        timestamp INTEGER PRIMARY KEY, score INTEGER, level TEXT, recovery_time INTEGER, acute_load INTEGER,
        acwr_factor_percent INTEGER, hrv_factor_percent INTEGER, sleep_score INTEGER, sleep_score_factor_percent INTEGER,
        stress_history_factor_percent INTEGER, recovery_time_factor_percent INTEGER);
    CREATE TABLE IF NOT EXISTS body_battery_intraday (timestamp INTEGER PRIMARY KEY, level INTEGER);
    CREATE TABLE IF NOT EXISTS stress_intraday (timestamp INTEGER PRIMARY KEY, level INTEGER);
    CREATE TABLE IF NOT EXISTS sleep_summary (
        timestamp INTEGER PRIMARY KEY, sleep_time_seconds INTEGER, sleep_score INTEGER, avg_overnight_hrv REAL,
        resting_heart_rate INTEGER, deep_sleep_seconds INTEGER, light_sleep_seconds INTEGER, rem_sleep_seconds INTEGER,
        awake_sleep_seconds INTEGER, avg_sleep_stress REAL, restless_moments_count INTEGER);
    CREATE TABLE IF NOT EXISTS steps_intraday (timestamp INTEGER PRIMARY KEY, steps INTEGER);
    CREATE TABLE IF NOT EXISTS race_predictions (
        timestamp INTEGER PRIMARY KEY, time_5k INTEGER, time_10k INTEGER, time_half_marathon INTEGER, time_marathon INTEGER);
    CREATE TABLE IF NOT EXISTS activity_summary (
        id INTEGER PRIMARY KEY AUTOINCREMENT, activity_id TEXT, timestamp INTEGER, activity_type TEXT, activity_name TEXT,
        distance REAL, moving_duration REAL, average_speed REAL, average_hr REAL, training_load REAL, aerobic_te REAL,
        anaerobic_te REAL, hr_zone_1 INTEGER, hr_zone_2 INTEGER, hr_zone_3 INTEGER, hr_zone_4 INTEGER, hr_zone_5 INTEGER,
        UNIQUE(timestamp, activity_type) ON CONFLICT REPLACE);
    CREATE TABLE IF NOT EXISTS suggested_workout (
        timestamp INTEGER PRIMARY KEY, title TEXT, detail TEXT, reason TEXT, kind TEXT, readiness REAL, race_days INTEGER);
    CREATE INDEX IF NOT EXISTS idx_act_type_time ON activity_summary(activity_type, timestamp);
";

pub fn db_path() -> PathBuf {
    let data = std::env::var("XDG_DATA_HOME").ok().filter(|d| d.starts_with('/')).unwrap_or_else(|| format!("{}/.local/share", home()));
    PathBuf::from(format!("{data}/mist/garmin.db"))
}

fn open() -> R<Connection> {
    let p = db_path();
    // first builds kept it next to the config
    let old = PathBuf::from(format!("{}/.config/mist/garmin.db", home()));
    if !p.exists() && old.exists() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::rename(&old, &p);
        let _ = std::fs::remove_file(old.with_extension("db-wal"));
        let _ = std::fs::remove_file(old.with_extension("db-shm"));
    }
    open_at(&p)
}

fn open_at(p: &Path) -> R<Connection> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(p).map_err(|e| format!("sqlite {}: {e}", p.display()))?;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;").map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    Ok(conn)
}

pub struct Readiness {
    pub ts: i64,
    pub score: i64,
    pub level: String,
    pub recovery_time: i64,
    pub acute_load: i64,
    pub acwr_factor: i64,
    pub hrv_factor: i64,
    pub sleep_score: i64,
    pub sleep_factor: i64,
    pub stress_factor: i64,
    pub recovery_factor: i64,
}

pub struct Sleep {
    pub ts: i64, // end of the sleep
    pub seconds: i64,
    pub score: i64,
    pub hrv: f64,
    pub rhr: i64,
    pub deep: i64,
    pub light: i64,
    pub rem: i64,
    pub awake: i64,
    pub avg_stress: f64,
    pub restless: i64,
}

pub struct Race {
    pub ts: i64,
    pub t5k: i64,
    pub t10k: i64,
    pub half: i64,
    pub marathon: i64,
}

pub struct Activity {
    pub id: String,
    pub ts: i64,
    pub kind: String,
    pub name: String,
    pub dist: f64,
    pub moving: f64,
    pub speed: f64,
    pub hr: f64,
    pub load: f64,
    pub ae: f64,
    pub an: f64,
    pub zones: (i64, i64, i64, i64, i64),
}

pub struct Workout {
    pub ts: i64,
    pub title: String,
    pub detail: String,
    pub reason: String,
    pub kind: String,
    pub readiness: f64,
    pub race_days: i64,
}

/// Everything one sync fetched; rows with the same timestamp are replaced, so syncing twice is harmless.
#[derive(Default)]
pub struct Snapshot {
    pub device: Option<(i64, String)>,
    pub readiness: Option<Readiness>,
    pub body_battery: Vec<(i64, i64)>,
    pub stress: Vec<(i64, i64)>,
    pub sleep: Option<Sleep>,
    pub steps: Vec<(i64, i64)>,
    pub race: Option<Race>,
    pub activities: Vec<Activity>,
    pub workout: Option<Workout>,
}

/// Writes the snapshot in one transaction, returns the number of rows.
pub fn save(s: &Snapshot) -> R<usize> {
    save_to(&mut open()?, s).map_err(|e| e.to_string())
}

fn save_to(conn: &mut Connection, s: &Snapshot) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    if let Some((ts, name)) = &s.device {
        n += tx.execute("INSERT OR REPLACE INTO device_sync VALUES (?1, ?2)", params![ts, name])?;
    }
    if let Some(r) = &s.readiness {
        n += tx.execute(
            "INSERT OR REPLACE INTO training_readiness VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![r.ts, r.score, r.level, r.recovery_time, r.acute_load, r.acwr_factor, r.hrv_factor, r.sleep_score,
                r.sleep_factor, r.stress_factor, r.recovery_factor],
        )?;
    }
    for (table, rows) in [("body_battery_intraday", &s.body_battery), ("stress_intraday", &s.stress), ("steps_intraday", &s.steps)] {
        let mut st = tx.prepare_cached(&format!("INSERT OR REPLACE INTO {table} VALUES (?1, ?2)"))?;
        for (ts, v) in rows {
            n += st.execute(params![ts, v])?;
        }
    }
    if let Some(x) = &s.sleep {
        n += tx.execute(
            "INSERT OR REPLACE INTO sleep_summary VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![x.ts, x.seconds, x.score, x.hrv, x.rhr, x.deep, x.light, x.rem, x.awake, x.avg_stress, x.restless],
        )?;
    }
    if let Some(r) = &s.race {
        n += tx.execute("INSERT OR REPLACE INTO race_predictions VALUES (?1, ?2, ?3, ?4, ?5)", params![r.ts, r.t5k, r.t10k, r.half, r.marathon])?;
    }
    {
        let mut st = tx.prepare_cached(
            "INSERT OR REPLACE INTO activity_summary (activity_id, timestamp, activity_type, activity_name, distance,
             moving_duration, average_speed, average_hr, training_load, aerobic_te, anaerobic_te,
             hr_zone_1, hr_zone_2, hr_zone_3, hr_zone_4, hr_zone_5)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )?;
        for a in &s.activities {
            let z = a.zones;
            n += st.execute(params![a.id, a.ts, a.kind, a.name, a.dist, a.moving, a.speed, a.hr, a.load, a.ae, a.an, z.0, z.1, z.2, z.3, z.4])?;
        }
    }
    if let Some(w) = &s.workout {
        n += tx.execute(
            "INSERT OR REPLACE INTO suggested_workout VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![w.ts, w.title, w.detail, w.reason, w.kind, w.readiness, w.race_days],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

/// The readiness card: `mistd data garmin`.
pub fn query_garmin() -> R<J> {
    garmin_from(&open()?, Utc::now().timestamp(), local_midnight(Local::now().date_naive()).timestamp()).map_err(|e| e.to_string())
}

fn garmin_from(c: &Connection, now: i64, midnight: i64) -> rusqlite::Result<J> {
    let day = now - 86400;
    let (ready, level): (Option<i64>, Option<String>) = c
        .query_row("SELECT score, level FROM training_readiness WHERE timestamp > ?1 ORDER BY timestamp DESC LIMIT 1", [now - 2 * 86400], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap_or_default();
    let level = match level.unwrap_or_default().to_uppercase().as_str() {
        "PRIME" => "najwyższa".to_string(), "HIGH" => "wysoka".into(), "MODERATE" => "umiarkowana".into(),
        "LOW" => "niska".into(), "POOR" => "słaba".into(), other => other.to_lowercase(),
    };
    let one = |sql: &str, arg: i64| -> Option<i64> { c.query_row(sql, [arg], |r| r.get(0)).ok().flatten() };
    let bb = one("SELECT level FROM body_battery_intraday WHERE timestamp > ?1 ORDER BY timestamp DESC LIMIT 1", day);
    let stress = one("SELECT level FROM stress_intraday WHERE timestamp > ?1 AND level >= 0 ORDER BY timestamp DESC LIMIT 1", day);
    let steps = one("SELECT SUM(steps) FROM steps_intraday WHERE timestamp >= ?1", midnight).unwrap_or(0);
    // 30-minute means over the last day, for the curve
    let bb_series: Vec<i64> = c
        .prepare("SELECT AVG(level) FROM body_battery_intraday WHERE timestamp > ?1 GROUP BY timestamp / 1800 ORDER BY timestamp / 1800")?
        .query_map([day], |r| r.get::<_, f64>(0))?
        .filter_map(|x| x.ok().map(r0))
        .collect();
    // newest night first; the 7 before it give the HRV baseline
    type Night = (Option<i64>, Option<i64>, Option<f64>, Option<i64>);
    let nights: Vec<Night> = c
        .prepare(
            "SELECT sleep_time_seconds, sleep_score, avg_overnight_hrv, resting_heart_rate FROM sleep_summary
             WHERE timestamp > ?1 ORDER BY timestamp DESC",
        )?
        .query_map([now - 9 * 86400], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(Result::ok)
        .collect();
    let last = nights.first().copied().unwrap_or_default();
    let base: Vec<f64> = nights.iter().skip(1).take(7).filter_map(|n| n.2).filter(|x| *x > 0.0).collect();
    let device: String = c.query_row("SELECT device_name FROM device_sync ORDER BY timestamp DESC LIMIT 1", [], |r| r.get(0)).unwrap_or_else(|_| "Garmin".into());
    let (workout, detail): (Option<String>, Option<String>) = c
        .query_row("SELECT title, detail FROM suggested_workout WHERE timestamp > ?1 ORDER BY timestamp DESC LIMIT 1", [day], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or_default();
    Ok(json!({
        "device": device, "ready": ready, "level": level, "bb": bb, "bbSeries": bb_series,
        "sleepSec": last.0, "sleepScore": last.1, "hrv": last.2.map(r0), "rhr": last.3,
        "hrv7": if base.is_empty() { None } else { Some(r0(base.iter().sum::<f64>() / base.len() as f64)) },
        "stress": stress, "steps": steps, "workout": workout, "workoutDetail": detail,
    }))
}

/// The running card: `mistd data running`.
pub fn query_running() -> R<J> {
    running_from(&open()?, Local::now()).map_err(|e| e.to_string())
}

fn running_from(c: &Connection, now: DateTime<Local>) -> rusqlite::Result<J> {
    let today = now.date_naive();
    let wd = today.weekday().num_days_from_monday() as i64;
    let mon_d = today - Duration::days(wd);
    let mon = local_midnight(mon_d).timestamp();
    let prev_mon = local_midnight(mon_d - Duration::days(7)).timestamp();
    let month = local_midnight(today.with_day(1).unwrap()).timestamp();
    let year = local_midnight(NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap()).timestamp();
    let now_ts = now.timestamp();

    // km per day of this week, and the whole of last week
    let mut week = [0.0f64; 7];
    let mut prev = 0.0;
    let mut st = c.prepare("SELECT timestamp, distance FROM activity_summary WHERE activity_type = 'running' AND timestamp >= ?1")?;
    for (ts, m) in st.query_map([prev_mon], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<f64>>(1)?.unwrap_or(0.0))))?.flatten() {
        if ts < mon {
            prev += m / 1000.0;
        } else if let Some(d) = week.get_mut(((ts - mon) / 86400) as usize) {
            *d += m / 1000.0;
        }
    }
    let since = |t: i64| -> (f64, i64) {
        c.query_row("SELECT COALESCE(SUM(distance), 0), COUNT(*) FROM activity_summary WHERE activity_type = 'running' AND timestamp >= ?1", [t], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap_or((0.0, 0))
    };
    let (month_m, month_n) = since(month);
    let (year_m, year_n) = since(year);
    let p5: Option<i64> = c
        .query_row("SELECT time_5k FROM race_predictions WHERE timestamp > ?1 ORDER BY timestamp DESC LIMIT 1", [now_ts - 7 * 86400], |r| r.get(0))
        .ok()
        .flatten();
    let race_days: Option<i64> = c
        .query_row("SELECT race_days FROM suggested_workout WHERE timestamp > ?1 ORDER BY timestamp DESC LIMIT 1", [now_ts - 86400], |r| r.get(0))
        .ok()
        .flatten();
    let last = c
        .query_row(
            "SELECT activity_name, distance, moving_duration, average_speed, average_hr, timestamp FROM activity_summary
             WHERE activity_type = 'running' ORDER BY timestamp DESC LIMIT 1",
            [],
            |r| {
                let f = |i| r.get::<_, Option<f64>>(i).map(|x| x.unwrap_or(0.0));
                Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), f(1)?, f(2)?, f(3)?, f(4)?, r.get::<_, i64>(5)?))
            },
        )
        .ok()
        .map(|(name, dist, moving, speed, hr, ts)| {
            let pace = if speed > 0.0 {
                let s = 1000.0 / speed;
                format!("{}:{:02}", (s / 60.0) as i64, (s % 60.0) as i64)
            } else {
                "—".into()
            };
            let day = DateTime::from_timestamp(ts, 0).map(|t| t.with_timezone(&Local).date_naive()).unwrap_or(today);
            let ago = match (today - day).num_days() {
                0 => "dziś".to_string(),
                1 => "wczoraj".to_string(),
                n => format!("{n} dni temu"),
            };
            json!({"name": if name.is_empty() { "Bieg".into() } else { name }, "km": r1(dist / 1000.0), "pace": pace,
                "min": r0(moving / 60.0), "hr": r0(hr), "ago": ago})
        });
    Ok(json!({
        "week": week.map(r1), "weekKm": r1(week.iter().sum()), "prevKm": r1(prev),
        "today": wd, "isoWeek": now.iso_week().week(),
        "monthKm": r1(month_m / 1000.0), "monthN": month_n, "yearKm": r1(year_m / 1000.0), "yearN": year_n,
        "p5": p5.filter(|s| *s > 0).map(|s| format!("{}:{:02}", s / 60, s % 60)),
        "last": last,
        "raceDays": race_days.filter(|d| *d >= 0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_then_cards() {
        let dir = std::env::temp_dir().join(format!("mistd-garmin-{}", std::process::id()));
        let path = dir.join("garmin.db");
        let mut c = open_at(&path).unwrap();
        let now = Local::now();
        let ts = now.timestamp();
        let run = |id: &str, ts: i64, dist: f64| Activity {
            id: id.into(), ts, kind: "running".into(), name: "Easy".into(), dist, moving: 1800.0, speed: 2.78, hr: 140.0,
            load: 50.0, ae: 2.5, an: 0.0, zones: (100, 120, 140, 160, 180),
        };
        let snap = Snapshot {
            device: Some((ts, "Forerunner 265".into())),
            readiness: Some(Readiness { ts, score: 71, level: "HIGH".into(), recovery_time: 4, acute_load: 300, acwr_factor: 90,
                hrv_factor: 80, sleep_score: 82, sleep_factor: 85, stress_factor: 70, recovery_factor: 95 }),
            body_battery: vec![(ts - 600, 60), (ts - 300, 58)],
            stress: vec![(ts - 300, -1), (ts - 600, 25)],
            sleep: Some(Sleep { ts: ts - 3600, seconds: 27000, score: 82, hrv: 54.4, rhr: 48, deep: 1, light: 2, rem: 3, awake: 4,
                avg_stress: 12.0, restless: 5 }),
            steps: vec![(ts - 60, 1200)],
            race: Some(Race { ts, t5k: 1335, t10k: 0, half: 0, marathon: 0 }),
            activities: vec![run("1", ts - 60, 5000.0), run("2", ts - 120, 3000.0)],
            workout: Some(Workout { ts, title: "Base".into(), detail: "40 min Z2".into(), reason: String::new(), kind: "easy".into(),
                readiness: 71.0, race_days: 30 }),
        };
        assert_eq!(save_to(&mut c, &snap).unwrap(), 12);
        assert_eq!(save_to(&mut c, &snap).unwrap(), 12); // a second sync replaces, not duplicates

        let g = garmin_from(&c, ts, ts - 3000).unwrap();
        assert_eq!(g["device"], "Forerunner 265");
        assert_eq!((g["ready"].as_i64(), g["level"].as_str()), (Some(71), Some("wysoka")));
        assert_eq!((g["bb"].as_i64(), g["stress"].as_i64(), g["steps"].as_i64()), (Some(58), Some(25), Some(1200)));
        assert_eq!((g["hrv"].as_i64(), g["hrv7"].is_null()), (Some(54), true));
        assert_eq!(g["workout"], "Base");

        let r = running_from(&c, now).unwrap();
        assert_eq!(r["yearN"].as_i64(), Some(2));
        assert_eq!(r["yearKm"].as_f64(), Some(8.0));
        assert_eq!(r["p5"], "22:15");
        assert_eq!(r["raceDays"].as_i64(), Some(30));
        assert_eq!((r["last"]["km"].as_f64(), r["last"]["pace"].as_str(), r["last"]["ago"].as_str()), (Some(5.0), Some("5:59"), Some("dziś")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
