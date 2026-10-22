// Mist design tokens shared by mist-comp, mist-ui and mist-wall (plain numbers, no crate types). The Zen theme
// (integrations/zen/chrome/mist/tokens.css, local only, not in the repo) mirrors them; mist-ui's test `tokens_match_zen_css` checks it.
#![allow(dead_code)]

/// Window corner radius (logical px): mist-comp's bars, rounded windows and shadow, mist-ui app windows, Zen.
pub const WINDOW_RADIUS: f32 = 12.0;
/// Window body: dark tinted glass, top to bottom (r, g, b, alpha). Mist Files, Zen's --mist-window.
pub const WINDOW_TOP: (u8, u8, u8, f32) = (20, 26, 38, 0.80);
pub const WINDOW_BOTTOM: (u8, u8, u8, f32) = (9, 12, 19, 0.84);
/// mist-comp's server-side title bar; Zen's --mist-glass (at 0.82).
pub const BAR: (u8, u8, u8) = (16, 21, 31);
/// Liquid glass tone (panels, popups).
pub const TONE: (u8, u8, u8) = (16, 22, 36);
/// System colours (Apple dark mode).
pub const ACCENT: (u8, u8, u8) = (0x0A, 0x84, 0xFF);
pub const GREEN: (u8, u8, u8) = (0x30, 0xD1, 0x58);
pub const YELLOW: (u8, u8, u8) = (0xFF, 0xD6, 0x0A);
pub const ORANGE: (u8, u8, u8) = (0xFF, 0x9F, 0x0A);
pub const DANGER: (u8, u8, u8) = (0xFF, 0x45, 0x3A);
