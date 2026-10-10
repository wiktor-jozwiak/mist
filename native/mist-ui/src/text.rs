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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Ui,
    Icons,
}

#[derive(Clone, Copy)]
pub struct Style {
    pub face: Face,
    pub size: f32,
    pub weight: f32,
    pub tnum: bool,
}
impl Style {
    pub fn ui(size: f32, weight: f32) -> Style {
        Style { face: Face::Ui, size, weight, tnum: false }
    }
    pub fn icon(size: f32) -> Style {
        Style { face: Face::Icons, size, weight: 400.0, tnum: false }
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
}

pub struct Shaped {
    pub glyphs: Vec<(u16, f32)>, // glyph id, pen x
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

pub struct Text {
    ui: &'static [u8],
    icons: &'static [u8],
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
        Text { ui, icons, scale: ScaleContext::new(), shape: ShapeContext::new(), cache: HashMap::new(), shaped: HashMap::new() }
    }
    fn font(&self, f: Face) -> FontRef<'static> {
        FontRef::from_index(if f == Face::Ui { self.ui } else { self.icons }, 0).unwrap()
    }
    // Txt.qml: wght = w, opsz = clamp(size, 8, 36)
    fn vars(st: &Style) -> [(&'static str, f32); 2] {
        [("wght", st.weight), ("opsz", st.size.clamp(8.0, 36.0))]
    }

    pub fn shape(&mut self, s: &str, st: &Style) -> std::rc::Rc<Shaped> {
        let key = (s.to_string(), st.face, st.size.to_bits(), st.weight.to_bits(), st.tnum);
        if let Some(r) = self.shaped.get(&key) {
            return r.clone();
        }
        if self.shaped.len() > 2000 {
            self.shaped.clear();
        }
        let font = self.font(st.face);
        let vars = Self::vars(st);
        let mut b = self.shape.builder(font).size(st.size);
        if st.face == Face::Ui {
            b = b.variations(vars.iter().copied());
        }
        let tn = [("tnum", 1u16)];
        if st.tnum {
            b = b.features(tn.iter().copied());
        }
        let mut sh = b.build();
        sh.add_str(s);
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        sh.shape_with(|c| {
            for g in c.glyphs {
                glyphs.push((g.id, x + g.x));
                x += g.advance;
            }
        });
        let coords: Vec<i16> = if st.face == Face::Ui { font.variations().normalized_coords(vars.iter().copied()).collect() } else { Vec::new() };
        let m = font.metrics(&coords).scale(st.size);
        let r = std::rc::Rc::new(Shaped { glyphs, width: x, ascent: m.ascent, descent: m.descent });
        self.shaped.insert(key, r.clone());
        r
    }

    pub fn width(&mut self, s: &str, st: &Style) -> f32 {
        self.shape(s, st).width
    }

    // the coverage bitmap of one glyph at a quarter-pixel horizontal offset
    pub fn glyph(&mut self, st: &Style, id: u16, sub: u8) -> &Glyph {
        let key = (st.face, id, st.size.to_bits(), st.weight.to_bits(), sub);
        if !self.cache.contains_key(&key) {
            if self.cache.len() > 4000 {
                self.cache.clear();
            }
            let font = self.font(st.face);
            let vars = Self::vars(st);
            let mut b = self.scale.builder(font).size(st.size).hint(false);
            if st.face == Face::Ui {
                b = b.variations(vars.iter().copied());
            }
            let mut sc = b.build();
            let img = Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .offset(Vector::new(sub as f32 / 4.0, 0.0))
                .render(&mut sc, id);
            let g = match img {
                Some(i) => Glyph { left: i.placement.left, top: i.placement.top, w: i.placement.width, h: i.placement.height, data: i.data },
                None => Glyph { left: 0, top: 0, w: 0, h: 0, data: Vec::new() },
            };
            self.cache.insert(key, g);
        }
        &self.cache[&key]
    }
}
