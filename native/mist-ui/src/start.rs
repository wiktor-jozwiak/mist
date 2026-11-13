// Start (was Start.qml; Win key / the dock's Start button): find and launch first.
//   left    search (prefixes: = licz, > terminal, ? sieć) with results, or favourites (most launched, then the
//           ~/.config/mist/apps.json groups; max 12) / A–Z, and recent files
//   right   agenda (week strip, the day at a glance, its events + weather), the phone card, now playing
//   footer  prefix hints, lock / log out / reboot / power off (the last three: click twice)
// Launches are counted in ~/.config/mist/usage.json (shared with Spotlight). Keys: ↑↓ results, Tab / ←→ (empty
// field) the views and the grid, Enter, Esc. The surface stops above the dock on the main monitor.
use crate::anim::{Anim, Ease};
use crate::calendar::{day, events_on, today, Day};
use crate::desktop::{border, circle, fill_lin, line_h, rect, text};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::phone::Phone;
use crate::spotlight::{calc, norm};
use crate::text::{Style, Text};
use crate::theme::*;
use serde_json::{json, Value};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

const W: f32 = 1000.0;
const H: f32 = 720.0;
const LW: f32 = 588.0;
const FH: f32 = 52.0;
// the search pill and the power buttons in one row, then the two columns; favourites: 6 x 3
const FY: f32 = 30.0;
const TOP: f32 = 106.0;
const FAVS: usize = 18;
// Mist tokens (theme.rs, Apple dark labels + system colours), as the other panes use them
const M_FG: Rgba = T1;
const M_FG2: Rgba = Rgba(1.0, 1.0, 1.0, 0.8);
const M_MUTED: Rgba = T2;
const M_FAINT: Rgba = T3;
const M_ACC_STRONG: Rgba = Rgba::rgb8(0x3d, 0x9b, 0xff, 1.0);
const M_ACC_SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.2);
const M_BLUE: Rgba = Rgba::rgb8(0x64, 0xd2, 0xff, 1.0);
const M_PURPLE: Rgba = Rgba::rgb8(0xbf, 0x5a, 0xf2, 1.0);
const M_WARN: Rgba = ORANGE;
const M_BAD: Rgba = Rgba::rgb8(0xff, 0x6b, 0x62, 1.0);
const M_TRACK: Rgba = FILL2;
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const INK: Rgba = Rgba::rgb8(0x0e, 0x14, 0x20, 1.0);
const CLEAR: Rgba = Rgba(0.0, 0.0, 0.0, 0.0);
const DAYS: [&str; 7] = ["nd", "pon", "wt", "śr", "czw", "pt", "sob"];
const DAYS_LONG: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];

#[derive(Clone)]
enum Kind {
    App(usize),
    File(String),
    Calc(String),
    Web(String),
    Sh(String),
    Cmd(Vec<String>, bool),
    Phone(&'static str, Option<Value>),
}
#[derive(Clone)]
struct Res {
    kind: Kind,
    cat: &'static str,
    title: String,
    sub: String,
    icon: String,
    act: &'static str,
}
#[derive(Clone, PartialEq, Debug)]
enum Hit {
    Sheet,
    Field,
    Res(usize),
    Switch,
    Cell(usize),
    AZ(usize),
    File(usize),
    Reveal(usize),
    CalHead,
    CalNew,
    Day(usize),
    Soon(i64),
    PhoneExpand,
    PhoneAct(&'static str),
    PhoneMore,
    PhoneAccept,
    PhoneReject,
    PhonePair,
    NoteX(String),
    Note,
    Art,
    Seek,
    Prev,
    Play,
    Next,
    Src,
    Shuffle,
    Loop,
    Power(usize),
    Menu(usize),
}
// the right-click menu: an app (Apps index) at a point in sheet coordinates
struct Menu {
    app: usize,
    x: f32,
    y: f32,
}
// a favourite being dragged: its cell, where the press began, whether it moved far enough to be a drag
struct Drag {
    from: usize,
    at: (f32, f32),
    live: bool,
}

pub struct Start {
    field: Field,
    sel: usize,
    armed: String,
    power_tip: String,
    tab: usize,
    gsel: i32,
    usage: Value,
    pinned: Vec<String>,
    files: Vec<(String, i64)>,
    downloads: Vec<(String, i64)>,
    dl_rx: Option<Receiver<Vec<(String, i64)>>>,
    pick: usize,
    ph: Phone,
    phone_more: bool,
    phone_said: (String, Option<Instant>),
    phone_hint: String,
    amb_key: String,
    amb: Rgba,
    enter: Anim,
    res_scroll: f32,
    az_scroll: f32,
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    ptr: (f32, f32),
    pressed_outside: bool,
    sheet: (f32, f32),
    origin: (f32, f32),
    seek: (f32, f32),
    screen: String,
    playing: bool,
    ticks: u32,
    bg: Option<tiny_skia::Pixmap>,
    // the accent the cached glass was tinted with
    bg_acc: Rgba,
    // the player the card shows when more than one has something (its bus), else the playing one
    np_pick: Option<String>,
    // recent files' sizes (one stat per file, not per paint)
    sizes: std::collections::HashMap<String, u64>,
    // ~/.config/mist/start.json "pinned": once set, the favourites are these (in order) and nothing else
    pins: Option<Vec<String>>,
    menu: Option<Menu>,
    drag: Option<Drag>,
    // the cell a dragged favourite would land on
    drop_at: Option<usize>,
    // a press that only closed the menu / opened it: its release does nothing
    eat: bool,
    // a short footer message ("favourites are full")
    note: (String, Option<Instant>),
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
// folders whose newest files Start lists: the XDG user directories, with the English names where
// user-dirs.dirs has no entry. A directory set to $HOME itself (how xdg-user-dirs marks "disabled") is left
// out: scanning the whole home two levels deep is neither quick nor what the list is for.
fn recent_dirs(xdg: &std::collections::HashMap<String, String>, home: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for (k, en) in [("DOWNLOAD", "Downloads"), ("PICTURES", "Pictures"), ("DOCUMENTS", "Documents"), ("DESKTOP", "Desktop"), ("VIDEOS", "Videos")] {
        let d = xdg.get(k).cloned().unwrap_or_else(|| format!("{home}/{en}"));
        let d = d.trim_end_matches('/').to_string();
        if !d.is_empty() && d != home.trim_end_matches('/') && !v.contains(&d) {
            v.push(d);
        }
    }
    v
}
#[cfg(test)]
mod recent_tests {
    #[test]
    fn folders_follow_the_system() {
        let mut x = std::collections::HashMap::new();
        x.insert("DOWNLOAD".to_string(), "/h/Pobrane".to_string());
        x.insert("PICTURES".to_string(), "/h/Obrazy/".to_string());
        x.insert("DESKTOP".to_string(), "/h/".to_string()); // disabled
        x.insert("VIDEOS".to_string(), "/h/Obrazy".to_string()); // same folder twice
        assert_eq!(super::recent_dirs(&x, "/h"), ["/h/Pobrane", "/h/Obrazy", "/h/Documents"]);
    }
}
fn pins_path() -> String {
    format!("{}/.config/mist/start.json", home())
}
fn usage_path() -> String {
    format!("{}/.config/mist/usage.json", home())
}
fn human_size(b: u64) -> String {
    let k = b as f64 / 1024.0;
    if b < 1024 { format!("{b} B") } else if k < 1024.0 { format!("{:.0} KB", k) } else if k < 1024.0 * 1024.0 { format!("{:.1} MB", k / 1024.0) } else { format!("{:.1} GB", k / 1024.0 / 1024.0) }
}
fn base(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}
fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
fn ago(t: i64) -> String {
    let s = (now_ms() - t) as f64 / 1000.0;
    if s < 90.0 {
        return "przed chwilą".into();
    }
    if s < 3600.0 {
        return format!("{} min temu", (s / 60.0).round());
    }
    if s < 86400.0 {
        return format!("{} godz. temu", (s / 3600.0).round());
    }
    let d = (s / 86400.0).round() as i64;
    if d == 1 { "wczoraj".into() } else { format!("{d} dni temu") }
}
fn file_icon(p: &str) -> &'static str {
    let e = base(p).rsplit('.').next().unwrap_or("").to_lowercase();
    match e.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "avif" | "heic" | "bmp" => "photo",
        "mp4" | "mkv" | "webm" | "mov" | "avi" => "video",
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "opus" => "music",
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "zst" | "bz2" => "fileZip",
        "pdf" | "doc" | "docx" | "odt" | "txt" | "md" | "rtf" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" | "csv" => "fileText",
        "qml" | "js" | "ts" | "py" | "sh" | "json" | "c" | "cpp" | "h" | "rs" | "go" | "lua" | "css" | "html" | "toml" | "yml" | "yaml" | "conf" | "ini" => "terminal",
        _ => "file",
    }
}
// a recent file's chip colour, by kind
fn kind_col(icon: &str) -> Rgba {
    match icon {
        "photo" | "fileText" => M_BLUE,
        "video" => M_PURPLE,
        "music" => Rgba::rgb8(0xff, 0x5e, 0x8a, 1.0),
        "fileZip" => M_WARN,
        "terminal" => Rgba::rgb8(0x5e, 0xd8, 0x8a, 1.0),
        _ => M_FG2,
    }
}
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
// ~/.local/share/recently-used.xbel: newest 80 local files
fn recent_files() -> Vec<(String, i64)> {
    let t = std::fs::read_to_string(format!("{}/.local/share/recently-used.xbel", home())).unwrap_or_default();
    let mut out = Vec::new();
    for chunk in t.split("<bookmark ").skip(1) {
        let attr = |k: &str| chunk.split(&format!("{k}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("");
        let href = attr("href");
        let Some(p) = href.strip_prefix("file://") else { continue };
        let m = attr("modified");
        out.push((url_decode(p), iso_ms(m)));
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.truncate(80);
    out
}
// 2026-10-15T09:12:33.123456Z -> unix ms (UTC)
fn iso_ms(s: &str) -> i64 {
    let n: Vec<i64> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).take(6).filter_map(|x| x.parse().ok()).collect();
    if n.len() < 6 {
        return 0;
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = n[0] as i32 - 1900;
    tm.tm_mon = n[1] as i32 - 1;
    tm.tm_mday = n[2] as i32;
    tm.tm_hour = n[3] as i32;
    tm.tm_min = n[4] as i32;
    tm.tm_sec = n[5] as i32;
    (unsafe { libc::timegm(&mut tm) }) as i64 * 1000
}
// ---------------------------------------------------------------- quick add ("jutro 15 dentysta")
// what the search field says about an event: its title, day, start (h, m) or all day, length in minutes
struct QuickEv {
    title: String,
    day: Day,
    at: Option<(i32, i32)>,
    mins: i64,
}
fn weekday_of(n: &str) -> Option<i32> {
    Some(match n {
        "nd" | "ndz" | "niedz" | "niedziela" | "niedziele" | "sun" | "sunday" => 0,
        "pon" | "poniedzialek" | "mon" | "monday" => 1,
        "wt" | "wtorek" | "tue" | "tuesday" => 2,
        "sr" | "sroda" | "srode" | "wed" | "wednesday" => 3,
        "czw" | "czwartek" | "thu" | "thursday" => 4,
        "pt" | "piatek" | "fri" | "friday" => 5,
        "sob" | "sobota" | "sobote" | "sat" | "saturday" => 6,
        _ => return None,
    })
}
fn hm_of(n: &str) -> Option<(i32, i32)> {
    let (h, m) = n.split_once(':').or_else(|| n.split_once('.'))?;
    let (h, m) = (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m) && n.len() >= 4).then_some((h, m))
}
// only when the text names a day, a date or a clock time, and something is left as the title
fn quick_event(raw: &str, td: Day, now_hm: (i32, i32)) -> Option<QuickEv> {
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    let mut day_off: Option<i32> = None;
    let mut date: Option<Day> = None;
    let mut at: Option<(i32, i32)> = None;
    let mut end: Option<(i32, i32)> = None;
    let mut mins: Option<i64> = None;
    let mut title: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].trim_end_matches([',', ';']);
        let n = norm(w);
        let next = words.get(i + 1).map(|x| norm(x.trim_end_matches([',', ';']))).unwrap_or_default();
        let next_is_day = matches!(next.as_str(), "dzis" | "dzisiaj" | "jutro" | "pojutrze" | "today" | "tomorrow") || weekday_of(&next).is_some();
        // "w sobotę", "na jutro", "o 15" (and "on friday", "at 15"): the little word goes with what follows
        if (n == "w" || n == "we" || n == "na" || n == "on") && next_is_day {
            i += 1;
            continue;
        }
        if (n == "o" || n == "at") && (next.parse::<i32>().is_ok_and(|h| (0..24).contains(&h)) || hm_of(&next).is_some()) {
            let (h, m) = hm_of(&next).unwrap_or((next.parse().unwrap_or(0), 0));
            at = Some((h, m));
            i += 2;
            continue;
        }
        match n.as_str() {
            "dzis" | "dzisiaj" | "today" => day_off = Some(0),
            "jutro" | "tomorrow" => day_off = Some(1),
            "pojutrze" => day_off = Some(2),
            _ => {
                if let Some(wd) = weekday_of(&n) {
                    // the next such day (today's weekday: a week from now)
                    let d = (wd - td.wd).rem_euclid(7);
                    day_off = Some(if d == 0 { 7 } else { d });
                } else if let Some((a, b)) = n.split_once('-').filter(|(a, b)| hm_of(a).is_some() && hm_of(b).is_some()) {
                    at = hm_of(a);
                    end = hm_of(b);
                } else if let Some(hm) = hm_of(&n).filter(|_| n.contains(':') || n.split('.').nth(1).is_some_and(|m| m.parse::<i32>().unwrap_or(0) > 12)) {
                    at = Some(hm);
                } else if let Some(d) = {
                    // 12.11 / 12.11.2026 / 12/11
                    let p: Vec<&str> = n.split(['.', '/']).collect();
                    let dd = p.first().and_then(|x| x.parse::<i32>().ok()).filter(|d| (1..=31).contains(d));
                    let mm = p.get(1).and_then(|x| x.parse::<i32>().ok()).filter(|m| (1..=12).contains(m));
                    let yy = p.get(2).and_then(|x| x.parse::<i32>().ok());
                    match (dd, mm, p.len()) {
                        (Some(dd), Some(mm), 2 | 3) => {
                            let mut d = day(yy.unwrap_or(td.y), mm - 1, dd);
                            if yy.is_none() && d.ts < td.ts {
                                d = day(td.y + 1, mm - 1, dd);
                            }
                            Some(d)
                        }
                        _ => None,
                    }
                } {
                    date = Some(d);
                } else if let Some(h) = n.strip_suffix('h').or_else(|| n.strip_suffix("godz")).and_then(|x| x.parse::<i64>().ok()).filter(|h| (1..=12).contains(h)) {
                    mins = Some(h * 60);
                } else if let Some(m) = n.strip_suffix("min").and_then(|x| x.parse::<i64>().ok()).filter(|m| (5..=600).contains(m)) {
                    mins = Some(m);
                } else if let Some(h) = n.parse::<i32>().ok().filter(|h| (0..24).contains(h) && at.is_none() && (day_off.is_some() || date.is_some())) {
                    // a bare hour right after the day: "jutro 15 dentysta"
                    at = Some((h, 0));
                } else {
                    title.push(w);
                }
            }
        }
        i += 1;
    }
    if day_off.is_none() && date.is_none() && at.is_none() {
        return None;
    }
    let t = title.join(" ");
    if t.chars().count() < 2 {
        return None;
    }
    let mut ch = t.chars();
    let title = ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default();
    let day_ = match (date, day_off) {
        (Some(d), _) => d,
        (None, Some(o)) => day(td.y, td.m, td.d + o),
        // only a time: today if it is still ahead, else tomorrow
        (None, None) => day(td.y, td.m, td.d + if at.is_some_and(|a| a > now_hm) { 0 } else { 1 }),
    };
    let mins = match (at, end) {
        (Some(a), Some(b)) => (((b.0 * 60 + b.1) - (a.0 * 60 + a.1)) as i64).max(15),
        _ => mins.unwrap_or(60),
    };
    Some(QuickEv { title, day: day_, at, mins })
}
fn url_enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}
fn gcal_stamp(ts: i64) -> String {
    let t = crate::data::local(ts);
    format!("{}{:02}{:02}T{:02}{:02}00", t.tm_year + 1900, t.tm_mon + 1, t.tm_mday, t.tm_hour, t.tm_min)
}
// Google Calendar's new-event page, filled in (the agenda reads that calendar)
fn gcal_new(title: &str, d: Day, at: Option<(i32, i32)>, mins: i64) -> String {
    let dates = match at {
        Some((h, m)) => {
            let s = d.ts + (h * 3600 + m * 60) as i64;
            format!("{}/{}", gcal_stamp(s), gcal_stamp(s + mins * 60))
        }
        None => {
            let n = day(d.y, d.m, d.d + 1);
            format!("{}{:02}{:02}/{}{:02}{:02}", d.y, d.m + 1, d.d, n.y, n.m + 1, n.d)
        }
    };
    format!("https://calendar.google.com/calendar/r/eventedit?text={}&dates={}", url_enc(title), dates)
}

