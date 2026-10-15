//! Windows 11 snapping, ported from Hyprland's windows-like.lua + the hyprbars mistsnap.cpp hooks; mist-ui
//! (snap.rs) draws the drag preview, Snap Layouts and Snap Assist from the same socket2 events as before:
//!   mistdrag>>begin,<addr>,x,y,w,h   a window starts moving (its geometry then = restore size after a snap)
//!   mistdrag>>end,x,y                the move ended at the cursor
//!   mistsnap>>snapped,<addr>,<mon>,fx,fy,fw,fh   placed into part of a screen (Snap Assist fills the rest)
//!   mistsnap>>maxhover,<addr>,x,y / maxleave,<addr>   the green button hovered (Snap Layouts flyout)
//!   mistsnap>>layouts,<addr>,x,y     Win+Z
//! Zones are fractions of the output's usable area with 8 px gaps; rectangles here are "outer" (window plus
//! its title bar) like the Lua `place()`.
use std::cell::Cell;

use smithay::{
    desktop::{layer_map_for_output, Window},
    output::Output,
    utils::{Logical, Point, Rectangle},
};

use crate::{wm::data, MistComp};

const GAP: i32 = 8;

type Rect = Rectangle<i32, Logical>;

#[derive(Clone, Copy, Debug)]
pub struct Snapped {
    /// outer rectangle to go back to
    restore: Rect,
    /// window geometry right after snapping (moved / resized by hand = no longer snapped)
    at: Rect,
    f: [f64; 4],
    max: bool,
}

/// Per-window snap state (in `WinData`).
#[derive(Default)]
pub struct SnapData {
    snapped: Cell<Option<Snapped>>,
    /// window size to take back when grabbed again (drag-restore)
    grab_size: Cell<Option<(i32, i32)>>,
}

impl SnapData {
    pub fn is_snapped(&self) -> bool {
        let s = self.snapped.take();
        self.snapped.set(s);
        s.is_some()
    }
}

fn near(a: i32, b: i32) -> bool {
    (a - b).abs() <= 3
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Stretch,
    Unsnap,
    Layouts,
}

impl MistComp {
    fn workarea(&self, o: &Output) -> Rect {
        let og = self.space.output_geometry(o).unwrap_or_default();
        let z = layer_map_for_output(o).non_exclusive_zone();
        Rectangle::new(og.loc + z.loc + Point::from((GAP, GAP)), (z.size.w - 2 * GAP, z.size.h - 2 * GAP).into())
    }

    /// The window with its title bar.
    pub fn outer(&self, w: &Window) -> Rect {
        let bar = self.bar_h(w);
        let g = self.space.element_geometry(w).unwrap_or_default();
        Rectangle::new((g.loc.x, g.loc.y - bar).into(), (g.size.w, g.size.h + bar).into())
    }

    /// Resize + move a window so that it (with its bar) fills `r`; returns the window geometry.
    fn place(&mut self, w: &Window, r: Rect) -> Rect {
        let bar = self.bar_h(w);
        let a = Rectangle::new((r.loc.x, r.loc.y + bar).into(), (r.size.w.max(1), (r.size.h - bar).max(1)).into());
        self.resize_to(w, a.size.w, a.size.h);
        if let Some(old) = self.space.element_location(w) {
            self.anim.moved(data(w).addr.get(), old - a.loc);
        }
        self.space.map_element(w.clone(), a.loc, true);
        a
    }

    pub fn resize_to(&mut self, w: &Window, cw: i32, ch: i32) {
        if let Some(t) = w.toplevel() {
            t.with_pending_state(|s| s.size = Some((cw, ch).into()));
            t.send_pending_configure();
        } else if let Some(x) = w.x11_surface() {
            let loc = self.space.element_location(w).unwrap_or_default();
            let _ = x.configure(Rectangle::new(loc, (cw, ch).into()));
        }
    }

    fn snap_state(&self, w: &Window) -> Option<Snapped> {
        let d = &data(w).snap;
        let s = d.snapped.get()?;
        let g = self.space.element_geometry(w)?;
        // geometry not yet committed after the configure counts as still snapped
        let pending = w.toplevel().and_then(|t| t.with_pending_state(|p| p.size)).is_some_and(|sz| sz == s.at.size);
        if near(g.loc.x, s.at.loc.x) && near(g.loc.y, s.at.loc.y) && ((near(g.size.w, s.at.size.w) && near(g.size.h, s.at.size.h)) || pending) {
            return Some(s);
        }
        d.snapped.set(None);
        None
    }

