// The weather card (Desktop.qml WeatherCard): a live sky you can scrub through time. mist-wall draws the
// sky under this surface; this draws the glass edge, the shades over the sky, the text, the timeline and
// the popovers. Hover a day: the timeline shows its 24 hours and the headline switches to it. Move along
// the timeline: the text follows the hour under the pointer (the sky stays live). Hover the sun or the moon
// for details. Leaving the card returns to "now" (2.5 s; 20 s after a click pinned a day / an hour).
use crate::anim::Ease;
use crate::desktop::*;
use crate::gfx::{Canvas, Rgba};
use crate::text::Text;
use serde_json::Value;
use std::time::Instant;
use tiny_skia::{Pixmap, PixmapPaint, Transform};

pub struct Wx {
    pub fx: Fx,
    pub hits: Hits,
    pub ptr: Option<(f32, f32)>, // card coords
    pub hovered: bool,
    pub sel_day: usize,
    pub pinned: bool,
    pub pin_hour: i32,
    pub ii: usize,
    pub editing: bool,
    pub last_pop: &'static str,
    pub away: Option<Instant>,
    pub turned: Instant,
    pub cursor: &'static str,
    last_n: usize,
    // the insight carousel's fade (only the footer changes, linearly in its opacity): two renders, pixels blended
    foot_fast: bool,
    foot_cache: Option<(Pixmap, Pixmap, usize, usize)>, // + the rows that differ (start, end)
    foot_o: Option<f32>,
}

const DAYS: [&str; 7] = ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"];
const MON3: [&str; 12] = ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"];

pub fn wmo(c: i64) -> &'static str {
    match c {
        0 => "Bezchmurnie",
        1 => "Przeważnie słonecznie",
        2 => "Częściowe zachmurzenie",
        3 => "Pochmurno",
        45 => "Mgła",
        48 => "Szadź",
        51 => "Lekka mżawka",
        53 => "Mżawka",
        55 => "Gęsta mżawka",
        56 | 57 => "Marznąca mżawka",
        61 => "Lekki deszcz",
        63 => "Deszcz",
        65 => "Ulewa",
        66 | 67 => "Marznący deszcz",
        71 => "Lekki śnieg",
        73 => "Śnieg",
        75 => "Intensywny śnieg",
        77 => "Ziarna śniegu",
        80 => "Przelotny deszcz",
        81 => "Przelotne opady",
        82 => "Ulewy",
        85 => "Przelotny śnieg",
        86 => "Śnieżyca",
        95 => "Burza",
        96 | 99 => "Burza z gradem",
        _ => "—",
    }
}
fn dir8(d: f64) -> &'static str {
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][((d / 45.0).round() as i64).rem_euclid(8) as usize]
}
fn dec1(x: f64) -> String {
    let v = (x * 10.0).round() / 10.0;
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }.replace('.', ",")
}
fn hhmm(s: &str) -> String {
    if s.is_empty() { "–".into() } else { s.strip_prefix('0').unwrap_or(s).to_string() }
}
fn mins(s: &str) -> i32 {
    let mut p = s.split(':');
    p.next().and_then(|x| x.trim().parse::<i32>().ok()).unwrap_or(0) * 60 + p.next().and_then(|x| x.trim().parse::<i32>().ok()).unwrap_or(0)
}
fn dur(m: i32) -> String {
    let m = m.max(0);
    format!("{} h {} min", m / 60, m % 60)
}
fn tm(ts: i64) -> libc::tm {
    crate::data::local(ts)
}
fn day_mon(ts: i64) -> String {
    let t = tm(ts);
    format!("{} {}", t.tm_mday, MON3[t.tm_mon as usize])
}
// local midnight of "YYYY-MM-DD" (+ hour)
fn date_ts(d: &str, hour: u32) -> i64 {
    let p: Vec<i32> = d.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() < 3 {
        return 0;
    }
    (crate::astro::local_ms(p[0], p[1] as u32, p[2] as u32, hour) / 1000.0) as i64
}
fn f(h: &Value, k: &str) -> f64 {
    h[k].as_f64().unwrap_or(0.0)
}

struct Ins {
    icon: String,
    c: Rgba,
    text: String,
    i0: i32,
    i1: i32,
    p: i32,
}

impl Wx {
    pub fn new() -> Wx {
        Wx { fx: Fx::new(), hits: Vec::new(), ptr: None, hovered: false, sel_day: 0, pinned: false, pin_hour: -1, ii: 0, editing: false, last_pop: "", away: None, turned: Instant::now(), cursor: "", last_n: 0, foot_fast: false, foot_cache: None, foot_o: None }
    }

