// Spotlight's quick answers besides the calculator: units ("5 km to mi", "20 c f"), currencies ("20 usd pln",
// rates from the ECB via frankfurter.app, fetched at most once a day and only when a currency is asked) and
// time zones ("15:00 tokio", "czas w londynie"). Each returns (answer, how it was read) or None.
use crate::spotlight::norm;

// unit names -> (dimension, factor to the base unit); temperatures apart
const UNITS: &[(&[&str], &str, f64)] = &[
    (&["mm", "milimetr", "milimetry", "milimetrow"], "len", 0.001),
    (&["cm", "centymetr", "centymetry", "centymetrow"], "len", 0.01),
    (&["m", "metr", "metry", "metrow", "meter", "meters"], "len", 1.0),
    (&["km", "kilometr", "kilometry", "kilometrow", "kilometer", "kilometers"], "len", 1000.0),
    (&["in", "inch", "inches", "cal", "cale", "cali", "\""], "len", 0.0254),
    (&["ft", "foot", "feet", "stopa", "stopy", "stop"], "len", 0.3048),
    (&["yd", "yard", "yards", "jard", "jardy"], "len", 0.9144),
    (&["mi", "mile", "miles", "mila", "mile", "mil"], "len", 1609.344),
    (&["nmi", "mila morska"], "len", 1852.0),
    (&["mg"], "mass", 0.000001),
    (&["g", "gram", "gramy", "gramow", "grams"], "mass", 0.001),
    (&["dag"], "mass", 0.01),
    (&["kg", "kilogram", "kilogramy", "kilogramow", "kilo"], "mass", 1.0),
    (&["t", "tona", "tony", "ton"], "mass", 1000.0),
    (&["lb", "lbs", "pound", "pounds", "funt", "funty", "funtow"], "mass", 0.453_592_37),
    (&["oz", "ounce", "ounces", "uncja", "uncje"], "mass", 0.028_349_523_125),
    (&["ml", "mililitr", "mililitry"], "vol", 0.001),
    (&["l", "litr", "litry", "litrow", "liter", "liters"], "vol", 1.0),
    (&["gal", "gallon", "gallons", "galon", "galony"], "vol", 3.785_411_784),
    (&["cup", "cups", "szklanka", "szklanki"], "vol", 0.25),
    (&["b", "bajt", "bajty", "bajtow", "byte", "bytes"], "data", 1.0),
    (&["kb"], "data", 1e3),
    (&["mb"], "data", 1e6),
    (&["gb"], "data", 1e9),
    (&["tb"], "data", 1e12),
    (&["kib"], "data", 1024.0),
    (&["mib"], "data", 1_048_576.0),
    (&["gib"], "data", 1_073_741_824.0),
    (&["tib"], "data", 1_099_511_627_776.0),
    (&["km/h", "kmh", "kph"], "speed", 1.0 / 3.6),
    (&["m/s", "mps"], "speed", 1.0),
    (&["mph"], "speed", 0.447_04),
    (&["kn", "knot", "knots", "wezel", "wezly"], "speed", 0.514_444),
    (&["s", "sek", "sekunda", "sekundy", "sekund", "sec", "second", "seconds"], "time", 1.0),
    (&["min", "minuta", "minuty", "minut", "minute", "minutes"], "time", 60.0),
    (&["h", "godz", "godzina", "godziny", "godzin", "hour", "hours"], "time", 3600.0),
    (&["d", "dzien", "dni", "day", "days"], "time", 86400.0),
    (&["tydzien", "tygodnie", "tygodni", "week", "weeks"], "time", 604_800.0),
    (&["m2", "m²"], "area", 1.0),
    (&["km2", "km²"], "area", 1e6),
    (&["ha", "hektar", "hektary"], "area", 1e4),
    (&["ar", "ary"], "area", 100.0),
    (&["acre", "acres", "akr", "akry"], "area", 4046.856_422_4),
    (&["w", "wat", "waty", "watt", "watts"], "power", 1.0),
    (&["kw", "kilowat", "kilowaty"], "power", 1000.0),
    (&["km_hp", "hp", "km_mech"], "power", 745.699_872),
];
const CONNECT: &[&str] = &["to", "in", "na", "w", "=", "->", "into"];

