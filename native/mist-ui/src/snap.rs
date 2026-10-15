// Window snapping 1:1 like Windows 11 (was Snap.qml).
// Dragging a window (title bar, CSD, Alt+drag):
//  - grabbing a maximized / snapped window restores its previous size under the cursor (hyprbars mistsnap.cpp)
//  - cursor at the top edge                -> full-screen preview, release = maximize
//  - cursor at the top edge in a corner     -> quarter
//  - cursor at the outer left / right edge  -> half; near a corner -> quarter
//  - window near the top                    -> the Snap bar pill slides down at the top centre;
//                                              cursor onto it expands Snap Layouts (hover a zone, release)
// Snap Layouts flyout: hover the maximize (green) button of the title bar, or Win+Z.
// Snap Assist: after snapping into part of the screen, the rest shows the other windows; pick one to fill it.
// Events (socket2, hyprbars plugin): "mistdrag>>begin,<addr>,x,y,w,h", "mistdrag>>end,x,y",
// "mistsnap>>snapped,<addr>,<monitor>,fx,fy,fw,fh", "mistsnap>>maxhover,<addr>,x,y", "maxleave,<addr>", "layouts,<addr>,x,y".
// The cursor is polled (j/cursorpos) only while a drag is in progress. MistSnapApply (windows-like.lua) places windows.
// Three panes: the drag overlay (no input), the flyout (hover: a small surface without keyboard; Win+Z: full
// screen with keyboard, a click away closes it), Snap Assist (full screen, keyboard).
use crate::anim::{Anim, Ease};
use crate::desktop::{border, line_h, rect, text};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::time::Instant;

pub type Z = [f32; 4];
pub const LAYOUTS: [&[Z]; 7] = [
    &[[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 1.0]],
    &[[0.0, 0.0, 0.667, 1.0], [0.667, 0.0, 0.333, 1.0]],
    &[[0.0, 0.0, 0.333, 1.0], [0.333, 0.0, 0.334, 1.0], [0.667, 0.0, 0.333, 1.0]],
    &[[0.0, 0.0, 0.5, 0.5], [0.5, 0.0, 0.5, 0.5], [0.0, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]],
    &[[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]],
    &[[0.0, 0.0, 0.25, 1.0], [0.25, 0.0, 0.5, 1.0], [0.75, 0.0, 0.25, 1.0]],
    &[[0.0, 0.0, 1.0, 1.0]],
];
const BAR_TOP: f32 = 40.0; // the Mist top bar (reserved) when Hyprland does not report it
const GAP: f32 = 8.0;
const CORNER: f32 = 120.0; // corner zone along an edge -> quarter
const PILL_SHOW_Y: f32 = 240.0; // window dragged this close to the top -> the Snap bar appears
const BAR_W: f32 = 736.0;
const BAR_H: f32 = 96.0;
const ACCENT_STRONG: Rgba = Rgba::rgb8(0x40, 0x9C, 0xFF, 1.0);
const ACCENT_SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);
const RIM: Rgba = Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16);
const M_FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);

fn a(c: Rgba, k: f32) -> Rgba {
    Rgba(c.0, c.1, c.2, c.3 * k)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn same(a: &Z, b: &Z) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}
