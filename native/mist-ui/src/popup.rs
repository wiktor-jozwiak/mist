// Popup menus of mist-ui: an Overlay layer surface at a given place on a monitor, with a click catcher
// behind it (1x1 buffer stretched by wp_viewporter) that closes it. Two looks:
// - Ctx: the Mist context menu (CtxMenu.qml): dark sheet, 12 px corners, teal highlight (tray menus)
// - Sheet: a liquid-glass sheet (Glass.qml, radius 18) with icon rows (the dock's app menu)
use crate::gfx::{Canvas, Rgba};
use crate::glass::Glass;
use crate::text::{Style, Text};
use crate::theme::*;

#[derive(Clone, Debug)]
pub enum Row {
    Header(String),
    Item { icon: String, label: String, enabled: bool, danger: bool, check: bool, sub: bool },
    Sep,
}
impl Row {
    pub fn item(icon: &str, label: &str) -> Row {
        Row::Item { icon: icon.into(), label: label.into(), enabled: true, danger: false, check: false, sub: false }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    Ctx,
    Sheet,
}

const SHEET_M: f32 = 30.0; // room for the sheet's shadow

pub fn row_h(look: Look, r: &Row) -> f32 {
    match (look, r) {
        (_, Row::Sep) => if look == Look::Ctx { 9.0 } else { 1.0 },
        (Look::Ctx, _) => 28.0,
        (Look::Sheet, Row::Header(_)) => 24.0,
        (Look::Sheet, _) => 36.0,
    }
}

pub fn size(t: &mut Text, look: Look, rows: &[Row]) -> (u32, u32) {
    match look {
        Look::Ctx => {
            let st = Style::ui(13.0, 500.0);
            let mut w: f32 = 160.0;
            for r in rows {
                if let Row::Item { label, .. } = r {
                    w = w.max(t.width(label, &st) + 56.0);
                }
            }
            let h: f32 = rows.iter().map(|r| row_h(look, r)).sum::<f32>() + 16.0;
            (w.ceil() as u32, h.ceil() as u32)
        }
        Look::Sheet => {
            let h: f32 = rows.iter().map(|r| row_h(look, r)).sum::<f32>() + 20.0;
            ((300.0 + 2.0 * SHEET_M) as u32, (h + 2.0 * SHEET_M).ceil() as u32)
        }
    }
}

// row index under (x, y) of the surface
pub fn row_at(look: Look, rows: &[Row], x: f32, y: f32, w: f32) -> Option<usize> {
    let (ox, oy, iw) = match look { Look::Ctx => (0.0, 8.0, w), Look::Sheet => (SHEET_M + 10.0, SHEET_M + 10.0, 280.0) };
    if x < ox || x > ox + iw {
        return None;
    }
    let mut ry = oy;
    for (i, r) in rows.iter().enumerate() {
        let h = row_h(look, r);
        if y >= ry && y < ry + h {
            return matches!(r, Row::Item { enabled: true, .. }).then_some(i);
        }
        ry += h;
    }
    None
}

pub fn paint(c: &mut Canvas, t: &mut Text, look: Look, rows: &[Row], hover: Option<usize>) {
    c.clear();
    let (w, h) = (c.w(), c.h());
    match look {
        Look::Ctx => {
            c.rrect(0.0, 0.0, w, h, 12.0, Rgba::rgb8(24, 30, 43, 0.96));
            crate::gfx::stroke_rrect(c, 0.5, 0.5, w - 1.0, h - 1.0, 11.5, Rgba(1.0, 1.0, 1.0, 0.09));
            let st = Style::ui(13.0, 500.0);
            let mut y = 8.0;
            for (i, r) in rows.iter().enumerate() {
                let rh = row_h(look, r);
                match r {
                    Row::Sep => c.rrect(12.0, y + 4.0, w - 24.0, 1.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.08)),
                    Row::Header(s) => { c.text(t, 16.0, y + rh / 2.0, s, &st, T3); }
                    Row::Item { label, enabled, check, sub, .. } => {
                        let hov = hover == Some(i) && *enabled;
                        if hov {
                            c.rrect(6.0, y + 1.0, w - 12.0, rh - 2.0, 8.0, Rgba::rgb8(138, 214, 208, 0.9));
                        }
                        let fg = if !enabled { T3 } else if hov { Rgba::hex("#0b1118") } else { T1 };
                        if *check {
                            c.icon(t, 20.0, y + rh / 2.0, "tick", 14.0, fg);
                        }
                        c.text(t, 36.0, y + rh / 2.0, label, &st, fg);
                        if *sub {
                            c.icon(t, w - 18.0, y + rh / 2.0, "right", 13.0, fg.alpha(0.7));
                        }
                    }
                }
                y += rh;
            }
        }
        Look::Sheet => {
            let (sw, sh) = (w - 2.0 * SHEET_M, h - 2.0 * SHEET_M);
            // Glass.qml: tint 0.92 -> glass tint 0.3 + 0.92 * 0.2, shine 1.15, shadow 0.2, rims drawn above
            let body = Glass { radius: 18.0, tint: 0.3 + 0.92 * 0.2, shine: 1.15, shadow_opacity: 0.2, rims: false, ..Default::default() };
            body.paint(c, SHEET_M, SHEET_M, sw, sh);
            let (x0, mut y) = (SHEET_M + 10.0, SHEET_M + 10.0);
            let iw = sw - 20.0;
            for (i, r) in rows.iter().enumerate() {
                let rh = row_h(look, r);
                match r {
                    Row::Sep => c.rrect(x0, y, iw, 1.0, 0.0, HAIR),
                    Row::Header(s) => {
                        let st = Style::ui(11.0, 600.0).spaced(1.2);
                        c.text(t, x0 + 10.0, y + 4.0 + 7.0, &s.to_uppercase(), &st, T3);
                    }
                    Row::Item { icon, label, danger, .. } => {
                        if hover == Some(i) {
                            c.rrect(x0, y, iw, rh, 10.0, FILL2);
                        }
                        c.rrect(x0 + 8.0, y + (rh - 24.0) / 2.0, 24.0, 24.0, 7.0, if *danger { Rgba::rgb8(238, 154, 170, 0.14) } else { FILL });
                        c.icon(t, x0 + 20.0, y + rh / 2.0, icon, 14.0, if *danger { DANGER } else { T2 });
                        let st = Style::ui(13.0, 450.0);
                        let lab = crate::bar::elide(t, label, &st, iw - 52.0);
                        c.text(t, x0 + 42.0, y + rh / 2.0, &lab, &st, if *danger { DANGER } else { T1 });
                    }
                }
                y += rh;
            }
            Glass { radius: 18.0, rim_only: true, shine: 1.15, ..Default::default() }.paint(c, SHEET_M, SHEET_M, sw, sh);
        }
    }
}

// the popup's top-left must be placed so the sheet (not its shadow margin) lands at (x, y)
pub fn margin(look: Look) -> f32 {
    if look == Look::Sheet { SHEET_M } else { 0.0 }
}
