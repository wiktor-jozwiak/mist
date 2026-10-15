//! Server-side macOS title bars (replaces hyprbars) for clients that accept xdg-decoration server-side
//! (Qt, kitty, Firefox); GTK keeps its own. The bar sits above the window's geometry, drawn on the CPU
//! (tiny-skia + SF Pro via mist-ui's text.rs) into a small buffer that is redrawn only when the title,
//! width, focus or button hover changes. Traffic lights on the left, symbols only while hovered.
use std::cell::RefCell;

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    desktop::{space::SpaceElement, Window},
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point, Rectangle, Transform},
};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform as Tf};

use crate::text::{Style, Text};
use crate::wm::data;
use crate::MistComp;

pub const BAR: i32 = 30;
const RADIUS: f32 = 10.0;
const PAD: f32 = 12.0;
const DIAM: f32 = 13.0;
const GAP: f32 = 8.0;
/// own title bars: Zen (macOS dots in its tab bar), Mist Files / Player (hyprbars:no_bar in windowrules.lua)
const NO_BAR: [&str; 5] = ["zen", "zen-browser", "zen-beta", "mist-files", "mist-player"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Close,
    Min,
    Max,
    Drag,
}

pub struct Bar {
    key: (String, bool, i32, bool),
    pub buf: MemoryRenderBuffer,
}

/// Per-window bar state (in `WinData`).
#[derive(Default)]
pub struct Deco {
    pub bar: RefCell<Option<Bar>>,
}

fn btn_cx(i: usize) -> f32 {
    PAD + DIAM / 2.0 + i as f32 * (DIAM + GAP)
}

/// What a point (relative to the bar's top left) hits.
pub fn hit(x: f64, y: f64) -> Hit {
    let cy = BAR as f64 / 2.0;
    for (i, h) in [Hit::Close, Hit::Min, Hit::Max].into_iter().enumerate() {
        let (dx, dy) = (x - btn_cx(i) as f64, y - cy);
        if dx * dx + dy * dy <= (DIAM as f64 / 2.0 + 2.0).powi(2) {
            return h;
        }
    }
    Hit::Drag
}

/// The three buttons' area (hover shows their symbols).
fn in_buttons(x: f64, y: f64) -> bool {
    x >= PAD as f64 - 4.0 && x <= (btn_cx(2) + DIAM / 2.0 + 4.0) as f64 && (0.0..BAR as f64).contains(&y)
}

impl MistComp {
    /// Height of the window's server-side bar (0: client-side decorations or fullscreen).
    pub fn bar_h(&self, w: &Window) -> i32 {
        if let Some(x) = w.x11_surface() {
            // X11: a bar unless the client draws its own (Motif hints) or is fullscreen / a menu
            let bar = !x.is_decorated() && !x.is_fullscreen() && !x.is_override_redirect() && !x.is_popup();
            return if bar && !NO_BAR.contains(&x.class().to_lowercase().as_str()) { BAR } else { 0 };
        }
        let Some(t) = w.toplevel() else { return 0 };
        let ssd = t.with_pending_state(|s| {
            s.decoration_mode == Some(smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ServerSide)
                && !s.states.contains(xdg_toplevel::State::Fullscreen)
        });
        // Zen draws macOS dots in its own tab bar (userChrome.css)
        if ssd && !NO_BAR.contains(&crate::hyprcompat::app_id(w).to_lowercase().as_str()) { BAR } else { 0 }
    }

    fn bar_rect(&self, w: &Window) -> Option<Rectangle<i32, Logical>> {
        let h = self.bar_h(w);
        if h == 0 {
            return None;
        }
        let g = self.space.element_geometry(w)?;
        Some(Rectangle::new((g.loc.x, g.loc.y - h).into(), (g.size.w, h).into()))
    }

