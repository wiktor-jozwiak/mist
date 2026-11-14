// The desktop's extra widgets, one bento place each (COL_W x BENTO_H, the same glass as Garmin / running):
// Muzyka (the MPRIS player the island follows: cover, progress, prev / play / next), Kalendarz (the month with
// the days that have events, or the next events), System (CPU / RAM / disk / network, rings or bars) and
// Zegary (an analog or digital local clock + up to three cities). Their data comes from what mist-ui already
// has (island players, the calendar feed, the 2 s stats thread): no new thread, no polling. Options in
// desktop::groups, the adjust button flips a card to them like the others.
use crate::anim::Ease;
use crate::desktop::*;
use crate::gfx::{Canvas, Rgba};
use crate::text::Text;
use serde_json::Value;
use tiny_skia::{PathBuilder, Pixmap};

const MONTHS: [&str; 12] = ["Styczeń", "Luty", "Marzec", "Kwiecień", "Maj", "Czerwiec", "Lipiec", "Sierpień", "Wrzesień", "Październik", "Listopad", "Grudzień"];

// the cities of Zegary: key, name, standard UTC offset (h), daylight saving rule
pub const CITIES: [(&str, &str, i32, Dst); 16] = [
    ("london", "Londyn", 0, Dst::Eu),
    ("newyork", "Nowy Jork", -5, Dst::Us),
    ("sf", "San Francisco", -8, Dst::Us),
    ("tokyo", "Tokio", 9, Dst::No),
    ("sydney", "Sydney", 10, Dst::Au),
    ("dubai", "Dubaj", 4, Dst::No),
    ("shanghai", "Szanghaj", 8, Dst::No),
    ("lisbon", "Lizbona", 0, Dst::Eu),
    ("warsaw", "Warszawa", 1, Dst::Eu),
    ("chicago", "Chicago", -6, Dst::Us),
    ("saopaulo", "São Paulo", -3, Dst::No),
    ("reykjavik", "Reykjavik", 0, Dst::No),
    ("moscow", "Moskwa", 3, Dst::No),
    ("bangkok", "Bangkok", 7, Dst::No),
    ("singapore", "Singapur", 8, Dst::No),
    ("seoul", "Seul", 9, Dst::No),
];
#[derive(Clone, Copy, PartialEq)]
pub enum Dst {
    No,
    Eu,
    Us,
    Au,
}

// days since 1970-01-01 of a civil date (H. Hinnant)
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
// day of the month of the n-th Sunday (n = 0: the last one)
fn sunday(y: i64, m: i64, n: i64) -> i64 {
    let wd = |d: i64| (days(y, m, d) + 4).rem_euclid(7); // 0 = Sunday
    if n > 0 {
        let first = (7 - wd(1)) % 7 + 1;
        return first + 7 * (n - 1);
    }
    let len = days(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1) - days(y, m, 1);
    len - wd(len)
}
fn at(y: i64, m: i64, d: i64, h: i64) -> i64 {
    days(y, m, d) * 86400 + h * 3600
}
// the city's UTC offset in seconds at `t`
pub fn offset(base: i32, rule: Dst, t: i64) -> i64 {
    let std = base as i64 * 3600;
    let y = 1970 + (t.div_euclid(86400) as f64 / 365.2425) as i64;
    let y = if at(y + 1, 1, 1, 0) <= t { y + 1 } else if at(y, 1, 1, 0) > t { y - 1 } else { y };
    let dst = match rule {
        Dst::No => false,
        // last Sunday of March 01:00 UTC .. last Sunday of October 01:00 UTC
        Dst::Eu => t >= at(y, 3, sunday(y, 3, 0), 1) && t < at(y, 10, sunday(y, 10, 0), 1),
        // second Sunday of March 02:00 local .. first Sunday of November 02:00 local (daylight)
        Dst::Us => t >= at(y, 3, sunday(y, 3, 2), 2) - std && t < at(y, 11, sunday(y, 11, 1), 2) - std - 3600,
        // southern: first Sunday of October 02:00 local .. first Sunday of April 03:00 local (daylight)
        Dst::Au => t >= at(y, 10, sunday(y, 10, 1), 2) - std || t < at(y, 4, sunday(y, 4, 1), 3) - std - 3600,
    };
    std + if dst { 3600 } else { 0 }
}

fn mmss(s: f64) -> String {
    let s = s.max(0.0) as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

// the mean colour of a cover, lifted to a light tint for the card's glow
fn mean(pm: &Pixmap) -> Rgba {
    let (mut r, mut g, mut b, mut n) = (0f64, 0f64, 0f64, 0f64);
    for px in pm.pixels().iter().step_by(7) {
        let a = px.alpha() as f64;
        if a < 8.0 {
            continue;
        }
        r += px.red() as f64 / a;
        g += px.green() as f64 / a;
        b += px.blue() as f64 / a;
        n += 1.0;
    }
    if n == 0.0 {
        return ACC;
    }
    let (r, g, b) = ((r / n) as f32, (g / n) as f32, (b / n) as f32);
    let mx = r.max(g).max(b).max(0.01);
    // keep the hue, push it up to a pastel
    let k = 0.85 / mx;
    Rgba((r * k).min(1.0) * 0.7 + 0.3 * 0.85, (g * k).min(1.0) * 0.7 + 0.3 * 0.85, (b * k).min(1.0) * 0.7 + 0.3 * 0.85, 1.0)
}

fn statvfs(path: &str) -> Option<(f64, f64)> {
    let p = std::ffi::CString::new(path).ok()?;
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(p.as_ptr(), &mut s) != 0 {
            return None;
        }
        let tot = s.f_blocks as f64 * s.f_frsize as f64;
        let free = s.f_bavail as f64 * s.f_frsize as f64;
        Some((tot - free, tot))
    }
}
// the note card's text: a plain file next to widgets.json
pub fn note_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let cfg = std::env::var("XDG_CONFIG_HOME").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
    format!("{cfg}/mist/note.txt")
}
// the parts of the day, the week (from Monday), the month and the year that have passed at `t` (0..1), the
// month's and the year's length in days
pub fn passed(t: i64) -> ([f32; 4], i32, i32) {
    let tm = crate::data::local(t);
    let (y, m) = ((tm.tm_year + 1900) as i64, (tm.tm_mon + 1) as i64);
    let day = (tm.tm_hour * 3600 + tm.tm_min * 60 + tm.tm_sec) as f64 / 86400.0;
    let mlen = (days(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1) - days(y, m, 1)) as i32;
    let ylen = (days(y + 1, 1, 1) - days(y, 1, 1)) as i32;
    // in f64, then below 1: the year's last second is 0.99999997 of it, which f32 rounds up to a full 1.0
    let f = |v: f64| (v as f32).min(0.999_999_94);
    ([f(day), f((((tm.tm_wday + 6) % 7) as f64 + day) / 7.0), f(((tm.tm_mday - 1) as f64 + day) / mlen as f64), f((tm.tm_yday as f64 + day) / ylen as f64)], mlen, ylen)
}
fn uptime() -> String {
    let s: f64 = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| s.split_whitespace().next().and_then(|x| x.parse().ok())).unwrap_or(0.0);
    let (d, h, m) = ((s / 86400.0) as i64, ((s % 86400.0) / 3600.0) as i64, ((s % 3600.0) / 60.0) as i64);
    if d > 0 { format!("{d} d {h} h") } else if h > 0 { format!("{h} h {m} min") } else { format!("{m} min") }
}

