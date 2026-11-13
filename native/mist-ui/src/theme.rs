// Mist tokens (Theme.qml): Apple dark-mode label colours and system colours.
use crate::gfx::Rgba;

pub const T1: Rgba = Rgba(1.0, 1.0, 1.0, 0.96);
pub const T2: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.6);
pub const T3: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.3);
pub const T4: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.18);
pub const ACCENT: Rgba = Rgba::rgb8(crate::tokens::ACCENT.0, crate::tokens::ACCENT.1, crate::tokens::ACCENT.2, 1.0);

// The accent as drawn: settings.json {"accent": "#rrggbb"} (Mist Settings, Personalizacja > Kolory), or
// "wallpaper" (the photo's colour, wall_accent_update), else ACCENT.
// Read once, again after reload_accent (`msg ui settings`); packed 0xRRGGBB, u32::MAX = not read yet.
static ACC: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
pub fn accent() -> Rgba {
    use std::sync::atomic::Ordering;
    let mut v = ACC.load(Ordering::Relaxed);
    if v == u32::MAX {
        v = read_accent();
        ACC.store(v, Ordering::Relaxed);
    }
    Rgba::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0)
}
pub fn reload_accent() {
    *PAL.write().unwrap() = None;
    ACC.store(read_accent(), std::sync::atomic::Ordering::Relaxed);
}