    /// The bar under a point, unless a window above covers it.
    pub fn bar_at(&self, pos: Point<f64, Logical>) -> Option<(Window, Hit, Point<f64, Logical>)> {
        for w in self.space.elements().rev() {
            if let Some(r) = self.bar_rect(w) {
                if r.to_f64().contains(pos) {
                    let rel = pos - r.loc.to_f64();
                    return Some((w.clone(), hit(rel.x, rel.y), rel));
                }
            }
            let loc = self.space.element_location(w)? - w.geometry().loc;
            if w.is_in_input_region(&(pos - loc.to_f64())) {
                return None;
            }
        }
        None
    }

    /// Pointer moved: buttons hovered on some bar? (redraws that bar only when it changes)
    pub fn bar_hover(&mut self, pos: Point<f64, Logical>) {
        let at = self.bar_at(pos);
        // the green button: Snap Layouts flyout under it (mist-ui)
        let green = at.as_ref().filter(|(_, h, _)| *h == Hit::Max).map(|(w, _, rel)| {
            let origin = pos - *rel;
            (w.clone(), origin + Point::from((btn_cx(2) as f64, (BAR as f64 + DIAM as f64) / 2.0)))
        });
        self.max_hover(green);
        let h = at.filter(|(_, _, rel)| in_buttons(rel.x, rel.y)).map(|(w, _, _)| data(&w).addr.get());
        if h != self.bar_hovered {
            self.bar_hovered = h;
            self.dirty = true;
        }
    }

    /// Bring every bar up to date before a frame (no-op unless something about it changed).
    pub fn update_bars(&mut self) {
        let focused = self.focused_window();
        let wins: Vec<Window> = self.space.elements().cloned().collect();
        for w in wins {
            let d = data(&w);
            if self.bar_h(&w) == 0 {
                d.deco.bar.borrow_mut().take();
                continue;
            }
            let width = w.geometry().size.w.max(1);
            let key = (crate::hyprcompat::title(&w), focused.as_ref() == Some(&w), width, self.bar_hovered == Some(d.addr.get()));
            if d.deco.bar.borrow().as_ref().is_some_and(|b| b.key == key) {
                continue;
            }
            let text = self.text.get_or_insert_with(Text::new);
            let pm = draw(text, &key.0, key.1, width, key.3);
            let buf = MemoryRenderBuffer::from_slice(pm.data(), Fourcc::Abgr8888, (width, BAR), 1, Transform::Normal, None);
            *d.deco.bar.borrow_mut() = Some(Bar { key, buf });
        }
    }
}

fn hex(v: u32, a: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, (a * 255.0) as u8)
}

