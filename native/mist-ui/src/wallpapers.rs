// Wallpaper gallery (was Wallpapers.qml; Super+Shift+W): the whole monitor previews the wallpaper under the
// pointer (or the selected one) with the desktop's date + clock on top. The preview itself is GL, drawn by
// mist-wall on a Top-layer surface under this pane ($XDG_RUNTIME_DIR/mist/preview.json, with the scrim);
// this pane is transparent above the glass shelf: filters, tempo / music, a carousel of still thumbnails
// (mist-wall renders them into ~/.cache/mist/thumbs when the gallery opens) of the scenes and the photos
// in ~/Obrazy/Tapety. Click sets, Enter sets and closes, ← → browse, Tab next filter, Space sets, Esc closes.
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::presets;
use crate::text::Style;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const FILTERS: &[(&str, &str, &str)] = &[
    ("all", "Wszystkie", "sparkles"),
    ("auto", "Same się dopasowują", "sunMoon"),
    ("calm", "Spokojne", "leaf"),
    ("live", "Żywe", "bolt"),
    ("toon", "Bajki", "heart"),
    ("photos", "Zdjęcia", "photo"),
];
const TEMPOS: &[(&str, &str)] = &[("calm", "Spokojnie"), ("normal", "Normalnie"), ("fast", "Dynamicznie")];
const ACC: Rgba = Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 1.0); // Theme.mAcc
const ACC_STRONG: Rgba = Rgba(168.0 / 255.0, 228.0 / 255.0, 223.0 / 255.0, 1.0); // Theme.mAccStrong
const INK: Rgba = Rgba(11.0 / 255.0, 15.0 / 255.0, 23.0 / 255.0, 1.0); // #0b0f17
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const TILE_W: f32 = 220.0;
const TILE_GAP: f32 = 16.0;
// the carousel stops short of the shelf's sides
const EDGE_CUT: f32 = 16.0;
const EDGE_FADE: f32 = 24.0;

#[derive(Clone)]
struct Item {
    key: String,
    name: String,
    desc: String,
    cat: String,
    auto: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Close,
    Apply,
    Filter(usize),
    Tempo(usize),
    Music,
    Folder,
    Tile(usize),
    Shelf,
}

pub struct Wallpapers {
    screen: String,
    presets: Vec<Value>,
    photos: Vec<String>,
    cfg: Value,
    filter: usize,
    items: Vec<Item>,
    sel: usize,
    hover: Option<usize>,
    scroll: Anim,
    scales: Vec<Anim>,
    enter: Anim,
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    ptr: (f32, f32),
    pressed: Option<Hit>,
    last_click: (usize, std::time::Instant),
    missing: Vec<String>,
    sent: String,
    view_w: f32,
}

fn cfg_path() -> String {
    format!("{}/.config/mist/wallpaper.json", std::env::var("HOME").unwrap_or_default())
}
fn preview_path() -> String {
    format!("{}/mist/preview.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()))
}
// write next to it, then rename (mist-wall reads on IN_MOVED_TO / IN_CLOSE_WRITE)
fn write_atomic(path: &str, s: &str) {
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, s).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}
fn nice(p: &str) -> String {
    let f = p.rsplit('/').next().unwrap_or(p);
    let f = f.rsplit_once('.').map(|x| x.0).unwrap_or(f);
    let s: String = f.split(|c| c == '-' || c == '_').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
fn cat_name(c: &str) -> &'static str {
    match c {
        "auto" => "Same się dopasowują",
        "calm" => "Spokojne",
        "live" => "Żywe",
        "toon" => "Bajki",
        _ => "Zdjęcie",
    }
}