fn st(size: f32, w: f32) -> Style {
    Style::ui(size, w)
}
fn caps(c: &mut Canvas, t: &mut Text, x: f32, y: f32, s: &str, col: Rgba) {
    text(c, t, x, y, &s.to_uppercase(), &st(11.0, 650.0).spaced(1.3), col, false, None);
}
fn text_cy(c: &mut Canvas, t: &mut Text, x: f32, cy: f32, s: &str, sty: &Style, col: Rgba, max: Option<f32>) -> f32 {
    let h = line_h(t, sty);
    text(c, t, x, cy - h / 2.0, s, sty, col, false, max)
}
fn plural(n: usize, one: &str, few: &str, many: &str) -> String {
    let w = if n == 1 { one } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) { few } else { many };
    format!("{n} {w}")
}
fn mmss(s: f64) -> String {
    let s = s.max(0.0) as i64;
    format!("{}:{:02}", s / 60, s % 60)
}
fn feed_color(e: &Value) -> Rgba {
    if e["holiday"].as_bool().unwrap_or(false) {
        return M_BAD;
    }
    [accent(), GREEN, M_PURPLE, M_WARN][(e["feed"].as_i64().unwrap_or(0) % 4) as usize]
}
fn hsla(h: f32, s: f32, l: f32) -> Rgba {
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t } else if t < 0.5 { q } else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 } else { p }
    };
    Rgba(f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0), 1.0)
}
// the cover's dominant colour (Start.qml ambient): the best scoring hue bucket, lightness fixed at .72
fn ambient(pm: &tiny_skia::Pixmap) -> Rgba {
    let mut buckets = [(0.0f32, 0.0f32, 0.0f32); 24]; // score sum, saturation sum, count
    let step = ((pm.width() * pm.height()) as usize / 4096).max(1);
    for px in pm.pixels().iter().step_by(step) {
        let a = px.alpha() as f32 / 255.0;
        if a < 0.5 {
            continue;
        }
        let (r, g, b) = (px.red() as f32 / 255.0 / a, px.green() as f32 / 255.0 / a, px.blue() as f32 / 255.0 / a);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let l = (mx + mn) / 2.0;
        let d = mx - mn;
        if d < 0.02 {
            continue;
        }
        let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
        let h = if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 } / 6.0;
        let sc = s * (1.0 - (l - 0.5).abs() * 1.4);
        let i = ((h * 24.0) as usize).min(23);
        buckets[i].0 += sc.max(0.0);
        buckets[i].1 += s;
        buckets[i].2 += 1.0;
    }
    let total: f32 = buckets.iter().map(|b| b.2).sum::<f32>().max(1.0);
    let (i, b) = buckets.iter().enumerate().max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap()).unwrap();
    let score = b.0 / total * 4.0;
    let sat = if b.2 > 0.0 { b.1 / b.2 } else { 0.0 };
    let h = (i as f32 + 0.5) / 24.0;
    if score > 0.12 {
        hsla(h, sat.clamp(0.42, 0.78), 0.72)
    } else if score > 0.03 {
        hsla(h, 0.34, 0.74)
    } else {
        Rgba(0.9, 0.92, 0.95, 1.0)
    }
}

