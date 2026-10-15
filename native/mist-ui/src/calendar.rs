// Calendar sheet under the island (was Calendar.qml): month grid (left) + day plan and upcoming (right
// inset panel). ←→↑↓ move the day, PgUp/PgDn the month, T today, Esc closes; click outside closes.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const MONTHS_NOM: [&str; 12] = ["Styczeń", "Luty", "Marzec", "Kwiecień", "Maj", "Czerwiec", "Lipiec", "Sierpień", "Wrzesień", "Październik", "Listopad", "Grudzień"];
const DAYS: [&str; 7] = ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"];
const INSET: Rgba = Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.26);
const INK: Rgba = Rgba(0x0e as f32 / 255.0, 0x14 as f32 / 255.0, 0x20 as f32 / 255.0, 1.0);
const ACCENT_SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);
const W: f32 = 810.0;
const H: f32 = 520.0;

// a local day: midnight unix time + its parts (mktime normalises overflowing days / months)
#[derive(Clone, Copy, PartialEq)]
pub struct Day {
    pub ts: i64,
    pub y: i32,
    pub m: i32, // 0..11
    pub d: i32,
    pub wd: i32, // 0 = Sunday
}
pub fn day(y: i32, m: i32, d: i32) -> Day {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = y - 1900;
    tm.tm_mon = m;
    tm.tm_mday = d;
    tm.tm_hour = 12; // DST-safe, then back to midnight
    tm.tm_isdst = -1;
    let t = unsafe { libc::mktime(&mut tm) };
    let mut m2: libc::tm = unsafe { std::mem::zeroed() };
    m2.tm_year = tm.tm_year;
    m2.tm_mon = tm.tm_mon;
    m2.tm_mday = tm.tm_mday;
    m2.tm_isdst = -1;
    let mid = unsafe { libc::mktime(&mut m2) };
    let _ = t;
    Day { ts: mid as i64, y: tm.tm_year + 1900, m: tm.tm_mon, d: tm.tm_mday, wd: tm.tm_wday }
}
pub fn today() -> Day {
    let t = crate::data::local(crate::data::now());
    day(t.tm_year + 1900, t.tm_mon, t.tm_mday)
}
pub fn events_on(cal: &Value, d: Day) -> Vec<Value> {
    let (s, e) = (d.ts, d.ts + 86400);
    cal["items"].as_array().map(|a| a.iter().filter(|x| x["start"].as_i64().unwrap_or(0) < e && x["end"].as_i64().unwrap_or(0) > s).cloned().collect()).unwrap_or_default()
}

pub struct Calendar {
    month: Day,
    sel: Day,
    ptr: (f32, f32),
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    pressed_outside: bool,
}
#[derive(Clone, PartialEq)]
enum Hit {
    Card,
    Prev,
    Next,
    Today,
    Cell(Day),
    New,
    Web,
}

impl Calendar {
    pub fn new() -> Calendar {
        let t = today();
        Calendar { month: day(t.y, t.m, 1), sel: t, ptr: (-1.0, -1.0), hits: Vec::new(), pressed_outside: false }
    }
    fn shift(&mut self, n: i32) {
        self.month = day(self.month.y, self.month.m + n, 1);
    }
    fn select(&mut self, d: Day) {
        self.sel = d;
        if d.m != self.month.m || d.y != self.month.y {
            self.month = day(d.y, d.m, 1);
        }
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1.clone())
    }
    fn hov(&self, r: (f32, f32, f32, f32)) -> bool {
        inside(self.ptr, r)
    }
}

fn label(c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, s: &str) {
    crate::desktop::text(c, t, x, y, &s.to_uppercase(), &Style::ui(11.0, 600.0).spaced(1.2), T3, false, None);
}
// a bar + title + when · where
fn event_row(c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, w: f32, ev: &Value, rel: bool) -> f32 {
    let col = if ev["holiday"].as_bool().unwrap_or(false) { DANGER } else { ACCENT };
    crate::desktop::rect(c, x, y, 3.0, 34.0, 1.5, col);
    let tx = x + 13.0;
    let tw = w - 13.0;
    crate::desktop::text(c, t, tx, y, ev["title"].as_str().unwrap_or(""), &Style::ui(13.0, 600.0), T1, false, Some(tw));
    let mut sub = String::new();
    if rel {
        sub += &crate::data::rel(ev["start"].as_i64().unwrap_or(0));
        sub += " · ";
    }
    sub += &crate::data::when(ev);
    if let Some(wh) = ev["where"].as_str().filter(|s| !s.is_empty()) {
        sub += " · ";
        sub += wh;
    }
    let l1 = lh(t, 13.0, 600.0);
    crate::desktop::text(c, t, tx, y + l1, &sub, &Style::ui(11.0, 450.0), T3, false, Some(tw));
    (l1 + lh(t, 11.0, 450.0)).max(34.0)
}