    /// The restore rectangle always on the screen; nothing sensible (as big as the area) = 62% centred.
    fn restore_rect(&self, r: Rect, o: &Output) -> Rect {
        let a = self.workarea(o);
        if r.size.w >= a.size.w - 3 && r.size.h >= a.size.h - 3 {
            let (w, h) = (a.size.w as f64, a.size.h as f64);
            return Rectangle::new(
                (a.loc.x + (w * 0.19) as i32, a.loc.y + (h * 0.14) as i32).into(),
                ((w * 0.62) as i32, (h * 0.72) as i32).into(),
            );
        }
        let (w, h) = (r.size.w.min(a.size.w), r.size.h.min(a.size.h));
        let x = r.loc.x.min(a.loc.x + a.size.w - w).max(a.loc.x);
        let y = r.loc.y.min(a.loc.y + a.size.h - h).max(a.loc.y);
        Rectangle::new((x, y).into(), (w, h).into())
    }

    /// Fullscreen / xdg-maximized windows leave that state first (Lua `prepare()`).
    fn prepare(&mut self, w: &Window) {
        if data(w).restore.borrow().is_some() {
            let full = w.toplevel().is_some_and(|t| t.with_pending_state(|s| s.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen)))
                || w.x11_surface().is_some_and(|x| x.is_fullscreen());
            self.toggle_max(w, full);
        }
    }

    /// Place a window into zone `f` of output `o`, remember `restore`.
    fn snap_into(&mut self, w: &Window, restore: Rect, f: [f64; 4], o: &Output) {
        let a = self.workarea(o);
        let (cw, ch) = ((a.size.w + GAP) as f64, (a.size.h + GAP) as f64);
        let r = Rectangle::new(
            ((a.loc.x as f64 + f[0] * cw).floor() as i32, (a.loc.y as f64 + f[1] * ch).floor() as i32).into(),
            ((f[2] * cw - GAP as f64).floor() as i32, (f[3] * ch - GAP as f64).floor() as i32).into(),
        );
        let at = self.place(w, r);
        let max = f[2] == 1.0 && f[3] == 1.0;
        let d = &data(w).snap;
        d.snapped.set(Some(Snapped { restore, at, f, max }));
        let bar = self.bar_h(w);
        d.grab_size.set(Some((restore.size.w, restore.size.h - bar)));
        self.space.raise_element(w, true);
        if !max {
            let addr = crate::hyprcompat::addr(w);
            self.hypr_event("mistsnap", &format!("snapped,{addr},{},{},{},{},{}", o.name(), f[0], f[1], f[2], f[3]));
        }
        self.dirty = true;
    }

    fn unsnap(&mut self, w: &Window) -> bool {
        let Some(s) = self.snap_state(w) else { return false };
        let d = &data(w).snap;
        d.snapped.set(None);
        d.grab_size.set(None);
        let o = self.output_of(w).or_else(|| self.active_output());
        let r = match o {
            Some(o) => self.restore_rect(s.restore, &o),
            None => s.restore,
        };
        self.place(w, r);
        true
    }

    /// The green button / double click / Win+Up: maximize into the work area (with gaps), or back.
    pub fn mist_maximize(&mut self, w: &Window) {
        if w.x11_surface().is_some_and(|x| x.is_fullscreen()) {
            return;
        }
        let s = self.snap_state(w);
        if s.is_some_and(|s| s.max) {
            self.unsnap(w);
            return;
        }
        self.prepare(w);
        let Some(o) = self.output_of(w).or_else(|| self.active_output()) else { return };
        let r = self.restore_rect(s.map(|s| s.restore).unwrap_or_else(|| self.outer(w)), &o);
        self.snap_into(w, r, [0.0, 0.0, 1.0, 1.0], &o);
    }

    fn neighbour_output(&self, o: &Output, left: bool) -> Option<Output> {
        let g = self.space.output_geometry(o)?;
        self.space
            .outputs()
            .find(|x| {
                let Some(xg) = self.space.output_geometry(x) else { return false };
                *x != o && xg.loc.y < g.loc.y + g.size.h && xg.loc.y + xg.size.h > g.loc.y
                    && if left { xg.loc.x + xg.size.w == g.loc.x } else { xg.loc.x == g.loc.x + g.size.w }
            })
            .cloned()
    }

    fn snap_active(&mut self, w: &Window, f: [f64; 4], o: Option<Output>) {
        let s = self.snap_state(w);
        self.prepare(w);
        let Some(o) = o.or_else(|| self.output_of(w)).or_else(|| self.active_output()) else { return };
        let r = self.restore_rect(s.map(|s| s.restore).unwrap_or_else(|| self.outer(w)), &o);
        self.snap_into(w, r, f, &o);
    }

    /// Win+arrows 1:1 like Windows 11 (windows-like.lua).
    pub fn snap_key(&mut self, k: Key) {
        if let Some(w) = self.focused_window() {
            self.snap_key_on(&w, k);
        }
    }

    pub fn snap_key_on(&mut self, w: &Window, k: Key) {
        let w = w.clone();
        let fullscreen = w.x11_surface().is_some_and(|x| x.is_fullscreen())
            || w.toplevel().is_some_and(|t| t.with_pending_state(|s| s.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen)));
        let s = self.snap_state(&w);
        match k {
            Key::Left | Key::Right => {
                if fullscreen {
                    return;
                }
                let left = matches!(k, Key::Left);
                if let Some(s) = s.filter(|s| !s.max) {
                    let f = s.f;
                    let is_left = f[0] == 0.0 && f[2] < 1.0;
                    let is_right = f[0] + f[2] >= 0.999 && f[0] > 0.0;
                    if (left && is_right) || (!left && is_left) {
                        self.unsnap(&w);
                        return;
                    }
                    if (left && is_left) || (!left && is_right) {
                        // same side again: the opposite half of the next screen
                        let o = self.output_of(&w).and_then(|o| self.neighbour_output(&o, left));
                        if let Some(o) = o {
                            self.snap_active(&w, [if left { 1.0 - f[2] } else { 0.0 }, f[1], f[2], f[3]], Some(o));
                        }
                        return;
                    }
                }
                self.snap_active(&w, [if left { 0.0 } else { 0.5 }, 0.0, 0.5, 1.0], None);
            }
            Key::Up => {
                if fullscreen {
                    return;
                }
                match s {
                    Some(s) if !s.max && !(s.f[3] < 1.0 && s.f[1] == 0.0) && s.f[2] != 1.0 => {
                        let f = s.f;
                        // half -> top quarter, bottom quarter -> half
                        let nf = if f[3] == 1.0 { [f[0], 0.0, f[2], 0.5] } else { [f[0], 0.0, f[2], 1.0] };
                        self.snap_active(&w, nf, None);
                    }
                    Some(s) if s.max => {}
                    _ => self.mist_maximize(&w),
                }
            }
            Key::Down => {
                if fullscreen {
                    let full = true;
                    self.toggle_max(&w, full);
                    return;
                }
                match s {
                    Some(s) if s.max || s.f[2] == 1.0 => {
                        self.unsnap(&w);
                    }
                    Some(s) if s.f[1] == 0.0 => {
                        let f = s.f;
                        // half -> bottom quarter, top quarter -> half
                        let nf = if f[3] == 1.0 { [f[0], 0.5, f[2], 0.5] } else { [f[0], 0.0, f[2], 1.0] };
                        self.snap_active(&w, nf, None);
                    }
                    _ => self.minimize(&w),
                }
            }
            Key::Stretch => {
                if fullscreen || s.is_some() {
                    return;
                }
                self.prepare(&w);
                let Some(o) = self.output_of(&w) else { return };
                let a = self.workarea(&o);
                let r = self.restore_rect(self.outer(&w), &o);
                let at = self.place(&w, Rectangle::new((r.loc.x, a.loc.y).into(), (r.size.w, a.size.h).into()));
                let f = [(r.loc.x - a.loc.x) as f64 / (a.size.w + GAP) as f64, 0.0, 1.0, 1.0];
                let d = &data(&w).snap;
                d.snapped.set(Some(Snapped { restore: r, at, f, max: false }));
                let bar = self.bar_h(&w);
                d.grab_size.set(Some((r.size.w, r.size.h - bar)));
            }
            Key::Unsnap => {
                self.unsnap(&w);
            }
            Key::Layouts => {
                if fullscreen {
                    return;
                }
                let g = self.space.element_geometry(&w).unwrap_or_default();
                let addr = crate::hyprcompat::addr(&w);
                self.hypr_event("mistsnap", &format!("layouts,{addr},{},{}", g.loc.x + 70, g.loc.y + 30));
            }
        }
    }

    /// `MistSnapApply(addr, fx, fy, fw, fh [, rx, ry, rw, rh])` from mist-ui: zone on the output under the
    /// cursor; r = the window (without bar) when it was grabbed.
    pub fn snap_apply(&mut self, w: &Window, f: [f64; 4], grab: Option<[f64; 4]>) {
        self.focus_window(Some(w));
        self.prepare(w);
        let p = self.seat.get_pointer().unwrap().current_location();
        let o = self
            .space
            .outputs()
            .find(|o| self.space.output_geometry(o).is_some_and(|g| g.to_f64().contains(p)))
            .cloned()
            .or_else(|| self.output_of(w));
        let Some(o) = o else { return };
        let bar = self.bar_h(w);
        let r = match grab {
            Some(g) => Rectangle::new((g[0] as i32, g[1] as i32 - bar).into(), (g[2] as i32, g[3] as i32 + bar).into()),
            None => self.snap_state(w).map(|s| s.restore).unwrap_or_else(|| self.outer(w)),
        };
        let r = self.restore_rect(r, &o);
        self.snap_into(w, r, f, &o);
    }

    /// Any move of a window (title bar, Super+drag, client CSD drag): a snapped window grabbed again takes back
    /// its size under the cursor; mist-ui learns about the drag (begin here, end when the grab ends).
    pub fn start_move(&mut self, w: Window, button: u32, serial: smithay::utils::Serial) {
        let pointer = self.seat.get_pointer().unwrap();
        let m = pointer.current_location();
        let Some(mut loc) = self.space.element_location(&w) else { return };
        let mut size = w.geometry().size;
        if let Some((rw, rh)) = data(&w).snap.grab_size.take() {
            if size.w > 0 && ((size.w - rw).abs() > 2 || (size.h - rh).abs() > 2) {
                // the cursor keeps its place along the title bar, proportionally to the width
                let fx = ((m.x - loc.x as f64) / size.w as f64).clamp(0.0, 1.0);
                loc.x = (m.x - fx * rw as f64).round() as i32;
                size = (rw, rh).into();
                self.resize_to(&w, rw, rh);
                self.space.map_element(w.clone(), loc, true);
            }
        }
        data(&w).snap.snapped.set(None);
        let addr = crate::hyprcompat::addr(&w);
        self.hypr_event("mistdrag", &format!("begin,{addr},{},{},{},{}", loc.x, loc.y, size.w, size.h));
        let start_data = smithay::input::pointer::GrabStartData { focus: None, button, location: m };
        let grab = crate::grabs::MoveSurfaceGrab { start_data, window: w, initial_window_location: loc, last: m };
        pointer.set_grab(self, grab, serial, smithay::input::pointer::Focus::Clear);
    }

    /// Win+Shift+Left/Right: the window to the previous / next output (left to right), same relative place.
    pub fn move_to_monitor(&mut self, w: &Window, d: i32) {
        let mut outs: Vec<(Output, Rect)> = self.space.outputs().filter_map(|o| Some((o.clone(), self.space.output_geometry(o)?))).collect();
        outs.sort_by_key(|(_, g)| (g.loc.x, g.loc.y));
        let Some(cur) = self.output_of(w) else { return };
        let Some(i) = outs.iter().position(|(o, _)| *o == cur) else { return };
        let n = outs.len() as i32;
        let (to, tg) = outs[((i as i32 + d).rem_euclid(n)) as usize].clone();
        if to == cur {
            return;
        }
        let ws = self.active_ws(&to);
        data(w).ws.set(ws);
        if let Some(s) = self.snap_state(w) {
            let r = self.restore_rect(s.restore, &to);
            self.snap_into(w, r, s.f, &to);
        } else {
            let og = outs[i].1;
            let o = self.outer(w);
            let r = Rectangle::new(o.loc - og.loc + tg.loc, o.size);
            let r = self.restore_rect(r, &to);
            self.place(w, r);
        }
        self.focus_window(Some(w));
    }

    /// Green button hover (Snap Layouts flyout under it).
    pub fn max_hover(&mut self, hit: Option<(Window, Point<f64, Logical>)>) {
        let now = hit.as_ref().map(|(w, _)| data(w).addr.get());
        if now == self.max_hovered {
            return;
        }
        if let Some(a) = self.max_hovered.take() {
            self.hypr_event("mistsnap", &format!("maxleave,0x{a:x}"));
        }
        if let Some((w, at)) = hit {
            self.max_hovered = now;
            let addr = crate::hyprcompat::addr(&w);
            self.hypr_event("mistsnap", &format!("maxhover,{addr},{},{}", at.x as i32, at.y as i32));
        }
    }
}
