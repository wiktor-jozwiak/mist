// Liquid glass (components/LiquidGlass.qml 1:1) on the CPU: Apple continuous corners (superellipse quadrants,
// sq()), a tinted body with a light fall-off, a soft bevel, a 1 px lens rim lit top-left and bottom-right,
// a specular line along the top, the "corners" variant with radial highlights, and the RectangularShadow
// under it. Hyprland blurs what is behind (layer rule "mist"); this only paints the glass itself.
use crate::gfx::{Canvas, Rgba};
use tiny_skia::{FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Point, RadialGradient, Shader, SpreadMode, Transform};

#[derive(Clone, Copy)]
pub struct Glass {
    pub radius: f32,
    pub tint: f32,
    pub tone: Rgba,
    pub shine: f32,
    pub shadow: bool,
    pub shadow_opacity: f32,
    pub rim_only: bool,
    pub rims: bool,
    pub corners: bool,
}
impl Default for Glass {
    fn default() -> Glass {
        Glass { radius: 26.0, tint: 0.32, tone: Rgba::rgb8(16, 22, 36, 1.0), shine: 1.0, shadow: true, shadow_opacity: 0.22, rim_only: false, rims: true, corners: false }
    }
}

// one superellipse rect as a closed subpath (LiquidGlass.sq: n = 4, corner span 1.55 r, 14 steps)
fn sq(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, rr: f32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let e = (rr * 1.55).min(w / 2.0).min(h / 2.0).max(0.01);
    let n = 14;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let q = [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    let mut first = true;
    for (k, (sx, sy)) in q.iter().enumerate() {
        let (ox, oy) = (cx + sx * (w / 2.0 - e), cy + sy * (h / 2.0 - e));
        for i in 0..=n {
            let t = (if k % 2 == 0 { i } else { n - i }) as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
            let px = ox + sx * e * t.cos().max(0.0).powf(0.5);
            let py = oy + sy * e * t.sin().max(0.0).powf(0.5);
            if first {
                pb.move_to(px, py);
                first = false;
            } else {
                pb.line_to(px, py);
            }
        }
    }
    pb.close();
}

pub fn sq_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    sq(&mut pb, x, y, w, h, r);
    pb.finish()
}
fn ring(a: (f32, f32, f32, f32, f32), b: (f32, f32, f32, f32, f32)) -> Option<Path> {
    let mut pb = PathBuilder::new();
    sq(&mut pb, a.0, a.1, a.2, a.3, a.4);
    sq(&mut pb, b.0, b.1, b.2, b.3, b.4);
    pb.finish()
}

fn col(c: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.0.clamp(0.0, 1.0), c.1.clamp(0.0, 1.0), c.2.clamp(0.0, 1.0), c.3.clamp(0.0, 1.0)).unwrap()
}
fn stops(s: &[(f32, Rgba)]) -> Vec<GradientStop> {
    s.iter().map(|(p, c)| GradientStop::new(*p, col(*c))).collect()
}
fn lin(x1: f32, y1: f32, x2: f32, y2: f32, s: &[(f32, Rgba)]) -> Option<Shader<'static>> {
    LinearGradient::new(Point::from_xy(x1, y1), Point::from_xy(x2, y2), stops(s), SpreadMode::Pad, Transform::identity())
}
fn rad(cx: f32, cy: f32, r: f32, s: &[(f32, Rgba)]) -> Option<Shader<'static>> {
    RadialGradient::new(Point::from_xy(cx, cy), Point::from_xy(cx, cy), r, stops(s), SpreadMode::Pad, Transform::identity())
}
fn fill(c: &mut Canvas, p: &Option<Path>, sh: Option<Shader>, rule: FillRule, tf: Transform) {
    let (Some(p), Some(sh)) = (p, sh) else { return };
    let paint = Paint { shader: sh, anti_alias: true, ..Default::default() };
    c.pm.fill_path(p, &paint, rule, tf, None);
}
fn solid(c: &mut Canvas, p: &Option<Path>, color: Rgba, rule: FillRule, tf: Transform) {
    let Some(p) = p else { return };
    let mut paint = Paint::default();
    paint.set_color(col(color));
    paint.anti_alias = true;
    c.pm.fill_path(p, &paint, rule, tf, None);
}

