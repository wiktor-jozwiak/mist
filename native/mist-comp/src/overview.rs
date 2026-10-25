//! Overview (Mission Control, `overview` action, SUPER+G, three fingers up): the monitor's workspaces one
//! under another (niri), each a card with its windows side by side in a grid, the shown one in the middle
//! over a dimmed desktop (250 ms there and back). Keyboard: ← → pick a window in the workspace, ↑ ↓ the
//! workspace above / below (the cards scroll), Tab / Shift+Tab every window in turn, Enter goes there
//! (switching workspace when it is another one), Escape / the bind again go back. A click picks a window (or
//! an empty card's workspace), a click beside the cards goes back; three fingers down too. With one
//! workspace the grid takes the whole monitor, as before.
//! Drawn from the windows' own surfaces rescaled (hidden workspaces' windows show their last frame), no
//! snapshots or offscreen passes: nothing runs while it is closed, and an open overview without changes or
//! scrolling draws no frames.
use std::time::{Duration, Instant};

use smithay::{
    desktop::{layer_map_for_output, Window},
    input::keyboard::keysyms,
    utils::{Logical, Point, Rectangle, Size},
};

use crate::MistComp;

const D: Duration = Duration::from_millis(250);
const SCROLL: Duration = Duration::from_millis(200);
const GAP: i32 = 32;
/// between workspace cards
const ROW_GAP: i32 = 28;
/// a workspace card's height, of the free area, when there are several: all of them when they fit at this
/// size or more (three per monitor: all three), else this much and the rest scrolls into view
const ROW_H_MIN: f64 = 0.3;
const ROW_H_MAX: f64 = 0.5;

pub struct Row {
    pub ws: u32,
    /// the card, in overview space (row 0 at y = 0; `offset` places it on the output)
    pub card: Rectangle<i32, Logical>,
    /// each window and its place in the grid (overview space, bar included), reading order
    pub slots: Vec<(Window, Rectangle<i32, Logical>)>,
    pub id: smithay::backend::renderer::element::Id,
}

