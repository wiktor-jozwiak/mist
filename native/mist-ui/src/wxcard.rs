// The weather card (Desktop.qml WeatherCard): a live sky you can scrub through time. mist-wall draws the
// sky under this surface; this draws the glass edge, the shades over the sky, the text, the timeline and
// the popovers. Hover a day: the timeline shows its 24 hours and the headline switches to it. Move along
// the timeline: the text follows the hour under the pointer, and so does the sky: the sun and the moon travel to
// where they stand at that hour, the weather becomes that hour's. Hover the sun or the moon
// for details, or the two chips at the end of the numbers' row (sunrise / sunset, the Moon's light): they are the
// same switch where the sky hides the bodies behind cloud; a click keeps the details open. Leaving the card returns to "now" (2.5 s; 20 s after a click pinned a day / an hour).
// The row under the headline is both the moment's numbers (rain, wind, humidity, UV) and the timeline's
// switch: click or wheel over it for temperature / rain / wind. The big number cycles what the card leads with.
// Cost: the glass and its shades are painted once and copied after that (bg), the text's shadow is blurred at
// half resolution (soft_shadow): a hover repaint is ~4 ms instead of ~11 ms.
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
    pub intent: Intent,
    pub sheet: (f32, f32),
    // the glass body with the sky's place cut out and the shades over it: the same until the tint changes
    bg: Option<(u32, Pixmap)>,
    // the content layer and the shadow's buffers, kept between paints
    layer: Canvas,
    blur: (Vec<u32>, Vec<u32>),
    n_ins: usize,
    // the sun's / moon's details kept open by a click ("" = none) and where the popover hangs
    pop_pin: &'static str,
    alayer: Canvas,
    // the moment the card shows when it is not now (unix s; 0 = now): the hour under the pointer or pinned, noon of
    // a picked day. mist-wall's sky follows it (sky.json), so do the hoverable sun and moon here
    pub sky_t: i64,
}

