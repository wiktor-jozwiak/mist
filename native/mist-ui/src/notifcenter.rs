// macOS Notification Center (was Notifications.qml; Super+A, the bar bell): a sheet under the bar, the
// Control centre look. Header (Do Not Disturb, clear), compact date + agenda tile, then the history of
// Mist's own server (notif.rs, ~/.local/state/mist/notifications.json) grouped per app inside day sections
// (Dzisiaj / Wczoraj / Wcześniej). A group shows its newest notification; "+N" expands it. ✕ on hover hides
// a group until it posts again. KDE Connect phone notifications group by the phone app (their summary).
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const SW: f32 = 400.0; // sheet width

#[derive(Clone, PartialEq)]
enum Hit {
    Sheet,
    Clear,
    Dnd,
    Agenda,
    Group(String),
    Hide(String),
}

struct Group {
    app: String,
    phone: bool,
    items: Vec<Value>,
}

pub struct NotifCenter {
    entries: Vec<Value>,
    dnd: bool,
    open: String,
    scroll: f32,
    content_h: f32,
    h: Anim,
    ptr: (f32, f32),
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    pressed_outside: bool,
    hover_group: String,
}

fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for ch in s.chars() {
        match ch {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").trim().to_string()
}
fn is_phone(n: &Value) -> bool {
    n["app_name"].as_str() == Some("KDE Connect") && !n["summary"].as_str().unwrap_or("").is_empty() && n["desktop_entry"].as_str() == Some("org.kde.kdeconnect.daemon")
}
fn key_of(n: &Value) -> String {
    if is_phone(n) { n["summary"].as_str().unwrap_or("").to_string() } else { n["app_name"].as_str().filter(|s| !s.is_empty()).unwrap_or("System").to_string() }
}
fn title_of(n: &Value) -> String {
    if is_phone(n) { String::new() } else { clean(n["summary"].as_str().unwrap_or("")) }
}
fn ms(n: &Value) -> i64 {
    n["received_wall_ms"].as_i64().unwrap_or(0)
}
fn day_start(ms: i64) -> i64 {
    let tm = crate::data::local(ms / 1000);
    (crate::astro::local_ms(tm.tm_year + 1900, (tm.tm_mon + 1) as u32, tm.tm_mday as u32, 0)) as i64
}
fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
fn ago(t: i64) -> String {
    let s = (now_ms() - t) / 1000;
    if s < 60 {
        return "teraz".into();
    }
    if s < 3600 {
        return format!("{} min", s / 60);
    }
    if day_start(t) == day_start(now_ms()) {
        return crate::data::hm(t / 1000);
    }
    let tm = crate::data::local(t / 1000);
    if s < 7 * 86400 {
        return format!("{} {}", crate::data::DAYS_SHORT[tm.tm_wday as usize], crate::data::hm(t / 1000));
    }
    format!("{}.{:02}", tm.tm_mday, tm.tm_mon + 1)
}

impl NotifCenter {
    pub fn new() -> NotifCenter {
        let mut n = NotifCenter { entries: Vec::new(), dnd: false, open: String::new(), scroll: 0.0, content_h: 0.0, h: Anim::new(0.0), ptr: (-1.0, -1.0), hits: Vec::new(), pressed_outside: false, hover_group: String::new() };
        n.reload();
        n
    }
    pub fn reload(&mut self) {
        let v: Value = std::fs::read_to_string(format!("{}/.local/state/mist/notifications.json", std::env::var("HOME").unwrap_or_default())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        self.entries = v["entries"].as_array().cloned().unwrap_or_default();
        self.dnd = v["dnd"].as_bool().unwrap_or(false);
    }
    fn sections(&self, hidden: &std::collections::HashMap<String, i64>) -> Vec<(&'static str, Vec<Group>)> {
        let mut e = self.entries.clone();
        e.sort_by_key(|n| -ms(n));
        let mut groups: Vec<Group> = Vec::new();
        for n in e {
            let k = key_of(&n);
            if hidden.get(&k).map(|h| ms(&n) <= *h).unwrap_or(false) {
                continue;
            }
            match groups.iter_mut().find(|g| g.app == k) {
                Some(g) => g.items.push(n),
                None => groups.push(Group { app: k, phone: is_phone(&n), items: vec![n] }),
            }
        }
        let today = day_start(now_ms());
        let mut out: Vec<(&'static str, Vec<Group>)> = vec![("Dzisiaj", vec![]), ("Wczoraj", vec![]), ("Wcześniej", vec![])];
        for g in groups {
            let d = day_start(ms(&g.items[0]));
            let i = if d >= today { 0 } else if d >= today - 86400000 { 1 } else { 2 };
            out[i].1.push(g);
        }
        out.into_iter().filter(|s| !s.1.is_empty()).collect()
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1.clone())
    }
}

// the white lens tile (LiquidGlass tone white, no shadow)
fn tile(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, hovered: bool) {
    crate::glass::Glass { radius: 22.0, tone: Rgba(1.0, 1.0, 1.0, 1.0), tint: if hovered { 0.085 } else { 0.06 }, shadow: false, shine: 0.6, ..Default::default() }.paint(c, x, y, w, h);
}

impl Pane for NotifCenter {
    fn name(&self) -> &'static str {
        "notifications"
    }
    fn animating(&self) -> bool {
        self.h.running()
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(Hit::Clear | Hit::Dnd | Hit::Agenda | Hit::Hide(_))) || matches!(self.hit(), Some(Hit::Group(ref g)) if !g.is_empty())
    }
    fn data(&mut self, what: &str) -> bool {
        if what == "notifications" {
            self.reload();
            return true;
        }
        false
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hits.clear();
        let secs = self.sections(cx.notif_hidden);
        let total: usize = secs.iter().map(|s| s.1.iter().map(|g| g.items.len()).sum::<usize>()).sum();
        let (sx, sy) = (c.w() - 6.0 - 420.0 + 10.0, 40.0);
        let max_h = c.h() - 40.0 - 20.0;
        // content into its own layer (scrolled, clipped to the sheet)
        let cw = SW - 32.0;
        let mut l = Canvas::new(cw as u32, 4000);
        let mut lh_: Vec<((f32, f32, f32, f32), Hit)> = Vec::new();
        let lp = (self.ptr.0 - sx - 16.0, self.ptr.1 - sy - 16.0 + self.scroll);
        let t = &mut *cx.t;
        let mut y = 0.0;
        // ---- header
        let hs = Style::ui(17.0, 650.0);
        let _h171_0 = crate::desktop::line_h(t, &hs);
        let hw = crate::desktop::text(&mut l, t, 0.0, y + (34.0 - _h171_0) / 2.0, "Powiadomienia", &hs, T1, false, None);
        if total > 0 {
            let s = total.to_string();
            let st = Style::ui(11.0, 650.0).tnum();
            let bw = (t.width(&s, &st) + 12.0).max(20.0);
            crate::desktop::rect(&mut l, hw + 8.0, y + 7.0, bw, 20.0, 10.0, FILL2);
            let th = crate::desktop::line_h(t, &st);
            let _h178_0 = t.width(&s, &st);
            crate::desktop::text(&mut l, t, hw + 8.0 + (bw - _h178_0) / 2.0, y + 7.0 + (20.0 - th) / 2.0, &s, &st, T2, false, None);
        }
        let mut rx = cw;
        // DND orb
        rx -= 32.0;
        let oh = inside(lp, (rx, y + 1.0, 32.0, 32.0));
        crate::desktop::rect(&mut l, rx, y + 1.0, 32.0, 32.0, 16.0, if self.dnd { ACCENT } else if oh { Rgba(1.0, 1.0, 1.0, 0.18) } else { FILL2 });
        l.icon(t, rx + 16.0, y + 17.0, if self.dnd { "bellZ" } else { "bell" }, 15.0, if self.dnd { Rgba(1.0, 1.0, 1.0, 1.0) } else { T1 });
        lh_.push(((rx, y + 1.0, 32.0, 32.0), Hit::Dnd));
        if total > 0 {
            let ps = Style::ui(12.0, 600.0);
            let pw = 13.0 + 6.0 + t.width("Wyczyść", &ps) + 22.0;
            rx -= 6.0 + pw;
            let ph = inside(lp, (rx, y + 1.0, pw, 32.0));
            crate::desktop::rect(&mut l, rx, y + 1.0, pw, 32.0, 16.0, if ph { Rgba(1.0, 1.0, 1.0, 0.18) } else { FILL2 });
            l.icon(t, rx + 11.0 + 6.5, y + 17.0, "trash", 13.0, T2);
            let th = crate::desktop::line_h(t, &ps);
            crate::desktop::text(&mut l, t, rx + 11.0 + 19.0, y + 1.0 + (32.0 - th) / 2.0, "Wyczyść", &ps, T1, false, None);
            lh_.push(((rx, y + 1.0, pw, 32.0), Hit::Clear));
        }
        y += 34.0 + 10.0;
        // ---- date + agenda tile
        {
            let evs = crate::data::upcoming(cx.cal, 3, 2);
            let s125 = Style::ui(12.5, 600.0);
            let s11 = Style::ui(11.0, 450.0);
            let row_h = (crate::desktop::line_h(t, &s125) + crate::desktop::line_h(t, &s11)).max(30.0);
            let ag_h = if evs.is_empty() { row_h } else { evs.len() as f32 * row_h + (evs.len() as f32 - 1.0) * 8.0 };
            let th = (ag_h + 24.0).max(72.0);
            let hov = inside(lp, (0.0, y, cw, th));
            tile(&mut l, 0.0, y, cw, th, hov);
            lh_.push(((0.0, y, cw, th), Hit::Agenda));
            let tmn = crate::data::local(crate::data::now());
            let ds = crate::data::DAYS_SHORT[tmn.tm_wday as usize].to_uppercase();
            let d1 = Style::ui(11.0, 700.0).spaced(1.0);
            let d2 = Style::ui(28.0, 600.0);
            let bh = crate::desktop::line_h(t, &d1) + crate::desktop::line_h(t, &d2);
            let by = y + (th - bh) / 2.0;
            let w1 = t.width(&ds, &d1);
            crate::desktop::text(&mut l, t, 16.0 + 26.0 - w1 / 2.0, by, &ds, &d1, DANGER, false, None);
            let dn = tmn.tm_mday.to_string();
            let w2 = t.width(&dn, &d2);
            let _h220_0 = crate::desktop::line_h(t, &d1);
            crate::desktop::text(&mut l, t, 16.0 + 26.0 - w2 / 2.0, by + _h220_0, &dn, &d2, T1, false, None);
            let sepx = 16.0 + 52.0 + 14.0;
            crate::desktop::rect(&mut l, sepx, y + 14.0, 1.0, th - 28.0, 0.0, HAIR);
            let ax = sepx + 1.0 + 14.0;
            let aw = cw - ax - 14.0;
            let mut ay = y + (th - ag_h) / 2.0;
            if evs.is_empty() {
                let d = ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"][tmn.tm_wday as usize];
                let cap = { let mut ch = d.chars(); ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default() };
                crate::desktop::text(&mut l, t, ax, ay, &format!("{cap}, {} {}", tmn.tm_mday, crate::data::MONTHS_GEN[tmn.tm_mon as usize]), &s125, T1, false, Some(aw));
                let _h230_0 = crate::desktop::line_h(t, &s125);
                crate::desktop::text(&mut l, t, ax, ay + _h230_0, "Brak wydarzeń dziś i jutro", &s11, T2, false, Some(aw));
            }
            for ev in &evs {
                crate::desktop::rect(&mut l, ax, ay + (row_h - 30.0) / 2.0, 3.0, 30.0, 2.0, ACCENT);
                let title = ev["title"].as_str().or(ev["summary"].as_str()).unwrap_or("");
                crate::desktop::text(&mut l, t, ax + 12.0, ay, title, &s125, T1, false, Some(aw - 12.0));
                let sub = format!("{} · {}", crate::data::rel(ev["start"].as_i64().unwrap_or(0)), crate::data::when(ev));
                let _h237_0 = crate::desktop::line_h(t, &s125);
                crate::desktop::text(&mut l, t, ax + 12.0, ay + _h237_0, &sub, &s11, T2, false, Some(aw - 12.0));
                ay += row_h + 8.0;
            }
            y += th + 10.0;
        }
        // ---- empty
        if total == 0 {
            y += 26.0;
            l.icon(t, cw / 2.0, y + 15.0, if self.dnd { "bellZ" } else { "bell" }, 30.0, T4);
            y += 30.0 + 8.0;
            let s = if self.dnd { "Nie przeszkadzać jest włączone" } else { "Brak powiadomień" };
            let st = Style::ui(13.0, 450.0);
            let sw = t.width(s, &st);
            crate::desktop::text(&mut l, t, (cw - sw) / 2.0, y, s, &st, T3, false, None);
            y += crate::desktop::line_h(t, &st) + 22.0 + 10.0;
        }
        // ---- day sections
        for (title, groups) in &secs {
            let ls = Style::ui(11.0, 600.0).spaced(1.2);
            crate::desktop::text(&mut l, t, 4.0, y + 4.0, &title.to_uppercase(), &ls, T3, false, None);
            y += 4.0 + crate::desktop::line_h(t, &ls) + 8.0;
            for g in groups {
                let expanded = self.open == g.app;
                let more = g.items.len() as i32 - 1;
                let rows: Vec<&Value> = g.items.iter().take(if expanded { 6 } else { 1 }).collect();
                let ix = 12.0 + 34.0 + 11.0;
                let iw = cw - ix - 12.0;
                // measure the rows first
                let s13 = Style::ui(13.0, 620.0);
                let s125 = Style::ui(12.5, 450.0);
                let mut heights = Vec::new();
                for (i, n) in rows.iter().enumerate() {
                    let thumb = n["image"].as_str().filter(|s| !s.is_empty() && n["image_w"].as_i64().unwrap_or(0) > 128);
                    let tw = iw - if thumb.is_some() { 54.0 } else { 0.0 };
                    let title = title_of(n);
                    let body = clean(n["body"].as_str().unwrap_or(""));
                    let mut th = 0.0;
                    if !title.is_empty() {
                        th += crate::desktop::line_h(t, &s13) + 1.0;
                    }
                    if !body.is_empty() {
                        th += crate::gfx::wrap(t, &body, &s125, tw, if expanded { 5 } else { 3 }).len() as f32 * crate::desktop::line_h(t, &s125);
                    }
                    let h = th.max(if thumb.is_some() { 44.0 } else { 0.0 }) + if i > 0 { 18.0 } else { 4.0 };
                    heights.push(h);
                }
                let inner_h = 18.0 + heights.iter().sum::<f32>() + if more > 0 { 26.0 } else { 0.0 };
                let gh = inner_h + 24.0;
                let hov = inside(lp, (0.0, y, cw, gh));
                if hov {
                    self.hover_group = g.app.clone();
                }
                tile(&mut l, 0.0, y, cw, gh, hov);
                if more > 0 {
                    lh_.push(((0.0, y, cw, gh), Hit::Group(g.app.clone())));
                } else {
                    lh_.push(((0.0, y, cw, gh), Hit::Group(String::new())));
                }
                // avatar: small image / app icon / tinted letter
                let n0 = &g.items[0];
                let small = n0["image"].as_str().filter(|s| !s.is_empty() && n0["image_w"].as_i64().unwrap_or(999) <= 128).map(String::from);
                let src = if g.phone {
                    small.clone().unwrap_or_default()
                } else if let Some(s) = small {
                    s
                } else {
                    let i = n0["icon"].as_str().unwrap_or("");
                    if i.starts_with('/') || i.starts_with("file:") {
                        i.to_string()
                    } else if !i.is_empty() {
                        i.to_string()
                    } else {
                        let de = n0["desktop_entry"].as_str().unwrap_or("");
                        cx.apps.lookup(if de.is_empty() { &g.app } else { de }).map(|e| e.icon.clone()).unwrap_or_default()
                    }
                };
                let pm = if src.is_empty() { None } else { cx.imgs.get(&src, 68, cx.icons) };
                match pm {
                    Some(pm) => crate::img::draw(&mut l, &pm, 12.0, y + 12.0, 34.0, 34.0, 34.0 * 0.24, false, 1.0),
                    None => {
                        let pal = [ACCENT, Rgba::hex("#BF5AF2"), Rgba::hex("#FF9F0A"), Rgba::hex("#30D158"), Rgba::hex("#FF375F"), Rgba::hex("#64D2FF"), Rgba::hex("#5E5CE6")];
                        let h = g.app.chars().fold(7i32, |h, ch| h.wrapping_mul(31).wrapping_add(ch as i32));
                        let tint = pal[(h.unsigned_abs() as usize) % pal.len()];
                        crate::desktop::rect(&mut l, 12.0, y + 12.0, 34.0, 34.0, 34.0 * 0.28, tint.alpha(0.22));
                        crate::desktop::border(&mut l, 12.0, y + 12.0, 34.0, 34.0, 34.0 * 0.28, 1.0, tint.alpha(0.3));
                        let s = g.app.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or("?".into());
                        let st = Style::ui(34.0 * 0.44, 700.0);
                        let sw = t.width(&s, &st);
                        let sh = crate::desktop::line_h(t, &st);
                        crate::desktop::text(&mut l, t, 12.0 + 17.0 - sw / 2.0, y + 12.0 + 17.0 - sh / 2.0, &s, &st, tint, false, None);
                    }
                }
                // app line: name, count, time / ✕ on hover
                let iy = y + 11.0;
                let an = Style::ui(11.5, 600.0);
                let ah = crate::desktop::line_h(t, &an);
                let aw = crate::desktop::text(&mut l, t, ix, iy + (18.0 - ah) / 2.0, &g.app, &an, T2, false, Some(iw - 100.0));
                if more > 0 {
                    let s = g.items.len().to_string();
                    let st = Style::ui(10.0, 650.0);
                    let bw = t.width(&s, &st) + 10.0;
                    crate::desktop::rect(&mut l, ix + aw + 6.0, iy + 1.0, bw, 16.0, 8.0, FILL2);
                    let sh = crate::desktop::line_h(t, &st);
                    crate::desktop::text(&mut l, t, ix + aw + 6.0 + 5.0, iy + 1.0 + (16.0 - sh) / 2.0, &s, &st, T2, false, None);
                }
                if hov {
                    let xx = ix + iw - 22.0;
                    let xh = inside(lp, (xx, iy - 2.0, 22.0, 22.0));
                    crate::desktop::rect(&mut l, xx, iy - 2.0, 22.0, 22.0, 11.0, if xh { Rgba(1.0, 1.0, 1.0, 0.18) } else { FILL2 });
                    l.icon(t, xx + 11.0, iy + 9.0, "x", 12.0, T1);
                    lh_.push(((xx, iy - 2.0, 22.0, 22.0), Hit::Hide(g.app.clone())));
                } else {
                    let s = ago(ms(n0));
                    let st = Style::ui(11.0, 450.0);
                    let sw = t.width(&s, &st);
                    let _h353_0 = crate::desktop::line_h(t, &st);
                    crate::desktop::text(&mut l, t, ix + iw - sw, iy + (18.0 - _h353_0) / 2.0, &s, &st, T3, false, None);
                }
                let mut ry = iy + 18.0;
                for (i, n) in rows.iter().enumerate() {
                    let thumb = n["image"].as_str().filter(|s| !s.is_empty() && n["image_w"].as_i64().unwrap_or(0) > 128).map(String::from);
                    let tw = iw - if thumb.is_some() { 54.0 } else { 0.0 };
                    if i > 0 {
                        crate::desktop::rect(&mut l, ix, ry + 8.0, iw, 1.0, 0.0, HAIR);
                        let s = ago(ms(n));
                        let st = Style::ui(10.5, 450.0);
                        let sw = t.width(&s, &st);
                        crate::desktop::text(&mut l, t, ix + tw - sw, ry + 16.0 + 1.0, &s, &st, T3, false, None);
                    }
                    let mut ty = ry + if i > 0 { 16.0 } else { 3.0 };
                    let ty0 = ty;
                    let title = title_of(n);
                    if !title.is_empty() {
                        crate::desktop::text(&mut l, t, ix, ty, &title, &s13, T1, false, Some(tw - if i > 0 { 50.0 } else { 0.0 }));
                        ty += crate::desktop::line_h(t, &s13) + 1.0;
                    }
                    let body = clean(n["body"].as_str().unwrap_or(""));
                    if !body.is_empty() {
                        for line in crate::gfx::wrap(t, &body, &s125, tw, if expanded { 5 } else { 3 }) {
                            crate::desktop::text(&mut l, t, ix, ty, &line, &s125, if title.is_empty() { T1 } else { T2 }, false, None);
                            ty += crate::desktop::line_h(t, &s125);
                        }
                    }
                    if let Some(th) = thumb {
                        crate::desktop::rect(&mut l, ix + iw - 44.0, ty0, 44.0, 44.0, 9.0, FILL2);
                        if let Some(pm) = cx.imgs.get(&th, 88, cx.icons) {
                            crate::img::draw(&mut l, &pm, ix + iw - 44.0, ty0, 44.0, 44.0, 9.0, true, 1.0);
                        }
                    }
                    ry += heights[i];
                }
                if more > 0 {
                    let s = if expanded { "Pokaż mniej".to_string() } else { format!("+{more} {}", if more == 1 { "powiadomienie" } else if more < 5 { "powiadomienia" } else { "powiadomień" }) };
                    crate::desktop::text(&mut l, t, ix, ry + 8.0, &s, &Style::ui(11.5, 600.0), Rgba::hex("#409CFF"), false, None);
                }
                y += gh + 8.0;
            }
            y += 2.0;
        }
        self.content_h = y;
        // the sheet: height follows the content (220 ms), scrolls past the monitor
        let want = (y + 32.0).min(max_h);
        if (self.h.target() - want).abs() > 0.5 {
            if self.h.target() == 0.0 { self.h = Anim::new(want) } else { self.h.set(want, 220.0, Ease::OutCubic) }
        }
        let sh = self.h.value();
        self.scroll = self.scroll.clamp(0.0, (y - (sh - 32.0)).max(0.0));
        glass(c, sx, sy, SW, sh, 26.0, 0.82);
        self.hits.push(((sx, sy, SW, sh), Hit::Sheet));
        let clip_h = (sh - 32.0).max(0.0);
        if let Some(rect) = tiny_skia::Rect::from_xywh(sx + 16.0, sy + 16.0, cw, clip_h) {
            let mut mask = tiny_skia::Mask::new(c.pm.width(), c.pm.height()).unwrap();
            mask.fill_path(&tiny_skia::PathBuilder::from_rect(rect), tiny_skia::FillRule::Winding, false, tiny_skia::Transform::identity());
            c.pm.draw_pixmap((sx + 16.0) as i32, (sy + 16.0 - self.scroll) as i32, l.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), Some(&mask));
        }
        for (r, h) in lh_ {
            let ry = r.1 + sy + 16.0 - self.scroll;
            if ry + r.3 > sy + 16.0 && ry < sy + 16.0 + clip_h {
                self.hits.push(((r.0 + sx + 16.0, ry, r.2, r.3), h));
            }
        }
        glass_rim(c, sx, sy, SW, sh, 26.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, cx: &mut Ctx) -> Act {
        let before = (self.hit(), self.hover_group.clone());
        self.ptr = (x, y);
        match ev {
            Ptr::Motion | Ptr::Enter | Ptr::Leave => {
                if ev == Ptr::Leave {
                    self.ptr = (-1.0, -1.0);
                }
                // the hovered group (✕) is found while painting
                let g = match self.hit() { Some(Hit::Group(g)) | Some(Hit::Hide(g)) => g, _ => String::new() };
                let in_group = self.hits.iter().any(|h| matches!(h.1, Hit::Group(_)) && inside(self.ptr, h.0));
                if !in_group {
                    self.hover_group.clear();
                }
                let _ = g;
                if (self.hit(), self.hover_group.clone()) != before || in_group { Act::Redraw } else { Act::None }
            }
            Ptr::Press(_) => {
                self.pressed_outside = self.hit().is_none();
                Act::None
            }
            Ptr::Release(_) => match self.hit() {
                None if self.pressed_outside => Act::Close,
                Some(Hit::Dnd) => {
                    self.dnd = !self.dnd;
                    Act::Exec(vec![format!("{}/.local/bin/mist-ui-bin", std::env::var("HOME").unwrap_or_default()), "msg".into(), "notif".into(), "dnd".into(), self.dnd.to_string()])
                }
                Some(Hit::Clear) => {
                    self.entries.clear();
                    Act::Exec(vec![format!("{}/.local/bin/mist-ui-bin", std::env::var("HOME").unwrap_or_default()), "msg".into(), "notif".into(), "clear".into()])
                }
                Some(Hit::Agenda) => Act::Toggle("calendar"),
                Some(Hit::Hide(app)) => {
                    cx.notif_hidden.insert(app.clone(), now_ms());
                    if self.open == app {
                        self.open.clear();
                    }
                    Act::Redraw
                }
                Some(Hit::Group(app)) if !app.is_empty() => {
                    self.open = if self.open == app { String::new() } else { app };
                    Act::Redraw
                }
                _ => Act::None,
            },
            Ptr::Wheel(dy) => {
                self.scroll = (self.scroll + dy * 40.0).max(0.0);
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        if ev.keysym == Keysym::Escape {
            if !self.open.is_empty() {
                self.open.clear();
                return Act::Redraw;
            }
            return Act::Close;
        }
        Act::None
    }
}
