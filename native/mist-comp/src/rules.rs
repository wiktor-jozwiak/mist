//! Window rules (windows-like.lua / windowrules.lua): every window floats centred in the usable area at the
//! size the client picks, the main apps open at 62 % x 72 % of the monitor, Picture-in-Picture is pinned
//! to every workspace at a quarter of the monitor in the bottom-right corner, games (steam_app*, gamescope)
//! go fullscreen without a bar, launchers / .exe / discord open on the primary monitor (DP-3), calculators
//! and ark get their windowrules.lua sizes, other apps reopen at their last size (persistent_size), nameless XWayland helpers (drag icons) never take focus.
//! xdg toplevels are held back until their first buffer (app id, title and size are known by then) and
//! only then join the Space; the initial configure carries the rule's size.
use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    desktop::{layer_map_for_output, Window},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use smithay::output::Output;

use crate::MistComp;

const MAIN_APPS: [&str; 8] = ["kitty", "alacritty", "org.kde.dolphin", "io.elementary.files", "zen", "com.anthropic.claude", "org.gnome.texteditor", "btop"];
const GAP: i32 = 8;

const PRIMARY: &str = "DP-3";

/// Size from windowrules.lua: a fraction of (max, min) of the monitor's sides, or of (w, h).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    MaxMin(f64, f64),
    Wh(f64, f64),
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct Rule {
    pub size: Option<Size>,
    pub pip: bool,
    pub game: bool,
    pub primary: bool,
    pub no_focus: bool,
}

/// The rule for a new window by (lowercased) app id / X11 class and title.
pub fn rule(app: &str, title: &str, x11: bool) -> Rule {
    let mut r = Rule::default();
    if is_pip(title) {
        r.pip = true;
        r.size = Some(Size::MaxMin(0.25, 0.25));
    } else if MAIN_APPS.contains(&app) {
        r.size = Some(Size::Wh(0.62, 0.72));
    } else if app.contains("calc") {
        r.size = Some(Size::MaxMin(0.17, 0.43));
    } else if app == "org.kde.ark" {
        r.size = Some(Size::MaxMin(0.40, 0.40));
    }
    // a game's real window has a title; the nameless one is its launcher splash
    r.game = (app.starts_with("steam_app") || app == "gamescope") && !title.is_empty();
    r.primary = r.game || app.ends_with(".exe") || app.contains("launcher") || app == "vesktop" || app == "discord";
    r.no_focus = x11 && app.is_empty() && title.is_empty();
    r
}

impl Size {
    pub fn on(self, og: Rectangle<i32, Logical>) -> (i32, i32) {
        let (w, h) = (og.size.w as f64, og.size.h as f64);
        match self {
            Size::MaxMin(a, b) => ((w.max(h) * a) as i32, (w.min(h) * b) as i32),
            Size::Wh(a, b) => ((w * a) as i32, (h * b) as i32),
        }
    }
}

pub fn is_pip(title: &str) -> bool {
    let t = title.to_lowercase();
    t.starts_with("picture-in-picture") || t.starts_with("picture in picture") || t.starts_with("pictureinpicture")
}

/// Surface data: a rule size held for a moment after the window shows. Zen / Firefox draw their first frame
/// at the configured size, then resize themselves to the size they remember; each such resize is answered
/// with the rule size again (a few times, so a client that insists is left alone).
struct SizeGuard {
    until: std::time::Instant,
    want: (i32, i32),
    left: std::cell::Cell<u8>,
    /// the size it last had, and the centre it was placed around: a new size keeps that centre
    last: std::cell::Cell<(i32, i32)>,
    centre: Point<i32, Logical>,
}

