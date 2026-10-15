//! xcursor theme images as render buffers, cached per shape name (only the first frame: no animated cursors,
//! so an idle pointer never forces a redraw).
use std::collections::HashMap;

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Point, Transform},
};
use xcursor::{parser::parse_xcursor, CursorTheme};

pub struct Cursors {
    theme: CursorTheme,
    size: u32,
    cache: HashMap<String, Option<(MemoryRenderBuffer, Point<i32, Logical>)>>,
}

impl Cursors {
    pub fn load() -> Self {
        let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(24);
        Self { theme: CursorTheme::load(&name), size, cache: HashMap::new() }
    }

    /// The buffer and hotspot for a cursor name ("default", "text", "pointer", ...).
    /// A named cursor (CSS name, then the theme's older aliases: top_left_corner, size_fdiag...).
    pub fn get_icon(&mut self, icon: smithay::input::pointer::CursorIcon) -> Option<(MemoryRenderBuffer, Point<i32, Logical>)> {
        let key = icon.name();
        if !self.cache.contains_key(key) {
            let img = std::iter::once(key).chain(icon.alt_names().iter().copied()).find_map(|n| self.load_one(n)).or_else(|| self.load_one("left_ptr"));
            self.cache.insert(key.to_string(), img);
        }
        self.cache.get(key).cloned().flatten()
    }

    fn load_one(&self, name: &str) -> Option<(MemoryRenderBuffer, Point<i32, Logical>)> {
        let path = self.theme.load_icon(name)?;
        let data = std::fs::read(path).ok()?;
        let images = parse_xcursor(&data)?;
        let img = images.iter().min_by_key(|i| (i.size as i32 - self.size as i32).abs())?;
        let buf = MemoryRenderBuffer::from_slice(
            &img.pixels_rgba,
            Fourcc::Abgr8888,
            (img.width as i32, img.height as i32),
            1,
            Transform::Normal,
            None,
        );
        Some((buf, (img.xhot as i32, img.yhot as i32).into()))
    }
}

#[cfg(test)]
mod tests {
    use smithay::input::pointer::CursorIcon;

    #[test]
    fn resize_cursors_from_the_theme() {
        let mut c = super::Cursors::load();
        let arrow = c.get_icon(CursorIcon::Default).map(|(_, h)| h);
        for i in [CursorIcon::NwResize, CursorIcon::SeResize, CursorIcon::EResize, CursorIcon::NResize] {
            let got = c.get_icon(i);
            assert!(got.is_some(), "{i:?}");
            // not the left_ptr fallback (its hotspot is the arrow tip)
            assert_ne!(got.map(|(_, h)| h), arrow, "{i:?} fell back to the arrow");
        }
    }
}