const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const LENS: Rgba = Rgba(236.0 / 255.0, 243.0 / 255.0, 1.0, 1.0);
const LENS2: Rgba = Rgba(240.0 / 255.0, 246.0 / 255.0, 1.0, 1.0);

impl Glass {
    // paint at (x, y) with size w x h
    pub fn paint(&self, c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
        let g = self;
        let r = g.radius.min(w / 2.0).min(h / 2.0);
        let tf = Transform::from_translate(x, y);
        let (dk, ck) = if g.corners { (0.0, 1.0) } else { (1.0, 0.0) };
        let sh = g.shine;
        let cr = (w.min(h) * 0.56).max(60.0);
        if g.shadow && !g.rim_only {
            shadow(c, x, y + 14.0, w, h, r, 30.0, -6.0, Rgba::rgb8(2, 5, 10, g.shadow_opacity));
        }
        if !g.rim_only {
            let body = sq_path(0.0, 0.0, w, h, r);
            solid(c, &body, g.tone.alpha(g.tint), FillRule::Winding, tf);
            fill(c, &body, lin(0.0, 0.0, 0.0, h, &[(0.0, WHITE.alpha(0.085 * sh)), (0.32, WHITE.alpha(0.025 * sh)), (0.7, WHITE.alpha(0.0)), (1.0, WHITE.alpha(0.045 * sh))]), FillRule::Winding, tf);
        }
        if !g.rims {
            return;
        }
        let eo = FillRule::EvenOdd;
        if dk > 0.0 {
            // soft bevel: 7 px inner band, diagonal light
            let bevel = ring((1.0, 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)), (7.0, 7.0, (w - 14.0).max(0.0), (h - 14.0).max(0.0), (r - 7.0).max(0.0)));
            fill(c, &bevel, lin(0.0, 0.0, w, h, &[(0.0, WHITE.alpha(0.075 * sh)), (0.35, WHITE.alpha(0.015 * sh)), (0.7, WHITE.alpha(0.0)), (1.0, WHITE.alpha(0.05 * sh))]), eo, tf);
            // lens rim
            let rim = ring((0.0, 0.0, w, h, r), (1.0, 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)));
            fill(c, &rim, lin(0.0, 0.0, w, h, &[(0.0, LENS.alpha(0.62 * sh)), (0.22, LENS.alpha(0.20 * sh)), (0.55, LENS.alpha(0.06 * sh)), (0.85, LENS.alpha(0.16 * sh)), (1.0, LENS.alpha(0.40 * sh))]), eo, tf);
            // specular line along the top inner edge + faint bottom reflection
            let spec = ring((1.0, 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)), (2.0, 2.2, w - 4.0, h - 4.2, (r - 2.0).max(0.0)));
            let a = (14.0 / h.max(1.0)).min(0.45);
            let b = (1.0 - 14.0 / h.max(1.0)).max(0.55);
            fill(c, &spec, lin(0.0, 0.0, 0.0, h, &[(0.0, WHITE.alpha(0.30 * sh)), (a, WHITE.alpha(0.0)), (b, WHITE.alpha(0.0)), (1.0, WHITE.alpha(0.10 * sh))]), eo, tf);
        }
        if ck > 0.0 {
            let rim = ring((0.0, 0.0, w, h, r), (1.0, 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)));
            solid(c, &rim, LENS.alpha(0.06 * sh), eo, tf);
            let rim12 = ring((0.0, 0.0, w, h, r), (1.2, 1.2, w - 2.4, h - 2.4, (r - 1.2).max(0.0)));
            let bev = ring((1.0, 1.0, w - 2.0, h - 2.0, (r - 1.0).max(0.0)), (6.0, 6.0, (w - 12.0).max(0.0), (h - 12.0).max(0.0), (r - 6.0).max(0.0)));
            let (tlx, tly, brx, bry) = (r * 0.6, r * 0.6, w - r * 0.6, h - r * 0.6);
            fill(c, &rim12, rad(tlx, tly, cr, &[(0.0, LENS2.alpha(0.95 * sh)), (0.45, LENS2.alpha(0.32 * sh)), (1.0, LENS2.alpha(0.0))]), eo, tf);
            fill(c, &bev, rad(tlx, tly, cr * 0.9, &[(0.0, WHITE.alpha(0.10 * sh)), (1.0, WHITE.alpha(0.0))]), eo, tf);
            fill(c, &rim12, rad(brx, bry, cr, &[(0.0, LENS2.alpha(0.62 * sh)), (0.45, LENS2.alpha(0.22 * sh)), (1.0, LENS2.alpha(0.0))]), eo, tf);
            fill(c, &bev, rad(brx, bry, cr * 0.9, &[(0.0, WHITE.alpha(0.07 * sh)), (1.0, WHITE.alpha(0.0))]), eo, tf);
        }
    }
}