fn paint(c: tiny_skia::Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn draw(text: &mut Text, title: &str, active: bool, w: i32, hover: bool) -> Pixmap {
    let (wf, hf) = (w as f32, BAR as f32);
    let mut pm = Pixmap::new(w as u32, BAR as u32).unwrap();
    // rounded top corners, square bottom (the window continues below)
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, hf);
    pb.line_to(0.0, RADIUS);
    pb.quad_to(0.0, 0.0, RADIUS, 0.0);
    pb.line_to(wf - RADIUS, 0.0);
    pb.quad_to(wf, 0.0, wf, RADIUS);
    pb.line_to(wf, hf);
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(&p, &paint(hex(0x10151f, 1.0)), FillRule::Winding, Tf::identity(), None);
    }

    let cy = hf / 2.0;
    let lights = [(0xff5f57, 0x4d0000), (0xfebc2e, 0x5a3d00), (0x28c840, 0x003d00)];
    for (i, (bg, fg)) in lights.into_iter().enumerate() {
        let cx = btn_cx(i);
        let col = if active || hover { hex(bg, 1.0) } else { hex(0x4a4a52, 1.0) };
        if let Some(c) = PathBuilder::from_circle(cx, cy, DIAM / 2.0) {
            pm.fill_path(&c, &paint(col), FillRule::Winding, Tf::identity(), None);
        }
        if hover {
            let s = 3.0;
            let mut pb = PathBuilder::new();
            match i {
                0 => {
                    pb.move_to(cx - s, cy - s);
                    pb.line_to(cx + s, cy + s);
                    pb.move_to(cx + s, cy - s);
                    pb.line_to(cx - s, cy + s);
                }
                1 => {
                    pb.move_to(cx - s - 0.5, cy);
                    pb.line_to(cx + s + 0.5, cy);
                }
                _ => {
                    pb.move_to(cx - s - 0.5, cy);
                    pb.line_to(cx + s + 0.5, cy);
                    pb.move_to(cx, cy - s - 0.5);
                    pb.line_to(cx, cy + s + 0.5);
                }
            }
            if let Some(p) = pb.finish() {
                let st = Stroke { width: 1.4, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
                pm.stroke_path(&p, &paint(hex(fg, 1.0)), &st, Tf::identity(), None);
            }
        }
    }

    // centred title, cut with an ellipsis before it reaches the buttons
    let st = Style::ui(13.0, 510.0);
    let room = wf - 2.0 * (btn_cx(2) + DIAM / 2.0 + 12.0);
    if room > 20.0 && !title.is_empty() {
        let mut s = title.to_string();
        if text.width(&s, &st) > room {
            let mut chars: Vec<char> = title.chars().collect();
            while !chars.is_empty() && text.width(&format!("{}…", chars.iter().collect::<String>()), &st) > room {
                chars.pop();
            }
            s = format!("{}…", chars.into_iter().collect::<String>().trim_end());
        }
        let tw = text.width(&s, &st);
        let col = if active { (0.92, 0.92, 0.94, 0.88) } else { (0.92, 0.92, 0.94, 0.45) };
        draw_text(&mut pm, text, ((wf - tw) / 2.0).round(), cy, &s, &st, col);
    }
    pm
}

// mist-ui gfx::Canvas::text: line box centred on cy
fn draw_text(pm: &mut Pixmap, t: &mut Text, x: f32, cy: f32, s: &str, st: &Style, c: (f32, f32, f32, f32)) {
    let sh = t.shape(s, st);
    let base = (cy - (sh.ascent + sh.descent) / 2.0 + sh.ascent).round();
    for (k, &(id, gx)) in sh.glyphs.iter().enumerate() {
        let px = x + gx;
        let ix = px.floor();
        let sub = (((px - ix) * 4.0).round() as u8).min(3);
        let face = sh.faces.get(k).copied().unwrap_or(st.face);
        let g = t.glyph_in(face, st, id, sub);
        let (ox, oy) = (ix as i32 + g.left, base as i32 - g.top);
        if g.color {
            if let Some(src) = tiny_skia::PixmapRef::from_bytes(&g.data, g.w, g.h) {
                pm.draw_pixmap(ox, oy, src, &tiny_skia::PixmapPaint { opacity: c.3, ..Default::default() }, Tf::identity(), None);
            }
        } else {
            blend(pm, ox, oy, g.w, g.h, &g.data, c);
        }
    }
}

fn blend(pm: &mut Pixmap, ox: i32, oy: i32, w: u32, h: u32, mask: &[u8], c: (f32, f32, f32, f32)) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    for my in 0..h as i32 {
        let y = oy + my;
        if y < 0 || y >= ph {
            continue;
        }
        for mx in 0..w as i32 {
            let x = ox + mx;
            if x < 0 || x >= pw {
                continue;
            }
            let a = mask[(my * w as i32 + mx) as usize] as f32 / 255.0 * c.3;
            if a <= 0.0 {
                continue;
            }
            let i = ((y * pw + x) * 4) as usize;
            let inv = 1.0 - a;
            data[i] = (c.0 * 255.0 * a + data[i] as f32 * inv).round() as u8;
            data[i + 1] = (c.1 * 255.0 * a + data[i + 1] as f32 * inv).round() as u8;
            data[i + 2] = (c.2 * 255.0 * a + data[i + 2] as f32 * inv).round() as u8;
            data[i + 3] = (a * 255.0 + data[i + 3] as f32 * inv).round() as u8;
        }
    }
}