pub fn norm(a: &str) -> String {
    format!("0x{}", a.trim_start_matches("0x"))
}
// every zone of every layout in reading order (keyboard navigation in the flyout)
pub fn flat() -> Vec<(Z, usize)> {
    LAYOUTS.iter().enumerate().flat_map(|(li, l)| l.iter().map(move |z| (*z, li))).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Mon {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub res: [f32; 4], // reserved left, top, right, bottom (same as workarea() in windows-like.lua)
    pub ws: i64,
    pub special: i64,
}
pub fn monitors() -> Vec<Mon> {
    let Some(Value::Array(ms)) = crate::sys::hypr_request("j/monitors") else { return Vec::new() };
    ms.iter()
        .map(|m| {
            let f = |k: &str| m[k].as_f64().unwrap_or(0.0) as f32;
            let sc = m["scale"].as_f64().unwrap_or(1.0).max(0.1) as f32;
            let r: Vec<f32> = m["reserved"].as_array().map(|r| r.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
            Mon {
                name: m["name"].as_str().unwrap_or("").into(),
                x: f("x"),
                y: f("y"),
                w: f("width") / sc,
                h: f("height") / sc,
                res: if r.len() == 4 { [r[0], r[1], r[2], r[3]] } else { [0.0, BAR_TOP, 0.0, 0.0] },
                ws: m["activeWorkspace"]["id"].as_i64().unwrap_or(0),
                special: m["specialWorkspace"]["id"].as_i64().unwrap_or(0),
            }
        })
        .collect()
}
pub fn cursor() -> Option<(f32, f32)> {
    let v = crate::sys::hypr_request("j/cursorpos")?;
    Some((v["x"].as_f64()? as f32, v["y"].as_f64()? as f32))
}
// zone -> rectangle in monitor-local coordinates
pub fn zone_rect(m: &Mon, z: &Z) -> (f32, f32, f32, f32) {
    let r = m.res;
    let (ax, ay) = (r[0] + GAP, r[1] + GAP);
    let (aw, ah) = (m.w - r[0] - r[2] - 2.0 * GAP, m.h - r[1] - r[3] - 2.0 * GAP);
    ((ax + z[0] * (aw + GAP)).floor(), (ay + z[1] * (ah + GAP)).floor(), (z[2] * (aw + GAP) - GAP).floor(), (z[3] * (ah + GAP) - GAP).floor())
}
// the rest of the layout a window was snapped into: the chosen layout, else halves / quarters
pub fn remaining(z: &Z, li: i32) -> Vec<Z> {
    let has = |l: &[Z]| l.iter().any(|o| same(o, z));
    let mut l: Option<&[Z]> = if li >= 0 { LAYOUTS.get(li as usize).copied() } else { None };
    if !l.map(has).unwrap_or(false) {
        l = if z[2] == 0.5 && z[3] == 0.5 {
            Some(LAYOUTS[3])
        } else if z[2] == 0.5 && z[3] == 1.0 {
            Some(LAYOUTS[0])
        } else {
            LAYOUTS.iter().find(|o| o.len() > 1 && has(o)).copied()
        };
    }
    match l {
        Some(l) if has(l) => l.iter().filter(|o| !same(o, z)).copied().collect(),
        _ => Vec::new(),
    }
}
// MistSnapApply (windows-like.lua) after `delay` ms: from a surface with keyboard focus Hyprland only focuses the
// window once that surface is gone
pub fn apply(addr: &str, z: &Z, grab: Option<[f32; 4]>, pre: &str, delay: u64) {
    let g = grab.map(|g| format!(", {}, {}, {}, {}", g[0], g[1], g[2], g[3])).unwrap_or_default();
    let lua = format!("{pre}MistSnapApply(\"{}\", {}, {}, {}, {}{g})", norm(addr), z[0], z[1], z[2], z[3]);
    let _ = std::thread::Builder::new().name("snap-apply".into()).stack_size(64 * 1024).spawn(move || {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        let _ = std::process::Command::new("hyprctl").args(["eval", &lua]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    });
}

// what a snap pane asks the app for
#[derive(Clone, Debug)]
pub enum Cmd {
    Fly(String, Z, usize),       // window, zone, layout
    AssistPick(String, String),  // window, its workspace name
    AssistEnd,
}

// ---------------------------------------------------------------- the drag overlay
#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub enum Mode {
    #[default]
    None,
    Layouts,
    Edge,
}
pub struct Drag {
    pub addr: String,
    pub grab: Option<[f32; 4]>, // its geometry when grabbed = the restore size after snapping
    pub mons: Vec<Mon>,
    pub scr: usize,
    pub cx: f32,
    pub cy: f32,
    pub mode: Mode,
    pub pill: bool,
    pub hot: Option<Z>,
    pub hot_layout: i32,
    pub target: Option<Z>,
    pub preview: bool, // ipc snap preview: the layouts panel without tracking
    // the preview rectangle grows out of the cursor
    from: (f32, f32, f32, f32),
    to: (f32, f32, f32, f32),
    shown_zone: Option<Z>,
    k: Anim,
    vis: Anim,
    bar: Anim,
    exp: Anim,
}
impl Drag {
    pub fn new(addr: &str, grab: Option<[f32; 4]>, mons: Vec<Mon>, x: f32, y: f32) -> Drag {
        let mut d = Drag {
            addr: norm(addr),
            grab,
            mons,
            scr: 0,
            cx: x,
            cy: y,
            mode: Mode::None,
            pill: false,
            hot: None,
            hot_layout: -1,
            target: None,
            preview: false,
            from: (0.0, 0.0, 0.0, 0.0),
            to: (0.0, 0.0, 0.0, 0.0),
            shown_zone: None,
            k: Anim::new(1.0),
            vis: Anim::new(0.0),
            bar: Anim::new(0.0),
            exp: Anim::new(0.0),
        };
        d.track(x, y);
        d
    }
    pub fn mon(&self) -> &Mon {
        &self.mons[self.scr.min(self.mons.len().saturating_sub(1))]
    }
    pub fn zone(&self) -> Option<Z> {
        match self.mode {
            Mode::Layouts => self.hot.or(self.target),
            Mode::Edge => self.target,
            Mode::None => None,
        }
    }
    fn hit_bar(&self, x: f32, y: f32, s: &Mon) -> Option<(Z, usize)> {
        let mid = s.x + s.w / 2.0;
        let (sx, sy) = (mid - BAR_W / 2.0 + 16.0, s.y + 8.0 + 22.0);
        let (cw, ch, sp) = (106.0, 64.0, 10.0);
        if y < sy || y > sy + ch {
            return None;
        }
        for i in 0..6 {
            let cx = sx + i as f32 * (cw + sp);
            if x >= cx && x <= cx + cw {
                let rx = ((x - (cx + 4.0)) / 98.0).clamp(0.0, 0.999);
                let ry = ((y - (sy + 4.0)) / 56.0).clamp(0.0, 0.999);
                let l = LAYOUTS[i];
                let f = l.iter().find(|z| rx >= z[0] && rx < z[0] + z[2] && ry >= z[1] && ry < z[1] + z[3]);
                return Some((*f.unwrap_or(&l[0]), i));
            }
        }
        None
    }
    // returns true when the cursor moved onto another monitor
    pub fn track(&mut self, x: f32, y: f32) -> bool {
        self.cx = x;
        self.cy = y;
        let old = self.scr;
        if let Some(i) = self.mons.iter().position(|m| x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h) {
            self.scr = i;
        }
        if self.mons.is_empty() {
            return false;
        }
        let s = self.mon().clone();
        let others: Vec<&Mon> = self.mons.iter().enumerate().filter(|(i, _)| *i != self.scr).map(|m| m.1).collect();
        let span_y = |o: &Mon| o.y < s.y + s.h && o.y + o.h > s.y;
        let span_x = |o: &Mon| o.x < s.x + s.w && o.x + o.w > s.x;
        // edges shared with another monitor do not snap (the cursor crosses over), like Windows
        let outer_left = !others.iter().any(|o| o.x + o.w == s.x && span_y(o));
        let outer_right = !others.iter().any(|o| o.x == s.x + s.w && span_y(o));
        let outer_top = !others.iter().any(|o| o.y + o.h == s.y && span_x(o));
        let mid = s.x + s.w / 2.0;
        let top_th = s.y + BAR_TOP + 10.0;
        // the Snap bar handle: window / cursor in the upper area near the centre
        self.pill = y < s.y + PILL_SHOW_Y && (x - mid).abs() < s.w / 2.0 - CORNER;
        let in_bar = y <= s.y + BAR_H + 36.0 && (x - mid).abs() <= if self.mode == Mode::Layouts { BAR_W / 2.0 + 50.0 } else { 160.0 };
        if self.pill && in_bar && (self.mode == Mode::Layouts || y <= s.y + 60.0) {
            self.mode = Mode::Layouts;
            match self.hit_bar(x, y, &s) {
                Some((z, li)) => {
                    self.hot = Some(z);
                    self.hot_layout = li as i32;
                    self.target = None;
                }
                None => {
                    self.hot = None;
                    self.hot_layout = -1;
                    // in the top bar without a zone: maximize
                    self.target = (y <= top_th && outer_top).then_some([0.0, 0.0, 1.0, 1.0]);
                }
            }
        } else {
            self.mode = Mode::None;
            self.hot = None;
            self.hot_layout = -1;
            let at_top = outer_top && y <= top_th;
            let at_left = outer_left && x <= s.x + 10.0;
            let at_right = outer_right && x >= s.x + s.w - 12.0;
            let (top, bottom) = (y < s.y + CORNER, y >= s.y + s.h - CORNER);
            let (left, right) = (x < s.x + CORNER, x >= s.x + s.w - CORNER);
            let z = if at_left {
                Some(if top { [0.0, 0.0, 0.5, 0.5] } else if bottom { [0.0, 0.5, 0.5, 0.5] } else { [0.0, 0.0, 0.5, 1.0] })
            } else if at_right {
                Some(if top { [0.5, 0.0, 0.5, 0.5] } else if bottom { [0.5, 0.5, 0.5, 0.5] } else { [0.5, 0.0, 0.5, 1.0] })
            } else if at_top {
                Some(if left && outer_left { [0.0, 0.0, 0.5, 0.5] } else if right && outer_right { [0.5, 0.0, 0.5, 0.5] } else { [0.0, 0.0, 1.0, 1.0] })
            } else {
                None
            };
            self.target = z;
            self.mode = if z.is_some() { Mode::Edge } else { Mode::None };
        }
        self.retarget();
        self.scr != old
    }
    // the drop: the zone to apply (the top bar without a zone = maximize / quarters), with its layout
    pub fn end(&mut self, x: f32, y: f32) -> Option<(Z, i32)> {
        if !x.is_nan() && !y.is_nan() {
            self.track(x, y);
        }
        let mut z = self.zone();
        if z.is_none() && !self.mons.is_empty() {
            let s = self.mon();
            if y <= s.y + BAR_TOP + 14.0 {
                let (left, right) = (x < s.x + CORNER, x >= s.x + s.w - CORNER);
                z = Some(if left { [0.0, 0.0, 0.5, 0.5] } else if right { [0.5, 0.0, 0.5, 0.5] } else { [0.0, 0.0, 1.0, 1.0] });
            }
        }
        z.map(|z| (z, if self.mode == Mode::Layouts && self.hot.is_some() { self.hot_layout } else { -1 }))
    }
    // animate the preview / the bar towards the new state
    fn retarget(&mut self) {
        let z = self.zone();
        let bar_on = self.pill || self.mode == Mode::Layouts || self.preview;
        if self.bar.target() != if bar_on { 1.0 } else { 0.0 } {
            self.bar.set(if bar_on { 1.0 } else { 0.0 }, 200.0, Ease::OutCubic);
        }
        let ex = if self.mode == Mode::Layouts { 1.0 } else { 0.0 };
        if self.exp.target() != ex {
            self.exp.set(ex, 200.0, Ease::OutCubic);
        }
        let same_z = match (&z, &self.shown_zone) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => true,
            _ => false,
        };
        if same_z || self.mons.is_empty() {
            return;
        }
        let m = self.mon().clone();
        let cur = self.rect_now();
        let (px, py) = (self.cx - m.x, self.cy - m.y);
        self.from = if self.shown_zone.is_none() || self.vis.value() < 0.05 { (px, py, 0.0, 0.0) } else { cur };
        self.to = match &z {
            Some(z) => zone_rect(&m, z),
            None => cur,
        };
        self.shown_zone = z;
        self.k = Anim::new(0.0);
        self.k.set(1.0, 170.0, Ease::OutCubic);
        self.vis.set(if z.is_some() { 1.0 } else { 0.0 }, 130.0, Ease::Linear);
    }
    fn rect_now(&self) -> (f32, f32, f32, f32) {
        let k = self.k.value();
        (lerp(self.from.0, self.to.0, k), lerp(self.from.1, self.to.1, k), lerp(self.from.2, self.to.2, k), lerp(self.from.3, self.to.3, k))
    }
    // the overlay moved to another monitor: no animation from the old one
    pub fn moved(&mut self) {
        self.shown_zone = None;
        self.vis = Anim::new(0.0);
        self.retarget();
    }
    pub fn set_preview(&mut self, on: bool, edge: bool) {
        self.preview = on;
        if edge {
            self.mode = if on { Mode::Edge } else { Mode::None };
            self.target = on.then_some([0.0, 0.0, 1.0, 1.0]);
        } else {
            self.hot = on.then_some([0.5, 0.0, 0.5, 1.0]);
        }
        self.retarget();
    }
}

pub struct DragPane(pub Drag);
impl Pane for DragPane {
    fn name(&self) -> &'static str {
        "snapdrag"
    }
    fn namespace(&self) -> &'static str {
        "mist-snap"
    }
    fn keyboard(&self) -> bool {
        false
    }
    fn input(&self) -> bool {
        false
    }
    fn as_drag(&mut self) -> Option<&mut Drag> {
        Some(&mut self.0)
    }
    fn animating(&self) -> bool {
        let d = &self.0;
        d.k.running() || d.vis.running() || d.bar.running() || d.exp.running()
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let d = &self.0;
        c.clear();
        // the translucent preview of where the window will land (Windows 11 acrylic snap preview)
        let o = d.vis.value();
        if o > 0.01 {
            let (x, y, w, h) = d.rect_now();
            if w > 2.0 && h > 2.0 {
                rect(c, x, y, w, h, 14.0, Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.16 * o));
                border(c, x, y, w, h, 14.0, 1.5, Rgba(64.0 / 255.0, 156.0 / 255.0, 1.0, 0.65 * o));
                border(c, x + 1.0, y + 1.0, w - 2.0, h - 2.0, 13.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.12 * o));
            }
        }
        // the Snap bar (top centre): a pill that expands into Snap Layouts
        let b = d.bar.value();
        if b < 0.01 {
            return;
        }
        let e = d.exp.value();
        let t = &mut *cx.t;
        let ps = Style::ui(12.0, 500.0);
        let pill_w = (13.0 + 7.0 + t.width("Układy przyciągania", &ps) + 7.0 + 11.0 + 24.0).max(180.0);
        let (w, h, r) = (lerp(pill_w, BAR_W, e), lerp(30.0, BAR_H, e), lerp(15.0, 18.0, e));
        let x = ((c.w() - w) / 2.0).round();
        let y = lerp(-h - 16.0, 6.0, b);
        let mut l = Canvas::new(w.ceil() as u32, h.ceil() as u32);
        rect(&mut l, 0.0, 0.0, w, h, r, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.94));
        rect(&mut l, r, 0.0, w - 2.0 * r, 1.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.18));
        // collapsed: icon, label, chevron
        if e < 0.99 {
            let k = 1.0 - e;
            let tw = t.width("Układy przyciągania", &ps);
            let rw = 13.0 + 7.0 + tw + 7.0 + 11.0;
            let rx = (w - rw) / 2.0;
            l.icon(t, rx + 6.5, h / 2.0, "layoutGrid", 13.0, a(ACCENT, k));
            let lh = line_h(t, &ps);
            text(&mut l, t, rx + 20.0, h / 2.0 - lh / 2.0, "Układy przyciągania", &ps, a(M_FG, k), false, None);
            l.icon(t, rx + 20.0 + tw + 7.0 + 5.5, h / 2.0, "down", 11.0, a(T3, k));
        }
        // expanded: the header and six layout cards
        if e > 0.01 {
            let ox = (w - BAR_W) / 2.0;
            let hs = Style::ui(11.0, 500.0);
            let hw = 12.0 + 6.0 + t.width("Wybierz układ rozmieszczenia okna", &hs);
            let hx = ox + (BAR_W - hw) / 2.0;
            l.icon(t, hx + 6.0, 5.0 + 7.0, "layoutGrid", 12.0, a(ACCENT, e));
            let hh = line_h(t, &hs);
            text(&mut l, t, hx + 18.0, 5.0 + 7.0 - hh / 2.0, "Wybierz układ rozmieszczenia okna", &hs, a(T2, e), false, None);
            for (i, lay) in LAYOUTS.iter().take(6).enumerate() {
                let (cx0, cy0) = (ox + 16.0 + i as f32 * 116.0, 22.0);
                rect(&mut l, cx0, cy0, 106.0, 64.0, 8.0, Rgba(1.0, 1.0, 1.0, 0.04 * e));
                border(&mut l, cx0, cy0, 106.0, 64.0, 8.0, 1.0, a(if d.hot_layout == i as i32 { ACCENT_SOFT } else { HAIR }, e));
                for z in lay.iter() {
                    let hot = d.hot_layout == i as i32 && d.hot.map(|h| same(&h, z)).unwrap_or(false);
                    let (zx, zy, zw, zh) = (cx0 + 4.0 + z[0] * 98.0, cy0 + 4.0 + z[1] * 56.0, (z[2] * 98.0 - 3.0).max(4.0), (z[3] * 56.0 - 3.0).max(4.0));
                    rect(&mut l, zx, zy, zw, zh, 4.0, if hot { a(ACCENT, e) } else { Rgba(1.0, 1.0, 1.0, 0.12 * e) });
                    border(&mut l, zx, zy, zw, zh, 4.0, 1.0, if hot { a(ACCENT_STRONG, e) } else { Rgba(1.0, 1.0, 1.0, 0.08 * e) });
                }
            }
        }
        border(&mut l, 0.0, 0.0, w, h, r, 1.0, if e > 0.5 { RIM } else { Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.18) });
        // clip to the rounded bar
        if let Some(p) = crate::gfx::rrect_path(0.0, 0.0, w, h, r) {
            if let Some(mut m) = tiny_skia::Mask::new(l.pm.width(), l.pm.height()) {
                m.fill_path(&p, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
                l.pm.apply_mask(&m);
            }
        }
        let paint = tiny_skia::PixmapPaint { opacity: b.min(1.0), ..Default::default() };
        c.pm.draw_pixmap(x as i32, y.round() as i32, l.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
    }
    fn pointer(&mut self, _ev: Ptr, _x: f32, _y: f32, _cx: &mut Ctx) -> Act {
        Act::None
    }
    fn key(&mut self, _ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        Act::None
    }
}

