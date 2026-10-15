// macOS Cmd+Tab (was Switcher.qml; Super+Tab): one icon per app in a glass strip, most recent first, the
// name under the selected one. Super+Tab / Super+Shift+Tab step (binds -> msg switcher next|prev),
// releasing Super switches. Enter / click switch, Esc cancels, Q closes the app's windows; a missed Super
// release commits after 8 s.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::time::Instant;

pub struct App {
    pub cls: String,
    pub name: String,
    pub icon: String,
    pub wins: Vec<(String, String)>, // address, workspace name
}

pub struct Switcher {
    apps: Vec<App>,
    sel: usize,
    opened: Instant,
    ptr: (f32, f32),
    cells: Vec<(f32, f32, f32, f32)>,
}

pub fn hypr(lua: &str) -> Vec<String> {
    vec!["hyprctl".into(), "eval".into(), lua.into()]
}

impl Switcher {
    // the first press goes to the previous app, like macOS
    pub fn new(apps: Vec<App>, step: i32) -> Switcher {
        let n = apps.len();
        let sel = if n > 1 { if step > 0 { 1 } else { n - 1 } } else { 0 };
        Switcher { apps, sel, opened: Instant::now(), ptr: (-1.0, -1.0), cells: Vec::new() }
    }
    pub fn step(&mut self, d: i32) {
        let n = self.apps.len() as i32;
        if n > 0 {
            self.sel = ((self.sel as i32 + d).rem_euclid(n)) as usize;
        }
    }
    fn commit(&self) -> Act {
        let Some(a) = self.apps.get(self.sel) else { return Act::Close };
        let Some((addr, ws)) = a.wins.first() else { return Act::Close };
        let addr = format!("0x{}", addr.trim_start_matches("0x"));
        if ws == "special:minimized" {
            // windows-like.lua: back from the minimized drawer onto the active workspace
            return Act::Run(vec!["sh".into(), "-c".into(), format!("hyprctl eval \"MistRestore('{addr}', $(hyprctl activeworkspace -j | jq .id))\"")]);
        }
        Act::Run(hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"address:{addr}\" }})); hl.dispatch(hl.dsp.window.bring_to_top())")))
    }
    fn quit_app(&mut self) -> Act {
        if self.sel >= self.apps.len() {
            return Act::None;
        }
        let a = self.apps.remove(self.sel);
        let lua: String = a.wins.iter().map(|(addr, _)| format!("hl.dispatch(hl.dsp.focus({{ window = \"address:0x{}\" }})); hl.dispatch(hl.dsp.window.close()); ", addr.trim_start_matches("0x"))).collect();
        if self.apps.is_empty() {
            return Act::Run(hypr(&lua));
        }
        self.sel = self.sel.min(self.apps.len() - 1);
        Act::Exec(hypr(&lua))
    }
}

impl Pane for Switcher {
    fn name(&self) -> &'static str {
        "switcher"
    }
    fn namespace(&self) -> &'static str {
        "mist-switcher"
    }
    fn timer(&mut self) -> Act {
        if self.opened.elapsed().as_secs() >= 8 { self.commit() } else { Act::None }
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.cells.clear();
        let n = self.apps.len() as f32;
        let rw = n * 108.0 + (n - 1.0).max(0.0) * 4.0;
        let (w, h) = (rw + 24.0, 136.0);
        let (x0, y0) = (((c.w() - w) / 2.0).round(), ((c.h() - h) / 2.0).round());
        glass(c, x0, y0, w, h, 30.0, 0.72);
        let (rx, ry) = (x0 + 12.0, y0 + (h - 108.0) / 2.0 - 9.0);
        for (i, a) in self.apps.iter().enumerate() {
            let cx0 = rx + i as f32 * 112.0;
            if i == self.sel {
                crate::desktop::rect(c, cx0, ry, 108.0, 108.0, 22.0, Rgba(1.0, 1.0, 1.0, 0.16));
                crate::desktop::border(c, cx0, ry, 108.0, 108.0, 22.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.14));
            }
            let pm = cx.imgs.get(&a.icon, 84, cx.icons).or_else(|| cx.imgs.get("application-x-executable", 84, cx.icons));
            if let Some(pm) = pm {
                crate::img::draw(c, &pm, cx0 + 12.0, ry + 12.0, 84.0, 84.0, 0.0, false, 1.0);
            }
            // a dot for apps only in the minimized drawer
            if a.wins.iter().all(|w| w.1 == "special:minimized") {
                crate::desktop::circle(c, cx0 + 54.0, ry + 108.0 - 4.0 - 2.5, 2.5, T3);
            }
            self.cells.push((cx0, ry, 108.0, 108.0));
        }
        if let Some(a) = self.apps.get(self.sel) {
            let lh13 = lh(cx.t, 13.0, 550.0);
            txt_c(c, cx.t, x0 + w / 2.0, y0 + h - 9.0 - lh13, &a.name, 13.0, 550.0, T1, w - 20.0);
        }
        glass_rim(c, x0, y0, w, h, 30.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let hit = self.cells.iter().position(|r| inside((x, y), *r));
        match ev {
            Ptr::Motion => match hit {
                Some(i) if i != self.sel => {
                    self.sel = i;
                    Act::Redraw
                }
                _ => Act::None,
            },
            Ptr::Release(_) => match hit {
                Some(i) => {
                    self.sel = i;
                    self.commit()
                }
                None => Act::Close,
            },
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape => Act::Close,
            Keysym::Return | Keysym::KP_Enter => self.commit(),
            Keysym::Right | Keysym::Tab => {
                self.step(1);
                Act::Redraw
            }
            Keysym::Left | Keysym::ISO_Left_Tab => {
                self.step(-1);
                Act::Redraw
            }
            Keysym::q | Keysym::Q => self.quit_app(),
            _ => Act::None,
        }
    }
    fn key_release(&mut self, ev: &KeyEvent) -> Act {
        match ev.keysym {
            Keysym::Super_L | Keysym::Super_R | Keysym::Meta_L | Keysym::Meta_R => self.commit(),
            _ => Act::None,
        }
    }
    fn as_switcher(&mut self) -> Option<&mut Switcher> {
        Some(self)
    }
}
