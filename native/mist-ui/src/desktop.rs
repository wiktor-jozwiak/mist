// The desktop (was Desktop.qml), main monitor, under the windows. One centred composition like a lock screen:
// the date in caps, the big clock, the sun's path, then a bento row: Garmin | weather (wide, live sky) |
// running, all one height. Every card is liquid glass with a lighter inner step; hovering a part of a card
// explains it in the footer, the adjust button flips a card to its settings (~/.config/mist/widgets.json).
// Two surfaces on the Bottom layer: "mist-desktop" (blurred behind: date, clock, Garmin, running) and
// "mist-desktop-card" (no blur: the weather card's text over its sky, wxcard.rs). The sky itself is drawn
// by mist-wall in a Background surface under the card; this writes where ($XDG_RUNTIME_DIR/mist/sky.json).
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::glass::Glass;
use crate::text::{Style, Text};
use serde_json::{json, Value};
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use std::collections::HashMap;
use tiny_skia::{FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, SpreadMode, Transform};

pub const STAGE_TOP: i32 = 118;
pub const BENTO_H: f32 = 312.0;
pub const WX_W: f32 = 548.0;
pub const COL_W: f32 = 308.0;
pub const BENTO_GAP: f32 = 16.0;
pub const BENTO_W: f32 = COL_W * 2.0 + WX_W + BENTO_GAP * 2.0; // 1196
pub const STAGE_W: u32 = BENTO_W as u32 + 80;
pub const PAD: f32 = 20.0;
pub const HERO_Y: f32 = 26.0;
pub const BAND_Y: f32 = 102.0;
pub const FOOT_Y: f32 = BENTO_H - 2.0 * PAD - 24.0;
pub const CARD_PAD: f32 = 40.0; // the weather card's window: room for its shadow

// card palette: shades of white + one accent (teal) + one warning (amber); red only when really bad
pub const FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);
pub const FG2: Rgba = Rgba(222.0 / 255.0, 230.0 / 255.0, 242.0 / 255.0, 0.86);
pub const MUTED: Rgba = Rgba(206.0 / 255.0, 216.0 / 255.0, 232.0 / 255.0, 0.74);
pub const FAINT: Rgba = Rgba(196.0 / 255.0, 208.0 / 255.0, 226.0 / 255.0, 0.6);
pub const ACC: Rgba = Rgba::rgb8(0x8a, 0xd6, 0xd0, 1.0);
pub const WARN: Rgba = Rgba::rgb8(0xec, 0xc4, 0x8e, 1.0);
pub const BAD: Rgba = Rgba::rgb8(0xee, 0x95, 0xa3, 1.0);
pub const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.10);
pub const C_YELLOW: Rgba = Rgba::rgb8(0xec, 0xd4, 0x8e, 1.0);
pub const C_BLUE: Rgba = Rgba::rgb8(0x9d, 0xbd, 0xf0, 1.0);
pub const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const RAISED: Rgba = Rgba(3.0 / 255.0, 6.0 / 255.0, 12.0 / 255.0, 0.30);

pub fn level(v: Option<f64>) -> Rgba {
    match v {
        None => MUTED,
        Some(v) if v >= 50.0 => ACC,
        Some(v) if v >= 25.0 => WARN,
        _ => BAD,
    }
}
pub fn km1(x: f64) -> String {
    format!("{:.1}", (x * 10.0).round() / 10.0).replace('.', ",")
}
pub fn runs_word(n: i64) -> &'static str {
    if n == 1 { "bieg" } else if (2..=4).contains(&(n % 10)) && !(10..20).contains(&(n % 100)) { "biegi" } else { "biegów" }
}
pub fn thousands(n: f64) -> String {
    let s = format!("{}", n.round() as i64);
    let (neg, d) = if let Some(x) = s.strip_prefix('-') { (true, x.to_string()) } else { (false, s) };
    let mut out = String::new();
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    if neg { format!("-{out}") } else { out }
}
pub fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}
// "–" for a missing number, else the number as JS prints it
pub fn ns(v: &Value) -> String {
    match v.as_f64() {
        Some(x) if x.fract() == 0.0 => format!("{}", x as i64),
        Some(x) => format!("{x}"),
        None => "–".into(),
    }
}

// ------------------------------------------------------------------ text (CTxt / Txt)
pub fn st(size: f32, w: f32) -> Style {
    Style::ui(size, w)
}
// line box (QML Text implicitHeight) and ascent
pub fn line_h(t: &mut Text, s: &Style) -> f32 {
    let sh = t.shape("Hg", s);
    (sh.ascent + sh.descent).ceil()
}
pub fn ascent(t: &mut Text, s: &Style) -> f32 {
    t.shape("Hg", s).ascent
}
// a text whose line box starts at (x, y); `raised` = CTxt's 1 px shadow; `max` = width with elide
pub fn text(c: &mut Canvas, t: &mut Text, x: f32, y: f32, s: &str, sty: &Style, col: Rgba, raised: bool, max: Option<f32>) -> f32 {
    let s = match max {
        Some(m) if t.width(s, sty) > m => crate::bar::elide(t, s, sty, m),
        _ => s.to_string(),
    };
    let sh = t.shape(&s, sty);
    let cy = y + (sh.ascent + sh.descent) / 2.0;
    if raised {
        c.text(t, x, cy + 1.0, &s, sty, RAISED.alpha(col.3.min(1.0)));
    }
    c.text(t, x, cy, &s, sty, col)
}
pub fn width(t: &mut Text, s: &str, sty: &Style) -> f32 {
    t.width(s, sty)
}

// ------------------------------------------------------------------ paint helpers
pub fn skia(c: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.0.clamp(0.0, 1.0), c.1.clamp(0.0, 1.0), c.2.clamp(0.0, 1.0), c.3.clamp(0.0, 1.0)).unwrap()
}
pub fn stops(s: &[(f32, Rgba)]) -> Vec<GradientStop> {
    s.iter().map(|(p, c)| GradientStop::new(*p, skia(*c))).collect()
}
pub fn fill_lin(c: &mut Canvas, path: &tiny_skia::Path, p0: (f32, f32), p1: (f32, f32), s: &[(f32, Rgba)]) {
    let Some(sh) = LinearGradient::new(Point::from_xy(p0.0, p0.1), Point::from_xy(p1.0, p1.1), stops(s), SpreadMode::Pad, Transform::identity()) else { return };
    let paint = Paint { shader: sh, anti_alias: true, ..Default::default() };
    c.pm.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}
pub fn fill_rad(c: &mut Canvas, path: &tiny_skia::Path, cx: f32, cy: f32, r: f32, s: &[(f32, Rgba)]) {
    let Some(sh) = RadialGradient::new(Point::from_xy(cx, cy), Point::from_xy(cx, cy), r, stops(s), SpreadMode::Pad, Transform::identity()) else { return };
    let paint = Paint { shader: sh, anti_alias: true, ..Default::default() };
    c.pm.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}
