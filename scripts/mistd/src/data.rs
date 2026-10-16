// Mist data backend: prints one JSON object for the requested source.
// Usage: mistd data weather|garmin|running|calendar|sys
// Cache in ~/.cache/mist/<src>.json (last good result is returned with "stale": true on failure).
use crate::common::*;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Map, Value as J, json};
use std::collections::{HashMap, HashSet};

const LAT: f64 = 52.2297;
const LON: f64 = 21.0122;
const CITY: &str = "Warszawa";

type R<T> = Result<T, String>;

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(timeout))).build().into()
}

fn get(url: &str, timeout: u64) -> R<Vec<u8>> {
    agent(timeout)
        .get(url)
        .header("User-Agent", "mist-shell/1.0")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(64 << 20)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

fn get_json(url: &str, timeout: u64) -> R<J> {
    serde_json::from_slice(&get(url, timeout)?).map_err(|e| e.to_string())
}

/// Python's round(): ties to even.
fn r0(x: f64) -> i64 {
    x.round_ties_even() as i64
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round_ties_even() / 10.0
}

fn f(v: &J) -> Option<f64> {
    v.as_f64()
}

/// Python's `v or 0`: the value as it came (int stays int), 0 for null / 0.
fn or0(v: &J) -> J {
    if v.is_null() || v.as_f64() == Some(0.0) { json!(0) } else { v.clone() }
}

fn fz(v: &J) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

type Rows = Vec<Map<String, J>>;

// InfluxDB 1.x of garmin-grafana; connection in ~/.config/mist/influx.json (kept out of the repo):
// {"url": "http://127.0.0.1:8086/query", "db": "GarminStats", "user": "...", "password": "..."}
fn influx(qs: &[String]) -> R<Vec<Rows>> {
    let path = format!("{}/.config/mist/influx.json", home());
    let cfg: J = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).ok_or(format!("brak {path}"))?;
    let s = |k: &str, d: &str| cfg[k].as_str().unwrap_or(d).to_string();
    let q = qs.join(";");
    let res = agent(8)
        .get(&s("url", "http://127.0.0.1:8086/query"))
        .query("db", &s("db", "GarminStats"))
        .query("q", &q)
        .query("epoch", "s")
        .query("u", &s("user", ""))
        .query("p", &s("password", ""))
        .header("User-Agent", "mist-shell/1.0")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_json::<J>()
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    for r in res["results"].as_array().ok_or("influx: no results")? {
        let mut rows = vec![];
        if let Some(s) = r["series"].as_array().and_then(|a| a.first()) {
            let cols: Vec<String> = s["columns"].as_array().map(|c| c.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
            for v in s["values"].as_array().into_iter().flatten() {
                let mut m = Map::new();
                for (c, x) in cols.iter().zip(v.as_array().into_iter().flatten()) {
                    m.insert(c.clone(), x.clone());
                }
                rows.push(m);
            }
        }
        out.push(rows);
    }
    Ok(out)
}

fn local(ts: i64) -> DateTime<Local> {
    Local.timestamp_opt(ts, 0).single().unwrap_or_else(Local::now)
}

fn local_midnight(d: NaiveDate) -> DateTime<Local> {
    zoned(&Zone::Local, d.and_hms_opt(0, 0, 0).unwrap())
}

// ---------------------------------------------------------------- weather
fn wmo(c: i64) -> &'static str {
    match c {
        0 => "Bezchmurnie", 1 => "Przeważnie słonecznie", 2 => "Częściowe zachmurzenie", 3 => "Pochmurno",
        45 => "Mgła", 48 => "Szadź", 51 => "Lekka mżawka", 53 => "Mżawka", 55 => "Gęsta mżawka", 56 | 57 => "Marznąca mżawka",
        61 => "Lekki deszcz", 63 => "Deszcz", 65 => "Ulewa", 66 | 67 => "Marznący deszcz", 71 => "Lekki śnieg", 73 => "Śnieg",
        75 => "Intensywny śnieg", 77 => "Ziarna śniegu", 80 => "Przelotny deszcz", 81 => "Przelotne opady", 82 => "Ulewy",
        85 => "Przelotny śnieg", 86 => "Śnieżyca", 95 => "Burza", 96 | 99 => "Burza z gradem", _ => "—",
    }
}

fn kp() -> f64 {
    // NOAA SWPC: last 3-hour Kp; aurora over Poland from ~Kp 6. Optional: 0 on any failure
    let Ok(rows) = get_json("https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json", 5) else { return 0.0 };
    let Some(last) = rows.as_array().and_then(|a| a.last()) else { return 0.0 };
    let v = if last.is_object() { &last["Kp"] } else { &last[1] };
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).unwrap_or(0.0)
}