// Themes: settings.json {"theme": "nord"} tints the glass (panels, popups, cards), the app windows' body and
// the accent; an "accent" of its own still wins. Built in below, or ~/.config/mist/themes/<id>.json with the
// same keys as hex strings ({"name": "My theme", "accent": "#88c0d0", "tone": "#2e3440", "window_top": "#2e3440",
// "window_bottom": "#242933"}) to share. Dark themes only: the text colours (T1..T4) stay white.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub accent: u32,
    /// glass tone under panels, popups and desktop cards (drawn at the glass's own opacity)
    pub tone: u32,
    /// app window body (Mist Files, Settings), top to bottom; alpha from tokens::WINDOW_TOP / _BOTTOM
    pub top: u32,
    pub bottom: u32,
}
const fn pack(c: (u8, u8, u8)) -> u32 {
    (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32
}
pub const MIST: Palette = Palette { accent: pack(crate::tokens::ACCENT), tone: pack(crate::tokens::TONE), top: pack((crate::tokens::WINDOW_TOP.0, crate::tokens::WINDOW_TOP.1, crate::tokens::WINDOW_TOP.2)), bottom: pack((crate::tokens::WINDOW_BOTTOM.0, crate::tokens::WINDOW_BOTTOM.1, crate::tokens::WINDOW_BOTTOM.2)) };
/// (id, shown name, palette); the first is the default
pub const THEMES: [(&str, &str, Palette); 7] = [
    ("mist", "Mist", MIST),
    ("catppuccin", "Catppuccin", Palette { accent: 0xcba6f7, tone: 0x181825, top: 0x1e1e2e, bottom: 0x11111b }),
    ("nord", "Nord", Palette { accent: 0x88c0d0, tone: 0x2e3440, top: 0x2e3440, bottom: 0x242933 }),
    ("gruvbox", "Gruvbox", Palette { accent: 0xfe8019, tone: 0x1d2021, top: 0x282828, bottom: 0x1d2021 }),
    ("tokyonight", "Tokyo Night", Palette { accent: 0x7aa2f7, tone: 0x1a1b26, top: 0x1f2335, bottom: 0x16161e }),
    ("rosepine", "Rosé Pine", Palette { accent: 0xebbcba, tone: 0x191724, top: 0x1f1d2e, bottom: 0x191724 }),
    ("dracula", "Dracula", Palette { accent: 0xbd93f9, tone: 0x21222c, top: 0x282a36, bottom: 0x21222c }),
];
static PAL: std::sync::RwLock<Option<(String, Palette)>> = std::sync::RwLock::new(None);

fn hex_u32(v: &serde_json::Value) -> Option<u32> {
    v.as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()).filter(|v| *v <= 0xffffff)
}
fn themes_dir() -> String {
    format!("{}/.config/mist/themes", std::env::var("HOME").unwrap_or_default())
}
/// a theme file's palette; keys it lacks come from Mist's own
pub fn parse_theme(v: &serde_json::Value) -> Palette {
    let k = |key: &str, d: u32| hex_u32(&v[key]).unwrap_or(d);
    Palette { accent: k("accent", MIST.accent), tone: k("tone", MIST.tone), top: k("window_top", MIST.top), bottom: k("window_bottom", MIST.bottom) }
}
fn theme_by_id(id: &str) -> Option<Palette> {
    if let Some(t) = THEMES.iter().find(|t| t.0 == id) {
        return Some(t.2);
    }
    // a file name, never a path
    if id.is_empty() || id.contains('/') || id.starts_with('.') {
        return None;
    }
    read(&format!("{}/{id}.json", themes_dir())).map(|v| parse_theme(&v))
}
/// is this a theme that can be drawn: built in, or a file in ~/.config/mist/themes
pub fn theme_known(id: &str) -> bool {
    theme_by_id(id).is_some()
}
/// every theme to pick: the built-in ones, then ~/.config/mist/themes/*.json by name
pub fn theme_list() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = THEMES.iter().map(|t| (t.0.to_string(), t.1.to_string())).collect();
    let mut own: Vec<(String, String)> = std::fs::read_dir(themes_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().strip_suffix(".json")?.to_string();
            let name = read(&e.path().to_string_lossy()).and_then(|j| j["name"].as_str().map(String::from)).unwrap_or_else(|| id.clone());
            (!THEMES.iter().any(|t| t.0 == id)).then_some((id, name))
        })
        .collect();
    own.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    v.extend(own);
    v
}
/// the chosen theme's id ("mist" when none or an unknown one)
pub fn theme_id() -> String {
    palette();
    PAL.read().unwrap().as_ref().map(|p| p.0.clone()).unwrap_or_else(|| "mist".into())
}
/// the chosen theme (MIST_THEME overrides settings.json); read once, again after reload_accent
pub fn palette() -> Palette {
    if let Some(p) = PAL.read().unwrap().as_ref() {
        return p.1;
    }
    let id = std::env::var("MIST_THEME").ok().filter(|t| !t.is_empty()).or_else(|| {
        read(&format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default())).and_then(|v| v["theme"].as_str().map(String::from))
    });
    let (id, p) = id.and_then(|id| theme_by_id(&id).map(|p| (id, p))).unwrap_or(("mist".into(), MIST));
    *PAL.write().unwrap() = Some((id, p));
    p
}
fn unpack(v: u32, a: f32) -> Rgba {
    Rgba::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8, a)
}
/// the glass tone (Glass::default)
pub fn tone() -> Rgba {
    unpack(palette().tone, 1.0)
}
/// the app window body, top and bottom, as tokens::WINDOW_TOP / _BOTTOM are written (r, g, b, alpha): the
/// theme's colours at the tokens' opacity
pub fn window_tokens() -> ((u8, u8, u8, f32), (u8, u8, u8, f32)) {
    let p = palette();
    let c = |v: u32, a: f32| ((v >> 16) as u8, (v >> 8) as u8, v as u8, a);
    (c(p.top, crate::tokens::WINDOW_TOP.3), c(p.bottom, crate::tokens::WINDOW_BOTTOM.3))
}
fn read_accent() -> u32 {
    // no accent of its own: the theme's (Mist's own theme: tokens::ACCENT)
    let def = palette().accent;
    let hex = |v: &serde_json::Value| v.as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()).filter(|v| *v <= 0xffffff);
    let set = setting_accent();
    if set.as_str() == Some("wallpaper") {
        return read(&wall_cache()).and_then(|c| hex(&c["color"])).unwrap_or(def);
    }
    hex(&set).unwrap_or(def)
}
pub fn read(path: &str) -> Option<serde_json::Value> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok())
}
fn setting_accent() -> serde_json::Value {
    read(&format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default())).map(|v| v["accent"].clone()).unwrap_or_default()
}
fn wall_cache() -> String {
    format!("{}/.cache/mist/accent-wall.json", std::env::var("HOME").unwrap_or_default())
}

