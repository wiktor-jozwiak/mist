// Images for mist-ui: any file (PNG / JPEG / WebP / SVG) or a theme icon name, decoded once per size into a
// premultiplied pixmap (small LRU), drawn cropped or fitted into a rounded rect (ClippingRectangle).
use crate::apps::Icons;
use crate::gfx::Canvas;
use std::collections::HashMap;
use std::rc::Rc;
use tiny_skia::{FillRule, FilterQuality, Paint, Pattern, Pixmap, SpreadMode, Transform};

pub struct Images {
    cache: HashMap<(String, u32), Option<Rc<Pixmap>>>,
    order: Vec<(String, u32)>,
}

fn decode_file(path: &str, max: u32) -> Option<Pixmap> {
    if path.ends_with(".svg") {
        return crate::apps::decode(path, max).and_then(|i| Pixmap::from_vec(i.rgba, tiny_skia::IntSize::from_wh(i.w, i.h)?));
    }
    let im = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    // sourceSize: decode at most `max` px on the long side
    let im = if im.width().max(im.height()) > max { im.thumbnail(max, max) } else { im };
    let rgba = im.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut data = rgba.into_raw();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (px[0] as u32 * a / 255) as u8;
        px[1] = (px[1] as u32 * a / 255) as u8;
        px[2] = (px[2] as u32 * a / 255) as u8;
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

impl Images {
    pub fn new() -> Images {
        Images { cache: HashMap::new(), order: Vec::new() }
    }
    // `src`: a path, file:// url, or an icon name (theme)
    pub fn get(&mut self, src: &str, max: u32, icons: &mut Icons) -> Option<Rc<Pixmap>> {
        if src.is_empty() {
            return None;
        }
        let key = (src.to_string(), max);
        if let Some(v) = self.cache.get(&key) {
            return v.clone();
        }
        let path = src.strip_prefix("file://").unwrap_or(src);
        let pm = if path.starts_with('/') {
            decode_file(path, max)
        } else {
            icons.image(path, max).and_then(|i| Pixmap::from_vec(i.rgba.clone(), tiny_skia::IntSize::from_wh(i.w, i.h)?))
        };
        let v = pm.map(Rc::new);
        self.cache.insert(key.clone(), v.clone());
        self.order.push(key);
        if self.order.len() > 48 {
            let k = self.order.remove(0);
            self.cache.remove(&k);
        }
        v
    }
    pub fn forget(&mut self, src: &str) {
        self.cache.retain(|k, _| k.0 != src);
        self.order.retain(|k| k.0 != src);
    }
}

// draw `pm` into (x, y, w, h) with corner radius r; crop = PreserveAspectCrop, else PreserveAspectFit
pub fn draw(c: &mut Canvas, pm: &Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, crop: bool, opacity: f32) {
    let (iw, ih) = (pm.width() as f32, pm.height() as f32);
    let k = if crop { (w / iw).max(h / ih) } else { (w / iw).min(h / ih) };
    let (dw, dh) = (iw * k, ih * k);
    let (ox, oy) = (x + (w - dw) / 2.0, y + (h - dh) / 2.0);
    let tf = Transform::from_row(k, 0.0, 0.0, k, ox, oy);
    let shader = Pattern::new(pm.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, opacity, tf);
    let paint = Paint { shader, anti_alias: true, ..Default::default() };
    let (cx, cy, cw, ch) = if crop { (x, y, w, h) } else { (ox, oy, dw, dh) };
    if let Some(p) = crate::gfx::rrect_path(cx, cy, cw, ch, r) {
        c.pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }
}