pub struct Overview {
    pub output: String,
    pub rows: Vec<Row>,
    /// the row of the workspace shown when it opened (its windows fly into the grid, the others slide in)
    pub home: usize,
    /// selection: row, window in it (None: an empty workspace)
    pub sel: (usize, Option<usize>),
    start: Instant,
    pub closing: bool,
    /// vertical scroll of the cards (overview space -> output), animated between rows
    scroll_from: f64,
    scroll_to: f64,
    scroll_start: Instant,
    /// the dimming's and the selection ring's damage ids
    pub dim: smithay::backend::renderer::element::Id,
    pub ring: smithay::backend::renderer::element::Id,
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
        now.saturating_duration_since(self.start) < D || now.saturating_duration_since(self.scroll_start) < SCROLL
    }

    pub fn done(&self, now: Instant) -> bool {
        self.closing && !now.saturating_duration_since(self.start).lt(&D)
    }

    /// overview space -> output-relative, now
    pub fn offset(&self, now: Instant) -> Point<i32, Logical> {
        let p = ease(now.saturating_duration_since(self.scroll_start).as_secs_f64() / SCROLL.as_secs_f64());
        (0, (self.scroll_from + (self.scroll_to - self.scroll_from) * p).round() as i32).into()
    }

    /// Every window's place on the output now (row, window, rect), front rows first.
    pub fn places(&self, now: Instant) -> Vec<(usize, &Window, Rectangle<i32, Logical>)> {
        let off = self.offset(now);
        self.rows.iter().enumerate().flat_map(|(i, r)| r.slots.iter().map(move |(w, s)| (i, w, Rectangle::new(s.loc + off, s.size)))).collect()
    }

    pub fn card(&self, row: usize, now: Instant) -> Option<Rectangle<i32, Logical>> {
        self.rows.get(row).map(|r| Rectangle::new(r.card.loc + self.offset(now), r.card.size))
    }

    pub fn selected(&self) -> Option<&Window> {
        let (r, w) = self.sel;
        self.rows.get(r).and_then(|r| r.slots.get(w?)).map(|(w, _)| w)
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

    /// Scroll so that `row` is in the middle of `area` (output-relative).
    fn scroll_to_row(&mut self, row: usize, area: Rectangle<i32, Logical>, animate: bool) {
        let Some(r) = self.rows.get(row) else { return };
        // all the cards fit: the stack stays centred, else the row comes to the middle
        let (top, bottom) = (self.rows[0].card.loc.y, self.rows.last().map_or(0, |l| l.card.loc.y + l.card.size.h));
        let to = if bottom - top <= area.size.h {
            (area.loc.y + (area.size.h - (bottom - top)) / 2 - top) as f64
        } else {
            (area.loc.y + (area.size.h - r.card.size.h) / 2 - r.card.loc.y) as f64
        };
        let now = Instant::now();
        self.scroll_from = if animate { self.offset(now).y as f64 } else { to };
        self.scroll_to = to;
        self.scroll_start = if animate { now } else { now.checked_sub(SCROLL).unwrap_or(now) };
    }

    /// The window in `row` nearest (by its centre's x) to the selected one, for ↑ ↓.
    fn nearest_in(&self, row: usize) -> Option<usize> {
        let x = self.rows.get(self.sel.0).and_then(|r| r.slots.get(self.sel.1?)).map(|(_, s)| s.loc.x + s.size.w / 2);
        let slots = &self.rows.get(row)?.slots;
        if slots.is_empty() {
            return None;
        }
        let Some(x) = x else { return Some(0) };
        slots.iter().enumerate().min_by_key(|(_, (_, s))| (s.loc.x + s.size.w / 2 - x).abs()).map(|(i, _)| i)
    }

    /// A key while open: true when it moved the selection (redraw).
    fn step(&mut self, sym: u32, shift: bool) -> bool {
        let (r, w) = self.sel;
        let n = self.rows.get(r).map_or(0, |r| r.slots.len());
        let sel = match sym {
            keysyms::KEY_Left | keysyms::KEY_h if n > 0 => (r, Some(w.map_or(0, |w| (w + n - 1) % n))),
            keysyms::KEY_Right | keysyms::KEY_l if n > 0 => (r, Some(w.map_or(0, |w| (w + 1) % n))),
            keysyms::KEY_Up | keysyms::KEY_k if r > 0 => (r - 1, self.nearest_in(r - 1)),
            keysyms::KEY_Down | keysyms::KEY_j if r + 1 < self.rows.len() => (r + 1, self.nearest_in(r + 1)),
            // every window in turn, across the workspaces
            keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => {
                let all: Vec<(usize, usize)> = self.rows.iter().enumerate().flat_map(|(i, row)| (0..row.slots.len()).map(move |k| (i, k))).collect();
                if all.is_empty() {
                    return false;
                }
                let at = w.and_then(|w| all.iter().position(|p| *p == (r, w)));
                let back = shift || sym == keysyms::KEY_ISO_Left_Tab;
                let k = match at {
                    Some(k) if back => (k + all.len() - 1) % all.len(),
                    Some(k) => (k + 1) % all.len(),
                    None => 0,
                };
                (all[k].0, Some(all[k].1))
            }
            _ => return false,
        };
        let moved = sel != self.sel;
        self.sel = sel;
        moved
    }
}

/// Grid places (in `area`) for windows of these sizes: as many columns as rows (wider first), every window
/// scaled down to its cell (never up, `gap` around it), the last row centred.
fn layout_gap(sizes: &[Size<i32, Logical>], area: Rectangle<i32, Logical>, gap: i32) -> Vec<Rectangle<i32, Logical>> {
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
            let k = ((cw - gap).max(1) as f64 / s.w.max(1) as f64).min((ch - gap).max(1) as f64 / s.h.max(1) as f64).min(1.0);
            let (w, h) = ((s.w as f64 * k).round() as i32, (s.h as f64 * k).round() as i32);
            let x = area.loc.x + shift + col as i32 * cw + (cw - w) / 2;
            let y = area.loc.y + row as i32 * ch + (ch - h) / 2;
            Rectangle::new((x, y).into(), (w, h).into())
        })
        .collect()
}

