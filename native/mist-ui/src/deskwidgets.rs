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
pub const CITIES: [(&str, &str, i32, Dst); 8] = [
    ("london", "Londyn", 0, Dst::Eu),
    ("newyork", "Nowy Jork", -5, Dst::Us),
    ("sf", "San Francisco", -8, Dst::Us),
    ("tokyo", "Tokio", 9, Dst::No),
    ("sydney", "Sydney", 10, Dst::Au),
    ("dubai", "Dubaj", 4, Dst::No),
    ("shanghai", "Szanghaj", 8, Dst::No),
    ("lisbon", "Lizbona", 0, Dst::Eu),
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
                "ram" => rows.push(("ram", "pamięć", ram, format!("{:.1}", mu).replace('.', ","), "GB", format!("z {:.0} GB", mt))),
                "disk" => {
                    if let Some((u, tot)) = disk {
                        rows.push(("drive", "dysk", (u / tot) as f32, gb(u), "GB", format!("z {} GB", gb(tot))));
                    }
                }
                "net" => rows.push(("down", "transfer", ((rx / 1048576.0) / 50.0).min(1.0) as f32, format!("{:.1}", rx / 1048576.0).replace('.', ","), "MB/s", "pobieranie".into())),
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
            circle(c, ox, oy, 4.5, ACC);
            circle(c, ox, oy, 1.8, Rgba(0.04, 0.06, 0.1, 1.0));
            // the digital time + the day next to it
            let tx = ox + r + 18.0;
            let ts = st(34.0, 300.0).spaced(-1.0).tnum();
            text(c, t, tx, oy - 34.0, &crate::data::hm(now), &ts, FG, true, None);
            const DAYS: [&str; 7] = ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"];
            text(c, t, tx, oy + 8.0, DAYS[tm.tm_wday as usize], &st(13.0, 600.0), ACC, true, None);
            text(c, t, tx, oy + 26.0, &format!("{} {}", tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]), &st(12.0, 500.0), MUTED, true, None);
        } else {
            let ts = st(52.0, 250.0).spaced(-2.0).tnum();
            text(c, t, cx, top - 4.0, &crate::data::hm(now), &ts, FG, true, None);
            text(c, t, cx, top + 62.0, "Tutaj", &st(13.0, 600.0), ACC, true, None);
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
            let diff = if dh == 0.0 { "ten sam czas".to_string() } else { format!("{}{} h", if dh > 0.0 { "+" } else { "−" }, (if dh.fract() == 0.0 { format!("{}", dh.abs() as i64) } else { format!("{:.1}", dh.abs()).replace('.', ",") })) };
            let diff = match day_diff { 1 => format!("{diff} · jutro"), -1 => format!("{diff} · wczoraj"), _ => diff };
            rect(c, cx, ry - 6.0, cw, 1.0, 0.0, HAIR);
            let night = !(6..20).contains(&hh);
            c.icon(t, cx + 8.0, ry + rh / 2.0 - 6.0, if night { "moon" } else { "sun" }, 14.0, if night { C_BLUE } else { C_YELLOW });
            let ns_ = st(if analog { 13.0 } else { 15.0 }, 600.0);
            text(c, t, cx + 24.0, ry, name, &ns_, FG, true, Some(cw * 0.55));
            let nh = line_h(t, &ns_);
            text(c, t, cx + 24.0, ry + nh - 1.0, &diff, &st(11.0, 500.0), MUTED, true, None);
            let s = format!("{hh}:{mi:02}");
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
