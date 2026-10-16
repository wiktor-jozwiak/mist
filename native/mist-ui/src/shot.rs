// Screenshots like macOS Cmd+Shift+5 (was Shot.qml): a toolbar at the bottom (whole screen / window /
// selection, delay, clipboard, Capture), then a floating thumbnail bottom-right for 6 s (thumb.rs side in
// app.rs): click edits it in Swash, the folder button opens the folder. The capture itself:
// scripts/shot.sh (grim + slurp). IPC: shot toolbar | screen | window | region.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

pub struct Opts {
    pub mode: &'static str,
    pub delay: u32,
    pub copy: bool,
}
impl Default for Opts {
    fn default() -> Opts {
        Opts { mode: "region", delay: 0, copy: true }
    }
}

// run shot.sh in a thread; the saved file comes back as Msg::Shot
pub fn capture(o: &Opts, tx: Option<calloop::channel::Sender<crate::sys::Msg>>) {
    let sh = crate::paths::mist_path("scripts/shot.sh");
    let args = vec![o.mode.to_string(), o.delay.to_string(), if o.copy { "1".into() } else { "0".into() }];
    std::thread::spawn(move || {
        // the toolbar must be gone from the screen first
        std::thread::sleep(std::time::Duration::from_millis(250));
        let out = std::process::Command::new(sh).args(&args).output();
        let f = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if !f.is_empty() {
            if let Some(tx) = tx {
                let _ = tx.send(crate::sys::Msg::Shot(f));
            }
        }
    });
}

pub struct Toolbar {
    pub o: Opts,
    ptr: (f32, f32),
    hits: Vec<((f32, f32, f32, f32), u8)>, // 0 close, 1 screen, 2 window, 3 region, 4 delay, 5 copy, 6 capture, 9 panel
    pressed_outside: bool,
}

impl Toolbar {
    pub fn new(o: Opts) -> Toolbar {
        Toolbar { o, ptr: (-1.0, -1.0), hits: Vec::new(), pressed_outside: false }
    }
    fn hit(&self) -> Option<u8> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1)
    }
}