fn unit(s: &str) -> Option<(&'static str, f64, &'static str)> {
    let s = s.trim_end_matches('.');
    UNITS.iter().find(|u| u.0.contains(&s)).map(|u| (u.1, u.2, u.0[0]))
}
fn temp(s: &str) -> Option<char> {
    match s.trim_start_matches('°') {
        "c" | "celsius" | "celsjusz" | "celsjusza" | "stopni" => Some('c'),
        "f" | "fahrenheit" | "fahrenheita" => Some('f'),
        "k" | "kelvin" | "kelwin" | "kelwina" | "kelwinow" => Some('k'),
        _ => None,
    }
}

// "5km" -> ("5", "km"); "1,5" -> 1.5
fn split_num(q: &str) -> Option<(f64, String)> {
    let q = q.trim();
    let end = q.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-')).unwrap_or(q.len());
    if end == 0 {
        return None;
    }
    let n: f64 = q[..end].replace(',', ".").parse().ok()?;
    Some((n, q[end..].trim().to_string()))
}

// the rest after the number: "km to mi" / "km mi" -> ("km", "mi")
fn two_units(rest: &str) -> Option<(String, String)> {
    let w: Vec<&str> = rest.split_whitespace().collect();
    let w: Vec<&str> = w.into_iter().filter(|x| !CONNECT.contains(x)).collect();
    // two-word units ("mila morska") are rare enough to join only when needed
    match w.as_slice() {
        [a, b] => Some((a.to_string(), b.to_string())),
        [a, b, c] if unit(&format!("{a} {b}")).is_some() => Some((format!("{a} {b}"), c.to_string())),
        [a, b, c] if unit(&format!("{b} {c}")).is_some() => Some((a.to_string(), format!("{b} {c}"))),
        _ => None,
    }
}

