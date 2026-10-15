// The dock's window picker (Windows taskbar thumbnails): a click on an app with more than one window shows
// them above its icon, each with its title and a live-at-open thumbnail (wincap.rs, the app icon until it
// arrives). Click a window to bring it (back from the minimized drawer too), middle click or × closes it,
// Esc / a click outside closes the picker; ←/→ + Enter work too.
use crate::anim::{Anim, Ease};
use crate::gfx::Canvas;
use crate::pane::*;
use crate::switcher::hypr;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use tiny_skia::Pixmap;

// one window card: title row + thumbnail
pub const THUMB_W: u32 = 232;
pub const THUMB_H: u32 = 130;
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const TITLE_H: f32 = 30.0;
const CARD_W: f32 = THUMB_W as f32 + 2.0 * PAD;
const CARD_H: f32 = TITLE_H + THUMB_H as f32 + PAD;

pub struct Win {
    pub address: String,
    pub title: String,
    pub minimized: bool,
}

pub struct WinPick {
    icon: String,
    wins: Vec<Win>,
    thumbs: HashMap<String, Pixmap>,
    anchor_x: f32, // the dock icon's centre on this monitor
    bottom: f32,   // the sheet's bottom edge (above the dock)
    focused_ws: i64,
    sel: Option<usize>,
    hot_close: Option<usize>,
    kbd: bool,
    o: Anim,
    cards: Vec<(f32, f32, f32, f32)>,
    closes: Vec<(f32, f32, f32, f32)>,
    sheet: (f32, f32, f32, f32),
}

impl WinPick {
    pub fn new(icon: String, wins: Vec<Win>, anchor_x: f32, bottom: f32, focused_ws: i64) -> WinPick {
        let mut o = Anim::new(0.0);
        o.set(1.0, 140.0, Ease::OutCubic);
        WinPick { icon, wins, thumbs: HashMap::new(), anchor_x, bottom, focused_ws, sel: None, hot_close: None, kbd: false, o, cards: Vec::new(), closes: Vec::new(), sheet: (0.0, 0.0, 0.0, 0.0) }
    }
    pub fn addresses(&self) -> Vec<String> {
        self.wins.iter().map(|w| w.address.clone()).collect()
    }
    // a thumbnail arrived: true = repaint
    pub fn thumb(&mut self, addr: String, pm: Pixmap) -> bool {
        if self.wins.iter().any(|w| w.address == addr) {
            self.thumbs.insert(addr, pm);
            return true;
        }
        false
    }
    fn addr(a: &str) -> String {
        format!("0x{}", a.trim_start_matches("0x"))
    }
    fn activate(&self, i: usize) -> Act {
        let Some(w) = self.wins.get(i) else { return Act::Close };
        let a = Self::addr(&w.address);
        if w.minimized {
            // windows-like.lua: back from the minimized drawer onto the active workspace
            return Act::Run(hypr(&format!("MistRestore('{a}', {})", self.focused_ws)));
        }
        Act::Run(hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"address:{a}\" }})); hl.dispatch(hl.dsp.window.bring_to_top())")))
    }
    fn close_win(&mut self, i: usize) -> Act {
        if i >= self.wins.len() {
            return Act::None;
        }
        let w = self.wins.remove(i);
        self.thumbs.remove(&w.address);
        self.sel = None;
        self.hot_close = None;
        let cmd = hypr(&format!("hl.dispatch(hl.dsp.window.close({{ window = \"address:{}\" }}))", Self::addr(&w.address)));
        if self.wins.is_empty() { Act::Run(cmd) } else { Act::Exec(cmd) }
    }
}