// "accent": "wallpaper": the photo's colour, worked out once per photo into ~/.cache/mist/accent-wall.json
// (a live wallpaper or a grey photo: ACCENT). Blocking (one decode, ~0.1 s): call it off the main thread.
// True when the colour may have changed (reload_accent then).
pub fn wall_accent_update() -> bool {
    if setting_accent().as_str() != Some("wallpaper") {
        return false;
    }
    let (live, image) = crate::wall::Wall::read_cfg();
    let key = if live.is_empty() { image } else { String::new() };
    if read(&wall_cache()).is_some_and(|c| c["image"].as_str() == Some(key.as_str())) {
        return false;
    }
    let col = (!key.is_empty())
        .then(|| image::ImageReader::open(&key).ok()?.with_guessed_format().ok()?.decode().ok())
        .flatten()
        .and_then(|im| pick_accent(&im.thumbnail(64, 64).to_rgb8()));
    let _ = std::fs::create_dir_all(format!("{}/.cache/mist", std::env::var("HOME").unwrap_or_default()));
    let _ = std::fs::write(wall_cache(), serde_json::json!({ "image": key, "color": col.map(|c| format!("#{c:06x}")) }).to_string());
    true
}

// the most vivid hue of a picture, as a readable accent on dark glass (0xRRGGBB); None for a grey one
pub fn pick_accent(img: &image::RgbImage) -> Option<u32> {
    // 24 hue bins of 15°: weight, r, g, b
    let mut bins = [[0.0f32; 4]; 24];
    for p in img.pixels() {
        let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let s = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        if s < 0.25 || mx < 0.2 {
            continue;
        }
        let d = mx - mn;
        let h = if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 } * 60.0;
        let w = s * s * mx;
        let bin = &mut bins[(h / 15.0) as usize % 24];
        bin[0] += w;
        bin[1] += r * w;
        bin[2] += g * w;
        bin[3] += b * w;
    }
    let score = |i: usize| bins[(i + 23) % 24][0] + 2.0 * bins[i][0] + bins[(i + 1) % 24][0];
    let best = (0..24).max_by(|a, b| score(*a).total_cmp(&score(*b)))?;
    let [w, r, g, b] = bins[best];
    if w < 0.02 * (img.width() * img.height()) as f32 {
        return None;
    }
    // as HSL: saturation and lightness kept where white text and the glass stay readable
    let (r, g, b) = (r / w, g / w, b / w);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 { 0.0 } else if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    let (s, l) = (s.clamp(0.5, 0.85), l.clamp(0.52, 0.66));
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let u = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    Some(u(r1) << 16 | u(g1) << 8 | u(b1))
}
pub const GREEN: Rgba = Rgba::rgb8(crate::tokens::GREEN.0, crate::tokens::GREEN.1, crate::tokens::GREEN.2, 1.0);
pub const YELLOW: Rgba = Rgba::rgb8(crate::tokens::YELLOW.0, crate::tokens::YELLOW.1, crate::tokens::YELLOW.2, 1.0);
pub const ORANGE: Rgba = Rgba::rgb8(crate::tokens::ORANGE.0, crate::tokens::ORANGE.1, crate::tokens::ORANGE.2, 1.0);
pub const DANGER: Rgba = Rgba::rgb8(crate::tokens::DANGER.0, crate::tokens::DANGER.1, crate::tokens::DANGER.2, 1.0);
pub const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.1);
pub const FILL: Rgba = Rgba(1.0, 1.0, 1.0, 0.055);
pub const FILL2: Rgba = Rgba(1.0, 1.0, 1.0, 0.1);