pub fn rect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, col: Rgba) {
    c.rrect(x, y, w, h, r, col);
}
pub fn border(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, bw: f32, col: Rgba) {
    // a QML Rectangle border: inside the rect
    let (Some(o), Some(i)) = (crate::gfx::rrect_path(x, y, w, h, r), crate::gfx::rrect_path(x + bw, y + bw, w - 2.0 * bw, h - 2.0 * bw, (r - bw).max(0.0))) else { return };
    let mut pb = PathBuilder::new();
    pb.push_path(&o);
    pb.push_path(&i);
    let Some(p) = pb.finish() else { return };
    let mut paint = Paint::default();
    paint.set_color(skia(col));
    paint.anti_alias = true;
    c.pm.fill_path(&p, &paint, FillRule::EvenOdd, Transform::identity(), None);
}
pub fn circle(c: &mut Canvas, cx: f32, cy: f32, r: f32, col: Rgba) {
    rect(c, cx - r, cy - r, 2.0 * r, 2.0 * r, r, col);
}
pub fn stroke(c: &mut Canvas, p: &tiny_skia::Path, w: f32, col: Rgba, round: bool) {
    let mut paint = Paint::default();
    paint.set_color(skia(col));
    paint.anti_alias = true;
    let s = tiny_skia::Stroke { width: w, line_cap: if round { tiny_skia::LineCap::Round } else { tiny_skia::LineCap::Butt }, ..Default::default() };
    c.pm.stroke_path(p, &paint, &s, Transform::identity(), None);
}

