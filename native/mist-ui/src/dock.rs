// Dock (Dock.qml 1:1): light liquid-glass slab, radius 22, 46 px tiles, padding 8, gap 8;
// [Start] | pinned + running (grouped per app) | [Kosz]. Shown on the bare desktop; when windows are
// open it slides away and comes back from a 2 px strip at the bottom edge. Hover lifts a tile and shows
// its name in a glass pill; a dot marks running apps, a badge counts their windows.
use crate::anim::{Anim, Ease};
use crate::apps::{Apps, Icons};
use crate::gfx::{Canvas, Rgba};
use crate::glass::Glass;
use crate::sys::{Client, Hypr};
use crate::text::{Style, Text};
use crate::theme::*;
use std::collections::HashMap;
use std::rc::Rc;

pub const SIZE: f32 = 46.0;
pub const PAD: f32 = 8.0;
pub const GAP: f32 = 8.0;
pub const WIN_H: u32 = 62 + 46 + 30;

#[derive(Clone)]
pub struct App {
    pub key: String,          // entry id (or the class when there is no entry)
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub pinned: bool,
    pub wins: Vec<Client>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Slot {
    Start,
    App(String),
    Trash,
    Sep,
}

pub struct Dock {
    pub pins: Vec<String>,
    pub apps: Vec<App>,
    pub slots: Vec<Slot>,
    pub hovered: bool,          // pointer on the dock (or the edge reveal fired)
    pub menu_open: bool,
    pub start_open: bool,
    pub bare: bool,
    pub y: Anim,                // slab top inside the window
    pub hover_slot: Option<usize>,
    pub pressed_slot: Option<usize>,
    pub lift: HashMap<String, (Anim, Anim)>, // per slot key: scale, translate y
    pub label: HashMap<String, Anim>,        // label pill opacity
    tiles: HashMap<String, Option<Rc<Tile>>>,
    pub asked: std::collections::HashSet<String>,
    glass_cache: Option<(u32, Canvas)>,
    label_cache: HashMap<String, Canvas>, // label pills (glass + text), by text: drawn every hover frame

}

pub struct Tile {
    img: tiny_skia::Pixmap,    // 92 px (sourceSize 2x)
    shadow: tiny_skia::Pixmap, // blurred alpha, same size + margin
    // both scaled to SIZE once: a tile at rest is a plain blit, only a hovered / pressed one is resampled
    rest: tiny_skia::Pixmap,
    rest_shadow: tiny_skia::Pixmap,
}
const SH_M: i32 = 12;

pub fn slot_key(s: &Slot) -> String {
    match s {
        Slot::Start => "@start".into(),
        Slot::Trash => "@trash".into(),
        Slot::App(k) => k.clone(),
        Slot::Sep => String::new(),
    }
}

impl Dock {
    pub fn new() -> Dock {
        let mut d = Dock { pins: Vec::new(), apps: Vec::new(), slots: Vec::new(), hovered: false, menu_open: false, start_open: false, bare: true, y: Anim::new(68.0), hover_slot: None, pressed_slot: None, lift: HashMap::new(), label: HashMap::new(), tiles: HashMap::new(), asked: Default::default(), glass_cache: None, label_cache: HashMap::new() };
        d.load_pins();
        d
    }
    pub fn conf_path() -> String {
        format!("{}/.config/mist/apps.json", std::env::var("HOME").unwrap_or_default())
    }
    pub fn load_pins(&mut self) {
        let v: serde_json::Value = std::fs::read_to_string(Self::conf_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        self.pins = v["dock"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    }
    pub fn save_pins(&self, pins: &[String]) {
        let p = Self::conf_path();
        let mut v: serde_json::Value = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::json!({}));
        v["dock"] = serde_json::json!(pins);
        let _ = std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    pub fn shown(&self) -> bool {
        self.bare || self.hovered || self.menu_open || self.start_open
    }

    // Dock.items: pinned apps in order (with their windows), then the other running apps
    pub fn rebuild(&mut self, h: &Hypr, apps: &Apps) {
        let mut groups: Vec<(String, Option<usize>, Vec<Client>)> = Vec::new();
        for c in &h.clients {
            let e = apps.lookup(&c.class).map(|e| apps.entries.iter().position(|x| x.id == e.id).unwrap());
            let key = e.map(|i| apps.entries[i].id.clone()).unwrap_or(c.class.clone());
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.2.push(c.clone()),
                None => groups.push((key, e, vec![c.clone()])),
            }
        }
        let mut out: Vec<App> = Vec::new();
        for id in &self.pins {
            let Some(e) = apps.lookup(id) else { continue };
            if out.iter().any(|a| a.key == e.id) {
                continue;
            }
            let wins = groups.iter().find(|g| g.0 == e.id).map(|g| g.2.clone()).unwrap_or_default();
            out.push(App { key: e.id.clone(), name: e.name.clone(), icon: e.icon.clone(), exec: e.exec.clone(), pinned: true, wins });
        }
        for (key, e, wins) in groups {
            if out.iter().any(|a| a.key == key) {
                continue;
            }
            let (name, icon, exec) = e.map(|i| (apps.entries[i].name.clone(), apps.entries[i].icon.clone(), apps.entries[i].exec.clone())).unwrap_or((key.clone(), key.clone(), String::new()));
            out.push(App { key, name, icon, exec, pinned: false, wins });
        }
        self.apps = out;
        let mut s = vec![Slot::Start, Slot::Sep];
        s.extend(self.apps.iter().map(|a| Slot::App(a.key.clone())));
        s.push(Slot::Sep);
        s.push(Slot::Trash);
        self.slots = s;
        self.glass_cache = None;
    }

    pub fn slab_w(&self) -> f32 {
        let w: f32 = self.slots.iter().map(|s| if *s == Slot::Sep { 1.0 } else { SIZE }).sum();
        w + GAP * (self.slots.len().saturating_sub(1)) as f32 + 2.0 * PAD
    }
    pub fn win_w(&self) -> u32 {
        (self.slab_w() + 88.0).ceil() as u32
    }
    // x of each slot inside the window
    pub fn slot_x(&self, win_w: f32) -> Vec<f32> {
        let mut x = (win_w - self.slab_w()) / 2.0 + PAD;
        self.slots.iter().map(|s| {
            let v = x;
            x += if *s == Slot::Sep { 1.0 } else { SIZE } + GAP;
            v
        }).collect()
    }
    pub fn app(&self, key: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.key == key)
    }
    pub fn label_of(&self, s: &Slot) -> String {
        match s {
            Slot::Start => "Start".into(),
            Slot::Trash => "Kosz".into(),
            Slot::Sep => String::new(),
            Slot::App(k) => self.app(k).map(|a| if a.wins.len() == 1 && !a.pinned { a.wins[0].title.clone() } else { a.name.clone() }).unwrap_or_default(),
        }
    }
    pub fn icon_of(&self, s: &Slot) -> String {
        match s {
            Slot::App(k) => self.app(k).map(|a| if a.icon.is_empty() { a.key.clone() } else { a.icon.clone() }).unwrap_or_default(),
            other => slot_key(other),
        }
    }

