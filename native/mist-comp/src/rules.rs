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
            if let Some(o) = self.rule_output(&r) {
                let og = self.space.output_geometry(&o).unwrap_or_default();
                let z = layer_map_for_output(&o).non_exclusive_zone();
                let bar = self.bar_h(&w);
                // no rule size: the size it had when it last closed (persistent_size)
                let remembered = self.sizes.get(&crate::hyprcompat::app_id(&w).to_lowercase()).map(|s| (s.w, s.h));
                if let Some((sw, sh)) = r.size.map(|s| s.on(og)).or(remembered) {
                    let (sw, sh) = if r.pip { (sw, sh) } else { (sw.min(z.size.w), sh.min(z.size.h - bar)) };
                    t.with_pending_state(|s| s.size = Some((sw, sh).into()));
                }
            }
            t.send_configure();
            return true;
        }
        if !with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false) {
            return true;
        }
        self.pending.remove(i);
        w.on_commit();
        let r = self.rule_of(&w);
        let loc = self.place_new(&w, &r);
        self.adopt_ruled(w, loc, r);
        true
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