fn weather() -> R<J> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m,cloud_cover,is_day\
         ,precipitation,rain,showers,snowfall,wind_direction_10m,wind_gusts_10m,visibility\
         &minutely_15=precipitation,snowfall\
         &hourly=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,precipitation_probability\
         ,precipitation,snowfall,cloud_cover,visibility,wind_speed_10m,wind_gusts_10m,wind_direction_10m,is_day,snow_depth\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
         ,precipitation_sum,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant,sunshine_duration,uv_index_max\
         &timezone=Europe%2FWarsaw&forecast_days=5&forecast_minutely_15=8",
        LAT, LON
    );
    let w = get_json(&url, 12)?;
    let (c, d) = (&w["current"], &w["daily"]);
    if !c.is_object() || !d.is_object() {
        return Err("open-meteo: brak danych".into());
    }
    // the next 2 hours in 15-minute steps (mm per 15 min): when does it start / stop raining
    let m = &w["minutely_15"];
    let soon: Vec<f64> = m["precipitation"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(m["snowfall"].as_array().into_iter().flatten())
        .map(|(p, s)| ((fz(p) + fz(s) * 0.7) * 100.0).round_ties_even() / 100.0)
        .collect();
    let names = ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"];
    let times = d["time"].as_array().cloned().unwrap_or_default();
    let tail5 = |v: &J| -> String {
        let s = v.as_str().unwrap_or("");
        s.chars().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect()
    };
    let mut days = vec![];
    for i in 0..times.len().min(5) {
        let ds = times[i].as_str().unwrap_or("");
        let t = NaiveDate::parse_from_str(ds, "%Y-%m-%d").map_err(|e| e.to_string())?;
        let at = |k: &str| &d[k][i];
        days.push(json!({"name": if i == 0 { "Dziś" } else { names[t.weekday().num_days_from_monday() as usize] },
            "code": at("weather_code"), "max": r0(fz(at("temperature_2m_max"))), "min": r0(fz(at("temperature_2m_min"))),
            "pop": or0(at("precipitation_probability_max")),
            "date": ds, "rise": tail5(at("sunrise")), "set": tail5(at("sunset")),
            "rain": r1(fz(at("precipitation_sum"))), "wind": r0(fz(at("wind_speed_10m_max"))),
            "gust": r0(fz(at("wind_gusts_10m_max"))), "dir": or0(at("wind_direction_10m_dominant")),
            "sun": r1(fz(at("sunshine_duration")) / 3600.0), "uv": r0(fz(at("uv_index_max")))}));
    }
    // hourly for the whole forecast (time scrubbing on the card): t = unix seconds of the hour
    let h = &w["hourly"];
    let tz: chrono_tz::Tz = chrono_tz::Europe::Warsaw;
    let mut hourly = vec![];
    for (i, ts) in h["time"].as_array().into_iter().flatten().enumerate() {
        let g = |k: &str, def: J| -> J {
            let v = &h[k][i];
            if v.is_null() { def } else { v.clone() }
        };
        let gf = |k: &str| fz(&h[k][i]);
        let n = NaiveDateTime::parse_from_str(ts.as_str().unwrap_or(""), "%Y-%m-%dT%H:%M").map_err(|e| e.to_string())?;
        let t = tz.from_local_datetime(&n).earliest().map(|x| x.timestamp()).unwrap_or(0);
        hourly.push(json!({"t": t, "temp": r1(gf("temperature_2m")), "feels": r0(gf("apparent_temperature")),
            "hum": g("relative_humidity_2m", json!(0)), "code": g("weather_code", json!(0)), "pop": g("precipitation_probability", json!(0)),
            "pr": g("precipitation", json!(0)), "sn": g("snowfall", json!(0)), "cloud": g("cloud_cover", json!(0)),
            "vis": g("visibility", json!(50000)), "wind": r0(gf("wind_speed_10m")), "gust": r0(gf("wind_gusts_10m")),
            "dir": g("wind_direction_10m", json!(0)), "day": g("is_day", json!(0)), "depth": g("snow_depth", json!(0))}));
    }
    let nowts = now();
    let cur = hourly.iter().rev().find(|x| x["t"].as_f64().unwrap_or(0.0) <= nowts);
    Ok(json!({"city": CITY, "temp": r0(fz(&c["temperature_2m"])), "feels": r0(fz(&c["apparent_temperature"])),
        "code": c["weather_code"], "wind": r0(fz(&c["wind_speed_10m"])), "hum": c["relative_humidity_2m"],
        "cloud": c["cloud_cover"], "day": c["is_day"], "desc": wmo(c["weather_code"].as_i64().unwrap_or(-1)),
        "sunrise": tail5(&d["sunrise"][0]), "sunset": tail5(&d["sunset"][0]), "days": days,
        // live state for the animated card: mm in the last 15 min, cm of snow, wind from (deg), gusts, visibility (m)
        "precip": or0(&c["precipitation"]), "snow": or0(&c["snowfall"]),
        "windDir": or0(&c["wind_direction_10m"]), "gust": r0(fz(&c["wind_gusts_10m"])),
        "vis": c["visibility"], "soon": soon, "hourly": hourly,
        "snowDepth": cur.map(|x| x["depth"].clone()).unwrap_or(json!(0)),
        // for the sky scene: observer (sun / moon positions) and the planetary K index (aurora)
        "lat": LAT, "lon": LON, "kp": kp()}))
}