const HEROES: [&str; 4] = ["temp", "feels", "rain", "wind"];
const METRICS: [&str; 3] = ["temp", "rain", "wind"];
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
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }.replace('.', crate::i18n::dec_sep())
}
// the Moon's phase (0 new .. 0.5 full .. 1) by name
pub fn phase_name(p: f64) -> &'static str {
    if !(0.03..=0.97).contains(&p) { "Nów" } else if p < 0.22 { "Przybywający sierp" } else if p < 0.28 { "Pierwsza kwadra" } else if p < 0.47 { "Przybywający garb" } else if p < 0.53 { "Pełnia" } else if p < 0.72 { "Ubywający garb" } else if p < 0.78 { "Ostatnia kwadra" } else { "Ubywający sierp" }
}
pub fn hhmm(s: &str) -> String {
    if s.is_empty() { "–".into() } else { s.strip_prefix('0').unwrap_or(s).to_string() }
}
pub fn mins(s: &str) -> i32 {
    let mut p = s.split(':');
    p.next().and_then(|x| x.trim().parse::<i32>().ok()).unwrap_or(0) * 60 + p.next().and_then(|x| x.trim().parse::<i32>().ok()).unwrap_or(0)
}
pub fn dur(m: i32) -> String {
    let m = m.max(0);
    format!("{} h {} min", m / 60, m % 60)
}
fn tm(ts: i64) -> libc::tm {
    crate::data::local(ts)
}
pub fn day_mon(ts: i64) -> String {
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
        Wx { fx: Fx::new(), hits: Vec::new(), ptr: None, hovered: false, sel_day: 0, pinned: false, pin_hour: -1, ii: 0, editing: false, last_pop: "", away: None, turned: Instant::now(), cursor: "", last_n: 0, foot_fast: false, foot_cache: None, foot_o: None, intent: Intent::new(), sheet: (0.0, 0.0), bg: None, layer: Canvas::new(1, 1), blur: (Vec::new(), Vec::new()), n_ins: 0, pop_pin: "", alayer: Canvas::new(1, 1), sky_t: 0 }
    }

    fn hours(cfg: &Cfg) -> usize {
        cfg.opt("weather", "hours").as_u64().unwrap_or(24) as usize
    }
    // the hours on the timeline: the next N from now, or the selected day 0-23 h (a span of wx.hourly)
    fn range<'a>(&self, wx: &'a Value, cfg: &Cfg) -> &'a [Value] {
        let h: &[Value] = wx["hourly"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        if h.is_empty() {
            return h;
        }
        let now = (crate::data::now() / 60 * 60) as f64;
        if self.sel_day == 0 {
            let i = h.iter().position(|x| f(x, "t") + 3600.0 > now).unwrap_or(0);
            return &h[i..(i + Self::hours(cfg) + 1).min(h.len())];
        }
        let Some(d) = wx["days"].get(self.sel_day) else { return &[] };
        let start = date_ts(d["date"].as_str().unwrap_or(""), 0) as f64;
        match h.iter().position(|x| f(x, "t") >= start) {
            Some(i) => &h[i..(i + 24).min(h.len())],
            None => &[],
        }
    }
    fn next24(wx: &Value) -> &[Value] {
        let h: &[Value] = wx["hourly"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let now = (crate::data::now() / 60 * 60) as f64;
        let i = h.iter().position(|x| f(x, "t") + 3600.0 > now).unwrap_or(0);
        &h[i.min(h.len())..(i + 24).min(h.len())]
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
        let days: &[Value] = wx["days"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
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
            let w: Vec<&Value> = r.iter().take(12).collect::<Vec<_>>();
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
    // the pointer once it rested on the card (hover intent), for what changes the content
    fn iptr(&self) -> Option<(f32, f32)> {
        if self.intent.engaged { self.ptr } else { None }
    }
    // the hour under the (resting) pointer on the timeline
    fn tl_idx(&self, n: usize) -> Option<usize> {
        self.tl_at(n, self.iptr())
    }
    fn tl_at(&self, n: usize, p: Option<(f32, f32)>) -> Option<usize> {
        let (tx, ty, tw, th) = (PAD, PAD + BAND_Y + 28.0, WX_W - 2.0 * PAD, 78.0);
        let p = p?;
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
        self.hits.clear();
        let (ox, oy) = (CARD_PAD, CARD_PAD);
        let (w, h) = (WX_W, BENTO_H);
        // glass: shadow + body, then the sky's place cut out (mist-wall's sky shows through); the shades over the
        // sky: behind the headline, over the land. Painted once per tint and theme (3.8 ms), copied after that
        let glass = crate::glass::Glass { radius: 26.0, tint: glass_tint(cfg), rims: false, shadow_opacity: 0.34, ..Default::default() };
        let key = [glass.tint, glass.tone.0, glass.tone.1, glass.tone.2, glass.tone.3].iter().fold(crate::theme::high_contrast() as u32, |k, v| k.wrapping_mul(31).wrapping_add(v.to_bits()));
        match &self.bg {
            Some((k, pm)) if *k == key && pm.width() == c.pm.width() && pm.height() == c.pm.height() => c.pm.data_mut().copy_from_slice(pm.data()),
            _ => {
                c.clear();
                glass.paint(c, ox, oy, w, h);
                if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + 1.0, w - 2.0, h - 2.0, 25.0) {
                    let paint = tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, anti_alias: true, ..Default::default() };
                    c.pm.fill_path(&p, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
                }
                let s = Rgba(4.0 / 255.0, 8.0 / 255.0, 16.0 / 255.0, 1.0);
                if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + 1.0, w - 2.0, h * 0.55, 25.0) {
                    fill_lin(c, &p, (0.0, oy + 1.0), (0.0, oy + 1.0 + h * 0.55), &[(0.0, s.alpha(0.26)), (0.6, s.alpha(0.08)), (1.0, s.alpha(0.0))]);
                }
                if let Some(p) = crate::gfx::rrect_path(ox + 1.0, oy + h * 0.40, w - 2.0, h * 0.60 - 1.0, 25.0) {
                    fill_lin(c, &p, (0.0, oy + h * 0.40), (0.0, oy + h - 1.0), &[(0.0, s.alpha(0.0)), (0.35, s.alpha(0.28)), (1.0, s.alpha(0.58))]);
                }
                self.bg = Some((key, c.pm.clone()));
            }
        }

        let days: &[Value] = wx["days"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let range = self.range(wx, cfg);
        let n = range.len();
        self.last_n = n;
        // while the card shows the Sun or the Moon the timeline is not there to scrub (a pinned hour stays)
        let hover_hour = if !self.pop_pin.is_empty() { self.pin_hour } else { self.hover_hour(n) };
        let hh: Option<&Value> = if hover_hour >= 0 && (hover_hour as usize) < n { Some(&range[hover_hour as usize]) } else { None };
        let sd: Option<&Value> = if self.sel_day > 0 { days.get(self.sel_day) } else { None };
        let live = hh.is_none() && sd.is_none();
        let code = hh.as_ref().map(|x| x["code"].as_i64().unwrap_or(3)).or(sd.as_ref().map(|x| x["code"].as_i64().unwrap_or(3))).unwrap_or(wx["code"].as_i64().unwrap_or(3));
        let ins = Self::insights(wx, garmin, cfg);
        self.n_ins = ins.len();
        let cur = if ins.is_empty() { None } else { Some(&ins[self.ii % ins.len()]) };
        let hl = if live && self.sel_day == 0 { cur.filter(|c| c.i0 >= 0).map(|c| (c.i0, c.i1)).unwrap_or((-1, -1)) } else { (-1, -1) };
        let now = crate::data::now();
        let hero = cfg.s("weather", "hero");
        let now_h: Option<&Value> = Self::next24(wx).first();
        let label = if let Some(h) = &hh {
            let ts = f(h, "t") as i64;
            if self.sel_day == 0 && hover_hour == 0 {
                crate::i18n::tr("teraz").into_owned()
            } else {
                let (a, b) = (tm(ts), tm(now));
                let same = a.tm_yday == b.tm_yday && a.tm_year == b.tm_year;
                format!("{}, {}:00", crate::i18n::tr(if same { "dziś" } else if self.sel_day == 0 { "jutro" } else { DAYS[a.tm_wday as usize] }), a.tm_hour)
            }
        } else if let Some(d) = &sd {
            let ts = date_ts(d["date"].as_str().unwrap_or(""), 12);
            format!("{}, {}", crate::i18n::tr(DAYS[tm(ts).tm_wday as usize]), crate::i18n::tr(&day_mon(ts)))
        } else {
            crate::i18n::tr(match hero.as_str() { "feels" => "odczuwalna", "rain" => "szansa opadu", "wind" => "wiatr, km/h", _ => "" }).into_owned()
        };
        // the footer: an hour, a day, or the insight carousel (translated before the capitals, which miss the lookup)
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

        self.sky_t = if let Some(h) = hh { f(h, "t") as i64 } else if let Some(d) = sd { date_ts(d["date"].as_str().unwrap_or(""), 12) } else { 0 };
        let m = if self.sky_t > 0 { crate::sky::Model::at(wx, self.sky_t as f64 * 1000.0, self.sky_t as f64 * 1000.0) } else { crate::sky::Model::new(wx, (now / 60 * 60) as f64 * 1000.0) };
        // the chips that open the sun's and the moon's details: (x, y, w, h) once drawn
        let mut chips: [Option<(f32, f32, f32, f32)>; 2] = [None, None];
        let mut chip_lab: [(&str, String); 2] = [("", String::new()), ("", String::new())];
        // ---- the content layer (MultiEffect shadow: rgb(4, 8, 18) .6, blur .6, 1 px down)
        let mut l = std::mem::replace(&mut self.layer, Canvas::new(1, 1));
        if l.pm.width() != w as u32 || l.pm.height() != h as u32 {
            l = Canvas::new(w as u32, h as u32);
        } else {
            l.clear();
        }
        // the adjust button and the popovers wait for the pointer to rest (hover intent); highlights do not
        let hov = self.hovered && self.intent.engaged;
        let ptr = self.ptr;
        let iptr = self.iptr();
        let inr = |x: f32, y: f32, ww: f32, hh: f32| ptr.map(|p| p.0 >= x && p.0 < x + ww && p.1 >= y && p.1 < y + hh).unwrap_or(false);
        let inr_i = |x: f32, y: f32, ww: f32, hh: f32| iptr.map(|p| p.0 >= x && p.0 < x + ww && p.1 >= y && p.1 < y + hh).unwrap_or(false);
        // headline
        let city = format!("{}{}", wx["city"].as_str().unwrap_or(""), if wx["stale"].as_bool().unwrap_or(false) { format!(" · dane z {}", crate::data::hm(wx["at"].as_i64().unwrap_or(0))) } else { String::new() });
        let cw_ = caps(&mut l, t, PAD, PAD, &city, FG2, None);
        let cs = caps_style();
        let clh = line_h(t, &cs);
        let mut lh: Hits = Vec::new();
        edit_btn(&mut l, t, &mut self.fx, &mut lh, "weather", PAD + cw_ + 8.0, PAD + clh / 2.0 - 13.0, hov && !self.editing, inr(PAD + cw_ + 8.0, PAD + clh / 2.0 - 13.0, 26.0, 26.0), "weather");
        let hs = hero_style();
        let big = if let Some(h) = &hh {
            format!("{}°", f(h, "temp").round())
        } else if let Some(d) = &sd {
            format!("{}°", ns(&d["max"]))
        } else {
            // the number the card leads with (Pogoda: Główna liczba); rain and wind from the current hour
            match (hero.as_str(), &now_h) {
                ("feels", _) if wx["feels"].is_number() => format!("{}°", ns(&wx["feels"])),
                ("rain", Some(h)) => format!("{}%", f(h, "pop") as i64),
                ("wind", Some(h)) => ns(&h["wind"]),
                _ if wx["temp"].is_number() => format!("{}°", ns(&wx["temp"])),
                _ => "–".into(),
            }
        };
        let bw = text(&mut l, t, PAD, PAD + HERO_Y, &big, &hs, FG, true, None);
        // the big number: click for the next thing to lead with (temperature, feels like, rain, wind)
        if live && !self.editing {
            lh.push((PAD, PAD + HERO_Y + 6.0, bw, 56.0, Hit::Hero));
        }
        let base = PAD + HERO_Y + ascent(t, &hs);
        let mut cy = base - 20.0;
        let colx = PAD + bw + 10.0;
        // what the number is (an hour, a day, the lead picked) sits above the description, whose place never
        // moves: the row of numbers below needs the room
        if !label.is_empty() {
            let ls = st(11.0, 650.0).spaced(0.6);
            let lh_ = line_h(t, &ls);
            text(&mut l, t, colx, cy - lh_ - 2.0, &label.to_uppercase(), &ls, ACC, true, None);
        }
        let ds = st(15.0, 650.0);
        let desc = if live { wx["desc"].as_str().unwrap_or("łączenie…").to_string() } else { wmo(code).to_string() };
        text(&mut l, t, colx, cy, &desc, &ds, FG, true, Some((w - bw - 300.0).max(10.0)));
        cy += line_h(t, &ds) + 2.0;
        let feels = if let Some(h) = &hh { format!("odczuwalna {}°", ns(&h["feels"])) } else if let Some(d) = &sd { format!("{}° / {}°", ns(&d["max"]), ns(&d["min"])) } else if hero != "temp" && wx["temp"].is_number() {
            if hero == "feels" { format!("temperatura {}°", ns(&wx["temp"])) } else { format!("{}° · odczuwalna {}°", ns(&wx["temp"]), ns(&wx["feels"])) }
        } else if wx["feels"].is_number() {
            // with today's high and low
            format!("odczuwalna {}°{}", ns(&wx["feels"]), days.first().filter(|d| d["max"].is_number()).map(|d| format!(" · {}° / {}°", ns(&d["max"]), ns(&d["min"]))).unwrap_or_default())
        } else { String::new() };
        text(&mut l, t, colx, cy, &feels, &st(11.0, 500.0), FG2, true, None);

        // footer: the one thing worth knowing; click for the next one (dots = how many)
        let stack = live && ins.len() > 1;
        if !summary.2.is_empty() {
            let (fx_, fy, fw) = (PAD, PAD + FOOT_Y, w - 2.0 * PAD);
            rect(&mut l, fx_, fy - 10.0, fw, 1.0, 0.0, HAIR);
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

        // the moment's numbers (now, the hour under the pointer, the picked day), always shown. The first three are
        // also what the timeline draws: click one (or turn the wheel over the row)
        let metric = cfg.s("weather", "metric");
        {
            let y = PAD + BAND_Y - 2.0;
            let (pop, mm, wind, dir) = if let Some(h) = hh {
                (f(h, "pop"), f(h, "pr") + f(h, "sn"), f(h, "wind"), f(h, "dir"))
            } else if let Some(d) = sd {
                (f(d, "pop"), f(d, "rain"), f(d, "wind"), f(d, "dir"))
            } else {
                (now_h.map(|h| f(h, "pop")).unwrap_or(0.0), f(wx, "precip"), f(wx, "wind"), f(wx, "windDir"))
            };
            let tabs = [
                ("temp", "temp", "Temp.".to_string(), Rgba::hex("#efb08a")),
                ("rain", "umbrella", format!("{}%{}", pop as i64, if mm > 0.0 { format!(" · {} mm", dec1(mm)) } else { String::new() }), C_BLUE),
                ("wind", "wind", format!("{} km/h {}", wind.round() as i64, dir8(dir)), Rgba::hex("#dce8f8")),
            ];
            let mut x = PAD - 4.0;
            for (k, icon, lab, col) in &tabs {
                let tw_ = tab_w(t, lab);
                if !self.editing {
                    tab(&mut l, t, x, y, icon, lab, metric == *k, inr(x, y, tw_, 26.0), *col);
                    lh.push((x, y, tw_, 26.0, Hit::Metric(k)));
                } else {
                    tab(&mut l, t, x, y, icon, lab, metric == *k, false, *col);
                }
                x += tw_ + 4.0;
            }
            // humidity and UV (cloud cover for an hour, hours of sun for a day), right-aligned; dropped when the
            // tabs leave no room (long translations)
            let mut extra: Vec<(&str, String)> = Vec::new();
            let today = days.first();
            if let Some(h) = hh {
                extra.push(("drop", format!("{}%", f(h, "hum") as i64)));
                extra.push(("cloud", format!("{}%", f(h, "cloud") as i64)));
            } else if let Some(d) = sd {
                if d["sun"].is_number() {
                    extra.push(("sun", format!("{} h", dec1(f(d, "sun")))));
                }
                if d["uv"].is_number() {
                    extra.push(("", format!("UV {}", ns(&d["uv"]))));
                }
            } else {
                if wx["hum"].is_number() {
                    extra.push(("drop", format!("{}%", ns(&wx["hum"]))));
                }
                if let Some(d) = today.filter(|d| d["uv"].is_number()) {
                    extra.push(("", format!("UV {}", ns(&d["uv"]))));
                }
            }
            let es = st(12.0, 600.0).tnum();
            let elh = line_h(t, &es);
            // the sun and the moon, at the row's end: the next sunrise or sunset, how much of the Moon is lit
            let sun_chip = today.map(|d| {
                let tmn = tm(now);
                let nm = tmn.tm_hour * 60 + tmn.tm_min;
                let (r, s_) = (d["rise"].as_str().or(wx["sunrise"].as_str()).unwrap_or(""), d["set"].as_str().or(wx["sunset"].as_str()).unwrap_or(""));
                if nm < mins(r) { ("sunrise", hhmm(r)) } else if nm < mins(s_) { ("sunset", hhmm(s_)) } else { ("sunrise", hhmm(days.get(1).and_then(|d| d["rise"].as_str()).unwrap_or(r))) }
            });
            let body: Vec<(&str, String, usize)> = sun_chip.into_iter().map(|(ic, s_)| (ic, s_, 0)).chain(std::iter::once(("moon", format!("{}%", (m.moon.fraction * 100.0).round() as i64), 1))).collect();
            let bw_: Vec<f32> = body.iter().map(|(_, s_, _)| (8.0 + 14.0 + 4.0 + width(t, s_, &es) + 8.0).round()).collect();
            let mut right = w - PAD + 6.0;
            if !self.editing && cfg.s("weather", "astro") != "hide" && x + 8.0 + bw_.iter().sum::<f32>() + 2.0 <= right {
                for ((ic, s_, which), cw) in body.iter().zip(&bw_).rev() {
                    let cx = right - cw;
                    let on = self.pop_pin == if *which == 0 { "sun" } else { "moon" };
                    if on || inr(cx, y, *cw, 26.0) {
                        rect(&mut l, cx, y, *cw, 26.0, 13.0, WHITE.alpha(if on { 0.15 } else { 0.08 }));
                    }
                    if on {
                        border(&mut l, cx, y, *cw, 26.0, 13.0, 1.0, WHITE.alpha(0.16));
                    }
                    l.icon(t, cx + 8.0 + 7.0, y + 13.0, ic, 13.0, if *which == 0 { C_YELLOW } else { Rgba::hex("#dce8f8") });
                    text(&mut l, t, cx + 8.0 + 14.0 + 4.0, y + (26.0 - elh) / 2.0, s_, &es, FG, true, None);
                    chips[*which] = Some((cx, y, *cw, 26.0));
                    chip_lab[*which] = (*ic, s_.clone());
                    right = cx - 2.0;
                }
                right -= 8.0;
            }
            let widths: Vec<f32> = extra.iter().map(|(ic, s)| width(t, s, &es) + if ic.is_empty() { 0.0 } else { 18.0 }).collect();
            let total = widths.iter().sum::<f32>() + 14.0 * (extra.len() as f32 - 1.0).max(0.0);
            if x + 10.0 + total <= right {
                let mut ex = right - total;
                for ((ic, s_), ew) in extra.iter().zip(&widths) {
                    if !ic.is_empty() {
                        l.icon(t, ex + 7.0, y + 13.0, ic, 13.0, FG2);
                    }
                    text(&mut l, t, ex + if ic.is_empty() { 0.0 } else { 18.0 }, y + (26.0 - elh) / 2.0, s_, &es, FG, true, None);
                    ex += ew + 14.0;
                }
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
            self.curve(&mut l, tx, ty, tw, range, &metric, hl, &ty_);
            // sunrise and sunset on the time axis: a hairline and the icon on the ground
            let (t0, t1) = (f(&range[0], "t"), f(&range[n - 1], "t"));
            for d in days {
                let mid = date_ts(d["date"].as_str().unwrap_or(""), 0) as f64;
                for (k, ic) in [("rise", "sunrise"), ("set", "sunset")] {
                    let Some(hm) = d[k].as_str().filter(|x| x.contains(':')) else { continue };
                    let ts = mid + mins(hm) as f64 * 60.0;
                    if ts <= t0 + 1200.0 || ts >= t1 - 1200.0 {
                        continue;
                    }
                    let x = tx + ((ts - t0) / 3600.0) as f32 * step;
                    rect(&mut l, x.round() - 0.5, ty + 12.0, 1.0, 36.0, 0.0, WHITE.alpha(0.10));
                    l.icon(t, x.round(), ty + 54.0, ic, 11.0, C_YELLOW.alpha(0.85));
                }
            }
            // where "now" is on the curve
            if self.sel_day == 0 && hover_hour < 0 {
                let dy = ty + ty_(val(&range[0]));
                circle(&mut l, tx + 1.0, dy, 5.0, Rgba(0.1, 0.14, 0.24, 0.8));
                circle(&mut l, tx + 1.0, dy, 3.0, FG);
            }
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
            let (rx, ry, rw, rh) = (w - 248.0 - 14.0, 10.0, 248.0, 92.0);
            let shown = || days.iter().take(5).filter(|d| d["max"].is_number() && d["min"].is_number());
            let (lo, hi) = (shown().map(|d| f(d, "min")).fold(f64::MAX, f64::min), shown().map(|d| f(d, "max")).fold(f64::MIN, f64::max));
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
                let colh = l1 + 4.0 + 18.0 + 4.0 + line_h(t, &l3a) + 5.0 + 4.0;
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
                yy += line_h(t, &l3a) + 5.0;
                if hi > lo && d["max"].is_number() && d["min"].is_number() {
                    let (bx, bw_) = (x + 11.0, rw / 4.0 - 22.0);
                    rect(&mut l, bx, yy, bw_, 4.0, 2.0, WHITE.alpha(0.13));
                    let (a, b) = (((f(d, "min") - lo) / (hi - lo)) as f32, ((f(d, "max") - lo) / (hi - lo)) as f32);
                    let (sx0, sw) = (bx + a * bw_, ((b - a) * bw_).max(4.0));
                    if let Some(p) = crate::gfx::rrect_path(sx0.min(bx + bw_ - sw), yy, sw, 4.0, 2.0) {
                        // cold end to warm end over the whole scale, so a bar's colours say where it sits
                        fill_lin(&mut l, &p, (bx, 0.0), (bx + bw_, 0.0), &[(0.0, C_BLUE), (1.0, Rgba::hex("#efb08a"))]);
                    }
                }
                if !self.editing {
                    lh.push((x, ry, rw / 4.0, rh, Hit::Day(day)));
                }
            }
        }
        // the Sun and the Moon can be pointed at, in the sky and on their chips: the card then shows that body
        // instead of the weather (a click keeps it; a click anywhere else, or 20 s away, brings the weather back)
        let (sw_, sh_) = (w as f64 - 2.0, h as f64 - 2.0);
        let (sx, sy) = m.sun_s(sw_, sh_);
        let (mx, my) = m.moon_s(sw_, sh_);
        let sun_c = (1.0 + sx as f32 * sw_ as f32, 1.0 + sy as f32 * sh_ as f32);
        let moon_c = (1.0 + mx as f32 * sw_ as f32, 1.0 + my as f32 * sh_ as f32);
        let ok = |p: (f32, f32)| p.0 > 6.0 && p.0 < w - 6.0 && p.1 > 6.0 && p.1 < 140.0;
        let sun_vis = m.sun.alt > -1.5 && ok(sun_c);
        let moon_vis = m.moon.alt > -1.0 && ok(moon_c);
        let on_chip = |i: usize| chips[i].map(|r| inr_i(r.0, r.1, r.2, r.3)).unwrap_or(false);
        let sun_body = sun_vis && inr_i(sun_c.0 - 22.0, sun_c.1 - 22.0, 44.0, 44.0) && !self.editing;
        let moon_body = moon_vis && inr_i(moon_c.0 - 22.0, moon_c.1 - 22.0, 44.0, 44.0) && !self.editing;
        let (sun_hov, moon_hov) = (sun_body || on_chip(0), !sun_body && (moon_body || on_chip(1)));
        let kind: &'static str = if self.editing { "" } else if sun_hov { "sun" } else if moon_hov { "moon" } else { self.pop_pin };
        if !kind.is_empty() {
            self.last_pop = kind;
            // the weather's own controls rest meanwhile
            lh.retain(|h| matches!(h.4, Hit::Edit(_)));
        }
        if !self.editing {
            if sun_vis {
                lh.push((sun_c.0 - 22.0, sun_c.1 - 22.0, 44.0, 44.0, Hit::Sun));
            }
            if moon_vis {
                lh.push((moon_c.0 - 22.0, moon_c.1 - 22.0, 44.0, 44.0, Hit::Moon));
            }
            for (i, r) in chips.iter().enumerate() {
                if let Some(r) = r {
                    lh.push((r.0, r.1, r.2, r.3, if i == 0 { Hit::Sun } else { Hit::Moon }));
                }
            }
        }
        let av = self.fx.v("wx.astro", if kind.is_empty() { 0.0 } else { 1.0 }, 220.0, Ease::OutCubic);
        if av > 0.0 && !self.last_pop.is_empty() {
            let mut a = std::mem::replace(&mut self.alayer, Canvas::new(1, 1));
            if a.pm.width() != w as u32 || a.pm.height() != h as u32 {
                a = Canvas::new(w as u32, h as u32);
            } else {
                a.clear();
            }
            self.astro(&mut a, t, wx, &m, &chips, &chip_lab, &city);
            // the weather fades out under it
            let k = ((1.0 - av) * 256.0) as u32;
            for b in l.pm.data_mut() {
                *b = ((*b as u32 * k) >> 8) as u8;
            }
            l.pm.draw_pixmap(0, ((1.0 - av) * 8.0).round() as i32, a.pm.as_ref(), &PixmapPaint { opacity: av, ..Default::default() }, Transform::identity(), None);
            self.alayer = a;
        }
        // the content layer with its shadow
        soft_shadow(c, &l.pm, ox as i32, oy as i32, Rgba(4.0 / 255.0, 8.0 / 255.0, 18.0 / 255.0, 1.0), 0.6, 1, &mut self.blur);
        c.pm.draw_pixmap(ox as i32, oy as i32, l.pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        self.layer = l;
        // a ring marks each body: faint while the card is hovered (this can be pointed at), bright while the card shows it
        let so = self.fx.v("wx.sunring", if kind == "sun" { 1.0 } else if hov && !self.editing { 0.4 } else { 0.0 }, 150.0, Ease::Linear);
        if so > 0.0 && sun_vis {
            border(c, ox + sun_c.0 - 19.0, oy + sun_c.1 - 19.0, 38.0, 38.0, 19.0, 1.0, WHITE.alpha(0.55 * so));
        }
        let mo = self.fx.v("wx.moonring", if kind == "moon" { 1.0 } else if hov && !self.editing { 0.4 } else { 0.0 }, 150.0, Ease::Linear);
        if mo > 0.0 && moon_vis {
            border(c, ox + moon_c.0 - 20.0, oy + moon_c.1 - 20.0, 40.0, 40.0, 20.0, 1.0, WHITE.alpha(0.55 * mo));
        }
        crate::desktop::Desk::card_rim(c, ox, oy, w, h);
        // the back of the card
        let mut sh: Hits = Vec::new();
        edit_sheet(c, t, &mut self.fx, &mut sh, cfg, "weather", "Pogoda", ox, oy, w, h, self.editing, &mut self.sheet, ptr.map(|p| (p.0 + ox, p.1 + oy)));
        for r in sh {
            lh.push((r.0 - ox, r.1 - oy, r.2, r.3, r.4));
        }
        self.hits = lh;
    }

    // the card about the Sun or the Moon (self.last_pop), in the weather's layout: the headline, three numbers
    // where the days were, the chips' row, a drawing where the timeline was, the footer
    #[allow(clippy::too_many_arguments)]
    fn astro(&mut self, a: &mut Canvas, t: &mut Text, wx: &Value, m: &crate::sky::Model, chips: &[Option<(f32, f32, f32, f32)>; 2], chip_lab: &[(&str, String); 2], city: &str) {
        // the moment shown: the hour or the day pointed at on the timeline, else now
        let now = if self.sky_t > 0 { self.sky_t } else { crate::data::now() };
        let nowms = (now / 60 * 60) as f64 * 1000.0;
        let (lat, lon) = (wx["lat"].as_f64().unwrap_or(51.48), wx["lon"].as_f64().unwrap_or(0.0));
        let days = wx["days"].as_array().cloned().unwrap_or_default();
        let moon = self.last_pop == "moon";
        let (w, p) = (WX_W, m.moon.phase);
        let ptr = self.ptr;
        let inr = |x: f32, y: f32, ww: f32, hh: f32| ptr.map(|p| p.0 >= x && p.0 < x + ww && p.1 >= y && p.1 < y + hh).unwrap_or(false);
        let dim = FG2.alpha(0.72);
        let tmn = tm(now);
        let date = format!("{:04}-{:02}-{:02}", tmn.tm_year + 1900, tmn.tm_mon + 1, tmn.tm_mday);
        caps(a, t, PAD, PAD, &format!("{} · {}", if moon { "Księżyc" } else { "Słońce" }, city), FG2, None);
        // the row of the chips: the way back on the left, the two bodies where they were
        let ry = PAD + BAND_Y - 2.0;
        let bw_ = tab_w(t, "Pogoda");
        tab(a, t, PAD - 4.0, ry, "cloud", "Pogoda", false, inr(PAD - 4.0, ry, bw_, 26.0), Rgba::hex("#dce8f8"));
        let es = st(12.0, 600.0).tnum();
        let elh = line_h(t, &es);
        for (i, r) in chips.iter().enumerate() {
            let Some((cx, y, cw, _)) = *r else { continue };
            let on = (i == 1) == moon;
            if on || inr(cx, y, cw, 26.0) {
                rect(a, cx, y, cw, 26.0, 13.0, WHITE.alpha(if on { 0.15 } else { 0.08 }));
            }
            if on {
                border(a, cx, y, cw, 26.0, 13.0, 1.0, WHITE.alpha(0.16));
            }
            a.icon(t, cx + 8.0 + 7.0, y + 13.0, chip_lab[i].0, 13.0, if i == 0 { C_YELLOW } else { Rgba::hex("#dce8f8") });
            text(a, t, cx + 8.0 + 14.0 + 4.0, y + (26.0 - elh) / 2.0, &chip_lab[i].1, &es, FG, true, None);
        }
        // three numbers, top right: a label over its value
        let (ls, vs) = (st(10.5, 500.0), st(14.0, 650.0).tnum());
        let lh_ = line_h(t, &ls);
        let cw = 80.0;
        let cell = |a: &mut Canvas, t: &mut Text, col: usize, label: &str, value: &str| {
            let x = w - PAD - (3 - col) as f32 * cw + 8.0;
            text(a, t, x, PAD + 6.0, label, &ls, dim, true, Some(cw - 8.0));
            text(a, t, x, PAD + 6.0 + lh_ + 2.0, value, &vs, FG, true, Some(cw - 8.0));
        };
        let alt_txt = |alt: f64| format!("{}{}°", if alt < -0.5 { "−" } else { "" }, alt.abs().round());
        // the headline: what leads (a clock time, the Moon's disc), a label over two lines beside it
        let hs = hero_style();
        let base = PAD + HERO_Y + ascent(t, &hs);
        let head = |a: &mut Canvas, t: &mut Text, x: f32, label: &str, l1: &str, l2: &str| {
            let cy = base - 20.0;
            let las = st(11.0, 650.0).spaced(0.6);
            let llh = line_h(t, &las);
            text(a, t, x, cy - llh - 2.0, label, &las, ACC, true, None);
            let ds = st(15.0, 650.0);
            text(a, t, x, cy, l1, &ds, FG, true, Some(w - PAD - x));
            let dlh = line_h(t, &ds);
            text(a, t, x, cy + dlh + 2.0, l2, &st(11.0, 500.0), FG2, true, Some(w - PAD - x));
        };
        let (fy, fw) = (PAD + FOOT_Y, w - 2.0 * PAD);
        rect(a, PAD, fy - 10.0, fw, 1.0, 0.0, HAIR);
        let fs = st(13.0, 600.0);
        let flh = line_h(t, &fs);
        if moon {
            let title = phase_name(p);
            let (rise, set) = crate::astro::moon_times(nowms, lat, lon);
            let hm = |x: Option<f64>| x.map(|v| crate::data::hm((v / 1000.0) as i64)).unwrap_or("–".into());
            let dm = |x: Option<f64>| x.map(|v| day_mon((v / 1000.0) as i64)).unwrap_or("–".into());
            moon_disc(a, PAD + 27.0, PAD + HERO_Y + 12.0 + 27.0, 27.0, p);
            let lit = format!("{} {}%", crate::i18n::tr("Oświetlenie").to_uppercase(), (m.moon.fraction * 100.0).round());
            head(a, t, PAD + 54.0 + 14.0, &lit, title, &format!("nów {} · pełnia {}", dm(crate::astro::next_phase(nowms, 0.0)), dm(crate::astro::next_phase(nowms, 0.5))));
            cell(a, t, 0, "Wschód", &hm(rise));
            cell(a, t, 1, "Zachód", &hm(set));
            cell(a, t, 2, "Wysokość", &alt_txt(m.moon.alt));
            // the Moon of the next days, where the timeline was
            let n = 10;
            let step = fw / n as f32;
            let noon = date_ts(&date, 12) as f64 * 1000.0;
            let ds = st(11.0, 600.0).tnum();
            for k in 0..n {
                let ms = noon + k as f64 * 86400e3;
                let (cx, cy) = (PAD + (k as f32 + 0.5) * step, PAD + BAND_Y + 28.0 + 34.0);
                moon_disc(a, cx, cy, 14.0, crate::astro::moon(ms, lat, lon).phase);
                if k == 0 {
                    border(a, cx - 18.0, cy - 18.0, 36.0, 36.0, 18.0, 1.0, WHITE.alpha(0.5));
                }
                let lab = if k == 0 { crate::i18n::tr("dziś").into_owned() } else { format!("{}", tm((ms / 1000.0) as i64).tm_mday) };
                let lw = width(t, &lab, &ds);
                text(a, t, cx - lw / 2.0, cy + 24.0, &lab, &ds, if k == 0 { FG } else { dim }, true, None);
            }
            a.icon(t, PAD + 7.0, fy + 12.0, "moon", 14.0, Rgba::hex("#dce8f8"));
            let x = PAD + 22.0;
            let x = x + text(a, t, x, fy + (24.0 - flh) / 2.0, "Odległość", &fs, FG2, true, None) + 6.0;
            text(a, t, x, fy + (24.0 - flh) / 2.0, &format!("{} km", thousands(crate::astro::moon_distance(nowms))), &fs, FG, true, None);
        } else {
            // that day's sunrise and sunset, and the next day's
            let di = days.iter().position(|d| d["date"].as_str() == Some(date.as_str())).unwrap_or(0);
            let td = days.get(di).cloned().unwrap_or(Value::Null);
            let r = td["rise"].as_str().or(wx["sunrise"].as_str()).unwrap_or("");
            let st_ = td["set"].as_str().or(wx["sunset"].as_str()).unwrap_or("");
            let nd = days.get(di + 1);
            let next_r = nd.and_then(|d| d["rise"].as_str()).unwrap_or(r);
            let (rm, sm, nm) = (mins(r), mins(st_).max(mins(r) + 1), tmn.tm_hour * 60 + tmn.tm_min);
            let day = nm >= rm && nm < sm;
            // leads with what comes next: the sunset by day, the sunrise by night
            let (big, label, l1) = if day {
                (hhmm(st_), "Zachód", format!("do zachodu {}", dur(sm - nm)))
            } else if nm < rm {
                (hhmm(r), "Wschód", format!("do wschodu {}", dur(rm - nm)))
            } else {
                (hhmm(next_r), "Wschód", format!("do wschodu {}", dur(mins(next_r) + 1440 - nm)))
            };
            let bw = text(a, t, PAD, PAD + HERO_Y, &big, &hs, FG, true, None);
            let g = (sm - 60).max(0);
            let l2 = if day || nm < rm { format!("złota godzina od {}", format!("{}:{:02}", g / 60, g % 60)) } else { format!("dzień trwa {}", dur(sm - rm)) };
            head(a, t, PAD + bw + 10.0, &crate::i18n::tr(label).to_uppercase(), &l1, &l2);
            cell(a, t, 0, "Wschód", &hhmm(r));
            cell(a, t, 1, "Zachód", &hhmm(st_));
            cell(a, t, 2, "Wysokość", &alt_txt(m.sun.alt));
            // the Sun of the next days, where the timeline was (as the Moon's view shows its phases): a column a
            // day with its sunrise, its sunset and how long it is light
            let list: Vec<&Value> = days.iter().skip(di).filter(|d| d["rise"].is_string() && d["set"].is_string()).take(5).collect();
            if !list.is_empty() {
                let step = fw / list.len() as f32;
                let (ds, ts, ms_) = (st(11.0, 650.0), st(13.0, 600.0).tnum(), st(10.5, 500.0).tnum());
                let (dlh, tlh) = (line_h(t, &ds), line_h(t, &ts));
                let top = PAD + BAND_Y + 28.0 + 16.0;
                for (k, d) in list.iter().enumerate() {
                    let x = PAD + k as f32 * step;
                    if k > 0 {
                        rect(a, x, top + 2.0, 1.0, dlh + 2.0 * tlh + 30.0, 0.0, HAIR);
                    }
                    let x = x + if k > 0 { 14.0 } else { 0.0 };
                    let (dr, dset) = (d["rise"].as_str().unwrap_or(""), d["set"].as_str().unwrap_or(""));
                    let name = if k == 0 && di == 0 { crate::i18n::tr("dziś").into_owned() } else { d["name"].as_str().unwrap_or("").to_string() };
                    text(a, t, x, top, &name, &ds, if k == 0 { ACC } else { FG2 }, true, None);
                    let mut y = top + dlh + 8.0;
                    for (icon, v) in [("sunrise", dr), ("sunset", dset)] {
                        a.icon(t, x + 7.0, y + tlh / 2.0, icon, 13.0, C_YELLOW.alpha(if k == 0 { 1.0 } else { 0.75 }));
                        text(a, t, x + 20.0, y, &hhmm(v), &ts, if k == 0 { FG } else { FG.alpha(0.86) }, true, None);
                        y += tlh + 4.0;
                    }
                    text(a, t, x, y + 2.0, &dur(mins(dset) - mins(dr)), &ms_, dim, true, None);
                }
            }
            // footer: how long the day is, and tomorrow's against it
            let len = sm - rm;
            let diff = nd.filter(|d| d["rise"].is_string() && d["set"].is_string()).map(|d| mins(d["set"].as_str().unwrap_or("")) - mins(d["rise"].as_str().unwrap_or("")) - len).unwrap_or(0);
            let foot = format!("dzień trwa {}{}", dur(len), if diff > 0 { format!(" · jutro dłuższy o {diff} min") } else if diff < 0 { format!(" · jutro krótszy o {} min", -diff) } else { String::new() });
            a.icon(t, PAD + 7.0, fy + 12.0, "sun", 14.0, C_YELLOW);
            text(a, t, PAD + 22.0, fy + (24.0 - flh) / 2.0, &foot, &fs, FG, true, Some(fw - 22.0));
        }
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
        paint_wash(soft(r.iter().map(|h| if h["day"].as_f64().unwrap_or(if h["day"].as_bool().unwrap_or(false) { 1.0 } else { 0.0 }) != 0.0 { 0.0 } else { 1.0 }).collect()), (0, 0, 0), 0.13);
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
        // the area under a curve, fading to the ground
        let area_of = |c: &mut Canvas, vals: &[f64], col: Rgba, a: f32| {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x0, y0 + base);
            pb.line_to(x0, y0 + ty(vals[0]));
            for i in 1..n {
                let mx = x0 + (i as f32 - 0.5) * st_;
                pb.cubic_to(mx, y0 + ty(vals[i - 1]), mx, y0 + ty(vals[i]), x0 + i as f32 * st_, y0 + ty(vals[i]));
            }
            pb.line_to(x0 + w, y0 + base);
            pb.close();
            if let Some(p) = pb.finish() {
                fill_lin(c, &p, (0.0, y0 + 12.0), (0.0, y0 + base), &[(0.0, col.alpha(a)), (1.0, col.alpha(0.0))]);
            }
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
                let vals = r.iter().map(|h| f(h, "wind")).collect::<Vec<_>>();
                area_of(c, &vals, Rgba::hex("#dce8f8"), 0.14);
                if let Some(p) = curve_of(&vals) {
                    stroke(c, &p, 2.2, Rgba::hex("#dce8f8"), true);
                }
            }
            _ => {
                let vals = r.iter().map(|h| f(h, "temp")).collect::<Vec<_>>();
                area_of(c, &vals, Rgba::hex("#c6b6bd"), 0.20);
                if let Some(p) = curve_of(&vals) {
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
        format!("{:?}{}{}{:?}{}{}", h, self.hovered, self.intent.engaged, self.tl_idx(self.last_n), self.sel_day, self.pop_pin)
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
            Some(Hit::Sheet) | None => "",
            _ => "hand",
        };
        self.intent.feed(p, if inside { "wx" } else { "" });
        self.preview();
    }
    // resting on a day previews it (its hours stay while scrubbing the timeline); anywhere else shows today again
    // at once, unless a day is pinned
    fn preview(&mut self) {
        if self.pinned {
            return;
        }
        let d = match self.iptr().and_then(|p| hit_at(&self.hits, p.0, p.1)) {
            Some(Hit::Day(d)) => d,
            Some(Hit::Timeline) => self.sel_day,
            _ => 0,
        };
        if self.sel_day != d {
            self.sel_day = d;
            self.pin_hour = -1;
        }
    }
    // the pointer rested (hover intent): true = repaint
    pub fn settle(&mut self) -> bool {
        if self.intent.settle() {
            self.preview();
            return true;
        }
        false
    }
    pub fn click(&mut self, cfg: &mut Cfg, wx: &Value) -> bool {
        let Some(p) = self.ptr else { return false };
        let hit = hit_at(&self.hits, p.0, p.1);
        // the sun's / moon's details: a click keeps them open, the next one (or a click anywhere else) closes them
        let was = std::mem::take(&mut self.pop_pin);
        match hit {
            Some(Hit::Sun) => self.pop_pin = if was == "sun" { "" } else { "sun" },
            Some(Hit::Moon) => self.pop_pin = if was == "moon" { "" } else { "moon" },
            Some(Hit::Edit(_)) => self.editing = true,
            Some(Hit::Done(_)) => self.editing = false,
            Some(Hit::Move(card, d)) => cfg.mv(card, d),
            Some(Hit::Opt(card, key, v)) => {
                if let Some(gr) = groups(card).into_iter().find(|g| g.key == key) {
                    cfg.click(card, &gr, &v);
                }
            }
            Some(Hit::Metric(k)) => cfg.set("weather", "metric", Value::String(k.into())),
            Some(Hit::Hero) => {
                let h = cfg.s("weather", "hero");
                let i = HEROES.iter().position(|x| *x == h).unwrap_or(0);
                cfg.set("weather", "hero", Value::String(HEROES[(i + 1) % HEROES.len()].into()));
            }
            Some(Hit::Foot) => {
                self.ii += 1;
                self.fx.from("wx.foot", 0.2, 1.0, 220.0);
                self.turned = Instant::now();
                self.foot_cache = None;
            }
            Some(Hit::Timeline) => {
                let n = self.range(wx, cfg).len();
                if let Some(i) = self.tl_at(n, self.ptr) {
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
            _ => return !was.is_empty(),
        }
        true
    }
    // wheel: walk through the days; over the numbers' row through what the timeline draws, over the footer
    // through the insights
    pub fn wheel(&mut self, down: bool, wx: &Value, cfg: &mut Cfg) -> bool {
        // turned to its settings: the wheel scrolls them
        if self.editing {
            let s = (self.sheet.0 + if down { 40.0 } else { -40.0 }).clamp(0.0, self.sheet.1);
            let moved = s != self.sheet.0;
            self.sheet.0 = s;
            return moved;
        }
        match self.ptr.and_then(|p| hit_at(&self.hits, p.0, p.1)) {
            Some(Hit::Metric(_)) => {
                let m = cfg.s("weather", "metric");
                let i = METRICS.iter().position(|x| *x == m).unwrap_or(0) as i32 + if down { 1 } else { -1 };
                cfg.set("weather", "metric", Value::String(METRICS[i.rem_euclid(METRICS.len() as i32) as usize].into()));
                return true;
            }
            Some(Hit::Foot) if self.n_ins > 1 => {
                self.ii = (self.ii % self.n_ins + if down { 1 } else { self.n_ins - 1 }) % self.n_ins;
                self.fx.from("wx.foot", 0.2, 1.0, 220.0);
                self.turned = Instant::now();
                self.foot_cache = None;
                return true;
            }
            _ => {}
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
        self.hovered || self.sel_day != 0 || self.pin_hour >= 0 || !self.pop_pin.is_empty()
    }
    // every 250 ms (1 s at rest): back to "now" after leaving, the insight carousel; true = repaint
    pub fn tick(&mut self, n_ins: usize) -> bool {
        let mut ch = false;
        // the fade's last full paint, when no frame came after its 220 ms (a stale blend would stay for good)
        if self.foot_fast && self.turned.elapsed().as_millis() >= 220 {
            self.foot_fast = false;
            self.foot_cache = None;
            ch = true;
        }
        if let Some(a) = self.away {
            let kept = !self.pop_pin.is_empty();
            if (self.sel_day != 0 || self.pin_hour >= 0 || kept) && a.elapsed().as_millis() >= if self.pinned || kept { 20000 } else { 2500 } {
                self.pop_pin = "";
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
            self.foot_cache = None;
            ch = true;
        }
        ch
    }
    pub fn n_insights(wx: &Value, garmin: &Value, cfg: &Cfg) -> usize {
        Self::insights(wx, garmin, cfg).len()
    }
    // how many insights the last paint had: the carousel's length, without working them out every second
    pub fn pin_pop(&mut self, k: &'static str) {
        self.pop_pin = k;
    }
    pub fn n_shown(&self) -> usize {
        self.n_ins
    }
}

// one of the numbers that is also a switch: icon + value in a pill (filled when it is what the timeline draws)
fn tab_w(t: &mut Text, label: &str) -> f32 {
    (9.0 + 14.0 + 5.0 + width(t, label, &st(12.0, 600.0).tnum()) + 10.0).round()
}
#[allow(clippy::too_many_arguments)]
fn tab(c: &mut Canvas, t: &mut Text, x: f32, y: f32, icon: &str, label: &str, on: bool, hovered: bool, col: Rgba) {
    let w = tab_w(t, label);
    if on || hovered {
        rect(c, x, y, w, 26.0, 13.0, WHITE.alpha(if on { 0.15 } else { 0.08 }));
    }
    if on {
        border(c, x, y, w, 26.0, 13.0, 1.0, WHITE.alpha(0.16));
    }
    c.icon(t, x + 9.0 + 7.0, y + 13.0, icon, 13.0, if on { col } else { FG2 });
    let sty = st(12.0, 600.0).tnum();
    let lh = line_h(t, &sty);
    text(c, t, x + 9.0 + 14.0 + 5.0, y + (26.0 - lh) / 2.0, label, &sty, if on { FG } else { FG2 }, true, None);
}

// The content layer's drop shadow (MultiEffect: its alpha blurred, tinted, `dy` px down), blended under where the
// layer goes at (x, y). The alpha is halved in size before the three box passes (9 wide there = the 17 of
// desktop::drop_shadow at sigma 7.7): the same to the eye, a quarter of the pixels, integers only. `buf` is kept
// between paints
fn soft_shadow(c: &mut Canvas, layer: &Pixmap, x: i32, y: i32, col: Rgba, opacity: f32, dy: i32, buf: &mut (Vec<u32>, Vec<u32>)) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    let (hw, hh) = (w / 2, h / 2);
    if hw < 2 || hh < 2 {
        return;
    }
    let (a, tmp) = (&mut buf.0, &mut buf.1);
    a.clear();
    a.resize(hw * hh, 0);
    tmp.clear();
    tmp.resize(hw * hh, 0);
    let src = layer.data();
    for yy in 0..hh {
        let (r0, r1) = (yy * 2 * w * 4, (yy * 2 + 1) * w * 4);
        for xx in 0..hw {
            let i = xx * 8 + 3;
            a[yy * hw + xx] = src[r0 + i] as u32 + src[r0 + i + 4] as u32 + src[r1 + i] as u32 + src[r1 + i + 4] as u32;
        }
    }
    const R: usize = 4;
    const WIN: u32 = 2 * R as u32 + 1;
    let mut sums = vec![0u32; hw];
    for _ in 0..3 {
        for yy in 0..hh {
            let row = &a[yy * hw..(yy + 1) * hw];
            let out = &mut tmp[yy * hw..(yy + 1) * hw];
            let mut sum: u32 = row[..=R.min(hw - 1)].iter().sum();
            for i in 0..hw {
                out[i] = (sum + WIN / 2) / WIN;
                if i + R + 1 < hw {
                    sum += row[i + R + 1];
                }
                if i >= R {
                    sum -= row[i - R];
                }
            }
        }
        // columns: one running sum per column, rows added and dropped as the window moves down
        sums.iter_mut().for_each(|v| *v = 0);
        for yy in 0..=R.min(hh - 1) {
            for (sv, v) in sums.iter_mut().zip(&tmp[yy * hw..(yy + 1) * hw]) {
                *sv += *v;
            }
        }
        for yy in 0..hh {
            for (o, sv) in a[yy * hw..(yy + 1) * hw].iter_mut().zip(&sums) {
                *o = (*sv + WIN / 2) / WIN;
            }
            if yy + R + 1 < hh {
                for (sv, v) in sums.iter_mut().zip(&tmp[(yy + R + 1) * hw..(yy + R + 2) * hw]) {
                    *sv += *v;
                }
            }
            if yy >= R {
                for (sv, v) in sums.iter_mut().zip(&tmp[(yy - R) * hw..(yy - R + 1) * hw]) {
                    *sv -= *v;
                }
            }
        }
    }
    // back to full size (bilinear: quarters of the two nearest halves each way), source-over on the canvas
    let (cw, ch) = (c.pm.width() as i32, c.pm.height() as i32);
    let k = (col.3 * opacity).clamp(0.0, 1.0) * 255.0 / (1020.0 * 16.0);
    let rgb = [(col.0 * 255.0) as u32, (col.1 * 255.0) as u32, (col.2 * 255.0) as u32];
    // per axis: the two half-size cells and the weight (of 4) of the first
    let cells = |p: usize, n: usize| -> (usize, usize, u32) {
        if p % 2 == 0 { ((p / 2).saturating_sub(1), (p / 2).min(n - 1), 1) } else { (p / 2, (p / 2 + 1).min(n - 1), 3) }
    };
    let xs: Vec<(usize, usize, u32)> = (0..w).map(|p| cells(p, hw)).collect();
    let dst = c.pm.data_mut();
    for py in 0..h as i32 {
        let (ty_, sy) = (y + py, py - dy);
        if ty_ < 0 || ty_ >= ch || sy < 0 || sy >= h as i32 {
            continue;
        }
        let (j0, j1, wy) = cells(sy as usize, hh);
        let (ra, rb) = (&a[j0 * hw..(j0 + 1) * hw], &a[j1 * hw..(j1 + 1) * hw]);
        for (px, (i0, i1, wx)) in xs.iter().enumerate() {
            let tx_ = x + px as i32;
            if tx_ < 0 || tx_ >= cw {
                continue;
            }
            let v = (ra[*i0] * wx + ra[*i1] * (4 - wx)) * wy + (rb[*i0] * wx + rb[*i1] * (4 - wx)) * (4 - wy);
            if v == 0 {
                continue;
            }
            let al = ((v as f32 * k) as u32).min(255);
            if al == 0 {
                continue;
            }
            let i = (ty_ as usize * cw as usize + tx_ as usize) * 4;
            let inv = 255 - al;
            for ch_ in 0..3 {
                dst[i + ch_] = ((rgb[ch_] * al + dst[i + ch_] as u32 * inv + 127) / 255) as u8;
            }
            dst[i + 3] = ((255 * al + dst[i + 3] as u32 * inv + 127) / 255) as u8;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // the half-size shadow against desktop::drop_shadow (what the card used): text and a block, never more than
    // a few levels of alpha apart
    #[test]
    fn soft_shadow_matches_the_full_blur() {
        let mut l = Canvas::new(200, 120);
        rect(&mut l, 30.0, 30.0, 60.0, 24.0, 6.0, WHITE);
        rect(&mut l, 120.0, 70.0, 40.0, 2.0, 0.0, WHITE);
        let col = Rgba(4.0 / 255.0, 8.0 / 255.0, 18.0 / 255.0, 1.0);
        let full = drop_shadow(&l.pm, 7.7, col, 0.6, 0, 1);
        let mut c = Canvas::new(200, 120);
        soft_shadow(&mut c, &l.pm, 0, 0, col, 0.6, 1, &mut (Vec::new(), Vec::new()));
        let worst = full.data().chunks_exact(4).zip(c.pm.data().chunks_exact(4)).map(|(a, b)| (a[3] as i32 - b[3] as i32).abs()).max().unwrap();
        assert!(worst <= 10, "alpha differs by {worst}");
        assert!(c.pm.data().chunks_exact(4).any(|p| p[3] > 60), "a shadow was drawn");
    }

    // the sun's details pinned by a click: they close on the next click and 20 s after the pointer left
    #[test]
    fn pinned_details_close() {
        let mut w = Wx::new();
        let mut cfg = Cfg::load();
        w.hits = vec![(0.0, 0.0, 10.0, 10.0, Hit::Sun), (20.0, 0.0, 10.0, 10.0, Hit::Moon)];
        w.ptr = Some((5.0, 5.0));
        assert!(w.click(&mut cfg, &Value::Null));
        assert_eq!(w.pop_pin, "sun");
        w.ptr = Some((25.0, 5.0));
        assert!(w.click(&mut cfg, &Value::Null));
        assert_eq!(w.pop_pin, "moon");
        assert!(w.click(&mut cfg, &Value::Null));
        assert_eq!(w.pop_pin, "");
        w.ptr = Some((5.0, 5.0));
        w.click(&mut cfg, &Value::Null);
        w.ptr = Some((200.0, 200.0));
        assert!(w.click(&mut cfg, &Value::Null), "a click elsewhere closes them and repaints");
        assert_eq!(w.pop_pin, "");
        w.pop_pin = "sun";
        assert!(w.pending());
        w.away = Some(Instant::now() - Duration::from_secs(21));
        assert!(w.tick(0));
        assert_eq!(w.pop_pin, "");
    }

    // a turn whose fade got no frame after 220 ms: the next tick paints it in full and drops the blend cache
    #[test]
    fn carousel_fade_never_sticks() {
        let mut w = Wx::new();
        w.turned -= Duration::from_secs(9);
        assert!(w.tick(5));
        assert_eq!(w.ii, 1);
        assert!(w.foot_fast);
        let px = Canvas::new(1, 1).pm;
        w.foot_cache = Some((px.clone(), px, 0, 1)); // the frames of insight 1, painted mid-fade
        w.turned -= Duration::from_millis(300);
        assert!(w.tick(5), "the fade's end repaints");
        assert!(!w.foot_fast && w.foot_cache.is_none());
        w.turned -= Duration::from_secs(9);
        assert!(w.tick(5));
        assert_eq!(w.ii, 2);
        assert!(w.foot_cache.is_none(), "a turn never blends the previous insight's frames");
    }
}