impl Start {
    pub fn new(ph: Phone) -> Start {
        let usage = std::fs::read_to_string(usage_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        let groups: Value = std::fs::read_to_string(format!("{}/.config/mist/apps.json", home())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut pinned: Vec<String> = Vec::new();
        for g in groups["groups"].as_array().cloned().unwrap_or_default() {
            for a in g["apps"].as_array().cloned().unwrap_or_default() {
                let a = a.as_str().unwrap_or("").to_string();
                if !a.is_empty() && !pinned.contains(&a) {
                    pinned.push(a);
                }
            }
        }
        // newest files in the user's Downloads, Pictures, Documents, Desktop and Videos folders, whatever the
        // system calls them (most never reach recently-used.xbel)
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("start-files".into()).stack_size(256 * 1024).spawn(move || {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", "find \"$@\" -maxdepth 2 -type f ! -name '.*' ! -name '*.part' ! -name '*.crdownload' -printf '%T@\\t%p\\n' 2>/dev/null | sort -rn | head -16", "sh"]);
            cmd.args(recent_dirs(&crate::files::fs::xdg_dirs(), &home()));
            let o = cmd.output();
            let v = o
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.split_once('\t')).map(|(t, p)| (p.to_string(), (t.parse::<f64>().unwrap_or(0.0) * 1000.0) as i64)).collect())
                .unwrap_or_default();
            let _ = tx.send(v);
        });
        Start {
            field: Field::default(),
            sel: 0,
            armed: String::new(),
            power_tip: String::new(),
            tab: 0,
            gsel: -1,
            usage,
            pinned,
            files: recent_files(),
            downloads: Vec::new(),
            dl_rx: Some(rx),
            pick: 0,
            ph,
            phone_more: false,
            phone_said: (String::new(), None),
            phone_hint: String::new(),
            amb_key: String::new(),
            amb: accent(),
            enter: { let mut a = Anim::new(0.0); a.set(1.0, 240.0, Ease::OutCubic); a },
            res_scroll: 0.0,
            az_scroll: 0.0,
            hits: Vec::new(),
            ptr: (-1.0, -1.0),
            pressed_outside: false,
            sheet: (0.0, 0.0),
            origin: (0.0, 0.0),
            seek: (0.0, 0.0),
            screen: String::new(),
            playing: false,
            ticks: 0,
            bg: None,
            bg_acc: CLEAR,
            np_pick: None,
            sizes: std::collections::HashMap::new(),
            pins: std::fs::read_to_string(pins_path()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["pinned"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())),
            menu: None,
            drag: None,
            drop_at: None,
            eat: false,
            note: (String::new(), None),
        }
    }
    fn entry_of(&self, id: &str, apps: &crate::apps::Apps) -> Option<usize> {
        let e = apps.lookup(id)?;
        apps.entries.iter().position(|x| std::ptr::eq(x, e))
    }
    // favourites: the pinned apps when the user has pinned (stable: nothing moves by itself); before that the
    // most launched apps first, then the curated groups in order
    fn tab_apps(&self, apps: &crate::apps::Apps) -> Vec<usize> {
        if let Some(pins) = &self.pins {
            let mut out: Vec<usize> = Vec::new();
            for id in pins {
                if let Some(i) = self.entry_of(id, apps).filter(|i| !out.contains(i)) {
                    out.push(i);
                }
            }
            out.truncate(12);
            return out;
        }
        let mut used: Vec<(&String, &Value)> = self.usage.as_object().map(|o| o.iter().collect()).unwrap_or_default();
        used.sort_by(|a, b| b.1["n"].as_i64().cmp(&a.1["n"].as_i64()).then(b.1["t"].as_i64().cmp(&a.1["t"].as_i64())));
        let mut out: Vec<usize> = Vec::new();
        for id in used.into_iter().map(|u| u.0.clone()).chain(self.pinned.iter().cloned()) {
            let Some(i) = self.entry_of(&id, apps) else { continue };
            if apps.entries[i].no_display || out.contains(&i) {
                continue;
            }
            out.push(i);
            if out.len() == FAVS {
                break;
            }
        }
        out
    }
    fn all_apps(apps: &crate::apps::Apps) -> Vec<usize> {
        let mut v: Vec<usize> = (0..apps.entries.len()).filter(|&i| !apps.entries[i].no_display && !apps.entries[i].exec.is_empty()).collect();
        v.sort_by_key(|&i| norm(&apps.entries[i].name));
        v.dedup_by_key(|i| norm(&apps.entries[*i].name));
        v
    }
    // recent files: newest first, one per file "family" (a.7z, a(1).7z, a (2).7z count as one)
    fn sug(&self) -> Vec<(String, i64)> {
        let mut fl: Vec<(String, i64)> = self.files.iter().take(12).cloned().chain(self.downloads.iter().cloned()).collect();
        fl.sort_by(|a, b| b.1.cmp(&a.1));
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for f in fl {
            let b = base(&f.0).to_lowercase();
            let (stem, ext) = b.rsplit_once('.').map(|(s, e)| (s.to_string(), e.to_string())).unwrap_or((b.clone(), String::new()));
            let stem = stem.trim_end_matches(|c: char| c == ')' || c.is_ascii_digit() || c == '(' || c == ' ').to_string();
            let k: String = format!("{stem}.{ext}").chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect();
            if seen.insert(k) {
                out.push(f);
            }
            if out.len() == 8 {
                break;
            }
        }
        out
    }
    fn mode(&self) -> &'static str {
        let q = &self.field.text;
        if q.starts_with('>') { "Terminal" } else if q.starts_with('?') { "Sieć" } else if q.starts_with('=') { "Kalkulator" } else { "" }
    }
    fn results(&self, apps: &crate::apps::Apps) -> Vec<Res> {
        let raw = self.field.text.trim();
        let mut out = Vec::new();
        if raw.is_empty() {
            return out;
        }
        let web = |t: &str| Res { kind: Kind::Web(t.into()), cat: "Sieć", title: format!("Szukaj w sieci: {t}"), sub: "Google".into(), icon: "world".into(), act: "Szukaj" };
        let calc_res = |v: String| Res { kind: Kind::Calc(v.clone()), cat: "Kalkulator", title: format!("= {v}"), sub: "Enter kopiuje wynik".into(), icon: "calc".into(), act: "Kopiuj" };
        if let Some(c) = raw.strip_prefix('>') {
            let c = c.trim();
            if !c.is_empty() {
                out.push(Res { kind: Kind::Sh(c.into()), cat: "Terminal", title: c.into(), sub: "Uruchom w terminalu (Alacritty)".into(), icon: "terminal".into(), act: "Uruchom" });
            }
            return out;
        }
        if let Some(c) = raw.strip_prefix('?') {
            if !c.trim().is_empty() {
                out.push(web(c.trim()));
            }
            return out;
        }
        if let Some(c) = raw.strip_prefix('=') {
            if let Some(v) = calc(&format!("{}+0", c.trim())).or_else(|| calc(c)) {
                out.push(calc_res(v));
            }
            return out;
        }
        let q = norm(raw);
        if let Some(v) = calc(raw) {
            out.push(calc_res(v));
        }
        // "jutro 15 dentysta": a new event in the calendar, filled in
        let tm = crate::data::local(crate::data::now());
        let td = today();
        if let Some(ev) = quick_event(raw, td, (tm.tm_hour, tm.tm_min)) {
            let off = ((ev.day.ts - td.ts) as f64 / 86400.0).round() as i64;
            let dn = match off { 0 => "Dziś".to_string(), 1 => "Jutro".into(), _ => DAYS_LONG[ev.day.wd as usize].into() };
            let when = match ev.at {
                Some((h, m)) => {
                    let e = h * 60 + m + ev.mins as i32;
                    format!("{:02}:{:02}–{:02}:{:02}", h, m, (e / 60) % 24, e % 60)
                }
                None => "cały dzień".into(),
            };
            out.push(Res {
                kind: Kind::Cmd(vec!["xdg-open".into(), gcal_new(&ev.title, ev.day, ev.at, ev.mins)], false),
                cat: "Kalendarz",
                title: format!("Dodaj do kalendarza: {}", ev.title),
                sub: format!("{dn}, {} {} · {when}", ev.day.d, crate::data::MONTHS_GEN[ev.day.m as usize]),
                icon: "calendar".into(),
                act: "Dodaj",
            });
        }
        let mut scored: Vec<(f64, usize)> = Vec::new();
        for (i, a) in apps.entries.iter().enumerate() {
            if a.no_display || a.exec.is_empty() {
                continue;
            }
            let (n, g, k, id) = (norm(&a.name), norm(&a.generic), norm(&a.keywords), norm(&a.id));
            let s = if n.starts_with(&q) { 100.0 } else if n.split_whitespace().any(|w| w.starts_with(&q)) { 80.0 } else if n.contains(&q) { 60.0 } else if g.contains(&q) || id.contains(&q) { 40.0 } else if k.contains(&q) { 30.0 } else { 0.0 };
            if s > 0.0 {
                let u = self.usage[&a.id]["n"].as_f64().unwrap_or(0.0);
                scored.push((s - n.chars().count() as f64 / 100.0 + (u * 2.5).min(25.0), i));
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        // one row per name (two .desktop files for one app, e.g. Discord from the repo and a flatpak)
        let mut names = std::collections::HashSet::new();
        scored.retain(|(_, i)| names.insert(norm(&apps.entries[*i].name)));
        for (_, i) in scored.into_iter().take(6) {
            let a = &apps.entries[i];
            let sub = if !a.generic.is_empty() { a.generic.clone() } else if !a.comment.is_empty() { a.comment.clone() } else { "Aplikacja".into() };
            out.push(Res { kind: Kind::App(i), cat: "Aplikacje", title: a.name.clone(), sub, icon: a.icon.clone(), act: "Otwórz" });
        }
        if q.chars().count() >= 2 {
            let h = home();
            for (p, t) in self.files.iter().filter(|f| norm(base(&f.0)).contains(&q)).take(4) {
                let dir = p.replacen(&h, "~", 1);
                let dir = dir.rsplit_once('/').map(|d| d.0.to_string()).unwrap_or_default();
                out.push(Res { kind: Kind::File(p.clone()), cat: "Pliki", title: base(p).into(), sub: format!("{dir} · {}", ago(*t)), icon: file_icon(p).into(), act: "Otwórz" });
            }
        }
        let bin = crate::paths::self_bin();
        let sid = std::env::var("XDG_SESSION_ID").unwrap_or("self".into());
        let cmds: [(&str, &str, Vec<String>, &str, bool); 8] = [
            ("ustawienia settings preferencje mist pulpit widzety", "Ustawienia Mist", vec![bin.clone(), "msg".into(), "mist".into(), "toggle".into(), "settings".into()], "settings", false),
            ("zablokuj lock", "Zablokuj ekran", vec![bin.clone(), "lock".into()], "lock", false),
            ("ustawienia settings centrum sterowania", "Centrum sterowania", vec![bin.clone(), "msg".into(), "mist".into(), "toggle".into(), "control".into()], "settings", false),
            ("ustawienia settings wyglad qt motyw", "Wygląd aplikacji Qt", vec!["qt6ct".into()], "settings", false),
            ("ustawienia settings wyglad gtk motyw", "Wygląd aplikacji GTK", vec!["nwg-look".into()], "settings", false),
            ("wyloguj logout", "Wyloguj", vec!["loginctl".into(), "terminate-session".into(), sid], "logout", true),
            ("uruchom ponownie restart reboot", "Uruchom ponownie", vec!["systemctl".into(), "reboot".into()], "reload", true),
            ("wylacz zamknij shutdown", "Wyłącz komputer", vec!["systemctl".into(), "poweroff".into()], "power", true),
        ];
        for (k, title, cmd, icon, risky) in cmds {
            if k.split(' ').any(|w| w.starts_with(&q)) {
                out.push(Res { kind: Kind::Cmd(cmd, risky), cat: "Polecenia", title: title.into(), sub: if risky { "Zapyta o potwierdzenie".into() } else { "Polecenie".into() }, icon: icon.into(), act: "Wykonaj" });
            }
        }
        let (online, plugins, name) = {
            let s = self.ph.st.lock().unwrap();
            (s.online(), s.main()["plugins"].as_array().cloned().unwrap_or_default(), s.name())
        };
        let has = |p: &str| online && plugins.iter().any(|x| x.as_str() == Some(p));
        if online {
            let ph: [(&str, &str, &'static str, &str, &str); 6] = [
                ("znajdz telefon zadzwon ring find", "Znajdź telefon", "ring", "bellRing", "findmyphone"),
                ("telefon schowek wyslij clipboard", "Wyślij schowek na telefon", "clip", "clipboard", "clipboard"),
                ("telefon wyslij pliki plik send", "Wyślij pliki na telefon", "pick", "fileUp", "share"),
                ("telefon pliki przegladaj files", "Pliki telefonu", "browse", "folderOpen", "sftp"),
                ("telefon zdjecie aparat photo camera", "Zrób zdjęcie telefonem", "photo", "camera", "photo"),
                ("telefon sms wiadomosci message", "Wiadomości SMS", "sms", "message", "sms"),
            ];
            for (k, title, c, icon, p) in ph {
                if has(p) && k.split(' ').any(|w| w.starts_with(&q)) {
                    out.push(Res { kind: Kind::Phone(c, None), cat: "Telefon", title: title.into(), sub: name.clone(), icon: icon.into(), act: "Wykonaj" });
                }
            }
        }
        // the query itself to the phone: a link opens there, text lands in its clipboard
        if has("share") {
            let url = raw.contains("://") && !raw.contains(char::is_whitespace);
            out.push(Res {
                kind: if url { Kind::Phone("share", Some(json!([raw]))) } else { Kind::Phone("text", Some(json!(raw))) },
                cat: "Telefon",
                title: format!("Wyślij na telefon: {raw}"),
                sub: format!("{name}{}", if url { " · otworzy link" } else { " · trafi do schowka" }),
                icon: "send".into(),
                act: "Wyślij",
            });
        }
        out.push(web(raw));
        out
    }
    fn is_pinned(&self, i: usize, apps: &crate::apps::Apps) -> bool {
        match &self.pins {
            Some(p) => p.iter().any(|id| self.entry_of(id, apps) == Some(i)),
            None => false,
        }
    }
    // the pin list to change: the favourites as shown become the pins on the first change
    fn pins_mut(&mut self, apps: &crate::apps::Apps) -> &mut Vec<String> {
        if self.pins.is_none() {
            let shown = self.tab_apps(apps).iter().map(|&i| apps.entries[i].id.clone()).collect();
            self.pins = Some(shown);
        }
        self.pins.as_mut().unwrap()
    }
    fn save_pins(&mut self) {
        let _ = std::fs::write(pins_path(), json!({ "pinned": self.pins }).to_string());
        self.gsel = -1;
    }
    // true when it was pinned (false: the favourites are full)
    fn pin(&mut self, i: usize, apps: &crate::apps::Apps) -> bool {
        if self.is_pinned(i, apps) {
            return true;
        }
        let id = apps.entries[i].id.clone();
        let p = self.pins_mut(apps);
        if p.len() >= FAVS {
            return false;
        }
        p.push(id);
        self.save_pins();
        true
    }
    fn unpin(&mut self, i: usize, apps: &crate::apps::Apps) {
        let id = apps.entries[i].id.clone();
        let ids: Vec<String> = self.tab_apps(apps).iter().map(|&k| apps.entries[k].id.clone()).collect();
        let p = self.pins_mut(apps);
        p.retain(|x| x != &id && ids.contains(x));
        self.save_pins();
    }
    // a favourite dragged from cell a onto cell b
    fn move_pin(&mut self, a: usize, b: usize, apps: &crate::apps::Apps) {
        let shown: Vec<String> = self.tab_apps(apps).iter().map(|&k| apps.entries[k].id.clone()).collect();
        if a >= shown.len() || a == b {
            return;
        }
        // the shown order is the pin order from here on (drops ids that no longer resolve)
        let mut p = shown;
        let id = p.remove(a);
        p.insert(b.min(p.len()), id);
        self.pins = Some(p);
        self.save_pins();
    }
    fn launch(&mut self, i: usize, apps: &crate::apps::Apps) -> Act {
        let id = apps.entries[i].id.clone();
        if !self.usage.is_object() {
            self.usage = json!({});
        }
        let n = self.usage[&id]["n"].as_i64().unwrap_or(0) + 1;
        self.usage[&id] = json!({ "n": n, "t": now_ms() });
        let _ = std::fs::write(usage_path(), self.usage.to_string());
        Act::Launch(i)
    }
    fn activate(&mut self, r: &Res, apps: &crate::apps::Apps) -> Act {
        match &r.kind {
            Kind::App(i) => self.launch(*i, apps),
            Kind::File(p) => Act::Run(vec!["xdg-open".into(), p.clone()]),
            Kind::Calc(v) => Act::Run(vec!["wl-copy".into(), "--".into(), v.replace(',', ".")]),
            Kind::Web(q) => {
                let enc: String = q.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
                Act::Run(vec!["xdg-open".into(), format!("https://www.google.com/search?q={enc}")])
            }
            Kind::Sh(c) => Act::Run(vec!["alacritty".into(), "-e".into(), "sh".into(), "-c".into(), format!("{c}; printf '\\n[Enter] zamyka'; read _")]),
            // log out / restart / shut down: the session pane's question (as from the bar)
            Kind::Cmd(cmd, true) => Act::Open("session", if cmd[0] == "loginctl" { "logout" } else if cmd[1] == "reboot" { "reboot" } else { "poweroff" }),
            Kind::Cmd(cmd, false) => Act::Run(cmd.clone()),
            Kind::Phone(c, a) => self.phone_cmd(c, a.clone()),
        }
    }
    fn phone_cmd(&mut self, c: &str, a: Option<Value>) -> Act {
        match c {
            "pick" => {
                let id = self.ph.st.lock().unwrap().main()["id"].as_str().unwrap_or("").to_string();
                Act::Run(vec!["kdeconnect-handler".into(), "--device".into(), id])
            }
            "sms" => Act::Run(vec!["kdeconnect-sms".into()]),
            "browse" => {
                self.ph.send("browse", None);
                Act::Close
            }
            "mirror" => {
                self.ph.send("mirror", None);
                Act::Close
            }
            _ => {
                self.ph.send(c, a);
                Act::Close
            }
        }
    }
    fn menu_items(&self, app: usize, apps: &crate::apps::Apps) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![("external", "Otwórz")];
        if self.is_pinned(app, apps) {
            v.push(("pinOff", "Odepnij od Ulubionych"));
            if self.tab_apps(apps).first() != Some(&app) {
                v.push(("arrowUp", "Przesuń na początek"));
            }
        } else {
            v.push(("pinned", "Przypnij do Ulubionych"));
        }
        v
    }
    fn menu_pick(&mut self, k: usize, apps: &crate::apps::Apps) -> Act {
        let Some(m) = self.menu.take() else { return Act::None };
        match self.menu_items(m.app, apps).get(k).map(|x| x.1) {
            Some("Otwórz") => self.launch(m.app, apps),
            Some("Odepnij od Ulubionych") => {
                self.unpin(m.app, apps);
                Act::Redraw
            }
            Some("Przesuń na początek") => {
                if let Some(a) = self.tab_apps(apps).iter().position(|&x| x == m.app) {
                    self.move_pin(a, 0, apps);
                }
                Act::Redraw
            }
            Some(_) => {
                if !self.pin(m.app, apps) {
                    self.note = ("Ulubione są pełne (18): najpierw coś odepnij".into(), Some(Instant::now()));
                }
                Act::Redraw
            }
            None => Act::Redraw,
        }
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1.clone())
    }
    fn hov(&self, h: &Hit) -> bool {
        self.hit().as_ref() == Some(h)
    }
    fn push_hit(&mut self, r: (f32, f32, f32, f32), h: Hit) {
        self.hits.push(((r.0 + self.origin.0, r.1 + self.origin.1, r.2, r.3), h));
    }
    fn app_icon(c: &mut Canvas, cx: &mut Ctx, icon: &str, x: f32, y: f32, size: f32) {
        match cx.imgs.get(icon, (size * 2.0) as u32, cx.icons) {
            Some(pm) => crate::img::draw(c, &pm, x, y, size, size, 0.0, false, 1.0),
            None => {
                rect(c, x, y, size, size, size * 0.24, FILL2);
                c.icon(cx.t, x + size / 2.0, y + size / 2.0, "apps", size * 0.5, M_FG2);
            }
        }
    }
    // the white lens tile of the Control Centre / notification centre
    fn lens(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, tint: f32) {
        crate::glass::Glass { radius: r, tint, tone: WHITE, shine: 0.6, shadow: false, ..Default::default() }.paint(c, x, y, w, h);
    }
    fn chip(c: &mut Canvas, t: &mut Text, x: f32, y: f32, s: f32, icon: &str, col: Rgba) {
        rect(c, x, y, s, s, s * 0.3, col.mul_a(0.14));
        border(c, x, y, s, s, s * 0.3, 1.0, col.mul_a(0.22));
        c.icon(t, x + s / 2.0, y + s / 2.0, icon, s * 0.5, col);
    }

    // ---------------------------------------------------------------- left (origin = left column)
    fn paint_field(&mut self, c: &mut Canvas, cx: &mut Ctx, fw_all: f32) {
        // the search field (sheet width), the blue caret; the prefixes (named) while hovered and empty
        let mode = self.mode();
        // a full-width pill of lit glass (brighter while typing); the lens draws its own rim, no second border
        let typing = !self.field.text.is_empty();
        Self::lens(c, 0.0, 0.0, fw_all, FH, FH / 2.0, if typing { 0.1 } else { 0.07 });
        let t = &mut *cx.t;
        let icon = match mode { "Terminal" => "terminal", "Kalkulator" => "calc", "Sieć" => "world", _ => "search" };
        c.icon(t, 22.0 + 9.0, FH / 2.0, icon, 18.0, if mode.is_empty() { T2 } else { accent() });
        // the typed text as typed ("jutro" stays "jutro" in an English UI)
        let fs = st(16.5, 400.0).verbatim();
        let mut hint_w = 0.0;
        if !mode.is_empty() {
            let hs = st(11.5, 600.0);
            hint_w = t.width(mode, &hs) + 18.0;
            let hx = fw_all - 13.0 - hint_w;
            rect(c, hx, (FH - 26.0) / 2.0, hint_w, 26.0, 13.0, M_ACC_SOFT);
            text_cy(c, t, hx + 9.0, FH / 2.0, mode, &hs, M_ACC_STRONG, None);
        } else if self.field.text.is_empty() && inside(self.ptr, (self.origin.0, self.origin.1, fw_all, FH)) {
            // the prefixes, named, only while the pointer is on the field (cryptic keycaps otherwise)
            let ks = st(11.0, 700.0);
            let ls = st(11.0, 500.0);
            let mut x = fw_all - 14.0;
            for (k, label) in [("?", "Sieć"), (">", "Terminal"), ("=", "Kalkulator")] {
                let lw = t.width(label, &ls);
                let cw = 18.0 + 6.0 + lw + 10.0;
                x -= cw;
                rect(c, x, (FH - 26.0) / 2.0, cw, 26.0, 13.0, FILL);
                rect(c, x + 4.0, (FH - 18.0) / 2.0, 18.0, 18.0, 9.0, FILL2);
                let kw = t.width(k, &ks);
                text_cy(c, t, x + 4.0 + (18.0 - kw) / 2.0, FH / 2.0, k, &ks, M_FG2, None);
                text_cy(c, t, x + 4.0 + 18.0 + 6.0, FH / 2.0, label, &ls, M_MUTED, None);
                x -= 6.0;
            }
            hint_w = fw_all - 14.0 - x;
        }
        let fw = fw_all - 50.0 - hint_w - 22.0;
        if self.field.text.is_empty() {
            rect(c, 50.0, FH / 2.0 - 10.0, 2.0, 20.0, 1.0, accent());
            text_cy(c, t, 55.0, FH / 2.0, "Szukaj aplikacji, plików i poleceń…", &st(16.5, 400.0), T3, Some(fw));
        } else {
            let mut shown: &str = &self.field.text;
            while t.width(shown, &fs) > fw - 4.0 && !shown.is_empty() {
                let mut it = shown.char_indices();
                it.next();
                shown = it.next().map(|(i, _)| &shown[i..]).unwrap_or("");
            }
            let w = t.width(shown, &fs);
            if self.field.sel_all {
                rect(c, 48.0, FH / 2.0 - 11.0, w + 4.0, 22.0, 4.0, accent().alpha(0.35));
            }
            text_cy(c, t, 50.0, FH / 2.0, shown, &fs, M_FG, None);
            if !self.field.sel_all {
                rect(c, 50.0 + w + 1.0, FH / 2.0 - 10.0, 2.0, 20.0, 1.0, accent());
            }
        }
        self.push_hit((0.0, 0.0, fw_all, FH), Hit::Field);
    }
    fn paint_left(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let lh = c.h();
        if !self.field.text.trim().is_empty() {
            self.paint_results(c, cx, 0.0, lh);
        } else {
            self.paint_home(c, cx, 0.0, lh);
        }
    }
    fn paint_results(&mut self, c: &mut Canvas, cx: &mut Ctx, top: f32, bottom: f32) {
        let res = self.results(cx.apps);
        let t = &mut *cx.t;
        if res.is_empty() {
            c.icon(t, LW / 2.0, top + 75.0, "search", 30.0, M_FAINT);
            let s = "Brak wyników";
            let w = t.width(s, &st(13.0, 400.0));
            text(c, t, (LW - w) / 2.0, top + 98.0, s, &st(13.0, 400.0), M_MUTED, false, None);
            return;
        }
        self.sel = self.sel.min(res.len() - 1);
        // layout: section headers + 58 px rows
        let mut ys: Vec<(f32, Option<&str>)> = Vec::new();
        let mut y = 0.0;
        let mut last = "";
        for r in &res {
            let head = (r.cat != last).then_some(r.cat);
            if head.is_some() {
                y += 12.0 + 16.0 + 6.0;
                last = r.cat;
            }
            ys.push((y, head));
            y += 60.0;
        }
        let view = bottom - top;
        let max_sc = (y - view).max(0.0);
        // keep the selection inside the view
        let (sy, _) = ys[self.sel];
        if sy - self.res_scroll < 0.0 {
            self.res_scroll = sy - if ys[self.sel].1.is_some() { 34.0 } else { 0.0 };
        } else if sy + 58.0 - self.res_scroll > view {
            self.res_scroll = sy + 58.0 - view;
        }
        self.res_scroll = self.res_scroll.clamp(0.0, max_sc);
        let mut page = Canvas::new(LW as u32, view as u32);
        let (oy, sc) = (self.origin.1 + top, self.res_scroll);
        let saved = self.origin;
        self.origin = (saved.0, oy - sc);
        for (i, r) in res.iter().enumerate() {
            let (ry, head) = ys[i];
            let y = ry - sc;
            if y + 58.0 < -40.0 || y > view {
                continue;
            }
            let t = &mut *cx.t;
            if let Some(hd) = head {
                caps(&mut page, t, 6.0, y - 22.0, hd, M_FAINT);
            }
            let cur = i == self.sel;
            let warn = matches!(r.kind, Kind::Cmd(_, true)) && self.armed == r.title;
            if cur {
                rect(&mut page, 0.0, y, LW, 58.0, 14.0, if warn { DANGER.mul_a(0.9) } else { accent().mul_a(0.92) });
            }
            match &r.kind {
                Kind::App(_) => Self::app_icon(&mut page, cx, &r.icon, 14.0, y + 11.0, 36.0),
                k => {
                    let col = if cur { WHITE } else if warn { M_BAD } else if matches!(k, Kind::File(_)) { M_FG2 } else if matches!(k, Kind::Sh(_)) { M_WARN } else { accent() };
                    Self::chip(&mut page, cx.t, 14.0, y + 11.0, 36.0, &r.icon, col);
                }
            }
            let t = &mut *cx.t;
            let act = if warn { "Potwierdź" } else { r.act };
            let (as_, rs) = (st(11.5, 500.0), st(12.0, 600.0));
            let aw = t.width(act, &as_) + 7.0 + t.width("↵", &rs) + 18.0;
            let tw = LW - 64.0 - aw - 26.0;
            let ts = st(if matches!(r.kind, Kind::Calc(_)) { 18.0 } else { 14.0 }, 600.0);
            let (h1, h2) = (line_h(t, &ts), line_h(t, &st(11.5, 400.0)));
            let ty = y + (58.0 - h1 - h2 - 1.0) / 2.0;
            text(&mut page, t, 64.0, ty, &r.title, &ts, if cur { WHITE } else if warn { M_BAD } else { M_FG }, false, Some(tw));
            text(&mut page, t, 64.0, ty + h1 + 1.0, if warn { "Naciśnij jeszcze raz, aby potwierdzić" } else { &r.sub }, &st(11.5, 400.0), if cur { WHITE.mul_a(0.78) } else { M_MUTED }, false, Some(tw));
            if cur {
                let ax = LW - 14.0 - aw;
                rect(&mut page, ax, y + 16.0, aw, 26.0, 13.0, Rgba(1.0, 1.0, 1.0, 0.2));
                let w1 = text_cy(&mut page, t, ax + 9.0, y + 29.0, act, &as_, WHITE, None);
                text_cy(&mut page, t, ax + 9.0 + w1 + 7.0, y + 29.0, "↵", &rs, WHITE.mul_a(0.75), None);
            }
            let (a, b) = (y.max(0.0), (y + 58.0).min(view));
            if b > a {
                self.push_hit((0.0, a, LW, b - a), Hit::Res(i));
            }
        }
        self.origin = saved;
        fade(&mut page, sc > 0.5, sc < max_sc - 0.5);
        c.pm.draw_pixmap(0, top as i32, page.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    }
    fn paint_home(&mut self, c: &mut Canvas, cx: &mut Ctx, top: f32, bottom: f32) {
        let all = self.tab == 1;
        {
            // the view switch: a segmented pill, the current half a lit lens, the other one clickable
            let t = &mut *cx.t;
            let ss = st(12.0, 600.0);
            let labels = ["Ulubione", "Wszystkie"];
            let ws = [t.width(labels[0], &ss) + 30.0, t.width(labels[1], &ss) + 30.0];
            let sh = 32.0;
            rect(c, 0.0, top, ws[0] + ws[1] + 6.0, sh, sh / 2.0, Rgba(0.0, 0.0, 0.0, 0.2));
            border(c, 0.0, top, ws[0] + ws[1] + 6.0, sh, sh / 2.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.06));
            let mut x = 3.0;
            for (k, l) in labels.iter().enumerate() {
                let on = (k == 1) == all;
                let r = (x, top + 3.0, ws[k], sh - 6.0);
                let hv = !on && self.hov(&Hit::Switch) && inside(self.ptr, (self.origin.0 + r.0, self.origin.1 + r.1, r.2, r.3));
                if on {
                    Self::lens(c, r.0, r.1, r.2, r.3, r.3 / 2.0, 0.13);
                } else if hv {
                    rect(c, r.0, r.1, r.2, r.3, r.3 / 2.0, FILL);
                }
                let lw = t.width(l, &ss);
                text_cy(c, t, x + (ws[k] - lw) / 2.0, top + sh / 2.0, l, &ss, if on { M_FG } else if hv { M_FG2 } else { M_MUTED }, None);
                if !on {
                    self.push_hit(r, Hit::Switch);
                }
                x += ws[k];
            }
        }
        if !all {
            // the favourites grid: 6 x 3 cells of 99 x 100
            let apps = self.tab_apps(cx.apps);
            let gy = top + 32.0 + 14.0;
            for (i, &ai) in apps.iter().enumerate() {
                let (x, y) = (-4.0 + (i % 6) as f32 * 99.0, gy + (i / 6) as f32 * 104.0);
                let h = Hit::Cell(i);
                let dragging = self.drag.as_ref().filter(|d| d.live).map(|d| d.from);
                let (hov, foc) = (self.hov(&h) && dragging.is_none(), i as i32 == self.gsel);
                if dragging.is_some() && self.drop_at == Some(i) && dragging != Some(i) {
                    Self::lens(c, x, y, 99.0, 100.0, 20.0, 0.06);
                    border(c, x, y, 99.0, 100.0, 20.0, 1.5, accent().mul_a(0.8));
                }
                if dragging == Some(i) {
                    // the dragged one's slot: an outline, its icon follows the pointer
                    border(c, x + 4.0, y + 4.0, 91.0, 92.0, 18.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.14));
                    self.push_hit((x, y, 99.0, 100.0), h);
                    continue;
                }
                if hov || foc {
                    Self::lens(c, x, y, 99.0, 100.0, 20.0, 0.075);
                }
                if foc {
                    border(c, x, y, 99.0, 100.0, 20.0, 1.5, accent().mul_a(0.8));
                }
                let e = &cx.apps.entries[ai];
                let (icon, name) = (e.icon.clone(), e.name.clone());
                let s = 54.0;
                Self::app_icon(c, cx, &icon, x + (99.0 - s) / 2.0, y + 10.0, s);
                let t = &mut *cx.t;
                let ns = st(11.5, 500.0);
                for (k, l) in crate::gfx::wrap(t, &name, &ns, 87.0, 2).iter().enumerate() {
                    let w = t.width(l, &ns);
                    text(c, t, x + (99.0 - w) / 2.0, y + 70.0 + k as f32 * 14.0, l, &ns, if hov { M_FG } else { M_FG2 }, false, None);
                }
                self.push_hit((x, y, 99.0, 100.0), h);
            }
            if apps.is_empty() {
                let t = &mut *cx.t;
                c.icon(t, LW / 2.0, gy + 34.0, "pinned", 22.0, M_FAINT);
                let s = "Prawy przycisk na aplikacji w „Wszystkie” przypina ją tutaj";
                let w = t.width(s, &st(12.0, 450.0));
                text(c, t, (LW - w) / 2.0, gy + 54.0, s, &st(12.0, 450.0), M_MUTED, false, None);
            }
            if let Some(&ai) = self.drag.as_ref().filter(|d| d.live).and_then(|d| apps.get(d.from)) {
                let icon = cx.apps.entries[ai].icon.clone();
                let (px, py) = (self.ptr.0 - self.origin.0, self.ptr.1 - self.origin.1);
                crate::glass::shadow(c, px - 28.0, py - 22.0, 56.0, 56.0, 14.0, 18.0, -2.0, Rgba(0.0, 0.0, 0.0, 0.45));
                Self::app_icon(c, cx, &icon, px - 28.0, py - 28.0, 56.0);
            }
            // recent files: one grouped glass panel of full-width rows (names readable), down to the column's end
            let sug = self.sug();
            if !sug.is_empty() {
                let rows = apps.len().div_ceil(6).max(1) as f32;
                let mut y = gy + rows * 104.0 - 4.0 + 22.0;
                caps(c, cx.t, 4.0, y, "Ostatnie pliki", M_FAINT);
                y += 16.0 + 10.0;
                let avail = bottom - y - 1.0;
                let fit = ((avail / 48.0).floor().max(0.0) as usize).min(sug.len());
                let rh = if fit > 0 { (avail / fit as f32).min(60.0) } else { 0.0 };
                let cw = LW;
                if fit > 0 {
                    Self::lens(c, 0.0, y, LW, fit as f32 * rh, 18.0, 0.045);
                }
                for (i, (p, ts)) in sug.iter().enumerate().take(fit) {
                    let (x, ry) = (0.0, y + i as f32 * rh);
                    if i > 0 {
                        rect(c, x + 62.0, ry, cw - 76.0, 1.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.06));
                    }
                    let h = Hit::File(i);
                    let hov = inside(self.ptr, (self.origin.0 + x, self.origin.1 + ry, cw, rh));
                    if hov {
                        rect(c, x + 4.0, ry + 4.0, cw - 8.0, rh - 8.0, 14.0, FILL);
                    }
                    let fi = file_icon(p);
                    let img = fi == "photo" && !p.to_lowercase().ends_with(".svg");
                    let thumb = if img { cx.imgs.get(p, 96, cx.icons) } else { None };
                    let t = &mut *cx.t;
                    let (ix, iy) = (x + 14.0, ry + (rh - 34.0) / 2.0);
                    match thumb {
                        Some(pm) => {
                            crate::img::draw(c, &pm, ix, iy, 34.0, 34.0, 9.0, true, 1.0);
                            border(c, ix, iy, 34.0, 34.0, 9.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.12));
                        }
                        None => Self::chip(c, t, ix, iy, 34.0, fi, kind_col(fi)),
                    }
                    let a = ago(*ts);
                    let aw = t.width(&a, &st(10.5, 400.0));
                    let tw = cw - 62.0 - aw.max(30.0) - 30.0;
                    let mid = ry + rh / 2.0;
                    text_cy(c, t, x + 62.0, mid - 8.0, base(p), &st(13.0, 600.0), M_FG, Some(tw));
                    let dir = p.rsplit('/').nth(1).unwrap_or("/");
                    let sz = *self.sizes.entry(p.clone()).or_insert_with(|| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
                    let sub = if sz > 0 { format!("{dir} · {}", human_size(sz)) } else { dir.to_string() };
                    text_cy(c, t, x + 62.0, mid + 9.0, &sub, &st(11.0, 450.0), M_MUTED, Some(tw));
                    if hov {
                        let (bx, by) = (x + cw - 14.0 - 30.0, mid - 15.0);
                        let rv = Hit::Reveal(i);
                        rect(c, bx, by, 30.0, 30.0, 15.0, if self.hov(&rv) { accent() } else { FILL2 });
                        c.icon(t, bx + 15.0, by + 15.0, "folderOpen", 14.0, M_FG);
                        self.push_hit((x, ry, cw, rh), h);
                        self.push_hit((bx, by, 30.0, 30.0), rv);
                    } else {
                        text_cy(c, t, x + cw - 16.0 - aw, mid, &a, &st(10.5, 400.0), M_FAINT, None);
                        self.push_hit((x, ry, cw, rh), h);
                    }
                }
            }
        } else {
            // all apps, A–Z, 2 columns of 52 px rows
            let list = Self::all_apps(cx.apps);
            let gy = top + 32.0 + 12.0;
            let view = bottom - gy;
            let rows = list.len().div_ceil(2) as f32;
            let max_sc = (rows * 52.0 - view).max(0.0);
            if self.gsel >= 0 {
                let ry = (self.gsel as usize / 2) as f32 * 52.0;
                if ry < self.az_scroll {
                    self.az_scroll = ry;
                } else if ry + 52.0 > self.az_scroll + view {
                    self.az_scroll = ry + 52.0 - view;
                }
            }
            self.az_scroll = self.az_scroll.clamp(0.0, max_sc);
            let mut page = Canvas::new((LW + 8.0) as u32, view as u32);
            let saved = self.origin;
            self.origin = (saved.0 - 4.0, saved.1 + gy - self.az_scroll);
            let cw = (LW + 8.0) / 2.0;
            let first = (self.az_scroll / 52.0) as usize * 2;
            for (k, &ai) in list.iter().enumerate().skip(first) {
                let (x, y) = ((k % 2) as f32 * cw, (k / 2) as f32 * 52.0 - self.az_scroll);
                if y > view {
                    break;
                }
                let h = Hit::AZ(k);
                let (hov, foc) = (self.hov(&h), k as i32 == self.gsel);
                if hov || foc {
                    Self::lens(&mut page, x + 2.0, y + 2.0, cw - 4.0, 48.0, 14.0, 0.075);
                    if foc {
                        border(&mut page, x + 2.0, y + 2.0, cw - 4.0, 48.0, 14.0, 1.5, accent().mul_a(0.8));
                    }
                }
                let (icon, name, sub) = { let e = &cx.apps.entries[ai]; (e.icon.clone(), e.name.clone(), if !e.generic.is_empty() { e.generic.clone() } else { e.comment.clone() }) };
                Self::app_icon(&mut page, cx, &icon, x + 12.0, y + 10.0, 32.0);
                let t = &mut *cx.t;
                if sub.is_empty() {
                    text_cy(&mut page, t, x + 56.0, y + 26.0, &name, &st(13.0, 500.0), M_FG, Some(cw - 66.0));
                } else {
                    text_cy(&mut page, t, x + 56.0, y + 18.0, &name, &st(13.0, 500.0), M_FG, Some(cw - 66.0));
                    text_cy(&mut page, t, x + 56.0, y + 35.0, &sub, &st(11.0, 400.0), M_MUTED, Some(cw - 66.0));
                }
                let (a, b) = (y.max(0.0), (y + 52.0).min(view));
                if b > a {
                    self.push_hit((x, a + self.az_scroll, cw, b - a), h);
                }
            }
            self.origin = saved;
            fade(&mut page, self.az_scroll > 0.5, self.az_scroll < max_sc - 0.5);
            c.pm.draw_pixmap(-4, gy as i32, page.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        }
    }

    // ---------------------------------------------------------------- right column (origin = column top-left)
    fn wx_at(wx: &Value, e: &Value) -> Option<(String, i64, bool)> {
        if e["holiday"].as_bool().unwrap_or(false) {
            return None;
        }
        let start = e["start"].as_i64().unwrap_or(0);
        if e["allDay"].as_bool().unwrap_or(false) {
            let t = crate::data::local(start);
            let k = format!("{}-{:02}-{:02}", t.tm_year + 1900, t.tm_mon + 1, t.tm_mday);
            let d = wx["days"].as_array()?.iter().find(|d| d["date"].as_str() == Some(&k))?;
            return Some((crate::data::wx_icon(d["code"].as_i64().unwrap_or(0), 1).into(), d["max"].as_f64().unwrap_or(0.0).round() as i64, d["pop"].as_i64().unwrap_or(0) >= 50));
        }
        let h = wx["hourly"].as_array()?.iter().find(|h| h["t"].as_i64().unwrap_or(0) <= start && h["t"].as_i64().unwrap_or(0) + 3600 > start)?;
        Some((crate::data::wx_icon(h["code"].as_i64().unwrap_or(0), h["day"].as_i64().unwrap_or(1)).into(), h["temp"].as_f64().unwrap_or(0.0).round() as i64, h["pop"].as_i64().unwrap_or(0) >= 50))
    }
    fn wx_chip(c: &mut Canvas, t: &mut Text, right: f32, cy: f32, w: &(String, i64, bool)) -> f32 {
        let s = format!("{}°", w.1);
        let ss = st(11.5, 600.0).tnum();
        let tw = t.width(&s, &ss);
        text_cy(c, t, right - tw, cy, &s, &ss, M_FG2, None);
        c.icon(t, right - tw - 4.0 - 6.5, cy, if w.2 { "umbrella" } else { &w.0 }, 13.0, if w.2 { M_BLUE } else { M_FG2 });
        tw + 4.0 + 13.0
    }
    fn card(c: &mut Canvas, y: f32, w: f32, h: f32, _fill: f32) {
        Self::lens(c, 0.0, y, w, h, 22.0, 0.06);
    }
    // the agenda card; returns its height
    // the picked day's events and the next month's (holidays included: the week strip dots them too, "nothing"
    // under a dot would be a lie)
    fn cal_lists(&self, cal: &Value) -> (Vec<Value>, Vec<Value>) {
        let td = today();
        let picked = day(td.y, td.m, td.d + self.pick as i32);
        let now = crate::data::now();
        let day_events: Vec<Value> = events_on(cal, picked).into_iter().filter(|e| self.pick > 0 || e["end"].as_i64().unwrap_or(0) > now).collect();
        let soon: Vec<Value> = cal["items"].as_array().map(|a| a.iter().filter(|x| x["end"].as_i64().unwrap_or(0) > now && x["start"].as_i64().unwrap_or(0) < now + 30 * 86400).take(6).cloned().collect()).unwrap_or_default();
        (day_events, soon)
    }
    // the agenda card's height when nothing is cut (paint_cal's layout, row by row): the card is as tall as what it shows
    fn cal_need(&self, cx: &Ctx) -> f32 {
        let td = today();
        let picked = day(td.y, td.m, td.d + self.pick as i32);
        let (day_events, soon) = self.cal_lists(cx.cal);
        let mut y = 38.0 + 64.0;
        let key = format!("{}-{:02}-{:02}", picked.y, picked.m + 1, picked.d);
        if cx.wx["days"].as_array().is_some_and(|a| a.iter().any(|d| d["date"].as_str() == Some(&key))) {
            y += 38.0;
        }
        if day_events_all(cx.cal, picked).iter().any(|e| !e["allDay"].as_bool().unwrap_or(false)) {
            y += 36.0;
        }
        y += if !day_events.is_empty() {
            day_events.len() as f32 * 46.0
        } else if soon.is_empty() {
            88.0
        } else {
            24.0 + soon.len() as f32 * 44.0
        };
        y + 30.0
    }
    fn paint_cal(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, avail: f32) -> f32 {
        let cal = cx.cal;
        let td = today();
        let day_at = |i: usize| day(td.y, td.m, td.d + i as i32);
        let picked: Day = day_at(self.pick);
        let now = crate::data::now();
        let (day_events, soon) = self.cal_lists(cal);
        let iw = w - 36.0;
        let mut l = Canvas::new(w as u32, avail.max(120.0) as u32);
        let t = &mut *cx.t;
        let (ox, oy) = (18.0, 16.0);
        let saved = self.origin;
        self.origin = (saved.0 + ox, saved.1 + oy);
        // header
        let head_h = Hit::CalHead;
        let label = match self.pick { 0 => "Dziś".to_string(), 1 => "Jutro".into(), _ => DAYS_LONG[picked.wd as usize].into() };
        caps(&mut l, t, ox, oy + 6.0, &label, if self.hov(&head_h) { M_FG } else { M_FAINT });
        let count = if !day_events.is_empty() {
            format!("{}{}", plural(day_events.len(), "wydarzenie", "wydarzenia", "wydarzeń"), if self.pick == 0 { " przed tobą" } else { "" })
        } else {
            format!("{}{}", if self.pick == 0 { "wolny dzień" } else { "brak wydarzeń" }, soon.first().map(|e| format!(" · następne {}", crate::data::rel(e["start"].as_i64().unwrap_or(0)))).unwrap_or_default())
        };
        let cs = st(11.0, 550.0);
        let cw = t.width(&count, &cs).min(iw - 26.0 - 8.0 - 60.0);
        text_cy(&mut l, t, ox + iw - 26.0 - 8.0 - cw, oy + 13.0, &count, &cs, M_MUTED, Some(iw - 26.0 - 8.0 - 60.0));
        self.push_hit((0.0, 0.0, iw - 26.0 - 8.0 - cw - 8.0, 26.0), head_h);
        let nh = Hit::CalNew;
        rect(&mut l, ox + iw - 26.0, oy, 26.0, 26.0, 13.0, if self.hov(&nh) { FILL2 } else { Rgba(1.0, 1.0, 1.0, 0.05) });
        l.icon(t, ox + iw - 13.0, oy + 13.0, "plus", 13.0, M_FG);
        self.push_hit((iw - 26.0, 0.0, 26.0, 26.0), nh);
        let mut y = 26.0 + 12.0;
        // week strip
        let dw = iw / 7.0;
        for i in 0..7 {
            let d = day_at(i);
            let evs = events_on(cal, d);
            let on = self.pick == i;
            let x = i as f32 * dw;
            let h = Hit::Day(i);
            if (on && i > 0) || (!on && self.hov(&h)) {
                rect(&mut l, ox + x + 2.0, oy + y + 2.0, dw - 4.0, 50.0, 14.0, if on { FILL2 } else { FILL });
            }
            if i == 0 {
                circle(&mut l, ox + x + dw / 2.0, oy + y + 27.5, 12.5, accent());
            }
            let ds = st(9.0, 700.0).spaced(0.5);
            let dn = DAYS[d.wd as usize].to_uppercase();
            let ww = t.width(&dn, &ds);
            text(&mut l, t, ox + x + (dw - ww) / 2.0, oy + y + 6.0, &dn, &ds, if on { M_FG2 } else { M_FAINT }, false, None);
            let ns = st(14.0, if i == 0 || on { 650.0 } else { 500.0 });
            let n = d.d.to_string();
            let nw = t.width(&n, &ns);
            text_cy(&mut l, t, ox + x + (dw - nw) / 2.0, oy + y + 27.5, &n, &ns, if i == 0 { WHITE } else if d.wd == 0 { M_BAD } else if on { M_FG } else { M_FG2 }, None);
            let k = evs.len().min(3);
            let dx = ox + x + dw / 2.0 - (k as f32 * 4.0 + (k.max(1) - 1) as f32 * 3.0) / 2.0;
            for (j, e) in evs.iter().take(3).enumerate() {
                circle(&mut l, dx + j as f32 * 7.0 + 2.0, oy + y + 46.0, 2.0, feed_color(e));
            }
            self.push_hit((x, y, dw, 54.0), h);
        }
        y += 54.0 + 10.0;
        // the picked day's weather: icon, high / low, rain, daylight
        let key = format!("{}-{:02}-{:02}", picked.y, picked.m + 1, picked.d);
        if let Some(wd) = cx.wx["days"].as_array().and_then(|a| a.iter().find(|d| d["date"].as_str() == Some(&key))) {
            let cyw = oy + y + 13.0;
            rect(&mut l, ox, oy + y, iw, 26.0, 13.0, Rgba(1.0, 1.0, 1.0, 0.04));
            l.icon(t, ox + 16.0, cyw, crate::data::wx_icon(wd["code"].as_i64().unwrap_or(0), 1), 14.0, M_FG2);
            let hi = format!("{}°", wd["max"].as_f64().unwrap_or(0.0).round() as i64);
            let lo = format!(" / {}°", wd["min"].as_f64().unwrap_or(0.0).round() as i64);
            let mut x = ox + 30.0;
            x += text_cy(&mut l, t, x, cyw, &hi, &st(12.0, 650.0).tnum(), M_FG, None);
            x += text_cy(&mut l, t, x, cyw, &lo, &st(12.0, 450.0).tnum(), M_MUTED, None) + 10.0;
            let pop = wd["pop"].as_i64().unwrap_or(0);
            if pop >= 30 {
                l.icon(t, x + 6.0, cyw, "umbrella", 12.0, M_BLUE);
                text_cy(&mut l, t, x + 15.0, cyw, &format!("{pop}%"), &st(11.0, 600.0).tnum(), M_BLUE, None);
            }
            let (rise, set) = (wd["rise"].as_str().unwrap_or(""), wd["set"].as_str().unwrap_or(""));
            if !rise.is_empty() {
                let ss = st(11.0, 500.0).tnum();
                let sw = t.width(set, &ss);
                let rw = t.width(rise, &ss);
                let rx = ox + iw - 10.0 - sw;
                text_cy(&mut l, t, rx, cyw, set, &ss, M_MUTED, None);
                l.icon(t, rx - 9.0, cyw, "moon", 12.0, M_BLUE.mul_a(0.85));
                text_cy(&mut l, t, rx - 18.0 - 8.0 - rw, cyw, rise, &ss, M_MUTED, None);
                l.icon(t, rx - 18.0 - 8.0 - rw - 9.0, cyw, "sunrise", 12.0, YELLOW.mul_a(0.85));
            }
            y += 26.0 + 12.0;
        }
        // the picked day at a glance, 7–23 h
        let timed: Vec<&Value> = day_events_all(cal, picked).into_iter().filter(|e| !e["allDay"].as_bool().unwrap_or(false)).collect::<Vec<_>>().leak().iter().collect();
        if !timed.is_empty() {
            let t0 = picked.ts + 7 * 3600;
            let xo = |ts: i64| ((ts - t0) as f32 / (16.0 * 3600.0)).clamp(0.0, 1.0) * iw;
            rect(&mut l, ox, oy + y + 3.0, iw, 6.0, 3.0, M_TRACK);
            for e in &timed {
                let (s, en) = (e["start"].as_i64().unwrap_or(0), e["end"].as_i64().unwrap_or(0));
                let x = xo(s);
                let clash = timed.iter().any(|o| !std::ptr::eq(*o, *e) && o["start"].as_i64().unwrap_or(0) < en && o["end"].as_i64().unwrap_or(0) > s);
                rect(&mut l, ox + x, oy + y + 3.0, (xo(en) - x).max(6.0), 6.0, 3.0, if clash { M_WARN } else { feed_color(e) });
            }
            if self.pick == 0 {
                rect(&mut l, ox + xo(now) - 1.0, oy + y, 2.0, 12.0, 1.0, M_FG);
            }
            for hh in [7, 11, 15, 19, 23] {
                let s = hh.to_string();
                let ss = st(9.5, 550.0).tnum();
                let sw = t.width(&s, &ss);
                let x = ((hh - 7) as f32 / 16.0 * iw - sw / 2.0).clamp(0.0, iw - sw);
                text(&mut l, t, ox + x, oy + y + 12.0, &s, &ss, M_FAINT, false, None);
            }
            y += 24.0 + 12.0;
        }
        // the picked day's events: as many as the card's height takes
        let room = avail - 30.0 - y;
        let n = if (day_events.len() as f32) * 46.0 <= room { day_events.len() } else { ((room - 18.0) / 46.0).floor().max(1.0) as usize };
        for (i, e) in day_events.iter().take(n).enumerate() {
            let (s, en) = (e["start"].as_i64().unwrap_or(0), e["end"].as_i64().unwrap_or(0));
            let live = s <= now && en > now;
            let first = self.pick == 0 && i == 0;
            let all_day = e["allDay"].as_bool().unwrap_or(false);
            if live || first {
                rect(&mut l, ox, oy + y, iw, 42.0, 12.0, if live { accent().mul_a(0.16) } else { FILL });
            }
            let other = if all_day { None } else { events_on(cal, picked).into_iter().find(|o| o != e && !o["allDay"].as_bool().unwrap_or(false) && o["start"].as_i64().unwrap_or(0) < en && o["end"].as_i64().unwrap_or(0) > s) };
            rect(&mut l, ox + 8.0, oy + y + 8.0, 3.0, 26.0, 1.5, if other.is_some() { M_WARN } else if live { accent() } else { feed_color(e) });
            let (a, b) = if all_day { ("cały".to_string(), "dzień".to_string()) } else { (crate::data::hm(s), crate::data::hm(en)) };
            text(&mut l, t, ox + 20.0, oy + y + 6.0, &a, &st(12.0, 600.0).tnum(), M_FG, false, None);
            text(&mut l, t, ox + 20.0, oy + y + 22.0, &b, &st(10.0, 400.0).tnum(), M_FAINT, false, None);
            let wxw = Self::wx_at(cx.wx, e).map(|w| Self::wx_chip(&mut l, t, ox + iw - 8.0, oy + y + 21.0, &w) + 8.0).unwrap_or(0.0);
            let tw = iw - 74.0 - wxw;
            text(&mut l, t, ox + 66.0, oy + y + 6.0, e["title"].as_str().unwrap_or(""), &st(12.5, 600.0), M_FG, false, Some(tw));
            let dur = |sec: i64| {
                let m = (sec as f64 / 60.0).round().max(1.0) as i64;
                if m < 60 { format!("{m} min") } else if m / 60 < 24 { format!("{} h{}", m / 60, if m % 60 > 0 { format!(" {} min", m % 60) } else { String::new() }) } else { crate::data::rel(now + sec) }
            };
            let wh = e["where"].as_str().unwrap_or("");
            let (sub, col) = if let Some(o) = &other {
                (format!("koliduje z: {}", o["title"].as_str().unwrap_or("")), M_WARN)
            } else if live {
                (format!("trwa · zostało {}", dur(en - now)), accent())
            } else if first && !all_day {
                (format!("za {}{}", dur(s - now), if wh.is_empty() { String::new() } else { format!(" · {wh}") }), accent())
            } else if !wh.is_empty() {
                (wh.to_string(), M_MUTED)
            } else if e["holiday"].as_bool().unwrap_or(false) {
                ("święto".into(), M_MUTED)
            } else {
                (crate::data::when(e), M_MUTED)
            };
            text(&mut l, t, ox + 66.0, oy + y + 22.0, &sub, &st(10.5, 400.0), col, false, Some(tw));
            if live {
                rect(&mut l, ox + 66.0, oy + y + 36.0, iw - 80.0, 2.0, 1.0, FILL2);
                rect(&mut l, ox + 66.0, oy + y + 36.0, (iw - 80.0) * ((now - s) as f32 / (en - s).max(1) as f32).min(1.0), 2.0, 1.0, accent());
            }
            y += 46.0;
        }
        if day_events.is_empty() {
            if soon.is_empty() {
                // the empty state in the middle of the card's free space
                // and how to add one: the search field takes "jutro 15 dentysta"
                let my = y + ((avail - 32.0 - y - 84.0) / 2.0).max(0.0);
                l.icon(t, ox + iw / 2.0, oy + my + 14.0, "calendar", 22.0, M_FAINT);
                let s = "Nic w kalendarzu na najbliższy miesiąc";
                let sw = t.width(s, &st(12.0, 500.0)).min(iw);
                text(&mut l, t, ox + (iw - sw) / 2.0, oy + my + 34.0, s, &st(12.0, 500.0), M_FG2, false, Some(iw));
                let hs = st(11.0, 450.0);
                for (k, line) in crate::gfx::wrap(t, "Wpisz w wyszukiwarce np. „jutro 15 dentysta”, aby dodać", &hs, iw - 20.0, 2).iter().enumerate() {
                    let lw = t.width(line, &hs);
                    text(&mut l, t, ox + (iw - lw) / 2.0, oy + my + 54.0 + k as f32 * 15.0, line, &hs, M_FAINT, false, None);
                }
                y = my + 88.0;
            } else {
                text(&mut l, t, ox + 8.0, oy + y + 2.0, "NAJBLIŻSZE", &st(11.0, 600.0).spaced(1.2), M_FAINT, false, Some(iw - 16.0));
                y += 20.0 + 4.0;
            }
            let fit = ((avail - 30.0 - y) / 44.0).floor().max(1.0) as usize;
            for e in soon.iter().take(fit) {
                let s = e["start"].as_i64().unwrap_or(0);
                let d = crate::data::local(s);
                let off = ((day(d.tm_year + 1900, d.tm_mon, d.tm_mday).ts - td.ts) as f64 / 86400.0).round() as i64;
                let h = Hit::Soon(off);
                if self.hov(&h) {
                    rect(&mut l, ox, oy + y, iw, 40.0, 12.0, Rgba(1.0, 1.0, 1.0, 0.05));
                }
                rect(&mut l, ox + 8.0, oy + y + 8.0, 3.0, 24.0, 1.5, feed_color(e));
                text(&mut l, t, ox + 20.0, oy + y + 6.0, &DAYS[d.tm_wday as usize].to_uppercase(), &st(9.5, 700.0), M_MUTED, false, None);
                text(&mut l, t, ox + 20.0, oy + y + 19.0, &format!("{}.{:02}", d.tm_mday, d.tm_mon + 1), &st(11.5, 600.0).tnum(), M_FG, false, None);
                let wxw = Self::wx_at(cx.wx, e).map(|w| Self::wx_chip(&mut l, t, ox + iw - 8.0, oy + y + 20.0, &w) + 8.0).unwrap_or(0.0);
                let tw = iw - 74.0 - wxw;
                text(&mut l, t, ox + 66.0, oy + y + 5.0, e["title"].as_str().unwrap_or(""), &st(12.5, 600.0), M_FG, false, Some(tw));
                let what = if e["holiday"].as_bool().unwrap_or(false) { "święto".to_string() } else { crate::data::when(e) };
                text(&mut l, t, ox + 66.0, oy + y + 21.0, &format!("{} · {}", crate::data::rel(s), what), &st(10.5, 400.0), M_MUTED, false, Some(tw));
                self.push_hit((0.0, y, iw, 40.0), h);
                y += 44.0;
            }
        }
        if day_events.len() > n {
            text(&mut l, t, ox + 8.0, oy + y, &format!("+{} więcej", day_events.len() - n), &st(10.5, 400.0), M_FAINT, false, None);
            y += 18.0;
        }
        self.origin = saved;
        let h = (y - 4.0 + 32.0).max(avail);
        Self::card(c, 0.0, w, h, 0.03);
        c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        h
    }
    // the phone card's height (paint_phone's layout)
    fn phone_h(&self, max_notes: usize) -> f32 {
        let (main, notes) = {
            let s = self.ph.st.lock().unwrap();
            (s.main(), s.notes())
        };
        let paired = main["pair"].as_i64() == Some(3);
        let online = paired && main["reach"].as_bool().unwrap_or(false);
        let shown = notes.iter().filter(|n| !promo(n)).take(max_notes).count();
        let mut h = 14.0 + 30.0;
        if main.is_object() && !paired {
            h += 10.0 + 30.0;
        }
        if online {
            h += 10.0 + 50.0;
            if self.phone_more {
                h += 10.0 + 50.0;
            }
        }
        if shown > 0 {
            h += 10.0 + shown as f32 * 50.0 - 4.0;
        }
        h + 14.0
    }
    // the phone card at y; returns its height
    fn paint_phone(&mut self, c: &mut Canvas, cx: &mut Ctx, y0: f32, w: f32, max_notes: usize) -> f32 {
        let (main, ok, notes) = {
            let s = self.ph.st.lock().unwrap();
            (s.main(), s.get("ok").as_bool().unwrap_or(false), s.notes())
        };
        let present = main.is_object();
        let paired = main["pair"].as_i64() == Some(3);
        let online = paired && main["reach"].as_bool().unwrap_or(false);
        let pair = main["pair"].as_i64().unwrap_or(0);
        let bat = if online { main["bat"].as_i64().unwrap_or(-1) } else { -1 };
        let chg = online && main["chg"].as_bool().unwrap_or(false);
        let sig = if online { main["sig"].as_i64().unwrap_or(-1) } else { -1 };
        let plugins: Vec<String> = main["plugins"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let has = |p: &str| online && plugins.iter().any(|x| x == p);
        let shown: Vec<Value> = notes.iter().filter(|n| !promo(n)).take(max_notes).cloned().collect();
        let bcol = if chg { GREEN } else if (0..=15).contains(&bat) { DANGER } else if (0..=30).contains(&bat) { YELLOW } else { T1 };
        let (x0, iw) = (14.0, w - 28.0);
        // height first
        let mut h = 14.0 + 30.0;
        let pair_row = present && !paired;
        if pair_row {
            h += 10.0 + 30.0;
        }
        if online {
            h += 10.0 + 50.0;
            if self.phone_more {
                h += 10.0 + 50.0;
            }
        }
        if !shown.is_empty() {
            h += 10.0 + shown.len() as f32 * 50.0 - 4.0;
        }
        h += 14.0;
        Self::card(c, y0, w, h, 0.045);
        let t = &mut *cx.t;
        let mut y = y0 + 14.0;
        // phone glyph filled to the battery level
        let (gx, gy) = (x0 + 5.0, y);
        border(c, gx, gy, 20.0, 30.0, 6.0, 1.5, Rgba(1.0, 1.0, 1.0, if online { 0.55 } else { 0.22 }));
        if bat >= 0 {
            let fh = (24.0 * bat as f32 / 100.0).max(2.0);
            rect(c, gx + 3.0, gy + 27.0 - fh, 14.0, fh, 3.0, bcol.mul_a(0.85));
        }
        if chg {
            c.icon(t, gx + 10.0, gy + 15.0, "bolt", 11.0, INK);
        } else if !online {
            c.icon(t, gx + 10.0, gy + 15.0, "x", 11.0, M_MUTED);
        }
        // name + status
        let name = if present { main["name"].as_str().unwrap_or("Telefon").to_string() } else { "Telefon".into() };
        let right_w = if online { 40.0 } else { 0.0 } + 28.0 + 12.0;
        let tx = x0 + 42.0;
        let tw = iw - 42.0 - right_w;
        let nn = if online { main["nNotes"].as_i64().unwrap_or(0) } else { 0 };
        let ns = st(13.5, 600.0);
        let nbw = if nn > 0 { (t.width(&nn.to_string(), &st(10.0, 700.0)) + 10.0).max(18.0) + 7.0 } else { 0.0 };
        let nw = text(c, t, tx, y - 1.0, &name, &ns, M_FG, false, Some(tw - nbw));
        if nn > 0 {
            let bw = nbw - 7.0;
            rect(c, tx + nw + 7.0, y + 1.0, bw, 18.0, 9.0, M_ACC_SOFT);
            border(c, tx + nw + 7.0, y + 1.0, bw, 18.0, 9.0, 1.0, accent().mul_a(0.3));
            let s = nn.to_string();
            let sw = t.width(&s, &st(10.0, 700.0));
            text_cy(c, t, tx + nw + 7.0 + (bw - sw) / 2.0, y + 10.0, &s, &st(10.0, 700.0), accent(), None);
        }
        let key = main["key"].as_str().unwrap_or("");
        let status = if !ok {
            "KDE Connect nie działa".to_string()
        } else if !present {
            "Brak urządzeń · otwórz KDE Connect na telefonie".into()
        } else if pair == 2 {
            format!("Chce się sparować · klucz {key}")
        } else if pair == 1 {
            format!("Potwierdź na telefonie · klucz {key}")
        } else if !paired {
            if main["reach"].as_bool().unwrap_or(false) { "W zasięgu · niesparowany".into() } else { "Niesparowany".into() }
        } else if !online {
            "Poza zasięgiem".into()
        } else {
            let mut p = Vec::new();
            if chg {
                p.push("ładuje".to_string());
            }
            if let Some(n) = main["net"].as_str().filter(|s| !s.is_empty()) {
                p.push(n.into());
            }
            if main["mount"].as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                p.push("pliki podłączone".into());
            }
            if p.is_empty() { "Połączono".into() } else { p.join(" · ") }
        };
        let said = self.phone_said.1.filter(|a| a.elapsed().as_millis() < 2400).map(|_| self.phone_said.0.clone());
        let (line, col) = match (&said, self.phone_hint.is_empty()) {
            (Some(s), _) => (s.clone(), accent()),
            (None, false) => (self.phone_hint.clone(), M_FG2),
            _ => (status, if pair == 1 || pair == 2 { M_WARN } else { M_MUTED }),
        };
        text(c, t, tx, y + 17.0, &line, &st(11.0, 400.0), col, false, Some(tw));
        let mut rx = x0 + iw - 28.0;
        let eh = Hit::PhoneExpand;
        rect(c, rx, y + 1.0, 28.0, 28.0, 14.0, if self.hov(&eh) { FILL2 } else { CLEAR });
        c.icon(t, rx + 14.0, y + 15.0, "layoutGrid", 14.0, M_FG);
        self.push_hit((rx, y + 1.0, 28.0, 28.0), eh);
        if online {
            rx -= 12.0;
            if bat >= 0 {
                let s = format!("{bat}%");
                let sw = t.width(&s, &st(13.0, 600.0).tnum());
                text(c, t, rx - sw, y - 2.0, &s, &st(13.0, 600.0).tnum(), bcol, false, None);
            }
            if sig >= 0 {
                let icons = ["barsOff", "bars1", "bars2", "bars3", "bars4", "bars5"];
                c.icon(t, rx - 6.5, y + 23.0, icons[(sig + 1).clamp(0, 5) as usize], 13.0, if sig <= 0 { M_BAD } else { M_MUTED });
            }
        }
        y += 30.0;
        if pair_row {
            y += 10.0;
            let mut bx = x0;
            let mut tb = |me: &mut Start, c: &mut Canvas, t: &mut Text, label: &str, primary: bool, h: Hit| {
                let ls = st(12.0, 600.0);
                let bw = t.width(label, &ls) + 24.0;
                let hov = me.hov(&h);
                rect(c, bx, y, bw, 30.0, 15.0, if primary { if hov { M_ACC_STRONG } else { accent() } } else if hov { FILL2 } else { FILL });
                let lw = t.width(label, &ls);
                text_cy(c, t, bx + (bw - lw) / 2.0, y + 15.0, label, &ls, if primary { WHITE } else { M_FG }, None);
                me.push_hit((bx, y, bw, 30.0), h);
                bx += bw + 8.0;
            };
            if pair == 2 {
                tb(self, c, t, "Akceptuj", true, Hit::PhoneAccept);
            }
            if pair == 2 || pair == 1 {
                tb(self, c, t, if pair == 1 { "Anuluj" } else { "Odrzuć" }, false, Hit::PhoneReject);
            } else if main["reach"].as_bool().unwrap_or(false) {
                tb(self, c, t, "Sparuj", true, Hit::PhonePair);
            }
            y += 30.0;
        }
        if online {
            let adb_ok = { let s = self.ph.st.lock().unwrap(); s.adb_ok() };
            let mut rows: Vec<Vec<(&'static str, &'static str, &'static str)>> = vec![vec![]];
            for (icon, tip, c_, p) in [("bellRing", "Znajdź", "ring", "findmyphone"), ("clipboard", "Schowek", "clip", "clipboard"), ("fileUp", "Wyślij pliki", "pick", "share")] {
                if has(p) {
                    rows[0].push((icon, tip, c_));
                }
            }
            if self.phone_more {
                let mut r = Vec::new();
                for (icon, tip, c_, p) in [("folderOpen", "Pliki", "browse", "sftp"), ("camera", "Zdjęcie", "photo", "photo"), ("message", "SMS", "expand", "sms")] {
                    if has(p) {
                        r.push((icon, tip, c_));
                    }
                }
                if adb_ok {
                    r.push(("screenOn", "Ekran na PC", "mirror"));
                }
                rows.push(r);
            }
            for (ri, row) in rows.iter().enumerate() {
                y += 10.0;
                let n = row.len() + if ri == 0 { 1 } else { 0 };
                let bw = (iw - (n.max(1) - 1) as f32 * 6.0) / n.max(1) as f32;
                let mut items: Vec<(&str, &str, Hit, bool)> = row.iter().map(|(i, tip, c_)| (*i, *tip, Hit::PhoneAct(c_), false)).collect();
                if ri == 0 {
                    items.push((if self.phone_more { "x" } else { "dots" }, if self.phone_more { "Mniej" } else { "Więcej" }, Hit::PhoneMore, self.phone_more));
                }
                // icon over its name: what each one does without hovering it first
                for (k, (icon, label, h, active)) in items.into_iter().enumerate() {
                    let bx = x0 + k as f32 * (bw + 6.0);
                    let hov = self.hov(&h);
                    rect(c, bx, y, bw, 50.0, 16.0, if active { accent() } else if hov { FILL2 } else { FILL });
                    c.icon(t, bx + bw / 2.0, y + 18.0, icon, 16.0, if active { WHITE } else { M_FG });
                    let ls = st(10.0, 550.0);
                    let lw = t.width(label, &ls).min(bw - 8.0);
                    text_cy(c, t, bx + (bw - lw) / 2.0, y + 37.0, label, &ls, if active { WHITE } else if hov { M_FG } else { M_MUTED }, Some(bw - 8.0));
                    self.push_hit((bx, y, bw, 50.0), h);
                }
                y += 50.0;
            }
        }
        if !shown.is_empty() {
            y += 10.0;
            for n in &shown {
                let id = n["id"].as_str().unwrap_or("").to_string();
                let hov = inside(self.ptr, (self.origin.0 + x0, self.origin.1 + y, iw, 46.0));
                rect(c, x0, y, iw, 46.0, 14.0, if hov { FILL2 } else { FILL });
                rect(c, x0 + 9.0, y + 9.0, 28.0, 28.0, 9.0, FILL2);
                let icon = n["icon"].as_str().unwrap_or("").to_string();
                let app = n["app"].as_str().unwrap_or("").to_string();
                let pm = (!icon.is_empty()).then(|| cx.imgs.get(&icon, 56, cx.icons)).flatten();
                // no icon from the phone: the same app's icon on this computer (Discord, Messenger…), else its initial
                let local = if pm.is_none() && !app.is_empty() {
                    let a = norm(&app);
                    cx.apps.entries.iter().find(|e| !e.icon.is_empty() && (norm(&e.name) == a || norm(&e.id).trim_end_matches(".desktop").rsplit('.').next() == Some(a.as_str()))).map(|e| e.icon.clone())
                } else {
                    None
                };
                if let Some(li) = &local {
                    Self::app_icon(c, cx, li, x0 + 9.0, y + 9.0, 28.0);
                }
                let t = &mut *cx.t;
                match pm {
                    Some(pm) => crate::img::draw(c, &pm, x0 + 9.0, y + 9.0, 28.0, 28.0, 9.0, true, 1.0),
                    None if local.is_some() => {}
                    None => {
                        let l = app.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or("?".into());
                        let lw = t.width(&l, &st(12.0, 700.0));
                        text_cy(c, t, x0 + 23.0 - lw / 2.0, y + 23.0, &l, &st(12.0, 700.0), M_FG, None);
                    }
                }
                let cw = iw - 46.0 - if hov { 34.0 } else { 10.0 };
                let body = n["text"].as_str().unwrap_or("");
                let ty = if body.is_empty() { y + 15.0 } else { y + 7.0 };
                let aw = text(c, t, x0 + 46.0, ty, &app.to_uppercase(), &st(10.0, 700.0).spaced(0.5), M_MUTED, false, Some(cw * 0.45));
                text(c, t, x0 + 46.0 + aw + 5.0, ty - 1.0, n["title"].as_str().unwrap_or(""), &st(11.5, 600.0), M_FG, false, Some(cw - aw - 5.0));
                if !body.is_empty() {
                    text(c, t, x0 + 46.0, y + 24.0, &body.replace('\n', " "), &st(11.0, 400.0), M_FG2, false, Some(cw));
                }
                self.push_hit((x0, y, iw, 46.0), Hit::Note);
                if hov {
                    let xh = Hit::NoteX(id);
                    let (bx, by) = (x0 + iw - 8.0 - 24.0, y + 11.0);
                    rect(c, bx, by, 24.0, 24.0, 12.0, if self.hov(&xh) { FILL2 } else { CLEAR });
                    c.icon(t, bx + 12.0, by + 12.0, "x", 12.0, M_FG);
                    self.push_hit((bx, by, 24.0, 24.0), xh);
                }
                y += 50.0;
            }
        }
        h
    }
    // now playing at y (146 high)
    fn paint_np(&mut self, c: &mut Canvas, cx: &mut Ctx, y: f32, w: f32, p: &crate::media::Player) {
        // the cover's colour, once per cover
        let art = cx.imgs.get(&p.art, 160, cx.icons).filter(|_| !p.art.is_empty());
        if self.amb_key != p.art {
            self.amb_key = p.art.clone();
            self.amb = art.as_ref().map(|pm| ambient(pm)).unwrap_or(accent());
        }
        let amb = self.amb;
        let hov = inside(self.ptr, (self.origin.0, self.origin.1 + y, w, 146.0));
        crate::glass::shadow(c, 0.0, y + 10.0, w, 146.0, 24.0, 34.0, -10.0, amb.mul_a(if p.playing { 0.22 } else { 0.08 }));
        rect(c, 0.0, y, w, 146.0, 22.0, Rgba(8.0 / 255.0, 10.0 / 255.0, 16.0 / 255.0, 0.5));
        // the cover as a dim, soft backdrop (scaled up, low opacity: no blur pass)
        if let Some(pm) = &art {
            let mut l = Canvas::new(w as u32, 146);
            crate::img::draw(&mut l, pm, -w * 0.2, (146.0 - w * 1.4) / 2.0, w * 1.4, w * 1.4, 0.0, true, 0.22);
            if let Some(path) = crate::gfx::rrect_path(0.0, y, w, 146.0, 22.0) {
                let mut mask = tiny_skia::Mask::new(c.pm.width(), c.pm.height()).unwrap();
                mask.fill_path(&path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
                c.pm.draw_pixmap(0, y as i32, l.pm.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, tiny_skia::Transform::identity(), Some(&mask));
            }
        }
        if let Some(path) = crate::gfx::rrect_path(0.0, y, w, 146.0, 22.0) {
            fill_lin(c, &path, (0.0, y), (0.0, y + 146.0), &[(0.0, Rgba(6.0 / 255.0, 9.0 / 255.0, 16.0 / 255.0, 0.12)), (1.0, Rgba(6.0 / 255.0, 9.0 / 255.0, 16.0 / 255.0, 0.62))]);
            fill_lin(c, &path, (0.0, y), (w, y), &[(0.0, amb.mul_a(0.16)), (0.6, amb.mul_a(0.0))]);
        }
        crate::glass::Glass { radius: 22.0, rim_only: true, shine: 0.6, ..Default::default() }.paint(c, 0.0, y, w, 146.0);
        if hov {
            border(c, 0.0, y, w, 146.0, 22.0, 1.0, amb.mul_a(0.3));
        }
        // art
        let ah = Hit::Art;
        let s = if self.hov(&ah) { 62.0 } else { 60.0 };
        let (ax, ay) = (14.0 + (60.0 - s) / 2.0, y + 14.0 + (60.0 - s) / 2.0);
        crate::glass::shadow(c, ax, ay + 6.0, s, s, 14.0, 18.0, -2.0, Rgba(amb.0 * 0.5, amb.1 * 0.5, amb.2 * 0.5, 0.55));
        rect(c, ax, ay, s, s, 14.0, FILL2);
        let t = &mut *cx.t;
        match &art {
            Some(pm) => crate::img::draw(c, pm, ax, ay, s, s, 14.0, true, 1.0),
            None => c.icon(t, ax + s / 2.0, ay + s / 2.0, "music", 24.0, amb),
        }
        border(c, ax, ay, s, s, 14.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.14));
        self.push_hit((14.0, y + 14.0, 60.0, 60.0), ah);
        // source, title, artist
        let tx = 14.0 + 60.0 + 14.0;
        let tw = w - tx - 16.0;
        let el = self.enter.value(); // (unused: the equalizer is static, no per-frame work)
        let _ = el;
        for i in 0..3 {
            let bh = if p.playing { [8.0, 10.0, 6.0][i] } else { 4.0 };
            rect(c, tx + i as f32 * 4.5, y + 26.0 - bh, 2.5, bh, 1.25, amb);
        }
        // the source; with more than one player it switches (its count after it)
        let n_src = Self::np_players(cx.players).len();
        let src = if n_src > 1 { format!("{} · {}/{}", p.identity.to_uppercase(), Self::np_players(cx.players).iter().position(|q| q.bus == p.bus).unwrap_or(0) + 1, n_src) } else { p.identity.to_uppercase() };
        let t = &mut *cx.t;
        let sw = text(c, t, tx + 18.0, y + 16.0, &src, &st(9.5, 700.0).spaced(1.2), if n_src > 1 && self.hov(&Hit::Src) { M_FG } else { amb }, false, Some(tw - 18.0));
        if n_src > 1 {
            c.icon(t, tx + 18.0 + sw + 9.0, y + 22.0, "chevron", 10.0, amb);
            self.push_hit((tx - 2.0, y + 10.0, (sw + 34.0).min(tw + 4.0), 20.0), Hit::Src);
        }
        text(c, t, tx, y + 30.0, &p.title, &st(16.0, 650.0), M_FG, false, Some(tw));
        text(c, t, tx, y + 52.0, &p.artist, &st(12.5, 400.0), M_FG2, false, Some(tw));
        // scrubber
        // a live stream reports a length of ~i64::MAX µs: anything over 100 h is no length
        let can_len = p.length > 0.0 && p.length < 360_000.0;
        let pos = (p.position + if p.playing { p.pos_at.map(|a| a.elapsed().as_secs_f64()).unwrap_or(0.0) } else { 0.0 }).min(p.length.max(0.0));
        let f = if can_len { (pos / p.length) as f32 } else { 0.0 };
        // the times at the bar's ends, the transport row below free for shuffle / repeat
        let ts_ = st(10.5, 500.0).tnum();
        let (lt, rt) = if can_len { (mmss(pos), format!("-{}", mmss(p.length - pos))) } else { (String::new(), String::new()) };
        let lw = if can_len { t.width("00:00", &ts_).max(t.width(&lt, &ts_)) + 10.0 } else { 0.0 };
        let rw = if can_len { t.width("-00:00", &ts_).max(t.width(&rt, &ts_)) + 10.0 } else { 0.0 };
        if can_len {
            text_cy(c, t, 16.0, y + 91.0, &lt, &ts_, M_FG2, None);
            let w2 = t.width(&rt, &ts_);
            text_cy(c, t, w - 16.0 - w2, y + 91.0, &rt, &ts_, M_MUTED, None);
        }
        let (bx, by, bw) = (16.0 + lw, y + 84.0, w - 32.0 - lw - rw);
        let sk = Hit::Seek;
        let sh = self.hov(&sk);
        let th = if sh { 6.0 } else { 4.0 };
        rect(c, bx, by + 7.0 - th / 2.0, bw, th, th / 2.0, Rgba(1.0, 1.0, 1.0, 0.14));
        if let Some(path) = crate::gfx::rrect_path(bx, by + 7.0 - th / 2.0, (bw * f).max(th), th, th / 2.0) {
            fill_lin(c, &path, (bx, by), (bx + (bw * f).max(th), by), &[(0.0, WHITE.mul_a(0.7)), (1.0, WHITE.mul_a(0.95))]);
        }
        if can_len && sh {
            circle(c, bx + bw * f, by + 7.0, 6.0, Rgba(1.0, 1.0, 1.0, 1.0));
        }
        if can_len && p.can_seek {
            self.seek = (self.origin.0 + bx, bw);
            self.push_hit((bx - 4.0, by - 4.0, bw + 8.0, 22.0), sk);
        }
        // times + transport
        let cy = y + 100.0 + 21.0;
        let cxm = w / 2.0;
        // shuffle / repeat (when the player has them), lit in the cover's colour while on
        for (hit, on, icon, x) in [(Hit::Shuffle, p.shuffle, "shuffle", 18.0), (Hit::Loop, p.loop_status.as_ref().map(|l| l != "None"), "repeat", w - 18.0 - 32.0)] {
            let Some(on) = on else { continue };
            let hv = self.hov(&hit);
            if on || hv {
                circle(c, x + 16.0, cy, 16.0, if on { amb.mul_a(0.22) } else { Rgba(1.0, 1.0, 1.0, 0.1) });
            }
            c.icon(t, x + 16.0, cy, icon, 14.0, if on { M_FG } else { M_FG2.mul_a(0.7) });
            if hit == Hit::Loop && p.loop_status.as_deref() == Some("Track") {
                let ss = st(8.0, 800.0);
                text_cy(c, t, x + 16.0 + 6.0, cy - 7.0, "1", &ss, M_FG, None);
            }
            self.push_hit((x, cy - 16.0, 32.0, 32.0), hit);
        }
        for (hit, icon, x, size, en) in [(Hit::Prev, "prev", cxm - 21.0 - 14.0 - 34.0, 34.0, p.can_prev), (Hit::Next, "next", cxm + 21.0 + 14.0, 34.0, p.can_next)] {
            let hv = self.hov(&hit);
            if hv && en {
                circle(c, x + size / 2.0, cy, size / 2.0, Rgba(1.0, 1.0, 1.0, 0.12));
            }
            c.icon(t, x + size / 2.0, cy, icon, 15.0, M_FG.mul_a(if en { 1.0 } else { 0.4 }));
            if en {
                self.push_hit((x, cy - size / 2.0, size, size), hit);
            }
        }
        let ph = Hit::Play;
        let phv = self.hov(&ph);
        let r = if phv { 22.7 } else { 21.0 };
        crate::glass::shadow(c, cxm - 21.0, cy - 21.0 + 3.0, 42.0, 42.0, 21.0, 16.0, 0.0, amb.mul_a(if phv { 0.55 } else { 0.35 }));
        circle(c, cxm, cy, r, WHITE.mul_a(0.96));
        c.icon(t, cxm, cy, if p.playing { "pause" } else { "play" }, 17.0, INK);
        self.push_hit((cxm - 21.0, cy - 21.0, 42.0, 42.0), ph);
    }
    // the players worth a card (something loaded), and the one shown: the picked one, else the playing one
    fn np_players(players: &[crate::media::Player]) -> Vec<&crate::media::Player> {
        players.iter().filter(|p| !p.title.is_empty()).collect()
    }
    fn np_player(&self, players: &[crate::media::Player]) -> Option<crate::media::Player> {
        let list = Self::np_players(players);
        self.np_pick.as_ref().and_then(|b| list.iter().find(|p| &p.bus == b)).or_else(|| list.iter().find(|p| p.playing)).or_else(|| list.first()).map(|p| (*p).clone())
    }
    fn paint_right(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32) {
        let player = self.np_player(cx.players);
        self.playing = player.as_ref().map(|p| p.playing).unwrap_or(false);
        // cards stacked from the top, each as tall as its content: the agenda as tall as what it lists, the phone's
        // notifications take what the agenda can spare (it keeps its own height up to 260 px: week, weather, two rows),
        // a long agenda takes what's left; free space stays at the bottom, like the left column's
        let gap = 12.0;
        let np = if player.is_some() { 146.0 + gap } else { 0.0 };
        let need = self.cal_need(cx);
        let mut notes = if player.is_some() { 3 } else { 6 };
        while notes > 1 && h - np - self.phone_h(notes) - gap < need.min(260.0) {
            notes -= 1;
        }
        let ph = self.phone_h(notes);
        let ch = self.paint_cal(c, cx, w, need.min(h - np - ph - gap));
        let py = ch + gap;
        let ph = self.paint_phone(c, cx, py, w, notes);
        if let Some(p) = player {
            self.paint_np(c, cx, py + ph + gap, w, &p);
        }
    }
}

