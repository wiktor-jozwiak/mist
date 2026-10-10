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
    // what the pointer should look like (Pointer over something clickable)
    fn hand(&self) -> bool {
        false
    }
}

// the popups that are native now (the rest are forwarded to Quickshell)
pub fn native(name: &str) -> bool {
    matches!(name, "session" | "calendar" | "notifications")
}
pub fn make(name: &str, _page: &str) -> Option<Box<dyn Pane>> {
    match name {
        "session" => Some(Box::new(crate::session::Session::new())),
        "calendar" => Some(Box::new(crate::calendar::Calendar::new())),
        "notifications" => Some(Box::new(crate::notifcenter::NotifCenter::new())),
        _ => None,
    }
}

// ------------------------------------------------------------------ shared drawing (Txt / Glass / buttons)
pub fn txt(c: &mut Canvas, t: &mut Text, x: f32, y: f32, s: &str, size: f32, w: f32, col: Rgba) -> f32 {
    crate::desktop::text(c, t, x, y, s, &Style::ui(size, w), col, false, None)
}
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
