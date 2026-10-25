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
    ACC.store(read_accent(), std::sync::atomic::Ordering::Relaxed);
}
fn read_accent() -> u32 {
    let def = (crate::tokens::ACCENT.0 as u32) << 16 | (crate::tokens::ACCENT.1 as u32) << 8 | crate::tokens::ACCENT.2 as u32;
    let hex = |v: &serde_json::Value| v.as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()).filter(|v| *v <= 0xffffff);
    let set = setting_accent();
    if set.as_str() == Some("wallpaper") {
        return read(&wall_cache()).and_then(|c| hex(&c["color"])).unwrap_or(def);
    }
    hex(&set).unwrap_or(def)
}
fn read(path: &str) -> Option<serde_json::Value> {
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