/// The cards of `n` workspaces in overview space (row i at y = i * (h + ROW_GAP)), for a free `area`: one
/// workspace fills it, several share its height (ROW_H_MIN..ROW_H_MAX each), in the monitor's proportions.
pub fn cards(n: usize, area: Rectangle<i32, Logical>, screen: Size<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    if n <= 1 {
        return vec![Rectangle::new((area.loc.x, 0).into(), area.size)];
    }
    let fit = (area.size.h - (n as i32 - 1) * ROW_GAP) as f64 / n as f64;
    let h = fit.clamp(area.size.h as f64 * ROW_H_MIN, area.size.h as f64 * ROW_H_MAX).round() as i32;
    let w = ((h as f64 * screen.w as f64 / screen.h.max(1) as f64).round() as i32).min(area.size.w);
    let x = area.loc.x + (area.size.w - w) / 2;
    (0..n).map(|i| Rectangle::new((x, i as i32 * (h + ROW_GAP)).into(), (w, h).into())).collect()
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

    /// The free area of the overview's output (output-relative), less the margins.
    fn overview_area(&self, o: &smithay::output::Output) -> Rectangle<i32, Logical> {
        let zone = layer_map_for_output(o).non_exclusive_zone();
        Rectangle::new(zone.loc + Point::from((GAP, GAP)), (zone.size.w - 2 * GAP, zone.size.h - 2 * GAP).into())
    }

    pub fn overview_toggle(&mut self) {
        if self.overview_open() {
            return self.overview_close();
        }
        let Some(o) = self.active_output() else { return };
        let Some(og) = self.space.output_geometry(&o) else { return };
        let cur = self.active_ws(&o);
        // the shown workspace's windows where they are, the hidden ones' where they were left
        let mut shown: Vec<(Window, Rectangle<i32, Logical>)> = self
            .space
            .elements()
            .filter(|w| self.output_of(w).as_ref() == Some(&o))
            .filter_map(|w| Some((w.clone(), visual(w, self.space.element_location(w)?, og.loc))))
            .collect();
        let taken: Vec<u32> = self.ws_active.iter().filter(|(n, _)| **n != o.name()).map(|(_, v)| *v).collect();
        let mut wss: Vec<u32> = self
            .stash
            .iter()
            .map(|(w, _)| crate::wm::data(w).ws.get())
            .filter(|ws| *ws < crate::wm::SPECIAL && *ws != cur && !taken.contains(ws) && self.ws_home(*ws).is_none_or(|h| h == o))
            .collect();
        wss.push(cur);
        wss.sort();
        wss.dedup();
        if shown.is_empty() && wss.len() == 1 {
            return;
        }
        let area = self.overview_area(&o);
        let cards = cards(wss.len(), area, og.size);
        let mut rows = Vec::new();
        for (ws, card) in wss.iter().zip(cards) {
            let mut wins: Vec<(Window, Rectangle<i32, Logical>)> = if *ws == cur {
                std::mem::take(&mut shown)
            } else {
                self.stash.iter().filter(|(w, _)| crate::wm::data(w).ws.get() == *ws).map(|(w, at)| (w.clone(), visual(w, *at, (0, 0).into()))).collect()
            };
            // reading order of where they are: the grid keeps them roughly in place
            wins.sort_by_key(|(_, r)| (r.loc.y / 200, r.loc.x));
            let inner = if wss.len() == 1 { card } else { Rectangle::new(card.loc + Point::from((12, 12)), (card.size.w - 24, card.size.h - 24).into()) };
            let gap = if wss.len() == 1 { GAP } else { GAP / 2 };
            let places = layout_gap(&wins.iter().map(|(_, r)| r.size).collect::<Vec<_>>(), inner, gap);
            let slots = wins.into_iter().zip(places).map(|((w, _), p)| (w, p)).collect();
            rows.push(Row { ws: *ws, card, slots, id: smithay::backend::renderer::element::Id::new() });
        }
        let home = wss.iter().position(|w| *w == cur).unwrap_or(0);
        // the focused window selected (else the first)
        let focused = self.focused_window();
        let sel_w = rows[home].slots.iter().position(|(w, _)| Some(w) == focused.as_ref()).or((!rows[home].slots.is_empty()).then_some(0));
        let now = Instant::now();
        let mut ov = Overview {
            output: o.name(),
            rows,
            home,
            sel: (home, sel_w),
            start: now,
            closing: false,
            scroll_from: 0.0,
            scroll_to: 0.0,
            scroll_start: now,
            dim: smithay::backend::renderer::element::Id::new(),
            ring: smithay::backend::renderer::element::Id::new(),
        };
        ov.scroll_to_row(home, area, false);
        self.anim.overview = Some(ov);
        self.dirty = true;
    }

    pub fn overview_close(&mut self) {
        if let Some(ov) = self.anim.overview.as_mut() {
            ov.close();
            self.dirty = true;
        }
    }

    /// Go to row `row`'s workspace, focusing `w` (or its top window): the overview closes on the spot when
    /// it is another workspace (its slide shows the way), animated back on this one.
    fn overview_go(&mut self, row: usize, w: Option<Window>) {
        let Some(ov) = self.anim.overview.as_ref() else { return };
        let ws = ov.rows.get(row).map(|r| r.ws);
        let same = row == ov.home;
        let out = self.space.outputs().find(|o| o.name() == ov.output).cloned();
        if same {
            if let Some(w) = w {
                self.focus_window(Some(&w));
            }
            return self.overview_close();
        }
        self.anim.overview = None;
        if let (Some(ws), Some(o)) = (ws, out) {
            self.switch_ws(&o, ws);
            match w {
                Some(w) => self.focus_window(Some(&w)),
                None => self.focus_top_on(&o),
            }
        }
        self.dirty = true;
    }

    /// A key while the overview is open (input.rs takes arrows, hjkl, Tab, Enter from the clients then).
    pub fn overview_key(&mut self, sym: u32, shift: bool) {
        let Some(ov) = self.anim.overview.as_mut().filter(|o| !o.closing) else { return };
        if sym == keysyms::KEY_Return || sym == keysyms::KEY_KP_Enter || sym == keysyms::KEY_space {
            let (row, w) = (ov.sel.0, ov.selected().cloned());
            return self.overview_go(row, w);
        }
        if ov.step(sym, shift) {
            let row = ov.sel.0;
            let out = self.space.outputs().find(|o| o.name() == ov.output).cloned();
            if let Some(o) = out {
                let area = self.overview_area(&o);
                if let Some(ov) = self.anim.overview.as_mut() {
                    ov.scroll_to_row(row, area, true);
                }
            }
            self.dirty = true;
        }
    }

    /// The overview takes this key (it is open and the key moves or picks).
    pub fn overview_wants(&self, sym: u32) -> bool {
        self.overview_open()
            && matches!(
                sym,
                keysyms::KEY_Left
                    | keysyms::KEY_Right
                    | keysyms::KEY_Up
                    | keysyms::KEY_Down
                    | keysyms::KEY_h
                    | keysyms::KEY_j
                    | keysyms::KEY_k
                    | keysyms::KEY_l
                    | keysyms::KEY_Tab
                    | keysyms::KEY_ISO_Left_Tab
                    | keysyms::KEY_Return
                    | keysyms::KEY_KP_Enter
                    | keysyms::KEY_space
            )
    }

    /// A press while the overview is open: the window under it comes forward (its workspace shown) and the
    /// overview closes; on an empty card its workspace; beside the cards it just closes. True: the press is
    /// the overview's (the client never hears it).
    pub fn overview_click(&mut self, at: Point<f64, Logical>) -> bool {
        let Some(ov) = self.anim.overview.as_ref().filter(|o| !o.closing) else { return false };
        let Some(og) = self.space.outputs().find(|o| o.name() == ov.output).and_then(|o| self.space.output_geometry(o)) else { return false };
        let p = (at - og.loc.to_f64()).to_i32_round();
        let now = Instant::now();
        let picked = ov.places(now).into_iter().find(|(_, _, r)| r.contains(p)).map(|(row, w, _)| (row, Some(w.clone())));
        let picked = picked.or_else(|| (0..ov.rows.len()).find(|i| ov.rows.len() > 1 && ov.card(*i, now).is_some_and(|c| c.contains(p))).map(|i| (i, None)));
        match picked {
            Some((row, w)) => self.overview_go(row, w),
            None => self.overview_close(),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fits_and_centres_last_row() {
        let area = Rectangle::new((0, 0).into(), (1200, 800).into());
        let r = layout_gap(&[(800, 600).into(), (800, 600).into(), (400, 300).into()], area, GAP);
        assert_eq!(r.len(), 3);
        for x in &r {
            assert!(area.contains_rect(*x), "{x:?}");
        }
        // two columns: the third one alone in the second row, centred
        assert_eq!(r[2].loc.x + r[2].size.w / 2, 600);
        // never scaled up
        assert!(r[2].size.w <= 400);
    }

    #[test]
    fn cards_stack_in_screen_proportions() {
        let area = Rectangle::new((32, 64).into(), (1856, 984).into());
        let one = cards(1, area, (1920, 1080).into());
        assert_eq!(one[0].size, area.size);
        let three = cards(3, area, (1920, 1080).into());
        assert_eq!(three.len(), 3);
        // same size, one under another, centred, 16:9
        assert!(three.windows(2).all(|p| p[0].size == p[1].size && p[1].loc.y == p[0].loc.y + p[0].size.h + ROW_GAP));
        assert!((three[0].loc.x + three[0].size.w / 2 - (area.loc.x + area.size.w / 2)).abs() <= 1);
        assert!((three[0].size.w as f64 / three[0].size.h as f64 - 16.0 / 9.0).abs() < 0.01);
    }
}