impl Pane for Calendar {
    fn name(&self) -> &'static str {
        "calendar"
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(Hit::Prev | Hit::Next | Hit::Today | Hit::Cell(_) | Hit::New | Hit::Web))
    }
    fn slow(&mut self) -> bool {
        false
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        let cal = cx.cal;
        c.clear();
        self.hits.clear();
        let x0 = ((c.w() - 830.0) / 2.0).round() + 10.0;
        let y0 = 34.0 + 6.0 + 10.0; // under the bar (ExclusionMode.Normal), window margin 6, sheet margin 10
        glass(c, x0, y0, W, H, 26.0, 0.8);
        self.hits.push(((x0, y0, W, H), Hit::Card));
        let now = today();
        // ---------------- month (left)
        let (lx, mut y) = (x0 + 30.0, y0 + 28.0);
        let mw = crate::desktop::text(c, t, lx, y, MONTHS_NOM[self.month.m as usize], &Style::ui(28.0, 600.0), T1, false, None);
        crate::desktop::text(c, t, lx + mw + 8.0, y, &self.month.y.to_string(), &Style::ui(28.0, 300.0), T3, false, None);
        let l28 = lh(t, 28.0, 600.0);
        let n_today = events_on(cal, now).iter().filter(|e| !e["holiday"].as_bool().unwrap_or(false)).count();
        let tmn = crate::data::local(crate::data::now());
        let long = format!("{}, {} {}", { let d = DAYS[tmn.tm_wday as usize]; let mut ch = d.chars(); ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default() }, tmn.tm_mday, crate::data::MONTHS_GEN[tmn.tm_mon as usize]);
        let sub = format!("{} · {}", long, if n_today > 0 { format!("{n_today} wydarzenia dziś") } else { "dziś wolne".into() });
        crate::desktop::text(c, t, lx, y + l28, &sub, &Style::ui(13.0, 450.0), T3, false, Some(300.0));
        let head_h = l28 + lh(t, 13.0, 450.0);
        // ‹ Dziś ›  (right end of the row, vertically centred)
        let by = y + (head_h - 34.0) / 2.0;
        let rx = lx + 430.0;
        let next_x = rx - 34.0;
        let dz = Style::ui(13.0, 600.0);
        let dw = t.width("Dziś", &dz);
        let dzx = next_x - 5.0 - dw;
        let prev_x = dzx - 5.0 - 34.0;
        c.icon(t, prev_x + 17.0, by + 17.0, "left", 16.0, T1);
        c.icon(t, next_x + 17.0, by + 17.0, "right", 16.0, T1);
        let dh = lh(t, 13.0, 600.0);
        crate::desktop::text(c, t, dzx, by + (34.0 - dh) / 2.0, "Dziś", &dz, T2, false, None);
        self.hits.push(((prev_x, by, 34.0, 34.0), Hit::Prev));
        self.hits.push(((next_x, by, 34.0, 34.0), Hit::Next));
        self.hits.push(((dzx - 8.0, by + (34.0 - dh) / 2.0 - 8.0, dw + 16.0, dh + 16.0), Hit::Today));
        y += head_h + 14.0;
        // weekday header
        let cw = 430.0 / 7.0;
        let lhh = lh(t, 11.0, 600.0);
        for (i, d) in ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"].iter().enumerate() {
            let s = d.to_uppercase();
            let st = Style::ui(11.0, 600.0).spaced(1.2);
            let sw = t.width(&s, &st);
            crate::desktop::text(c, t, lx + i as f32 * cw + (cw - sw) / 2.0, y, &s, &st, T3, false, None);
        }
        y += lhh + 6.0 + 2.0;
        // 6 weeks from the Monday on or before the 1st
        let off = (self.month.wd + 6) % 7;
        for i in 0..42 {
            let d = day(self.month.y, self.month.m, 1 - off + i);
            let (gx, gy) = (lx + (i % 7) as f32 * cw, y + (i / 7) as f32 * 52.0);
            let evs = events_on(cal, d);
            let other = d.m != self.month.m;
            let is_today = d == now;
            let sel = d == self.sel;
            let holiday = evs.iter().any(|e| e["holiday"].as_bool().unwrap_or(false));
            let busy = evs.iter().any(|e| !e["holiday"].as_bool().unwrap_or(false));
            let (ccx, ccy) = (gx + cw / 2.0, gy + 25.0 - 5.0);
            let hov = self.hov((gx, gy, cw, 50.0));
            let fill = if sel { Some(ACCENT) } else if hov { Some(FILL2) } else if busy { Some(FILL) } else { None };
            if let Some(f) = fill {
                crate::desktop::circle(c, ccx, ccy, 18.0, f);
            }
            if is_today && !sel {
                crate::desktop::border(c, ccx - 18.0, ccy - 18.0, 36.0, 36.0, 18.0, 1.5, ACCENT);
            }
            let col = if sel { INK } else if other { T4 } else if holiday { DANGER } else if d.wd == 0 || d.wd == 6 { T3 } else { T1 };
            let ns = Style::ui(14.0, if is_today || sel { 700.0 } else { 500.0 });
            let s = d.d.to_string();
            let sw = t.width(&s, &ns);
            let nh = crate::desktop::line_h(t, &ns);
            crate::desktop::text(c, t, (ccx - sw / 2.0).round(), (ccy - nh / 2.0).round(), &s, &ns, col, false, None);
            let n = evs.iter().filter(|e| !e["holiday"].as_bool().unwrap_or(false)).count().min(3);
            let dots_w = n as f32 * 4.0 + (n as f32 - 1.0).max(0.0) * 3.0;
            for k in 0..n {
                crate::desktop::circle(c, ccx - dots_w / 2.0 + k as f32 * 7.0 + 2.0, gy + 50.0 - 2.0 - 2.0, 2.0, if other { T4 } else { ACCENT });
            }
            self.hits.push(((gx, gy, cw, 50.0), Hit::Cell(d)));
        }
        // footer: + Nowe wydarzenie · world
        let fy = y0 + H - 22.0 - 36.0;
        crate::desktop::rect(c, lx, fy - 14.0, 430.0, 1.0, 0.0, HAIR);
        let ns = Style::ui(13.0, 600.0);
        let nw = 15.0 + 6.0 + t.width("Nowe wydarzenie", &ns);
        let pw = nw + 30.0;
        let nh = self.hov((lx, fy, pw, 36.0));
        crate::desktop::rect(c, lx, fy, pw, 36.0, 18.0, if nh { ACCENT_SOFT } else { FILL });
        c.icon(t, lx + 15.0 + 7.5, fy + 18.0, "plus", 15.0, ACCENT);
        let th = crate::desktop::line_h(t, &ns);
        crate::desktop::text(c, t, lx + 15.0 + 21.0, fy + (36.0 - th) / 2.0, "Nowe wydarzenie", &ns, T1, false, None);
        self.hits.push(((lx, fy, pw, 36.0), Hit::New));
        c.icon(t, lx + 430.0 - 17.0, fy + 18.0, "world", 16.0, T1);
        self.hits.push(((lx + 430.0 - 34.0, fy + 1.0, 34.0, 34.0), Hit::Web));

        // ---------------- day plan + upcoming (right)
        let (px, py) = (x0 + 490.0, y0 + 1.0);
        let (pw2, ph2) = (W - 491.0, H - 2.0);
        crate::desktop::rect(c, px, py, pw2, ph2, 25.0, INSET);
        crate::desktop::rect(c, px, py, 1.0, ph2, 0.0, HAIR);
        let (rx0, cw2) = (px + 26.0, pw2 - 52.0);
        let mut ry = py + 28.0;
        label(c, t, rx0, ry, DAYS[self.sel.wd as usize]);
        ry += lh(t, 11.0, 600.0) + 10.0;
        let d30 = Style::ui(30.0, 600.0);
        let w30 = crate::desktop::text(c, t, rx0, ry, &self.sel.d.to_string(), &d30, T1, false, None);
        let m18 = Style::ui(18.0, 500.0);
        let dy = crate::desktop::ascent(t, &d30) - crate::desktop::ascent(t, &m18);
        crate::desktop::text(c, t, rx0 + w30 + 8.0, ry + dy, crate::data::MONTHS_GEN[self.sel.m as usize], &m18, T2, false, None);
        ry += crate::desktop::line_h(t, &d30) + 10.0;
        let evs = events_on(cal, self.sel);
        for ev in &evs {
            ry += event_row(c, t, rx0, ry, cw2, ev, false) + 10.0;
        }
        if evs.is_empty() {
            crate::desktop::text(c, t, rx0, ry, "Nic nie zaplanowano", &Style::ui(13.0, 450.0), T3, false, None);
            ry += lh(t, 13.0, 450.0) + 10.0;
        }
        crate::desktop::rect(c, rx0, ry, cw2, 1.0, 0.0, HAIR);
        ry += 1.0 + 10.0;
        label(c, t, rx0, ry, "Nadchodzące");
        ry += lh(t, 11.0, 600.0) + 10.0;
        let up = crate::data::upcoming(cal, 6, 60);
        let mut prev: Option<Day> = None;
        for ev in &up {
            if ry > y0 + H - 40.0 {
                break;
            }
            let tm = crate::data::local(ev["start"].as_i64().unwrap_or(0));
            let d = day(tm.tm_year + 1900, tm.tm_mon, tm.tm_mday);
            let first = prev != Some(d);
            prev = Some(d);
            let rh = event_row(c, t, rx0 + 30.0 + 12.0, ry, cw2 - 42.0, ev, true);
            if first {
                let ds = crate::data::DAYS_SHORT[d.wd as usize].to_uppercase();
                let s10 = Style::ui(10.0, 600.0);
                let w1 = t.width(&ds, &s10);
                let col_h = lh(t, 10.0, 600.0) + lh(t, 18.0, 600.0);
                let cy = ry + (rh - col_h) / 2.0;
                crate::desktop::text(c, t, rx0 + 15.0 - w1 / 2.0, cy, &ds, &s10, T3, false, None);
                let s18 = Style::ui(18.0, 600.0);
                let dn = d.d.to_string();
                let w2 = t.width(&dn, &s18);
                let l10 = lh(t, 10.0, 600.0);
                crate::desktop::text(c, t, rx0 + 15.0 - w2 / 2.0, cy + l10, &dn, &s18, T1, false, None);
            }
            ry += rh + 10.0;
        }
        glass_rim(c, x0, y0, W, H, 26.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let before = self.hit();
        self.ptr = (x, y);
        match ev {
            Ptr::Motion | Ptr::Enter | Ptr::Leave => {
                if ev == Ptr::Leave {
                    self.ptr = (-1.0, -1.0);
                }
                if self.hit() != before { Act::Redraw } else { Act::None }
            }
            Ptr::Press(_) => {
                self.pressed_outside = self.hit().is_none();
                Act::None
            }
            Ptr::Release(_) => match self.hit() {
                None if self.pressed_outside => Act::Close,
                Some(Hit::Prev) => {
                    self.shift(-1);
                    Act::Redraw
                }
                Some(Hit::Next) => {
                    self.shift(1);
                    Act::Redraw
                }
                Some(Hit::Today) => {
                    let t = today();
                    self.month = day(t.y, t.m, 1);
                    self.sel = t;
                    Act::Redraw
                }
                Some(Hit::Cell(d)) => {
                    self.sel = d;
                    Act::Redraw
                }
                Some(Hit::New) => Act::Run(vec!["xdg-open".into(), "https://calendar.google.com/calendar/r/eventedit".into()]),
                Some(Hit::Web) => Act::Run(vec!["xdg-open".into(), format!("https://calendar.google.com/calendar/r/day/{}/{}/{}", self.sel.y, self.sel.m + 1, self.sel.d)]),
                _ => Act::None,
            },
            Ptr::Wheel(dy) => {
                self.shift(if dy > 0.0 { 1 } else { -1 });
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        let s = self.sel;
        match ev.keysym {
            Keysym::Escape => return Act::Close,
            Keysym::Left => self.select(day(s.y, s.m, s.d - 1)),
            Keysym::Right => self.select(day(s.y, s.m, s.d + 1)),
            Keysym::Up => self.select(day(s.y, s.m, s.d - 7)),
            Keysym::Down => self.select(day(s.y, s.m, s.d + 7)),
            Keysym::Page_Up => self.shift(-1),
            Keysym::Page_Down => self.shift(1),
            Keysym::t | Keysym::T => {
                let t = today();
                self.sel = t;
                self.month = day(t.y, t.m, 1);
            }
            _ => return Act::None,
        }
        Act::Redraw
    }
}