impl Pane for Toolbar {
    fn name(&self) -> &'static str {
        "shot"
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(0..=6))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        c.clear();
        self.hits.clear();
        let chip_label = |o: &Opts, k: u8| -> String {
            if k == 4 { if o.delay > 0 { format!("Opóźnienie {} s", o.delay) } else { "Bez opóźnienia".into() } } else if o.copy { "✓ Schowek".into() } else { "Schowek".into() }
        };
        let cs = Style::ui(12.5, 550.0);
        let caps = Style::ui(13.0, 600.0);
        let w4 = t.width(&chip_label(&self.o, 4), &cs) + 22.0;
        let w5 = t.width(&chip_label(&self.o, 5), &cs) + 22.0;
        let wc = t.width("Przechwyć", &caps) + 30.0;
        // x · | screen window region | delay copy | Capture
        let tools_w = 46.0 + 6.0 + 1.0 + 6.0 + 3.0 * 46.0 + 2.0 * 6.0 + 6.0 + 1.0 + 6.0 + w4 + 6.0 + w5 + 6.0 + 1.0 + 6.0 + wc;
        let (pw, ph) = (tools_w + 16.0, 58.0);
        let (px, py) = (((c.w() - pw) / 2.0).round(), c.h() - 120.0 - 74.0 + 8.0);
        glass(c, px, py, pw, ph, 18.0, 0.8);
        self.hits.push(((px, py, pw, ph), 9));
        let mut x = px + 8.0;
        let ty = py + 8.0;
        let ptr = self.ptr;
        let tool = |c: &mut Canvas, t: &mut crate::text::Text, x: f32, icon: &str, on: bool, k: u8, tip: &str, hits: &mut Vec<((f32, f32, f32, f32), u8)>| {
            let hov = inside(ptr, (x, ty, 46.0, 42.0));
            if on {
                crate::desktop::rect(c, x, ty, 46.0, 42.0, 11.0, Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18));
                crate::desktop::border(c, x, ty, 46.0, 42.0, 11.0, 1.0, Rgba(169.0 / 255.0, 194.0 / 255.0, 238.0 / 255.0, 0.35));
            } else if hov {
                crate::desktop::rect(c, x, ty, 46.0, 42.0, 11.0, FILL2);
            }
            c.icon(t, x + 23.0, ty + 21.0, icon, 21.0, if on { ACCENT } else { T1 });
            if hov {
                let st = Style::ui(11.5, 450.0);
                let tw = t.width(tip, &st) + 16.0;
                let (bx, by) = (x + 23.0 - tw / 2.0, ty - 8.0 - 24.0);
                crate::desktop::rect(c, bx, by, tw, 24.0, 8.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.86));
                let th = crate::desktop::line_h(t, &st);
                crate::desktop::text(c, t, bx + 8.0, by + (24.0 - th) / 2.0, tip, &st, T1, false, None);
            }
            hits.push(((x, ty, 46.0, 42.0), k));
        };
        let mut hits = Vec::new();
        tool(c, t, x, "x", false, 0, "Zamknij (Esc)", &mut hits);
        x += 46.0 + 6.0;
        crate::desktop::rect(c, x, py + 15.0, 1.0, 28.0, 0.0, HAIR);
        x += 1.0 + 6.0;
        for (k, icon, m, tip) in [(1u8, "desktop", "screen", "Cały ekran"), (2, "window", "window", "Wybrane okno"), (3, "screenshot", "region", "Zaznaczony obszar")] {
            tool(c, t, x, icon, self.o.mode == m, k, tip, &mut hits);
            x += 46.0 + 6.0;
        }
        crate::desktop::rect(c, x, py + 15.0, 1.0, 28.0, 0.0, HAIR);
        x += 1.0 + 6.0;
        for (k, w) in [(4u8, w4), (5u8, w5)] {
            let on = if k == 4 { self.o.delay > 0 } else { self.o.copy };
            let hov = inside(ptr, (x, py + 13.0, w, 32.0));
            crate::desktop::rect(c, x, py + 13.0, w, 32.0, 16.0, if hov { FILL2 } else { FILL });
            let s = chip_label(&self.o, k);
            let th = crate::desktop::line_h(t, &cs);
            crate::desktop::text(c, t, x + 11.0, py + 13.0 + (32.0 - th) / 2.0, &s, &cs, if on { T1 } else { T3 }, false, None);
            hits.push(((x, py + 13.0, w, 32.0), k));
            x += w + 6.0;
        }
        crate::desktop::rect(c, x, py + 15.0, 1.0, 28.0, 0.0, HAIR);
        x += 1.0 + 6.0;
        let hov = inside(ptr, (x, py + 11.0, wc, 36.0));
        crate::desktop::rect(c, x, py + 11.0, wc, 36.0, 18.0, if hov { ACCENT } else { Rgba::hex("#409CFF") });
        let th = crate::desktop::line_h(t, &caps);
        crate::desktop::text(c, t, x + 15.0, py + 11.0 + (36.0 - th) / 2.0, "Przechwyć", &caps, Rgba::hex("#0e1420"), false, None);
        hits.push(((x, py + 11.0, wc, 36.0), 6));
        self.hits.extend(hits);
        glass_rim(c, px, py, pw, ph, 18.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let before = self.hit();
        self.ptr = (x, y);
        match ev {
            Ptr::Motion | Ptr::Enter | Ptr::Leave => if self.hit() != before { Act::Redraw } else { Act::None },
            Ptr::Press(_) => {
                self.pressed_outside = self.hit().is_none();
                Act::None
            }
            Ptr::Release(_) => {
                match self.hit() {
                    Some(0) => return Act::Close,
                    Some(1) => self.o.mode = "screen",
                    Some(2) => self.o.mode = "window",
                    Some(3) => self.o.mode = "region",
                    Some(4) => self.o.delay = match self.o.delay { 0 => 5, 5 => 10, _ => 0 },
                    Some(5) => self.o.copy = !self.o.copy,
                    Some(6) => return Act::Capture,
                    None if self.pressed_outside => return Act::Close,
                    _ => return Act::None,
                }
                Act::Redraw
            }
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape => return Act::Close,
            Keysym::Return | Keysym::KP_Enter => return Act::Capture,
            Keysym::_1 => self.o.mode = "screen",
            Keysym::_2 => self.o.mode = "window",
            Keysym::_3 => self.o.mode = "region",
            _ => return Act::None,
        }
        Act::Redraw
    }
    fn as_shot(&mut self) -> Option<&mut Toolbar> {
        Some(self)
    }
}