// High contrast (accessibility): ~/.config/mist/settings.json {"contrast": "high"}, MIST_CONTRAST=high
// overrides; read once, again after reload_contrast. Secondary text gets near full opacity (gfx::Canvas::text), glass panels an
// opaque body (glass.rs), so nothing depends on the wallpaper behind it.
pub fn high_contrast() -> bool {
    use std::sync::atomic::Ordering;
    match HC.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => {
            let on = read_contrast();
            HC.store(on as u8, Ordering::Relaxed);
            on
        }
    }
}
// 2 = not read yet; reload_contrast after `msg ui settings`
static HC: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);
pub fn reload_contrast() {
    HC.store(read_contrast() as u8, std::sync::atomic::Ordering::Relaxed);
}
fn read_contrast() -> bool {
    match std::env::var("MIST_CONTRAST") {
        Ok(v) => v == "high",
        Err(_) => std::fs::read_to_string(format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default()))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .is_some_and(|v| v["contrast"] == "high"),
    }
}

// a text colour as drawn: in high contrast 2.5x its opacity (T3 0.3 -> 0.75, T2 -> 1), fades still fade
pub fn text_colour(c: Rgba) -> Rgba {
    if high_contrast() {
        Rgba(c.0, c.1, c.2, (c.3 * 2.5).min(1.0))
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    // the Zen theme copies the tokens by hand: keep them equal (the theme is local only, not in the repo:
    // without it there is nothing to compare)
    #[test]
    fn tokens_match_zen_css() {
        use crate::tokens as t;
        let Ok(css) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../integrations/zen/chrome/mist/tokens.css")) else {
            return eprintln!("no local integrations/zen: skipped");
        };
        let has = |s: String| assert!(css.contains(&s), "tokens.css lacks `{s}`");
        let hex = |c: (u8, u8, u8)| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
        let rgba = |c: (u8, u8, u8, f32)| format!("rgba({}, {}, {}, {})", c.0, c.1, c.2, c.3);
        has(format!("--mist-r-window: {}px;", t::WINDOW_RADIUS));
        has(format!("--mist-window: linear-gradient(180deg, {}, {});", rgba(t::WINDOW_TOP), rgba(t::WINDOW_BOTTOM)));
        has(format!("--mist-glass: rgba({}, {}, {}, 0.82);", t::BAR.0, t::BAR.1, t::BAR.2));
        has(format!("--mist-tone: {}, {}, {};", t::TONE.0, t::TONE.1, t::TONE.2));
        for (k, c) in [("accent", t::ACCENT), ("green", t::GREEN), ("yellow", t::YELLOW), ("orange", t::ORANGE), ("danger", t::DANGER)] {
            has(format!("--mist-{k}: {};", hex(c)));
        }
    }
}

#[cfg(test)]
mod accent_tests {
    #[test]
    fn accent_from_a_picture() {
        // a dull grey photo with a teal sea in the lower half: teal, made readable
        let img = image::RgbImage::from_fn(40, 40, |_, y| if y < 20 { image::Rgb([120, 120, 125]) } else { image::Rgb([20, 110, 120]) });
        let c = super::pick_accent(&img).unwrap();
        let (r, g, b) = (c >> 16, (c >> 8) & 255, c & 255);
        assert!(g > r + 60 && b > r + 60, "{c:06x}");
        assert!((r + g + b) / 3 > 100, "too dark: {c:06x}");
        let grey = image::RgbImage::from_pixel(40, 40, image::Rgb([90, 92, 95]));
        assert_eq!(super::pick_accent(&grey), None);
    }
}

#[cfg(test)]
mod theme_tests {
    #[test]
    fn theme_files_and_presets() {
        // a file with only some keys keeps Mist's for the rest
        let p = super::parse_theme(&serde_json::json!({ "accent": "#88c0d0", "tone": "2e3440", "window_top": "#zz" }));
        assert_eq!((p.accent, p.tone, p.top, p.bottom), (0x88c0d0, 0x2e3440, super::MIST.top, super::MIST.bottom));
        assert_eq!(super::theme_by_id("nord").unwrap().accent, 0x88c0d0);
        // ids are file names, never paths
        for bad in ["../settings", "/etc/passwd", ".hidden", ""] {
            assert!(super::theme_by_id(bad).is_none(), "{bad}");
        }
        // ids unique, the default first
        assert_eq!(super::THEMES[0].0, "mist");
        let mut ids: Vec<_> = super::THEMES.iter().map(|t| t.0).collect();
        ids.dedup();
        assert_eq!(ids.len(), super::THEMES.len());
    }
}
