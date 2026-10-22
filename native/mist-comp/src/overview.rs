//! Overview (Mission Control, `overview` action, SUPER+O, three fingers up): every window of the monitor's
//! shown workspace side by side, scaled into a grid over a dimmed desktop (250 ms there and back). A click
//! picks one (focus + raise), Escape / the bind again / three fingers down / a click beside the windows go
//! back. Drawn from the windows' own surfaces rescaled, no snapshots or offscreen passes: nothing runs while
//! it is closed, and an open overview without changes draws no frames.
use std::time::{Duration, Instant};

use smithay::{
    desktop::{layer_map_for_output, Window},
    utils::{Logical, Point, Rectangle, Size},
};

use crate::MistComp;

const D: Duration = Duration::from_millis(250);
const GAP: i32 = 32;

pub struct Overview {
    pub output: String,
    /// each window and its place in the grid (output-relative, bar included)
    pub slots: Vec<(Window, Rectangle<i32, Logical>)>,
    start: Instant,
    pub closing: bool,
    /// the dimming's damage id
    pub dim: smithay::backend::renderer::element::Id,
}

fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(4)
}

impl Overview {
    /// 0 = the desktop as it is, 1 = the grid.
    pub fn progress(&self, now: Instant) -> f64 {
        let p = ease(now.saturating_duration_since(self.start).as_secs_f64() / D.as_secs_f64());
        if self.closing {
            1.0 - p
        } else {
            p
        }
    }

    pub fn moving(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < D
    }

    pub fn done(&self, now: Instant) -> bool {
        self.closing && !self.moving(now)
    }

    pub fn slot(&self, w: &Window) -> Option<Rectangle<i32, Logical>> {
        self.slots.iter().find(|(s, _)| s == w).map(|(_, r)| *r)
    }

    fn close(&mut self) {
        if !self.closing {
            // a close during the opening runs back from where it is
            let now = Instant::now();
            let p = self.progress(now);
            self.closing = true;
            // progress(now) = 1 - ease(t) = p  =>  t = 1 - p^(1/4)
            let t = 1.0 - p.clamp(0.0, 1.0).powf(0.25);
            self.start = now.checked_sub(D.mul_f64(t)).unwrap_or(now);
        }
    }
}

/// Grid places (in `area`) for windows of these sizes: as many columns as rows (wider first), every window
/// scaled down to its cell (never up), the last row centred.
pub fn layout(sizes: &[Size<i32, Logical>], area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    let n = sizes.len().max(1);
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    let cw = area.size.w / cols as i32;
    let ch = area.size.h / rows as i32;
    sizes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (row, col) = (i / cols, i % cols);
            let in_row = if row == rows - 1 { n - row * cols } else { cols };
            let shift = (cols - in_row) as i32 * cw / 2;
            let k = ((cw - GAP) as f64 / s.w.max(1) as f64).min((ch - GAP) as f64 / s.h.max(1) as f64).min(1.0);
            let (w, h) = ((s.w as f64 * k).round() as i32, (s.h as f64 * k).round() as i32);
            let x = area.loc.x + shift + col as i32 * cw + (cw - w) / 2;
            let y = area.loc.y + row as i32 * ch + (ch - h) / 2;
            Rectangle::new((x, y).into(), (w, h).into())
        })
        .collect()
}

/// What a window covers with its bar, relative to `origin`.
pub fn visual(w: &Window, loc: Point<i32, Logical>, origin: Point<i32, Logical>) -> Rectangle<i32, Logical> {
    let bar = if crate::wm::data(w).deco.bar.borrow().is_some() { crate::decor::BAR } else { 0 };
    let g = w.geometry().size;
    Rectangle::new((loc.x - origin.x, loc.y - origin.y - bar).into(), (g.w, g.h + bar).into())
}

impl MistComp {
    pub fn overview_open(&self) -> bool {
        self.anim.overview.as_ref().is_some_and(|o| !o.closing)
    }

    pub fn overview_toggle(&mut self) {
        if self.overview_open() {
            return self.overview_close();
        }
        let Some(o) = self.active_output() else { return };
        let Some(og) = self.space.output_geometry(&o) else { return };
        let mut wins: Vec<(Window, Rectangle<i32, Logical>)> = self
            .space
            .elements()
            .filter(|w| self.output_of(w).as_ref() == Some(&o))
            .filter_map(|w| Some((w.clone(), visual(w, self.space.element_location(w)?, og.loc))))
            .collect();
        if wins.is_empty() {
            return;
        }
        // reading order of where they are now: the grid keeps them roughly in place
        wins.sort_by_key(|(_, r)| (r.loc.y / 200, r.loc.x));
        let zone = layer_map_for_output(&o).non_exclusive_zone();
        let area = Rectangle::new(zone.loc + Point::from((GAP, GAP)), (zone.size.w - 2 * GAP, zone.size.h - 2 * GAP).into());
        let places = layout(&wins.iter().map(|(_, r)| r.size).collect::<Vec<_>>(), area);
        let slots = wins.into_iter().zip(places).map(|((w, _), p)| (w, p)).collect();
        self.anim.overview = Some(Overview { output: o.name(), slots, start: Instant::now(), closing: false, dim: smithay::backend::renderer::element::Id::new() });
        self.dirty = true;
    }

    pub fn overview_close(&mut self) {
        if let Some(ov) = self.anim.overview.as_mut() {
            ov.close();
            self.dirty = true;
        }
    }

    /// A press while the overview is open: the window under it comes forward and the overview closes; a
    /// press beside the windows just closes it. True: the press is the overview's (the client never hears it).
    pub fn overview_click(&mut self, at: Point<f64, Logical>) -> bool {
        let Some(ov) = self.anim.overview.as_ref().filter(|o| !o.closing) else { return false };
        let og = self.space.outputs().find(|o| o.name() == ov.output).and_then(|o| self.space.output_geometry(o));
        let picked = og.and_then(|og| {
            let p = (at - og.loc.to_f64()).to_i32_round();
            ov.slots.iter().find(|(_, r)| r.contains(p)).map(|(w, _)| w.clone())
        });
        if let Some(w) = picked {
            self.focus_window(Some(&w));
        }
        self.overview_close();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fits_and_centres_last_row() {
        let area = Rectangle::new((0, 0).into(), (1200, 800).into());
        let r = layout(&[(800, 600).into(), (800, 600).into(), (400, 300).into()], area);
        assert_eq!(r.len(), 3);
        for x in &r {
            assert!(area.contains_rect(*x), "{x:?}");
        }
        // two columns: the third one alone in the second row, centred
        assert_eq!(r[2].loc.x + r[2].size.w / 2, 600);
        // never scaled up
        assert!(r[2].size.w <= 400);
    }
}