    fn hours(cfg: &Cfg) -> usize {
        cfg.opt("weather", "hours").as_u64().unwrap_or(24) as usize
    }
    // the hours on the timeline: the next N from now, or the selected day 0-23 h
    fn range(&self, wx: &Value, cfg: &Cfg) -> Vec<Value> {
        let h = wx["hourly"].as_array().cloned().unwrap_or_default();
        if h.is_empty() {
            return h;
        }
        let now = (crate::data::now() / 60 * 60) as f64;
        if self.sel_day == 0 {
            let i = h.iter().position(|x| f(x, "t") + 3600.0 > now).unwrap_or(0);
            return h[i..(i + Self::hours(cfg) + 1).min(h.len())].to_vec();
        }
        let Some(d) = wx["days"].get(self.sel_day) else { return vec![] };
        let start = date_ts(d["date"].as_str().unwrap_or(""), 0) as f64;
        match h.iter().position(|x| f(x, "t") >= start) {
            Some(i) => h[i..(i + 24).min(h.len())].to_vec(),
            None => vec![],
        }
    }
    fn next24(wx: &Value) -> Vec<Value> {
        let h = wx["hourly"].as_array().cloned().unwrap_or_default();
        let now = (crate::data::now() / 60 * 60) as f64;
        let i = h.iter().position(|x| f(x, "t") + 3600.0 > now).unwrap_or(0);
        h[i.min(h.len())..(i + 24).min(h.len())].to_vec()
    }
    fn nowcast(wx: &Value) -> String {
        let q: Vec<f64> = wx["soon"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
        if q.is_empty() {
            return String::new();
        }
        if f(wx, "precip") > 0.0 {
            return match q.iter().position(|v| *v < 0.05) {
                None => "pada jeszcze 2 h+".into(),
                Some(0) => String::new(),
                Some(i) => format!("przestanie za {} min", i * 15),
            };
        }
        match q.iter().position(|v| *v >= 0.05) {
            None => String::new(),
            Some(0) => "zaraz zacznie padać".into(),
            Some(i) => format!("opad za {} min", i * 15),
        }
    }

    // a small stack of things worth knowing, most urgent first
    fn insights(wx: &Value, garmin: &Value, cfg: &Cfg) -> Vec<Ins> {
        let r = Self::next24(wx);
        let en = cfg.list("weather", "insights");
        let has = |k: &str| en.iter().any(|x| x == k);
        let mut out: Vec<Ins> = Vec::new();
        if r.is_empty() {
            return out;
        }
        let days = wx["days"].as_array().cloned().unwrap_or_default();
        let today = days.first();
        let wet = |h: &Value| f(h, "pr") >= 0.1 || f(h, "sn") > 0.0;
        let snowy = |h: &Value| f(h, "sn") > 0.0 || f(h, "temp") <= 0.5;
        let hr = |h: &Value| tm(f(h, "t") as i64).tm_hour;
        let hour_of = |h: &Value| format!("{}:00", hr(h));
        let nowcast = Self::nowcast(wx);
        if has("rain") {
            let (t, i0, i1, p);
            if f(wx, "precip") > 0.0 || wet(&r[0]) {
                let i = r.iter().position(|h| !wet(h));
                t = if nowcast.starts_with("przestanie") {
                    let mut c = nowcast.chars();
                    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                } else {
                    match i { None => "Opady przez całą dobę".into(), Some(i) => format!("Przestanie padać ok. {}", hour_of(&r[i])) }
                };
                i0 = 0;
                i1 = match i { None => r.len() as i32 - 1, Some(i) => i as i32 - 1 };
                p = 9;
            } else if let Some(i) = r.iter().position(wet) {
                let mut j = i;
                while j + 1 < r.len() && wet(&r[j + 1]) {
                    j += 1;
                }
                let mm: f64 = r[i..=j].iter().map(|h| f(h, "pr")).sum();
                let kind = |h: &Value| if snowy(h) { "Śnieg" } else { "Deszcz" };
                t = (if nowcast.starts_with("opad za") { format!("{} za {}", kind(&r[0]), &nowcast[9..]) } else { format!("{} {}–{}:00", kind(&r[i]), hr(&r[i]), (hr(&r[j]) + 1) % 24) }) + &format!(" · {} mm", dec1(mm));
                i0 = i as i32;
                i1 = j as i32;
                p = if i < 4 { 8 } else if i < 10 { 5 } else { 3 };
            } else {
                t = "Bez opadów przez 24 h".into();
                i0 = -1;
                i1 = -1;
                p = 1;
            }
            out.push(Ins { icon: "umbrella".into(), c: if p >= 5 { C_BLUE } else { ACC }, text: t, i0, i1, p });
        }
        if has("run") {
            let (mut best, mut bs) = (-1i32, 1e9);
            for (i, h) in r.iter().enumerate().take(15) {
                let hh = hr(h);
                if f(h, "day") == 0.0 && !h["day"].as_bool().unwrap_or(false) || !(6..=21).contains(&hh) {
                    continue;
                }
                let s = f(h, "pr") * 12.0 + f(h, "pop") / 8.0 + (f(h, "wind") - 12.0).max(0.0) / 2.0 + (f(h, "feels") - 10.0).abs() / 2.0 + i as f64 * 0.15;
                if s < bs {
                    bs = s;
                    best = i as i32;
                }
            }
            if best >= 0 {
                let h = &r[best as usize];
                let w = garmin["workout"].as_str().filter(|s| !s.is_empty());
                let dry = f(h, "pop") < 30.0 && f(h, "pr") <= 0.0;
                out.push(Ins {
                    icon: "run".into(),
                    c: ACC,
                    i0: best,
                    i1: best,
                    p: if w.is_some() { 6 } else { 4 },
                    text: format!("{} najlepiej ok. {} · {}°, {}, wiatr {}", if w.is_some() { "Trening" } else { "Bieg" }, hour_of(h), f(h, "temp").round(), if dry { "sucho".to_string() } else { format!("opad {}%", ns(&h["pop"])) }, ns(&h["wind"])),
                });
            }
        }
        if has("wear") {
            let fl = r.iter().take(4).map(|h| f(h, "feels")).fold(f64::MAX, f64::min);
            let rf = fl + 10.0;
            let coat = if fl <= -5.0 { "Ciepła kurtka, czapka, rękawiczki" } else if fl <= 3.0 { "Kurtka i czapka" } else if fl <= 10.0 { "Kurtka" } else if fl <= 16.0 { "Bluza lub lekka kurtka" } else if fl <= 22.0 { "Długi rękaw" } else { "Krótki rękaw" };
            let run = if rf >= 18.0 { "krótki rękaw" } else if rf >= 12.0 { "długi rękaw" } else if rf >= 5.0 { "długi rękaw + rękawiczki" } else { "warstwy, czapka, rękawiczki" };
            out.push(Ins { icon: "temp".into(), c: ACC, i0: 0, i1: 3, p: if fl <= 3.0 { 5 } else { 2 }, text: format!("{coat} · na bieg {run}") });
        }
        if has("wind") {
            let w: Vec<&Value> = r.iter().take(12).collect();
            let g = w.iter().map(|h| f(h, "gust")).fold(0.0, f64::max);
            if g >= 40.0 {
                let i = w.iter().position(|h| f(h, "gust") == g).unwrap_or(0);
                out.push(Ins {
                    icon: "wind".into(),
                    c: if g >= 60.0 { BAD } else { WARN },
                    i0: i as i32,
                    i1: i as i32,
                    p: if g >= 60.0 { 9 } else if g >= 50.0 { 6 } else { 3 },
                    text: format!("Porywy do {} km/h ok. {} · z {}", g as i64, hour_of(w[i]), dir8(f(w[i], "dir"))),
                });
            }
        }
        if has("light") {
            if let Some(td) = today {
                let t = tm(crate::data::now());
                let nm = t.tm_hour * 60 + t.tm_min;
                let rs = td["rise"].as_str().or(wx["sunrise"].as_str()).unwrap_or("");
                let ss = td["set"].as_str().or(wx["sunset"].as_str()).unwrap_or("");
                let (r0, s0) = (mins(rs), mins(ss));
                let t1 = days.get(1);
                let len0 = s0 - r0;
                let len1 = t1.map(|d| mins(d["set"].as_str().unwrap_or("")) - mins(d["rise"].as_str().unwrap_or(""))).unwrap_or(len0);
                let dl = len1 - len0;
                let tail = if dl != 0 { format!(" · jutro {} o {} min", if dl < 0 { "krótszy" } else { "dłuższy" }, dl.abs()) } else { String::new() };
                out.push(Ins {
                    icon: (if nm < s0 { "sunset" } else { "sunrise" }).into(),
                    c: C_YELLOW,
                    i0: -1,
                    i1: -1,
                    p: if nm < s0 && s0 - nm < 90 { 5 } else { 1 },
                    text: if nm < r0 {
                        format!("Wschód {} · za {}", hhmm(td["rise"].as_str().unwrap_or("")), dur(r0 - nm))
                    } else if nm < s0 {
                        format!("Zachód {} · światła jeszcze {}{tail}", hhmm(td["set"].as_str().unwrap_or("")), dur(s0 - nm))
                    } else {
                        format!("Wschód jutro {}{tail}", hhmm(t1.and_then(|d| d["rise"].as_str()).unwrap_or("")))
                    },
                });
            }
        }
        if has("tomorrow") {
            if let (Some(t1), Some(td)) = (days.get(1), today) {
                let dt = f(t1, "max") - f(td, "max");
                out.push(Ins {
                    icon: crate::data::wx_icon(t1["code"].as_i64().unwrap_or(3), 1).into(),
                    c: ACC,
                    i0: -1,
                    i1: -1,
                    p: if dt.abs() >= 5.0 { 4 } else { 1 },
                    text: format!("Jutro {}°/{}° · {}{}", ns(&t1["max"]), ns(&t1["min"]), wmo(t1["code"].as_i64().unwrap_or(3)).to_lowercase(), if dt <= -2.0 { format!(", {}° chłodniej", -dt) } else if dt >= 2.0 { format!(", {dt}° cieplej") } else { String::new() }),
                });
            }
        }
        out.sort_by(|a, b| b.p.cmp(&a.p));
        out
    }

    fn hover_hour(&self, n: usize) -> i32 {
        match self.tl_idx(n) {
            Some(i) => i as i32,
            None => self.pin_hour,
        }
    }
    // the hour under the pointer on the timeline
    fn tl_idx(&self, n: usize) -> Option<usize> {
        let (tx, ty, tw, th) = (PAD, PAD + BAND_Y + 28.0, WX_W - 2.0 * PAD, 78.0);
        let p = self.ptr?;
        if self.editing || n == 0 || p.0 < tx || p.0 >= tx + tw || p.1 < ty || p.1 >= ty + th {
            return None;
        }
        let step = if n > 1 { tw / (n - 1) as f32 } else { tw };
        Some((((p.0 - tx) / step).round() as i32).clamp(0, n as i32 - 1) as usize)
    }

    // the card's window: card at (CARD_PAD, CARD_PAD); returns nothing, fills self.hits (card coords)
    // the carousel's fade: the footer at .2 and at 1 rendered once, the frames between blended (a full paint costs
    // ~13 ms, a blend < 1 ms) and only over the rows that differ; the last frame is a full paint again.
    // Returns the rows changed (y, h) when the frame was a blend, None after a full paint
    pub fn paint_cached(&mut self, c: &mut Canvas, t: &mut Text, wx: &Value, garmin: &Value, cfg: &Cfg) -> Option<(i32, i32)> {
        if !self.foot_fast {
            self.foot_cache = None;
            self.paint(c, t, wx, garmin, cfg);
            return None;
        }
        let k = self.turned.elapsed().as_secs_f32() * 1000.0 / 220.0;
        if k >= 1.0 {
            self.foot_fast = false;
            self.foot_cache = None;
            self.paint(c, t, wx, garmin, cfg);
            return None;
        }
        let mut fresh = false;
        if self.foot_cache.as_ref().map(|f| f.0.width() != c.pm.width() || f.0.height() != c.pm.height()).unwrap_or(true) {
            self.foot_o = Some(0.2);
            self.paint(c, t, wx, garmin, cfg);
            let f0 = c.pm.clone();
            self.foot_o = Some(1.0);
            self.paint(c, t, wx, garmin, cfg);
            self.foot_o = None;
            let row = c.pm.width() as usize * 4;
            let same = |y: usize| f0.data()[y * row..(y + 1) * row] == c.pm.data()[y * row..(y + 1) * row];
            let n = c.pm.height() as usize;
            let r0 = (0..n).find(|&y| !same(y)).unwrap_or(n);
            let r1 = (r0..n).rev().find(|&y| !same(y)).map(|y| y + 1).unwrap_or(r0);
            self.foot_cache = Some((f0, c.pm.clone(), r0, r1));
            fresh = true;
        }
        let (f0, f1, r0, r1) = self.foot_cache.as_ref().unwrap();
        let kk = (k.max(0.0) * 256.0) as u32;
        let row = c.pm.width() as usize * 4;
        let span = r0 * row..r1 * row;
        for (d, (a, b)) in c.pm.data_mut()[span.clone()].iter_mut().zip(f0.data()[span.clone()].iter().zip(&f1.data()[span])) {
            *d = ((*a as u32 * (256 - kk) + *b as u32 * kk) >> 8) as u8;
        }
        // outside the rows the canvas already holds the frame (f0 = f1 there)
        if fresh {
            return None;
        }
        Some((*r0 as i32, (*r1 - *r0) as i32))
    }
    pub fn paint(&mut self, c: &mut Canvas, t: &mut Text, wx: &Value, garmin: &Value, cfg: &Cfg) {
        c.clear();
        self.hits.clear();
        let (ox, oy) = (CARD_PAD, CARD_PAD);
        let (w, h) = (WX_W, BENTO_H);
        // glass: shadow + body, then the sky's place cut out (mist-wall's sky shows through), rim on top
        crate::glass::Glass { radius: 26.0, tint: 0.34, rims: false, shadow_opacity: 0.34, ..Default::default() }.paint(c, ox, oy, w, h);
        if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + 1.0, w - 2.0, h - 2.0, 25.0) {
            let paint = tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, anti_alias: true, ..Default::default() };
            c.pm.fill_path(&p, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
        }
        // shades over the sky: behind the headline, over the land
        if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + 1.0, w - 2.0, h * 0.55, 25.0) {
            let s = Rgba(4.0 / 255.0, 8.0 / 255.0, 16.0 / 255.0, 1.0);
            fill_lin(c, &p, (0.0, oy + 1.0), (0.0, oy + 1.0 + h * 0.55), &[(0.0, s.alpha(0.26)), (0.6, s.alpha(0.08)), (1.0, s.alpha(0.0))]);
        }
        if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + h * 0.40, w - 2.0, h * 0.60 - 1.0, 25.0) {
            let s = Rgba(4.0 / 255.0, 8.0 / 255.0, 16.0 / 255.0, 1.0);
            fill_lin(c, &p, (0.0, oy + h * 0.40), (0.0, oy + h - 1.0), &[(0.0, s.alpha(0.0)), (0.35, s.alpha(0.28)), (1.0, s.alpha(0.58))]);
        }

        let days = wx["days"].as_array().cloned().unwrap_or_default();
        let range = self.range(wx, cfg);
        let n = range.len();
        self.last_n = n;
        let hover_hour = self.hover_hour(n);
        let hh = if hover_hour >= 0 && (hover_hour as usize) < n { Some(range[hover_hour as usize].clone()) } else { None };
        let sd = if self.sel_day > 0 { days.get(self.sel_day).cloned() } else { None };
        let live = hh.is_none() && sd.is_none();
        let code = hh.as_ref().map(|x| x["code"].as_i64().unwrap_or(3)).or(sd.as_ref().map(|x| x["code"].as_i64().unwrap_or(3))).unwrap_or(wx["code"].as_i64().unwrap_or(3));
        let ins = Self::insights(wx, garmin, cfg);
        let cur = if ins.is_empty() { None } else { Some(&ins[self.ii % ins.len()]) };
        let hl = if live && self.sel_day == 0 { cur.filter(|c| c.i0 >= 0).map(|c| (c.i0, c.i1)).unwrap_or((-1, -1)) } else { (-1, -1) };
        let now = crate::data::now();
        let label = if let Some(h) = &hh {
            let ts = f(h, "t") as i64;
            if self.sel_day == 0 && hover_hour == 0 {
                "teraz".to_string()
            } else {
                let (a, b) = (tm(ts), tm(now));
                let same = a.tm_yday == b.tm_yday && a.tm_year == b.tm_year;
                format!("{}, {}:00", if same { "dziś" } else if self.sel_day == 0 { "jutro" } else { DAYS[a.tm_wday as usize] }, a.tm_hour)
            }
        } else if let Some(d) = &sd {
            let ts = date_ts(d["date"].as_str().unwrap_or(""), 12);
            format!("{}, {}", DAYS[tm(ts).tm_wday as usize], day_mon(ts))
        } else {
            String::new()
        };
        // the footer: an hour, a day, or the insight carousel
        let summary: (String, Rgba, String) = if let Some(h) = &hh {
            let mut parts = Vec::new();
            if f(h, "pop") >= 20.0 || f(h, "pr") > 0.0 {
                parts.push(format!("opad {}%{}", f(h, "pop") as i64, if f(h, "pr") > 0.0 { format!(", {} mm", dec1(f(h, "pr"))) } else { String::new() }));
            }
            parts.push(format!("wiatr {}{} km/h", ns(&h["wind"]), if f(h, "gust") >= f(h, "wind") + 10.0 { format!("–{}", ns(&h["gust"])) } else { String::new() }));
            ((if f(h, "pop") >= 20.0 { "umbrella" } else { "wind" }).into(), ACC, parts.join(" · "))
        } else if let Some(d) = &sd {
            let wet = f(d, "pop") >= 30.0;
            ((if wet { "umbrella" } else { "wind" }).into(), ACC, format!("{}porywy do {} km/h", if wet { format!("Opad {}%, {} mm · ", ns(&d["pop"]), dec1(f(d, "rain"))) } else { "Bez większych opadów · ".into() }, ns(&d["gust"])))
        } else {
            cur.map(|c| (c.icon.clone(), c.c, c.text.clone())).unwrap_or((String::new(), ACC, String::new()))
        };

        // ---- the content layer (MultiEffect shadow: rgb(4, 8, 18) .6, blur .6, 1 px down)
        let mut l = Canvas::new(w as u32, h as u32);
        let hov = self.hovered;
        let ptr = self.ptr;
        let inr = |x: f32, y: f32, ww: f32, hh: f32| ptr.map(|p| p.0 >= x && p.0 < x + ww && p.1 >= y && p.1 < y + hh).unwrap_or(false);
        // headline
        let city = format!("{}{}", wx["city"].as_str().unwrap_or("Warszawa"), if wx["stale"].as_bool().unwrap_or(false) { format!(" · dane z {}", crate::data::hm(wx["at"].as_i64().unwrap_or(0))) } else { String::new() });
        let cw_ = caps(&mut l, t, PAD, PAD, &city, FG2, None);
        let cs = caps_style();
        let clh = line_h(t, &cs);
        let mut lh: Hits = Vec::new();
        edit_btn(&mut l, t, &mut self.fx, &mut lh, "weather", PAD + cw_ + 8.0, PAD + clh / 2.0 - 13.0, hov && !self.editing, inr(PAD + cw_ + 8.0, PAD + clh / 2.0 - 13.0, 26.0, 26.0), "weather");
        let hs = hero_style();
        let big = if let Some(h) = &hh { format!("{}°", f(h, "temp").round()) } else if let Some(d) = &sd { format!("{}°", ns(&d["max"])) } else if wx["temp"].is_number() { format!("{}°", ns(&wx["temp"])) } else { "–".into() };
        let bw = text(&mut l, t, PAD, PAD + HERO_Y, &big, &hs, FG, true, None);
        let base = PAD + HERO_Y + ascent(t, &hs);
        let mut cy = base - 20.0;
        let colx = PAD + bw + 10.0;
        if !label.is_empty() {
            let ls = st(11.0, 650.0).spaced(0.6);
            text(&mut l, t, colx, cy, &label.to_uppercase(), &ls, ACC, true, None);
            cy += line_h(t, &ls) + 2.0;
        }
        let ds = st(15.0, 650.0);
        let desc = if live { wx["desc"].as_str().unwrap_or("łączenie…").to_string() } else { wmo(code).to_string() };
        text(&mut l, t, colx, cy, &desc, &ds, FG, true, Some((w - bw - 300.0).max(10.0)));
        cy += line_h(t, &ds) + 2.0;
        let feels = if let Some(h) = &hh { format!("odczuwalna {}°", ns(&h["feels"])) } else if let Some(d) = &sd { format!("{}° / {}°", ns(&d["max"]), ns(&d["min"])) } else if wx["feels"].is_number() { format!("odczuwalna {}°", ns(&wx["feels"])) } else { String::new() };
        text(&mut l, t, colx, cy, &feels, &st(11.0, 500.0), FG2, true, None);

        // footer: the one thing worth knowing; click for the next one (dots = how many)
        let stack = live && ins.len() > 1;
        if !summary.2.is_empty() {
            let (fx_, fy, fw) = (PAD, PAD + FOOT_Y, w - 2.0 * PAD);
            let dots_w = if stack { ins.len() as f32 * 5.0 + (ins.len() as f32 - 1.0) * 5.0 } else { 0.0 };
            let o = self.foot_o.unwrap_or_else(|| self.fx.v("wx.foot", 1.0, 220.0, Ease::Linear));
            let sty = st(13.0, 600.0);
            let flh = line_h(t, &sty);
            let iw = Canvas::icon_w(t, &summary.0, 14.0).max(if summary.0.is_empty() { 0.0 } else { 14.0 });
            if !summary.0.is_empty() {
                l.icon(t, fx_ + iw / 2.0, fy + 12.0, &summary.0, 14.0, summary.1.alpha(summary.1.3 * o));
            }
            let tx = fx_ + if iw > 0.0 { iw + 8.0 } else { 0.0 };
            text(&mut l, t, tx, fy + (24.0 - flh) / 2.0, &summary.2, &sty, FG.alpha(FG.3 * o), true, Some(fw - dots_w - 34.0));
            if stack {
                let mut dx = fx_ + fw - dots_w;
                for i in 0..ins.len() {
                    circle(&mut l, dx + 2.5, fy + 12.0, 2.5, if i == self.ii % ins.len() { FG } else { WHITE.alpha(0.3) });
                    dx += 10.0;
                }
                lh.push((fx_, fy, fw, 24.0, Hit::Foot));
            }
        }

        // what the timeline draws (shown while the card is hovered)
        let metric = cfg.s("weather", "metric");
        {
            let o = self.fx.v("wx.chips", if hov && !self.editing { 1.0 } else { 0.0 }, 160.0, Ease::Linear);
            let opts = [("temp", "Temp."), ("rain", "Opady"), ("wind", "Wiatr")];
            let total: f32 = opts.iter().map(|(k, lab)| chip_w(t, lab, metric == *k)).sum::<f32>() + 12.0;
            let mut x = w - total - PAD;
            let y = PAD + BAND_Y - 2.0;
            for (k, lab) in opts {
                let cw = chip_w(t, lab, metric == k);
                if o > 0.0 {
                    chip(&mut l, t, x, y, lab, metric == k, inr(x, y, cw, 26.0), o);
                }
                if hov && !self.editing {
                    lh.push((x, y, cw, 26.0, Hit::Metric(k)));
                }
                x += cw + 6.0;
            }
        }

        // timeline: temperature / precipitation / wind; hover to scrub, click to pin an hour
        let (tx, ty, tw) = (PAD, PAD + BAND_Y + 28.0, w - 2.0 * PAD);
        if !self.editing {
            lh.push((tx, ty, tw, 78.0, Hit::Timeline));
        }
        let step = if n > 1 { tw / (n - 1) as f32 } else { tw };
        let every = ((n as f32 - 1.0) / 4.0).round().max(3.0) as usize;
        let val = |h: &Value| -> f64 { match metric.as_str() { "rain" => f(h, "pop"), "wind" => f(h, "wind"), _ => f(h, "temp") } };
        let (vmin, vmax) = if n == 0 {
            (0.0, 1.0)
        } else {
            match metric.as_str() {
                "rain" => (0.0, 100.0),
                "wind" => (0.0, range.iter().map(|h| if f(h, "gust") > 0.0 { f(h, "gust") } else { f(h, "wind") }).fold(30.0, f64::max)),
                _ => (range.iter().map(|h| f(h, "temp")).fold(f64::MAX, f64::min), range.iter().map(|h| f(h, "temp")).fold(f64::MIN, f64::max)),
            }
        };
        let ty_ = |v: f64| -> f32 { 16.0 + (1.0 - ((v - vmin) / (vmax - vmin).max(1.0)) as f32) * 30.0 };
        if n >= 2 {
            self.curve(&mut l, tx, ty, tw, &range, &metric, hl, &ty_);
        }
        // values every few hours, above the curve; hour labels
        let vs = st(11.0, 650.0);
        let hs2 = st(11.0, 550.0);
        for (i, hv) in range.iter().enumerate() {
            if i % every == 0 && i > 0 && hover_hour < 0 && (metric != "rain" || f(hv, "pop") >= 10.0) {
                let s = match metric.as_str() { "rain" => format!("{}%", f(hv, "pop") as i64), "wind" => ns(&hv["wind"]), _ => format!("{}°", f(hv, "temp").round()) };
                let sw = width(t, &s, &vs);
                let x = (i as f32 * step - sw / 2.0).clamp(0.0, tw - sw);
                text(&mut l, t, tx + x, ty + ty_(val(hv)) - 16.0, &s, &vs, FG, true, None);
            }
            if i % every == 0 && (hover_hour < 0 || (i as f32 - hover_hour as f32).abs() > every as f32 / 3.0) {
                let s = if i == 0 && self.sel_day == 0 { "teraz".to_string() } else { tm(f(hv, "t") as i64).tm_hour.to_string() };
                let sw = width(t, &s, &hs2);
                let x = (i as f32 * step - sw / 2.0).clamp(0.0, tw - sw);
                text(&mut l, t, tx + x, ty + 63.0, &s, &hs2, FG2, true, None);
            }
        }
        // scrubber (pinned: accent)
        if hover_hour >= 0 && (hover_hour as usize) < n {
            let pin = self.tl_idx(n).is_none();
            let sx = self.fx.v("wx.scrub", hover_hour as f32 * step, 90.0, Ease::Linear);
            let col = if pin { ACC } else { WHITE.alpha(0.45) };
            rect(&mut l, tx + sx - 0.5, ty + 4.0, 1.0, 56.0, 0.0, col);
            if let Some(h) = &hh {
                let dy = ty + ty_(val(h));
                circle(&mut l, tx + sx, dy, 4.5, Rgba(0.1, 0.14, 0.24, 0.8));
                circle(&mut l, tx + sx, dy, 2.5, if pin { ACC } else { FG });
                let s = format!("{}:00", tm(f(h, "t") as i64).tm_hour);
                let ss = st(11.0, 700.0);
                let sw = width(t, &s, &ss);
                text(&mut l, t, tx + sx - sw / 2.0, ty + 62.0, &s, &ss, if pin { ACC } else { FG }, true, None);
            }
        } else {
            self.fx.anims.remove("wx.scrub");
        }

        // the next four days, top right; hover a day to preview it, click to keep it
        if cfg.s("weather", "days") == "show" {
            let (rx, ry, rw, rh) = (w - 248.0 - 14.0, 12.0, 248.0, 84.0);
            for (i, d) in days.iter().skip(1).take(4).enumerate() {
                let day = i + 1;
                let sel = self.sel_day == day;
                let x = rx + i as f32 * rw / 4.0;
                let dh = inr(x, ry, rw / 4.0, rh) && !self.editing;
                let a = self.fx.v(&format!("wx.day{day}"), if sel { 0.16 } else if dh { 0.09 } else { 0.0 }, 160.0, Ease::Linear);
                if a > 0.0 {
                    rect(&mut l, x + 2.0, ry + 2.0, rw / 4.0 - 4.0, rh - 4.0, 16.0, WHITE.alpha(a));
                }
                if sel {
                    border(&mut l, x + 2.0, ry + 2.0, rw / 4.0 - 4.0, rh - 4.0, 16.0, 1.0, if self.pinned { ACC.alpha(0.8) } else { WHITE.alpha(0.14) });
                }
                let ns_ = st(11.0, 650.0).spaced(0.8);
                let name = d["name"].as_str().unwrap_or("").to_uppercase();
                let (l1, l3a, l3b) = (line_h(t, &ns_), st(13.0, 650.0), st(11.0, 500.0));
                let colh = l1 + 4.0 + 18.0 + 4.0 + line_h(t, &l3a);
                let ccx = x + rw / 8.0;
                let mut yy = ry + (rh - colh) / 2.0;
                let nw = width(t, &name, &ns_);
                text(&mut l, t, (ccx - nw / 2.0).round(), yy, &name, &ns_, if sel { FG } else { FG2 }, true, None);
                yy += l1 + 4.0;
                let ic = crate::data::wx_icon(d["code"].as_i64().unwrap_or(3), 1);
                let icol = match ic { "sun" | "cloudSun" => C_YELLOW, "rain" | "storm" => C_BLUE, _ => FG };
                l.icon(t, ccx, yy + 9.0, ic, 18.0, icol);
                yy += 18.0 + 4.0;
                let (mx, mn) = (format!("{}°", ns(&d["max"])), format!("{}°", ns(&d["min"])));
                let (w1, w2) = (width(t, &mx, &l3a), width(t, &mn, &l3b));
                let x0 = (ccx - (w1 + 3.0 + w2) / 2.0).round();
                text(&mut l, t, x0, yy, &mx, &l3a, FG, true, None);
                let dyb = ascent(t, &l3a) - ascent(t, &l3b);
                text(&mut l, t, x0 + w1 + 3.0, yy + dyb, &mn, &l3b, MUTED, true, None);
                if !self.editing {
                    lh.push((x, ry, rw / 4.0, rh, Hit::Day(day)));
                }
            }
        }
        // the content layer with its shadow
        put_with_shadow(c, &l.pm, ox as i32, oy as i32, 7.7, Rgba(4.0 / 255.0, 8.0 / 255.0, 18.0 / 255.0, 1.0), 0.6, 1);

        // the sun and the moon in the sky are hoverable: details pop up next to them
        let m = crate::sky::Model::new(wx, (now / 60 * 60) as f64 * 1000.0);
        let (sw_, sh_) = (w as f64 - 2.0, h as f64 - 2.0);
        let (sx, sy) = m.sun_s(sw_, sh_);
        let (mx, my) = m.moon_s(sw_, sh_);
        let sun_c = (1.0 + sx as f32 * sw_ as f32, 1.0 + sy as f32 * sh_ as f32);
        let moon_c = (1.0 + mx as f32 * sw_ as f32, 1.0 + my as f32 * sh_ as f32);
        let ok = |p: (f32, f32)| p.0 > 6.0 && p.0 < w - 6.0 && p.1 > 6.0 && p.1 < 140.0;
        let sun_vis = m.sun.alt > -1.5 && ok(sun_c);
        let moon_vis = m.moon.alt > -1.0 && ok(moon_c);
        let sun_hov = sun_vis && inr(sun_c.0 - 20.0, sun_c.1 - 20.0, 40.0, 40.0) && !self.editing;
        let moon_hov = moon_vis && inr(moon_c.0 - 18.0, moon_c.1 - 18.0, 36.0, 36.0) && !self.editing;
        let so = self.fx.v("wx.sunring", if sun_hov { 1.0 } else { 0.0 }, 150.0, Ease::Linear);
        if so > 0.0 && sun_vis {
            border(c, ox + sun_c.0 - 17.0, oy + sun_c.1 - 17.0, 34.0, 34.0, 17.0, 1.0, WHITE.alpha(0.55 * so));
        }
        let mo = self.fx.v("wx.moonring", if moon_hov { 1.0 } else { 0.0 }, 150.0, Ease::Linear);
        if mo > 0.0 && moon_vis {
            border(c, ox + moon_c.0 - 16.0, oy + moon_c.1 - 16.0, 32.0, 32.0, 16.0, 1.0, WHITE.alpha(0.55 * mo));
        }
        if sun_vis {
            lh.push((sun_c.0 - 20.0, sun_c.1 - 20.0, 40.0, 40.0, Hit::Sun));
        }
        if moon_vis {
            lh.push((moon_c.0 - 18.0, moon_c.1 - 18.0, 36.0, 36.0, Hit::Moon));
        }
        // popover: sun or moon details next to the body (stays while the pointer is on it)
        let pop_rect = self.hits_pop_rect();
        let on_pop = pop_rect.map(|r| inr(r.0, r.1, r.2, r.3)).unwrap_or(false);
        let kind: &'static str = if sun_hov { "sun" } else if moon_hov { "moon" } else if on_pop { self.last_pop } else { "" };
        if !kind.is_empty() {
            self.last_pop = kind;
        }
        let po = self.fx.v("wx.pop", if kind.is_empty() { 0.0 } else { 1.0 }, 160.0, Ease::Linear);
        let ps = self.fx.v("wx.pops", if kind.is_empty() { 0.96 } else { 1.0 }, 180.0, Ease::OutCubic);
        if po > 0.0 && !self.last_pop.is_empty() {
            let r = self.popover(c, t, wx, &m, ox, oy, if self.last_pop == "moon" { moon_c } else { sun_c }, po, ps);
            if !kind.is_empty() {
                lh.push((r.0, r.1, r.2, r.3, Hit::Pop));
            }
        }
        crate::desktop::Desk::card_rim(c, ox, oy, w, h);
        // the back of the card
        let mut sh: Hits = Vec::new();
        edit_sheet(c, t, &mut self.fx, &mut sh, cfg, "weather", "Pogoda", ox, oy, w, h, self.editing, ptr.map(|p| (p.0 + ox, p.1 + oy)));
        for r in sh {
            lh.push((r.0 - ox, r.1 - oy, r.2, r.3, r.4));
        }
        self.hits = lh;
    }

    fn hits_pop_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.hits.iter().find(|h| h.4 == Hit::Pop).map(|h| (h.0, h.1, h.2, h.3))
    }

    #[allow(clippy::too_many_arguments)]
    fn popover(&mut self, c: &mut Canvas, t: &mut Text, wx: &Value, m: &crate::sky::Model, ox: f32, oy: f32, at: (f32, f32), o: f32, s: f32) -> (f32, f32, f32, f32) {
        let now = crate::data::now();
        let nowms = (now / 60 * 60) as f64 * 1000.0;
        let (lat, lon) = (wx["lat"].as_f64().unwrap_or(52.2297), wx["lon"].as_f64().unwrap_or(21.0122));
        let days = wx["days"].as_array().cloned().unwrap_or_default();
        let moon = self.last_pop == "moon";
        let p = m.moon.phase;
        let title = if moon {
            if !(0.03..=0.97).contains(&p) { "Nów" } else if p < 0.22 { "Przybywający sierp" } else if p < 0.28 { "Pierwsza kwadra" } else if p < 0.47 { "Przybywający garb" } else if p < 0.53 { "Pełnia" } else if p < 0.72 { "Ubywający garb" } else if p < 0.78 { "Ostatnia kwadra" } else { "Ubywający sierp" }
        } else {
            "Słońce"
        };
        let mut lines: Vec<String> = Vec::new();
        if moon {
            let (rise, set) = crate::astro::moon_times(nowms, lat, lon);
            let hm = |x: Option<f64>| x.map(|v| crate::data::hm((v / 1000.0) as i64)).unwrap_or("–".into());
            let dm = |x: Option<f64>| x.map(|v| day_mon((v / 1000.0) as i64)).unwrap_or("–".into());
            lines.push(format!("oświetlenie {}% · dzień {}", (m.moon.fraction * 100.0).round(), dec1(p * 29.53)));
            lines.push(format!("wschód {} · zachód {}", hm(rise), hm(set)));
            lines.push(format!("nów {} · pełnia {}", dm(crate::astro::next_phase(nowms, 0.0)), dm(crate::astro::next_phase(nowms, 0.5))));
            lines.push(format!("{} · {} km", if m.moon.alt > 0.0 { format!("{}° nad horyzontem", m.moon.alt.round()) } else { "pod horyzontem".into() }, thousands(crate::astro::moon_distance(nowms))));
        } else if let Some(td) = days.first() {
            let r = td["rise"].as_str().or(wx["sunrise"].as_str()).unwrap_or("");
            let st_ = td["set"].as_str().or(wx["sunset"].as_str()).unwrap_or("");
            let len = mins(st_) - mins(r);
            lines.push(format!("wschód {} · zachód {}", hhmm(r), hhmm(st_)));
            lines.push(format!("dzień trwa {}", dur(len)));
            let tmn = tm(now);
            let nm = tmn.tm_hour * 60 + tmn.tm_min;
            lines.push(if m.sun.alt > 0.0 { format!("słońce {}° nad horyzontem", m.sun.alt.round()) } else { format!("słońce {}° pod horyzontem", (-m.sun.alt).round()) });
            lines.push(if nm < mins(r) {
                format!("do wschodu {}", dur(mins(r) - nm))
            } else if nm < mins(st_) {
                let g = mins(st_) - 60;
                format!("do zachodu {} · złota godzina od {}:{:02}", dur(mins(st_) - nm), g / 60, g % 60)
            } else {
                format!("wschód jutro {}", hhmm(days.get(1).and_then(|d| d["rise"].as_str()).unwrap_or("")))
            });
        }
        let ts = st(13.5, 650.0);
        let ls = st(11.5, 500.0);
        let mut cw = width(t, title, &ts);
        for l in &lines {
            cw = cw.max(width(t, l, &ls));
        }
        let ch = line_h(t, &ts) + lines.len() as f32 * (line_h(t, &ls) + 3.0);
        let disc = if moon { 46.0 + 12.0 } else { 0.0 };
        let rw_ = (cw + disc).min(336.0 - 24.0);
        let (pw, ph) = ((rw_ + 24.0).min(336.0), ch + 22.0);
        let px = (at.0 - pw / 2.0).clamp(8.0, WX_W - pw - 8.0);
        let py = if at.1 < 80.0 { at.1 + 24.0 } else { (at.1 - ph - 24.0).max(6.0) };
        let mut l = Canvas::new(pw.ceil() as u32, ph.ceil() as u32);
        rect(&mut l, 0.0, 0.0, pw, ph, 16.0, Rgba(10.0 / 255.0, 14.0 / 255.0, 26.0 / 255.0, 0.86));
        border(&mut l, 0.0, 0.0, pw, ph, 16.0, 1.0, WHITE.alpha(0.12));
        let mut x = 12.0;
        if moon {
            moon_disc(&mut l, x + 23.0, ph / 2.0, 23.0, p);
            x += 46.0 + 12.0;
        }
        let mut y = 11.0;
        text(&mut l, t, x, y, title, &ts, FG, true, Some(pw - x - 12.0));
        y += line_h(t, &ts) + 3.0;
        for li in &lines {
            text(&mut l, t, x, y, li, &ls, FG2, true, Some(pw - x - 12.0));
            y += line_h(t, &ls) + 3.0;
        }
        let tf = Transform::from_translate(ox + px + pw / 2.0, oy + py + ph / 2.0).pre_scale(s, s).pre_translate(-pw / 2.0, -ph / 2.0);
        c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &PixmapPaint { opacity: o, quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, tf, None);
        (px, py, pw, ph)
    }

    // timeline washes + the curve (Canvas in Desktop.qml)
    #[allow(clippy::too_many_arguments)]
    fn curve(&self, c: &mut Canvas, x0: f32, y0: f32, w: f32, r: &[Value], m: &str, hl: (i32, i32), ty: &dyn Fn(f64) -> f32) {
        let n = r.len();
        let st_ = w / (n - 1) as f32;
        let base = 60.0f32;
        // background washes softened over neighbouring hours: night, rain (temperature mode), the insight's hours
        let soft = |v: Vec<f32>| -> Vec<f32> {
            (0..n).map(|i| {
                let (mut s, mut wsum) = (0.0, 0.0);
                for j in -3i32..=3 {
                    let k = i as i32 + j;
                    if k < 0 || k >= n as i32 {
                        continue;
                    }
                    let q = (4 - j.abs()) as f32;
                    s += v[k as usize] * q;
                    wsum += q;
                }
                s / wsum
            }).collect()
        };
        let mut wash = Pixmap::new(w.ceil() as u32, base as u32 + 2).unwrap();
        let mut paint_wash = |v: Vec<f32>, rgb: (u8, u8, u8), a: f32| {
            let st: Vec<tiny_skia::GradientStop> = (0..n).map(|i| tiny_skia::GradientStop::new(i as f32 / (n - 1) as f32, tiny_skia::Color::from_rgba8(rgb.0, rgb.1, rgb.2, ((v[i] * a).clamp(0.0, 1.0) * 255.0) as u8))).collect();
            if let Some(sh) = tiny_skia::LinearGradient::new(tiny_skia::Point::from_xy(0.0, 0.0), tiny_skia::Point::from_xy(w, 0.0), st, tiny_skia::SpreadMode::Pad, Transform::identity()) {
                let paint = tiny_skia::Paint { shader: sh, ..Default::default() };
                if let Some(rc) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, base) {
                    wash.fill_rect(rc, &paint, Transform::identity(), None);
                }
            }
        };
        paint_wash(soft(r.iter().map(|h| if h["day"].as_f64().unwrap_or(if h["day"].as_bool().unwrap_or(false) { 1.0 } else { 0.0 }) != 0.0 { 0.0 } else { 1.0 }).collect()), (0, 0, 0), 0.08);
        if m == "temp" {
            paint_wash(soft(r.iter().map(|h| ((f(h, "pr") / 1.5).min(1.0)).max(if f(h, "pop") >= 40.0 { (f(h, "pop") / 100.0 - 0.4) * 0.5 } else { 0.0 }) as f32).collect()), (157, 189, 240), 0.22);
        }
        if hl.0 >= 0 {
            paint_wash(soft((0..n).map(|i| if i as i32 >= hl.0 && i as i32 <= hl.1 { 1.0 } else { 0.0 }).collect()), (138, 214, 208), 0.2);
        }
        // fade it all in from the ground up (destination-in)
        let ww = wash.width() as usize;
        let d = wash.data_mut();
        for yy in 0..(base as usize + 2) {
            let t = yy as f32 / base;
            let k = if t <= 0.55 { 0.35 * t / 0.55 } else { 0.35 + 0.65 * ((t - 0.55) / 0.45).min(1.0) };
            for xx in 0..ww {
                let i = (yy * ww + xx) * 4;
                for ch in 0..4 {
                    d[i + ch] = (d[i + ch] as f32 * k) as u8;
                }
            }
        }
        c.pm.draw_pixmap(x0 as i32, y0 as i32, wash.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        let curve_of = |vals: &[f64]| -> Option<tiny_skia::Path> {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x0, y0 + ty(vals[0]));
            for i in 1..n {
                let mx = x0 + (i as f32 - 0.5) * st_;
                pb.cubic_to(mx, y0 + ty(vals[i - 1]), mx, y0 + ty(vals[i]), x0 + i as f32 * st_, y0 + ty(vals[i]));
            }
            pb.finish()
        };
        match m {
            "rain" => {
                // bars = millimetres (2 mm fills the band), line = chance
                for (i, h) in r.iter().enumerate() {
                    let mm = f(h, "pr") + f(h, "sn");
                    let hgt = ((mm / 2.0).min(1.0) * 44.0) as f32;
                    if hgt < 0.5 {
                        continue;
                    }
                    let col = if f(h, "sn") > 0.0 { Rgba::rgb8(230, 240, 255, 0.85) } else { Rgba::rgb8(157, 189, 240, 0.85) };
                    rect(c, x0 + i as f32 * st_ - st_ * 0.32, y0 + base - hgt, (st_ * 0.64).max(2.0), hgt, 0.0, col);
                }
                if let Some(p) = curve_of(&r.iter().map(|h| f(h, "pop")).collect::<Vec<_>>()) {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(skia(Rgba::rgb8(157, 189, 240, 0.9)));
                    paint.anti_alias = true;
                    let s = tiny_skia::Stroke { width: 1.6, dash: tiny_skia::StrokeDash::new(vec![3.0 * 1.6, 3.0 * 1.6], 0.0), ..Default::default() };
                    c.pm.stroke_path(&p, &paint, &s, Transform::identity(), None);
                }
            }
            "wind" => {
                // band between wind and gusts, line = mean wind
                let gw = |h: &Value| if f(h, "gust") > 0.0 { f(h, "gust") } else { f(h, "wind") };
                let mut pb = tiny_skia::PathBuilder::new();
                pb.move_to(x0, y0 + ty(gw(&r[0])));
                for (i, h) in r.iter().enumerate().skip(1) {
                    pb.line_to(x0 + i as f32 * st_, y0 + ty(gw(h)));
                }
                for i in (0..n).rev() {
                    pb.line_to(x0 + i as f32 * st_, y0 + ty(f(&r[i], "wind")));
                }
                pb.close();
                if let Some(p) = pb.finish() {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(skia(Rgba::rgb8(220, 232, 248, 0.16)));
                    paint.anti_alias = true;
                    c.pm.fill_path(&p, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
                }
                if let Some(p) = curve_of(&r.iter().map(|h| f(h, "wind")).collect::<Vec<_>>()) {
                    stroke(c, &p, 2.2, Rgba::hex("#dce8f8"), true);
                }
            }
            _ => {
                if let Some(p) = curve_of(&r.iter().map(|h| f(h, "temp")).collect::<Vec<_>>()) {
                    let sh = tiny_skia::LinearGradient::new(tiny_skia::Point::from_xy(0.0, y0 + 12.0), tiny_skia::Point::from_xy(0.0, y0 + 46.0), stops(&[(0.0, Rgba::hex("#efb08a")), (1.0, Rgba::hex("#9dbdf0"))]), tiny_skia::SpreadMode::Pad, Transform::identity());
                    if let Some(sh) = sh {
                        let paint = tiny_skia::Paint { shader: sh, anti_alias: true, ..Default::default() };
                        let s = tiny_skia::Stroke { width: 2.2, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
                        c.pm.stroke_path(&p, &paint, &s, Transform::identity(), None);
                    }
                }
            }
        }
    }

    // what the pointer is over (a change = repaint)
    pub fn sig(&self) -> String {
        let h = self.ptr.and_then(|p| hit_at(&self.hits, p.0, p.1));
        format!("{:?}{}{:?}{}", h, self.hovered, self.tl_idx(self.last_n), self.sel_day)
    }

    // ---------------------------------------------------------------- input (card coords)
    pub fn motion(&mut self, p: Option<(f32, f32)>) {
        self.ptr = p;
        let inside = p.map(|p| p.0 >= 0.0 && p.0 < WX_W && p.1 >= 0.0 && p.1 < BENTO_H).unwrap_or(false);
        if inside != self.hovered {
            self.hovered = inside;
            self.away = if inside { None } else { Some(Instant::now()) };
        }
        self.cursor = match p.and_then(|p| hit_at(&self.hits, p.0, p.1)) {
            Some(Hit::Timeline) => "ew",
            Some(Hit::Sheet) | Some(Hit::Sun) | Some(Hit::Moon) | Some(Hit::Pop) | None => "",
            _ => "hand",
        };
        // hovering a day previews it (unless one is pinned)
        if let Some(Hit::Day(d)) = p.and_then(|p| hit_at(&self.hits, p.0, p.1)) {
            if !self.pinned && self.sel_day != d {
                self.sel_day = d;
                self.pin_hour = -1;
            }
        }
    }
    pub fn click(&mut self, cfg: &mut Cfg, wx: &Value) -> bool {
        let Some(p) = self.ptr else { return false };
        match hit_at(&self.hits, p.0, p.1) {
            Some(Hit::Edit(_)) => self.editing = true,
            Some(Hit::Done(_)) => self.editing = false,
            Some(Hit::Move(card, d)) => cfg.mv(card, d),
            Some(Hit::Opt(card, key, v)) => {
                if let Some(gr) = groups(card).into_iter().find(|g| g.key == key) {
                    cfg.click(card, &gr, &v);
                }
            }
            Some(Hit::Metric(k)) => cfg.set("weather", "metric", Value::String(k.into())),
            Some(Hit::Foot) => {
                self.ii += 1;
                self.fx.from("wx.foot", 0.2, 1.0, 220.0);
                self.turned = Instant::now();
            }
            Some(Hit::Timeline) => {
                let n = self.range(wx, cfg).len();
                if let Some(i) = self.tl_idx(n) {
                    self.pin_hour = if self.pin_hour == i as i32 { -1 } else { i as i32 };
                    self.pinned = self.pin_hour >= 0;
                }
            }
            Some(Hit::Day(d)) => {
                let p = self.pinned && self.sel_day == d;
                self.sel_day = if p { 0 } else { d };
                self.pinned = !p;
                self.pin_hour = -1;
            }
            _ => return false,
        }
        true
    }
    // wheel: walk through the days
    pub fn wheel(&mut self, down: bool, wx: &Value) -> bool {
        if self.editing {
            return false;
        }
        let nd = wx["days"].as_array().map(|a| a.len()).unwrap_or(0);
        let n = 4.min(nd.saturating_sub(1));
        let d = if down { (self.sel_day + 1).min(n) } else { self.sel_day.saturating_sub(1) };
        self.sel_day = d;
        self.pinned = d > 0;
        self.pin_hour = -1;
        true
    }
    // a day / hour picked: the way back to "now" is timed by the 250 ms tick (the carousel's 8 s is not)
    pub fn pending(&self) -> bool {
        self.hovered || self.sel_day != 0 || self.pin_hour >= 0
    }
    // every 250 ms (1 s at rest): back to "now" after leaving, the insight carousel; true = repaint
    pub fn tick(&mut self, n_ins: usize) -> bool {
        let mut ch = false;
        if let Some(a) = self.away {
            if (self.sel_day != 0 || self.pin_hour >= 0) && a.elapsed().as_millis() >= if self.pinned { 20000 } else { 2500 } {
                self.sel_day = 0;
                self.pin_hour = -1;
                self.pinned = false;
                ch = true;
            }
        }
        let live = self.sel_day == 0 && self.pin_hour < 0;
        if live && n_ins > 1 && !self.hovered && self.turned.elapsed().as_secs() >= 8 {
            self.ii = (self.ii + 1) % n_ins;
            self.fx.from("wx.foot", 0.2, 1.0, 220.0);
            self.turned = Instant::now();
            self.foot_fast = true;
            ch = true;
        }
        ch
    }
    pub fn n_insights(wx: &Value, garmin: &Value, cfg: &Cfg) -> usize {
        Self::insights(wx, garmin, cfg).len()
    }
}

