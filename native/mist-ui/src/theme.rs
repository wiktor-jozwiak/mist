// Mist tokens (Theme.qml): Apple dark-mode label colours and system colours.
use crate::gfx::Rgba;

pub const T1: Rgba = Rgba(1.0, 1.0, 1.0, 0.96);
pub const T2: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.6);
pub const T3: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.3);
pub const T4: Rgba = Rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.18);
pub const ACCENT: Rgba = Rgba::rgb8(crate::tokens::ACCENT.0, crate::tokens::ACCENT.1, crate::tokens::ACCENT.2, 1.0);

// The accent as drawn: settings.json {"accent": "#rrggbb"} (Mist Settings, Personalizacja > Kolory), else ACCENT.
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
    std::fs::read_to_string(format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default()))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["accent"].as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()))
        .filter(|v| *v <= 0xffffff)
        .unwrap_or(def)
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