// ---------------------------------------------------------------- the Snap Layouts flyout
const FLY_W: f32 = 4.0 * 132.0 + 3.0 * 12.0 + 28.0;
const FLY_H: f32 = 2.0 * 76.0 + 12.0 + 28.0;
const PAD: f32 = 24.0; // room for the shadow around the hover flyout
pub struct Fly {
    pub addr: String,
    pinned: bool,
    mon: Mon,
    pos: (f32, f32), // the body in surface coordinates
    idx: i32,        // into flat() (hover / keyboard)
    open: Anim,
    btn: bool,                  // the pointer is on the title bar's maximize button
    left_at: Option<Instant>,   // ... or left it then
    inside: bool,
    flat: Vec<(Z, usize)>,
}
impl Fly {
    pub fn new(addr: &str, x: f32, y: f32, pinned: bool, mons: &[Mon]) -> Fly {
        let mon = mons.iter().find(|m| x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h).or(mons.first()).cloned().unwrap_or_default();
        let bx = (x - mon.x - 30.0).min(mon.w - FLY_W - 8.0).max(8.0);
        let by = (y - mon.y + 8.0).min(mon.h - FLY_H - 8.0).max(8.0);
        let mut open = Anim::new(0.0);
        open.set(1.0, 150.0, Ease::OutCubic);
        Fly { addr: norm(addr), pinned, mon, pos: if pinned { (bx, by) } else { (PAD, PAD) }, idx: if pinned { 0 } else { -1 }, open, btn: !pinned, left_at: None, inside: false, flat: flat() }
    }
    pub fn screen(&self) -> &str {
        &self.mon.name
    }
    // the hover flyout's surface: (left, top, w, h) on its monitor
    pub fn surface(&self, x: f32, y: f32) -> (i32, i32, u32, u32) {
        let bx = (x - self.mon.x - 30.0).min(self.mon.w - FLY_W - 8.0).max(8.0);
        let by = (y - self.mon.y + 8.0).min(self.mon.h - FLY_H - 8.0).max(8.0);
        ((bx - PAD) as i32, (by - PAD) as i32, (FLY_W + 2.0 * PAD) as u32, (FLY_H + 2.0 * PAD) as u32)
    }
    fn zone_at(&self, px: f32, py: f32) -> Option<usize> {
        let (gx, gy) = (px - self.pos.0 - 14.0, py - self.pos.1 - 14.0);
        self.flat.iter().position(|(z, li)| {
            let (ox, oy) = ((li % 4) as f32 * 144.0, (li / 4) as f32 * 88.0);
            let (x, y, w, h) = (ox + 7.0 + z[0] * 122.0, oy + 7.0 + z[1] * 66.0, z[2] * 122.0 - 4.0, z[3] * 66.0 - 4.0);
            gx >= x - 2.0 && gx <= x + w + 2.0 && gy >= y - 2.0 && gy <= y + h + 2.0
        })
    }
    fn pick(&self) -> Act {
        match self.flat.get(self.idx.max(0) as usize) {
            Some((z, li)) if self.idx >= 0 => Act::Snap(Cmd::Fly(self.addr.clone(), *z, *li)),
            _ => Act::None,
        }
    }
}
pub struct FlyPane(pub Fly, pub (i32, i32, u32, u32));
impl Pane for FlyPane {
    fn name(&self) -> &'static str {
        "snapfly"
    }
    fn namespace(&self) -> &'static str {
        "mist-popup"
    }
    fn keyboard(&self) -> bool {
        self.0.pinned
    }
    fn rect(&self) -> Option<(i32, i32, u32, u32)> {
        (!self.0.pinned).then_some(self.1)
    }
    fn animating(&self) -> bool {
        self.0.open.running()
    }
    fn hand(&self) -> bool {
        self.0.idx >= 0 && self.0.inside
    }
    // maxhover / maxleave from the title bar
    fn data(&mut self, what: &str) -> bool {
        match what {
            "maxhover" => {
                self.0.btn = true;
                self.0.left_at = None;
            }
            "maxleave" => {
                self.0.btn = false;
                self.0.left_at = Some(Instant::now());
            }
            _ => {}
        }
        false
    }
    // the hover flyout closes 320 ms after the pointer left both the button and the flyout
    fn timer(&mut self) -> Act {
        let f = &self.0;
        if !f.pinned && !f.btn && !f.inside && f.left_at.map(|t| t.elapsed().as_millis() >= 320).unwrap_or(true) {
            return Act::Close;
        }
        Act::None
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let f = &self.0;
        c.clear();
        let k = f.open.value();
        let (fx, fy) = f.pos;
        let mut l = Canvas::new(FLY_W as u32 + 2 * PAD as u32, FLY_H as u32 + 2 * PAD as u32);
        crate::glass::Glass { radius: 18.0, tint: 0.9, ..Default::default() }.paint(&mut l, PAD, PAD, FLY_W, FLY_H);
        for (li, lay) in LAYOUTS.iter().enumerate() {
            let (ox, oy) = (PAD + 14.0 + (li % 4) as f32 * 144.0, PAD + 14.0 + (li / 4) as f32 * 88.0);
            rect(&mut l, ox, oy, 132.0, 76.0, 12.0, Rgba(1.0, 1.0, 1.0, 0.04));
            border(&mut l, ox, oy, 132.0, 76.0, 12.0, 1.0, HAIR);
            for z in lay.iter() {
                let hot = f.idx >= 0 && f.flat.get(f.idx as usize).map(|(hz, hl)| *hl == li && same(hz, z)).unwrap_or(false);
                let (x, y, w, h) = (ox + 5.0 + z[0] * 122.0 + 2.0, oy + 5.0 + z[1] * 66.0 + 2.0, z[2] * 122.0 - 4.0, z[3] * 66.0 - 4.0);
                rect(&mut l, x, y, w, h, 6.0, if hot { ACCENT } else { Rgba(1.0, 1.0, 1.0, 0.12) });
                border(&mut l, x, y, w, h, 6.0, 1.0, if hot { ACCENT_STRONG } else { Rgba(1.0, 1.0, 1.0, 0.08) });
            }
        }
        let _ = &cx;
        // opacity + scale .9 -> 1 from the top left
        let s = 0.9 + 0.1 * k;
        let paint = tiny_skia::PixmapPaint { opacity: k.min(1.0), quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        let (bx, by) = (fx - PAD, fy - PAD);
        let tf = tiny_skia::Transform::from_row(s, 0.0, 0.0, s, bx + PAD * (1.0 - s), by + PAD * (1.0 - s));
        c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &paint, tf, None);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let f = &mut self.0;
        let body = inside((x, y), (f.pos.0, f.pos.1, FLY_W, FLY_H));
        f.inside = body && !matches!(ev, Ptr::Leave);
        if !f.inside && !f.btn {
            f.left_at.get_or_insert(Instant::now());
        } else {
            f.left_at = None;
        }
        match ev {
            Ptr::Motion | Ptr::Enter => {
                let z = if body { f.zone_at(x, y).map(|i| i as i32).unwrap_or(-1) } else { -1 };
                if z != f.idx && (z >= 0 || !f.pinned) {
                    f.idx = z;
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Leave => {
                if !f.pinned && f.idx >= 0 {
                    f.idx = -1;
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Release(_) => {
                if !body {
                    return if f.pinned { Act::Close } else { Act::None };
                }
                match f.zone_at(x, y) {
                    Some(i) => {
                        f.idx = i as i32;
                        f.pick()
                    }
                    None => Act::None,
                }
            }
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        let f = &mut self.0;
        let n = f.flat.len() as i32;
        let cur = f.idx.max(0);
        match ev.keysym {
            Keysym::Escape => return Act::Close,
            Keysym::Right | Keysym::Tab => f.idx = (cur + 1) % n,
            Keysym::Left | Keysym::ISO_Left_Tab => f.idx = (cur - 1 + n) % n,
            Keysym::Down | Keysym::Up => {
                // the first zone of the layout in the next / previous row
                let li = f.flat[cur as usize].1 as i32;
                let to = (li + if ev.keysym == Keysym::Down { 4 } else { -4 }).clamp(0, LAYOUTS.len() as i32 - 1) as usize;
                f.idx = f.flat.iter().position(|z| z.1 == to).unwrap_or(0) as i32;
            }
            Keysym::Return | Keysym::KP_Enter | Keysym::space => return f.pick(),
            k => {
                let d = k.raw() as i32 - Keysym::_1.raw() as i32;
                if (0..7).contains(&d) {
                    f.idx = f.flat.iter().position(|z| z.1 == d as usize).unwrap_or(0) as i32;
                } else {
                    return Act::None;
                }
            }
        }
        Act::Redraw
    }
}

// ---------------------------------------------------------------- Snap Assist (the window picker in the free zone)
#[derive(Clone, Debug)]
pub struct Win {
    pub addr: String,
    pub class: String,
    pub title: String,
    pub ws_name: String,
}
// the app's Snap Assist state between picks
#[derive(Clone, Debug, Default)]
pub struct Assist {
    pub mon: String,
    pub zones: Vec<Z>,
    pub group: Vec<String>,
}
impl Assist {
    // after Hyprland reports fresh geometry: drop the zones already filled, collect the candidates
    pub fn candidates(&mut self) -> Option<(Mon, Vec<Win>)> {
        let mons = monitors();
        let m = mons.iter().find(|m| m.name == self.mon)?.clone();
        let Some(Value::Array(cs)) = crate::sys::hypr_request("j/clients") else { return None };
        // workspaces on screen: each monitor's active one plus an open special workspace (e.g. "gaming")
        let shown: Vec<i64> = mons.iter().flat_map(|m| [m.ws, m.special]).filter(|i| *i != 0).collect();
        let filled = |z: &Z| {
            let r = zone_rect(&m, z);
            cs.iter().any(|o| {
                let (Some(at), Some(sz)) = (o["at"].as_array(), o["size"].as_array()) else { return false };
                let f = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
                let (x, y, w, h) = (f(&at[0]) - m.x, f(&at[1]) - m.y, f(&sz[0]), f(&sz[1]));
                // at / size exclude the title bar drawn above the window: left, right, bottom match; top within a bar
                (x - r.0).abs() <= 4.0 && (x + w - r.0 - r.2).abs() <= 4.0 && (y + h - r.1 - r.3).abs() <= 4.0 && y - r.1 >= -4.0 && y - r.1 <= 48.0
            })
        };
        self.zones.retain(|z| !filled(z));
        let wins: Vec<Win> = cs
            .iter()
            .filter(|o| o["mapped"].as_bool().unwrap_or(true) && !o["hidden"].as_bool().unwrap_or(false))
            .filter(|o| !self.group.contains(&norm(o["address"].as_str().unwrap_or(""))))
            .filter(|o| shown.contains(&o["workspace"]["id"].as_i64().unwrap_or(0)) || o["workspace"]["name"].as_str() == Some("special:minimized"))
            .map(|o| Win { addr: norm(o["address"].as_str().unwrap_or("")), class: o["class"].as_str().unwrap_or("").into(), title: o["title"].as_str().unwrap_or("").into(), ws_name: o["workspace"]["name"].as_str().unwrap_or("").into() })
            .collect();
        if self.zones.is_empty() || wins.is_empty() {
            return None;
        }
        Some((m, wins))
    }
}

pub struct AssistPane {
    mon: Mon,
    zone: Z,
    wins: Vec<Win>,
    sel: usize,
    scroll: f32,
    cols: usize,
    hits: Vec<((f32, f32, f32, f32), usize)>,
    ptr: (f32, f32),
    pressed_out: bool,
    open: Anim,
}
impl AssistPane {
    pub fn new(mon: Mon, zone: Z, wins: Vec<Win>) -> AssistPane {
        let mut open = Anim::new(0.0);
        open.set(1.0, 180.0, Ease::OutCubic);
        AssistPane { mon, zone, wins, sel: 0, scroll: 0.0, cols: 1, hits: Vec::new(), ptr: (-1.0, -1.0), pressed_out: false, open }
    }
    pub fn titles(&self) -> Vec<String> {
        self.wins.iter().map(|w| w.title.clone()).collect()
    }
    pub fn pick(&self, i: usize) -> Act {
        match self.wins.get(i) {
            Some(w) => Act::Snap(Cmd::AssistPick(w.addr.clone(), w.ws_name.clone())),
            None => Act::None,
        }
    }
    fn area(&self) -> (f32, f32, f32, f32) {
        zone_rect(&self.mon, &self.zone)
    }
}
impl Pane for AssistPane {
    fn name(&self) -> &'static str {
        "snapassist"
    }
    fn namespace(&self) -> &'static str {
        "mist-snap-assist"
    }
    fn animating(&self) -> bool {
        self.open.running()
    }
    fn hand(&self) -> bool {
        self.hits.iter().any(|h| inside(self.ptr, h.0))
    }
    fn as_assist(&mut self) -> Option<&mut AssistPane> {
        Some(self)
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hits.clear();
        let (zx, zy, zw, zh) = self.area();
        let k = self.open.value();
        let mut l = Canvas::new(zw as u32 + 48, zh as u32 + 48);
        crate::glass::Glass { radius: 12.0, tint: 0.55, ..Default::default() }.paint(&mut l, 24.0, 24.0, zw, zh);
        // the cards: a centred flow, scrolled when it does not fit
        let (fw, fh) = (zw - 40.0, zh - 40.0);
        let sp = 14.0;
        let n = self.wins.len();
        let cols = ((((fw + sp) / (220.0 + sp)).floor() as usize).min(n)).max(1);
        self.cols = cols;
        let cw = ((fw - (cols - 1) as f32 * sp) / cols as f32).clamp(150.0, 280.0);
        let th = (cw - 16.0) * 0.6;
        let ch = 34.0 + th.round() + 10.0;
        let rows = n.div_ceil(cols);
        let total_h = rows as f32 * ch + (rows.saturating_sub(1)) as f32 * sp;
        let max_sc = (total_h - fh).max(0.0);
        // keep the selection in view
        let sel_y = (self.sel / cols) as f32 * (ch + sp);
        if sel_y < self.scroll {
            self.scroll = sel_y;
        } else if sel_y + ch > self.scroll + fh {
            self.scroll = sel_y + ch - fh;
        }
        self.scroll = self.scroll.clamp(0.0, max_sc);
        let row_w = cols as f32 * cw + (cols - 1) as f32 * sp;
        let (ox, oy) = (24.0 + 20.0 + ((fw - row_w) / 2.0).max(0.0), 24.0 + 20.0 + ((fh - total_h) / 2.0).max(0.0) - self.scroll);
        let mut page = Canvas::new(fw as u32, fh as u32);
        let (px, py) = (24.0 + 20.0, 24.0 + 20.0);
        for (i, w) in self.wins.iter().enumerate() {
            let (x, y) = (ox + (i % cols) as f32 * (cw + sp) - px, oy + (i / cols) as f32 * (ch + sp) - py);
            if y + ch < 0.0 || y > fh {
                continue;
            }
            let sx = zx - 24.0 + px + x;
            let sy = zy - 24.0 + py + y;
            let hov = inside(self.ptr, (sx, sy, cw, ch));
            let sel = i == self.sel;
            rect(&mut page, x, y, cw, ch, 10.0, Rgba(1.0, 1.0, 1.0, if sel || hov { 0.1 } else { 0.03 }));
            border(&mut page, x, y, cw, ch, 10.0, if sel { 2.0 } else { 1.0 }, if sel { ACCENT } else { HAIR });
            let icon = cx.apps.lookup(&w.class).map(|e| e.icon.clone()).unwrap_or(w.class.to_lowercase());
            match cx.imgs.get(&icon, 36, cx.icons) {
                Some(pm) => crate::img::draw(&mut page, &pm, x + 10.0, y + 8.0, 18.0, 18.0, 0.0, false, 1.0),
                None => page.icon(cx.t, x + 19.0, y + 17.0, "apps", 16.0, T2),
            }
            let t = &mut *cx.t;
            let title = if !w.title.is_empty() { &w.title } else { &w.class };
            let ts = Style::ui(12.5, 500.0);
            let lh = line_h(t, &ts);
            text(&mut page, t, x + 36.0, y + 17.0 - lh / 2.0, title, &ts, T1, false, Some(cw - 46.0));
            rect(&mut page, x + 8.0, y + 34.0, cw - 16.0, th.round(), 6.0, Rgba(0.0, 0.0, 0.0, 0.25));
            match cx.imgs.get(&icon, 96, cx.icons) {
                Some(pm) => crate::img::draw(&mut page, &pm, x + cw / 2.0 - 24.0, y + 34.0 + th.round() / 2.0 - 24.0, 48.0, 48.0, 0.0, false, 1.0),
                None => page.icon(cx.t, x + cw / 2.0, y + 34.0 + th.round() / 2.0, "apps", 40.0, T2),
            }
            let (a0, b0) = (sy.max(zy + 20.0), (sy + ch).min(zy + 20.0 + fh));
            if b0 > a0 {
                self.hits.push(((sx, a0, cw, b0 - a0), i));
            }
        }
        l.pm.draw_pixmap(px as i32, py as i32, page.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        let paint = tiny_skia::PixmapPaint { opacity: k.min(1.0), ..Default::default() };
        let s = 0.97 + 0.03 * k;
        let (lw, lh) = (l.pm.width() as f32, l.pm.height() as f32);
        let tf = tiny_skia::Transform::from_row(s, 0.0, 0.0, s, zx - 24.0 + lw * (1.0 - s) / 2.0, zy - 24.0 + lh * (1.0 - s) / 2.0);
        c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &paint, tf, None);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let (zx, zy, zw, zh) = self.area();
        let in_zone = inside((x, y), (zx, zy, zw, zh));
        let hit = self.hits.iter().rev().find(|h| inside((x, y), h.0)).map(|h| h.1);
        match ev {
            Ptr::Motion | Ptr::Enter => {
                if let Some(i) = hit {
                    self.sel = i;
                }
                Act::Redraw
            }
            Ptr::Leave => Act::Redraw,
            Ptr::Press(_) => {
                self.pressed_out = !in_zone;
                Act::None
            }
            Ptr::Release(_) => {
                // a click outside the free zone dismisses Snap Assist (like Windows)
                if self.pressed_out && !in_zone {
                    return Act::Snap(Cmd::AssistEnd);
                }
                match hit {
                    Some(i) => self.pick(i),
                    None => Act::None,
                }
            }
            Ptr::Wheel(d) => {
                self.scroll += d * 60.0;
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        let n = self.wins.len().max(1);
        match ev.keysym {
            Keysym::Escape => return Act::Snap(Cmd::AssistEnd),
            Keysym::Right | Keysym::Tab => self.sel = (self.sel + 1) % n,
            Keysym::Left | Keysym::ISO_Left_Tab => self.sel = (self.sel + n - 1) % n,
            Keysym::Down => self.sel = (self.sel + self.cols).min(n - 1),
            Keysym::Up => self.sel = self.sel.saturating_sub(self.cols),
            Keysym::Return | Keysym::KP_Enter | Keysym::space => return self.pick(self.sel),
            _ => return Act::None,
        }
        Act::Redraw
    }
}