impl Pane for WinPick {
    fn name(&self) -> &'static str {
        "winpick"
    }
    fn animating(&self) -> bool {
        self.o.running()
    }
    fn hand(&self) -> bool {
        self.sel.is_some()
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.cards.clear();
        self.closes.clear();
        let n = self.wins.len() as f32;
        let (w, h) = (n * CARD_W + (n - 1.0).max(0.0) * GAP + 2.0 * PAD, CARD_H + 2.0 * PAD);
        let x0 = (self.anchor_x - w / 2.0).clamp(12.0, (c.w() - w - 12.0).max(12.0)).round();
        let o = self.o.value();
        // rises 8 px into place while it fades in
        let y0 = (self.bottom - h + (1.0 - o) * 8.0).round();
        self.sheet = (x0, y0, w, h);
        // drawn into a layer of the sheet + its shadow (faded as one), not the whole monitor
        const M: f32 = 40.0;
        let mut l = Canvas::new((w + 2.0 * M) as u32, (h + 2.0 * M) as u32);
        let (ax, ay) = (x0 - M, y0 - M);
        glass(&mut l, M, M, w, h, 18.0, 0.72);
        let ts = crate::text::Style::ui(12.5, 550.0);
        let tlh = lh(cx.t, 12.5, 550.0);
        for (i, win) in self.wins.iter().enumerate() {
            let (x, y) = (M + PAD + i as f32 * (CARD_W + GAP), M + PAD);
            let hot = self.sel == Some(i);
            if hot {
                crate::desktop::rect(&mut l, x, y, CARD_W, CARD_H, 12.0, FILL2);
                crate::desktop::border(&mut l, x, y, CARD_W, CARD_H, 12.0, 1.0, HAIR);
            }
            // title row: icon · title · ×
            let pm = cx.imgs.get(&self.icon, 16, cx.icons).or_else(|| cx.imgs.get("application-x-executable", 16, cx.icons));
            if let Some(pm) = pm {
                crate::img::draw(&mut l, &pm, x + PAD + 2.0, y + (TITLE_H - 16.0) / 2.0, 16.0, 16.0, 0.0, false, 1.0);
            }
            let close = (x + CARD_W - PAD - 22.0, y + (TITLE_H - 22.0) / 2.0, 22.0, 22.0);
            crate::desktop::text(&mut l, cx.t, x + PAD + 26.0, y + (TITLE_H - tlh) / 2.0, &win.title, &ts, if win.minimized { T2 } else { T1 }, false, Some(CARD_W - 2.0 * PAD - 26.0 - (if hot { 28.0 } else { 4.0 })));
            if hot {
                let ch = self.hot_close == Some(i);
                crate::desktop::rect(&mut l, close.0, close.1, close.2, close.3, 11.0, if ch { DANGER } else { FILL2 });
                l.icon(cx.t, close.0 + 11.0, close.1 + 11.0, "x", 12.0, T1);
            }
            self.closes.push((close.0 + ax, close.1 + ay, close.2, close.3));
            // thumbnail (the app icon until it arrives), letterboxed in its box
            let (bx, by) = (x + PAD, y + TITLE_H);
            match self.thumbs.get(&win.address) {
                Some(t) => {
                    let (tw, th) = (t.width() as f32, t.height() as f32);
                    crate::img::draw(&mut l, t, (bx + (THUMB_W as f32 - tw) / 2.0).round(), (by + (THUMB_H as f32 - th) / 2.0).round(), tw, th, 6.0, false, if win.minimized { 0.7 } else { 1.0 });
                }
                None => {
                    crate::desktop::rect(&mut l, bx, by, THUMB_W as f32, THUMB_H as f32, 6.0, FILL);
                    let pm = cx.imgs.get(&self.icon, 48, cx.icons).or_else(|| cx.imgs.get("application-x-executable", 48, cx.icons));
                    if let Some(pm) = pm {
                        crate::img::draw(&mut l, &pm, bx + (THUMB_W as f32 - 48.0) / 2.0, by + (THUMB_H as f32 - 48.0) / 2.0, 48.0, 48.0, 0.0, false, 0.8);
                    }
                }
            }
            self.cards.push((x + ax, y + ay, CARD_W, CARD_H));
        }
        glass_rim(&mut l, M, M, w, h, 18.0);
        let paint = tiny_skia::PixmapPaint { opacity: o, ..Default::default() };
        c.pm.draw_pixmap(ax as i32, ay as i32, l.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let card = self.cards.iter().position(|r| inside((x, y), *r));
        let close = card.filter(|&i| self.closes.get(i).map(|r| inside((x, y), *r)).unwrap_or(false));
        match ev {
            Ptr::Motion | Ptr::Enter => {
                if self.kbd && card.is_none() {
                    return Act::None;
                }
                self.kbd = false;
                if (card, close) != (self.sel, self.hot_close) {
                    self.sel = card;
                    self.hot_close = close;
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Release(b) => match (card, close) {
                (Some(i), Some(_)) => self.close_win(i),
                (Some(i), None) if b == 0x112 => self.close_win(i), // BTN_MIDDLE
                (Some(i), None) => self.activate(i),
                _ if inside((x, y), self.sheet) => Act::None,
                _ => Act::Close,
            },
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        let n = self.wins.len();
        match ev.keysym {
            Keysym::Escape => Act::Close,
            Keysym::Return | Keysym::KP_Enter => self.activate(self.sel.unwrap_or(0)),
            Keysym::Right | Keysym::Tab if n > 0 => {
                self.kbd = true;
                self.sel = Some(self.sel.map(|s| (s + 1) % n).unwrap_or(0));
                Act::Redraw
            }
            Keysym::Left | Keysym::ISO_Left_Tab if n > 0 => {
                self.kbd = true;
                self.sel = Some(self.sel.map(|s| (s + n - 1) % n).unwrap_or(n - 1));
                Act::Redraw
            }
            Keysym::Delete => match self.sel {
                Some(i) => self.close_win(i),
                None => Act::None,
            },
            _ => Act::None,
        }
    }
    fn as_winpick(&mut self) -> Option<&mut WinPick> {
        Some(self)
    }
}