// a scrolled list's edges fade out (premultiplied: every channel scaled) where more is hidden
fn fade(c: &mut Canvas, top: bool, bottom: bool) {
    let (w, h) = (c.pm.width() as usize, c.pm.height() as usize);
    let n = 28.min(h / 2);
    let data = c.pm.data_mut();
    for r in 0..n {
        let k = (r as f32 + 0.5) / n as f32;
        let k = k * k * (3.0 - 2.0 * k);
        for (on, row) in [(top, r), (bottom, h - 1 - r)] {
            if !on {
                continue;
            }
            for px in data[row * w * 4..(row + 1) * w * 4].iter_mut() {
                *px = (*px as f32 * k) as u8;
            }
        }
    }
}

// a phone notification that only advertises (kept off the card)
fn promo(n: &Value) -> bool {
    let s = format!("{} {} {}", n["app"].as_str().unwrap_or(""), n["title"].as_str().unwrap_or(""), n["text"].as_str().unwrap_or("")).to_lowercase();
    ["promocj", "okazj", "rabat", "zniżk", "wyprzeda", "kupon", "oferta", "newsletter", "black friday", " sale ", "% taniej"].iter().any(|k| s.contains(k))
}

// all of a day's events (the strip / bar: not filtered by "still ahead")
fn day_events_all(cal: &Value, d: Day) -> Vec<Value> {
    events_on(cal, d)
}