impl Wallpapers {
    pub fn new() -> Wallpapers {
        let presets = presets::load().into_iter().filter(|p| p.get("mode").is_some()).collect();
        let cfg = std::fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
        let mut w = Wallpapers {
            screen: String::new(),
            presets,
            photos: presets::photos(),
            cfg,
            filter: 0,
            items: Vec::new(),
            sel: 0,
            hover: None,
            scroll: Anim::new(0.0),
            scales: Vec::new(),
            enter: Anim::new(0.0),
            hits: Vec::new(),
            ptr: (-1.0, -1.0),
            pressed: None,
            last_click: (usize::MAX, std::time::Instant::now()),
            missing: Vec::new(),
            sent: String::new(),
            view_w: 0.0,
        };
        w.enter.set(1.0, 520.0, Ease::OutCubic);
        w.rebuild();
        w
    }
    fn applied(&self) -> String {
        let live = self.cfg["live"].as_str().unwrap_or("");
        let image = self.cfg["image"].as_str().unwrap_or("");
        if !live.is_empty() {
            format!("live:{live}")
        } else if !image.is_empty() {
            format!("photo:{image}")
        } else {
            String::new()
        }
    }
    fn count(&self, id: &str) -> usize {
        match id {
            "photos" => self.photos.len(),
            "all" => self.presets.len() + self.photos.len(),
            c => self.presets.iter().filter(|p| p["cat"].as_str() == Some(c)).count(),
        }
    }
    // the carousel's items for the filter; the selection on the wallpaper that is set
    fn rebuild(&mut self) {
        let f = FILTERS[self.filter].0;
        self.items.clear();
        if f != "photos" {
            for p in &self.presets {
                let cat = p["cat"].as_str().unwrap_or("");
                if f == "all" || cat == f {
                    let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
                    self.items.push(Item { key: format!("live:{}", s("id")), name: s("name"), desc: s("desc"), cat: cat.into(), auto: p["auto"].as_bool().unwrap_or(false) });
                }
            }
        }
        if f == "photos" || f == "all" {
            for ph in &self.photos {
                self.items.push(Item { key: format!("photo:{ph}"), name: nice(ph), desc: "Zdjęcie z ~/Obrazy/Tapety".into(), cat: "photos".into(), auto: false });
            }
        }
        let ap = self.applied();
        self.sel = self.items.iter().position(|i| i.key == ap).unwrap_or(0);
        self.hover = None;
        self.scales = (0..self.items.len()).map(|i| Anim::new(if i == self.sel { 1.0 } else { 0.0 })).collect();
        self.scroll_to_sel(false);
    }
    fn shown(&self) -> Option<&Item> {
        self.items.get(self.hover.unwrap_or(self.sel))
    }
    // ListView: keep the selection in the middle band (ApplyRange, ±120 px), within the bounds
    fn scroll_to_sel(&mut self, animate: bool) {
        if self.view_w <= 0.0 {
            return;
        }
        let content = 22.0 * 2.0 + self.items.len() as f32 * (TILE_W + TILE_GAP) - TILE_GAP;
        let max = (content - self.view_w).max(0.0);
        let tile_x = 22.0 + self.sel as f32 * (TILE_W + TILE_GAP);
        let cur = self.scroll.target();
        let (lo, hi) = (self.view_w / 2.0 - 120.0, self.view_w / 2.0 + 120.0);
        let mut to = cur;
        if tile_x - cur < lo {
            to = tile_x - lo;
        } else if tile_x + TILE_W - cur > hi {
            to = tile_x + TILE_W - hi;
        }
        let to = to.clamp(0.0, max);
        if animate {
            self.scroll.set(to, 380.0, Ease::OutCubic);
        } else {
            self.scroll = Anim::new(to);
        }
    }
    // tell mist-wall what to preview
    fn send(&mut self) {
        if self.screen.is_empty() {
            return;
        }
        let key = self.shown().map(|i| i.key.clone()).unwrap_or_default();
        if key != self.sent {
            self.sent = key.clone();
            write_atomic(&preview_path(), &serde_json::json!({ "screen": self.screen, "key": key }).to_string());
        }
    }
    fn save(&mut self, patch: Value) {
        let mut c: Value = std::fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
        if !c.is_object() {
            c = serde_json::json!({});
        }
        for (k, v) in patch.as_object().unwrap() {
            c[k] = v.clone();
        }
        write_atomic(&cfg_path(), &c.to_string());
        self.cfg = c;
    }
    fn apply(&mut self, i: usize) {
        let Some(it) = self.items.get(i).cloned() else { return };
        if let Some(id) = it.key.strip_prefix("live:") {
            self.save(serde_json::json!({ "live": id }));
        } else if let Some(f) = it.key.strip_prefix("photo:") {
            self.save(serde_json::json!({ "live": "", "image": f }));
        }
    }
    fn set_filter(&mut self, f: usize) {
        if f != self.filter {
            self.filter = f;
            self.rebuild();
            self.send();
        }
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1)
    }
    fn hovered(&self, h: Hit) -> bool {
        self.hit() == Some(h)
    }

    // a pill: icon + label (+ count), returns its width
    fn pill(c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, icon: &str, label: &str, count: Option<usize>, fill: Rgba, fg: Rgba, fg2: Rgba) -> f32 {
        let (ls, cs) = (Style::ui(13.0, 600.0), Style::ui(12.0, 500.0));
        let cnt = count.map(|n| n.to_string()).unwrap_or_default();
        let iw = 15.0;
        let lw = t.width(label, &ls);
        let cw = if cnt.is_empty() { 0.0 } else { 7.0 + t.width(&cnt, &cs) };
        let w = iw + 7.0 + lw + cw + 26.0;
        crate::desktop::rect(c, x, y, w, 34.0, 17.0, fill);
        c.icon(t, x + 13.0 + iw / 2.0, y + 17.0, icon, 15.0, fg);
        let lh = crate::desktop::line_h(t, &ls);
        crate::desktop::text(c, t, x + 13.0 + iw + 7.0, y + (34.0 - lh) / 2.0, label, &ls, fg, false, None);
        if !cnt.is_empty() {
            let ch = crate::desktop::line_h(t, &cs);
            crate::desktop::text(c, t, x + 13.0 + iw + 7.0 + lw + 7.0, y + (34.0 - ch) / 2.0, &cnt, &cs, fg2, false, None);
        }
        w
    }
    fn pill_w(t: &mut crate::text::Text, label: &str, count: Option<usize>) -> f32 {
        let cw = count.map(|n| 7.0 + t.width(&n.to_string(), &Style::ui(12.0, 500.0))).unwrap_or(0.0);
        15.0 + 7.0 + t.width(label, &Style::ui(13.0, 600.0)) + cw + 26.0
    }
}

