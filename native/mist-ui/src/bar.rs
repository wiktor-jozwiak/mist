// Top bar (TopBar.qml 1:1): transparent, icons and text only; the centre stays free for the Dynamic Island.
//   left   [Start] app name (bold) · window title
//   right  [CPU · RAM · download] [tray] [phone · Bluetooth · network · volume · notifications · adjust] [power]
use crate::gfx::{Canvas, Rgba};
use crate::text::{Style, Text};
use crate::theme::*;

pub const HEIGHT: u32 = 34;

#[derive(Clone, Default)]
pub struct TrayItem {
    pub id: String,
}

#[derive(Clone, Default)]
pub struct BarState {
    pub primary: bool,
    pub app: String,
    pub title: String,
    pub show_win: bool,
    pub cpu: i32,
    pub mem: f32,
    pub mem_total: f32,
    pub rx: f64,
    pub tray: Vec<TrayItem>,
    pub phone: Option<Phone>,
    pub bt_on: bool,
    pub bt_dev: Option<Option<f32>>, // a connected device (its battery 0..1 when known)
    pub net: String,                 // ethernet | wifi | none
    pub vol: f32,
    pub muted: bool,
    pub dnd: bool,
    pub control_open: bool,
}

#[derive(Clone, Default)]
pub struct Phone {
    pub pair: bool,
    pub charging: bool,
    pub battery: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Start,
    Stats,
    Tray(String),
    Phone,
    Bluetooth,
    Network,
    Volume,
    Bell,
    Adjust,
    Power,
}

pub struct Item {
    pub hit: Hit,
    pub x: f32,
    pub w: f32,
}

pub fn bytes(b: f64) -> String {
    if b < 1024.0 {
        format!("{} B", b.round())
    } else if b < 1048576.0 {
        format!("{:.1} KB", b / 1024.0).replace('.', ",")
    } else {
        format!("{:.1} MB", b / 1048576.0).replace('.', ",")
    }
}

struct Btn {
    hit: Hit,
    icon: &'static str,
    text: String,
    tint: Rgba,
    icon_size: f32,
    fixed_w: Option<f32>,
}

// BarItem.qml: Row(icon, 5, text) centred, width = row + 12, opacity 0.85 at rest / 1 hovered / 0.6 pressed
fn btn_w(t: &mut Text, b: &Btn) -> f32 {
    if let Some(w) = b.fixed_w {
        return w;
    }
    let mut w = Canvas::icon_w(t, b.icon, b.icon_size);
    if !b.text.is_empty() {
        w += 5.0 + t.width(&b.text, &Style::ui(13.0, 550.0).tnum());
    }
    w + 12.0
}
fn draw_btn(c: &mut Canvas, t: &mut Text, b: &Btn, x: f32, w: f32, hover: bool, pressed: bool, fade: Option<f32>) {
    let op = fade.unwrap_or(if pressed { 0.6 } else if hover { 1.0 } else { 0.85 });
    let iw = Canvas::icon_w(t, b.icon, b.icon_size);
    let tw = if b.text.is_empty() { 0.0 } else { 5.0 + t.width(&b.text, &Style::ui(13.0, 550.0).tnum()) };
    let mut rx = x + (w - iw - tw) / 2.0;
    let cy = HEIGHT as f32 / 2.0;
    c.icon(t, rx + iw / 2.0, cy, b.icon, b.icon_size, b.tint.alpha(op));
    rx += iw + 5.0;
    if !b.text.is_empty() {
        c.text(t, rx, cy, &b.text, &Style::ui(13.0, 550.0).tnum(), b.tint.alpha(op));
    }
}

pub fn draw(c: &mut Canvas, t: &mut Text, s: &BarState, hover: Option<&Hit>, pressed: Option<&Hit>, stats_open: bool, tray_px: &dyn Fn(&mut Canvas, &TrayItem, f32, f32, f32)) -> Vec<Item> {
    draw_with(c, t, s, hover, pressed, stats_open, &mut |c, ti, x, y, o| tray_px(c, ti, x, y, o), &[])
}

