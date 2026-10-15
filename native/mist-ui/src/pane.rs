// Native popups (were the LazyLoader'ed PanelWindows of shell.qml + Ui.qml): one open at a time, on one
// monitor, a full-monitor Overlay surface (dim / click-outside-closes like HyprlandFocusGrab) with the
// keyboard while open. Each popup implements Pane; app.rs owns the surface and routes input.
// Popups not ported yet stay in Quickshell: `mist toggle <name>` is forwarded there (pane::native()).
use crate::gfx::{Canvas, Rgba};
use crate::text::{Style, Text};
use smithay_client_toolkit::seat::keyboard::KeyEvent;

pub enum Act {
    None,
    Redraw,
    Close,
    // close, then run a command (detached)
    Run(Vec<String>),
    // run a command, stay open
    Exec(Vec<String>),
    // open another popup instead
    Toggle(&'static str),
    // the screenshot toolbar: capture now
    Capture,
    // close, start an app (Apps index)
    Launch(usize),
    // a media player command, stay open
    Media(crate::media::Cmd),
    // window snapping (snap.rs)
    Snap(crate::snap::Cmd),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Ptr {
    Enter,
    Leave,
    Motion,
    Press(u32),
    Release(u32),
    Wheel(f32),
}

pub struct Ctx<'a> {
    pub t: &'a mut Text,
    pub cal: &'a serde_json::Value,
    pub icons: &'a mut crate::apps::Icons,
    pub imgs: &'a mut crate::img::Images,
    pub apps: &'a crate::apps::Apps,
    pub notif_hidden: &'a mut std::collections::HashMap<String, i64>,
    pub ctrl: bool,
    pub st: &'a crate::bar::BarState,
    pub players: &'a [crate::media::Player],
    pub phone_name: &'a str,
    pub wx: &'a serde_json::Value,
}

pub trait Pane {
    fn name(&self) -> &'static str;
    fn namespace(&self) -> &'static str {
        "mist-popup"
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx);
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, cx: &mut Ctx) -> Act;
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act;
    fn animating(&self) -> bool {
        false
    }
    // every 250 ms; true = repaint
    fn slow(&mut self) -> bool {
        false
    }
    // a data source changed ("notifications", ...); true = repaint
    fn data(&mut self, _what: &str) -> bool {
        false
    }
    // every 250 ms: a timed action (the session menu's countdown)
    fn timer(&mut self) -> Act {
        Act::None
    }
    fn key_release(&mut self, _ev: &KeyEvent) -> Act {
        Act::None
    }
    fn as_switcher(&mut self) -> Option<&mut crate::switcher::Switcher> {
        None
    }
    fn as_shot(&mut self) -> Option<&mut crate::shot::Toolbar> {
        None
    }
    // the surface is up on this monitor
    fn opened(&mut self, _screen: &str) {}
    // `--pane-png`: type this into the search field
    fn seed(&mut self, _q: &str) {}
    // what the pointer should look like (Pointer over something clickable)
    fn hand(&self) -> bool {
        false
    }
    // `controlOn <page>` while open: switch to it; false = it is already shown (close)
    fn set_page(&mut self, _p: &str) -> bool {
        false
    }
    // an event of the phone (done / err / shot ...); true = repaint
    fn phone_ev(&mut self, _e: &serde_json::Value) -> bool {
        false
    }
    // the surface: exclusive keyboard, takes the pointer, covers the monitor (else left, top, w, h)
    fn keyboard(&self) -> bool {
        true
    }
    fn input(&self) -> bool {
        true
    }
    fn rect(&self) -> Option<(i32, i32, u32, u32)> {
        None
    }
    fn as_drag(&mut self) -> Option<&mut crate::snap::Drag> {
        None
    }
    fn as_assist(&mut self) -> Option<&mut crate::snap::AssistPane> {
        None
    }
}

// the popups that are native now (the rest are forwarded to Quickshell)
pub fn native(name: &str) -> bool {
    matches!(name, "session" | "calendar" | "notifications" | "clipboard" | "spotlight" | "emoji" | "wallpaper" | "control" | "phone" | "start")
}
pub fn make(name: &str, page: &str) -> Option<Box<dyn Pane>> {
    match name {
        "session" => Some(Box::new(crate::session::Session::new())),
        "calendar" => Some(Box::new(crate::calendar::Calendar::new())),
        "notifications" => Some(Box::new(crate::notifcenter::NotifCenter::new())),
        "clipboard" => Some(Box::new(crate::clipboard::Clipboard::new())),
        "spotlight" => Some(Box::new(crate::spotlight::Spotlight::new(false))),
        "emoji" => Some(Box::new(crate::spotlight::Spotlight::new(true))),
        "wallpaper" => Some(Box::new(crate::wallpapers::Wallpapers::new())),
        "control" => Some(Box::new(crate::control::Control::new(page))),
        // app.rs opens it with the running phone (this one only reads the state file: --pane-png)
        "phone" => {
            let mut p = crate::phonepanel::PhonePanel::new(crate::phone::Phone::start_idle());
            p.set_page(page);
            Some(Box::new(p))
        }
        "start" => {
            let mut p = crate::start::Start::new(crate::phone::Phone::start_idle());
            p.set_page(page);
            Some(Box::new(p))
        }
        _ => None,
    }
}