impl MistComp {
    /// A held-back toplevel committed: send its initial configure, map it once it has a buffer.
    /// True when the surface was one of them.
    pub fn pending_commit(&mut self, surface: &WlSurface) -> bool {
        let Some(i) = self.pending.iter().position(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface)) else { return false };
        let w = self.pending[i].clone();
        let t = w.toplevel().unwrap().clone();
        let sent = with_states(surface, |s| s.data_map.get::<XdgToplevelSurfaceData>().map(|d| d.lock().unwrap().initial_configure_sent).unwrap_or(true));
        if !sent {
            let r = self.rule_of(&w);
            if let Some((sw, sh)) = self.restore_size(&w).or_else(|| self.initial_size(&w, &r)) {
                t.with_pending_state(|s| s.size = Some((sw, sh).into()));
            }
            t.send_configure();
            return true;
        }
        if !with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            return true;
        }
        w.on_commit();
        let r = self.rule_of(&w);
        self.pending.remove(i);
        let slot = self.restore_take(&w);
        let loc = match &slot {
            Some(s) => s.rect.loc,
            None => self.place_new(&w, &r),
        };
        if let Some(want) = r.size.filter(|_| slot.is_none()).filter(|_| !r.pip && !r.game).and_then(|_| self.initial_size(&w, &r)) {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let shown = w.geometry().size;
            let centre = loc + Point::from((shown.w / 2, shown.h / 2));
            let last = std::cell::Cell::new((shown.w, shown.h));
            with_states(surface, |s| s.data_map.insert_if_missing(|| SizeGuard { until, want, left: std::cell::Cell::new(3), last, centre }));
        }
        let game = r.game;
        self.adopt_ruled(w.clone(), loc, r);
        if let Some(s) = slot {
            if s.ws != crate::wm::data(&w).ws.get() {
                self.move_to_ws(&w, s.ws);
            }
            if s.max {
                self.mist_maximize(&w);
            }
        }
        // maximized / fullscreen asked for before the first frame (mpv --fs, a browser that closed maximized)
        match with_states(surface, |s| s.data_map.get::<MapWish>().and_then(|m| m.0.get())) {
            Some(Wish::Max) => self.mist_maximize(&w),
            Some(Wish::Full) if !game => self.toggle_max(&w, true),
            _ => {}
        }
        true
    }

    /// A shown toplevel committed: within its SizeGuard, a size of its own choosing gets the rule size back.
    pub fn guard_size(&mut self, surface: &WlSurface) {
        let Some(w) = self.space.elements().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface)).cloned() else { return };
        // maximized / snapped by Mist: the snap owns its size
        if self.snap_state(&w).is_some() {
            return;
        }
        let t = w.toplevel().unwrap();
        let got = w.geometry().size;
        let now = std::time::Instant::now();
        let (centre, want) = with_states(surface, |s| {
            let Some(g) = s.data_map.get::<SizeGuard>().filter(|g| now < g.until) else { return (None, None) };
            let resized = g.last.replace((got.w, got.h)) != (got.w, got.h);
            let want = (g.left.get() > 0 && (got.w, got.h) != g.want).then(|| {
                g.left.set(g.left.get() - 1);
                g.want
            });
            (resized.then_some(g.centre), want)
        });
        // resized by itself right after showing: kept around the centre it was placed at
        if let Some(c) = centre.filter(|_| !self.seat.get_pointer().is_some_and(|p| p.is_grabbed())) {
            let loc = c - Point::from((got.w / 2, got.h / 2));
            let r = self.rule_of(&w);
            let area = self.rule_output(&r).map(|o| {
                let og = self.space.output_geometry(&o).unwrap_or_default();
                let z = layer_map_for_output(&o).non_exclusive_zone();
                Rectangle::<i32, Logical>::new(og.loc + z.loc, z.size)
            });
            let loc = match area {
                Some(a) => Point::from((loc.x.clamp(a.loc.x, (a.loc.x + a.size.w - got.w).max(a.loc.x)), loc.y.clamp(a.loc.y, (a.loc.y + a.size.h - got.h).max(a.loc.y)))),
                None => loc,
            };
            self.space.map_element(w.clone(), loc, false);
        }
        let Some(want) = want else { return };
        tracing::debug!("size guard: {}x{} -> {}x{}", got.w, got.h, want.0, want.1);
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
        let free = t.with_pending_state(|s| !s.states.contains(State::Maximized) && !s.states.contains(State::Fullscreen) && !s.states.contains(State::TiledLeft) && !s.states.contains(State::TiledRight));
        if free {
            t.with_pending_state(|s| s.size = Some(want.into()));
            // the same size as the last configure: send_pending_configure would skip it
            t.send_configure();
        }
    }

    /// A toplevel not on screen yet asks to be maximized / fullscreen (None: back to normal): done at map.
    pub fn wish_at_map(&mut self, surface: &WlSurface, wish: Option<Wish>) -> bool {
        if !self.pending.iter().any(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface)) {
            return false;
        }
        with_states(surface, |s| s.data_map.get_or_insert(|| MapWish(std::cell::Cell::new(None))).0.set(wish));
        true
    }

    /// The mapped window an xdg toplevel says it belongs to (dialogs, file pickers).
    pub fn parent_of(&self, w: &Window) -> Option<Window> {
        let parent = w.toplevel()?.parent()?;
        self.space.elements().find(|p| p.toplevel().is_some_and(|t| *t.wl_surface() == parent)).cloned()
    }

    /// Windows on `o` whose bar would sit above the usable area move down to its top.
    pub fn keep_below_zone(&mut self, o: &smithay::output::Output) {
        let Some(og) = self.space.output_geometry(o) else { return };
        let top = og.loc.y + layer_map_for_output(o).non_exclusive_zone().loc.y;
        let wins: Vec<Window> = self.space.elements().cloned().collect();
        for w in wins {
            let Some(loc) = self.space.element_location(&w) else { continue };
            let full = w.toplevel().is_some_and(|t| t.current_state().states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen))
                || w.x11_surface().is_some_and(|x| x.is_fullscreen());
            if full || !og.contains(loc) && !og.contains(loc - Point::from((0, self.bar_h(&w)))) {
                continue;
            }
            let y = top + self.bar_h(&w);
            if loc.y < y {
                self.space.map_element(w.clone(), (loc.x, y), false);
                if let Some(x) = w.x11_surface() {
                    let _ = x.configure(Rectangle::new((loc.x, y).into(), x.geometry().size));
                }
                self.dirty = true;
            }
        }
    }

    pub fn rule_of(&self, w: &Window) -> Rule {
        rule(&crate::hyprcompat::app_id(w).to_lowercase(), &crate::hyprcompat::title(w), w.x11_surface().is_some())
    }

    /// The output a rule opens on: the primary monitor when asked and connected, else the active one.
    pub fn rule_output(&self, r: &Rule) -> Option<Output> {
        r.primary.then(|| self.space.outputs().find(|o| o.name() == PRIMARY).cloned()).flatten().or_else(|| self.active_output())
    }

    /// Map a placed new window with its rule (pin, fullscreen, focus).
    pub fn adopt_ruled(&mut self, w: Window, loc: Point<i32, Logical>, r: Rule) {
        crate::wm::data(&w).pinned.set(r.pip);
        self.adopt_window(w.clone(), loc, !r.no_focus);
        self.swallow(&w);
        if r.game {
            self.toggle_max(&w, true);
        }
    }

    /// Where a new window goes: centred in the usable area of its rule's output (PiP: bottom right).
    /// The size a new window is configured with: its rule's, else the one it had when it last closed
    /// (persistent_size), kept inside the usable area (PiP excepted).
    fn initial_size(&self, w: &Window, r: &Rule) -> Option<(i32, i32)> {
        let o = self.rule_output(r)?;
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let z = layer_map_for_output(&o).non_exclusive_zone();
        let bar = self.bar_h(w);
        let remembered = self.sizes.get(&crate::hyprcompat::app_id(w).to_lowercase()).map(|s| (s.w, s.h));
        let (sw, sh) = r.size.map(|s| s.on(og)).or(remembered)?;
        Some(if r.pip { (sw, sh) } else { (sw.min(z.size.w), sh.min(z.size.h - bar)) })
    }

    pub fn place_new(&self, w: &Window, r: &Rule) -> Point<i32, Logical> {
        let pip = r.pip;
        let Some(o) = self.rule_output(r) else { return Point::default() };
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let z = layer_map_for_output(&o).non_exclusive_zone();
        let area = Rectangle::<i32, Logical>::new(og.loc + z.loc, z.size);
        let size = w.geometry().size;
        let bar = self.bar_h(w);
        let (ow, oh) = (size.w, size.h + bar);
        if pip {
            return Point::from((area.loc.x + area.size.w - ow - GAP, area.loc.y + area.size.h - oh - GAP + bar));
        }
        // a dialog: centred on its parent, kept inside the usable area
        if let Some((pl, pg)) = self.parent_of(w).and_then(|p| Some((self.space.element_location(&p)?, p.geometry()))) {
            let pbar = self.bar_h(&self.parent_of(w).unwrap());
            let (cx, cy) = (pl.x + pg.size.w / 2, pl.y - pbar + (pg.size.h + pbar) / 2);
            let x = (cx - ow / 2).clamp(area.loc.x, (area.loc.x + area.size.w - ow).max(area.loc.x));
            let y = (cy - oh / 2).clamp(area.loc.y, (area.loc.y + area.size.h - oh).max(area.loc.y)) + bar;
            return Point::from((x, y));
        }
        let x = area.loc.x + ((area.size.w - ow) / 2).max(0);
        let y = area.loc.y + ((area.size.h - oh) / 2).max(0) + bar;
        Point::from((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windowrules() {
        let og = Rectangle::<i32, Logical>::new((0, 0).into(), (1920, 1080).into());
        assert_eq!(rule("kitty", "zsh", false).size.map(|s| s.on(og)), Some((1190, 777)));
        assert_eq!(rule("org.gnome.calculator", "Calculator", false).size.map(|s| s.on(og)), Some((326, 464)));
        let pip = rule("zen", "Picture-in-Picture", false);
        assert!(pip.pip && pip.size.map(|s| s.on(og)) == Some((480, 270)));
        let game = rule("steam_app_1091500", "Cyberpunk 2077", true);
        assert!(game.game && game.primary);
        assert!(!rule("steam_app_1091500", "", true).game);
        assert!(rule("battle.net.exe", "x", true).primary && rule("vesktop", "Discord", false).primary);
        assert!(rule("", "", true).no_focus && !rule("", "", false).no_focus);
        assert_eq!(rule("gedit", "x", false), Rule::default());
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Wish {
    Max,
    Full,
}

/// What an unmapped toplevel asked to be (xdg set_maximized / set_fullscreen before its first buffer).
struct MapWish(std::cell::Cell<Option<Wish>>);