// fades: opacity of items in the middle of a hover / press transition
pub fn draw_with(c: &mut Canvas, t: &mut Text, s: &BarState, hover: Option<&Hit>, pressed: Option<&Hit>, stats_open: bool, tray_px: &mut dyn FnMut(&mut Canvas, &TrayItem, f32, f32, f32), fades: &[(Hit, f32)]) -> Vec<Item> {
    c.clear();
    let fade = |x: &Hit| fades.iter().find(|f| &f.0 == x).map(|f| f.1);
    let mut items = Vec::new();
    let cy = HEIGHT as f32 / 2.0;
    let h = |x: &Hit| hover == Some(x);
    let p = |x: &Hit| pressed == Some(x);

    // ---------------- left: Start, app name, title (spacing 14)
    let start = Btn { hit: Hit::Start, icon: "apps", text: String::new(), tint: T1, icon_size: 16.0, fixed_w: Some(26.0) };
    let mut x = 10.0;
    draw_btn(c, t, &start, x, 26.0, h(&start.hit), p(&start.hit), fade(&start.hit));
    items.push(Item { hit: Hit::Start, x, w: 26.0 });
    x += 26.0 + 14.0;
    let name = if s.show_win { s.app.as_str() } else { "Pulpit" };
    x += c.text(t, x, cy, name, &Style::ui(13.0, 700.0), T1) + 14.0;
    if s.show_win && !s.title.is_empty() {
        let st = Style::ui(13.0, 500.0);
        let title = elide(t, &s.title, &st, 420.0);
        c.text(t, x, cy, &title, &st, T2);
    }

    // ---------------- right (anchored 8 px from the edge), groups with 2 px between items
    let mut groups: Vec<Vec<Btn>> = Vec::new();
    if s.primary {
        let cpu_hot = s.cpu > 40;
        let mem_hot = s.mem_total > 0.0 && s.mem / s.mem_total > 0.75;
        let net_hot = s.rx > 1048576.0;
        let mut g = Vec::new();
        if !stats_open && !cpu_hot && !mem_hot && !net_hot {
            g.push(Btn { hit: Hit::Stats, icon: "activity", text: String::new(), tint: T2, icon_size: 15.0, fixed_w: None });
        }
        if stats_open || cpu_hot {
            let tint = if s.cpu > 85 { DANGER } else if s.cpu > 55 { YELLOW } else { T2 };
            g.push(Btn { hit: Hit::Stats, icon: "cpu", text: format!("{}%", s.cpu), tint, icon_size: 15.0, fixed_w: None });
        }
        if stats_open || mem_hot {
            let tint = if s.mem_total > 0.0 && s.mem / s.mem_total > 0.85 { DANGER } else { T2 };
            g.push(Btn { hit: Hit::Stats, icon: "ram", text: format!("{:.1} GB", s.mem).replace('.', ","), tint, icon_size: 15.0, fixed_w: None });
        }
        if stats_open || net_hot {
            g.push(Btn { hit: Hit::Stats, icon: "down", text: bytes(s.rx) + "/s", tint: T2, icon_size: 15.0, fixed_w: None });
        }
        groups.push(g);
    }
    let tray_start = groups.len();
    if !s.tray.is_empty() {
        groups.push(s.tray.iter().map(|ti| Btn { hit: Hit::Tray(ti.id.clone()), icon: "", text: String::new(), tint: T1, icon_size: 16.0, fixed_w: Some(26.0) }).collect());
    }
    let mut g = Vec::new();
    if let Some(ph) = &s.phone {
        let tint = if ph.pair { YELLOW } else { phone_color(ph) };
        let text = if ph.battery >= 0 && (ph.battery <= 30 || ph.charging) { format!("{}%", ph.battery) } else { String::new() };
        g.push(Btn { hit: Hit::Phone, icon: if ph.pair { "mobileShare" } else if ph.charging { "charging" } else { "mobile" }, text, tint, icon_size: 15.0, fixed_w: None });
    }
    if s.bt_on {
        let (icon, tint, text) = match s.bt_dev {
            Some(b) => ("btOn", ACCENT, b.filter(|v| *v <= 0.3).map(|v| format!("{}%", (v * 100.0).round())).unwrap_or_default()),
            None => ("bluetooth", T2, String::new()),
        };
        g.push(Btn { hit: Hit::Bluetooth, icon, text, tint, icon_size: 15.0, fixed_w: None });
    }
    g.push(Btn {
        hit: Hit::Network,
        icon: match s.net.as_str() { "ethernet" => "ethernet", "wifi" => "wifi", _ => "worldOff" },
        text: String::new(),
        tint: if s.net == "none" { DANGER } else { T1 },
        icon_size: 15.0,
        fixed_w: None,
    });
    g.push(Btn {
        hit: Hit::Volume,
        icon: if s.muted || s.vol == 0.0 { "volumeOff" } else if s.vol < 0.4 { "volLow" } else { "volume" },
        text: String::new(),
        tint: if s.muted { T3 } else { T1 },
        icon_size: 15.0,
        fixed_w: None,
    });
    g.push(Btn { hit: Hit::Bell, icon: if s.dnd { "bellOff" } else { "bell" }, text: String::new(), tint: if s.dnd { T3 } else { T1 }, icon_size: 15.0, fixed_w: None });
    g.push(Btn { hit: Hit::Adjust, icon: "adjust", text: String::new(), tint: if s.control_open { ACCENT } else { T1 }, icon_size: 15.0, fixed_w: None });
    groups.push(g);
    groups.push(vec![Btn { hit: Hit::Power, icon: "power", text: String::new(), tint: DANGER, icon_size: 15.0, fixed_w: Some(24.0) }]);

    // widths right to left
    let mut layout: Vec<(usize, Vec<f32>)> = Vec::new();
    let mut total = 0.0;
    for (gi, g) in groups.iter().enumerate() {
        let ws: Vec<f32> = g.iter().map(|b| btn_w(t, b)).collect();
        let gw: f32 = ws.iter().sum::<f32>() + 2.0 * (ws.len().saturating_sub(1)) as f32;
        total += gw;
        layout.push((gi, ws));
    }
    let mut x = c.w() - 8.0 - total;
    for (gi, ws) in layout {
        for (b, w) in groups[gi].iter().zip(ws.iter()) {
            if gi == tray_start && !s.tray.is_empty() {
                if let Hit::Tray(id) = &b.hit {
                    if let Some(ti) = s.tray.iter().find(|t| &t.id == id) {
                        let op = fade(&b.hit).unwrap_or(if h(&b.hit) { 1.0 } else { 0.85 });
                        tray_px(c, ti, x + w / 2.0, cy, op);
                    }
                }
            } else {
                draw_btn(c, t, b, x, *w, h(&b.hit), p(&b.hit), fade(&b.hit));
            }
            items.push(Item { hit: b.hit.clone(), x, w: *w });
            x += w + 2.0;
        }
        x -= 2.0;
    }
    items
}

pub fn phone_color(p: &Phone) -> Rgba {
    if p.charging {
        GREEN
    } else if p.battery >= 0 && p.battery <= 15 {
        DANGER
    } else if p.battery <= 30 {
        YELLOW
    } else {
        T1
    }
}

pub fn elide(t: &mut Text, s: &str, st: &Style, max: f32) -> String {
    if t.width(s, st) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if t.width(&cand, st) <= max { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>() + "…"
}