// MultiEffect drop shadow of a layer: blurred alpha (3 box passes ~ gaussian `sigma`), tinted, offset
pub fn drop_shadow(src: &Pixmap, sigma: f32, col: Rgba, opacity: f32, dx: i32, dy: i32) -> Pixmap {
    let (w, h) = (src.width() as usize, src.height() as usize);
    let mut a: Vec<f32> = src.data().chunks_exact(4).map(|p| p[3] as f32).collect();
    let r = sigma.round().max(1.0) as usize;
    let mut tmp = vec![0f32; w.max(h)];
    for _ in 0..3 {
        for y in 0..h {
            box_line(&mut a[y * w..(y + 1) * w], 1, r, &mut tmp);
        }
        for x in 0..w {
            // column: gather, blur, scatter
            let mut col: Vec<f32> = (0..h).map(|y| a[y * w + x]).collect();
            box_line(&mut col, 1, r, &mut tmp);
            for y in 0..h {
                a[y * w + x] = col[y];
            }
        }
    }
    let mut out = Pixmap::new(w as u32, h as u32).unwrap();
    let k = col.3 * opacity / 255.0;
    let d = out.data_mut();
    for y in 0..h as i32 {
        let sy = y - dy;
        if sy < 0 || sy >= h as i32 {
            continue;
        }
        for x in 0..w as i32 {
            let sx = x - dx;
            if sx < 0 || sx >= w as i32 {
                continue;
            }
            let al = (a[sy as usize * w + sx as usize] * k).clamp(0.0, 1.0);
            if al <= 0.0 {
                continue;
            }
            let i = (y as usize * w + x as usize) * 4;
            d[i] = (col.0 * al * 255.0) as u8;
            d[i + 1] = (col.1 * al * 255.0) as u8;
            d[i + 2] = (col.2 * al * 255.0) as u8;
            d[i + 3] = (al * 255.0) as u8;
        }
    }
    out
}
fn box_line(v: &mut [f32], _stride: usize, r: usize, tmp: &mut [f32]) {
    let n = v.len();
    if n == 0 {
        return;
    }
    let win = (2 * r + 1) as f32;
    let mut sum = 0.0f32;
    for i in 0..=r.min(n - 1) {
        sum += v[i];
    }
    for i in 0..n {
        tmp[i] = sum / win;
        let add = i + r + 1;
        if add < n {
            sum += v[add];
        }
        if i >= r {
            sum -= v[i - r];
        }
    }
    v.copy_from_slice(&tmp[..n]);
}
// layer + its shadow onto the canvas at (x, y)
pub fn put_with_shadow(c: &mut Canvas, layer: &Pixmap, x: i32, y: i32, sigma: f32, col: Rgba, opacity: f32, dy: i32) {
    let sh = drop_shadow(layer, sigma, col, opacity, 0, dy);
    c.pm.draw_pixmap(x, y, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    c.pm.draw_pixmap(x, y, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

// ------------------------------------------------------------------ hits
#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    Edit(&'static str),
    Move(&'static str, i32),
    Done(&'static str),
    Opt(&'static str, &'static str, Value),
    Sheet,                 // swallows the pointer
    // weather card
    Metric(&'static str),
    Day(usize),
    Foot,
    Timeline,
    Sun,
    Moon,
    Pop,
}
pub type Hits = Vec<(f32, f32, f32, f32, Hit)>;
pub fn hit_at(h: &Hits, x: f32, y: f32) -> Option<Hit> {
    h.iter().rev().find(|r| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3).map(|r| r.4.clone())
}

// ------------------------------------------------------------------ settings (widgets.json)
pub struct Group {
    pub label: &'static str,
    pub key: &'static str,
    pub multi: bool,
    pub max: usize,
    pub min: usize,
    pub opts: Vec<(Value, &'static str)>,
}
fn g(label: &'static str, key: &'static str, multi: bool, max: usize, min: usize, opts: &[(Value, &'static str)]) -> Group {
    Group { label, key, multi, max, min, opts: opts.to_vec() }
}
pub fn groups(card: &str) -> Vec<Group> {
    let s = |x: &str| Value::String(x.into());
    match card {
        "weather" => vec![
            g("Wykres", "metric", false, 0, 0, &[(s("temp"), "Temperatura"), (s("rain"), "Opady"), (s("wind"), "Wiatr")]),
            g("Oś czasu", "hours", false, 0, 0, &[(json!(12), "12 h"), (json!(24), "24 h"), (json!(48), "48 h")]),
            g("Prognoza dni", "days", false, 0, 0, &[(s("show"), "Pokaż"), (s("hide"), "Ukryj")]),
            g("Podpowiedzi w stopce", "insights", true, 0, 1, &[(s("rain"), "Opady"), (s("run"), "Okno na bieg"), (s("wear"), "Ubiór"), (s("wind"), "Porywy"), (s("light"), "Światło dnia"), (s("tomorrow"), "Jutro")]),
        ],
        "garmin" => vec![
            g("Główna liczba", "hero", false, 0, 0, &[(s("ready"), "Gotowość"), (s("bb"), "Body Battery"), (s("sleep"), "Sen"), (s("hrv"), "HRV")]),
            g("Wartości na dole", "vitals", true, 4, 1, &[(s("sleep"), "Sen"), (s("hrv"), "HRV"), (s("rhr"), "Tętno"), (s("stress"), "Stres"), (s("steps"), "Kroki"), (s("bb"), "Battery"), (s("ready"), "Gotowość")]),
            g("Stopka", "foot", false, 0, 0, &[(s("insight"), "Podpowiedź"), (s("hide"), "Tylko po najechaniu")]),
        ],
        _ => vec![
            g("Główna liczba", "hero", false, 0, 0, &[(s("race"), "Do startu"), (s("week"), "Tydzień"), (s("year"), "Rok")]),
            g("Pokaż", "show", true, 0, 0, &[(s("strip"), "Dni do startu"), (s("workout"), "Trening dnia"), (s("p5"), "Prognoza 5 km")]),
            g("Stopka", "foot", false, 0, 0, &[(s("last"), "Ostatni bieg"), (s("week"), "Tydzień"), (s("year"), "Rok"), (s("hide"), "Ukryj")]),
        ],
    }
}
fn defaults() -> Value {
    json!({
        "order": ["garmin", "weather", "running"],
        "garmin": { "hero": "ready", "vitals": ["sleep", "hrv", "rhr", "stress"], "foot": "insight" },
        "weather": { "metric": "temp", "hours": 24, "days": "show", "insights": ["rain", "run", "wear", "wind", "light", "tomorrow"] },
        "running": { "hero": "race", "show": ["strip", "workout", "p5"], "foot": "last" }
    })
}

pub struct Cfg {
    pub v: Value,
}
impl Cfg {
    fn path() -> String {
        format!("{}/.config/mist/widgets.json", std::env::var("HOME").unwrap_or_default())
    }
    pub fn load() -> Cfg {
        Cfg { v: std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({})) }
    }
    pub fn opt(&self, card: &str, key: &str) -> Value {
        let o = &self.v[card][key];
        if o.is_null() { defaults()[card][key].clone() } else { o.clone() }
    }
    pub fn s(&self, card: &str, key: &str) -> String {
        self.opt(card, key).as_str().unwrap_or("").to_string()
    }
    pub fn list(&self, card: &str, key: &str) -> Vec<String> {
        self.opt(card, key).as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
    }
    pub fn order(&self) -> Vec<String> {
        let o: Vec<String> = self.v["order"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if o.len() == 3 && ["garmin", "weather", "running"].iter().all(|k| o.iter().any(|x| x == k)) {
            o
        } else {
            vec!["garmin".into(), "weather".into(), "running".into()]
        }
    }
    pub fn set(&mut self, card: &str, key: &str, v: Value) {
        if !self.v.is_object() {
            self.v = json!({});
        }
        if card == "order" {
            self.v["order"] = v;
        } else {
            if !self.v[card].is_object() {
                self.v[card] = json!({});
            }
            self.v[card][key] = v;
        }
        let _ = std::fs::write(Self::path(), serde_json::to_string_pretty(&self.v).unwrap_or_default());
    }
    pub fn mv(&mut self, card: &str, d: i32) {
        let mut o = self.order();
        let Some(i) = o.iter().position(|x| x == card) else { return };
        let j = i as i32 + d;
        if j < 0 || j >= o.len() as i32 {
            return;
        }
        o.swap(i, j as usize);
        self.set("order", "", json!(o));
    }
    // a chip of a group clicked
    pub fn click(&mut self, card: &str, gr: &Group, v: &Value) {
        if !gr.multi {
            self.set(card, gr.key, v.clone());
            return;
        }
        let cur = self.opt(card, gr.key);
        let mut a: Vec<Value> = cur.as_array().cloned().unwrap_or_default();
        if a.contains(v) {
            a.retain(|x| x != v);
        } else {
            a.push(v.clone());
            if gr.max > 0 && a.len() > gr.max {
                a.remove(0);
            }
        }
        if !a.is_empty() || gr.min == 0 {
            self.set(card, gr.key, Value::Array(a));
        }
    }
}

// ------------------------------------------------------------------ shared card parts
pub struct Fx {
    pub anims: HashMap<String, Anim>,
}
impl Fx {
    pub fn new() -> Fx {
        Fx { anims: HashMap::new() }
    }
    // a Behavior: the value moving to `to` over `dur` ms (starts at `to` the first time)
    pub fn v(&mut self, key: &str, to: f32, dur: f32, ease: Ease) -> f32 {
        let a = self.anims.entry(key.to_string()).or_insert_with(|| Anim::new(to));
        a.set(to, dur, ease);
        a.value()
    }
    pub fn from(&mut self, key: &str, from: f32, to: f32, dur: f32) {
        let mut a = Anim::new(from);
        a.set(to, dur, Ease::Linear);
        self.anims.insert(key.to_string(), a);
    }
    pub fn running(&self) -> bool {
        self.anims.values().any(|a| a.running())
    }
}

// round adjust button (shown while the card is hovered)
pub fn edit_btn(c: &mut Canvas, t: &mut Text, fx: &mut Fx, hits: &mut Hits, key: &str, x: f32, y: f32, shown: bool, hovered: bool, card: &'static str) {
    let o = fx.v(&format!("{key}.eb"), if shown { 1.0 } else { 0.0 }, 160.0, Ease::Linear);
    if o <= 0.0 {
        return;
    }
    rect(c, x, y, 26.0, 26.0, 13.0, WHITE.alpha((if hovered { 0.2 } else { 0.11 }) * o));
    border(c, x, y, 26.0, 26.0, 13.0, 1.0, WHITE.alpha(0.12 * o));
    c.icon(t, x + 13.0, y + 13.0, "adjust", 14.0, FG.alpha(FG.3 * o));
    if shown {
        hits.push((x, y, 26.0, 26.0, Hit::Edit(card)));
    }
}

// one choice chip; returns its width
pub fn chip(c: &mut Canvas, t: &mut Text, x: f32, y: f32, label: &str, on: bool, hovered: bool, o: f32) -> f32 {
    let sty = st(12.0, if on { 650.0 } else { 550.0 });
    let tw = width(t, label, &sty);
    let w = tw.round() + 20.0;
    rect(c, x, y, w, 26.0, 13.0, if on { ACC.alpha(0.22 * o) } else { WHITE.alpha((if hovered { 0.1 } else { 0.05 }) * o) });
    border(c, x, y, w, 26.0, 13.0, 1.0, if on { ACC.alpha(0.7 * o) } else { WHITE.alpha(0.08 * o) });
    let lh = line_h(t, &sty);
    let col = if on { FG } else { FG2 };
    text(c, t, x + (w - tw) / 2.0, y + (26.0 - lh) / 2.0, label, &sty, col.alpha(col.3 * o), true, None);
    w
}
pub fn chip_w(t: &mut Text, label: &str, on: bool) -> f32 {
    width(t, label, &st(12.0, if on { 650.0 } else { 550.0 })).round() + 20.0
}

pub fn caps_style() -> Style {
    st(11.0, 650.0).spaced(1.3)
}

// the back of a card: its options as chip groups, card order, done (drawn into its own layer, scaled in)
#[allow(clippy::too_many_arguments)]
pub fn edit_sheet(c: &mut Canvas, t: &mut Text, fx: &mut Fx, hits: &mut Hits, cfg: &Cfg, card: &'static str, title: &str, x: f32, y: f32, w: f32, h: f32, open: bool, ptr: Option<(f32, f32)>) {
    let o = fx.v(&format!("{card}.sheet.o"), if open { 1.0 } else { 0.0 }, 180.0, Ease::Linear);
    let s = fx.v(&format!("{card}.sheet.s"), if open { 1.0 } else { 0.97 }, 220.0, Ease::OutCubic);
    if o <= 0.0 {
        return;
    }
    let mut l = Canvas::new(w.ceil() as u32, h.ceil() as u32);
    let mut lh: Hits = Vec::new();
    let lp = ptr.map(|p| (p.0 - x, p.1 - y));
    let hov = |r: (f32, f32, f32, f32)| lp.map(|p| p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3).unwrap_or(false);
    rect(&mut l, 0.0, 0.0, w, h, 26.0, Rgba(9.0 / 255.0, 13.0 / 255.0, 24.0 / 255.0, 0.95));
    border(&mut l, 0.0, 0.0, w, h, 26.0, 1.0, WHITE.alpha(0.1));
    lh.push((0.0, 0.0, w, h, Hit::Sheet));
    // header: title · ‹ › · Gotowe
    let order = cfg.order();
    let i = order.iter().position(|k| k == card).unwrap_or(0);
    let done_w = chip_w(t, "Gotowe", true);
    let mut rx = w - 20.0 - done_w;
    chip(&mut l, t, rx, 16.0, "Gotowe", true, hov((rx, 16.0, done_w, 26.0)), 1.0);
    lh.push((rx, 16.0, done_w, 26.0, Hit::Done(card)));
    for (icon, d) in [("chevron", 1), ("chevLeft", -1)] {
        rx -= 6.0 + 26.0;
        let can = if d < 0 { i > 0 } else { i + 1 < order.len() };
        let a = if can { 1.0 } else { 0.3 };
        rect(&mut l, rx, 16.0, 26.0, 26.0, 13.0, WHITE.alpha((if hov((rx, 16.0, 26.0, 26.0)) && can { 0.14 } else { 0.06 }) * a));
        l.icon(t, rx + 13.0, 29.0, icon, 14.0, FG.alpha(FG.3 * a));
        if can {
            lh.push((rx, 16.0, 26.0, 26.0, Hit::Move(card, d)));
        }
    }
    let cs = caps_style();
    let clh = line_h(t, &cs);
    text(&mut l, t, 20.0, 16.0 + (26.0 - clh) / 2.0, &title.to_uppercase(), &cs, FG2, true, Some(rx - 6.0 - 20.0));
    // groups
    let mut gy = 56.0;
    for gr in groups(card) {
        let cur = cfg.opt(card, gr.key);
        let ls = st(11.0, 600.0);
        let label = if gr.multi && gr.max > 0 { format!("{}  ·  maks. {}", gr.label, gr.max) } else { gr.label.to_string() };
        text(&mut l, t, 20.0, gy, &label, &ls, MUTED, true, None);
        let mut cy = gy + line_h(t, &ls) + 6.0;
        let mut cx = 20.0;
        for (v, lab) in &gr.opts {
            let on = if gr.multi { cur.as_array().map(|a| a.contains(v)).unwrap_or(false) } else { &cur == v };
            let cw = chip_w(t, lab, on);
            if cx > 20.0 && cx + cw > w - 20.0 {
                cx = 20.0;
                cy += 26.0 + 6.0;
            }
            chip(&mut l, t, cx, cy, lab, on, hov((cx, cy, cw, 26.0)), 1.0);
            lh.push((cx, cy, cw, 26.0, Hit::Opt(card, gr.key, v.clone())));
            cx += cw + 6.0;
        }
        gy = cy + 26.0 + 12.0;
    }
    // scale about the centre, fade
    let tf = Transform::from_translate(x + w / 2.0, y + h / 2.0).pre_scale(s, s).pre_translate(-w / 2.0, -h / 2.0);
    let paint = PixmapPaint { opacity: o, quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &paint, tf, None);
    if open {
        for r in lh {
            hits.push((x + r.0, y + r.1, r.2, r.3, r.4));
        }
    }
}

// card text row helpers
pub fn caps(c: &mut Canvas, t: &mut Text, x: f32, y: f32, s: &str, col: Rgba, max: Option<f32>) -> f32 {
    text(c, t, x, y, &s.to_uppercase(), &caps_style(), col, true, max)
}
pub fn hero_style() -> Style {
    st(58.0, 300.0).spaced(-2.5).tnum()
}

// Footer: hairline above, icon + one sentence
pub fn footer(c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32, icon: &str, s: &str, col: Rgba) {
    rect(c, x, y - 11.0, w, 1.0, 0.0, HAIR);
    let sty = st(13.0, 600.0);
    let lh = line_h(t, &sty);
    let mut tx = x;
    if !icon.is_empty() {
        c.icon(t, x + 7.0, y + 12.0, icon, 14.0, col);
        tx += Canvas::icon_w(t, icon, 14.0).max(14.0) + 8.0;
    }
    text(c, t, tx, y + (24.0 - lh) / 2.0, s, &sty, FG, true, Some(w - 24.0));
}

// ------------------------------------------------------------------ the stage
pub struct Desk {
    pub stage: Option<LayerSurface>,
    pub stage_cfg: bool,
    pub card: Option<LayerSurface>,
    pub card_cfg: bool,
    pub out_w: i32,
    pub weather: Value,
    pub garmin: Value,
    pub running: Value,
    pub cfg: Cfg,
    pub fx: Fx,
    pub hits: Hits,
    pub ptr: Option<(f32, f32)>,
    pub editing: HashMap<&'static str, bool>,
    pub dirty: bool,
    pub card_dirty: bool,
    top: Option<(i64, Pixmap)>,
    pub wx: crate::wxcard::Wx,
    pub canvas: Canvas,
    pub card_canvas: Canvas,
    pub sky_written: String,
    pub cursor_hand: bool,
    bento_y: f32,
}

pub fn stage_h(t: &mut Text) -> f32 {
    top_h(t) + 16.0 + 20.0 + 40.0 + BENTO_H
}
// date + clock (line boxes; the clock's padding -14 / -20)
fn top_h(t: &mut Text) -> f32 {
    line_h(t, &st(22.0, 600.0)) + line_h(t, &st(150.0, 620.0)) - 34.0
}

impl Desk {
    pub fn new() -> Desk {
        Desk {
            stage: None,
            stage_cfg: false,
            card: None,
            card_cfg: false,
            out_w: 1920,
            weather: Value::Null,
            garmin: Value::Null,
            running: Value::Null,
            cfg: Cfg::load(),
            fx: Fx::new(),
            hits: Vec::new(),
            ptr: None,
            editing: HashMap::new(),
            dirty: true,
            card_dirty: true,
            top: None,
            wx: crate::wxcard::Wx::new(),
            canvas: Canvas::new(1, 1),
            card_canvas: Canvas::new(1, 1),
            sky_written: String::new(),
            cursor_hand: false,
            bento_y: 0.0,
        }
    }
    pub fn editing(&self, card: &str) -> bool {
        *self.editing.get(card).unwrap_or(&false)
    }
    pub fn animating(&self) -> bool {
        self.fx.running() || self.wx.fx.running()
    }

    // x of each card in the bento row
    pub fn slots(&self) -> Vec<(String, f32, f32)> {
        let mut x = 0.0;
        self.cfg.order().into_iter().map(|k| {
            let w = if k == "weather" { WX_W } else { COL_W };
            let r = (k, x, w);
            x += w + BENTO_GAP;
            r
        }).collect()
    }
    // the weather card on the monitor (top-left), from the stage layout
    pub fn card_at(&self, t: &mut Text) -> (i32, i32) {
        let sx = self.slots().into_iter().find(|s| s.0 == "weather").map(|s| s.1).unwrap_or(COL_W + BENTO_GAP);
        let wx = (self.out_w - STAGE_W as i32) / 2 + 40 + sx as i32;
        let wy = STAGE_TOP + (stage_h(t) - BENTO_H) as i32;
        (wx, wy)
    }
    // tell mist-wall where the sky goes (only when it changed)
    pub fn write_sky(&mut self, t: &mut Text) {
        let (x, y) = self.card_at(t);
        let s = json!({ "show": true, "x": x + 1, "y": y + 1, "w": WX_W as i32 - 2, "h": BENTO_H as i32 - 2 }).to_string();
        if s != self.sky_written {
            let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
            let tmp = format!("{dir}/.sky.json");
            if std::fs::write(&tmp, &s).is_ok() && std::fs::rename(&tmp, format!("{dir}/sky.json")).is_ok() {
                self.sky_written = s;
            }
        }
    }

    // ---------------------------------------------------------------- paint: the stage window
    pub fn paint_stage(&mut self, t: &mut Text) {
        let h = stage_h(t) + 60.0;
        if self.canvas.pm.width() != STAGE_W || self.canvas.pm.height() != h as u32 {
            self.canvas = Canvas::new(STAGE_W, h as u32);
        }
        let mut c = std::mem::replace(&mut self.canvas, Canvas::new(1, 1));
        c.clear();
        self.hits.clear();
        let col_x = 40.0;
        // date, clock, sun path (cached per minute: their shadows are blurred)
        let minute = crate::data::now() / 60;
        let key = minute * 100000 + self.weather["sunrise"].as_str().map(|s| s.len() as i64).unwrap_or(0);
        if self.top.as_ref().map(|t| t.0 != key).unwrap_or(true) {
            self.top = Some((key, self.paint_top(t)));
        }
        if let Some((_, pm)) = &self.top {
            c.pm.draw_pixmap(0, 0, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        let by = stage_h(t) - BENTO_H;
        self.bento_y = by;
        for (k, x, w) in self.slots() {
            let (x, y) = (col_x + x, by);
            match k.as_str() {
                "garmin" => self.garmin_card(&mut c, t, x, y, w),
                "running" => self.running_card(&mut c, t, x, y, w),
                _ => {}
            }
        }
        self.canvas = c;
    }

    // the top of the stage: date, clock (MultiEffect shadows) and the sun's path
    fn paint_top(&mut self, t: &mut Text) -> Pixmap {
        let w = STAGE_W as f32;
        let hh = top_h(t) + 16.0 + 20.0 + 60.0;
        let mut out = Canvas::new(STAGE_W, hh as u32);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        const DAYS: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];
        let date = format!("{}, {} {}", DAYS[tm.tm_wday as usize], tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]);
        let ds = st(22.0, 600.0);
        let dh = line_h(t, &ds);
        // date: shadow black .25, blur .5 (of 32), 1 px down
        {
            let mut l = Canvas::new(STAGE_W, (dh + 40.0) as u32);
            let dw = width(t, &date, &ds);
            text(&mut l, t, ((w - dw) / 2.0).round(), 20.0, &date, &ds, WHITE.alpha(0.82), false, None);
            put_with_shadow(&mut out, &l.pm, 0, -20, 6.4, Rgba(0.0, 0.0, 0.0, 1.0), 0.25, 1);
        }
        // clock: shadow rgb(2, 6, 16) .22, blur 1 (of 48), 6 px down
        {
            let cs = st(150.0, 620.0).spaced(-4.0).tnum();
            let s = crate::data::hm(now);
            let cw = width(t, &s, &cs);
            let mut l = Canvas::new(STAGE_W, (line_h(t, &cs) + 120.0) as u32);
            text(&mut l, t, ((w - cw) / 2.0).round(), 60.0, &s, &cs, WHITE.alpha(0.86), false, None);
            put_with_shadow(&mut out, &l.pm, 0, (dh - 14.0 - 60.0) as i32, 19.2, Rgba(2.0 / 255.0, 6.0 / 255.0, 16.0 / 255.0, 1.0), 0.22, 6);
        }
        // the sun's path: rise · line with a dot for now · set (at night: set -> next rise)
        let (rise, set) = (self.weather["sunrise"].as_str().unwrap_or(""), self.weather["sunset"].as_str().unwrap_or(""));
        if !rise.is_empty() {
            let mins = |s: &str| -> i32 {
                let mut p = s.split(':');
                p.next().and_then(|x| x.parse::<i32>().ok()).unwrap_or(0) * 60 + p.next().and_then(|x| x.parse::<i32>().ok()).unwrap_or(0)
            };
            let (r0, s0) = (mins(rise), mins(set));
            let n0 = tm.tm_hour * 60 + tm.tm_min;
            let day = n0 >= r0 && n0 < s0;
            let p = if day { (n0 - r0) as f32 / (s0 - r0).max(1) as f32 } else { ((n0 - s0 + 1440) % 1440) as f32 / (1440 - (s0 - r0)).max(1) as f32 };
            let ts = st(13.0, 500.0).tnum();
            let t2 = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.6);
            let a = (if day { rise } else { set }).trim_start_matches('0').to_string();
            let b = (if day { set } else { rise }).trim_start_matches('0').to_string();
            let (aw, bw) = (width(t, &a, &ts), width(t, &b, &ts));
            let rw = aw + 12.0 + 120.0 + 12.0 + bw;
            let ay = top_h(t) + 16.0;
            let x0 = ((w - rw) / 2.0).round();
            let lh = line_h(t, &ts);
            text(&mut out, t, x0, ay + (20.0 - lh) / 2.0, &a, &ts, t2, true, None);
            let lx = x0 + aw + 12.0;
            let ly = ay + 10.0;
            rect(&mut out, lx, ly - 0.5, 120.0, 1.0, 0.0, WHITE.alpha(0.22));
            rect(&mut out, lx, ly - 0.5, 120.0 * p, 1.0, 0.0, WHITE.alpha(0.7));
            circle(&mut out, lx + 120.0 * p, ly, 3.0, WHITE);
            text(&mut out, t, lx + 132.0, ay + (20.0 - lh) / 2.0, &b, &ts, t2, true, None);
        }
        out.pm
    }

    // CardGlass: glass, inner glow, (content), rim
    pub fn card_glass(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, glow: Option<(Rgba, f32, f32, f32)>) {
        Glass { radius: 26.0, tint: 0.34, rims: false, shadow_opacity: 0.34, ..Default::default() }.paint(c, x, y, w, h);
        if let Some((gc, gx, gy, gs)) = glow {
            if let Some(p) = crate::gfx::rrect_path(x + 1.0, y + 1.0, w - 2.0, h - 2.0, 25.0) {
                let (cx, cy, r) = (x + w * gx, y + h * gy, w.max(h) * gs);
                fill_rad(c, &p, cx, cy, r, &[(0.0, gc), (0.45, gc.alpha(gc.3 * 0.35)), (1.0, gc.alpha(0.0))]);
            }
        }
    }
    pub fn card_rim(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
        Glass { radius: 26.0, rim_only: true, corners: true, ..Default::default() }.paint(c, x, y, w, h);
    }

    fn inside(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.ptr.map(|p| p.0 >= x && p.0 < x + w && p.1 >= y && p.1 < y + h).unwrap_or(false)
    }

    // ---------------- body (Garmin): how ready am I today? One number, the day's energy curve, four vitals
    fn garmin_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let g = self.garmin.clone();
        let editing = self.editing("garmin");
        let hov = self.inside(x, y, w, BENTO_H);
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let bb = num(&g["bb"]);
        let bb_col = level(bb);
        let ready = num(&g["ready"]);
        let series: Vec<f64> = g["bbSeries"].as_array().map(|a| a.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default();
        let hrv_delta = match (num(&g["hrv"]), num(&g["hrv7"])) { (Some(a), Some(b)) if a != 0.0 && b != 0.0 => a - b, _ => 0.0 };
        let sleep_sec = num(&g["sleepSec"]).unwrap_or(0.0);
        let sleep_h = sleep_sec / 3600.0;
        let sleep_txt = if sleep_sec > 0.0 { format!("{}:{:02}", (sleep_sec / 3600.0).floor() as i64, ((sleep_sec % 3600.0) / 60.0).floor() as i64) } else { "–".into() };
        let stress = num(&g["stress"]);
        let stress_word = match stress { None => "", Some(s) if s <= 25.0 => "spokój", Some(s) if s <= 50.0 => "niski", Some(s) if s <= 75.0 => "średni", _ => "wysoki" };
        let sleep_score = num(&g["sleepScore"]);
        let hero_key = self.cfg.s("garmin", "hero");
        let hero: (String, String, String, String, Rgba) = match hero_key.as_str() {
            "bb" => ("Body Battery".into(), ns(&g["bb"]), (if bb.unwrap_or(0.0) >= 50.0 { "naładowana" } else if bb.unwrap_or(0.0) >= 25.0 { "średnio" } else { "nisko" }).into(), "na 100".into(), bb_col),
            "sleep" => ("Sen".into(), sleep_txt.clone(), (if sleep_score.unwrap_or(0.0) >= 80.0 { "dobry" } else if sleep_score.unwrap_or(0.0) >= 60.0 { "przeciętny" } else { "słaby" }).into(), sleep_score.map(|s| format!("ocena {s}")).unwrap_or_default(), level(sleep_score)),
            "hrv" => ("HRV".into(), ns(&g["hrv"]), "ms".into(), format!("średnia 7 dni {}", ns(&g["hrv7"])), if hrv_delta >= -4.0 { ACC } else { WARN }),
            _ => ("Gotowość".into(), ns(&g["ready"]), g["level"].as_str().unwrap_or("").into(), "na 100".into(), level(ready)),
        };
        let steps = num(&g["steps"]);
        let vital = |k: &str| -> Option<(&'static str, String, &'static str, &'static str, bool)> {
            Some(match k {
                "sleep" => ("sen", sleep_txt.clone(), "", "sen", sleep_sec > 0.0 && sleep_h < 6.5),
                "hrv" => ("hrv", ns(&g["hrv"]), "ms", "HRV", hrv_delta <= -5.0),
                "rhr" => ("hr", ns(&g["rhr"]), "bpm", "tętno", false),
                "stress" => ("st", ns(&g["stress"]), "", "stres", stress.unwrap_or(0.0) > 50.0),
                "steps" => ("steps", steps.map(|s| if s >= 10000.0 { format!("{:.1}k", s / 1000.0).replace('.', ",") } else { thousands(s) }).unwrap_or("–".into()), "", "kroki", false),
                "bb" => ("bb", ns(&g["bb"]), "", "battery", bb.map(|b| b < 25.0).unwrap_or(false)),
                "ready" => ("ready", ns(&g["ready"]), "", "gotowość", ready.map(|r| r < 40.0).unwrap_or(false)),
                _ => return None,
            })
        };
        let vitals: Vec<_> = self.cfg.list("garmin", "vitals").iter().filter_map(|k| vital(k)).collect();
        // the part under the pointer -> the footer explains it
        let mut hot = String::new();
        if !editing {
            if self.inside(cx, cy + BAND_Y - 14.0, cw, 70.0) {
                hot = "bb".into();
            }
            let vw = cw / vitals.len().max(1) as f32;
            for (i, v) in vitals.iter().enumerate() {
                if self.inside(cx + i as f32 * vw, cy + BAND_Y + 72.0, vw, 44.0) {
                    hot = v.0.into();
                }
            }
        }
        let insight: (&str, Rgba, String) = match hot.as_str() {
            "bb" => ("bolt", bb_col, if series.len() > 1 { format!("Body Battery {} · 24 h: maks {}, min {}", ns(&g["bb"]), series.iter().cloned().fold(f64::MIN, f64::max), series.iter().cloned().fold(f64::MAX, f64::min)) } else { format!("Body Battery {}", ns(&g["bb"])) }),
            "sen" => ("zzz", level(sleep_score), format!("Sen {}{}", sleep_txt.replace(':', " h "), sleep_score.map(|s| format!(" · ocena {s}")).unwrap_or_default())),
            "hrv" => ("activity", if hrv_delta >= 0.0 { ACC } else { WARN }, format!("HRV {} ms · średnia 7 dni {}", ns(&g["hrv"]), ns(&g["hrv7"]))),
            "hr" => ("heart", ACC, format!("Tętno spoczynkowe {} bpm", ns(&g["rhr"]))),
            "steps" => ("walk", ACC, format!("Kroki dziś: {}", thousands(steps.unwrap_or(0.0)))),
            "ready" => ("gauge", hero.4, format!("Gotowość {} · {}", ns(&g["ready"]), g["level"].as_str().unwrap_or(""))),
            "st" => ("leaf", if stress.unwrap_or(0.0) > 50.0 { WARN } else { ACC }, format!("Stres {} · {}", ns(&g["stress"]), stress_word)),
            _ if !g["error"].is_null() => ("alert", WARN, "Brak połączenia z Garmin".into()),
            _ if bb.map(|b| b < 25.0).unwrap_or(false) => ("bolt", BAD, "Mało energii — odpuść intensywność".into()),
            _ if hrv_delta <= -5.0 => ("activity", WARN, format!("HRV {} ms poniżej średniej 7 dni", hrv_delta.abs())),
            _ if sleep_sec > 0.0 && sleep_h < 6.5 => ("zzz", WARN, format!("Krótka noc: {}", sleep_txt.replace(':', " h "))),
            _ if stress.unwrap_or(0.0) > 50.0 => ("leaf", WARN, "Podwyższony stres — zwolnij".into()),
            _ if ready.unwrap_or(0.0) >= 70.0 => ("check", ACC, "Gotowy na mocny trening".into()),
            _ => ("leaf", ACC, "Dzień na spokojne tempo".into()),
        };
        Self::card_glass(c, x, y, w, BENTO_H, Some((hero.4.alpha(0.20), 0.18, 0.22, 0.7)));
        // header: caps title · steps (fades out while hovered: the adjust button sits there)
        caps(c, t, cx, cy, &hero.0, FAINT, None);
        let ro = self.fx.v("garmin.hdr", if hov && !editing { 0.0 } else { 1.0 }, 160.0, Ease::Linear);
        let right = if g["stale"].as_bool().unwrap_or(false) { format!("dane z {}", crate::data::hm(g["at"].as_i64().unwrap_or(0))) } else { steps.map(|s| format!("{} kroków", thousands(s))).unwrap_or_default() };
        if ro > 0.0 && !right.is_empty() {
            let rs = st(11.0, 550.0);
            let rw = width(t, &right, &rs);
            text(c, t, cx + cw - rw, cy, &right, &rs, MUTED.alpha(MUTED.3 * ro), true, None);
        }
        let ptr = self.ptr;
        let eb_hov = ptr.map(|p| p.0 >= cx + cw - 26.0 && p.0 < cx + cw && p.1 >= cy - 6.0 && p.1 < cy + 20.0).unwrap_or(false);
        edit_btn(c, t, &mut self.fx, &mut self.hits, "garmin", cx + cw - 26.0, cy - 6.0, hov && !editing, eb_hov, "garmin");
        // hero + qualifier
        let hs = hero_style();
        let hw = text(c, t, cx, cy + HERO_Y, &hero.1, &hs, FG, true, None);
        let base = cy + HERO_Y + ascent(t, &hs);
        let q1 = st(15.0, 650.0);
        text(c, t, cx + hw + 10.0, base - 20.0, &hero.2, &q1, hero.4, true, None);
        if !hero.3.is_empty() {
            { let q1h = line_h(t, &q1); text(c, t, cx + hw + 10.0, base - 20.0 + q1h, &hero.3, &st(11.0, 500.0), MUTED, true, None); }
        }
        // Body Battery, the last 24 h
        {
            let (bx, by) = (cx, cy + BAND_Y - 14.0);
            let ls = st(11.0, 550.0);
            let lw = width(t, "Body Battery", &ls);
            text(c, t, bx + cw - lw, by, "Body Battery", &ls, MUTED, true, None);
            let (ox, oy, ch) = (bx, by + 16.0, 54.0);
            let n = series.len();
            if n >= 2 {
                let (pw, ph) = (cw - 36.0, ch - 8.0);
                let px = |i: usize| ox + i as f32 / (n - 1) as f32 * pw + 2.0;
                let py = |v: f64| oy + 4.0 + ph - v as f32 / 100.0 * ph;
                let mut pb = PathBuilder::new();
                pb.move_to(px(0), py(series[0]));
                for i in 1..n {
                    let xm = (px(i - 1) + px(i)) / 2.0;
                    pb.cubic_to(xm, py(series[i - 1]), xm, py(series[i]), px(i), py(series[i]));
                }
                let line = pb.clone().finish();
                pb.line_to(px(n - 1), oy + ch);
                pb.line_to(px(0), oy + ch);
                pb.close();
                if let Some(area) = pb.finish() {
                    fill_lin(c, &area, (0.0, oy), (0.0, oy + ch), &[(0.0, bb_col.alpha(0.22)), (1.0, bb_col.alpha(0.0))]);
                }
                if let Some(l) = line {
                    stroke(c, &l, 2.0, bb_col.alpha(0.95), true);
                }
                // the last value: a lit dot and its number
                let (lx, ly) = (px(n - 1), py(series[n - 1]));
                for (r, a) in [(6.5, 0.12), (5.0, 0.22), (4.0, 0.35)] {
                    circle(c, lx, ly, r, bb_col.alpha(a * 0.8));
                }
                circle(c, lx, ly, 3.2, WHITE);
                let vs = st(13.0, 600.0);
                let vh = line_h(t, &vs);
                let vy = ly.clamp(oy + 10.0, oy + ch - 8.0);
                text(c, t, lx + 9.0, vy - vh / 2.0, &format!("{}", series[n - 1] as i64), &vs, bb_col, false, None);
            }
        }
        // four vitals in one quiet strip; a dot flags the one that is off
        {
            let vw = cw / vitals.len().max(1) as f32;
            let vy = cy + BAND_Y + 72.0;
            for (i, v) in vitals.iter().enumerate() {
                let vx = cx + i as f32 * vw;
                let vs = st(18.0, 550.0).tnum();
                let w1 = text(c, t, vx, vy, &v.1, &vs, if hot == v.0 { ACC } else { FG }, true, None);
                if !v.2.is_empty() {
                    let us = st(11.0, 500.0);
                    let b = vy + ascent(t, &vs) - ascent(t, &us);
                    text(c, t, vx + w1 + 2.0, b, v.2, &us, MUTED, true, None);
                }
                let ls = st(11.0, 550.0);
                let lh = line_h(t, &ls);
                let mut lx = vx;
                if v.4 {
                    circle(c, vx + 2.5, vy + 25.0 + lh / 2.0, 2.5, WARN);
                    lx += 5.0 + 5.0;
                }
                text(c, t, lx, vy + 25.0, v.3, &ls, MUTED, true, None);
            }
        }
        if self.cfg.s("garmin", "foot") == "insight" || !hot.is_empty() {
            footer(c, t, cx, cy + FOOT_Y, cw, insight.0, &insight.2, insight.1);
        }
        Self::card_rim(c, x, y, w, BENTO_H);
        edit_sheet(c, t, &mut self.fx, &mut self.hits, &self.cfg, "garmin", "Garmin", x, y, w, BENTO_H, editing, ptr);
    }

    // ---------------- training: the race countdown, the days left, today's session, the last run
    fn running_card(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32) {
        let r = self.running.clone();
        let g = self.garmin.clone();
        let editing = self.editing("running");
        let hov = self.inside(x, y, w, BENTO_H);
        let (cx, cy, cw) = (x + PAD, y + PAD, w - 2.0 * PAD);
        let race_days = r["raceDays"].as_i64();
        let race = race_days.is_some();
        let week: Vec<f64> = r["week"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()).unwrap_or(vec![0.0; 7]);
        let now = crate::data::now();
        let tm = crate::data::local(now);
        let year = tm.tm_year + 1900;
        let show = self.cfg.list("running", "show");
        let hk = self.cfg.s("running", "hero");
        let hero_key = if race || hk != "race" { hk } else { "week".into() };
        let foot = self.cfg.s("running", "foot");
        let year_n = r["yearN"].as_i64().unwrap_or(0);
        let hero: (String, String, String, String) = match hero_key.as_str() {
            "year" => (format!("Rok {year}"), km1(r["yearKm"].as_f64().unwrap_or(0.0)), "km".into(), format!("{year_n} {}", runs_word(year_n))),
            "week" => (format!("Tydzień {}", r["isoWeek"].as_i64().map(|w| w.to_string()).unwrap_or_default()), km1(r["weekKm"].as_f64().unwrap_or(0.0)), "km".into(), format!("poprzedni {} km", km1(r["prevKm"].as_f64().unwrap_or(0.0)))),
            _ => ("Start · 5 km".into(), race_days.map(|d| d.to_string()).unwrap_or_default(), (if race_days == Some(1) { "dzień" } else { "dni" }).into(), "do startu".into()),
        };
        let insight: (&str, Rgba, String) = match foot.as_str() {
            "year" => ("trophy", ACC, format!("Rok {year}: {} km · {year_n} {}", km1(r["yearKm"].as_f64().unwrap_or(0.0)), runs_word(year_n))),
            "week" => ("calendar", ACC, format!("Tydzień {} km · poprzedni {}", km1(r["weekKm"].as_f64().unwrap_or(0.0)), km1(r["prevKm"].as_f64().unwrap_or(0.0)))),
            _ => match r["last"].as_object() {
                None => ("run", MUTED, "Brak biegów w historii".into()),
                Some(l) => ("run", MUTED, format!("Ostatni bieg {} km · {}", km1(l.get("km").and_then(|v| v.as_f64()).unwrap_or(0.0)), l.get("ago").and_then(|v| v.as_str()).unwrap_or(""))),
            },
        };
        Self::card_glass(c, x, y, w, BENTO_H, Some((Rgba(157.0 / 255.0, 189.0 / 255.0, 240.0 / 255.0, 0.16), 0.15, 0.2, 0.75)));
        caps(c, t, cx, cy, &hero.0, FAINT, None);
        if r["yearKm"].as_f64().is_some() {
            let ro = self.fx.v("running.hdr", if hov && !editing { 0.0 } else { 1.0 }, 160.0, Ease::Linear);
            let right = if hero_key == "year" || foot == "year" {
                if race && hero_key != "race" { format!("start za {} d", race_days.unwrap_or(0)) } else { format!("tydzień {}", r["isoWeek"].as_i64().map(|w| w.to_string()).unwrap_or_default()) }
            } else {
                format!("rok {} km", km1(r["yearKm"].as_f64().unwrap_or(0.0)))
            };
            if ro > 0.0 {
                let rs = st(11.0, 550.0);
                let rw = width(t, &right, &rs);
                text(c, t, cx + cw - rw, cy, &right, &rs, MUTED.alpha(MUTED.3 * ro), true, None);
            }
        }
        let ptr = self.ptr;
        let eb_hov = ptr.map(|p| p.0 >= cx + cw - 26.0 && p.0 < cx + cw && p.1 >= cy - 6.0 && p.1 < cy + 20.0).unwrap_or(false);
        edit_btn(c, t, &mut self.fx, &mut self.hits, "running", cx + cw - 26.0, cy - 6.0, hov && !editing, eb_hov, "running");
        let hs = hero_style();
        let hw = text(c, t, cx, cy + HERO_Y, &hero.1, &hs, FG, true, None);
        let base = cy + HERO_Y + ascent(t, &hs);
        let q1 = st(15.0, 650.0);
        text(c, t, cx + hw + 10.0, base - 20.0, &hero.2, &q1, ACC, true, None);
        { let q1h = line_h(t, &q1); text(c, t, cx + hw + 10.0, base - 20.0 + q1h, &hero.3, &st(11.0, 500.0), MUTED, true, None); }
        if let Some(p5) = r["p5"].as_str().filter(|s| !s.is_empty() && show.iter().any(|x| x == "p5")) {
            let ps = st(24.0, 400.0).tnum();
            let pw = width(t, p5, &ps);
            text(c, t, cx + cw - pw, base - 20.0, p5, &ps, FG, true, None);
            let ls = st(11.0, 500.0);
            let lw = width(t, "prognoza", &ls);
            let psh = line_h(t, &ps);
            text(c, t, cx + cw - lw, base - 20.0 + psh, "prognoza", &ls, MUTED, true, None);
        }
        // the days that are left (today ringed, race day flagged), else this week
        let strip_on = show.iter().any(|x| x == "strip");
        if strip_on {
            let ti = r["today"].as_i64().unwrap_or(((tm.tm_wday + 6) % 7) as i64);
            let n = if race { (race_days.unwrap_or(0) + 1).min(10) } else { 7 };
            let cwid = cw / n.max(1) as f32;
            let sy = cy + BAND_Y + 12.0;
            for i in 0..n {
                let off = if race { i } else { i - ti };
                let wd = ((tm.tm_wday as i64 + off).rem_euclid(7)) as usize;
                let wi = ti + off;
                let today = off == 0;
                let is_race = race && i == n - 1 && Some(off) == race_days;
                let km = if wi >= 0 && wi < 7 && off <= 0 { week.get(wi as usize).copied().unwrap_or(0.0) } else { 0.0 };
                let ccx = cx + i as f32 * cwid + cwid / 2.0;
                let ls = st(11.0, if today { 700.0 } else { 550.0 });
                let l = ["N", "P", "W", "Ś", "C", "P", "S"][wd];
                let lw = width(t, l, &ls);
                text(c, t, (ccx - lw / 2.0).round(), sy, l, &ls, if today { ACC } else if is_race { WARN } else { MUTED }, true, None);
                let (rx, ry) = (ccx - 10.0, sy + 18.0);
                let fillc = if km > 0.0 { Some(ACC.alpha(0.7)) } else if is_race { Some(WARN.alpha(0.18)) } else { None };
                if let Some(f) = fillc {
                    rect(c, rx, ry, 20.0, 20.0, 10.0, f);
                }
                if today {
                    border(c, rx, ry, 20.0, 20.0, 10.0, 1.5, ACC);
                }
                if !today && !is_race && km <= 0.0 {
                    circle(c, ccx, ry + 10.0, 2.0, WHITE.alpha(0.28));
                }
                if is_race {
                    c.icon(t, ccx, ry + 10.0, "flag", 12.0, WARN);
                }
            }
        }
        // today's session (Garmin plan)
        if let Some(wk) = g["workout"].as_str().filter(|s| !s.is_empty() && show.iter().any(|x| x == "workout")) {
            let wy = if strip_on { cy + BAND_Y + 60.0 } else { cy + BAND_Y + 10.0 };
            caps(c, t, cx, wy, "Dziś", ACC, None);
            text(c, t, cx, wy + 16.0, wk, &st(16.0, 650.0), FG, true, Some(cw));
            let ds = st(11.0, 500.0);
            let lines = crate::gfx::wrap(t, g["workoutDetail"].as_str().unwrap_or(""), &ds, cw, 2);
            let lh = line_h(t, &ds) * 1.1;
            for (i, l) in lines.iter().enumerate() {
                text(c, t, cx, wy + 38.0 + i as f32 * lh, l, &ds, MUTED, true, None);
            }
        }
        if foot != "hide" {
            footer(c, t, cx, cy + FOOT_Y, cw, insight.0, &insight.2, insight.1);
        }
        Self::card_rim(c, x, y, w, BENTO_H);
        edit_sheet(c, t, &mut self.fx, &mut self.hits, &self.cfg, "running", "Bieganie", x, y, w, BENTO_H, editing, ptr);
    }

    // what the pointer is over (a change = repaint)
    pub fn sig(&self) -> String {
        let Some(p) = self.ptr else { return String::new() };
        let mut s = format!("{:?}", hit_at(&self.hits, p.0, p.1));
        let by = self.bento_y;
        for (k, x, w) in self.slots() {
            let cx = 40.0 + x;
            if p.0 >= cx && p.0 < cx + w && p.1 >= by && p.1 < by + BENTO_H {
                s += &k;
                if k == "garmin" && !self.editing("garmin") {
                    let (ix, iw) = (cx + PAD, w - 2.0 * PAD);
                    if p.0 >= ix && p.0 < ix + iw {
                        let yy = p.1 - by - PAD;
                        if (BAND_Y - 14.0..BAND_Y + 56.0).contains(&yy) {
                            s += ":bb";
                        }
                        if (BAND_Y + 72.0..BAND_Y + 116.0).contains(&yy) {
                            let n = self.cfg.list("garmin", "vitals").len().max(1) as f32;
                            s += &format!(":v{}", ((p.0 - ix) / (iw / n)).floor());
                        }
                    }
                }
            }
        }
        s
    }

    // ---------------------------------------------------------------- input (stage)
    // returns true when something changed (repaint)
    pub fn stage_motion(&mut self, p: Option<(f32, f32)>) -> bool {
        self.ptr = p;
        let hand = p.and_then(|p| hit_at(&self.hits, p.0, p.1)).map(|h| !matches!(h, Hit::Sheet)).unwrap_or(false);
        self.cursor_hand = hand;
        true
    }
    pub fn stage_click(&mut self) -> bool {
        let Some(p) = self.ptr else { return false };
        match hit_at(&self.hits, p.0, p.1) {
            Some(Hit::Edit(card)) => {
                self.editing.insert(card, true);
            }
            Some(Hit::Done(card)) => {
                self.editing.insert(card, false);
            }
            Some(Hit::Move(card, d)) => self.cfg.mv(card, d),
            Some(Hit::Opt(card, key, v)) => {
                if let Some(gr) = groups(card).into_iter().find(|g| g.key == key) {
                    self.cfg.click(card, &gr, &v);
                }
            }
            _ => return false,
        }
        self.card_dirty = true;
        true
    }
}
