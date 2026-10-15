//! Motion, Hyprland's animations.lua in spirit: a new window fades in rising 24 px (200 ms), switching a
//! workspace slides the old windows out and the new ones in (300 ms), ease-out like the "quick" curve.
//! Nothing runs while idle: the loop keeps `dirty` only while something is animating (one frame per
//! vblank / tick), the scene just offsets locations and alpha (no extra passes). MIST_COMP_ANIM=0 = off.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
                texture::{TextureBuffer, TextureRenderElement},
                Kind,
            },
            gles::{GlesRenderer, GlesTexture},
            Bind, Offscreen,
        },
    },
    desktop::Window,
    utils::{Logical, Point, Rectangle, Scale, Transform},
};

use crate::MistComp;

const OPEN: Duration = Duration::from_millis(200);
const OPEN_RISE: f64 = 24.0;
const WS: Duration = Duration::from_millis(300);
const MOVE: Duration = Duration::from_millis(250);
const CLOSE: Duration = Duration::from_millis(180);
const MINIMIZE: Duration = Duration::from_millis(250);
const LAYER: Duration = Duration::from_millis(200);

/// Layer namespaces without the open fade (windowrules.lua `no_anim`): always-there surfaces and the ones
/// that animate themselves.
pub fn layer_fades(namespace: &str) -> bool {
    !matches!(
        namespace,
        "mist-desktop" | "mist-bar" | "mist-island" | "mist-snap" | "mist-snap-assist" | "mist-snap-catcher" | "mist-gallery" | "mist-wallpreview" | "mist-spotlight" | "mist-switcher"
    )
}

/// A closed / minimized window's last picture (bar included) shrinking and fading out.
pub struct Closing {
    tex: TextureBuffer<GlesTexture>,
    /// global rect of the picture
    rect: Rectangle<i32, Logical>,
    /// where it shrinks to (global; the window centre for close, the dock for minimize)
    to: Point<f64, Logical>,
    end_scale: f64,
    d: Duration,
    start: Instant,
}

pub struct WsSlide {
    /// the hidden workspace's windows and where they were (global)
    pub leaving: Vec<(Window, Point<i32, Logical>)>,
    /// +1: the new workspace comes in from the right
    pub dir: i32,
    pub start: Instant,
}

#[derive(Default)]
pub struct Anims {
    pub open: HashMap<u64, Instant>,
    /// snapped / maximized: (offset from the new place to where it was, start)
    pub moved: HashMap<u64, (Point<i32, Logical>, Instant)>,
    pub ws: HashMap<String, WsSlide>,
    pub closing: Vec<Closing>,
    /// layer surfaces fading in (by wl_surface id)
    pub layers: HashMap<smithay::reexports::wayland_server::backend::ObjectId, Instant>,
}

fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(4)
}

fn progress(start: Instant, d: Duration, now: Instant) -> f64 {
    ease(now.saturating_duration_since(start).as_secs_f64() / d.as_secs_f64())
}

pub fn enabled() -> bool {
    thread_local!(static ON: bool = std::env::var("MIST_COMP_ANIM").map_or(true, |v| v != "0"));
    ON.with(|o| *o)
}

impl Anims {
    pub fn opened(&mut self, addr: u64) {
        if enabled() {
            self.open.insert(addr, Instant::now());
        }
    }

    pub fn moved(&mut self, addr: u64, from: Point<i32, Logical>) {
        if enabled() && from != Point::from((0, 0)) {
            // a move while one runs starts from where the window shows now
            let now = Instant::now();
            let cur = self.moved.get(&addr).map(|(f, s)| f.to_f64().upscale(1.0 - progress(*s, MOVE, now)).to_i32_round()).unwrap_or_default();
            self.moved.insert(addr, (from + cur, now));
        }
    }

    pub fn layer_opened(&mut self, id: smithay::reexports::wayland_server::backend::ObjectId) {
        if enabled() {
            self.layers.insert(id, Instant::now());
        }
    }

    pub fn layer_alpha(&self, id: &smithay::reexports::wayland_server::backend::ObjectId, now: Instant) -> f32 {
        self.layers.get(id).map_or(1.0, |s| progress(*s, LAYER, now) as f32)
    }

    pub fn ws_switch(&mut self, output: String, leaving: Vec<(Window, Point<i32, Logical>)>, dir: i32) {
        if enabled() {
            self.ws.insert(output, WsSlide { leaving, dir, start: Instant::now() });
        }
    }

    /// Drop what has finished; true while anything still moves.
    pub fn active(&mut self) -> bool {
        let now = Instant::now();
        self.open.retain(|_, s| now.saturating_duration_since(*s) < OPEN);
        self.moved.retain(|_, (_, s)| now.saturating_duration_since(*s) < MOVE);
        self.ws.retain(|_, s| now.saturating_duration_since(s.start) < WS);
        self.closing.retain(|c| now.saturating_duration_since(c.start) < c.d);
        self.layers.retain(|_, s| now.saturating_duration_since(*s) < LAYER);
        !self.layers.is_empty() || !self.open.is_empty() || !self.moved.is_empty() || !self.ws.is_empty() || !self.closing.is_empty()
    }