// ---------------------------------------------------------------- garmin
fn opt_round(v: &J) -> J {
    f(v).map(|x| json!(r0(x))).unwrap_or(J::Null)
}

fn garmin() -> R<J> {
    let mid = local_midnight(Local::now().date_naive()).timestamp();
    let r = influx(&[
        r#"SELECT last("score"), last("level") FROM "TrainingReadiness" WHERE time > now() - 2d"#.into(),
        r#"SELECT mean("BodyBatteryLevel") FROM "BodyBatteryIntraday" WHERE time > now() - 24h GROUP BY time(30m) fill(none)"#.into(),
        r#"SELECT "sleepTimeSeconds","sleepScore","avgOvernightHrv","restingHeartRate" FROM "SleepSummary" WHERE time > now() - 9d ORDER BY time DESC"#.into(),
        r#"SELECT last("stressLevel") FROM "StressIntraday" WHERE time > now() - 1d AND "stressLevel" >= 0"#.into(),
        r#"SELECT "title","detail" FROM "SuggestedWorkout" WHERE time > now() - 1d ORDER BY time DESC LIMIT 1"#.into(),
        format!(r#"SELECT sum("StepsCount") FROM "StepsIntraday" WHERE time >= {}s"#, mid),
        r#"SELECT last("Device_Name") FROM "DeviceSync""#.into(),
        r#"SELECT last("BodyBatteryLevel") FROM "BodyBatteryIntraday" WHERE time > now() - 24h"#.into(),
    ])?;
    if r.len() < 8 {
        return Err("influx: za mało wyników".into());
    }
    if r.iter().all(|x| x.is_empty()) {
        return Err("brak danych w InfluxDB".into());
    }
    let (ready, bbs, sleep, stress, work, steps, dev, bblast) = (&r[0], &r[1], &r[2], &r[3], &r[4], &r[5], &r[6], &r[7]);
    let empty = Map::new();
    let sl = sleep.first().unwrap_or(&empty);
    let hrv7: Vec<f64> = sleep.iter().skip(1).take(7).filter_map(|s| f(s.get("avgOvernightHrv").unwrap_or(&J::Null))).filter(|x| *x != 0.0).collect();
    let rd = ready.first().unwrap_or(&empty);
    let l1 = match rd.get("last_1") {
        Some(J::String(s)) => s.clone(),
        Some(J::Null) | None => String::new(),
        Some(o) => o.to_string(),
    };
    let level = match l1.to_uppercase().as_str() {
        "PRIME" => "najwyższa".to_string(), "HIGH" => "wysoka".into(), "MODERATE" => "umiarkowana".into(),
        "LOW" => "niska".into(), "POOR" => "słaba".into(), _ => l1.to_lowercase(),
    };
    let g = |m: &Map<String, J>, k: &str| m.get(k).cloned().unwrap_or(J::Null);
    let steps_v = steps.first().and_then(|s| f(s.get("sum").unwrap_or(&J::Null))).filter(|x| *x != 0.0).map(r0).unwrap_or(0);
    Ok(json!({
        "device": dev.first().map(|d| g(d, "last")).unwrap_or(json!("Garmin")),
        "ready": opt_round(rd.get("last").unwrap_or(&J::Null)),
        "level": level,
        "bb": bblast.first().map(|b| opt_round(b.get("last").unwrap_or(&J::Null))).unwrap_or(J::Null),
        "bbSeries": bbs.iter().filter_map(|x| f(x.get("mean").unwrap_or(&J::Null))).map(r0).collect::<Vec<_>>(),
        "sleepSec": g(sl, "sleepTimeSeconds"), "sleepScore": g(sl, "sleepScore"),
        "hrv": g(sl, "avgOvernightHrv"),
        "hrv7": if hrv7.is_empty() { J::Null } else { json!(r0(hrv7.iter().sum::<f64>() / hrv7.len() as f64)) },
        "rhr": g(sl, "restingHeartRate"),
        "stress": stress.first().map(|s| opt_round(s.get("last").unwrap_or(&J::Null))).unwrap_or(J::Null),
        "steps": steps_v,
        "workout": work.first().map(|w| g(w, "title")).unwrap_or(J::Null),
        "workoutDetail": work.first().map(|w| g(w, "detail")).unwrap_or(J::Null),
    }))
}

// ---------------------------------------------------------------- running
fn running() -> R<J> {
    let now_l = Local::now();
    let today_d = now_l.date_naive();
    let wd = today_d.weekday().num_days_from_monday() as i64;
    let mon_d = today_d - Duration::days(wd);
    let mon = local_midnight(mon_d);
    let prev_mon = local_midnight(mon_d - Duration::days(7));
    let iso = |t: DateTime<Local>| t.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let month_start = local_midnight(today_d.with_day(1).unwrap());
    let year_start = local_midnight(NaiveDate::from_ymd_opt(today_d.year(), 1, 1).unwrap());
    let r = influx(&[
        format!(r#"SELECT sum("distance") FROM "ActivitySummary" WHERE "activityType" = 'running' AND time >= '{}' GROUP BY time(1d) tz('Europe/Warsaw')"#, iso(prev_mon)),
        r#"SELECT "activityName","distance","movingDuration","averageSpeed","averageHR" FROM "ActivitySummary" WHERE "activityType" = 'running' ORDER BY time DESC LIMIT 1"#.into(),
        format!(r#"SELECT sum("distance"), count("distance") FROM "ActivitySummary" WHERE "activityType" = 'running' AND time >= '{}'"#, iso(month_start)),
        format!(r#"SELECT sum("distance"), count("distance") FROM "ActivitySummary" WHERE "activityType" = 'running' AND time >= '{}'"#, iso(year_start)),
        r#"SELECT last("time5K") FROM "RacePredictions" WHERE time > now() - 7d"#.into(),
        r#"SELECT "race_days" FROM "SuggestedWorkout" WHERE time > now() - 1d ORDER BY time DESC LIMIT 1"#.into(),
    ])?;
    if r.len() < 6 {
        return Err("influx: za mało wyników".into());
    }
    let (days, last, month, year, p5, race) = (&r[0], &r[1], &r[2], &r[3], &r[4], &r[5]);
    let mut week = [0.0f64; 7];
    let mut prev = 0.0;
    for d in days {
        let t = local(d.get("time").and_then(|x| x.as_i64()).unwrap_or(0));
        let km = d.get("sum").and_then(|x| x.as_f64()).unwrap_or(0.0) / 1000.0;
        if t >= mon {
            let i = (t - mon).num_seconds().div_euclid(86400) as usize;
            if i < 7 {
                week[i] += km;
            }
        } else {
            prev += km;
        }
    }
    let gf = |rows: &Rows, k: &str| rows.first().and_then(|m| m.get(k)).and_then(|x| x.as_f64());
    let mut out = json!({"week": week.iter().map(|x| r1(*x)).collect::<Vec<_>>(), "weekKm": r1(week.iter().sum()), "prevKm": r1(prev),
        "today": wd, "isoWeek": now_l.iso_week().week(),
        "monthKm": if month.is_empty() { json!(0) } else { json!(r1(gf(month, "sum").unwrap_or(0.0) / 1000.0)) },
        "monthN": if month.is_empty() { json!(0) } else { month[0].get("count").cloned().unwrap_or(json!(0)) },
        "yearKm": if year.is_empty() { json!(0) } else { json!(r1(gf(year, "sum").unwrap_or(0.0) / 1000.0)) },
        "yearN": if year.is_empty() { json!(0) } else { year[0].get("count").cloned().unwrap_or(json!(0)) },
        "p5": null, "last": null,
        "raceDays": gf(race, "race_days").filter(|x| *x >= 0.0).map(|_| race[0]["race_days"].clone()).unwrap_or(J::Null)});
    if let Some(s) = gf(p5, "last").filter(|x| *x != 0.0) {
        let s = s as i64;
        out["p5"] = json!(format!("{}:{:02}", s / 60, s % 60));
    }
    if let Some(l) = last.first() {
        let lf = |k: &str| l.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
        let km = lf("distance") / 1000.0;
        let sp = lf("averageSpeed");
        let pace = if sp != 0.0 {
            let x = 1000.0 / sp;
            format!("{}:{:02}", (x / 60.0).floor() as i64, (x % 60.0) as i64)
        } else {
            "—".into()
        };
        let ago = (today_d - local(l.get("time").and_then(|x| x.as_i64()).unwrap_or(0)).date_naive()).num_days();
        let name = l.get("activityName").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).unwrap_or("Bieg");
        out["last"] = json!({"name": name, "km": r1(km), "pace": pace, "min": r0(lf("movingDuration") / 60.0),
            "hr": r0(lf("averageHR")),
            "ago": match ago { 0 => "dziś".to_string(), 1 => "wczoraj".to_string(), n => format!("{} dni temu", n) }});
    }
    Ok(out)
}

// ---------------------------------------------------------------- calendar (iCal feeds)
#[derive(Clone)]
enum Zone {
    Local,
    Utc,
    Tz(chrono_tz::Tz),
}

/// Wall time in a zone -> instant (a DST gap moves on by an hour, an overlap takes the earlier one).
fn zoned(z: &Zone, n: NaiveDateTime) -> DateTime<Local> {
    let pick = |n: NaiveDateTime| -> Option<DateTime<Local>> {
        match z {
            Zone::Local => Local.from_local_datetime(&n).earliest(),
            Zone::Utc => Some(Utc.from_utc_datetime(&n).with_timezone(&Local)),
            Zone::Tz(tz) => tz.from_local_datetime(&n).earliest().map(|x| x.with_timezone(&Local)),
        }
    };
    pick(n).or_else(|| pick(n + Duration::hours(1))).unwrap_or_else(Local::now)
}

struct When {
    zone: Zone,
    naive: NaiveDateTime,
    allday: bool,
}

impl When {
    fn ts(&self) -> i64 {
        zoned(&self.zone, self.naive).timestamp()
    }
}

fn pdate(val: &str, params: &str) -> Option<When> {
    let val = val.trim();
    if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") || (val.len() == 8 && val.bytes().all(|c| c.is_ascii_digit())) {
        let d = NaiveDate::parse_from_str(val.get(..8)?, "%Y%m%d").ok()?;
        return Some(When { zone: Zone::Local, naive: d.and_hms_opt(0, 0, 0)?, allday: true });
    }
    if let Some(v) = val.strip_suffix('Z') {
        return Some(When { zone: Zone::Utc, naive: NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?, allday: false });
    }
    let zone = params
        .split(';')
        .find_map(|p| p.strip_prefix("TZID="))
        .and_then(|z| z.trim_matches('"').parse::<chrono_tz::Tz>().ok())
        .map(Zone::Tz)
        .unwrap_or(Zone::Local);
    Some(When { zone, naive: NaiveDateTime::parse_from_str(val.get(..15)?, "%Y%m%dT%H%M%S").ok()?, allday: false })
}

struct Ev {
    start: Option<When>,
    end: Option<i64>,
    title: Option<String>,
    place: Option<String>,
    rrule: Option<String>,
    uid: Option<String>,
    recid: Option<i64>,
    exdates: HashSet<i64>,
}

fn add_months(n: NaiveDateTime, months: i32) -> NaiveDateTime {
    let m = n.month0() as i32 + months;
    let (y, m) = (n.year() + m.div_euclid(12), m.rem_euclid(12) as u32 + 1);
    let d = NaiveDate::from_ymd_opt(y, m, n.day()).or_else(|| NaiveDate::from_ymd_opt(y, m, 28)).unwrap();
    d.and_time(n.time())
}

fn expand(ev: &Ev, start: i64, end: i64) -> Vec<(i64, i64)> {
    let s = ev.start.as_ref().unwrap();
    let s_ts = s.ts();
    let dur = ev.end.unwrap() - s_ts;
    let Some(rr) = &ev.rrule else {
        return if s_ts + dur > start && s_ts < end { vec![(s_ts, s_ts + dur)] } else { vec![] };
    };
    let p: HashMap<&str, &str> = rr.split(';').filter_map(|x| x.split_once('=')).collect();
    let freq = p.get("FREQ").copied().unwrap_or("");
    let iv: i64 = p.get("INTERVAL").and_then(|x| x.parse().ok()).unwrap_or(1);
    let cnt: Option<i64> = p.get("COUNT").and_then(|x| x.parse().ok());
    let until = p.get("UNTIL").and_then(|u| pdate(u, "")).map(|w| w.ts());
    let wdn = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
    let bydays: Option<Vec<i64>> = p.get("BYDAY").filter(|b| !b.is_empty()).map(|b| {
        b.split(',').filter_map(|x| x.get(x.len().saturating_sub(2)..)).filter_map(|x| wdn.iter().position(|w| *w == x)).map(|i| i as i64).collect()
    });
    let mut res = vec![];
    let mut n = 0i64;
    let mut cur = s.naive;
    for _ in 0..3000 {
        let mut cands = vec![cur];
        if freq == "WEEKLY" {
            if let Some(bd) = &bydays {
                let wk = cur - Duration::days(cur.weekday().num_days_from_monday() as i64);
                cands = bd.iter().map(|i| wk + Duration::days(*i)).filter(|c| *c >= s.naive).collect();
                cands.sort();
            }
        }
        for c in cands {
            let c_ts = zoned(&s.zone, c).timestamp();
            if until.is_some_and(|u| c_ts > u) || cnt.is_some_and(|k| n >= k) || c_ts >= end {
                return res;
            }
            n += 1;
            if !ev.exdates.contains(&c_ts) && c_ts + dur > start {
                res.push((c_ts, c_ts + dur));
            }
        }
        cur = match freq {
            "DAILY" => cur + Duration::days(iv),
            "WEEKLY" => cur + Duration::weeks(iv),
            "MONTHLY" => add_months(cur, iv as i32),
            "YEARLY" => add_months(cur, 12 * iv as i32),
            _ => return res,
        };
    }
    res
}

fn unescape(v: &str) -> String {
    v.replace("\\,", ",").replace("\\;", ";").replace("\\n", " ").replace("\\N", " ")
}

/// One iCal feed -> the occurrences between start and end (unix s) as Mist calendar items.
fn parse_feed(raw: &str, fi: usize, holiday: bool, start: i64, end: i64) -> Vec<J> {
    let mut items: Vec<J> = vec![];
    let text = raw.replace("\r\n ", "").replace("\r\n\t", "").replace("\n ", "").replace("\n\t", "");
    let mut evs: Vec<Ev> = vec![];
    let mut ev: Option<Ev> = None;
    let mut overrides: HashMap<String, HashSet<i64>> = HashMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            ev = Some(Ev { start: None, end: None, title: None, place: None, rrule: None, uid: None, recid: None, exdates: HashSet::new() });
        } else if line == "END:VEVENT" {
            if let Some(mut e) = ev.take() {
                if let Some(s) = &e.start {
                    if e.end.is_none() {
                        let n = if s.allday { s.naive + Duration::days(1) } else { s.naive + Duration::hours(1) };
                        e.end = Some(zoned(&s.zone, n).timestamp());
                    }
                    if let (Some(r), Some(u)) = (e.recid, &e.uid) {
                        overrides.entry(u.clone()).or_default().insert(r);
                    }
                    evs.push(e);
                }
            }
        } else if let (Some(e), Some((k, v))) = (ev.as_mut(), line.split_once(':')) {
            let (name, params) = k.split_once(';').unwrap_or((k, ""));
            match name {
                "DTSTART" => e.start = pdate(v, params),
                "DTEND" => e.end = pdate(v, params).map(|w| w.ts()),
                "SUMMARY" => e.title = Some(unescape(v)),
                "LOCATION" => e.place = Some(v.replace("\\,", ",").split(',').next().unwrap_or("").to_string()),
                "RRULE" => e.rrule = Some(v.to_string()),
                "UID" => e.uid = Some(v.to_string()),
                "RECURRENCE-ID" => e.recid = pdate(v, params).map(|w| w.ts()),
                "EXDATE" => {
                    for x in v.split(',') {
                        if let Some(w) = pdate(x, params) {
                            e.exdates.insert(w.ts());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for mut e in evs {
        if e.rrule.is_some() {
            if let Some(o) = e.uid.as_ref().and_then(|u| overrides.get(u)) {
                e.exdates.extend(o.iter().copied());
            }
        }
        let allday = e.start.as_ref().map(|s| s.allday).unwrap_or(false);
        for (s, en) in expand(&e, start, end) {
            items.push(json!({"title": e.title.clone().unwrap_or_else(|| "(bez tytułu)".into()), "where": e.place.clone().unwrap_or_default(),
                "start": s, "end": en, "allDay": allday, "holiday": holiday, "feed": fi}));
        }
    }
    items
}

fn calendar() -> R<J> {
    let path = format!("{}/.config/mist/feeds.json", home());
    let feeds: J = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let today = local_midnight(Local::now().date_naive());
    let (start, end) = ((today - Duration::days(40)).timestamp(), (today + Duration::days(70)).timestamp());
    let mut items: Vec<J> = vec![];
    let (mut fetched, mut last_err) = (0, String::from("brak kalendarzy w feeds.json"));
    for (fi, u) in feeds.as_array().into_iter().flatten().enumerate() {
        let url = u.as_str().or_else(|| u["url"].as_str()).unwrap_or("");
        let holiday = url.contains("holiday");
        match get(url, 15) {
            Ok(raw) => {
                fetched += 1;
                items.extend(parse_feed(&String::from_utf8_lossy(&raw), fi, holiday, start, end));
            }
            Err(e) => last_err = e,
        }
    }
    // no feed answered (offline): an error, so the last good calendar stays instead of an empty one
    if fetched == 0 {
        return Err(last_err);
    }
    items.sort_by_key(|x| x["start"].as_i64().unwrap_or(0));
    Ok(json!({"items": items}))
}

// ---------------------------------------------------------------- system
fn sysinfo() -> R<J> {
    let mem: HashMap<String, f64> = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.to_string(), v.split_whitespace().next()?.parse().ok()?))
        })
        .collect();
    let total = *mem.get("MemTotal").ok_or("meminfo")?;
    let avail = *mem.get("MemAvailable").ok_or("meminfo")?;
    let du = crate::island::statvfs("/").ok_or("statvfs")?;
    let (free, dtotal) = (du.f_bavail as f64 * du.f_frsize as f64, du.f_blocks as f64 * du.f_frsize as f64);
    let up: f64 = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| s.split_whitespace().next()?.parse().ok()).unwrap_or(0.0);
    // the biggest process by RSS
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f64;
    let mut top = (0.0f64, String::new());
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let pid = e.file_name().to_string_lossy().into_owned();
            if !pid.bytes().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let rss = std::fs::read_to_string(format!("/proc/{}/statm", pid))
                .ok()
                .and_then(|s| s.split_whitespace().nth(1)?.parse::<f64>().ok())
                .unwrap_or(0.0)
                * page;
            if rss > top.0 {
                top = (rss, std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default().trim().to_string());
            }
        }
    }
    let top_s = if top.0 > 0.0 { format!("{} {:.1} GB", top.1, top.0 / 1024.0 / 1048576.0) } else { String::new() };
    // CPU temperature: k10temp Tctl (AMD), else the first coretemp / acpitz sensor
    let mut hw: HashMap<String, String> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/hwmon") {
        for e in rd.flatten() {
            if let Ok(n) = std::fs::read_to_string(e.path().join("name")) {
                hw.insert(n.trim().to_string(), e.path().to_string_lossy().into_owned());
            }
        }
    }
    let temp = ["k10temp", "coretemp", "zenpower", "acpitz"].iter().find_map(|n| hw.get(*n)).and_then(|d| {
        std::fs::read_to_string(format!("{}/temp1_input", d)).ok()?.trim().parse::<f64>().ok().map(|t| r0(t / 1000.0))
    });
    // live CPU % comes from Data.qml (/proc/stat every 2 s), no sampling here
    Ok(json!({"memUsed": r1((total - avail) / 1048576.0), "memTotal": r0(total / 1048576.0),
        "diskFree": r0(free / 1e9), "diskTotal": r0(dtotal / 1e9),
        "uptime": format!("{} h {} min", (up / 3600.0) as i64, ((up % 3600.0) / 60.0) as i64), "top": top_s, "cpuTemp": temp}))
}