impl Desk {
    // the card's header: caps title, a quiet note on the right (fades while hovered), the adjust button
    fn head(&mut self, c: &mut Canvas, t: &mut Text, key: &'static str, title: &str, right: &str, x: f32, y: f32, w: f32) {
        let editing = self.editing(key);
        let hov = self.inside(x, y, w, BENTO_H);
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        caps(c, t, cx, cy, title, FAINT, Some(cw - 40.0));
        let ro = self.fx.v(&format!("{key}.hdr"), if hov && !editing { 0.0 } else { 1.0 }, 160.0, Ease::Linear);
        if ro > 0.0 && !right.is_empty() {
            let rs = st(11.0, 550.0);
            let rw = width(t, right, &rs).min(cw * 0.55);
            text(c, t, cx + cw - rw, cy, right, &rs, MUTED.alpha(MUTED.3 * ro), true, Some(cw * 0.55));
        }
        let eb_hov = self.ptr.map(|p| p.0 >= cx + cw - 26.0 && p.0 < cx + cw && p.1 >= cy - 6.0 && p.1 < cy + 20.0).unwrap_or(false);
        edit_btn(c, t, &mut self.fx, &mut self.hits, key, cx + cw - 26.0, cy - 6.0, hov && !editing, eb_hov, key);
    }
    fn finish(&mut self, c: &mut Canvas, t: &mut Text, key: &'static str, title: &str, x: f32, y: f32, w: f32) {
        Self::card_rim(c, x, y, w, BENTO_H);
        let ptr = self.ptr;
        let editing = self.editing(key);
        edit_sheet(c, t, &mut self.fx, &mut self.hits, &self.cfg, key, title, x, y, w, BENTO_H, editing, self.sheet_scroll.entry(key).or_default(), ptr);
    }
    fn hovering(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.ptr.map(|p| p.0 >= x && p.0 < x + w && p.1 >= y && p.1 < y + h).unwrap_or(false)
    }

