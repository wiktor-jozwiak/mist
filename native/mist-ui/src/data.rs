// Data sources (was Data.qml): `mistd data <src>` every few minutes (weather 10 min, Garmin 5, running 10,
// calendar 15); a failed / stale answer is retried after 60 s. Plus the date helpers the views use.
use crate::sys::Msg;
use calloop::channel::Sender;
use serde_json::Value;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

// refresh(): wake a source's thread now (the weather place changed); a Condvar, so nothing polls meanwhile
static WAKE: (Mutex<Vec<&'static str>>, Condvar) = (Mutex::new(Vec::new()), Condvar::new());
pub fn refresh(name: &'static str) {
    WAKE.0.lock().unwrap().push(name);
    WAKE.1.notify_all();
}
fn sleep_or_wake(name: &str, d: Duration) {
    let until = std::time::Instant::now() + d;
    let mut q = WAKE.0.lock().unwrap();
    loop {
        if let Some(i) = q.iter().position(|n| *n == name) {
            q.remove(i);
            return;
        }
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return;
        }
        q = WAKE.1.wait_timeout(q, left).unwrap().0;
    }
}

pub fn spawn(tx: Sender<Msg>) {
    for (name, every) in [("weather", 600u64), ("garmin", 300), ("running", 600), ("calendar", 900)] {
        let tx = tx.clone();
        let _ = std::thread::Builder::new().name(format!("data-{name}")).stack_size(128 * 1024).spawn(move || loop {
            let bin = crate::paths::mistd();
            let out = std::process::Command::new(bin).args(["data", name]).stderr(std::process::Stdio::null()).output();
            let v: Option<Value> = out.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok());
            // no place chosen yet is not a failure: wait for refresh() instead of asking every minute
            let unset = v.as_ref().and_then(|v| v["error"].as_str()).map(|e| e == "brak lokalizacji").unwrap_or(false);
            let retry = !unset && v.as_ref().map(|v| v.get("stale").and_then(|x| x.as_bool()).unwrap_or(false) || v.get("error").is_some()).unwrap_or(true);
            if let Some(v) = v {
                let _ = tx.send(Msg::Data(name.to_string(), v));
            }
            sleep_or_wake(name, Duration::from_secs(if retry { 60 } else { every }));
        });
    }
}

pub const MONTHS_GEN: [&str; 12] = ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"];
pub const DAYS_SHORT: [&str; 7] = ["nd", "pon", "wt", "śr", "czw", "pt", "sob"];

// local time parts of a unix timestamp
pub fn local(ts: i64) -> libc::tm {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = ts as libc::time_t;
        libc::localtime_r(&t, &mut tm);
        tm
    }
}
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
pub fn hm(ts: i64) -> String {
    let t = local(ts);
    format!("{}:{:02}", t.tm_hour, t.tm_min)
}
fn day_start(ts: i64) -> i64 {
    let t = local(ts);
    ts - (t.tm_hour * 3600 + t.tm_min * 60 + t.tm_sec) as i64
}
// Data.rel: dziś / jutro / pojutrze / za n dni / za n tyg.
pub fn rel(ts: i64) -> String {
    let n = ((day_start(ts) - day_start(now())) as f64 / 86400.0).round() as i64;
    match n {
        0 => "dziś".into(),
        1 => "jutro".into(),
        2 => "pojutrze".into(),
        n if n < 7 => format!("za {n} dni"),
        n => format!("za {} tyg.", (n as f64 / 7.0).round()),
    }
}
pub fn when(ev: &Value) -> String {
    if ev["allDay"].as_bool().unwrap_or(false) {
        return "cały dzień".into();
    }
    format!("{}–{}", hm(ev["start"].as_i64().unwrap_or(0)), hm(ev["end"].as_i64().unwrap_or(0)))
}
// Data.upcoming(n, daysAhead): events not over yet, starting within the window, no holidays
pub fn upcoming(cal: &Value, n: usize, days: i64) -> Vec<Value> {
    let t = now();
    cal["items"].as_array().map(|a| a.iter().filter(|x| x["end"].as_i64().unwrap_or(0) > t && x["start"].as_i64().unwrap_or(0) < t + days * 86400 && !x["holiday"].as_bool().unwrap_or(false)).take(n).cloned().collect()).unwrap_or_default()
}
// Theme.wxIcon
pub fn wx_icon(code: i64, day: i64) -> &'static str {
    match code {
        0 | 1 => if day == 0 { "moon" } else { "sun" },
        2 => if day == 0 { "moonStars" } else { "cloudSun" },
        3 => "cloud",
        45 | 48 => "fog",
        71..=77 | 85 | 86 => "snow",
        c if c >= 95 => "storm",
        _ => "rain",
    }
}