/// Resize by the frame (Hyprland resize_on_border + extend_border_grab_area): a band this wide around
/// every window's outside (bar included) grabs the pointer.
pub const EDGE: i32 = 12;

impl MistComp {
    /// The window frame under the pointer: (window, edges, its location). Client surfaces (popups, CSD
    /// shadows) and windows above win; not over fullscreen or override-redirect X11 windows.
    pub fn edge_at(&self, pos: Point<f64, Logical>) -> Option<(Window, crate::grabs::resize_grab::ResizeEdge, Point<i32, Logical>)> {
        use crate::grabs::resize_grab::ResizeEdge;
        if self.locked || self.layer_at(pos).is_some() {
            return None;
        }
        for w in self.space.elements().rev() {
            let Some(loc) = self.space.element_location(w) else { continue };
            if w.surface_under(pos - (loc - w.geometry().loc).to_f64(), smithay::desktop::WindowSurfaceType::ALL).is_some() {
                return None;
            }
            let bar = self.bar_h(w);
            let g = w.geometry();
            let outer = Rectangle::<i32, Logical>::new((loc.x, loc.y - bar).into(), (g.size.w, g.size.h + bar).into()).to_f64();
            if outer.contains(pos) {
                return None;
            }
            let mut band = outer;
            band.loc -= Point::from((EDGE as f64, EDGE as f64));
            band.size += smithay::utils::Size::from((2.0 * EDGE as f64, 2.0 * EDGE as f64));
            if !band.contains(pos) {
                continue;
            }
            let fullscreen = w.toplevel().is_some_and(|t| t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Fullscreen)));
            let x11_fixed = w.x11_surface().is_some_and(|x| x.is_fullscreen() || x.is_override_redirect() || crate::wm::data(w).addr.get() == 0);
            if fullscreen || x11_fixed {
                return None;
            }
            let mut e = ResizeEdge::empty();
            if pos.x < outer.loc.x {
                e |= ResizeEdge::LEFT;
            } else if pos.x >= outer.loc.x + outer.size.w {
                e |= ResizeEdge::RIGHT;
            }
            if pos.y < outer.loc.y {
                e |= ResizeEdge::TOP;
            } else if pos.y >= outer.loc.y + outer.size.h {
                e |= ResizeEdge::BOTTOM;
            }
            return Some((w.clone(), e, loc));
        }
        None
    }

    /// Pointer moved: the resize cursor over a frame, back to the default when it leaves.
    pub fn edge_hover(&mut self, pos: Point<f64, Logical>) {
        use crate::grabs::resize_grab::ResizeEdge;
        use smithay::input::pointer::{CursorIcon, CursorImageStatus};
        let icon = self.edge_at(pos).map(|(_, e, _)| match e {
            e if e == ResizeEdge::TOP | ResizeEdge::LEFT => CursorIcon::NwResize,
            e if e == ResizeEdge::TOP | ResizeEdge::RIGHT => CursorIcon::NeResize,
            e if e == ResizeEdge::BOTTOM | ResizeEdge::LEFT => CursorIcon::SwResize,
            e if e == ResizeEdge::BOTTOM | ResizeEdge::RIGHT => CursorIcon::SeResize,
            ResizeEdge::TOP => CursorIcon::NResize,
            ResizeEdge::BOTTOM => CursorIcon::SResize,
            ResizeEdge::LEFT => CursorIcon::WResize,
            _ => CursorIcon::EResize,
        });
        match icon {
            Some(i) => {
                self.cursor_status = CursorImageStatus::Named(i);
                self.edge_cursor = true;
                self.dirty = true;
            }
            None if self.edge_cursor => {
                self.edge_cursor = false;
                if self.seat.get_pointer().is_some_and(|p| p.current_focus().is_none()) {
                    self.cursor_status = CursorImageStatus::default_named();
                }
                self.dirty = true;
            }
            None => {}
        }
    }
}