impl Drop for Wallpapers {
    fn drop(&mut self) {
        // the gallery closed: mist-wall drops the preview
        write_atomic(&preview_path(), r#"{"screen":"","key":""}"#);
    }
}

impl Pane for Wallpapers {
    fn name(&self) -> &'static str {
        "wallpaper"
    }
    fn namespace(&self) -> &'static str {
        "mist-gallery"
    }
    fn opened(&mut self, screen: &str) {
        self.screen = screen.to_string();
        self.send();
    }
    fn animating(&self) -> bool {
        self.enter.running() || self.scroll.running() || self.scales.iter().any(|a| a.running())
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(h) if h != Hit::Shelf)
    }
    // thumbnails mist-wall is still rendering
    fn slow(&mut self) -> bool {
        if self.missing.iter().any(|p| std::path::Path::new(p).exists()) {
            self.missing.clear();
            return true;
        }
        false
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        c.clear();
        self.hits.clear();
        let (w, h) = (c.w(), c.h());
        let enter = self.enter.value();
        let applied = self.applied();
        let white = |a: f32| WHITE.alpha(a);

        // the desktop's own composition (date + clock), so the preview reads like the real thing
        {
            let now = crate::data::now();
            let tm = crate::data::local(now);
            const DAYS: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];
            let date = format!("{}, {} {}", DAYS[tm.tm_wday as usize], tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]);
            let ds = Style::ui(22.0, 600.0);
            let dw = t.width(&date, &ds);
            crate::desktop::text(c, t, ((w - dw) / 2.0).round(), 118.0, &date, &ds, white(0.82 * enter), false, None);
            let cs = Style::ui(150.0, 620.0).spaced(-4.0).tnum();
            let s = crate::data::hm(now);
            let cw = t.width(&s, &cs);
            let y = 118.0 + crate::desktop::line_h(t, &ds);
            crate::desktop::text(c, t, ((w - cw) / 2.0).round(), y, &s, &cs, white(0.86 * enter), false, None);
        }

        // close
        {
            let (x, y) = (w - 28.0 - 40.0, 28.0);
            let hov = self.hovered(Hit::Close);
            crate::desktop::rect(c, x, y, 40.0, 40.0, 20.0, white(if hov { 0.22 } else { 0.1 }));
            crate::desktop::border(c, x, y, 40.0, 40.0, 20.0, 1.0, white(0.14));
            c.icon(t, x + 20.0, y + 20.0, "x", 18.0, white(0.9));
            self.hits.push(((x, y, 40.0, 40.0), Hit::Close));
        }

        // the shelf
        let (sx, sw, sh) = (40.0, w - 80.0, 280.0);
        let sy = (h - sh - 28.0 + (1.0 - enter) * 140.0).round();
        glass(c, sx, sy, sw, sh, 30.0, 0.55);
        glass_rim(c, sx, sy, sw, sh, 30.0);
        self.hits.push(((sx, sy, sw, sh), Hit::Shelf));

        // filters
        let mut px = sx + 22.0;
        for (i, f) in FILTERS.iter().enumerate() {
            let on = i == self.filter;
            let hov = self.hovered(Hit::Filter(i));
            let fill = if on { white(0.92) } else if hov { white(0.14) } else { white(0.07) };
            let (fg, fg2) = if on { (INK, INK.alpha(0.5)) } else { (white(0.86), white(0.4)) };
            let pw = Self::pill(c, t, px, sy + 18.0, f.2, f.1, Some(self.count(f.0)), fill, if on { INK } else { fg }, fg2);
            self.hits.push(((px, sy + 18.0, pw, 34.0), Hit::Filter(i)));
            px += pw + 6.0;
        }
        // tempo + music (scenes), folder (photos)
        let mut rx = sx + sw - 22.0;
        if FILTERS[self.filter].0 == "photos" {
            let fw = Self::pill_w(t, "Otwórz folder", None);
            rx -= fw;
            let fill = white(if self.hovered(Hit::Folder) { 0.14 } else { 0.07 });
            Self::pill(c, t, rx, sy + 18.0, "folder", "Otwórz folder", None, fill, white(0.86), white(0.4));
            self.hits.push(((rx, sy + 18.0, fw, 34.0), Hit::Folder));
        } else {
            let music = self.cfg["music"].as_bool().unwrap_or(false);
            let mw = Self::pill_w(t, "Rytm muzyki", None);
            rx -= mw;
            let fill = if music { ACC.alpha(0.22) } else { white(if self.hovered(Hit::Music) { 0.14 } else { 0.07 }) };
            let fg = if music { ACC_STRONG } else { white(0.86) };
            Self::pill(c, t, rx, sy + 18.0, "music", "Rytm muzyki", None, fill, fg, fg);
            if music {
                crate::desktop::border(c, rx, sy + 18.0, mw, 34.0, 17.0, 1.0, ACC.alpha(0.5));
            }
            self.hits.push(((rx, sy + 18.0, mw, 34.0), Hit::Music));
            // segmented tempo
            let tempo = self.cfg["tempo"].as_str().unwrap_or("normal").to_string();
            let segs: Vec<(f32, bool)> = TEMPOS.iter().map(|(id, n)| {
                let on = tempo == *id;
                (t.width(n, &Style::ui(12.5, if on { 650.0 } else { 500.0 })) + 24.0, on)
            }).collect();
            let segw: f32 = segs.iter().map(|s| s.0).sum::<f32>() + 6.0;
            rx -= 10.0 + segw;
            crate::desktop::rect(c, rx, sy + 18.0, segw, 34.0, 17.0, white(0.07));
            let mut x = rx + 3.0;
            for (i, (sw2, on)) in segs.iter().enumerate() {
                let hov = self.hovered(Hit::Tempo(i));
                if *on || hov {
                    crate::desktop::rect(c, x, sy + 21.0, *sw2, 28.0, 14.0, white(if *on { 0.2 } else { 0.08 }));
                }
                let st = Style::ui(12.5, if *on { 650.0 } else { 500.0 });
                let lh = crate::desktop::line_h(t, &st);
                crate::desktop::text(c, t, x + 12.0, sy + 21.0 + (28.0 - lh) / 2.0, TEMPOS[i].1, &st, if *on { WHITE } else { white(0.6) }, false, None);
                self.hits.push(((x, sy + 21.0, *sw2, 28.0), Hit::Tempo(i)));
                x += sw2;
            }
        }

        // the carousel (clipped to the shelf's inside)
        if self.view_w != sw {
            // the first paint knows the width: show the selection
            self.view_w = sw;
            self.scroll_to_sel(false);
        }
        let cy = sy + 60.0;
        let off = self.scroll.value();
        let first = (((off - 22.0) / (TILE_W + TILE_GAP)).floor().max(0.0)) as usize;
        let mut clip = Canvas::new(sw as u32, 192);
        for i in first..self.items.len() {
            let tx = 22.0 + i as f32 * (TILE_W + TILE_GAP) - off;
            if tx > sw {
                break;
            }
            let it = self.items[i].clone();
            let (is_sel, hov) = (i == self.sel, self.hover == Some(i));
            // 0 = rest, 0.5 = hovered, 1 = selected
            let want = if is_sel { 1.0 } else if hov { 0.5 } else { 0.0 };
            if (self.scales[i].target() - want).abs() > 0.01 {
                self.scales[i].set(want, 220.0, Ease::OutCubic);
            }
            let k = self.scales[i].value();
            let (scale, dy) = if k >= 0.5 { (0.97 + (k - 0.5) * 0.06, 12.0 - (k - 0.5) * 4.0) } else { (0.93 + k * 0.08, 16.0 - k * 8.0) };
            let (cw, ch) = (TILE_W * scale, 124.0 * scale);
            let (x, y) = (tx + (TILE_W - cw) / 2.0, 8.0 + dy + (124.0 - ch) / 2.0);
            let thumb = presets::thumb_path(&it.key);
            match cx.imgs.get(&thumb, TILE_W as u32, cx.icons) {
                Some(pm) => crate::img::draw(&mut clip, &pm, x, y, cw, ch, 16.0 * scale, true, 1.0),
                None => {
                    crate::desktop::rect(&mut clip, x, y, cw, ch, 16.0 * scale, white(0.06));
                    cx.imgs.forget(&thumb);
                    if !self.missing.contains(&thumb) {
                        self.missing.push(thumb.clone());
                    }
                }
            }
            // dim the ones you are not looking at
            if !is_sel && !hov {
                crate::desktop::rect(&mut clip, x, y, cw, ch, 16.0 * scale, Rgba(0.0, 0.0, 0.0, 0.22));
            }
            // ring: white = selected, teal = the one that is set
            if is_sel {
                crate::desktop::border(&mut clip, x - 4.0, y - 4.0, cw + 8.0, ch + 8.0, 20.0, 2.5, WHITE);
            }
            crate::desktop::border(&mut clip, x, y, cw, ch, 16.0 * scale, 1.0, white(0.14));
            if it.key == applied {
                let (bx, by) = (x + cw - 8.0 - 22.0, y + 8.0);
                crate::desktop::rect(&mut clip, bx, by, 22.0, 22.0, 11.0, ACC);
                clip.icon(t, bx + 11.0, by + 11.0, "check", 14.0, INK);
            }
            if it.auto {
                let st = Style::ui(9.5, 700.0).spaced(0.8);
                let aw = t.width("AUTO", &st) + 14.0;
                crate::desktop::rect(&mut clip, x + 8.0, y + 8.0, aw, 20.0, 10.0, Rgba(0.0, 0.0, 0.0, 0.45));
                let lh = crate::desktop::line_h(t, &st);
                crate::desktop::text(&mut clip, t, x + 15.0, y + 8.0 + (20.0 - lh) / 2.0, "AUTO", &st, white(0.9), false, None);
            }
            let ns = Style::ui(13.0, if is_sel { 650.0 } else { 500.0 });
            let nw = t.width(&it.name, &ns).min(TILE_W);
            crate::desktop::text(&mut clip, t, (tx + (TILE_W - nw) / 2.0).round(), 8.0 + 148.0, &it.name, &ns, if is_sel { WHITE } else { white(0.6) }, false, Some(TILE_W));
            let vis_x = tx.max(EDGE_CUT);
            let vis_w = (tx + TILE_W).min(sw - EDGE_CUT) - vis_x;
            if vis_w > 0.0 {
                self.hits.push(((sx + vis_x, cy, vis_w, 184.0), Hit::Tile(i)));
            }
        }
        // keep the tiles off the shelf's rim: cut 16 px in from each side, fade over the next 24 px
        {
            let cw = clip.pm.width() as usize;
            let data = clip.pm.data_mut();
            for (i, px) in data.chunks_exact_mut(4).enumerate() {
                let x = (i % cw) as f32 + 0.5;
                let d = x.min(cw as f32 - x);
                let k = ((d - EDGE_CUT) / EDGE_FADE).clamp(0.0, 1.0);
                if k < 1.0 {
                    for ch in px.iter_mut() {
                        *ch = (*ch as f32 * k) as u8;
                    }
                }
            }
        }
        c.pm.draw_pixmap(sx as i32, (cy - 8.0) as i32, clip.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        // keys
        {
            let s = "← →  przeglądaj   ·   Tab  kategoria   ·   Spacja  ustaw   ·   Enter  ustaw i zamknij   ·   Esc  zamknij";
            let st = Style::ui(11.0, 400.0);
            let kw = t.width(s, &st);
            let lh = crate::desktop::line_h(t, &st);
            crate::desktop::text(c, t, sx + sw - 24.0 - kw, sy + sh - 12.0 - lh, s, &st, white(0.38), false, None);
        }

        // the title of what is shown, above the shelf
        if let Some(it) = self.shown().cloned() {
            let a = enter;
            let tx = 64.0;
            let ds = Style::ui(16.0, 400.0);
            let lines = crate::gfx::wrap(t, &it.desc, &ds, (w - 128.0).min(620.0), 4);
            let dlh = crate::desktop::line_h(t, &ds);
            let ns = Style::ui(52.0, 650.0).spaced(-1.2);
            let nlh = crate::desktop::line_h(t, &ns);
            let cs = Style::ui(11.5, 650.0).spaced(1.6);
            let clh = crate::desktop::line_h(t, &cs);
            let bottom = sy - 28.0;
            let total = clh.max(22.0) + 8.0 + nlh + 8.0 + dlh * lines.len() as f32;
            let mut y = bottom - total;
            let cat = format!("{}{}", cat_name(&it.cat), if it.key.starts_with("live:") { " · animowana" } else { "" }).to_uppercase();
            let cw = t.width(&cat, &cs);
            let row_h = clh.max(22.0);
            crate::desktop::text(c, t, tx, y + (row_h - clh) / 2.0, &cat, &cs, white(0.62 * a), false, None);
            if it.key == applied {
                let ls = Style::ui(11.5, 600.0);
                let lw = t.width("Ustawiona", &ls);
                let (bx, bw) = (tx + cw + 10.0, 12.0 + 4.0 + lw + 16.0);
                let by = y + (row_h - 22.0) / 2.0;
                crate::desktop::rect(c, bx, by, bw, 22.0, 11.0, ACC.alpha(0.2 * a));
                crate::desktop::border(c, bx, by, bw, 22.0, 11.0, 1.0, ACC.alpha(0.45 * a));
                c.icon(t, bx + 8.0 + 6.0, by + 11.0, "check", 12.0, ACC.alpha(a));
                let llh = crate::desktop::line_h(t, &ls);
                crate::desktop::text(c, t, bx + 8.0 + 12.0 + 4.0, by + (22.0 - llh) / 2.0, "Ustawiona", &ls, ACC_STRONG.alpha(a), false, None);
            }
            y += row_h + 8.0;
            crate::desktop::text(c, t, tx, y, &it.name, &ns, white(a), false, Some(w - 128.0 - 260.0));
            y += nlh + 8.0;
            for l in &lines {
                crate::desktop::text(c, t, tx, y, l, &ds, white(0.78 * a), false, None);
                y += dlh;
            }
            // what Enter does, on the right
            if it.key != applied {
                let ls = Style::ui(14.0, 650.0);
                let lw = t.width("Ustaw tapetę", &ls);
                let bw = 16.0 + 8.0 + lw + 32.0;
                let (bx, by) = (w - 64.0 - bw, sy - 34.0 - 44.0);
                let hov = self.hovered(Hit::Apply);
                crate::desktop::rect(c, bx, by, bw, 44.0, 22.0, white(if hov { 1.0 } else { 0.9 } * a));
                c.icon(t, bx + 16.0 + 8.0, by + 22.0, "check", 16.0, INK.alpha(a));
                let llh = crate::desktop::line_h(t, &ls);
                crate::desktop::text(c, t, bx + 16.0 + 16.0 + 8.0, by + (44.0 - llh) / 2.0, "Ustaw tapetę", &ls, INK.alpha(a), false, None);
                self.hits.push(((bx, by, bw, 44.0), Hit::Apply));
            }
        }
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let hit = self.hit();
        match ev {
            Ptr::Enter | Ptr::Motion | Ptr::Leave => {
                let hov = if ev == Ptr::Leave { None } else { match hit { Some(Hit::Tile(i)) => Some(i), _ => None } };
                if hov != self.hover {
                    self.hover = hov;
                    self.send();
                }
                Act::Redraw
            }
            Ptr::Press(_) => {
                self.pressed = hit;
                Act::None
            }
            Ptr::Release(_) => {
                let p = self.pressed.take();
                if p != hit {
                    return Act::None;
                }
                match hit {
                    Some(Hit::Close) => Act::Close,
                    Some(Hit::Apply) => {
                        let i = self.hover.unwrap_or(self.sel);
                        self.apply(i);
                        self.sel = i;
                        Act::Redraw
                    }
                    Some(Hit::Filter(i)) => {
                        self.set_filter(i);
                        Act::Redraw
                    }
                    Some(Hit::Tempo(i)) => {
                        self.save(serde_json::json!({ "tempo": TEMPOS[i].0 }));
                        Act::Redraw
                    }
                    Some(Hit::Music) => {
                        let m = !self.cfg["music"].as_bool().unwrap_or(false);
                        self.save(serde_json::json!({ "music": m }));
                        Act::Redraw
                    }
                    Some(Hit::Folder) => Act::Run(vec!["xdg-open".into(), presets::photo_dir()]),
                    Some(Hit::Tile(i)) => {
                        let dbl = self.last_click.0 == i && self.last_click.1.elapsed().as_millis() < 400;
                        self.last_click = (i, std::time::Instant::now());
                        self.sel = i;
                        self.apply(i);
                        self.send();
                        if dbl { Act::Close } else { Act::Redraw }
                    }
                    Some(Hit::Shelf) => Act::None,
                    // the preview area: like the QML window, a click there does nothing
                    None => Act::None,
                }
            }
            Ptr::Wheel(d) => {
                // the carousel scrolls sideways
                let content = 22.0 * 2.0 + self.items.len() as f32 * (TILE_W + TILE_GAP) - TILE_GAP;
                let max = (content - self.view_w).max(0.0);
                let to = (self.scroll.target() + d * 60.0).clamp(0.0, max);
                self.scroll.set(to, 160.0, Ease::OutCubic);
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        let n = self.items.len();
        match ev.keysym {
            Keysym::Escape => return Act::Close,
            Keysym::Right => {
                self.hover = None;
                self.sel = (self.sel + 1).min(n.saturating_sub(1));
                self.scroll_to_sel(true);
            }
            Keysym::Left => {
                self.hover = None;
                self.sel = self.sel.saturating_sub(1);
                self.scroll_to_sel(true);
            }
            Keysym::Tab => self.set_filter((self.filter + 1) % FILTERS.len()),
            Keysym::ISO_Left_Tab => self.set_filter((self.filter + FILTERS.len() - 1) % FILTERS.len()),
            Keysym::space => self.apply(self.sel),
            Keysym::Return | Keysym::KP_Enter => {
                self.apply(self.sel);
                return Act::Close;
            }
            _ => return Act::None,
        }
        self.send();
        Act::Redraw
    }
}
