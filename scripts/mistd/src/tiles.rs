// Dock icon tiles, the same look as the Windows Mist dock (seelen-widgets/tools/make_tiles.py):
// every app becomes one iOS squircle tile with a top sheen and a hairline rim.
//   * full-bleed artwork (its own square / rounded background) fills the tile;
//   * shaped icons sit at ~66 % on a soft dark gradient tinted with the icon's dominant colour;
//   * glyph tiles for the dock's own buttons: "@start" (3x3 grid), "@trash", "@trash-full".
// Usage: mistd tiles <icon name | absolute path | @glyph> ...   -> ~/.cache/mist/tiles/<key>.png
// The source is the app's original icon (hicolor / pixmaps), then Papirus, then anything else installed.
use crate::common::home;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use resvg::tiny_skia::{self, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use std::path::{Path, PathBuf};

const S: u32 = 160; // tile size (px), shown at 46 px
const SS: u32 = 4; // supersampling for the drawn shapes
const THEMES: [&str; 8] = ["hicolor", "Papirus-Dark", "Papirus", "breeze-dark", "breeze", "Adwaita", "MacTahoe-dark", "MacTahoe"];

/// file name for an icon name or a path (same rule as Tiles.qml key()): anything but [A-Za-z0-9._-@] becomes "_"
fn key_of(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || "._-@".contains(c) { c } else { '_' }).collect()
}

// ------------------------------------------------------------------ lookup
/// sort key for an icon directory: scalable best, then the largest bitmap
fn size_of(d: &Path) -> i32 {
    let s = d.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    match s.as_str() {
        "scalable" => 10000,
        "symbolic" => -1,
        // "24x24@3x" / "24@3x": 72 real pixels
        _ => {
            let (base, scale) = s.split_once('@').unwrap_or((&s, "1x"));
            let n: i32 = base.split('x').next().and_then(|x| x.parse().ok()).unwrap_or(0);
            n * scale.trim_end_matches('x').parse::<i32>().unwrap_or(1)
        }
    }
}

fn walk(dir: &Path, prefix: &str, found: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() {
            walk(&p, prefix, found);
        } else if e.file_name().to_string_lossy().starts_with(prefix) {
            found.push(p);
        }
    }
}

fn find_icon(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Path::new(name).exists().then(|| PathBuf::from(name));
    }
    let h = home();
    let dirs = [PathBuf::from(&h).join(".local/share/icons"), PathBuf::from("/usr/share/icons")];
    let prefix = format!("{}.", name);
    let mut best: Option<(i32, PathBuf)> = None;
    for theme in THEMES {
        for base in &dirs {
            let root = base.join(theme);
            if !root.is_dir() {
                continue;
            }
            let mut found = vec![];
            walk(&root, &prefix, &mut found);
            for f in found {
                let ext = f.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                let fs = f.to_string_lossy();
                if (ext != "png" && ext != "svg") || fs.contains("symbolic") || fs.contains("/actions/") {
                    continue;
                }
                let par = f.parent().unwrap();
                let pn = par.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                let sz = if ["apps", "places", "devices"].contains(&pn.as_str()) { size_of(par.parent().unwrap_or(par)) } else { size_of(par) };
                // the same size twice (16 / 16@1x links): the first path in name order, whatever the disk order
                if best.as_ref().is_none_or(|b| sz > b.0 || (sz == b.0 && f < b.1)) {
                    best = Some((sz, f));
                }
            }
        }
        if best.as_ref().is_some_and(|b| b.0 >= 128) {
            return best.map(|b| b.1);
        }
    }
    if best.is_some() {
        return best.map(|b| b.1);
    }
    for p in ["/usr/share/pixmaps".to_string(), format!("{}/.local/share/pixmaps", h)] {
        for ext in ["svg", "png"] {
            let f = PathBuf::from(&p).join(format!("{}.{}", name, ext));
            if f.exists() {
                return Some(f);
            }
        }
    }
    None
}