    pub fn animating(&self) -> bool {
        self.y.running() || self.lift.values().any(|(a, b)| a.running() || b.running()) || self.label.values().any(|a| a.running())
    }

    // hover / press: scale 1.07 / 0.96, lift -3 / -1 (200 ms OutCubic); label opacity (140 ms)
    pub fn update_lift(&mut self) {
        for (i, s) in self.slots.iter().enumerate() {
            if *s == Slot::Sep {
                continue;
            }
            let k = slot_key(s);
            let hov = self.hover_slot == Some(i);
            let pr = self.pressed_slot == Some(i);
            let e = self.lift.entry(k.clone()).or_insert((Anim::new(1.0), Anim::new(0.0)));
            e.0.set(if pr { 0.96 } else if hov { 1.07 } else { 1.0 }, 200.0, Ease::OutCubic);
            e.1.set(if pr { -1.0 } else if hov { -3.0 } else { 0.0 }, 200.0, Ease::OutCubic);
            let l = self.label.entry(k).or_insert(Anim::new(0.0));
            l.set(if hov && !self.menu_open { 1.0 } else { 0.0 }, 140.0, Ease::Linear);
        }
    }
    pub fn update_y(&mut self) {
        let to = if self.shown() { WIN_H as f32 - (SIZE + 2.0 * PAD) - 8.0 } else { WIN_H as f32 + 4.0 };
        self.y.set(to, 240.0, Ease::OutCubic);
    }