// QtQuick.Effects RectangularShadow: a rounded rect grown by `spread`, blurred by `blur` px (soft edge from
// the signed distance), drawn under the glass
pub fn shadow(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, spread: f32, color: Rgba) {
    let (x, y, w, h) = (x - spread, y - spread, w + 2.0 * spread, h + 2.0 * spread);
    let r = (r + spread).max(0.0).min(w / 2.0).min(h / 2.0);
    let (pw, ph) = (c.pm.width() as i32, c.pm.height() as i32);
    let x0 = ((x - blur).floor() as i32).max(0);
    let y0 = ((y - blur).floor() as i32).max(0);
    let x1 = ((x + w + blur).ceil() as i32).min(pw);
    let y1 = ((y + h + blur).ceil() as i32).min(ph);
    let (cx, cy, hx, hy) = (x + w / 2.0, y + h / 2.0, w / 2.0 - r, h / 2.0 - r);
    let data = c.pm.data_mut();
    // the inside (d <= -blur/2: full alpha) is the shape shrunk by blur/2; its span per row is found
    // analytically and blended without the distance maths (same formula, same pixels)
    // and outside the shape grown by blur/2 nothing is drawn (k = 0): only the band between is computed
    let m = blur * 0.5;
    // half width at row offset dy of the shape grown by o (shrunk when o < 0)
    let span = |dy: f32, o: f32| -> f32 {
        let (ra, ha, hb) = (r + o, w / 2.0 + o, h / 2.0 + o);
        if dy > hb || ha <= 0.0 {
            -1.0
        } else if ra <= 0.0 || dy <= hb - ra {
            ha
        } else {
            let e = dy - (hb - ra);
            (ha - ra) + (ra * ra - e * e).max(0.0).sqrt()
        }
    };
    // pixel columns whose centre is inside [cx - half, cx + half]
    let cols = |half: f32| if half > 0.0 { (((cx - half - 0.5).ceil() as i32).max(x0), ((cx + half - 0.5).floor() as i32 + 1).min(x1)) } else { (x1, x1) };
    let a_in = color.3;
    let (cr, cg, cb, ca) = (color.0 * 255.0 * a_in, color.1 * 255.0 * a_in, color.2 * 255.0 * a_in, a_in * 255.0);
    let inv_in = 1.0 - a_in;
    let kf = |qx: f32, qy: f32| -> f32 {
        // hypot as glibc's hypotf does it (in f64), inline
        let (ex, ey) = (qx.max(0.0) as f64, qy.max(0.0) as f64);
        let d = ((ex * ex + ey * ey).sqrt() as f32) + qx.max(qy).min(0.0) - r;
        // gaussian-like falloff over [-blur/2, blur/2]
        let t = ((d + blur * 0.5) / blur).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    };
    // beside the straight sides the falloff depends on the column only (qx > 0, qy <= 0: d = qx - r) or on
    // the row only (qx <= 0, qy > 0: d = qy - r): computed once each; only the corners need the full formula
    let qxs: Vec<f32> = (x0..x1).map(|px| ((px as f32 + 0.5) - cx).abs() - hx).collect();
    let kcol: Vec<f32> = qxs.iter().map(|&qx| kf(qx, -1.0)).collect();
    for py in y0..y1 {
        let dy = ((py as f32 + 0.5) - cy).abs();
        let qy = dy - hy;
        let krow = kf(-1.0, qy);
        let (sx0, sx1) = cols(span(dy, -m) - 0.01);
        let (bx0, bx1) = cols(span(dy, m) + 0.01);
        let (bx0, bx1) = if bx0 >= bx1 { (x0, x0) } else { (bx0, bx1) };
        let row = (py * pw) as usize * 4;
        for px in sx0.max(x0)..sx1 {
            let i = row + px as usize * 4;
            data[i] = (cr + data[i] as f32 * inv_in) as u8;
            data[i + 1] = (cg + data[i + 1] as f32 * inv_in) as u8;
            data[i + 2] = (cb + data[i + 2] as f32 * inv_in) as u8;
            data[i + 3] = (ca + data[i + 3] as f32 * inv_in) as u8;
        }
        for px in bx0..bx1 {
            if px >= sx0 && px < sx1 {
                continue;
            }
            let j = (px - x0) as usize;
            let qx = qxs[j];
            let k = match (qx > 0.0, qy > 0.0) {
                (true, false) => kcol[j],
                (false, true) => krow,
                _ => kf(qx, qy),
            };
            if k <= 0.0 {
                continue;
            }
            let a = color.3 * k;
            let i = ((py * pw + px) * 4) as usize;
            let inv = 1.0 - a;
            data[i] = (color.0 * 255.0 * a + data[i] as f32 * inv) as u8;
            data[i + 1] = (color.1 * 255.0 * a + data[i + 1] as f32 * inv) as u8;
            data[i + 2] = (color.2 * 255.0 * a + data[i + 2] as f32 * inv) as u8;
            data[i + 3] = (a * 255.0 + data[i + 3] as f32 * inv) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // the per-pixel version the fast path replaced: the reference
    fn shadow_ref(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, spread: f32, color: Rgba) {
        let (x, y, w, h) = (x - spread, y - spread, w + 2.0 * spread, h + 2.0 * spread);
        let r = (r + spread).max(0.0).min(w / 2.0).min(h / 2.0);
        let (pw, ph) = (c.pm.width() as i32, c.pm.height() as i32);
        let x0 = ((x - blur).floor() as i32).max(0);
        let y0 = ((y - blur).floor() as i32).max(0);
        let x1 = ((x + w + blur).ceil() as i32).min(pw);
        let y1 = ((y + h + blur).ceil() as i32).min(ph);
        let (cx, cy, hx, hy) = (x + w / 2.0, y + h / 2.0, w / 2.0 - r, h / 2.0 - r);
        let data = c.pm.data_mut();
        for py in y0..y1 {
            for px in x0..x1 {
                let (qx, qy) = (((px as f32 + 0.5) - cx).abs() - hx, ((py as f32 + 0.5) - cy).abs() - hy);
                let d = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;
                // gaussian-like falloff over [-blur/2, blur/2]
                let t = ((d + blur * 0.5) / blur).clamp(0.0, 1.0);
                let k = 1.0 - t * t * (3.0 - 2.0 * t);
                if k <= 0.0 {
                    continue;
                }
                let a = color.3 * k;
                let i = ((py * pw + px) * 4) as usize;
                let inv = 1.0 - a;
                data[i] = (color.0 * 255.0 * a + data[i] as f32 * inv) as u8;
                data[i + 1] = (color.1 * 255.0 * a + data[i + 1] as f32 * inv) as u8;
                data[i + 2] = (color.2 * 255.0 * a + data[i + 2] as f32 * inv) as u8;
                data[i + 3] = (a * 255.0 + data[i + 3] as f32 * inv) as u8;
            }
        }
    }
    #[test]
    fn shadow_fast_inside_is_pixel_identical() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut rnd = |lo: f32, hi: f32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            lo + (seed % 10_000) as f32 / 10_000.0 * (hi - lo)
        };
        for _ in 0..400 {
            let (x, y, w, h) = (rnd(-40.0, 200.0), rnd(-40.0, 120.0), rnd(1.0, 300.0), rnd(1.0, 160.0));
            let (r, blur, spread) = (rnd(0.0, 40.0), rnd(0.5, 40.0), rnd(-8.0, 8.0));
            let col = Rgba(rnd(0.0, 1.0), rnd(0.0, 1.0), rnd(0.0, 1.0), rnd(0.05, 1.0));
            let mut a = Canvas::new(320, 200);
            a.rrect(10.0, 10.0, 200.0, 120.0, 20.0, Rgba(0.3, 0.6, 0.9, 0.7));
            let mut b = Canvas::new(320, 200);
            b.rrect(10.0, 10.0, 200.0, 120.0, 20.0, Rgba(0.3, 0.6, 0.9, 0.7));
            shadow(&mut a, x, y, w, h, r, blur, spread, col);
            shadow_ref(&mut b, x, y, w, h, r, blur, spread, col);
            assert!(a.pm.data() == b.pm.data(), "differs: {x} {y} {w} {h} r{r} blur{blur} spread{spread}");
        }
    }
}
