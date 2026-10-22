//! Which desktop layers (Background / Bottom: the live wallpaper, the weather sky and card) are hidden under
//! windows: `mistcover>>OUTPUT,NAMESPACE,COVERED,FULLSCREEN` on the Hyprland event socket, only when that
//! changes. COVERED = windows of the shown workspace over 85 % of the layer's usable part (Cover.qml's rule),
//! FULLSCREEN = a maximized / fullscreen window on the output. mist-wall and mist-ui pause what nobody sees
//! from it, with no window queries and no polling (resizes included, which have no Hyprland event).
use crate::state::MistComp;
use smithay::desktop::layer_map_for_output;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::shell::wlr_layer::Layer;
use std::hash::{Hash, Hasher};

/// Share of 32 x 18 sample points of `r` inside any of `wins`.
fn covered(r: Rectangle<i32, Logical>, wins: &[Rectangle<i32, Logical>]) -> bool {
    if wins.is_empty() || r.size.w <= 0 || r.size.h <= 0 {
        return false;
    }
    let mut n = 0;
    for i in 0..32 {
        let x = r.loc.x as f64 + (i as f64 + 0.5) * r.size.w as f64 / 32.0;
        for j in 0..18 {
            let y = r.loc.y as f64 + (j as f64 + 0.5) * r.size.h as f64 / 18.0;
            if wins.iter().any(|w| x >= w.loc.x as f64 && x < (w.loc.x + w.size.w) as f64 && y >= w.loc.y as f64 && y < (w.loc.y + w.size.h) as f64) {
                n += 1;
            }
        }
    }
    n * 100 >= 576 * 85
}

impl MistComp {
    /// Once per loop turn: a hash of what decides coverage; the 576-point check only when it changed (and
    /// someone listens).
    pub fn cover_sync(&mut self) {
        // windows (with their bar), full flag, outputs, desktop layers
        let mut wins = Vec::new();
        for w in self.space.elements() {
            let Some(mut g) = self.space.element_geometry(w) else { continue };
            if crate::wm::data(w).deco.bar.borrow().is_some() {
                g.loc.y -= crate::decor::BAR;
                g.size.h += crate::decor::BAR;
            }
            wins.push((g, crate::wm::data(w).restore.borrow().is_some()));
        }
        let mut layers = Vec::new();
        for o in self.space.outputs() {
            let Some(og) = self.space.output_geometry(o) else { continue };
            let map = layer_map_for_output(o);
            let zone = map.non_exclusive_zone();
            for l in map.layers().filter(|l| matches!(l.layer(), Layer::Background | Layer::Bottom)) {
                let Some(mut g) = map.layer_geometry(l) else { continue };
                // a full-monitor layer: only the part windows can cover (not under the bar)
                g = g.intersection(zone).unwrap_or(g);
                g.loc += og.loc;
                layers.push((o.name(), l.namespace().to_string(), g, og));
            }
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (g, f) in &wins {
            (g.loc.x, g.loc.y, g.size.w, g.size.h, f).hash(&mut h);
        }
        for (o, ns, g, _) in &layers {
            (o, ns, g.loc.x, g.loc.y, g.size.w, g.size.h).hash(&mut h);
        }
        let sig = h.finish() | 1;
        if sig == self.cover_sig {
            return;
        }
        self.cover_sig = sig;
        // the same change moves the saved session (session.rs)
        self.session_mark();
        if self.hypr_listeners.is_empty() {
            return;
        }
        let rects: Vec<_> = wins.iter().map(|w| w.0).collect();
        for (o, ns, g, og) in layers {
            let full = wins.iter().any(|(w, f)| *f && w.overlaps(og));
            let state = (covered(g, &rects), full);
            if self.cover_sent.get(&(o.clone(), ns.clone())) != Some(&state) {
                self.hypr_event("mistcover", &format!("{o},{ns},{},{}", state.0 as u8, state.1 as u8));
                self.cover_sent.insert((o, ns), state);
            }
        }
    }
}
