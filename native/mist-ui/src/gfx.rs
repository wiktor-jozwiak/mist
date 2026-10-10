// Drawing for mist-ui on the CPU: a premultiplied RGBA canvas (tiny-skia) with the few primitives the
// Mist look needs (rounded rects, gradients, text, tabler icons). Surfaces repaint fully; they are small.
use crate::text::{Style, Text};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);
impl Rgba {
    pub const fn rgb8(r: u8, g: u8, b: u8, a: f32) -> Rgba {
        Rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
    }
    pub fn hex(s: &str) -> Rgba {
        let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
        Rgba::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0)
    }
    pub fn alpha(self, k: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, self.3 * k)
    }
    pub fn mix(self, o: Rgba, t: f32) -> Rgba {
        Rgba(self.0 + (o.0 - self.0) * t, self.1 + (o.1 - self.1) * t, self.2 + (o.2 - self.2) * t, self.3 + (o.3 - self.3) * t)
    }
    fn skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba(self.0.clamp(0.0, 1.0), self.1.clamp(0.0, 1.0), self.2.clamp(0.0, 1.0), self.3.clamp(0.0, 1.0)).unwrap()
    }
}

pub struct Canvas {
    pub pm: Pixmap,
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Canvas {
        Canvas { pm: Pixmap::new(w.max(1), h.max(1)).unwrap() }
    }
    pub fn w(&self) -> f32 {
        self.pm.width() as f32
    }
    pub fn h(&self) -> f32 {
        self.pm.height() as f32
    }
    pub fn clear(&mut self) {
        self.pm.fill(tiny_skia::Color::TRANSPARENT);
    }

    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba) {
        let Some(p) = rrect_path(x, y, w, h, r) else { return };
        let mut paint = Paint::default();
        paint.set_color(c.skia());
        paint.anti_alias = true;
        self.pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // text with its line box (ascent + descent) centred on cy, starting at x; returns the advance
    pub fn text(&mut self, t: &mut Text, x: f32, cy: f32, s: &str, st: &Style, c: Rgba) -> f32 {
        let sh = t.shape(s, st);
        let base = (cy - (sh.ascent + sh.descent) / 2.0 + sh.ascent).round();
        for (k, &(id, gx)) in sh.glyphs.iter().enumerate() {
            let px = x + gx;
            let ix = px.floor();
            let sub = (((px - ix) * 4.0).round() as u8).min(3);
            let face = sh.faces.get(k).copied().unwrap_or(st.face);
            let g = t.glyph_in(face, st, id, sub);
            let (ox, oy) = (ix as i32 + g.left, base as i32 - g.top);
            if g.color {
                // colour emoji: its own colours, the text's opacity
                if let Some(pm) = tiny_skia::PixmapRef::from_bytes(&g.data, g.w, g.h) {
                    self.pm.draw_pixmap(ox, oy, pm, &tiny_skia::PixmapPaint { opacity: c.3.clamp(0.0, 1.0), ..Default::default() }, Transform::identity(), None);
                }
            } else {
                blend_alpha(&mut self.pm, ox, oy, g.w, g.h, &g.data, c);
            }
        }
        sh.width
    }
    // a tabler glyph centred on (cx, cy) (Icon.qml: Text centred both ways in its box)
    pub fn icon(&mut self, t: &mut Text, cx: f32, cy: f32, name: &str, size: f32, c: Rgba) {
        let Some(ch) = crate::icons::icon(name) else { return };
        let st = Style::icon(size);
        let s = ch.to_string();
        let w = t.width(&s, &st);
        self.text(t, cx - w / 2.0, cy, &s, &st, c);
    }
    pub fn icon_w(t: &mut Text, name: &str, size: f32) -> f32 {
        crate::icons::icon(name).map(|ch| t.width(&ch.to_string(), &Style::icon(size))).unwrap_or(0.0)
    }

    // circle / ring arc (IslandRing): from -90° clockwise over `frac` of the turn
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, frac: f32, width: f32, c: Rgba) {
        let mut pb = PathBuilder::new();
        let n = ((64.0 * frac).ceil() as i32).max(2);
        for i in 0..=n {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac * i as f32 / n as f32;
            let (px, py) = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
        }
        let Some(p) = pb.finish() else { return };
        let mut paint = Paint::default();
        paint.set_color(c.skia());
        paint.anti_alias = true;
        let st = tiny_skia::Stroke { width, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
        self.pm.stroke_path(&p, &paint, &st, Transform::identity(), None);
    }

    // BGRA (wl_shm Argb8888 little endian) copy of the premultiplied canvas
    pub fn to_bgra(&self, out: &mut [u8]) {
        for (d, s) in out.chunks_exact_mut(4).zip(self.pm.data().chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
    }
}

pub fn stroke_rrect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, col: Rgba) {
    if let Some(p) = rrect_path(x, y, w, h, r) {
        let mut paint = Paint::default();
        paint.set_color(col.skia());
        paint.anti_alias = true;
        c.pm.stroke_path(&p, &paint, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);
    }
}

pub fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r == 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    let k = 0.5522848 * r;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
    p.finish()
}

// src-over of a coverage mask in colour c (straight alpha) onto the premultiplied pixmap
pub fn blend_alpha(pm: &mut Pixmap, ox: i32, oy: i32, w: u32, h: u32, mask: &[u8], c: Rgba) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    let (cr, cg, cb, ca) = (c.0 * 255.0, c.1 * 255.0, c.2 * 255.0, c.3);
    for my in 0..h as i32 {
        let y = oy + my;
        if y < 0 || y >= ph {
            continue;
        }
        for mx in 0..w as i32 {
            let x = ox + mx;
            if x < 0 || x >= pw {
                continue;
            }
            let cov = mask[(my * w as i32 + mx) as usize] as f32 / 255.0;
            if cov <= 0.0 {
                continue;
            }
            let a = cov * ca;
            let i = ((y * pw + x) * 4) as usize;
            let inv = 1.0 - a;
            data[i] = (cr * a + data[i] as f32 * inv).round() as u8;
            data[i + 1] = (cg * a + data[i + 1] as f32 * inv).round() as u8;
            data[i + 2] = (cb * a + data[i + 2] as f32 * inv).round() as u8;
            data[i + 3] = (a * 255.0 + data[i + 3] as f32 * inv).round() as u8;
        }
    }
}

// word wrap (Text.Wrap + maximumLineCount + elide on the last line)
pub fn wrap(t: &mut Text, s: &str, st: &Style, max_w: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if t.width(&cand, st) <= max_w || cur.is_empty() {
                cur = cand;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.pop().unwrap_or_default();
        lines.push(crate::bar::elide(t, &(last + " …"), st, max_w));
    }
    lines.into_iter().map(|l| if t.width(&l, st) > max_w { crate::bar::elide(t, &l, st, max_w) } else { l }).collect()
}