fn from_pixmap(p: &Pixmap) -> RgbaImage {
    let mut im = RgbaImage::new(p.width(), p.height());
    for (i, px) in p.pixels().iter().enumerate() {
        let c = px.demultiply();
        im.put_pixel(i as u32 % p.width(), i as u32 / p.width(), Rgba([c.red(), c.green(), c.blue(), c.alpha()]));
    }
    im
}

fn load(path: &Path) -> Result<RgbaImage, String> {
    if path.extension().is_some_and(|e| e == "svg") {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut opt = resvg::usvg::Options::default();
        opt.resources_dir = path.parent().map(|p| p.to_path_buf());
        let tree = resvg::usvg::Tree::from_data(&data, &opt).map_err(|e| e.to_string())?;
        let sz = tree.size();
        let scale = (256.0 / sz.width()).min(256.0 / sz.height());
        let (w, h) = ((sz.width() * scale).round().max(1.0) as u32, (sz.height() * scale).round().max(1.0) as u32);
        let mut pm = Pixmap::new(w, h).ok_or("pixmap")?;
        resvg::render(&tree, Transform::from_scale(scale, scale), &mut pm.as_mut());
        return Ok(from_pixmap(&pm));
    }
    let im = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    Ok(if im.width().max(im.height()) < 256 { resize(&im, 256, 256) } else { im })
}

// ------------------------------------------------------------------ pixels
/// Lanczos resize on premultiplied colour (no dark fringes from transparent pixels).
fn resize(im: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut pre = image::Rgba32FImage::new(im.width(), im.height());
    for (x, y, p) in im.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        pre.put_pixel(x, y, Rgba([p[0] as f32 / 255.0 * a, p[1] as f32 / 255.0 * a, p[2] as f32 / 255.0 * a, a]));
    }
    let r = image::imageops::resize(&pre, w.max(1), h.max(1), FilterType::Lanczos3);
    let mut out = RgbaImage::new(r.width(), r.height());
    for (x, y, p) in r.enumerate_pixels() {
        let a = p[3].clamp(0.0, 1.0);
        let k = if a > 0.0 { 255.0 / a } else { 0.0 };
        let c = |v: f32| (v * k).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([c(p[0]), c(p[1]), c(p[2]), (a * 255.0).round() as u8]));
    }
    out
}

/// `src` over `dst` at (x, y), clipped (non-premultiplied, like PIL alpha_composite).
fn over(dst: &mut RgbaImage, src: &RgbaImage, x: i32, y: i32) {
    for (sx, sy, s) in src.enumerate_pixels() {
        let (dx, dy) = (sx as i32 + x, sy as i32 + y);
        if dx < 0 || dy < 0 || dx >= dst.width() as i32 || dy >= dst.height() as i32 {
            continue;
        }
        let d = dst.get_pixel_mut(dx as u32, dy as u32);
        let sa = s[3] as f32 / 255.0;
        if sa <= 0.0 {
            continue;
        }
        let da = d[3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        for i in 0..3 {
            d[i] = ((s[i] as f32 * sa + d[i] as f32 * da * (1.0 - sa)) / oa).round().clamp(0.0, 255.0) as u8;
        }
        d[3] = (oa * 255.0).round() as u8;
    }
}

fn bbox(im: &RgbaImage, thr: u8) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in im.enumerate_pixels() {
        if p[3] > thr {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x0 != u32::MAX).then_some((x0, y0, x1, y1))
}

fn crop(im: &RgbaImage, b: (u32, u32, u32, u32)) -> RgbaImage {
    image::imageops::crop_imm(im, b.0, b.1, b.2 - b.0, b.3 - b.1).to_image()
}

fn squircle_mask() -> Vec<u8> {
    let big = (S * SS) as f64;
    let c = (big - 1.0) / 2.0;
    let mut m = vec![0u8; (S * S) as usize];
    for y in 0..S {
        for x in 0..S {
            let mut acc = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let (bx, by) = ((x * SS + sx) as f64, (y * SS + sy) as f64);
                    let d = ((bx - c) / c).abs().powf(5.0) + ((by - c) / c).abs().powf(5.0);
                    acc += ((1.0 - d) * big * 0.6).clamp(0.0, 1.0);
                }
            }
            m[(y * S + x) as usize] = (acc / (SS * SS) as f64 * 255.0).round() as u8;
        }
    }
    m
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (a[i] as f64 * (1.0 - t) + b[i] as f64 * t).round() as u8)
}

