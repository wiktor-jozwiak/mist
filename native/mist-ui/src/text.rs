// Text for mist-ui: SF Pro (variable: wght + opsz, like Txt.qml) and the tabler icon font, shaped and
// rasterised by swash on the CPU. Font files are mmap'ed (only the touched pages cost RAM), glyph bitmaps
// are cached per (font, glyph, size, weight, subpixel offset).
use std::collections::HashMap;
use swash::scale::{Render, ScaleContext, Source};
use swash::shape::ShapeContext;
use swash::zeno::{Format, Vector};
use swash::FontRef;

pub struct Mapped {
    ptr: *const u8,
    len: usize,
}
impl Mapped {
    pub fn open(path: &str) -> Option<Mapped> {
        use std::os::fd::AsRawFd;
        let f = std::fs::File::open(path).ok()?;
        let len = f.metadata().ok()?.len() as usize;
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, f.as_raw_fd(), 0) };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        Some(Mapped { ptr: ptr as *const u8, len })
    }
    pub fn bytes(&self) -> &'static [u8] {
        // the mapping lives for the whole process
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    Ui,
    Icons,
    // a fallback font for characters SF Pro lacks (FALLBACKS index)
    Fb(u8),
}

// what Qt's font fallback would find on this system: colour emoji, symbols, Latin / Greek / Cyrillic, CJK
pub const FALLBACKS: [&str; 6] = [
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
];

#[derive(Clone, Copy)]
pub struct Style {
    pub face: Face,
    pub size: f32,
    pub weight: f32,
    pub tnum: bool,
    pub spacing: f32, // font.letterSpacing (px)
}
impl Style {
    pub fn ui(size: f32, weight: f32) -> Style {
        Style { face: Face::Ui, size, weight, tnum: false, spacing: 0.0 }
    }
    pub fn icon(size: f32) -> Style {
        Style { face: Face::Icons, size, weight: 400.0, tnum: false, spacing: 0.0 }
    }
    pub fn spaced(mut self, s: f32) -> Style {
        self.spacing = s;
        self
    }
    pub fn tnum(mut self) -> Style {
        self.tnum = true;
        self
    }
}

pub struct Glyph {
    pub left: i32,
    pub top: i32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    pub color: bool, // data is premultiplied RGBA (colour emoji), else coverage
}