// MoonDisc.qml (no halo): earthshine disc, lit part bounded by the terminator ellipse, a few maria
pub fn moon_disc(c: &mut Canvas, cx: f32, cy: f32, r: f32, phase: f64) {
    let lit = (1.0 - (2.0 * std::f64::consts::PI * phase).cos()) / 2.0;
    circle(c, cx, cy, r, Rgba(0.55, 0.62, 0.78, 0.22));
    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_circle(cx, cy, r);
    if let Some(p) = pb.finish() {
        stroke(c, &p, 0.8, Rgba(0.85, 0.9, 1.0, 0.18), false);
    }
    if lit < 0.005 {
        return;
    }
    let s = if phase < 0.5 { 1.0 } else { -1.0 };
    let rx = r * (2.0 * std::f64::consts::PI * phase).cos() as f32 * s;
    let k = 0.5523;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + s * r * k, cy - r, cx + s * r, cy - r * k, cx + s * r, cy);
    pb.cubic_to(cx + s * r, cy + r * k, cx + s * r * k, cy + r, cx, cy + r);
    pb.cubic_to(cx + rx * k, cy + r, cx + rx, cy + r * k, cx + rx, cy);
    pb.cubic_to(cx + rx, cy - r * k, cx + rx * k, cy - r, cx, cy - r);
    pb.close();
    let Some(lit_p) = pb.finish() else { return };
    let Some(sh) = tiny_skia::RadialGradient::new(tiny_skia::Point::from_xy(cx - r * 0.3, cy - r * 0.35), tiny_skia::Point::from_xy(cx, cy), r, stops(&[(0.0, Rgba::hex("#fbf6e9")), (1.0, Rgba::hex("#d9d2bf"))]), tiny_skia::SpreadMode::Pad, Transform::identity()) else { return };
    let paint = tiny_skia::Paint { shader: sh, anti_alias: true, ..Default::default() };
    c.pm.fill_path(&lit_p, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    // maria, clipped to the lit part
    if let Some(mask) = {
        let mut m = tiny_skia::Mask::new(c.pm.width(), c.pm.height());
        m.as_mut().map(|mm| {
            mm.fill_path(&lit_p, tiny_skia::FillRule::Winding, true, Transform::identity());
        });
        m
    } {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(skia(Rgba(0.45, 0.45, 0.42, 0.13)));
        paint.anti_alias = true;
        for (dx, dy, rr) in [(-0.28, -0.30, 0.30), (0.22, -0.30, 0.22), (0.32, 0.02, 0.24), (-0.52, 0.05, 0.30), (-0.12, 0.38, 0.18)] {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_circle(cx + dx * r, cy + dy * r, rr * r);
            if let Some(p) = pb.finish() {
                c.pm.fill_path(&p, &paint, tiny_skia::FillRule::Winding, Transform::identity(), Some(&mask));
            }
        }
    }
}