fn dominant_color(im: &RgbaImage) -> [u8; 3] {
    let a: Vec<[f64; 3]> = im.pixels().filter(|p| p[3] > 200).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    if a.is_empty() {
        return [120, 140, 170];
    }
    let sub: Vec<[f64; 3]> = a.iter().step_by((a.len() / 4000).max(1)).copied().collect();
    // saturation * value of HSV = (max - min) / 255
    let sat: Vec<f64> = sub.iter().map(|p| (p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])) / 255.0).collect();
    let mut sorted = sat.clone();
    sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let pos = 0.6 * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    let pct = sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64);
    let pick: Vec<&[f64; 3]> = sub.iter().zip(&sat).filter(|(_, s)| **s > pct).map(|(p, _)| p).collect();
    let src: Vec<&[f64; 3]> = if pick.is_empty() { a.iter().collect() } else { pick };
    let n = src.len() as f64;
    [0, 1, 2].map(|i| (src.iter().map(|p| p[i]).sum::<f64>() / n) as u8)
}

fn vertical_gradient(top: [u8; 3], bottom: [u8; 3]) -> RgbaImage {
    RgbaImage::from_fn(S, S, |_, y| {
        let t = y as f64 / (S - 1) as f64;
        let c = [0, 1, 2].map(|i| (top[i] as f64 * (1.0 - t) + bottom[i] as f64 * t) as u8);
        Rgba([c[0], c[1], c[2], 255])
    })
}

fn is_full_bleed(im: &RgbaImage) -> bool {
    // judged on the artwork itself (padding trimmed): themed squircle icons count as full-bleed too
    let Some(b) = bbox(im, 30) else { return false };
    let (w, h) = ((b.2 - b.0) as f64, (b.3 - b.1) as f64);
    if w.min(h) < 0.8 * w.max(h) {
        return false;
    }
    let c = crop(im, b);
    let solid = c.pixels().filter(|p| p[3] > 200).count();
    solid as f64 / (c.width() * c.height()) as f64 > 0.72
}

fn edge_color(im: &RgbaImage) -> [u8; 3] {
    let (w, h) = (im.width(), im.height());
    let (w8, h8) = (w / 8, h / 8);
    let (w8c, h8c) = (w.div_ceil(8), h.div_ceil(8));
    let mut band = vec![];
    for x in 0..w8 {
        band.push(*im.get_pixel(x, h / 2));
    }
    for x in w - w8c..w {
        band.push(*im.get_pixel(x, h / 2));
    }
    for y in 0..h8 {
        band.push(*im.get_pixel(w / 2, y));
    }
    for y in h - h8c..h {
        band.push(*im.get_pixel(w / 2, y));
    }
    let b: Vec<&Rgba<u8>> = band.iter().filter(|p| p[3] > 200).collect();
    if b.is_empty() {
        return [30, 34, 44];
    }
    let n = b.len() as f64;
    [0, 1, 2].map(|i| (b.iter().map(|p| p[i] as f64).sum::<f64>() / n) as u8)
}