    /// A window's (offset, alpha) right now.
    pub fn window(&self, addr: u64, output: &str, width: i32, now: Instant) -> (Point<i32, Logical>, f32) {
        let mut off = Point::<i32, Logical>::from((0, 0));
        let mut alpha = 1.0;
        if let Some(s) = self.open.get(&addr) {
            let p = progress(*s, OPEN, now);
            off.y += ((1.0 - p) * OPEN_RISE).round() as i32;
            alpha = p as f32;
        }
        if let Some((from, s)) = self.moved.get(&addr) {
            off += from.to_f64().upscale(1.0 - progress(*s, MOVE, now)).to_i32_round();
        }
        if let Some(s) = self.ws.get(output) {
            let p = progress(s.start, WS, now);
            off.x += (s.dir as f64 * (1.0 - p) * width as f64).round() as i32;
        }
        (off, alpha)
    }

    /// The leaving windows of a workspace slide on `output`: (window, location now).
    pub fn leaving(&self, output: &str, width: i32, now: Instant) -> Vec<(Window, Point<i32, Logical>)> {
        let Some(s) = self.ws.get(output) else { return Vec::new() };
        let p = progress(s.start, WS, now);
        let dx = (-(s.dir as f64) * p * width as f64).round() as i32;
        s.leaving.iter().map(|(w, l)| (w.clone(), *l + Point::from((dx, 0)))).collect()
    }
}

impl Anims {
    /// The closing pictures over this output, at their current size and alpha.
    pub fn closing(&self, og: Rectangle<i32, Logical>, scale: Scale<f64>, now: Instant) -> Vec<TextureRenderElement<GlesTexture>> {
        let mut out = Vec::new();
        for c in self.closing.iter().rev() {
            let p = progress(c.start, c.d, now);
            let k = 1.0 + (c.end_scale - 1.0) * p;
            let from = c.rect.to_f64();
            let centre = from.loc + from.size.to_point().downscale(2.0);
            let centre = centre + (c.to - centre).upscale(p);
            let size = from.size.upscale(k);
            let loc = centre - size.to_point().downscale(2.0);
            let rect = Rectangle::new(loc, size).to_i32_round();
            if !rect.overlaps(og) {
                continue;
            }
            let at = (loc - og.loc.to_f64()).to_physical(scale);
            out.push(TextureRenderElement::from_texture_buffer(at, &c.tex, Some((1.0 - p) as f32), None, Some(rect.size), Kind::Unspecified));
        }
        out
    }
}

enum Out {
    Close,
    Minimize,
    SlideDown,
}

impl MistComp {
    /// Picture a window (bar included) before it goes; closing / minimize animations draw that.
    pub fn animate_out(&mut self, w: &Window, minimize: bool) {
        self.snapshot_out(w, if minimize { Out::Minimize } else { Out::Close });
    }

    /// The special workspace hides: its windows slide down off the output (specialWorkspaceOut slide bottom).
    pub fn animate_slide_down(&mut self, w: &Window) {
        self.snapshot_out(w, Out::SlideDown);
    }

    fn snapshot_out(&mut self, w: &Window, how: Out) {
        if !enabled() {
            return;
        }
        let Some(loc) = self.space.element_location(w) else { return };
        let Some(o) = self.output_of(w) else { return };
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let scale = Scale::from(o.current_scale().fractional_scale());
        let bar = if crate::wm::data(w).deco.bar.borrow().is_some() { crate::decor::BAR } else { 0 };
        let g = w.geometry();
        let rect = Rectangle::<i32, Logical>::new((loc.x, loc.y - bar).into(), (g.size.w, g.size.h + bar).into());
        if rect.size.w < 2 || rect.size.h < 2 {
            return;
        }
        let snap = |r: &mut GlesRenderer| -> Option<TextureBuffer<GlesTexture>> {
            let size = rect.size.to_physical_precise_round(scale);
            let mut tex: GlesTexture = r.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into()).ok()?;
            let elements = crate::scene::window_elements(r, w, (0, bar).into(), scale, 1.0);
            {
                let mut fb = r.bind(&mut tex).ok()?;
                let mut tracker = OutputDamageTracker::new(size, scale, Transform::Normal);
                tracker.render_output(r, &mut fb, 0, &elements, [0.0, 0.0, 0.0, 0.0]).ok()?;
            }
            Some(TextureBuffer::from_texture(r, tex, scale.x.round() as i32, Transform::Normal, None))
        };
        let MistComp { headless, udev, gles, .. } = self;
        let tex = if let Some(h) = headless.as_mut() {
            snap(&mut h.renderer)
        } else if let Some(u) = udev.as_mut() {
            snap(&mut u.renderer)
        } else if let Some(g) = gles.as_ref() {
            snap(g.borrow_mut().renderer())
        } else {
            None
        };
        let Some(tex) = tex else { return };
        let centre = rect.to_f64().loc + rect.to_f64().size.to_point().downscale(2.0);
        let (to, end_scale, d) = match how {
            // towards the dock: bottom centre of the output
            Out::Minimize => (Point::from((og.loc.x as f64 + og.size.w as f64 / 2.0, (og.loc.y + og.size.h) as f64 - 40.0)), 0.15, MINIMIZE),
            Out::Close => (centre, 0.94, CLOSE),
            Out::SlideDown => (centre + Point::from((0.0, og.size.h as f64)), 1.0, MOVE),
        };
        self.anim.closing.push(Closing { tex, rect, to, end_scale, d, start: Instant::now() });
        self.dirty = true;
    }
}