trait MulA {
    fn mul_a(self, k: f32) -> Rgba;
}
impl MulA for Rgba {
    fn mul_a(self, k: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, self.3 * k)
    }
}

impl Pane for Start {
    fn name(&self) -> &'static str {
        "start"
    }
    fn namespace(&self) -> &'static str {
        "mist-start"
    }
    fn opened(&mut self, screen: &str) {
        self.screen = screen.to_string();
    }
    fn animating(&self) -> bool {
        self.enter.running()
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(h) if !matches!(h, Hit::Sheet | Hit::Field))
    }
    fn seed(&mut self, q: &str) {
        self.field.text = q.into();
    }
    fn set_page(&mut self, p: &str) -> bool {
        // `--pane-png start:all`
        if p == "all" {
            self.tab = 1;
            return true;
        }
        false
    }
    fn data(&mut self, what: &str) -> bool {
        matches!(what, "players" | "phone" | "calendar")
    }
    fn slow(&mut self) -> bool {
        let mut ch = false;
        if let Some(rx) = &self.dl_rx {
            if let Ok(v) = rx.try_recv() {
                self.downloads = v;
                self.dl_rx = None;
                ch = true;
            }
        }
        // the phone card's feedback follows the 250 ms tick, the scrubber once a second
        self.ticks += 1;
        if self.note.1.is_some_and(|a| a.elapsed().as_millis() > 2700) {
            self.note.1 = None;
            ch = true;
        }
        if self.phone_said.1.is_some_and(|a| a.elapsed().as_millis() > 2600) {
            self.phone_said.1 = None;
            ch = true;
        }
        ch || (self.playing && self.ticks % 4 == 0)
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        self.hits.clear();
        // the dim (< ignore_alpha .25: not blurred)
        let k = self.enter.value();
        let (cw, ch) = (c.w(), c.h());
        let d = 0.22 * k.min(1.0);
        c.pm.fill(tiny_skia::Color::from_rgba(3.0 / 255.0, 6.0 / 255.0, 12.0 / 255.0, d).unwrap());
        let (sx, sy) = (((cw - W) / 2.0).round(), ((ch - H) / 2.0 - 12.0).round());
        self.sheet = (sx, sy);
        self.hits.push(((sx, sy, W, H), Hit::Sheet));
        // the sheet into a layer (scale .965 -> 1 and fade in while opening); its glass and glows are drawn once
        // (again when the accent follows a new wallpaper)
        let acc = accent();
        if self.bg_acc != acc {
            self.bg = None;
            self.bg_acc = acc;
        }
        let bg = self.bg.get_or_insert_with(|| {
            // the panes' graphite glass, lit by the accent from the top left and a violet haze from the bottom right
            let mut l = Canvas::new(W as u32, H as u32);
            glass(&mut l, 0.0, 0.0, W, H, 30.0, 0.82);
            if let Some(p) = crate::glass::sq_path(0.0, 0.0, W, H, 30.0) {
                crate::desktop::fill_rad(&mut l, &p, 140.0, -60.0, 560.0, &[(0.0, acc.mul_a(0.26)), (0.55, acc.mul_a(0.07)), (1.0, CLEAR)]);
                crate::desktop::fill_rad(&mut l, &p, W - 60.0, H + 80.0, 520.0, &[(0.0, M_PURPLE.mul_a(0.14)), (1.0, CLEAR)]);
            }
            glass_rim(&mut l, 0.0, 0.0, W, H, 30.0);
            l.pm
        });
        let mut l = Canvas { pm: bg.clone() };
        let id = tiny_skia::Transform::identity();
        let pp = tiny_skia::PixmapPaint::default();
        // the top row: the search pill, the power buttons at its right
        self.origin = (sx, sy);
        let t = &mut *cx.t;
        let cy = FY + FH / 2.0;
        let powers = ["lock", "zzz", "logout", "reload", "power"];
        let arms = ["", "", "Wyloguj", "Uruchom ponownie", "Wyłącz"];
        // one quiet glass group (the session's actions are not the menu's main thing), a hairline between
        // lock / sleep and the ones that end the session
        let (bs, pad, gap) = (40.0, 6.0, 2.0);
        let pw = pad * 2.0 + 5.0 * bs + 4.0 * gap + 9.0;
        let gx = W - 32.0 - pw;
        Self::lens(&mut l, gx, FY, pw, FH, FH / 2.0, 0.07);
        let mut px = gx + pad;
        for (i, icon) in powers.iter().enumerate() {
            if i == 2 {
                rect(&mut l, px + 3.5, cy - 10.0, 1.0, 20.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.1));
                px += 9.0;
            }
            let h = Hit::Power(i);
            let hov = self.hov(&h);
            let armed = !arms[i].is_empty() && self.armed == arms[i];
            let red = armed || (*icon == "power" && hov);
            if red {
                rect(&mut l, px, cy - bs / 2.0, bs, bs, bs / 2.0, DANGER.mul_a(0.28));
            } else if hov {
                rect(&mut l, px, cy - bs / 2.0, bs, bs, bs / 2.0, FILL2);
            }
            l.icon(t, px + bs / 2.0, cy, icon, 16.0, if red { M_BAD } else if hov { M_FG } else { M_FG2 });
            self.push_hit((px, cy - bs / 2.0, bs, bs), h);
            px += bs + gap;
        }
        let fw = W - 64.0 - pw - 12.0;
        self.origin = (sx + 32.0, sy + FY);
        let mut field = Canvas::new(fw as u32, FH as u32);
        self.paint_field(&mut field, cx, fw);
        l.pm.draw_pixmap(32, FY as i32, field.pm.as_ref(), &pp, id, None);
        // a power tip, the second-click warning or a note: a small pill at the right of the view switch
        let note = self.note.1.filter(|a| a.elapsed().as_millis() < 2600).map(|_| self.note.0.clone());
        let tip = if let Some(n) = note {
            Some((n, M_WARN))
        } else if !self.armed.is_empty() {
            Some((format!("Kliknij jeszcze raz: {}", self.armed.to_lowercase()), M_BAD))
        } else if !self.power_tip.is_empty() {
            Some((self.power_tip.clone(), M_FG2))
        } else {
            None
        };
        if let Some((s, col)) = tip {
            let t = &mut *cx.t;
            let ts = st(11.5, 600.0);
            let w = t.width(&s, &ts) + 22.0;
            let (tx, ty) = (32.0 + LW - w, TOP + 4.0);
            rect(&mut l, tx, ty, w, 24.0, 12.0, Rgba(0.0, 0.0, 0.0, 0.35));
            text_cy(&mut l, t, tx + 11.0, ty + 12.0, &s, &ts, col, None);
        }
        // left
        self.origin = (sx + 32.0, sy + TOP);
        let mut left = Canvas::new(LW as u32, (H - TOP - 30.0) as u32);
        self.paint_left(&mut left, cx);
        l.pm.draw_pixmap(32, TOP as i32, left.pm.as_ref(), &pp, id, None);
        // right (the now playing card's glow may hang below the column)
        let rx = 32.0 + LW + 40.0;
        let (ry, rw, rh) = (TOP, W - rx - 32.0, H - TOP - 30.0);
        self.origin = (sx + rx, sy + ry);
        let mut right = Canvas::new(rw as u32, rh as u32 + 24);
        self.paint_right(&mut right, cx, rw, rh);
        l.pm.draw_pixmap(rx as i32, ry as i32, right.pm.as_ref(), &pp, id, None);
        self.origin = (sx, sy);
        let t = &mut *cx.t;
        // the right-click menu (a small glass sheet, the dock's app menu style)
        if let Some(m) = &self.menu {
            let items = self.menu_items(m.app, cx.apps);
            let (mw, rh) = (220.0, 34.0);
            let mh = items.len() as f32 * rh + 12.0;
            let (mx, my) = (m.x.min(W - mw - 12.0), m.y.min(H - mh - 12.0));
            crate::glass::shadow(&mut l, mx, my + 8.0, mw, mh, 14.0, 24.0, -4.0, Rgba(0.0, 0.0, 0.0, 0.35));
            // nothing blurs inside the sheet: a solid body so the icons under it do not show through
            if let Some(p) = crate::glass::sq_path(mx, my, mw, mh, 14.0) {
                fill_lin(&mut l, &p, (0.0, my), (0.0, my + mh), &[(0.0, Rgba::rgb8(44, 46, 54, 0.98)), (1.0, Rgba::rgb8(34, 36, 43, 0.98))]);
            }
            glass_rim(&mut l, mx, my, mw, mh, 14.0);
            for (k, (icon, label)) in items.iter().enumerate() {
                let (ix, iy) = (mx + 6.0, my + 6.0 + k as f32 * rh);
                let h = Hit::Menu(k);
                let hv = self.hov(&h);
                if hv {
                    rect(&mut l, ix, iy, mw - 12.0, rh, 9.0, accent().mul_a(0.92));
                }
                l.icon(t, ix + 18.0, iy + rh / 2.0, icon, 15.0, if hv { WHITE } else { M_FG2 });
                text_cy(&mut l, t, ix + 36.0, iy + rh / 2.0, label, &st(13.0, 500.0), if hv { WHITE } else { M_FG }, Some(mw - 56.0));
                self.push_hit((ix, iy, mw - 12.0, rh), h);
            }
        }
        // the layer onto the surface
        if k >= 1.0 {
            c.pm.draw_pixmap(sx as i32, sy as i32, l.pm.as_ref(), &pp, id, None);
        } else {
            let s = 0.965 + 0.035 * k;
            let paint = tiny_skia::PixmapPaint { opacity: (k / 0.7).min(1.0), quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
            let tf = tiny_skia::Transform::from_row(s, 0.0, 0.0, s, sx + W * (1.0 - s) / 2.0, sy + H * (1.0 - s) / 2.0);
            c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &paint, tf, None);
        }
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let hit = self.hit();
        match ev {
            Ptr::Enter | Ptr::Leave => Act::Redraw,
            Ptr::Motion => {
                // the power tooltip in the footer; hovering a result selects it
                let tip = match &hit { Some(Hit::Power(i)) => ["Zablokuj", "Uśpij", "Wyloguj", "Uruchom ponownie", "Wyłącz"][*i].to_string(), _ => String::new() };
                self.power_tip = tip;
                self.phone_hint = match &hit {
                    Some(Hit::PhoneAct(c)) => match *c { "ring" => "Znajdź telefon (dzwoni)", "clip" => "Wyślij schowek", "pick" => "Wyślij pliki", "browse" => "Przeglądaj pliki telefonu", "photo" => "Zrób zdjęcie telefonem", "expand" => "Wiadomości SMS", "mirror" => "Ekran telefonu na komputerze", _ => "" }.into(),
                    Some(Hit::PhoneMore) => if self.phone_more { "Mniej".into() } else { "Więcej akcji".into() },
                    Some(Hit::Note) => "Otwórz w panelu telefonu".into(),
                    _ => String::new(),
                };
                if let Some(Hit::Res(i)) = hit {
                    self.sel = i;
                }
                if let Some(d) = self.drag.as_mut() {
                    if !d.live && ((x - d.at.0).powi(2) + (y - d.at.1).powi(2)).sqrt() > 6.0 {
                        d.live = true;
                    }
                    self.drop_at = match hit { Some(Hit::Cell(j)) => Some(j), _ => None };
                }
                Act::Redraw
            }
            Ptr::Press(b) => {
                self.pressed_outside = hit.is_none();
                // an open menu: a press elsewhere only closes it
                if self.menu.is_some() && !matches!(hit, Some(Hit::Menu(_))) {
                    self.menu = None;
                    self.eat = true;
                    return Act::Redraw;
                }
                if b == 0x111 {
                    let app = match &hit {
                        Some(Hit::Cell(i)) => self.tab_apps(cx.apps).get(*i).copied(),
                        Some(Hit::AZ(k)) => Self::all_apps(cx.apps).get(*k).copied(),
                        Some(Hit::Res(i)) => match self.results(cx.apps).get(*i).map(|r| r.kind.clone()) {
                            Some(Kind::App(a)) => Some(a),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(app) = app {
                        self.menu = Some(Menu { app, x: x - self.sheet.0 + 2.0, y: y - self.sheet.1 + 2.0 });
                        self.eat = true;
                        return Act::Redraw;
                    }
                    return Act::None;
                }
                if let Some(Hit::Cell(i)) = hit {
                    self.drag = Some(Drag { from: i, at: (x, y), live: false });
                }
                Act::None
            }
            Ptr::Wheel(d) => {
                if !self.field.text.trim().is_empty() {
                    self.res_scroll += d * 48.0;
                } else if self.tab == 1 {
                    self.az_scroll += d * 52.0;
                }
                Act::Redraw
            }
            Ptr::Release(b) => {
                if std::mem::take(&mut self.eat) {
                    return Act::None;
                }
                if let Some(d) = self.drag.take() {
                    if d.live {
                        if let Some(j) = self.drop_at.take() {
                            self.move_pin(d.from, j, cx.apps);
                        }
                        return Act::Redraw;
                    }
                }
                if b == 0x111 {
                    return Act::None;
                }
                let Some(h) = hit else {
                    return if self.pressed_outside { Act::Close } else { Act::None };
                };
                match h {
                    Hit::Field => {
                        if self.field.sel_all {
                            self.field.sel_all = false;
                            Act::Redraw
                        } else {
                            Act::None
                        }
                    }
                    Hit::Sheet => Act::None,
                    // a phone notification: the phone panel, where it can be answered
                    Hit::Note => Act::Toggle("phone"),
                    Hit::Menu(k) => self.menu_pick(k, cx.apps),
                    Hit::Res(i) => {
                        let res = self.results(cx.apps);
                        match res.get(i) {
                            Some(r) => self.activate(r, cx.apps),
                            None => Act::None,
                        }
                    }
                    Hit::Switch => {
                        self.tab = 1 - self.tab;
                        self.gsel = -1;
                        Act::Redraw
                    }
                    Hit::Cell(i) => match self.tab_apps(cx.apps).get(i) {
                        Some(&ai) => self.launch(ai, cx.apps),
                        None => Act::None,
                    },
                    Hit::AZ(k) => match Self::all_apps(cx.apps).get(k) {
                        Some(&ai) => self.launch(ai, cx.apps),
                        None => Act::None,
                    },
                    Hit::File(i) => match self.sug().get(i) {
                        Some((p, _)) => Act::Run(vec!["xdg-open".into(), p.clone()]),
                        None => Act::None,
                    },
                    Hit::Reveal(i) => match self.sug().get(i) {
                        Some((p, _)) => Act::Run(vec![format!("{}/.local/bin/mist-files", home()), "--select".into(), p.clone()]),
                        None => Act::None,
                    },
                    Hit::CalHead => Act::Toggle("calendar"),
                    Hit::CalNew => {
                        // a new event on the picked day: today at the next full hour, other days at 9
                        let td = today();
                        let d = day(td.y, td.m, td.d + self.pick as i32);
                        let h = crate::data::local(crate::data::now()).tm_hour;
                        let at = if self.pick == 0 { (h < 23).then_some((h + 1, 0)) } else { Some((9, 0)) };
                        Act::Run(vec!["xdg-open".into(), gcal_new("", d, at, 60)])
                    }
                    Hit::Day(i) => {
                        self.pick = i;
                        Act::Redraw
                    }
                    Hit::Soon(off) => {
                        if (0..7).contains(&off) {
                            self.pick = off as usize;
                            Act::Redraw
                        } else {
                            Act::Toggle("calendar")
                        }
                    }
                    Hit::PhoneExpand => Act::Toggle("phone"),
                    Hit::PhoneMore => {
                        self.phone_more = !self.phone_more;
                        Act::Redraw
                    }
                    Hit::PhoneAct(c) => match c {
                        "ring" => {
                            self.ph.send("ring", None);
                            self.phone_said = ("Telefon dzwoni".into(), Some(Instant::now()));
                            Act::Redraw
                        }
                        "clip" => {
                            self.ph.send("clip", None);
                            self.phone_said = ("Schowek wysłany".into(), Some(Instant::now()));
                            Act::Redraw
                        }
                        "photo" => {
                            self.ph.send("photo", None);
                            self.phone_said = ("Zrób zdjęcie na telefonie".into(), Some(Instant::now()));
                            Act::Redraw
                        }
                        "expand" => Act::Toggle("phone"),
                        c => self.phone_cmd(c, None),
                    },
                    Hit::PhoneAccept => { self.ph.send("accept", None); Act::Redraw }
                    Hit::PhoneReject => { self.ph.send("reject", None); Act::Redraw }
                    Hit::PhonePair => { self.ph.send("pair", None); Act::Redraw }
                    Hit::NoteX(id) => { self.ph.send("dismiss", Some(json!(id))); Act::Redraw }
                    Hit::Art => {
                        // raise the player: its desktop entry's window
                        let p = self.np_player(cx.players);
                        match p {
                            Some(p) if crate::switcher::lua_name_ok(&p.entry) => Act::Run(crate::switcher::hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"class:(?i){}\" }}))", p.entry))),
                            _ => Act::None,
                        }
                    }
                    Hit::Seek => {
                        let p = self.np_player(cx.players);
                        match p {
                            Some(p) => Act::Media(crate::media::Cmd::Seek(p.bus.clone(), p.track_id.clone(), (((x - self.seek.0) / self.seek.1).clamp(0.0, 1.0) as f64) * p.length)),
                            None => Act::None,
                        }
                    }
                    Hit::Prev | Hit::Play | Hit::Next => {
                        let p = self.np_player(cx.players);
                        match p {
                            Some(p) => Act::Media(match h { Hit::Prev => crate::media::Cmd::Prev(p.bus), Hit::Next => crate::media::Cmd::Next(p.bus), _ => crate::media::Cmd::Toggle(p.bus) }),
                            None => Act::None,
                        }
                    }
                    Hit::Src => {
                        // the next player with something loaded
                        let list = Self::np_players(cx.players);
                        let cur = self.np_player(cx.players).map(|p| p.bus).unwrap_or_default();
                        let k = list.iter().position(|p| p.bus == cur).map(|k| (k + 1) % list.len().max(1)).unwrap_or(0);
                        self.np_pick = list.get(k).map(|p| p.bus.clone());
                        Act::Redraw
                    }
                    Hit::Shuffle | Hit::Loop => match self.np_player(cx.players) {
                        Some(p) if h == Hit::Shuffle => Act::Media(crate::media::Cmd::Shuffle(p.bus.clone(), !p.shuffle.unwrap_or(false))),
                        Some(p) => Act::Media(crate::media::Cmd::Loop(p.bus.clone(), match p.loop_status.as_deref() { Some("None") => "Playlist", Some("Playlist") => "Track", _ => "None" }.into())),
                        None => Act::None,
                    },
                    Hit::Power(i) => {
                        // lock at once; the rest ask in the session pane, as from the bar
                        match i {
                            0 => Act::Run(crate::session::cmd("lock")),
                            1 => Act::Run(crate::session::cmd("suspend")),
                            2 => Act::Open("session", "logout"),
                            3 => Act::Open("session", "reboot"),
                            _ => Act::Open("session", "poweroff"),
                        }
                    }
                }
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        let empty = self.field.text.is_empty();
        let gcols: i32 = if self.tab == 1 { 2 } else { 6 };
        let gcount = if self.tab == 1 { Self::all_apps(cx.apps).len() } else { self.tab_apps(cx.apps).len() } as i32;
        let gmove = |g: i32, d: i32| if g < 0 { 0 } else { (g + d).clamp(0, (gcount - 1).max(0)) };
        match ev.keysym {
            Keysym::Escape if self.menu.is_some() || self.drag.is_some() => {
                self.menu = None;
                self.drag = None;
                Act::Redraw
            }
            Keysym::Escape => {
                if !empty {
                    self.field.text.clear();
                    self.field.sel_all = false;
                    self.sel = 0;
                    self.armed.clear();
                    Act::Redraw
                } else {
                    Act::Close
                }
            }
            Keysym::Down => {
                if !empty {
                    self.sel += 1;
                } else {
                    self.gsel = gmove(self.gsel, if self.gsel < 0 { 0 } else { gcols });
                }
                Act::Redraw
            }
            Keysym::Up => {
                if !empty {
                    self.sel = self.sel.saturating_sub(1);
                } else if self.gsel >= 0 {
                    self.gsel = gmove(self.gsel, -gcols);
                }
                Act::Redraw
            }
            Keysym::Left | Keysym::Right if empty => {
                self.gsel = gmove(self.gsel, if ev.keysym == Keysym::Left { -1 } else { 1 });
                Act::Redraw
            }
            Keysym::Tab | Keysym::ISO_Left_Tab => {
                let back = ev.keysym == Keysym::ISO_Left_Tab;
                if !empty {
                    let n = self.results(cx.apps).len().max(1);
                    self.sel = if back { self.sel.saturating_sub(1) } else { (self.sel + 1) % n };
                } else {
                    self.tab = 1 - self.tab;
                    self.gsel = -1;
                }
                Act::Redraw
            }
            Keysym::Return | Keysym::KP_Enter => {
                if !empty {
                    let res = self.results(cx.apps);
                    match res.get(self.sel.min(res.len().saturating_sub(1))) {
                        Some(r) => self.activate(r, cx.apps),
                        None => Act::None,
                    }
                } else if self.gsel >= 0 {
                    let ai = if self.tab == 1 { Self::all_apps(cx.apps).get(self.gsel as usize).copied() } else { self.tab_apps(cx.apps).get(self.gsel as usize).copied() };
                    match ai {
                        Some(ai) => self.launch(ai, cx.apps),
                        None => Act::None,
                    }
                } else {
                    Act::None
                }
            }
            _ => {
                if self.field.key(ev, cx.ctrl) {
                    self.sel = 0;
                    self.armed.clear();
                    self.res_scroll = 0.0;
                    Act::Redraw
                } else {
                    Act::None
                }
            }
        }
    }
}