/// A layer drawn at SS x the tile size with anti-aliasing, box-downsampled to the tile.
fn layer(draw: impl Fn(&mut Pixmap)) -> RgbaImage {
    let mut pm = Pixmap::new(S * SS, S * SS).unwrap();
    draw(&mut pm);
    let big = pm.data();
    let mut out = RgbaImage::new(S, S);
    for y in 0..S {
        for x in 0..S {
            let mut acc = [0f64; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (((y * SS + sy) * S * SS + x * SS + sx) * 4) as usize;
                    for k in 0..4 {
                        acc[k] += big[i + k] as f64;
                    }
                }
            }
            let n = (SS * SS) as f64;
            let a = acc[3] / n;
            let c = |v: f64| if a > 0.0 { (v / n * 255.0 / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            out.put_pixel(x, y, Rgba([c(acc[0]), c(acc[1]), c(acc[2]), a.round() as u8]));
        }
    }
    out
}

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

fn rrect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> tiny_skia::Path {
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let k = 0.5523 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    pb.close();
    pb.finish().unwrap()
}

/// PIL-style outline: a band of `w` px inside the box (x0, y0)-(x1, y1) (inclusive pixel coordinates).
fn stroke_inside(pm: &mut Pixmap, b: [f32; 4], r: f32, w: f32, c: [u8; 4]) {
    let h = w / 2.0;
    let p = rrect(b[0] + h, b[1] + h, b[2] + 1.0 - h, b[3] + 1.0 - h, (r - h).max(0.0));
    pm.stroke_path(&p, &paint(c), &Stroke { width: w, ..Default::default() }, Transform::identity(), None);
}

fn fill_rr(pm: &mut Pixmap, b: [f32; 4], r: f32, c: [u8; 4]) {
    pm.fill_path(&rrect(b[0], b[1], b[2] + 1.0, b[3] + 1.0, r), &paint(c), FillRule::Winding, Transform::identity(), None);
}

/// squircle clip + top sheen + hairline rim
fn finish(mut tile: RgbaImage, mask: &[u8]) -> RgbaImage {
    let half = S / 2;
    let sheen = RgbaImage::from_fn(S, S, |_, y| {
        let a = if y < half { (34.0 - 34.0 * y as f64 / (half - 1) as f64) as u8 } else { 0 };
        Rgba([255, 255, 255, a])
    });
    over(&mut tile, &sheen, 0, 0);
    let k = SS as f32;
    let big = (S * SS) as f32;
    let rim = layer(|pm| stroke_inside(pm, [k, k, big - k - 1.0, big - k - 1.0], (big * 0.27).floor(), k * 2.0, [255, 255, 255, 40]));
    over(&mut tile, &rim, 0, 0);
    for (i, p) in tile.pixels_mut().enumerate() {
        p[3] = ((p[3] as u32 * mask[i] as u32 + 127) / 255) as u8;
        if p[3] == 0 {
            *p = Rgba([0, 0, 0, 0]);
        }
    }
    tile
}

fn make_tile(im: &RgbaImage, mask: &[u8]) -> RgbaImage {
    if is_full_bleed(im) {
        let b = bbox(im, 200).unwrap_or((0, 0, im.width(), im.height()));
        let art = crop(im, b);
        let (w, h) = (art.width(), art.height());
        let a = |x: u32, y: u32| art.get_pixel(x.min(w - 1), y.min(h - 1))[3];
        let square = w > 4 && h > 4 && [a(2, 2), a(w - 3, 2), a(2, h - 3), a(w - 3, h - 3)].iter().min().copied().unwrap_or(0) > 200;
        // rounded artwork: bleed past its own corners; hard-square artwork: shrink so nothing is cut
        let size = (S as f64 * if square { 0.9 } else { 1.06 }) as u32;
        let art = resize(&art, size, size);
        let e = edge_color(&art);
        let mut tile = RgbaImage::from_pixel(S, S, Rgba([e[0], e[1], e[2], 255]));
        let off = (S as i32 - size as i32).div_euclid(2);
        over(&mut tile, &art, off, off);
        return finish(tile, mask);
    }
    let dom = dominant_color(im);
    let mut tile = vertical_gradient(mix(dom, [40, 48, 64], 0.72), mix(dom, [16, 20, 28], 0.86));
    let art = crop(im, bbox(im, 0).unwrap_or((0, 0, im.width(), im.height())));
    let target = (S as f64 * 0.66) as u32;
    let (w, h) = (art.width(), art.height());
    let art = if w > target || h > target {
        let k = (target as f64 / w as f64).min(target as f64 / h as f64);
        resize(&art, ((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1))
    } else {
        art
    };
    let (ox, oy) = ((S as i32 - art.width() as i32).div_euclid(2), (S as i32 - art.height() as i32).div_euclid(2));
    let mut shadow = RgbaImage::new(S, S);
    let sh = RgbaImage::from_fn(art.width(), art.height(), |x, y| Rgba([0, 0, 0, (art.get_pixel(x, y)[3] as f64 * 0.45) as u8]));
    over(&mut shadow, &sh, ox, oy + 4);
    let shadow = image::imageops::blur(&shadow, 5.0);
    over(&mut tile, &shadow, 0, 0);
    over(&mut tile, &art, ox, oy);
    finish(tile, mask)
}

// ------------------------------------------------------------------ glyph tiles
fn glyph_tile(name: &str, mask: &[u8]) -> Option<RgbaImage> {
    let k = SS as f32;
    let w = [255, 255, 255, 235];
    let shape = |pm: &mut Pixmap| match name {
        "@start" => {
            for r in 0..3 {
                for c in 0..3 {
                    let (x, y) = ((47 + c * 25) as f32, (47 + r * 25) as f32);
                    fill_rr(pm, [x * k, y * k, (x + 16.0) * k, (y + 16.0) * k], 5.0 * k, w);
                }
            }
        }
        _ => {
            let full = name == "@trash-full";
            fill_rr(pm, [44.0 * k, 50.0 * k, 116.0 * k, 57.0 * k], 3.0 * k, w);
            stroke_inside(pm, [68.0 * k, 40.0 * k, 92.0 * k, 50.0 * k], 3.0 * k, 5.0 * k, w);
            let mut pb = PathBuilder::new();
            let pts = [(52.0, 62.0), (108.0, 62.0), (102.0, 120.0), (58.0, 120.0)];
            pb.move_to(pts[0].0 * k, pts[0].1 * k);
            for p in &pts[1..] {
                pb.line_to(p.0 * k, p.1 * k);
            }
            pb.close();
            // PIL draws a polygon outline inside the shape: a double-width stroke clipped to the polygon
            let path = pb.finish().unwrap();
            let mut clip = tiny_skia::Mask::new(pm.width(), pm.height()).unwrap();
            clip.fill_path(&path, FillRule::Winding, true, Transform::identity());
            let st = Stroke { width: 14.0 * k, line_join: tiny_skia::LineJoin::Miter, ..Default::default() };
            pm.stroke_path(&path, &paint(w), &st, Transform::identity(), Some(&clip));
            for x in [70.0f32, 80.0, 90.0] {
                let mut pb = PathBuilder::new();
                pb.move_to(x * k, 74.0 * k);
                pb.line_to(x * k, 108.0 * k);
                let c = if full { w } else { [255, 255, 255, 150] };
                pm.stroke_path(&pb.finish().unwrap(), &paint(c), &Stroke { width: 5.0 * k, ..Default::default() }, Transform::identity(), None);
            }
            if full {
                let r = Rect::from_ltrb(104.0 * k, 34.0 * k, 125.0 * k, 55.0 * k).unwrap();
                let p = PathBuilder::from_oval(r).unwrap();
                pm.fill_path(&p, &paint([255, 69, 58, 255]), FillRule::Winding, Transform::identity(), None);
            }
        }
    };
    if !["@start", "@trash", "@trash-full"].contains(&name) {
        return None;
    }
    let mut tile = vertical_gradient([64, 74, 94], [30, 36, 48]);
    over(&mut tile, &layer(shape), 0, 0);
    Some(finish(tile, mask))
}

pub fn main(names: &[String]) {
    let h = home();
    let out = PathBuf::from(&h).join(".cache/mist/tiles");
    let win = PathBuf::from(&h).join(".local/share/mist/wintiles");
    let _ = std::fs::create_dir_all(&out);
    let mask = squircle_mask();
    for name in names {
        let dst = out.join(format!("{}.png", key_of(name)));
        // the exact tiles from the Windows Seelen pack, used as they are when present
        let w = win.join(format!("{}.png", key_of(name)));
        if w.exists() {
            if std::fs::copy(&w, &dst).is_ok() {
                println!("{}", dst.display());
            }
            continue;
        }
        let tile = if name.starts_with('@') {
            match glyph_tile(name, &mask) {
                Some(t) => t,
                None => {
                    eprintln!("no glyph: {}", name);
                    continue;
                }
            }
        } else {
            let Some(src) = find_icon(name) else {
                eprintln!("no icon: {}", name);
                continue;
            };
            match load(&src) {
                Ok(im) => make_tile(&im, &mask),
                Err(e) => {
                    eprintln!("{}: {}", name, e);
                    continue;
                }
            }
        };
        let tmp = out.join(format!("{}.tmp.png", key_of(name)));
        if tile.save_with_format(&tmp, image::ImageFormat::Png).is_ok() && std::fs::rename(&tmp, &dst).is_ok() {
            println!("{}", dst.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_sizes() {
        assert_eq!(key_of("org.kde.dolphin"), "org.kde.dolphin");
        assert_eq!(key_of("/usr/share/x y.png"), "_usr_share_x_y.png");
        assert_eq!(key_of("@trash-full"), "@trash-full");
        assert_eq!(size_of(Path::new("/i/48x48")), 48);
        assert_eq!(size_of(Path::new("/i/24x24@2x")), 48);
        assert_eq!(size_of(Path::new("/i/24@3x")), 72);
        assert_eq!(size_of(Path::new("/i/scalable")), 10000);
        assert_eq!(size_of(Path::new("/i/symbolic")), -1);
        assert_eq!(size_of(Path::new("/i/apps")), 0);
    }

    #[test]
    fn tile_shape() {
        let mask = squircle_mask();
        assert_eq!(mask[0], 0); // corners are cut
        assert_eq!(mask[(80 * S + 80) as usize], 255); // the middle is solid
        assert_eq!(mask[(80 * S) as usize] > 0, true); // the edge middle is (almost) solid
        for g in ["@start", "@trash", "@trash-full"] {
            let t = glyph_tile(g, &mask).unwrap();
            assert_eq!((t.width(), t.height()), (S, S));
            assert_eq!(t.get_pixel(0, 0)[3], 0);
            assert_eq!(t.get_pixel(80, 140)[3], 255);
        }
        assert!(glyph_tile("@nope", &mask).is_none());
    }

    #[test]
    fn tiles_from_synthetic_icons() {
        let mask = squircle_mask();
        // a tall red bar on transparent: shaped icon -> gradient tile with the icon centred
        let circle = RgbaImage::from_fn(256, 256, |x, y| {
            if (98..158).contains(&x) && (28..228).contains(&y) { Rgba([230, 30, 30, 255]) } else { Rgba([0, 0, 0, 0]) }
        });
        // a round icon counts as full-bleed artwork (its own background), like the Python version
        let disc = RgbaImage::from_fn(256, 256, |x, y| {
            let (dx, dy) = (x as f64 - 128.0, y as f64 - 128.0);
            if dx * dx + dy * dy < 100.0 * 100.0 { Rgba([230, 30, 30, 255]) } else { Rgba([0, 0, 0, 0]) }
        });
        assert!(is_full_bleed(&disc));
        assert!(!is_full_bleed(&circle));
        let t = make_tile(&circle, &mask);
        let c = t.get_pixel(80, 80);
        assert!(c[0] > 200 && c[1] < 60, "icon in the middle: {:?}", c);
        let bg = t.get_pixel(80, 8);
        assert!(bg[3] == 255 && bg[0] > bg[2], "red-tinted background: {:?}", bg);
        // an opaque blue square: full-bleed -> fills the tile
        let sq = RgbaImage::from_pixel(256, 256, Rgba([20, 90, 220, 255]));
        assert!(is_full_bleed(&sq));
        let t = make_tile(&sq, &mask);
        let p = t.get_pixel(20, 80);
        assert!(p[2] > 180 && p[3] == 255, "{:?}", p);
        assert_eq!(dominant_color(&sq), [20, 90, 220]);
        assert_eq!(dominant_color(&RgbaImage::new(4, 4)), [120, 140, 170]);
    }

    #[test]
    fn resize_has_no_dark_fringe() {
        // white opaque left half, transparent black right half: no grey after downscaling
        let im = RgbaImage::from_fn(64, 64, |x, _| if x < 32 { Rgba([255, 255, 255, 255]) } else { Rgba([0, 0, 0, 0]) });
        let r = resize(&im, 16, 16);
        for p in r.pixels().filter(|p| p[3] > 20) {
            assert!(p[0] > 240, "{:?}", p);
        }
    }
}
