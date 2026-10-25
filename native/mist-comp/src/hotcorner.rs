//! Hot corners (`corner tl overview` in comp.conf, GNOME / macOS): the pointer pushed into a screen corner
//! runs the action once, and again only after it left the corner. Corners another monitor continues past
//! are not corners. Nothing fires during a drag, while locked or over a fullscreen window (games aim
//! there). Cost: one comparison per pointer motion, none without `corner` lines.
use crate::config::Action;
use crate::state::MistComp;
use smithay::utils::{Logical, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// `tl overview`, `br exec mist-ui-bin lock`; `tl off` removes that corner
pub fn parse(rest: &str) -> Option<(Corner, Option<Action>)> {
    let (c, a) = rest.split_once(char::is_whitespace)?;
    let corner = match c {
        "tl" => Corner::TopLeft,
        "tr" => Corner::TopRight,
        "bl" => Corner::BottomLeft,
        "br" => Corner::BottomRight,
        _ => return None,
    };
    match a.trim() {
        "off" => Some((corner, None)),
        a => Some((corner, Some(crate::config::parse_action(a)?))),
    }
}

/// How far from the corner still counts (the pointer is clamped to the screen, so 0 is reached).
const REACH: f64 = 2.0;
/// Leaving this far re-arms the corner.
const AWAY: f64 = 24.0;

/// The corner of one of `outputs` (x, y, w, h) at `pos`, within `r`: only where no other output goes on.
fn corner_at(outputs: &[(f64, f64, f64, f64)], pos: Point<f64, Logical>, r: f64) -> Option<Corner> {
    let inside = |x: f64, y: f64| outputs.iter().any(|&(ox, oy, w, h)| x >= ox && x < ox + w && y >= oy && y < oy + h);
    for &(x, y, w, h) in outputs {
        if !inside(pos.x, pos.y) || pos.x < x || pos.x >= x + w || pos.y < y || pos.y >= y + h {
            continue;
        }
        let (l, t) = (pos.x - x <= r, pos.y - y <= r);
        let (rr, b) = (x + w - 1.0 - pos.x <= r, y + h - 1.0 - pos.y <= r);
        let (c, cx, cy, dx, dy) = match (l, rr, t, b) {
            (true, _, true, _) => (Corner::TopLeft, x, y, -1.0, -1.0),
            (_, true, true, _) => (Corner::TopRight, x + w - 1.0, y, 1.0, -1.0),
            (true, _, _, true) => (Corner::BottomLeft, x, y + h - 1.0, -1.0, 1.0),
            (_, true, _, true) => (Corner::BottomRight, x + w - 1.0, y + h - 1.0, 1.0, 1.0),
            _ => return None,
        };
        // a monitor beside or above / below: the pointer goes on there, no corner to hit
        if inside(cx + dx, cy) || inside(cx, cy + dy) || inside(cx + dx, cy + dy) {
            return None;
        }
        return Some(c);
    }
    None
}

impl MistComp {
    pub fn hot_corner(&mut self, pos: Point<f64, Logical>) {
        if self.config.corners.is_empty() {
            return;
        }
        let outs: Vec<(f64, f64, f64, f64)> = self
            .space
            .outputs()
            .filter_map(|o| self.space.output_geometry(o))
            .map(|g| (g.loc.x as f64, g.loc.y as f64, g.size.w as f64, g.size.h as f64))
            .collect();
        if let Some(c) = self.corner_in {
            // armed again once the pointer left the corner
            if corner_at(&outs, pos, AWAY) != Some(c) {
                self.corner_in = None;
            }
            return;
        }
        let Some(c) = corner_at(&outs, pos, REACH) else { return };
        let Some(action) = self.config.corners.iter().find(|(k, _)| *k == c).map(|(_, a)| a.clone()) else { return };
        self.corner_in = Some(c);
        let pointer = self.seat.get_pointer().unwrap();
        if self.locked || pointer.is_grabbed() || self.active_output().is_some_and(|o| self.fullscreen_on(&o)) {
            return;
        }
        tracing::info!("hot corner {c:?}: {action:?}");
        self.do_action(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_only_where_the_screen_ends() {
        let one = [(0.0, 0.0, 1920.0, 1080.0)];
        assert_eq!(corner_at(&one, (0.0, 0.0).into(), REACH), Some(Corner::TopLeft));
        assert_eq!(corner_at(&one, (1919.0, 1.0).into(), REACH), Some(Corner::TopRight));
        assert_eq!(corner_at(&one, (1.0, 1079.0).into(), REACH), Some(Corner::BottomLeft));
        assert_eq!(corner_at(&one, (500.0, 0.0).into(), REACH), None);
        // two side by side: the shared edge has no corners, the outer ones stay
        let two = [(0.0, 0.0, 1920.0, 1080.0), (1920.0, 0.0, 1920.0, 1080.0)];
        assert_eq!(corner_at(&two, (1919.0, 0.0).into(), REACH), None);
        assert_eq!(corner_at(&two, (1920.0, 0.0).into(), REACH), None);
        assert_eq!(corner_at(&two, (3839.0, 0.0).into(), REACH), Some(Corner::TopRight));
        assert_eq!(parse("tl overview"), Some((Corner::TopLeft, Some(Action::Overview))));
        assert_eq!(parse("br off"), Some((Corner::BottomRight, None)));
        assert_eq!(parse("xx overview"), None);
    }
}