pub struct Shaped {
    pub glyphs: Vec<(u16, f32)>, // glyph id, pen x
    pub faces: Vec<Face>,        // the font of each glyph
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

pub struct Text {
    ui: &'static [u8],
    icons: &'static [u8],
    fb: Vec<Option<&'static [u8]>>, // mapped on first use
    fb_tried: Vec<bool>,
    scale: ScaleContext,
    shape: ShapeContext,
    cache: HashMap<(Face, u16, u32, u32, u8), Glyph>,
    shaped: HashMap<(String, Face, u32, u32, bool), std::rc::Rc<Shaped>>,
}

pub const UI_FONT: &str = "/home/wiktor/.local/share/fonts/SF-Pro/SF-Pro.ttf";
pub const ICON_FONT: &str = "/usr/share/noctalia/assets/fonts/noctalia-tabler.ttf";

impl Text {
    pub fn new() -> Text {
        let home = std::env::var("HOME").unwrap_or_default();
        let ui_path = UI_FONT.replace("/home/wiktor", &home);
        let ui = Mapped::open(&ui_path).expect("SF Pro font");
        let icons = Mapped::open(ICON_FONT).expect("tabler icon font");
        let (ui, icons) = (ui.bytes(), icons.bytes());
        Text { ui, icons, fb: vec![None; FALLBACKS.len()], fb_tried: vec![false; FALLBACKS.len()], scale: ScaleContext::new(), shape: ShapeContext::new(), cache: HashMap::new(), shaped: HashMap::new() }
    }
    fn font(&self, f: Face) -> FontRef<'static> {
        match f {
            Face::Ui => FontRef::from_index(self.ui, 0).unwrap(),
            Face::Icons => FontRef::from_index(self.icons, 0).unwrap(),
            Face::Fb(i) => FontRef::from_index(self.fb[i as usize].unwrap_or(self.ui), 0).unwrap_or(FontRef::from_index(self.ui, 0).unwrap()),
        }
    }
    fn fallback(&mut self, i: usize) -> Option<FontRef<'static>> {
        if !self.fb_tried[i] {
            self.fb_tried[i] = true;
            self.fb[i] = Mapped::open(FALLBACKS[i]).map(|m| m.bytes());
        }
        self.fb[i].and_then(|b| FontRef::from_index(b, 0))
    }
    // the face that draws `ch`: SF Pro, else the first fallback that has it
    fn face_for(&mut self, base: Face, ch: char) -> Face {
        if base == Face::Icons || ch.is_control() || ch == ' ' {
            return base;
        }
        if self.font(Face::Ui).charmap().map(ch) != 0 {
            return Face::Ui;
        }
        // variation selectors / joiners stay with what precedes them
        if matches!(ch as u32, 0xFE00..=0xFE0F | 0x200D | 0x20E3 | 0x1F3FB..=0x1F3FF) {
            return Face::Fb(0);
        }
        for i in 0..FALLBACKS.len() {
            if let Some(f) = self.fallback(i) {
                if f.charmap().map(ch) != 0 {
                    return Face::Fb(i as u8);
                }
            }
        }
        Face::Ui
    }
    // Txt.qml: wght = w, opsz = clamp(size, 8, 36)
    fn vars(st: &Style) -> [(&'static str, f32); 2] {
        [("wght", st.weight), ("opsz", st.size.clamp(8.0, 36.0))]
    }

    pub fn shape(&mut self, s: &str, st: &Style) -> std::rc::Rc<Shaped> {
        let key = (s.to_string(), st.face, st.size.to_bits(), st.weight.to_bits() ^ st.spacing.to_bits().rotate_left(7), st.tnum);
        if let Some(r) = self.shaped.get(&key) {
            return r.clone();
        }
        if self.shaped.len() > 2000 {
            self.shaped.clear();
        }
        let vars = Self::vars(st);
        // runs of one face each
        let mut runs: Vec<(Face, String)> = Vec::new();
        let mut prev = st.face;
        for ch in s.chars() {
            let mut f = self.face_for(st.face, ch);
            // a joiner / selector keeps the run it is in
            if matches!(ch as u32, 0xFE00..=0xFE0F | 0x200D | 0x20E3 | 0x1F3FB..=0x1F3FF) && !runs.is_empty() {
                f = prev;
            }
            match runs.last_mut() {
                Some((rf, rs)) if *rf == f => rs.push(ch),
                _ => runs.push((f, ch.to_string())),
            }
            prev = f;
        }
        let mut glyphs = Vec::new();
        let mut faces = Vec::new();
        let mut x = 0.0;
        let tn = [("tnum", 1u16)];
        for (f, rs) in &runs {
            let font = self.font(*f);
            let mut b = self.shape.builder(font).size(st.size);
            if *f == Face::Ui {
                b = b.variations(vars.iter().copied());
            }
            if st.tnum && *f == Face::Ui {
                b = b.features(tn.iter().copied());
            }
            let mut sh = b.build();
            sh.add_str(rs);
            sh.shape_with(|c| {
                for g in c.glyphs {
                    glyphs.push((g.id, x + g.x));
                    faces.push(*f);
                    x += g.advance + st.spacing;
                }
            });
        }
        let font = self.font(if st.face == Face::Icons { Face::Icons } else { Face::Ui });
        let coords: Vec<i16> = if st.face == Face::Ui { font.variations().normalized_coords(vars.iter().copied()).collect() } else { Vec::new() };
        let m = font.metrics(&coords).scale(st.size);
        let r = std::rc::Rc::new(Shaped { glyphs, faces, width: x, ascent: m.ascent, descent: m.descent });
        self.shaped.insert(key, r.clone());
        r
    }

    pub fn width(&mut self, s: &str, st: &Style) -> f32 {
        self.shape(s, st).width
    }

    // the coverage bitmap of one glyph at a quarter-pixel horizontal offset
    pub fn glyph(&mut self, st: &Style, id: u16, sub: u8) -> &Glyph {
        self.glyph_in(st.face, st, id, sub)
    }
    pub fn glyph_in(&mut self, face: Face, st: &Style, id: u16, sub: u8) -> &Glyph {
        let key = (face, id, st.size.to_bits(), st.weight.to_bits(), sub);
        if !self.cache.contains_key(&key) {
            if self.cache.len() > 4000 {
                self.cache.clear();
            }
            let font = self.font(face);
            let vars = Self::vars(st);
            let mut b = self.scale.builder(font).size(st.size).hint(false);
            if face == Face::Ui {
                b = b.variations(vars.iter().copied());
            }
            let mut sc = b.build();
            let g = if face == Face::Fb(0) {
                // colour emoji (CBDT bitmaps): RGBA, scaled to the text size
                let img = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(swash::scale::StrikeWith::BestFit), Source::Outline]).format(Format::CustomSubpixel([0.0, 0.0, 0.0])).render(&mut sc, id);
                match img {
                    Some(i) if i.content == swash::scale::image::Content::Color => {
                        let (w, h) = (i.placement.width, i.placement.height);
                        let want = st.size * 1.17;
                        let k = if h as f32 > want * 1.2 { want / h as f32 } else { 1.0 };
                        let mut data = i.data.clone();
                        for p in data.chunks_exact_mut(4) {
                            let a = p[3] as u32;
                            for c in 0..3 {
                                p[c] = (p[c] as u32 * a / 255) as u8;
                            }
                        }
                        let (nw, nh) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
                        let mut out = vec![0u8; (nw * nh * 4) as usize];
                        if let (Some(src), Some(mut dst)) = (tiny_skia::Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w.max(1), h.max(1)).unwrap()), tiny_skia::Pixmap::new(nw, nh)) {
                            dst.draw_pixmap(0, 0, src.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() }, tiny_skia::Transform::from_scale(nw as f32 / w as f32, nh as f32 / h as f32), None);
                            out = dst.take();
                        }
                        Glyph { left: (i.placement.left as f32 * k).round() as i32, top: (i.placement.top as f32 * k).round() as i32, w: nw, h: nh, data: out, color: true }
                    }
                    Some(i) => Glyph { left: i.placement.left, top: i.placement.top, w: i.placement.width, h: i.placement.height, data: i.data.chunks_exact(4).map(|p| p[3]).collect(), color: false },
                    None => Glyph { left: 0, top: 0, w: 0, h: 0, data: Vec::new(), color: false },
                }
            } else {
                let img = Render::new(&[Source::Outline])
                    .format(Format::Alpha)
                    .offset(Vector::new(sub as f32 / 4.0, 0.0))
                    .render(&mut sc, id);
                match img {
                    Some(i) => Glyph { left: i.placement.left, top: i.placement.top, w: i.placement.width, h: i.placement.height, data: i.data, color: false },
                    None => Glyph { left: 0, top: 0, w: 0, h: 0, data: Vec::new(), color: false },
                }
            };
            self.cache.insert(key, g);
        }
        &self.cache[&key]
    }
}