pub fn fmt(v: f64) -> String {
    let s = if v.abs() >= 1e15 || (v != 0.0 && v.abs() < 1e-6) {
        format!("{v:e}")
    } else {
        let r = (v * 1e6).round() / 1e6;
        let s = format!("{r:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    s.replace('.', ",")
}

pub fn units(raw: &str) -> Option<(String, String)> {
    let q = norm(raw);
    let (n, rest) = split_num(&q)?;
    let (a, b) = two_units(&rest)?;
    if let (Some(x), Some(y)) = (temp(&a), temp(&b)) {
        let c = match x {
            'c' => n,
            'f' => (n - 32.0) * 5.0 / 9.0,
            _ => n - 273.15,
        };
        let v = match y {
            'c' => c,
            'f' => c * 9.0 / 5.0 + 32.0,
            _ => c + 273.15,
        };
        let name = |t: char| match t { 'c' => "°C", 'f' => "°F", _ => "K" };
        return Some((format!("{} {}", fmt(v), name(y)), format!("{} {} =", fmt(n), name(x))));
    }
    let (da, fa, na) = unit(&a)?;
    let (db, fb, nb) = unit(&b)?;
    if da != db {
        return None;
    }
    Some((format!("{} {nb}", fmt(n * fa / fb)), format!("{} {na} =", fmt(n))))
}

// ------------------------------------------------------------------ currencies
fn rates_path() -> String {
    format!("{}/.cache/mist/rates.json", std::env::var("HOME").unwrap_or_default())
}
static FETCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// EUR-based rates, fresh for a day; a stale or missing file starts one fetch in the background
pub fn rates() -> Option<serde_json::Value> {
    let p = rates_path();
    let v: Option<serde_json::Value> = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok());
    let age = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(u64::MAX);
    if age > 86_400 && !FETCHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        std::thread::spawn(move || {
            let out = std::process::Command::new("curl").args(["-sfL", "--max-time", "8", "https://api.frankfurter.app/latest"]).output();
            if let Some(o) = out.ok().filter(|o| o.status.success()) {
                if serde_json::from_slice::<serde_json::Value>(&o.stdout).is_ok_and(|v| v["rates"].is_object()) {
                    let _ = std::fs::create_dir_all(std::path::Path::new(&p).parent().unwrap());
                    let tmp = format!("{p}.tmp");
                    if std::fs::write(&tmp, &o.stdout).is_ok() {
                        let _ = std::fs::rename(&tmp, &p);
                    }
                }
            }
            FETCHING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
    v
}
pub fn fetching() -> bool {
    FETCHING.load(std::sync::atomic::Ordering::SeqCst)
}

fn ccy(s: &str) -> Option<String> {
    let s = match s {
        "zl" | "zloty" | "zlotych" | "zlote" | "pln" => "PLN",
        "$" | "dolar" | "dolary" | "dolarow" | "usd" => "USD",
        "€" | "euro" | "eur" => "EUR",
        "£" | "funt" | "funty" | "funtow" | "gbp" => "GBP",
        "frank" | "franki" | "frankow" | "chf" => "CHF",
        "jen" | "jeny" | "jenow" | "¥" | "jpy" => "JPY",
        s if s.len() == 3 && s.chars().all(|c| c.is_ascii_alphabetic()) => return Some(s.to_uppercase()),
        _ => return None,
    };
    Some(s.into())
}

// Some(None): a currency question while the rates are not here yet
pub fn currency(raw: &str, rates: Option<&serde_json::Value>) -> Option<Option<(String, String)>> {
    let q = norm(raw);
    let (n, rest) = split_num(&q)?;
    let (a, b) = two_units(&rest)?;
    let (a, b) = (ccy(&a)?, ccy(&b)?);
    let Some(r) = rates else { return Some(None) };
    let rate = |c: &str| if c == "EUR" { Some(1.0) } else { r["rates"][c].as_f64() };
    let (ra, rb) = (rate(&a)?, rate(&b)?);
    let v = n / ra * rb;
    let shown = format!("{:.2}", v).replace('.', ",");
    Some(Some((format!("{shown} {b}"), format!("{} {a} · kurs EBC z {}", fmt(n), r["date"].as_str().unwrap_or("")))))
}

// ------------------------------------------------------------------ time zones
const CITY_ALIAS: &[(&str, &str)] = &[
    ("tokio", "Asia/Tokyo"),
    ("londyn", "Europe/London"),
    ("nowy jork", "America/New_York"),
    ("ny", "America/New_York"),
    ("waszyngton", "America/New_York"),
    ("paryz", "Europe/Paris"),
    ("rzym", "Europe/Rome"),
    ("moskwa", "Europe/Moscow"),
    ("pekin", "Asia/Shanghai"),
    ("beijing", "Asia/Shanghai"),
    ("san francisco", "America/Los_Angeles"),
    ("seattle", "America/Los_Angeles"),
    ("dubaj", "Asia/Dubai"),
    ("seul", "Asia/Seoul"),
    ("singapur", "Asia/Singapore"),
    ("kijow", "Europe/Kyiv"),
    ("kyiv", "Europe/Kyiv"),
    ("wilno", "Europe/Vilnius"),
    ("praga", "Europe/Prague"),
    ("wieden", "Europe/Vienna"),
    ("madryt", "Europe/Madrid"),
    ("lizbona", "Europe/Lisbon"),
    ("ateny", "Europe/Athens"),
    ("stambul", "Europe/Istanbul"),
    ("kair", "Africa/Cairo"),
    ("delhi", "Asia/Kolkata"),
    ("new delhi", "Asia/Kolkata"),
    ("bombaj", "Asia/Kolkata"),
    ("mumbai", "Asia/Kolkata"),
    ("warszawa", "Europe/Warsaw"),
    ("sztokholm", "Europe/Stockholm"),
    ("kopenhaga", "Europe/Copenhagen"),
    ("bruksela", "Europe/Brussels"),
    ("utc", "UTC"),
    ("gmt", "UTC"),
];

// a city / zone name -> its tz database name (/usr/share/zoneinfo/Region/City)
pub fn zone(city: &str) -> Option<String> {
    let c = norm(city);
    if c.len() < 2 {
        return None;
    }
    if let Some(z) = CITY_ALIAS.iter().find(|a| a.0 == c) {
        return Some(z.1.into());
    }
    let root = std::env::var("MIST_ZONEINFO").unwrap_or_else(|_| "/usr/share/zoneinfo".into());
    for region in ["Europe", "America", "Asia", "Africa", "Australia", "Pacific", "Atlantic", "Indian"] {
        let Ok(rd) = std::fs::read_dir(format!("{root}/{region}")) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if norm(&n.replace('_', " ")) == c {
                return Some(format!("{region}/{n}"));
            }
            // America/Argentina/Buenos_Aires, America/Indiana/...
            if e.path().is_dir() {
                for e2 in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                    let n2 = e2.file_name().to_string_lossy().into_owned();
                    if norm(&n2.replace('_', " ")) == c {
                        return Some(format!("{region}/{n}/{n2}"));
                    }
                }
            }
        }
    }
    None
}

// "15:00 tokio" / "15 tokio" -> (hour, minute, city); "czas w tokio" / "time in tokyo" / "tokio czas" -> (now, city)
fn tz_query(q: &str) -> Option<(Option<(u32, u32)>, String)> {
    for p in ["ktora godzina w ", "ktora w ", "czas w ", "godzina w ", "time in ", "what time in "] {
        if let Some(c) = q.strip_prefix(p) {
            return Some((None, c.trim().to_string()));
        }
    }
    for s in [" czas", " time", " godzina"] {
        if let Some(c) = q.strip_suffix(s) {
            return Some((None, c.trim().to_string()));
        }
    }
    let (t, city) = q.split_once(' ')?;
    let (h, m) = match t.split_once(':') {
        Some((h, m)) => (h.parse().ok()?, m.parse().ok()?),
        None if t.len() <= 2 => (t.parse().ok()?, 0),
        None => return None,
    };
    if h > 23 || m > 59 {
        return None;
    }
    let city = city.trim_start_matches("w ").trim_start_matches("in ").trim();
    Some((Some((h, m)), city.to_string()))
}

fn date(tz: &str, args: &[&str]) -> Option<String> {
    let o = std::process::Command::new("date").args(args).env("TZ", tz).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn time_zone(raw: &str) -> Option<(String, String)> {
    let q = norm(raw);
    let (at, city) = tz_query(&q)?;
    let z = zone(&city)?;
    let local = std::env::var("TZ").ok();
    // the moment: now, or that clock time today here
    let secs = match at {
        None => date(local.as_deref().unwrap_or(""), &["+%s"])?,
        Some((h, m)) => {
            let mut c = std::process::Command::new("date");
            c.args(["-d", &format!("{h:02}:{m:02}"), "+%s"]);
            String::from_utf8_lossy(&c.output().ok()?.stdout).trim().to_string()
        }
    };
    let there = date(&z, &["-d", &format!("@{secs}"), "+%H:%M|%:z|%F"])?;
    let mut it = there.split('|');
    let (hm, off, day) = (it.next()?, it.next()?, it.next()?);
    // another day there: say which
    let here_day = String::from_utf8_lossy(&std::process::Command::new("date").args(["-d", &format!("@{secs}"), "+%F"]).output().ok()?.stdout).trim().to_string();
    let hm = match day.cmp(here_day.as_str()) {
        std::cmp::Ordering::Greater => format!("{hm} ({})", crate::i18n::tr("jutro")),
        std::cmp::Ordering::Less => format!("{hm} ({})", crate::i18n::tr("wczoraj")),
        _ => hm.to_string(),
    };
    let place = z.rsplit('/').next().unwrap_or(&z).replace('_', " ");
    let sub = match at {
        Some((h, m)) => format!("{h:02}:{m:02} tutaj = {place} (UTC{off})"),
        None => format!("teraz · {place} (UTC{off})"),
    };
    Some((hm, sub))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversions() {
        assert_eq!(units("5 km to mi").unwrap().0, "3,106856 mi");
        assert_eq!(units("100 c f").unwrap().0, "212 °F");
        assert_eq!(units("1,5 kg na lb").unwrap().0, "3,306934 lb");
        assert_eq!(units("1 gib mb").unwrap().0, "1073,741824 mb");
        assert!(units("5 km kg").is_none());
        assert!(units("hello").is_none());
        let r = serde_json::json!({"date": "2026-10-23", "rates": {"PLN": 4.25, "USD": 1.08}});
        assert_eq!(currency("108 usd pln", Some(&r)).unwrap().unwrap().0, "425,00 PLN");
        assert_eq!(currency("10 euro na zł", Some(&r)).unwrap().unwrap().0, "42,50 PLN");
        assert_eq!(currency("10 usd pln", None), Some(None));
        assert_eq!(tz_query("15:00 tokio"), Some((Some((15, 0)), "tokio".into())));
        assert_eq!(tz_query("czas w nowy jork"), Some((None, "nowy jork".into())));
        assert_eq!(tz_query("15 km"), Some((Some((15, 0)), "km".into())));
        assert_eq!(zone("tokio").as_deref(), Some("Asia/Tokyo"));
    }
}