pub fn main(name: &str) {
    let cache = format!("{}/.cache/mist", home());
    let _ = std::fs::create_dir_all(&cache);
    let path = format!("{}/{}.json", cache, name);
    let res = match name {
        "weather" => weather(),
        "garmin" => garmin(),
        "running" => running(),
        "calendar" => calendar(),
        "sys" => sysinfo(),
        _ => Err(format!("nieznane źródło: {}", name)),
    };
    let data = match res {
        Ok(mut d) => {
            d["at"] = json!(now() as i64);
            if name != "sys" {
                let tmp = format!("{}.tmp", path);
                if std::fs::write(&tmp, d.to_string()).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
            d
        }
        Err(e) => match std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<J>(&s).ok()) {
            Some(mut d) if d.is_object() => {
                d["stale"] = json!(true);
                d
            }
            _ => json!({"error": e}),
        },
    };
    println!("{}", data);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> i64 {
        Local.from_local_datetime(&NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).earliest().unwrap().timestamp()
    }

    fn wall(t: i64) -> String {
        local(t).format("%Y-%m-%d %H:%M").to_string()
    }

    fn feed(events: &str) -> Vec<J> {
        let ics = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{}END:VCALENDAR\r\n", events);
        let mut v = parse_feed(&ics, 0, false, ts("2026-09-01 00:00"), ts("2027-03-01 00:00"));
        v.sort_by_key(|x| x["start"].as_i64().unwrap());
        v
    }

    fn starts(v: &[J]) -> Vec<String> {
        v.iter().map(|x| wall(x["start"].as_i64().unwrap())).collect()
    }

    #[test]
    fn weekly_keeps_wall_time_across_dst() {
        let v = feed("BEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Europe/Warsaw:20261019T100000\r\nDTEND;TZID=Europe/Warsaw:20261019T110000\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nSUMMARY:Trening\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-10-19 10:00", "2026-10-26 10:00", "2026-11-02 10:00"]);
        assert!(v.iter().all(|x| x["end"].as_i64().unwrap() - x["start"].as_i64().unwrap() == 3600));
    }

    #[test]
    fn byday_count() {
        let v = feed("BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20261021T080000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=5\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-10-21 08:00", "2026-10-23 08:00", "2026-10-26 08:00", "2026-10-28 08:00", "2026-10-30 08:00"]);
        assert_eq!(v[0]["title"], "(bez tytułu)");
    }

    #[test]
    fn until_and_exdate() {
        let v = feed("BEGIN:VEVENT\r\nUID:c\r\nDTSTART:20261021T090000\r\nRRULE:FREQ=DAILY;UNTIL=20261025T235959Z\r\nEXDATE:20261023T090000\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-10-21 09:00", "2026-10-22 09:00", "2026-10-24 09:00", "2026-10-25 09:00"]);
    }

    #[test]
    fn interval_daily() {
        let v = feed("BEGIN:VEVENT\r\nUID:d\r\nDTSTART:20261101T070000\r\nRRULE:FREQ=DAILY;INTERVAL=3;COUNT=3\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-11-01 07:00", "2026-11-04 07:00", "2026-11-07 07:00"]);
    }

    #[test]
    fn monthly_31st_clamps_like_python() {
        let v = feed("BEGIN:VEVENT\r\nUID:e\r\nDTSTART:20261031T120000\r\nRRULE:FREQ=MONTHLY;COUNT=3\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-10-31 12:00", "2026-11-28 12:00", "2026-12-28 12:00"]);
    }

    #[test]
    fn recurrence_id_replaces_one_instance() {
        let v = feed("BEGIN:VEVENT\r\nUID:f\r\nDTSTART:20261102T180000\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nSUMMARY:Basen\r\nEND:VEVENT\r\n\
                      BEGIN:VEVENT\r\nUID:f\r\nRECURRENCE-ID:20261109T180000\r\nDTSTART:20261110T190000\r\nSUMMARY:Basen (przeniesiony)\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-11-02 18:00", "2026-11-10 19:00", "2026-11-16 18:00"]);
        assert_eq!(v[1]["title"], "Basen (przeniesiony)");
    }

    #[test]
    fn all_day_after_dst_change_is_local_midnight() {
        let v = feed("BEGIN:VEVENT\r\nUID:g\r\nDTSTART;VALUE=DATE:20261101\r\nSUMMARY:Wszystkich Świętych\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-11-01 00:00"]);
        assert_eq!(v[0]["allDay"], true);
        assert_eq!(wall(v[0]["end"].as_i64().unwrap()), "2026-11-02 00:00");
    }

    #[test]
    fn date_time_value_is_not_all_day() {
        let v = feed("BEGIN:VEVENT\r\nUID:h\r\nDTSTART;VALUE=DATE-TIME:20261105T143000\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-11-05 14:30"]);
        assert_eq!(v[0]["allDay"], false);
    }

    #[test]
    fn utc_and_other_zone() {
        let v = feed("BEGIN:VEVENT\r\nUID:i\r\nDTSTART:20261105T120000Z\r\nEND:VEVENT\r\n\
                      BEGIN:VEVENT\r\nUID:j\r\nDTSTART;TZID=America/New_York:20261105T090000\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2026-11-05 13:00", "2026-11-05 15:00"]);
    }

    #[test]
    fn folding_and_escapes() {
        let v = feed("BEGIN:VEVENT\r\nUID:k\r\nDTSTART:20261105T100000\r\nSUMMARY:Spotkanie\\, długi\r\n  tytuł\\; koniec\r\nLOCATION:Biuro\\, Toruń\\, PL\r\nEND:VEVENT\r\n");
        assert_eq!(v[0]["title"], "Spotkanie, długi tytuł; koniec");
        assert_eq!(v[0]["where"], "Biuro");
    }

    #[test]
    fn window_filter() {
        let v = feed("BEGIN:VEVENT\r\nUID:l\r\nDTSTART:20250101T100000\r\nEND:VEVENT\r\n\
                      BEGIN:VEVENT\r\nUID:m\r\nDTSTART:20280101T100000\r\nEND:VEVENT\r\n\
                      BEGIN:VEVENT\r\nUID:n\r\nDTSTART:20250101T100000\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n");
        assert_eq!(starts(&v), ["2027-01-01 10:00"]);
    }

    #[test]
    fn broken_event_is_skipped() {
        let v = feed("BEGIN:VEVENT\r\nUID:o\r\nSUMMARY:bez daty\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:garbage\r\nEND:VEVENT\r\n");
        assert!(v.is_empty());
    }

    #[test]
    fn python_rounding() {
        assert_eq!((r0(2.5), r0(3.5), r0(-0.5), r0(7.49)), (2, 4, 0, 7));
        assert_eq!(r1(0.25), 0.2);
        assert_eq!(wmo(61), "Lekki deszcz");
        assert_eq!(wmo(1234), "—");
    }
}