// ------------------------------------------------------------------ shared drawing (Txt / Glass / buttons)
pub fn txt_c(c: &mut Canvas, t: &mut Text, cx: f32, y: f32, s: &str, size: f32, w: f32, col: Rgba, max: f32) -> f32 {
    let st = Style::ui(size, w);
    let sw = t.width(s, &st).min(max);
    crate::desktop::text(c, t, (cx - sw / 2.0).round(), y, s, &st, col, false, Some(max))
}
pub fn lh(t: &mut Text, size: f32, w: f32) -> f32 {
    crate::desktop::line_h(t, &Style::ui(size, w))
}
// Glass.qml (liquid): body + rim, `tint` as in QML (0..1 -> 0.3 + tint * 0.2)
pub fn glass(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, tint: f32) {
    crate::glass::Glass { radius: r, tint: 0.3 + tint * 0.2, shine: 1.15, shadow_opacity: 0.2, rims: false, ..Default::default() }.paint(c, x, y, w, h);
}
pub fn glass_rim(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32) {
    crate::glass::Glass { radius: r, rim_only: true, shine: 1.15, ..Default::default() }.paint(c, x, y, w, h);
}
pub fn dim(c: &mut Canvas) {
    let (w, h) = (c.w(), c.h());
    crate::desktop::rect(c, 0.0, 0.0, w, h, 0.0, Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.42));
}
// a 150 x 34 pill button with centred text
pub fn pill(c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32, label: &str, fill: Rgba, fg: Rgba, weight: f32) {
    crate::desktop::rect(c, x, y, w, 34.0, 17.0, fill);
    let st = Style::ui(13.5, weight);
    let lw = t.width(label, &st);
    let h = crate::desktop::line_h(t, &st);
    crate::desktop::text(c, t, x + (w - lw) / 2.0, y + (34.0 - h) / 2.0, label, &st, fg, false, None);
}
pub fn inside(p: (f32, f32), r: (f32, f32, f32, f32)) -> bool {
    p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3
}
// the user's full name (GECOS, word case normalised)
pub fn full_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let u = std::ffi::CString::new(user.clone()).unwrap_or_default();
    let pw = unsafe { libc::getpwnam(u.as_ptr()) };
    let g = if pw.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().split(',').next().unwrap_or("").trim().to_string() } };
    let g = if g.is_empty() { user } else { g };
    g.split(' ').map(|s| { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
}

// ------------------------------------------------------------------ a one-line text field (TextInput)
#[derive(Default)]
pub struct Field {
    pub text: String,
}
impl Field {
    // typing, Backspace, Ctrl+Backspace / Ctrl+W (a word), Ctrl+U (all), Ctrl+V (paste); true = changed
    pub fn key(&mut self, ev: &KeyEvent, ctrl: bool) -> bool {
        use smithay_client_toolkit::seat::keyboard::Keysym;
        match ev.keysym {
            Keysym::BackSpace if ctrl => {
                let t = self.text.trim_end().to_string();
                let cut = t.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
                self.text.truncate(cut);
                true
            }
            Keysym::BackSpace => self.text.pop().is_some(),
            Keysym::w | Keysym::W if ctrl => self.key(&KeyEvent { keysym: Keysym::BackSpace, ..ev.clone() }, true),
            Keysym::u | Keysym::U if ctrl => {
                self.text.clear();
                true
            }
            Keysym::v | Keysym::V if ctrl => {
                if let Ok(o) = std::process::Command::new("wl-paste").args(["--no-newline", "--type", "text/plain"]).output() {
                    let s = String::from_utf8_lossy(&o.stdout).replace('\n', " ");
                    self.text.push_str(&s);
                    return true;
                }
                false
            }
            _ if ctrl => false,
            _ => match &ev.utf8 {
                Some(t) if !t.chars().any(|c| c.is_control()) => {
                    self.text.push_str(t);
                    true
                }
                _ => false,
            },
        }
    }
}