// ------------------------------------------------------------------ the floating thumbnail (240 wide card)
pub struct Thumb {
    pub path: String,
    pub copy: bool,
    pub ptr: (f32, f32),
    pub img_h: f32,
}
impl Thumb {
    pub const W: u32 = 260;
    pub fn height(&self) -> u32 {
        (self.img_h + 52.0 + 20.0) as u32
    }
    pub fn paint(&mut self, c: &mut Canvas, t: &mut crate::text::Text, imgs: &mut crate::img::Images, icons: &mut crate::apps::Icons) {
        c.clear();
        let (x, y, w) = (10.0, 10.0, 240.0);
        let h = self.img_h + 52.0;
        glass(c, x, y, w, h, 16.0, 0.78);
        if let Some(pm) = imgs.get(&self.path, 480, icons) {
            crate::img::draw(c, &pm, x + 8.0, y + 8.0, w - 16.0, self.img_h, 0.0, false, 1.0);
        }
        let by = y + h - 8.0 - 30.0;
        for (i, icon) in ["palette", "folderOpen"].iter().enumerate() {
            let bx = x + 10.0 + i as f32 * 36.0;
            let hov = inside(self.ptr, (bx, by, 30.0, 30.0));
            crate::desktop::rect(c, bx, by, 30.0, 30.0, 15.0, if hov { FILL2 } else { FILL });
            c.icon(t, bx + 15.0, by + 15.0, icon, 14.0, T1);
        }
        let cx = x + w - 10.0 - 30.0;
        let hov = inside(self.ptr, (cx, by, 30.0, 30.0));
        crate::desktop::rect(c, cx, by, 30.0, 30.0, 15.0, if hov { FILL2 } else { FILL });
        c.icon(t, cx + 15.0, by + 15.0, "x", 13.0, T1);
        let s = if self.copy { "W schowku" } else { "Zapisano" };
        let st = Style::ui(11.5, 450.0);
        let sw = t.width(s, &st);
        let sh = crate::desktop::line_h(t, &st);
        crate::desktop::text(c, t, cx - 8.0 - sw, by + (30.0 - sh) / 2.0, s, &st, T3, false, None);
        glass_rim(c, x, y, w, h, 16.0);
    }
    // a click: the command to run (None = just close)
    pub fn click(&self) -> Option<Vec<String>> {
        let (x, y, w) = (10.0, 10.0, 240.0);
        let h = self.img_h + 52.0;
        let by = y + h - 8.0 - 30.0;
        let p = self.ptr;
        let dir = self.path.rsplit_once('/').map(|d| d.0.to_string()).unwrap_or_default();
        if inside(p, (x + 8.0, y + 8.0, w - 16.0, self.img_h)) || inside(p, (x + 10.0, by, 30.0, 30.0)) {
            return Some(vec!["swash".into(), self.path.clone()]);
        }
        if inside(p, (x + 46.0, by, 30.0, 30.0)) {
            return Some(vec!["xdg-open".into(), dir]);
        }
        None
    }
}