    // ---------------- Notatka: a text file on the desktop; "- [ ]" and "- [x]" lines are a to-do list, "# " a
    // heading, "- " a point. A click opens the file in the system's editor; the card reads it again with the
    // stage's next paint (the minute at the latest)
    pub(crate) fn note_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let paper = self.cfg.s("note", "look") == "paper";
        let glow = if paper { C_YELLOW.alpha(0.30) } else { WHITE.alpha(0.07) };
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((glow, 0.5, 0.15, 0.95)));
        let body = std::fs::read_to_string(note_path()).unwrap_or_default();
        let todo = body.lines().filter(|l| l.trim_start().starts_with("- [ ]")).count();
        let right = if todo > 0 { format!("{todo} do zrobienia") } else { String::new() };
        self.head(c, t, "note", "Notatka", &right, x, y, w);
        let fsz = match self.cfg.s("note", "size").as_str() { "small" => 12.0, "large" => 16.0, _ => 14.0 };
        let (top, bottom) = (cy + 30.0, y + BENTO_H - PAD);
        let ink = if paper { Rgba::rgb8(0xfb, 0xf1, 0xd6, 0.97) } else { FG };
        if body.trim().is_empty() {
            c.icon(t, x + w / 2.0, y + BENTO_H / 2.0 - 18.0, "fileText", 34.0, WHITE.alpha(0.35));
            for (i, (s, sty, col)) in [("Pusta notatka", st(14.0, 600.0), MUTED), ("Kliknij, aby pisać", st(12.0, 500.0), FAINT)].into_iter().enumerate() {
                let tw = width(t, s, &sty);
                text(c, t, x + (w - tw) / 2.0, y + BENTO_H / 2.0 + 10.0 + 20.0 * i as f32, s, &sty, col, true, None);
            }
        }
        let mut ly = top;
        'lines: for raw in body.lines() {
            let l = raw.trim_end();
            if l.trim().is_empty() {
                ly += fsz * 0.6;
                continue;
            }
            let tl = l.trim_start();
            // what leads the line: a box (done or not), a dot, nothing; a heading is heavier
            let (mark, s, sty, col) = if let Some(r) = tl.strip_prefix("- [ ] ") {
                (1, r, st(fsz, 550.0), ink)
            } else if let Some(r) = tl.strip_prefix("- [x] ").or(tl.strip_prefix("- [X] ")) {
                (2, r, st(fsz, 500.0), MUTED.alpha(0.6))
            } else if let Some(r) = tl.strip_prefix("- ").or(tl.strip_prefix("* ")) {
                (3, r, st(fsz, 550.0), ink)
            } else if let Some(r) = tl.strip_prefix("# ") {
                (0, r, st(fsz + 3.0, 700.0), ink)
            } else {
                (0, tl, st(fsz, 550.0), ink)
            };
            let ind = if mark > 0 { fsz + 8.0 } else { 0.0 };
            let lh = (line_h(t, &sty) * 1.08).round();
            for (i, part) in crate::gfx::wrap(t, s, &sty, cw - ind, 4).iter().enumerate() {
                if ly + lh > bottom {
                    // more than fits: say so in the corner
                    let ms = st(11.0, 600.0);
                    let mw = width(t, "…", &ms);
                    text(c, t, cx + cw - mw, bottom - 12.0, "…", &ms, MUTED, true, None);
                    break 'lines;
                }
                if i == 0 {
                    let (bx, by, bs) = (cx, ly + (lh - fsz) / 2.0 + 1.0, fsz - 2.0);
                    match mark {
                        1 => border(c, bx, by, bs, bs, 4.0, 1.5, FG2.alpha(0.7)),
                        2 => {
                            rect(c, bx, by, bs, bs, 4.0, ACC.alpha(0.75));
                            c.icon(t, bx + bs / 2.0, by + bs / 2.0, "tick", bs - 2.0, Rgba(0.04, 0.06, 0.1, 0.95));
                        }
                        3 => circle(c, bx + bs / 2.0, by + bs / 2.0, 2.2, FG2.alpha(0.8)),
                        _ => {}
                    }
                }
                text(c, t, cx + ind, ly, part, &sty, col, true, Some(cw - ind));
                ly += lh;
            }
            ly += 2.0;
        }
        if !self.editing("note") {
            self.hits.push((cx, top, cw, bottom - top, Hit::Note));
        }
        self.finish(c, t, "note", "Notatka", x, y, w);
    }

    // ---------------- Czas: how much of the day, the week, the month and the year has passed, and a countdown
    pub(crate) fn time_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        let tint = Rgba::rgb8(0xa9, 0xa6, 0xf5, 1.0);
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((tint.alpha(0.16), 0.75, 0.2, 0.7)));
        self.head(c, t, "time", "Czas", &format!("{} {}", tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]), x, y, w);
        let (f, mlen, ylen) = passed(now);
        let left = 1440 - (tm.tm_hour * 60 + tm.tm_min);
        let rows: Vec<(&str, f32, String)> = self.cfg.list("time", "show").iter().filter_map(|k| match k.as_str() {
            "day" => Some(("Dzień", f[0], crate::wxcard::dur(left))),
            "week" => Some(("Tydzień", f[1], format!("{}/7", (tm.tm_wday + 6) % 7 + 1))),
            "month" => Some(("Miesiąc", f[2], format!("{}/{}", tm.tm_mday, mlen))),
            "year" => Some(("Rok", f[3], format!("{}/{}", tm.tm_yday + 1, ylen))),
            _ => None,
        }).collect();
        let count = self.cfg.s("time", "count");
        let room = if count == "hide" { BENTO_H - 2.0 * PAD - 30.0 } else { FOOT_Y - 12.0 - 30.0 };
        let step = (room / rows.len().max(1) as f32).min(60.0);
        let mut ry = cy + 30.0 + (room - step * rows.len() as f32) / 2.0;
        for (lab, v, det) in &rows {
            text(c, t, cx, ry + 3.0, lab, &st(12.0, 550.0), MUTED, true, None);
            let (vs, us) = (st(18.0, 600.0).tnum(), st(11.0, 500.0).tnum());
            let pct = format!("{}", (v * 100.0).floor());
            let (vw, uw, dw) = (width(t, &pct, &vs), width(t, "%", &us), width(t, det, &us));
            text(c, t, cx + cw - vw - uw - 2.0, ry - 3.0, &pct, &vs, FG, true, None);
            text(c, t, cx + cw - uw, ry + 3.0, "%", &us, MUTED, true, None);
            text(c, t, cx + cw - vw - uw - 12.0 - dw, ry + 3.0, det, &us, FAINT, true, None);
            let by = ry + 27.0;
            rect(c, cx, by, cw, 5.0, 2.5, WHITE.alpha(0.12));
            rect(c, cx, by, (cw * v.clamp(0.0, 1.0)).max(5.0), 5.0, 2.5, tint.alpha(0.92));
            ry += step;
        }
        // the countdown: the next event of the calendar, the weekend, the New Year
        let n_days = |n: i64| match n { 0 => "dziś".to_string(), 1 => "jutro".to_string(), n => format!("za {n} dni") };
        let foot: Option<(&str, String, String)> = match count.as_str() {
            "hide" => None,
            "weekend" => {
                let wd = (tm.tm_wday + 6) % 7; // 0 = Monday
                Some(("calendar", "Weekend".into(), if wd >= 5 { "trwa".into() } else { n_days(5 - wd as i64) }))
            }
            "newyear" => Some(("sparkles", "Nowy Rok".into(), n_days((ylen - tm.tm_yday) as i64))),
            _ => Some(match crate::data::upcoming(&self.cal, 1, 365).first() {
                Some(e) => ("calEvent", e["title"].as_str().unwrap_or("").to_string(), crate::data::rel(e["start"].as_i64().unwrap_or(0))),
                None => ("calEvent", "Nic w planach".into(), String::new()),
            }),
        };
        if let Some((icon, title, when)) = foot {
            let fy = y + PAD + FOOT_Y;
            rect(c, cx, fy - 11.0, cw, 1.0, 0.0, HAIR);
            c.icon(t, cx + 7.0, fy + 12.0, icon, 14.0, tint);
            let ws = st(12.0, 650.0).tnum();
            let ww = width(t, &when, &ws);
            let fs = st(13.0, 600.0);
            let flh = line_h(t, &fs);
            text(c, t, cx + 22.0, fy + (24.0 - flh) / 2.0, &title, &fs, FG, true, Some(cw - 22.0 - ww - 10.0));
            let wlh = line_h(t, &ws);
            text(c, t, cx + cw - ww, fy + (24.0 - wlh) / 2.0, &when, &ws, tint, true, None);
        }
        self.finish(c, t, "time", "Czas", x, y, w);
    }

    // ---------------- Słońce i Księżyc: the day's arc with the Sun on it, the Moon's disc and phase
    pub(crate) fn astro_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        let nowms = (now / 60 * 60) as f64 * 1000.0;
        let wx = self.weather.clone();
        let (lat, lon) = (wx["lat"].as_f64().unwrap_or(51.48), wx["lon"].as_f64().unwrap_or(0.0));
        let view = self.cfg.s("astro", "view");
        let (sun_on, moon_on) = (view != "moon", view != "sun");
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((if sun_on { C_YELLOW.alpha(0.14) } else { C_BLUE.alpha(0.14) }, 0.5, 0.2, 0.75)));
        let sun = crate::astro::sun(nowms, lat, lon);
        let moon = crate::astro::moon(nowms, lat, lon);
        self.head(c, t, "astro", match view.as_str() { "sun" => "Słońce", "moon" => "Księżyc", _ => "Słońce i Księżyc" }, "", x, y, w);
        let dim = FG2.alpha(0.72);
        let (ls, vs) = (st(10.5, 500.0), st(13.0, 650.0).tnum());
        let lh_ = line_h(t, &ls);
        let alt_txt = |alt: f64| format!("{}{}°", if alt < -0.5 { "−" } else { "" }, alt.abs().round());
        let cell = |c: &mut Canvas, t: &mut Text, n: usize, col: usize, yy: f32, label: &str, value: &str| {
            let cwid = cw / n as f32;
            let xx = cx + col as f32 * cwid;
            text(c, t, xx, yy, label, &ls, dim, true, Some(cwid - 6.0));
            text(c, t, xx, yy + lh_ + 1.0, value, &vs, FG, true, Some(cwid - 6.0));
        };
        let mut top = cy + 30.0;
        if sun_on {
            let days = wx["days"].as_array().cloned().unwrap_or_default();
            let (r, s_) = (days.first().and_then(|d| d["rise"].as_str()).or(wx["sunrise"].as_str()).unwrap_or(""), days.first().and_then(|d| d["set"].as_str()).or(wx["sunset"].as_str()).unwrap_or(""));
            let (rm, sm, nm) = (crate::wxcard::mins(r), crate::wxcard::mins(s_).max(crate::wxcard::mins(r) + 1), tm.tm_hour * 60 + tm.tm_min);
            let day = nm >= rm && nm < sm;
            // the arc: taller when the card is the Sun's alone
            let amp = if moon_on { 44.0 } else { 84.0 };
            let (ax0, ax1, hy) = (cx + 30.0, cx + cw - 30.0, top + amp + 8.0);
            let pt = |f: f32| (ax0 + f * (ax1 - ax0), hy - amp * (std::f32::consts::PI * f).sin());
            let arc = |f1: f32| {
                let mut pb = PathBuilder::new();
                let n = (48.0 * f1).ceil().max(1.0) as usize;
                for i in 0..=n {
                    let (px, py) = pt(f1 * i as f32 / n as f32);
                    if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
                }
                pb.finish()
            };
            if let Some(pa) = arc(1.0) {
                stroke(c, &pa, 1.5, WHITE.alpha(0.24), true);
            }
            rect(c, cx, hy, cw, 1.0, 0.0, WHITE.alpha(0.20));
            if day {
                let f = (nm - rm) as f32 / (sm - rm) as f32;
                if let Some(pa) = arc(f) {
                    stroke(c, &pa, 2.2, C_YELLOW, true);
                }
                let (px, py) = pt(f);
                circle(c, px, py, 10.0, C_YELLOW.alpha(0.16));
                circle(c, px, py, 6.5, C_YELLOW.alpha(0.30));
                circle(c, px, py, 4.0, Rgba::hex("#fff3c4"));
            } else {
                let night = (rm + 1440 - sm).max(1) as f32;
                let g = (if nm >= sm { nm - sm } else { nm + 1440 - sm }) as f32 / night;
                circle(c, ax1 - g * (ax1 - ax0), hy + 6.0 * (std::f32::consts::PI * g).sin(), 3.5, FG2.alpha(0.6));
            }
            let yy = hy + 12.0;
            let left = if day { crate::wxcard::dur(sm - nm) } else { crate::wxcard::dur(if nm < rm { rm - nm } else { rm + 1440 - nm }) };
            if moon_on {
                cell(c, t, 3, 0, yy, "Wschód", &crate::wxcard::hhmm(r));
                cell(c, t, 3, 1, yy, "Zachód", &crate::wxcard::hhmm(s_));
                cell(c, t, 3, 2, yy, if day { "Do zachodu" } else { "Do wschodu" }, &left);
                top = yy + lh_ + line_h(t, &vs) + 14.0;
                rect(c, cx, top - 7.0, cw, 1.0, 0.0, HAIR);
            } else {
                cell(c, t, 2, 0, yy, "Wschód", &crate::wxcard::hhmm(r));
                cell(c, t, 2, 1, yy, "Zachód", &crate::wxcard::hhmm(s_));
                let yy = yy + lh_ + line_h(t, &vs) + 10.0;
                cell(c, t, 2, 0, yy, if day { "Do zachodu" } else { "Do wschodu" }, &left);
                cell(c, t, 2, 1, yy, "Wysokość", &alt_txt(sun.alt));
                footer(c, t, cx, y + PAD + FOOT_Y, cw, "sun", &format!("dzień trwa {}", crate::wxcard::dur(sm - rm)), C_YELLOW);
            }
        }
        if moon_on {
            let dm = |v: Option<f64>| v.map(|v| crate::wxcard::day_mon((v / 1000.0) as i64)).unwrap_or("–".into());
            let pct = format!("{}%", (moon.fraction * 100.0).round());
            if sun_on {
                // one row: the disc, the phase and how much is lit, the next full moon
                let r = 22.0;
                crate::wxcard::moon_disc(c, cx + r, top + r + 2.0, r, moon.phase);
                let tx = cx + 2.0 * r + 12.0;
                text(c, t, tx, top + 2.0, crate::wxcard::phase_name(moon.phase), &st(14.0, 650.0), FG, true, Some(cx + cw - tx));
                text(c, t, tx, top + 22.0, &format!("{} · {}", pct, alt_txt(moon.alt)), &st(12.0, 550.0).tnum(), FG2, true, None);
                let full = format!("pełnia {}", dm(crate::astro::next_phase(nowms, 0.5)));
                footer(c, t, cx, y + PAD + FOOT_Y, cw, "moon", &full, Rgba::hex("#dce8f8"));
            } else {
                let r = 46.0;
                crate::wxcard::moon_disc(c, x + w / 2.0, top + r + 6.0, r, moon.phase);
                let ns_ = st(15.0, 650.0);
                let name = crate::i18n::tr(crate::wxcard::phase_name(moon.phase)).into_owned();
                let nw = width(t, &name, &ns_);
                text(c, t, x + (w - nw) / 2.0, top + 2.0 * r + 16.0, &name, &ns_, FG, true, None);
                let yy = top + 2.0 * r + 44.0;
                let (rise, set) = crate::astro::moon_times(nowms, lat, lon);
                let hm = |v: Option<f64>| v.map(|v| crate::data::hm((v / 1000.0) as i64)).unwrap_or("–".into());
                cell(c, t, 4, 0, yy, "Oświetlenie", &pct);
                cell(c, t, 4, 1, yy, "Wschód", &hm(rise));
                cell(c, t, 4, 2, yy, "Zachód", &hm(set));
                cell(c, t, 4, 3, yy, "Wysokość", &alt_txt(moon.alt));
                footer(c, t, cx, y + PAD + FOOT_Y, cw, "moon", &format!("nów {} · pełnia {}", dm(crate::astro::next_phase(nowms, 0.0)), dm(crate::astro::next_phase(nowms, 0.5))), Rgba::hex("#dce8f8"));
            }
        }
        self.finish(c, t, "astro", match view.as_str() { "sun" => "Słońce", "moon" => "Księżyc", _ => "Słońce i Księżyc" }, x, y, w);
    }

    // ---------------- Zdjęcie: a picture frame. A photo of the wallpaper folders, another one every day or hour,
    // or the file ~/.config/mist/photo.jpg (.jpeg, .png). Decoded once per pick (640 px), kept until the next
    pub(crate) fn photo_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let now = crate::data::now();
        let pick = self.cfg.s("photo", "pick");
        let slot = match pick.as_str() { "hourly" => now / 3600, "file" => 0, _ => (now + crate::data::local(now).tm_gmtoff) / 86400 };
        let key = format!("{pick}|{slot}");
        if self.photo.as_ref().map(|p| p.0 != key).unwrap_or(true) {
            let is_pic = |p: &str| { let l = p.to_lowercase(); l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") };
            let path = if pick == "file" {
                let dir = note_path().trim_end_matches("note.txt").to_string();
                ["photo.jpg", "photo.jpeg", "photo.png"].iter().map(|n| format!("{dir}{n}")).find(|p| std::path::Path::new(p).exists())
            } else {
                let mut all: Vec<String> = crate::paths::photo_dirs().iter().filter_map(|d| std::fs::read_dir(d).ok()).flat_map(|rd| rd.flatten().map(|e| e.path().to_string_lossy().into_owned())).filter(|p| is_pic(p)).collect();
                all.sort();
                if all.is_empty() { None } else { Some(all[(slot.rem_euclid(all.len() as i64)) as usize].clone()) }
            };
            let pm = path.as_ref().and_then(|p| crate::img::decode_file(p, 640));
            self.photo = Some((key, path.unwrap_or_default(), pm));
        }
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), None);
        let (path, has) = self.photo.as_ref().map(|p| (p.1.clone(), p.2.is_some())).unwrap_or_default();
        if has {
            let fill = self.cfg.s("photo", "fit") != "fit";
            if let Some(pm) = self.photo.as_ref().and_then(|p| p.2.as_ref()) {
                crate::img::draw(c, pm, x + 1.0, y + 1.0, w - 2.0, BENTO_H - 2.0, 25.0, fill, 1.0);
            }
            // the own file is always called "photo": nothing to caption
            if self.cfg.s("photo", "caption") == "show" && pick != "file" {
                let name = std::path::Path::new(&path).file_stem().map(|n| n.to_string_lossy().replace(['-', '_'], " ")).unwrap_or_default();
                if let Some(p) = crate::gfx::rrect_path(x + 1.0, y + BENTO_H - 80.0, w - 2.0, 79.0, 25.0) {
                    let k = Rgba(0.0, 0.0, 0.0, 1.0);
                    fill_lin(c, &p, (0.0, y + BENTO_H - 80.0), (0.0, y + BENTO_H), &[(0.0, k.alpha(0.0)), (1.0, k.alpha(0.55))]);
                }
                text(c, t, x + PAD, y + BENTO_H - PAD - 16.0, &name, &st(13.0, 600.0), FG, true, Some(w - 2.0 * PAD));
            }
        } else {
            c.icon(t, x + w / 2.0, y + BENTO_H / 2.0 - 18.0, "photo", 34.0, WHITE.alpha(0.35));
            let (a, b) = ("Brak zdjęcia", if pick == "file" { "~/.config/mist/photo.jpg" } else { "Dodaj zdjęcia do folderu tapet" });
            for (i, (s, sty, col)) in [(a, st(14.0, 600.0), MUTED), (b, st(11.5, 500.0), FAINT)].into_iter().enumerate() {
                let tw = width(t, s, &sty).min(w - 2.0 * PAD);
                text(c, t, x + (w - tw) / 2.0, y + BENTO_H / 2.0 + 10.0 + 20.0 * i as f32, s, &sty, col, true, Some(w - 2.0 * PAD));
            }
        }
        self.head(c, t, "photo", "", "", x, y, w);
        self.finish(c, t, "photo", "Zdjęcie", x, y, w);
    }

    // ---------------- Urządzenia: the batteries the bar already knows (laptop, phone, a Bluetooth device)
    pub(crate) fn devices_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let green = Rgba::rgb8(0x7d, 0xd8, 0x9a, 1.0);
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((green.alpha(0.14), 0.2, 0.25, 0.7)));
        self.head(c, t, "devices", "Urządzenia", "", x, y, w);
        let (bat, phone, bt_on, bt_dev) = self.devs.clone();
        // icon, name, level 0..1 when known, what to say instead of (or next to) it, charging
        let mut rows: Vec<(&str, &str, Option<f32>, String, bool)> = Vec::new();
        for k in self.cfg.list("devices", "show") {
            match k.as_str() {
                "laptop" => rows.push(match bat {
                    Some(b) => ("battery", "Komputer", Some(b.pct / 100.0), if b.full { "naładowany".into() } else if b.secs > 0 { crate::wxcard::dur((b.secs / 60) as i32) } else { String::new() }, b.charging),
                    None => ("plug", "Komputer", None, "zasilanie z sieci".into(), false),
                }),
                "phone" => rows.push(match &phone {
                    Some(p) if p.battery >= 0 => ("mobile", "Telefon", Some(p.battery as f32 / 100.0), String::new(), p.charging),
                    Some(_) => ("mobile", "Telefon", None, "połączony".into(), false),
                    None => ("mobileOff", "Telefon", None, "niepołączony".into(), false),
                }),
                "bt" => rows.push(match (bt_on, bt_dev) {
                    (false, _) => ("btOff", "Bluetooth", None, "wyłączony".into(), false),
                    (true, Some(Some(f))) => ("headphones", "Bluetooth", Some(f), String::new(), false),
                    (true, Some(None)) => ("btLink", "Bluetooth", None, "połączony".into(), false),
                    (true, None) => ("bluetooth", "Bluetooth", None, "brak urządzeń".into(), false),
                }),
                _ => {}
            }
        }
        let room = BENTO_H - 2.0 * PAD - 34.0;
        let step = (room / rows.len().max(1) as f32).min(76.0);
        let mut ry = cy + 34.0 + (room - step * rows.len() as f32) / 2.0;
        for (icon, name, lvl, note, charging) in &rows {
            c.icon(t, cx + 9.0, ry + 11.0, icon, 17.0, FG2);
            text(c, t, cx + 28.0, ry + 2.0, name, &st(13.0, 600.0), FG, true, None);
            let ns_ = st(11.0, 500.0);
            match lvl {
                Some(f) => {
                    let (vs, us) = (st(20.0, 600.0).tnum(), st(11.0, 500.0));
                    let v = format!("{}", (f * 100.0).round());
                    let (vw, uw) = (width(t, &v, &vs), width(t, "%", &us));
                    text(c, t, cx + cw - vw - uw - 2.0, ry - 4.0, &v, &vs, FG, true, None);
                    text(c, t, cx + cw - uw, ry + 4.0, "%", &us, MUTED, true, None);
                    if *charging {
                        c.icon(t, cx + cw - vw - uw - 14.0, ry + 10.0, "bolt", 13.0, C_YELLOW);
                    }
                    if !note.is_empty() {
                        text(c, t, cx + 28.0, ry + 20.0, note, &ns_, MUTED, true, None);
                    }
                    let by = ry + 42.0;
                    let col = if *charging { green } else if *f <= 0.15 { BAD } else if *f <= 0.30 { WARN } else { green };
                    rect(c, cx, by, cw, 5.0, 2.5, WHITE.alpha(0.12));
                    rect(c, cx, by, (cw * f.clamp(0.0, 1.0)).max(5.0), 5.0, 2.5, col.alpha(0.92));
                }
                None => {
                    let nw = width(t, note, &ns_);
                    text(c, t, cx + cw - nw, ry + 4.0, note, &ns_, MUTED, true, None);
                    rect(c, cx, ry + 42.0, cw, 1.0, 0.0, HAIR);
                }
            }
            ry += step;
        }
        self.finish(c, t, "devices", "Urządzenia", x, y, w);
    }

    // ---------------- Muzyka: what plays now, its cover, where it is, three buttons
    pub(crate) fn music_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let p = self.player.clone();
        let art_path = p.as_ref().map(|p| p.art.clone()).unwrap_or_default();
        if self.art.as_ref().map(|a| a.0 != art_path).unwrap_or(true) {
            let pm = if art_path.is_empty() { None } else { crate::img::decode_file(&art_path, 240) };
            let tint = pm.as_ref().map(mean).unwrap_or(ACC);
            self.art = Some((art_path.clone(), pm, tint));
        }
        let tint = self.art.as_ref().map(|a| a.2).unwrap_or(ACC);
        let style = self.cfg.s("music", "style");
        let show = self.cfg.list("music", "show");
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((tint.alpha(0.24), 0.2, 0.25, 0.75)));
        let src = if show.iter().any(|s| s == "source") { p.as_ref().map(|p| p.identity.clone()).unwrap_or_default() } else { String::new() };
        self.head(c, t, "music", "Muzyka", &src, x, y, w);
        let Some(p) = p.filter(|p| !p.title.is_empty()) else {
            c.icon(t, x + w / 2.0, y + BENTO_H / 2.0 - 18.0, "music", 34.0, WHITE.alpha(0.35));
            let s = st(14.0, 600.0);
            let tw = width(t, "Nic nie gra", &s);
            text(c, t, x + (w - tw) / 2.0, y + BENTO_H / 2.0 + 10.0, "Nic nie gra", &s, MUTED, true, None);
            self.finish(c, t, "music", "Muzyka", x, y, w);
            return;
        };
        // cover + titles
        let mut ty = cy + 26.0;
        let mut tx = cx;
        if style != "minimal" {
            let sz = 112.0;
            if let Some(pm) = self.art.as_ref().and_then(|a| a.1.as_ref()) {
                let mut sh = Canvas::new(w as u32, BENTO_H as u32);
                rect(&mut sh, cx - x, ty - y, sz, sz, 16.0, Rgba(0.0, 0.0, 0.0, 1.0));
                put_with_shadow(c, &sh.pm, x as i32, y as i32, 10.0, Rgba(0.0, 0.0, 0.0, 1.0), 0.35, 6);
                crate::img::draw(c, pm, cx, ty, sz, sz, 16.0, true, 1.0);
                border(c, cx, ty, sz, sz, 16.0, 1.0, WHITE.alpha(0.10));
            } else {
                rect(c, cx, ty, sz, sz, 16.0, tint.alpha(0.16));
                c.icon(t, cx + sz / 2.0, ty + sz / 2.0, "music", 36.0, tint.alpha(0.8));
            }
            tx = cx + sz + 14.0;
            ty += 4.0;
        }
        let tw = cx + cw - tx;
        let big = style == "minimal";
        let ts = st(if big { 22.0 } else { 16.0 }, 650.0);
        let lines = crate::gfx::wrap(t, &p.title, &ts, tw, if big { 2 } else { 3 });
        let lh = line_h(t, &ts) * 1.02;
        for l in &lines {
            text(c, t, tx, ty, l, &ts, FG, true, Some(tw));
            ty += lh;
        }
        let as_ = st(if big { 15.0 } else { 13.0 }, 550.0);
        text(c, t, tx, ty + 2.0, &p.artist, &as_, tint, true, Some(tw));
        ty += line_h(t, &as_) + 2.0;
        if !p.album.is_empty() && !big {
            text(c, t, tx, ty + 2.0, &p.album, &st(11.0, 500.0), MUTED, true, Some(tw));
        }
        // progress
        if show.iter().any(|s| s == "progress") && p.length > 0.0 {
            let py = cy + 150.0;
            let f = (p.pos() / p.length).clamp(0.0, 1.0) as f32;
            rect(c, cx, py, cw, 4.0, 2.0, WHITE.alpha(0.16));
            rect(c, cx, py, (cw * f).max(4.0), 4.0, 2.0, WHITE.alpha(0.85));
            let ss = st(11.0, 550.0).tnum();
            text(c, t, cx, py + 10.0, &mmss(p.pos()), &ss, MUTED, true, None);
            let rest = format!("-{}", mmss(p.length - p.pos()));
            let rw = width(t, &rest, &ss);
            text(c, t, cx + cw - rw, py + 10.0, &rest, &ss, MUTED, true, None);
        }
        // prev · play / pause · next
        if show.iter().any(|s| s == "controls") {
            let my = cy + 216.0;
            let mid = cx + cw / 2.0;
            let btns: [(&str, &'static str, f32, f32, bool); 3] = [
                ("prevF", "prev", mid - 64.0, 18.0, p.can_prev),
                (if p.playing { "pauseF" } else { "playF" }, "toggle", mid, 24.0, p.can_toggle || true),
                ("nextF", "next", mid + 64.0, 18.0, p.can_next),
            ];
            for (icon, cmd, bx, r, on) in btns {
                let hot = self.hovering(bx - r, my - r, 2.0 * r, 2.0 * r);
                if cmd == "toggle" {
                    circle(c, bx, my, r, WHITE.alpha(if hot { 0.95 } else { 0.86 }));
                    c.icon(t, bx + if p.playing { 0.0 } else { 1.5 }, my, icon, 20.0, Rgba(0.04, 0.06, 0.1, 0.95));
                } else {
                    if hot && on {
                        circle(c, bx, my, r, WHITE.alpha(0.12));
                    }
                    c.icon(t, bx, my, icon, 18.0, if on { FG } else { FG.alpha(0.3) });
                }
                if on {
                    self.hits.push((bx - r, my - r, 2.0 * r, 2.0 * r, Hit::Media(cmd)));
                }
            }
        }
        self.finish(c, t, "music", "Muzyka", x, y, w);
    }

    // ---------------- Kalendarz: this month (days with events dotted) or the next events
    pub(crate) fn calendar_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        let view = self.cfg.s("calendar", "view");
        let monday = self.cfg.s("calendar", "week") != "sun";
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((C_BLUE.alpha(0.16), 0.8, 0.15, 0.7)));
        let items: Vec<Value> = self.cal["items"].as_array().cloned().unwrap_or_default();
        let upcoming = crate::data::upcoming(&self.cal, 4, 30);
        let title = format!("{} {}", MONTHS[tm.tm_mon as usize], tm.tm_year + 1900);
        let today_n = items.iter().filter(|e| {
            let s = crate::data::local(e["start"].as_i64().unwrap_or(0));
            s.tm_yday == tm.tm_yday && s.tm_year == tm.tm_year && !e["holiday"].as_bool().unwrap_or(false)
        }).count();
        let right = match today_n { 0 => String::new(), 1 => "1 wydarzenie dziś".into(), n if n < 5 => format!("{n} wydarzenia dziś"), n => format!("{n} wydarzeń dziś") };
        self.head(c, t, "calendar", if view == "agenda" { "Najbliższe" } else { &title }, &right, x, y, w);
        if view == "agenda" {
            let mut ey = cy + 30.0;
            if upcoming.is_empty() {
                text(c, t, cx, ey, "Nic w planach", &st(16.0, 600.0), FG2, true, None);
                text(c, t, cx, ey + 24.0, "na najbliższe 30 dni", &st(12.0, 500.0), MUTED, true, None);
            }
            for e in upcoming.iter().take(4) {
                let s = e["start"].as_i64().unwrap_or(0);
                let soon = s - now < 86400 && s > now - 86400;
                rect(c, cx, ey + 2.0, 3.0, 40.0, 1.5, if soon { ACC } else { WHITE.alpha(0.25) });
                let day = crate::data::rel(s);
                let ds = st(11.0, 650.0).spaced(0.6);
                let dw = text(c, t, cx + 12.0, ey, &day.to_uppercase(), &ds, if soon { ACC } else { FAINT }, true, None);
                text(c, t, cx + 12.0 + dw + 8.0, ey, &crate::data::when(e), &st(11.0, 500.0).tnum(), MUTED, true, None);
                text(c, t, cx + 12.0, ey + 16.0, e["title"].as_str().unwrap_or(""), &st(15.0, 600.0), FG, true, Some(cw - 12.0));
                if let Some(wh) = e["where"].as_str().filter(|s| !s.is_empty()) {
                    let tw = width(t, e["title"].as_str().unwrap_or(""), &st(15.0, 600.0));
                    if tw < cw - 80.0 {
                        text(c, t, cx + 12.0 + tw + 8.0, ey + 19.0, wh, &st(11.0, 500.0), MUTED, true, Some(cw - tw - 24.0));
                    }
                }
                ey += 56.0;
            }
            self.finish(c, t, "calendar", "Kalendarz", x, y, w);
            return;
        }
        // the month grid
        let (yy, mm) = ((tm.tm_year + 1900) as i64, (tm.tm_mon + 1) as i64);
        let first_wd = (days(yy, mm, 1) + 4).rem_euclid(7) as usize; // 0 = Sunday
        let len = (days(if mm == 12 { yy + 1 } else { yy }, if mm == 12 { 1 } else { mm + 1 }, 1) - days(yy, mm, 1)) as usize;
        let lead = if monday { (first_wd + 6) % 7 } else { first_wd };
        let rows = (lead + len).div_ceil(7);
        let colw = cw / 7.0;
        let gy = cy + 30.0;
        let ls = st(11.0, 650.0);
        for i in 0..7 {
            let wd = if monday { (i + 1) % 7 } else { i };
            let l = crate::i18n::day_initial(wd);
            let lw = width(t, &l, &ls);
            text(c, t, (cx + i as f32 * colw + (colw - lw) / 2.0).round(), gy, &l, &ls, if wd == 0 || wd == 6 { FAINT.alpha(0.45) } else { FAINT }, true, None);
        }
        // days with events (this month, no holidays); holidays tinted
        let mut ev = [0u8; 32];
        for e in &items {
            let s = crate::data::local(e["start"].as_i64().unwrap_or(0));
            if s.tm_year == tm.tm_year && s.tm_mon == tm.tm_mon {
                let d = s.tm_mday as usize;
                ev[d] |= if e["holiday"].as_bool().unwrap_or(false) { 2 } else { 1 };
            }
        }
        let rh = if rows > 5 { 25.0 } else { 29.0 };
        let ns = st(13.0, 550.0).tnum();
        let nh = line_h(t, &ns);
        for d in 1..=len {
            let k = lead + d - 1;
            let (col, row) = (k % 7, k / 7);
            let wd = if monday { (col + 1) % 7 } else { col };
            let ccx = cx + col as f32 * colw + colw / 2.0;
            let ccy = gy + 26.0 + row as f32 * rh + 9.0;
            let today = d as i32 == tm.tm_mday;
            let past = (d as i32) < tm.tm_mday;
            let s = d.to_string();
            let sw = width(t, &s, &ns);
            if today {
                circle(c, ccx, ccy, 12.0, WHITE.alpha(0.92));
            }
            let col_ = if today { Rgba(0.04, 0.06, 0.1, 1.0) } else if ev[d] & 2 != 0 { WARN } else if wd == 0 || wd == 6 { FG2.alpha(0.6) } else if past { FG2.alpha(0.55) } else { FG };
            text(c, t, (ccx - sw / 2.0).round(), ccy - nh / 2.0, &s, &if today { st(13.0, 750.0).tnum() } else { ns.clone() }, col_, !today, None);
            if ev[d] & 1 != 0 {
                circle(c, ccx, ccy + 11.0, 1.8, if today { ACC } else { ACC.alpha(if past { 0.5 } else { 1.0 }) });
            }
        }
        if self.cfg.s("calendar", "next") != "hide" {
            let (icon, s, col) = match upcoming.first() {
                Some(e) => ("calEvent", format!("{} {} · {}", crate::data::rel(e["start"].as_i64().unwrap_or(0)), crate::data::when(e), e["title"].as_str().unwrap_or("")), ACC),
                None => ("calendar", "Brak wydarzeń w planie".into(), MUTED),
            };
            footer(c, t, cx, cy + FOOT_Y, cw, icon, &s, col);
        }
        self.finish(c, t, "calendar", "Kalendarz", x, y, w);
    }

    // ---------------- System: how busy is the machine
    pub(crate) fn system_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let (cpu, mu, mt, rx) = self.stats;
        let disk = statvfs("/");
        let show = self.cfg.list("system", "show");
        let style = self.cfg.s("system", "style");
        let ram = if mt > 0.0 { mu / mt } else { 0.0 };
        let lvl = |f: f32| if f >= 0.9 { BAD } else if f >= 0.7 { WARN } else { ACC };
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((lvl(cpu as f32 / 100.0).alpha(0.16), 0.2, 0.2, 0.7)));
        self.head(c, t, "system", "System", &format!("działa {}", uptime()), x, y, w);
        let gb = |b: f64| format!("{:.0}", b / 1e9);
        // (key, label, fraction, value, unit, detail)
        let mut rows: Vec<(&str, &str, f32, String, &str, String)> = Vec::new();
        for k in &show {
            match k.as_str() {
                "cpu" => rows.push(("cpu", "procesor", cpu as f32 / 100.0, cpu.to_string(), "%", String::new())),
                "ram" => rows.push(("ram", "pamięć", ram, format!("{:.1}", mu).replace('.', crate::i18n::dec_sep()), "GB", format!("z {:.0} GB", mt))),
                "disk" => {
                    if let Some((u, tot)) = disk {
                        rows.push(("drive", "dysk", (u / tot) as f32, gb(u), "GB", format!("z {} GB", gb(tot))));
                    }
                }
                "net" => rows.push(("down", "transfer", ((rx / 1048576.0) / 50.0).min(1.0) as f32, format!("{:.1}", rx / 1048576.0).replace('.', crate::i18n::dec_sep()), "MB/s", "pobieranie".into())),
                _ => {}
            }
        }
        if style == "bars" || rows.len() > 2 && style != "rings" {
            let mut ry = cy + 34.0;
            let step = ((FOOT_Y + 18.0 - 34.0) / rows.len().max(1) as f32).min(62.0);
            for (icon, lab, f, v, unit, det) in &rows {
                c.icon(t, cx + 8.0, ry + 10.0, icon, 15.0, FG2);
                text(c, t, cx + 24.0, ry + 2.0, lab, &st(12.0, 550.0), MUTED, true, None);
                let vs = st(17.0, 600.0).tnum();
                let us = st(11.0, 500.0);
                let label = if det.is_empty() { unit.to_string() } else { format!("{unit} {det}") };
                let uw = width(t, &label, &us);
                let vw = width(t, v, &vs);
                text(c, t, cx + cw - uw - vw - 4.0, ry - 2.0, v, &vs, FG, true, None);
                text(c, t, cx + cw - uw, ry + 3.0, &label, &us, MUTED, true, None);
                let by = ry + 26.0;
                rect(c, cx, by, cw, 5.0, 2.5, WHITE.alpha(0.12));
                let col = if *icon == "down" { C_BLUE } else { lvl(*f) };
                rect(c, cx, by, (cw * f.clamp(0.0, 1.0)).max(5.0), 5.0, 2.5, col.alpha(0.9));
                ry += step;
            }
            self.finish(c, t, "system", "System", x, y, w);
            return;
        }
        // rings: the first two big, the rest as one line each under them
        let n = rows.len().min(2);
        let rr = 44.0;
        for (i, (icon, lab, f, v, unit, _)) in rows.iter().take(2).enumerate() {
            let rcx = if n == 1 { cx + cw / 2.0 } else { cx + cw * (0.25 + 0.5 * i as f32) };
            let rcy = cy + 34.0 + rr + 6.0 + if rows.len() <= 2 { 34.0 } else { 0.0 };
            let col = if *icon == "down" { C_BLUE } else { lvl(*f) };
            c.arc(rcx, rcy, rr, 1.0, 8.0, WHITE.alpha(0.10));
            if *f > 0.005 {
                c.arc(rcx, rcy, rr, f.clamp(0.0, 1.0), 8.0, col);
            }
            let vs = st(24.0, 600.0).tnum();
            let us = st(11.0, 550.0);
            let vw = width(t, v, &vs);
            let uw = width(t, unit, &us);
            let vh = line_h(t, &vs);
            text(c, t, rcx - (vw + uw + 2.0) / 2.0, rcy - vh / 2.0 - 2.0, v, &vs, FG, true, None);
            text(c, t, rcx - (vw + uw + 2.0) / 2.0 + vw + 2.0, rcy - 2.0, unit, &us, MUTED, true, None);
            let ls = st(12.0, 550.0);
            let lw = width(t, lab, &ls);
            text(c, t, rcx - lw / 2.0, rcy + rr + 12.0, lab, &ls, MUTED, true, None);
        }
        let mut ly = cy + 34.0 + 2.0 * rr + 50.0;
        for (icon, lab, _, v, unit, det) in rows.iter().skip(2) {
            c.icon(t, cx + 8.0, ly + 9.0, icon, 14.0, FG2);
            text(c, t, cx + 24.0, ly, lab, &st(12.0, 550.0), MUTED, true, None);
            let s = format!("{v} {unit} {det}");
            let ss = st(12.0, 600.0).tnum();
            let sw = width(t, &s, &ss);
            text(c, t, cx + cw - sw, ly, &s, &ss, FG, true, None);
            ly += 24.0;
        }
        self.finish(c, t, "system", "System", x, y, w);
    }

    // ---------------- Zegary: the time here (analog or digital) and in up to three cities
    pub(crate) fn clocks_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        let here = tm.tm_gmtoff;
        let analog = self.cfg.s("clocks", "style") != "digital";
        let face = self.cfg.s("clocks", "face");
        let h12 = self.cfg.s("clocks", "h12") == "12";
        let acc = match self.cfg.s("clocks", "accent").as_str() { "warm" => Rgba::rgb8(0xf6, 0xd9, 0xb0, 1.0), "blue" => C_BLUE, "white" => FG, _ => ACC };
        Self::card_glass(c, x, y, w, BENTO_H, glass_tint(&self.cfg), Some((C_YELLOW.alpha(0.12), 0.25, 0.3, 0.7)));
        self.head(c, t, "clocks", "Zegary świata", "", x, y, w);
        let cities: Vec<_> = self.cfg.list("clocks", "cities").iter().filter_map(|k| CITIES.iter().find(|c| c.0 == k)).take(3).cloned().collect();
        let top = cy + 26.0;
        if analog {
            let (ox, oy, r) = (cx + 64.0, top + 66.0, 62.0);
            circle(c, ox, oy, r, WHITE.alpha(0.07));
            border(c, ox - r, oy - r, 2.0 * r, 2.0 * r, r, 1.0, WHITE.alpha(0.16));
            for i in 0..12 {
                let a = i as f32 / 12.0 * std::f32::consts::TAU;
                let (s, co) = (a.sin(), -a.cos());
                // Tarcza: every hour ticked; 12, 3, 6 and 9 as numbers; or the quarters alone
                if face == "minimal" && i % 3 != 0 {
                    continue;
                }
                if face == "numbers" && i % 3 == 0 {
                    let lab = if i == 0 { "12".to_string() } else { i.to_string() };
                    let ns = st(12.0, 650.0).tnum();
                    let (lw, lh) = (width(t, &lab, &ns), line_h(t, &ns));
                    text(c, t, ox + s * (r - 14.0) - lw / 2.0, oy + co * (r - 14.0) - lh / 2.0, &lab, &ns, FG.alpha(0.85), false, None);
                    continue;
                }
                let (r0, r1, wd) = if i % 3 == 0 { (r - 13.0, r - 5.0, 2.4) } else { (r - 9.0, r - 5.0, 1.4) };
                let mut pb = PathBuilder::new();
                pb.move_to(ox + s * r0, oy + co * r0);
                pb.line_to(ox + s * r1, oy + co * r1);
                if let Some(p) = pb.finish() {
                    stroke(c, &p, wd, WHITE.alpha(if i % 3 == 0 { 0.8 } else { 0.4 }), true);
                }
            }
            let mins = tm.tm_min as f32;
            let hours = (tm.tm_hour % 12) as f32 + mins / 60.0;
            for (a, len, wd, col) in [(hours / 12.0, r * 0.52, 5.0, FG), (mins / 60.0, r * 0.78, 3.0, FG), ] {
                let a = a * std::f32::consts::TAU;
                let mut pb = PathBuilder::new();
                pb.move_to(ox - a.sin() * 8.0, oy + a.cos() * 8.0);
                pb.line_to(ox + a.sin() * len, oy - a.cos() * len);
                if let Some(p) = pb.finish() {
                    stroke(c, &p, wd, col, true);
                }
            }
            circle(c, ox, oy, 4.5, acc);
            circle(c, ox, oy, 1.8, Rgba(0.04, 0.06, 0.1, 1.0));
            // the digital time + the day next to it
            let tx = ox + r + 18.0;
            let ts = st(34.0, 300.0).spaced(-1.0).tnum();
            text(c, t, tx, oy - 34.0, &clock_str(now, h12), &ts, FG, true, None);
            const DAYS: [&str; 7] = ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"];
            text(c, t, tx, oy + 8.0, DAYS[tm.tm_wday as usize], &st(13.0, 600.0), acc, true, None);
            text(c, t, tx, oy + 26.0, &format!("{} {}", tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]), &st(12.0, 500.0), MUTED, true, None);
        } else {
            let ts = st(52.0, 250.0).spaced(-2.0).tnum();
            text(c, t, cx, top - 4.0, &clock_str(now, h12), &ts, FG, true, None);
            text(c, t, cx, top + 62.0, "Tutaj", &st(13.0, 600.0), acc, true, None);
        }
        // the cities: name, the difference, the time (a moon at night)
        let mut ry = if analog { top + 146.0 } else { top + 98.0 };
        let rh = if analog { 31.0 } else { 40.0 };
        if cities.is_empty() {
            text(c, t, cx, ry + 6.0, "Dodaj miasta w ustawieniach karty", &st(12.0, 500.0), MUTED, true, Some(cw));
        }
        for (_, name, base, rule) in cities {
            let off = offset(base, rule, now);
            let lt = now + off;
            let (hh, mi) = ((lt.rem_euclid(86400)) / 3600, (lt.rem_euclid(3600)) / 60);
            let day_diff = (lt + 0).div_euclid(86400) - (now + here).div_euclid(86400);
            let dh = (off - here) as f32 / 3600.0;
            let diff = if dh == 0.0 { "ten sam czas".to_string() } else { format!("{}{} h", if dh > 0.0 { "+" } else { "−" }, (if dh.fract() == 0.0 { format!("{}", dh.abs() as i64) } else { format!("{:.1}", dh.abs()).replace('.', crate::i18n::dec_sep()) })) };
            let diff = match day_diff { 1 => format!("{diff} · jutro"), -1 => format!("{diff} · wczoraj"), _ => diff };
            rect(c, cx, ry - 6.0, cw, 1.0, 0.0, HAIR);
            let night = !(6..20).contains(&hh);
            c.icon(t, cx + 8.0, ry + rh / 2.0 - 6.0, if night { "moon" } else { "sun" }, 14.0, if night { C_BLUE } else { C_YELLOW });
            let ns_ = st(if analog { 13.0 } else { 15.0 }, 600.0);
            text(c, t, cx + 24.0, ry, name, &ns_, FG, true, Some(cw * 0.55));
            let nh = line_h(t, &ns_);
            text(c, t, cx + 24.0, ry + nh - 1.0, &diff, &st(11.0, 500.0), MUTED, true, None);
            let s = format!("{}:{mi:02}", if h12 { (hh + 11) % 12 + 1 } else { hh });
            let ss = st(if analog { 20.0 } else { 26.0 }, 400.0).tnum();
            let sw = width(t, &s, &ss);
            text(c, t, cx + cw - sw, ry + if analog { 0.0 } else { -2.0 }, &s, &ss, FG, true, None);
            ry += rh;
        }
        self.finish(c, t, "clocks", "Zegary", x, y, w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // the Czas card: every part is a fraction, the lengths are a calendar's, the day's part follows the clock
    #[test]
    fn parts_of_time_passed() {
        for t in [1_794_585_600i64, 1_704_067_200, 1_709_251_199, 1_735_689_599] {
            let (f, mlen, ylen) = passed(t);
            assert!(f.iter().all(|v| (0.0..1.0).contains(v)), "{t}: {f:?}");
            assert!((28..=31).contains(&mlen) && (365..=366).contains(&ylen), "{t}: {mlen} {ylen}");
            let tm = crate::data::local(t);
            assert!((f[0] - (tm.tm_hour * 3600 + tm.tm_min * 60 + tm.tm_sec) as f32 / 86400.0).abs() < 1e-6);
            assert!(f[3] >= tm.tm_yday as f32 / ylen as f32);
        }
        // every widget has a name, an icon of its own and options
        for (k, name) in WIDGETS {
            assert!(!name.is_empty() && !groups(k).is_empty(), "{k}");
        }
    }
    #[test]
    fn dst_rules() {
        // 2026-07-01 12:00 UTC: summer everywhere north, winter in Sydney
        let t = at(2026, 7, 1, 12);
        assert_eq!(offset(0, Dst::Eu, t), 3600);
        assert_eq!(offset(-5, Dst::Us, t), -4 * 3600);
        assert_eq!(offset(10, Dst::Au, t), 10 * 3600);
        // 2026-01-15: winter north, summer in Sydney
        let t = at(2026, 1, 15, 12);
        assert_eq!(offset(0, Dst::Eu, t), 0);
        assert_eq!(offset(-8, Dst::Us, t), -8 * 3600);
        assert_eq!(offset(10, Dst::Au, t), 11 * 3600);
        // EU switch 2026-03-29 01:00 UTC; US 2026-03-08 07:00 UTC (NY)
        assert_eq!(offset(0, Dst::Eu, at(2026, 3, 29, 1) - 1), 0);
        assert_eq!(offset(0, Dst::Eu, at(2026, 3, 29, 1)), 3600);
        assert_eq!(offset(-5, Dst::Us, at(2026, 3, 8, 7) - 1), -5 * 3600);
        assert_eq!(offset(-5, Dst::Us, at(2026, 3, 8, 7)), -4 * 3600);
        assert_eq!(sunday(2026, 10, 0), 25);
        assert_eq!(sunday(2026, 11, 1), 1);
    }
}