#[cfg(test)]
mod quick_tests {
    use super::*;
    // Saturday 7 November 2026, 20:00
    fn td() -> Day {
        day(2026, 10, 7)
    }
    fn q(s: &str) -> Option<(String, (i32, i32, i32), Option<(i32, i32)>, i64)> {
        quick_event(s, td(), (20, 0)).map(|e| (e.title, (e.day.y, e.day.m + 1, e.day.d), e.at, e.mins))
    }
    #[test]
    fn parses() {
        assert_eq!(q("jutro 15 dentysta"), Some(("Dentysta".into(), (2026, 11, 8), Some((15, 0)), 60)));
        assert_eq!(q("spotkanie w poniedziałek o 10:30"), Some(("Spotkanie".into(), (2026, 11, 9), Some((10, 30)), 60)));
        assert_eq!(q("kino 12.11 18:00-20:30"), Some(("Kino".into(), (2026, 11, 12), Some((18, 0)), 150)));
        assert_eq!(q("urodziny mamy 3.01"), Some(("Urodziny mamy".into(), (2027, 1, 3), None, 60)));
        assert_eq!(q("trening 7:30 45min"), Some(("Trening".into(), (2026, 11, 8), Some((7, 30)), 45)));
        assert_eq!(q("sobota grill"), Some(("Grill".into(), (2026, 11, 14), None, 60)));
        assert_eq!(q("dentist tomorrow at 15:30"), Some(("Dentist".into(), (2026, 11, 8), Some((15, 30)), 60)));
    }
    #[test]
    fn ignores_searches() {
        assert_eq!(q("spotify"), None);
        assert_eq!(q("zen browser"), None);
        assert_eq!(q("2 + 2"), None);
        assert_eq!(q("jutro"), None);
        assert_eq!(q("kitty 15"), None);
    }
    #[test]
    fn url() {
        let u = gcal_new("Dentysta Ząb", day(2026, 10, 8), Some((15, 0)), 60);
        assert!(u.ends_with("text=Dentysta%20Z%C4%85b&dates=20261108T150000/20261108T160000"), "{u}");
        assert!(gcal_new("X", day(2026, 11, 31), None, 60).ends_with("dates=20261231/20270101"));
    }
}