    fn tile(&mut self, name: &str, icons: &mut Icons) -> Option<Rc<Tile>> {
        if let Some(t) = self.tiles.get(name) {
            return t.clone();
        }
        let key: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._@-".contains(c) { c } else { '_' }).collect();
        let path = format!("{}/.cache/mist/tiles/{}.png", std::env::var("HOME").unwrap_or_default(), key);
        let img = tiny_skia::Pixmap::load_png(&path).ok().map(|p| scale_to(&p, 92)).or_else(|| {
            // no tile yet: ask mistd once, show the theme icon meanwhile (IconImage, margins 2)
            if !name.starts_with('@') && self.asked.insert(name.to_string()) {
                let bin = crate::paths::mistd();
                let _ = std::process::Command::new(bin).args(["tiles", name]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
            }
            let im = icons.image(if name.starts_with('@') { "application-x-executable" } else { name }, 84).or_else(|| icons.image("application-x-executable", 84))?;
            let mut pm = tiny_skia::Pixmap::new(92, 92)?;
            let src = tiny_skia::Pixmap::from_vec(im.rgba.clone(), tiny_skia::IntSize::from_wh(im.w, im.h)?)?;
            pm.draw_pixmap(4, 4, src.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
            Some(pm)
        });
        let t = img.map(|img| {
            let shadow = drop_shadow(&img);
            let k = SIZE / 92.0;
            let rest = scaled(&img, k, tiny_skia::FilterQuality::Bicubic);
            let rest_shadow = scaled(&shadow, k, tiny_skia::FilterQuality::Bilinear);
            Rc::new(Tile { img, shadow, rest, rest_shadow })
        });
        // a missing tile is looked up again next time (mistd may have made it)
        if t.as_ref().map(|_| true).unwrap_or(false) && std::path::Path::new(&path).exists() || name.starts_with('@') {
            self.tiles.insert(name.to_string(), t.clone());
        }
        t
    }
    pub fn forget_tiles(&mut self) {
        self.tiles.clear();
    }

    pub fn draw(&mut self, c: &mut Canvas, t: &mut Text, icons: &mut Icons) {
        c.clear();
        let ww = c.w();
        let slab_w = self.slab_w();
        let slab_h = SIZE + 2.0 * PAD;
        let sx = (ww - slab_w) / 2.0;
        let sy = self.y.value();
        // glass slab (cached: it only changes with its width)
        let gw = (slab_w + 80.0).ceil() as u32;
        let gh = (slab_h + 80.0).ceil() as u32;
        if self.glass_cache.as_ref().map(|g| g.0 != gw).unwrap_or(true) {
            let mut gc = Canvas::new(gw, gh);
            Glass { radius: 22.0, tone: Rgba::rgb8(96, 104, 126, 1.0), tint: 0.30, shine: 1.4, shadow_opacity: 0.30, ..Default::default() }.paint(&mut gc, 40.0, 40.0 - 0.0, slab_w, slab_h);
            self.glass_cache = Some((gw, gc));
        }
        if let Some((_, gc)) = &self.glass_cache {
            c.pm.draw_pixmap((sx - 40.0).round() as i32, (sy - 40.0).round() as i32, gc.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        }
        let xs = self.slot_x(ww);
        let slots = self.slots.clone();
        let iy = sy + PAD;
        for (i, s) in slots.iter().enumerate() {
            let x = xs[i];
            if *s == Slot::Sep {
                c.rrect(x, iy + SIZE * 0.2, 1.0, SIZE * 0.6, 0.0, Rgba(1.0, 1.0, 1.0, 0.18));
                continue;
            }
            let k = slot_key(s);
            let (sc, ty) = self.lift.get(&k).map(|(a, b)| (a.value(), b.value())).unwrap_or((1.0, 0.0));
            let name = self.icon_of(s);
            if let Some(tile) = self.tile(&name, icons) {
                // scale about the tile centre, then lift
                let k2 = SIZE / 92.0 * sc;
                let cx = x + SIZE / 2.0;
                let cy = iy + SIZE / 2.0 + ty;
                let ox = cx - 46.0 * k2;
                let oy = cy - 46.0 * k2;
                if (sc - 1.0).abs() < 1e-3 {
                    let (bx, by) = (ox.round() as i32, oy.round() as i32);
                    let m = (SH_M as f32 * k2).round() as i32;
                    c.pm.draw_pixmap(bx - m, by - m + 3, tile.rest_shadow.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
                    c.pm.draw_pixmap(bx, by, tile.rest.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
                } else {
                    let sp = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
                    c.pm.draw_pixmap(0, 0, tile.shadow.as_ref(), &sp, tiny_skia::Transform::from_row(k2, 0.0, 0.0, k2, ox - SH_M as f32 * k2, oy - SH_M as f32 * k2 + 3.0), None);
                    let ip = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
                    c.pm.draw_pixmap(0, 0, tile.img.as_ref(), &ip, tiny_skia::Transform::from_row(k2, 0.0, 0.0, k2, ox, oy), None);
                }
            }
            if let Slot::App(key) = s {
                if let Some(a) = self.app(key) {
                    if !a.wins.is_empty() {
                        let focused = a.wins.iter().any(|w| w.focus == 0);
                        c.rrect(x + SIZE / 2.0 - 2.0, iy + SIZE + PAD / 2.0 - 2.0, 4.0, 4.0, 2.0, Rgba(1.0, 1.0, 1.0, if focused { 0.9 } else { 0.6 }));
                    }
                    if a.wins.len() > 1 {
                        let txt = a.wins.len().to_string();
                        let st = Style::ui(10.0, 600.0);
                        let bw = (t.width(&txt, &st) + 9.0).max(16.0);
                        let bx = x + SIZE - bw * 0.75;
                        c.rrect(bx, iy + SIZE - 20.0, bw, 16.0, 8.0, Rgba::rgb8(58, 64, 78, 0.95));
                        let tw = t.width(&txt, &st);
                        c.text(t, bx + (bw - tw) / 2.0, iy + SIZE - 12.0, &txt, &st, Rgba(1.0, 1.0, 1.0, 1.0));
                    }
                }
            }
        }
        // label pills on top
        for (i, s) in slots.iter().enumerate() {
            if *s == Slot::Sep {
                continue;
            }
            let op = self.label.get(&slot_key(s)).map(|a| a.value()).unwrap_or(0.0);
            if op <= 0.01 {
                continue;
            }
            let txt = self.label_of(s);
            let st = Style::ui(12.0, 500.0);
            let tw = t.width(&txt, &st);
            let pw = (tw + 20.0).min(260.0);
            let px = xs[i] + SIZE / 2.0 - pw / 2.0;
            let py = iy - PAD - 26.0 - 10.0;
            if !self.label_cache.contains_key(&txt) {
                if self.label_cache.len() > 32 {
                    self.label_cache.clear();
                }
                let mut pc = Canvas::new((pw + 80.0) as u32, 26 + 80);
                Glass { radius: 10.0, tint: 0.86, shadow_opacity: 0.25, ..Default::default() }.paint(&mut pc, 40.0, 40.0, pw, 26.0);
                let label = crate::bar::elide(t, &txt, &st, pw - 20.0);
                let lw = t.width(&label, &st);
                pc.text(t, 40.0 + (pw - lw) / 2.0, 40.0 + 13.0, &label, &st, T1);
                self.label_cache.insert(txt.clone(), pc);
            }
            let pc = &self.label_cache[&txt];
            let paint = tiny_skia::PixmapPaint { opacity: op, ..Default::default() };
            c.pm.draw_pixmap((px - 40.0).round() as i32, (py - 40.0).round() as i32, pc.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
        }
    }

    // slot under x (window coordinates), only over the slab
    pub fn slot_at(&self, ww: f32, x: f32, y: f32) -> Option<usize> {
        let sy = self.y.value();
        if y < sy - 6.0 || y > sy + SIZE + 2.0 * PAD + 14.0 {
            return None;
        }
        let xs = self.slot_x(ww);
        self.slots.iter().enumerate().position(|(i, s)| *s != Slot::Sep && x >= xs[i] && x < xs[i] + SIZE)
    }
}

fn scaled(p: &tiny_skia::Pixmap, k: f32, quality: tiny_skia::FilterQuality) -> tiny_skia::Pixmap {
    let (w, h) = (((p.width() as f32) * k).ceil() as u32, ((p.height() as f32) * k).ceil() as u32);
    let mut o = tiny_skia::Pixmap::new(w.max(1), h.max(1)).unwrap();
    o.draw_pixmap(0, 0, p.as_ref(), &tiny_skia::PixmapPaint { quality, ..Default::default() }, tiny_skia::Transform::from_scale(k, k), None);
    o
}

fn scale_to(p: &tiny_skia::Pixmap, size: u32) -> tiny_skia::Pixmap {
    if p.width() == size && p.height() == size {
        return p.clone();
    }
    let mut o = tiny_skia::Pixmap::new(size, size).unwrap();
    let k = size as f32 / p.width().max(p.height()) as f32;
    o.draw_pixmap(0, 0, p.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() }, tiny_skia::Transform::from_scale(k, k), None);
    o
}

// MultiEffect shadow (blur 0.5 of blurMax 32, colour rgba(2,5,10,.45)): the tile's alpha, blurred
fn drop_shadow(img: &tiny_skia::Pixmap) -> tiny_skia::Pixmap {
    let (w, h) = (img.width() as i32 + 2 * SH_M, img.height() as i32 + 2 * SH_M);
    let mut a = vec![0f32; (w * h) as usize];
    for y in 0..img.height() as i32 {
        for x in 0..img.width() as i32 {
            a[((y + SH_M) * w + x + SH_M) as usize] = img.data()[((y * img.width() as i32 + x) * 4 + 3) as usize] as f32 / 255.0;
        }
    }
    // three box blurs ~ gaussian (radius in the 92 px tile: 2x the 46 px one)
    for _ in 0..3 {
        box_blur(&mut a, w, h, 5);
    }
    let mut o = tiny_skia::Pixmap::new(w as u32, h as u32).unwrap();
    let col = (2.0f32, 5.0f32, 10.0f32);
    for (i, px) in o.data_mut().chunks_exact_mut(4).enumerate() {
        let al = a[i] * 0.45;
        px[0] = (col.0 * al) as u8;
        px[1] = (col.1 * al) as u8;
        px[2] = (col.2 * al) as u8;
        px[3] = (al * 255.0) as u8;
    }
    o
}
fn box_blur(a: &mut [f32], w: i32, h: i32, r: i32) {
    let mut t = vec![0f32; a.len()];
    let n = (2 * r + 1) as f32;
    for y in 0..h {
        let mut s = 0.0;
        for x in -r..=r {
            s += a[(y * w + x.clamp(0, w - 1)) as usize];
        }
        for x in 0..w {
            t[(y * w + x) as usize] = s / n;
            s += a[(y * w + (x + r + 1).min(w - 1)) as usize] - a[(y * w + (x - r).max(0)) as usize];
        }
    }
    for x in 0..w {
        let mut s = 0.0;
        for y in -r..=r {
            s += t[(y.clamp(0, h - 1) * w + x) as usize];
        }
        for y in 0..h {
            a[(y * w + x) as usize] = s / n;
            s += t[((y + r + 1).min(h - 1) * w + x) as usize] - t[((y - r).max(0) * w + x) as usize];
        }
    }
}
