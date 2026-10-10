// mist-ui Wayland side: one bar layer surface per monitor (wl_shm, drawn on the CPU), pointer input with
// the hand cursor, hover / press fades (120 ms, BarItem.qml Behaviors), actions (popups still live in
// Quickshell: they open over its IPC), and native tray menus (dbusmenu) in an Overlay layer surface with a
// click catcher behind it (1x1 buffer stretched by wp_viewporter, so no screen-sized buffer).
use crate::apps::{Apps, Icons};
use crate::bar::{self, BarState, Hit, Item, TrayItem};
use crate::gfx::{Canvas, Rgba};
use crate::sys::{Hypr, Msg};
use crate::text::{Style, Text};
use crate::theme::*;
use crate::tray::{self, MenuEntry, Req};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use std::time::{Duration, Instant};
use wayland_client::{
    globals::GlobalList,
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, Proxy, QueueHandle,
};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

const FADE: f32 = 120.0; // ms

pub struct BarSurf {
    output: wl_output::WlOutput,
    name: String,
    layer: LayerSurface,
    w: u32,
    configured: bool,
    dirty: bool,
    items: Vec<Item>,
    hover: Option<Hit>,
    pressed: Option<Hit>,
    stats_open: bool,
    left_end: f32,
    canvas: Canvas,
}

struct Menu {
    item: String,
    entries: Vec<MenuEntry>,
    layer: LayerSurface,
    catcher: LayerSurface,
    _viewport: Option<wp_viewport::WpViewport>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    hover: Option<usize>,
    configured: bool,
    catcher_configured: bool,
    dirty: bool,
    canvas: Canvas,
}

pub struct App {
    pub registry: RegistryState,
    pub outputs: OutputState,
    pub seats: SeatState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub shm: Shm,
    pub pool: SlotPool,
    pub cursor_mgr: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub viewporter: Option<wp_viewporter::WpViewporter>,
    pub pointer: Option<wl_pointer::WlPointer>,
    cursor_dev: Option<wp_cursor_shape_device_v1::WpCursorShapeDeviceV1>,
    pub bars: Vec<BarSurf>,
    menu: Option<Menu>,
    pub text: Text,
    pub icons: Icons,
    pub apps: Apps,
    pub st: BarState,
    pub hypr: Hypr,
    pub popup: (String, String),
    pub tray_items: Vec<tray::Item>,
    pub tray_req: std::sync::mpsc::Sender<Req>,
    // animated opacity per (bar index, hit)
    fades: Vec<(usize, Hit, f32, f32, Instant)>, // bar, hit, from, to, start
    pending_menu: Option<(String, usize, i32, i32)>, // tray item, bar, x, y of the menu asked for
    pub qh: QueueHandle<App>,
    pub test: bool,
}

impl App {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<App>, tray_req: std::sync::mpsc::Sender<Req>, test: bool) -> App {
        let compositor = CompositorState::bind(globals, qh).expect("wl_compositor");
        let layer_shell = LayerShell::bind(globals, qh).expect("layer shell");
        let shm = Shm::bind(globals, qh).expect("wl_shm");
        let pool = SlotPool::new(1920 * 40 * 4, &shm).expect("shm pool");
        let cursor_mgr = globals.bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(qh, 1..=1, ()).ok();
        let viewporter = globals.bind::<wp_viewporter::WpViewporter, _, _>(qh, 1..=1, ()).ok();
        App {
            registry: RegistryState::new(globals),
            outputs: OutputState::new(globals, qh),
            seats: SeatState::new(globals, qh),
            compositor,
            layer_shell,
            shm,
            pool,
            cursor_mgr,
            viewporter,
            pointer: None,
            cursor_dev: None,
            bars: Vec::new(),
            menu: None,
            text: Text::new(),
            icons: Icons::new(),
            apps: Apps::load(),
            st: BarState { net: "ethernet".into(), ..Default::default() },
            hypr: Hypr::default(),
            popup: (String::new(), String::new()),
            tray_items: Vec::new(),
            tray_req,
            fades: Vec::new(),
            pending_menu: None,
            qh: qh.clone(),
            test,
        }
    }

    fn add_bar(&mut self, output: wl_output::WlOutput) {
        let Some(info) = self.outputs.info(&output) else { return };
        let name = info.name.clone().unwrap_or_default();
        if self.test && name != "HDMI-A-1" {
            return;
        }
        if self.bars.iter().any(|b| b.name == name) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, if self.test { Layer::Overlay } else { Layer::Top }, Some("mist-bar"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, bar::HEIGHT);
        layer.set_exclusive_zone(if self.test { 0 } else { bar::HEIGHT as i32 });
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        self.bars.push(BarSurf { output, name, layer, w: 0, configured: false, dirty: true, items: Vec::new(), hover: None, pressed: None, stats_open: false, left_end: 0.0, canvas: Canvas::new(1, 1) });
    }

    // the per-monitor view of the shared state
    fn bar_state(&self, b: &BarSurf) -> BarState {
        let mut s = self.st.clone();
        s.primary = b.name == "DP-3";
        let mon = self.hypr.monitors.iter().find(|m| m.1 == b.name);
        let bare = mon.map(|m| self.hypr.ws_windows.iter().find(|w| w.0 == m.2).map(|w| w.1 == 0).unwrap_or(true)).unwrap_or(true);
        let focused_here = match (&self.hypr.active, mon) {
            (Some(a), Some(m)) => a.monitor == m.0,
            _ => false,
        };
        s.show_win = !bare && focused_here;
        if let Some(a) = &self.hypr.active {
            s.app = self.apps.lookup(&a.class).map(|e| e.name.clone()).unwrap_or(a.class.clone());
            s.title = a.title.clone();
        }
        if !s.primary {
            s.phone = None;
        }
        s.control_open = self.popup.0 == "control";
        s.tray = self.tray_items.iter().map(|t| TrayItem { id: t.key.clone(), icon: None }).collect();
        s
    }

    pub fn mark_all(&mut self) {
        for b in &mut self.bars {
            b.dirty = true;
        }
    }

    fn fade_value(&self, bi: usize, hit: &Hit, target: f32) -> (f32, bool) {
        if let Some(f) = self.fades.iter().find(|f| f.0 == bi && &f.1 == hit) {
            let t = (f.4.elapsed().as_secs_f32() * 1000.0 / FADE).min(1.0);
            return (f.2 + (f.3 - f.2) * t, t < 1.0);
        }
        (target, false)
    }

    pub fn draw_all(&mut self) {
        let now_anim = self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE);
        for i in 0..self.bars.len() {
            if self.bars[i].configured && (self.bars[i].dirty || now_anim) {
                self.draw_bar(i);
            }
        }
        self.fades.retain(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE + 50.0);
        if self.menu.as_ref().map(|m| m.configured && m.dirty).unwrap_or(false) {
            self.draw_menu();
        }
    }

    pub fn animating(&self) -> bool {
        self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE)
    }

    fn draw_bar(&mut self, i: usize) {
        let st = self.bar_state(&self.bars[i]);
        let w = self.bars[i].w;
        if self.bars[i].canvas.pm.width() != w {
            self.bars[i].canvas = Canvas::new(w, bar::HEIGHT);
        }
        // hover / press opacity through the fades (BarItem: 0.85 rest, 1 hover, 0.6 pressed)
        let hover = self.bars[i].hover.clone();
        let pressed = self.bars[i].pressed.clone();
        let stats_open = self.bars[i].stats_open;
        let fades: Vec<(Hit, f32)> = self.fades.iter().filter(|f| f.0 == i).map(|f| {
            let t = (f.4.elapsed().as_secs_f32() * 1000.0 / FADE).min(1.0);
            (f.1.clone(), f.2 + (f.3 - f.2) * t)
        }).collect();
        let tray: Vec<tray::Item> = self.tray_items.clone();
        let icons = &mut self.icons;
        let mut canvas = std::mem::replace(&mut self.bars[i].canvas, Canvas::new(1, 1));
        let mut tray_px = |c: &mut Canvas, ti: &TrayItem, cx: f32, cy: f32, op: f32| {
            let Some(t) = tray.iter().find(|t| t.key == ti.id) else { return };
            draw_tray_icon(c, icons, t, cx, cy, op);
        };
        let items = bar::draw_with(&mut canvas, &mut self.text, &st, hover.as_ref(), pressed.as_ref(), stats_open, &mut tray_px, &fades);
        self.bars[i].canvas = canvas;
        self.bars[i].left_end = items.iter().filter(|it| it.hit == Hit::Start).map(|it| it.x + it.w).next().unwrap_or(40.0) + 500.0;
        self.bars[i].items = items;
        self.bars[i].dirty = false;
        // copy into a shm buffer and commit
        let (wi, hi) = (w as i32, bar::HEIGHT as i32);
        let Ok((buffer, data)) = self.pool.create_buffer(wi, hi, wi * 4, wl_shm::Format::Argb8888) else { return };
        self.bars[i].canvas.to_bgra(data);
        let s = self.bars[i].layer.wl_surface();
        s.damage_buffer(0, 0, wi, hi);
        let _ = buffer.attach_to(s);
        self.bars[i].layer.commit();
    }

    // ---------------------------------------------------------------- pointer
    fn bar_at(&self, s: &wl_surface::WlSurface) -> Option<usize> {
        self.bars.iter().position(|b| b.layer.wl_surface() == s)
    }
    fn hit_at(b: &BarSurf, x: f64) -> Option<Hit> {
        b.items.iter().find(|it| x as f32 >= it.x && (x as f32) < it.x + it.w).map(|it| it.hit.clone())
    }
    fn set_hover(&mut self, i: usize, h: Option<Hit>) {
        let old = self.bars[i].hover.clone();
        if old == h {
            return;
        }
        let pressed = self.bars[i].pressed.clone();
        for (hit, from, to) in [(old.clone(), 1.0, 0.85), (h.clone(), 0.85, 1.0)] {
            if let Some(hit) = hit {
                let cur = self.fade_value(i, &hit, from).0;
                if pressed.as_ref() != Some(&hit) {
                    self.fades.retain(|f| !(f.0 == i && f.1 == hit));
                    self.fades.push((i, hit, cur, to, Instant::now()));
                }
            }
        }
        // the stats group opens while hovered (HoverHandler on the capsule)
        let so = matches!(h, Some(Hit::Stats));
        if so != self.bars[i].stats_open {
            self.bars[i].stats_open = so;
        }
        self.bars[i].hover = h;
        self.bars[i].dirty = true;
    }

    fn pointer_event(&mut self, e: &PointerEvent) {
        if let Some(m) = &self.menu {
            if &e.surface == m.layer.wl_surface() || &e.surface == m.catcher.wl_surface() {
                self.menu_pointer(e);
                return;
            }
        }
        let Some(i) = self.bar_at(&e.surface) else { return };
        let (x, _y) = e.position;
        match e.kind {
            PointerEventKind::Enter { serial } => {
                if let (Some(mgr), Some(p)) = (&self.cursor_mgr, &self.pointer) {
                    if self.cursor_dev.is_none() {
                        self.cursor_dev = Some(mgr.get_pointer(p, &self.qh, ()));
                    }
                    if let Some(d) = &self.cursor_dev {
                        d.set_shape(serial, wp_cursor_shape_device_v1::Shape::Pointer);
                    }
                }
                let h = Self::hit_at(&self.bars[i], x);
                self.set_hover(i, h);
            }
            PointerEventKind::Leave { .. } => {
                self.set_hover(i, None);
                self.bars[i].pressed = None;
            }
            PointerEventKind::Motion { .. } => {
                let h = Self::hit_at(&self.bars[i], x);
                self.set_hover(i, h);
            }
            PointerEventKind::Press { button, .. } => {
                if let Some(h) = Self::hit_at(&self.bars[i], x) {
                    self.fades.retain(|f| !(f.0 == i && f.1 == h));
                    self.fades.push((i, h.clone(), 1.0, 0.6, Instant::now()));
                    self.bars[i].pressed = Some(h);
                    let _ = button;
                }
                self.bars[i].dirty = true;
            }
            PointerEventKind::Release { button, .. } => {
                let h = Self::hit_at(&self.bars[i], x);
                if let Some(p) = self.bars[i].pressed.take() {
                    self.fades.retain(|f| !(f.0 == i && f.1 == p));
                    self.fades.push((i, p.clone(), 0.6, if h.as_ref() == Some(&p) { 1.0 } else { 0.85 }, Instant::now()));
                    if h.as_ref() == Some(&p) {
                        let item = self.bars[i].items.iter().find(|it| it.hit == p).map(|it| (it.x, it.w)).unwrap_or((0.0, 0.0));
                        self.action(i, &p, button, item);
                    }
                }
                self.bars[i].dirty = true;
            }
            PointerEventKind::Axis { vertical, .. } => {
                let dy = if vertical.discrete != 0 { vertical.discrete as f64 } else { vertical.absolute };
                if dy == 0.0 {
                    return;
                }
                let up = dy < 0.0;
                match Self::hit_at(&self.bars[i], x) {
                    Some(Hit::Volume) => {
                        let mut cmd = String::new();
                        if self.st.muted && up {
                            cmd.push_str("wpctl set-mute @DEFAULT_AUDIO_SINK@ 0; ");
                        }
                        cmd.push_str(if up { "wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+" } else { "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-" });
                        sh(&cmd);
                    }
                    _ if (x as f32) < self.bars[i].left_end => {
                        hypr(&format!("hl.dispatch(hl.dsp.focus({{ workspace = \"{}\" }}))", if up { "m-1" } else { "m+1" }));
                    }
                    _ => {}
                }
            }
        }
    }

    fn action(&mut self, i: usize, h: &Hit, button: u32, item: (f32, f32)) {
        let scr = self.bars[i].name.clone();
        let mid = button == BTN_MIDDLE;
        let right = button == BTN_RIGHT;
        let _ = BTN_LEFT;
        match h {
            Hit::Start => ipc(&["toggleOn", "start", &scr]),
            Hit::Stats => spawn(&["kitty", "-e", "btop"]),
            Hit::Phone => if mid { ipc(&["phoneRing"]) } else { ipc(&["toggleOn", "phone", &scr]) },
            Hit::Bluetooth => ipc(&["controlOn", "bluetooth", &scr]),
            Hit::Network => ipc(&["controlOn", "network", &scr]),
            Hit::Volume => if mid { sh("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle") } else { ipc(&["controlOn", "sound", &scr]) },
            Hit::Bell => if right { ipc(&["dnd", if self.st.dnd { "false" } else { "true" }]) } else { ipc(&["toggleOn", "notifications", &scr]) },
            Hit::Adjust => ipc(&["controlOn", "", &scr]),
            Hit::Power => ipc(&["toggleOn", "session", &scr]),
            Hit::Tray(key) => {
                let it = self.tray_items.iter().find(|t| &t.key == key).cloned();
                let Some(it) = it else { return };
                if right || it.item_is_menu {
                    if it.menu.is_some() {
                        self.pending_menu = Some((key.clone(), i, item.0 as i32, bar::HEIGHT as i32 - 2));
                        let _ = self.tray_req.send(Req::Menu(key.clone()));
                    }
                } else if mid {
                    let _ = self.tray_req.send(Req::Secondary(key.clone()));
                } else {
                    let _ = self.tray_req.send(Req::Activate(key.clone(), item.0 as i32, 0));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- tray menus
impl App {
    pub fn open_menu(&mut self, key: String, entries: Vec<MenuEntry>) {
        let Some((pk, bi, x, y)) = self.pending_menu.take() else { return };
        if pk != key || entries.is_empty() {
            return;
        }
        self.close_menu();
        let output = self.bars[bi].output.clone();
        let (w, h) = menu_size(&mut self.text, &entries);
        let bw = self.bars[bi].w as i32;
        let x = x.min(bw - w as i32 - 6).max(6);
        // catcher first (below the menu in the same layer)
        let cs = self.compositor.create_surface(&self.qh);
        let catcher = self.layer_shell.create_layer_surface(&self.qh, cs, Layer::Overlay, Some("mist-menu-catcher"), Some(&output));
        catcher.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        catcher.set_exclusive_zone(-1);
        catcher.set_keyboard_interactivity(KeyboardInteractivity::None);
        catcher.commit();
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(catcher.wl_surface(), &self.qh, ()));
        let ms = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, ms, Layer::Overlay, Some("mist-popup"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(y, 0, 0, x);
        layer.set_size(w, h);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        layer.commit();
        self.menu = Some(Menu { item: key, entries, layer, catcher, _viewport: viewport, x, y, w, h, hover: None, configured: false, catcher_configured: false, dirty: true, canvas: Canvas::new(w, h) });
    }
    pub fn close_menu(&mut self) {
        self.menu = None;
    }
    fn menu_rows(m: &Menu) -> Vec<(usize, f32, f32)> {
        let mut y = 8.0;
        let mut v = Vec::new();
        for (i, e) in m.entries.iter().enumerate() {
            let h = if e.separator { 9.0 } else { 28.0 };
            v.push((i, y, h));
            y += h;
        }
        v
    }
    fn draw_menu(&mut self) {
        let Some(m) = self.menu.as_mut() else { return };
        paint_menu(&mut m.canvas, &mut self.text, &m.entries, m.hover);
        let (wi, hi) = (m.w as i32, m.h as i32);
        let Ok((buffer, data)) = self.pool.create_buffer(wi, hi, wi * 4, wl_shm::Format::Argb8888) else { return };
        m.canvas.to_bgra(data);
        let s = m.layer.wl_surface();
        s.damage_buffer(0, 0, wi, hi);
        let _ = buffer.attach_to(s);
        m.layer.commit();
        m.dirty = false;
    }
    fn menu_pointer(&mut self, e: &PointerEvent) {
        let Some(m) = self.menu.as_mut() else { return };
        let on_menu = &e.surface == m.layer.wl_surface();
        match e.kind {
            PointerEventKind::Enter { serial } => {
                if let Some(d) = &self.cursor_dev {
                    d.set_shape(serial, wp_cursor_shape_device_v1::Shape::Default);
                }
            }
            PointerEventKind::Motion { .. } | PointerEventKind::Leave { .. } => {
                let hv = if on_menu && !matches!(e.kind, PointerEventKind::Leave { .. }) {
                    Self::menu_rows(m).into_iter().find(|(i, y, h)| !m.entries[*i].separator && e.position.1 as f32 >= *y && (e.position.1 as f32) < y + h).map(|r| r.0)
                } else {
                    None
                };
                if hv != m.hover {
                    m.hover = hv;
                    m.dirty = true;
                }
            }
            PointerEventKind::Release { .. } => {
                if !on_menu {
                    self.close_menu();
                    return;
                }
                if let Some(i) = m.hover {
                    let en = &m.entries[i];
                    if en.enabled && en.submenu.is_empty() {
                        let _ = self.tray_req.send(Req::Click(m.item.clone(), en.id));
                        self.close_menu();
                    } else if !en.submenu.is_empty() {
                        // open the submenu in place (one level deep, like a sheet)
                        let sub = en.submenu.clone();
                        m.entries = sub;
                        m.hover = None;
                        m.dirty = true;
                    }
                }
            }
            _ => {}
        }
    }
}

// Mist context menu (CtxMenu.qml look): dark glass sheet, 12 px corners, hairline rim, teal highlight
pub fn menu_size(t: &mut Text, entries: &[MenuEntry]) -> (u32, u32) {
    let st = Style::ui(13.0, 500.0);
    let mut w: f32 = 160.0;
    let mut h = 8.0;
    for e in entries {
        if e.separator { h += 9.0 } else { w = w.max(t.width(&e.label, &st) + 56.0); h += 28.0 }
    }
    (w.ceil() as u32, (h + 8.0) as u32)
}
pub fn paint_menu(c: &mut Canvas, t: &mut Text, entries: &[MenuEntry], hover: Option<usize>) {
    c.clear();
    let (w, h) = (c.w(), c.h());
    c.rrect(0.0, 0.0, w, h, 12.0, Rgba::rgb8(24, 30, 43, 0.96));
    stroke_rrect(c, 0.5, 0.5, w - 1.0, h - 1.0, 11.5, Rgba(1.0, 1.0, 1.0, 0.09));
    let st = Style::ui(13.0, 500.0);
    let mut y = 8.0;
    for (i, e) in entries.iter().enumerate() {
        let rh = if e.separator { 9.0 } else { 28.0 };
        if e.separator {
            c.rrect(12.0, y + 4.0, w - 24.0, 1.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.08));
            y += rh;
            continue;
        }
        let hov = hover == Some(i) && e.enabled;
        if hov {
            c.rrect(6.0, y + 1.0, w - 12.0, rh - 2.0, 8.0, Rgba::rgb8(138, 214, 208, 0.9));
        }
        let fg = if !e.enabled { T3 } else if hov { Rgba::hex("#0b1118") } else { T1 };
        if e.toggle == Some(true) {
            c.icon(t, 20.0, y + rh / 2.0, "tick", 14.0, fg);
        }
        c.text(t, 36.0, y + rh / 2.0, &e.label, &st, fg);
        if !e.submenu.is_empty() {
            c.icon(t, w - 18.0, y + rh / 2.0, "right", 13.0, fg.alpha(0.7));
        }
        y += rh;
    }
}

fn stroke_rrect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32, col: Rgba) {
    if let Some(p) = crate::gfx::rrect_path(x, y, w, h, r) {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba(col.0, col.1, col.2, col.3).unwrap());
        paint.anti_alias = true;
        c.pm.stroke_path(&p, &paint, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, tiny_skia::Transform::identity(), None);
    }
}

fn draw_tray_icon(c: &mut Canvas, icons: &mut Icons, t: &tray::Item, cx: f32, cy: f32, op: f32) {
    // IconImage implicitSize 16, opacity 0.85 / 1 hovered
    let size = 16u32;
    let img = if let Some((w, h, px)) = &t.pixmap {
        Some((*w, *h, px.clone()))
    } else {
        let name = if !t.theme_path.is_empty() {
            let p = format!("{}/{}.png", t.theme_path, t.icon_name);
            if std::path::Path::new(&p).exists() { p } else { t.icon_name.clone() }
        } else {
            t.icon_name.clone()
        };
        icons.image(&name, size * 2).map(|i| (i.w, i.h, i.rgba.clone()))
    };
    let Some((w, h, px)) = img else { return };
    let Some(src) = tiny_skia::Pixmap::from_vec(px, tiny_skia::IntSize::from_wh(w, h).unwrap()) else { return };
    let k = size as f32 / w.max(h) as f32;
    let paint = tiny_skia::PixmapPaint { opacity: op, quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
    c.pm.draw_pixmap(0, 0, src.as_ref(), &paint, tiny_skia::Transform::from_row(k, 0.0, 0.0, k, cx - w as f32 * k / 2.0, cy - h as f32 * k / 2.0), None);
}

// ---------------------------------------------------------------- commands
fn spawn(argv: &[&str]) {
    use std::os::unix::process::CommandExt;
    let mut c = std::process::Command::new(argv[0]);
    c.args(&argv[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).process_group(0);
    if let Ok(mut ch) = c.spawn() {
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}
fn sh(cmd: &str) {
    spawn(&["sh", "-c", cmd]);
}
fn ipc(args: &[&str]) {
    let home = std::env::var("HOME").unwrap_or_default();
    let qs = format!("{home}/.local/bin/mist-qs");
    let dir = format!("{home}/.config/quickshell/mist");
    let mut v: Vec<&str> = vec![&qs, "-p", &dir, "ipc", "call", "mist"];
    v.extend_from_slice(args);
    spawn(&v);
}
fn hypr(lua: &str) {
    spawn(&["hyprctl", "eval", lua]);
}

// ---------------------------------------------------------------- messages from the data threads
impl App {
    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Hypr(h) => self.hypr = h,
            Msg::Net(n) => self.st.net = n,
            Msg::Bt(on, dev) => {
                self.st.bt_on = on;
                self.st.bt_dev = dev;
            }
            Msg::Volume(v, m) => {
                self.st.vol = v;
                self.st.muted = m;
            }
            Msg::Stats(c, mu, mt, rx) => {
                self.st.cpu = c;
                self.st.mem = mu;
                self.st.mem_total = mt;
                self.st.rx = rx;
            }
            Msg::Phone(p) => self.st.phone = p,
            Msg::Ui(p, s) => self.popup = (p, s),
            Msg::Dnd(d) => self.st.dnd = d,
            Msg::Tray(t) => self.tray_items = t,
        }
        self.mark_all();
    }
}

// ---------------------------------------------------------------- sctk plumbing
impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.add_bar(o);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.bars.retain(|b| b.output != o);
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface) {
        self.bars.retain(|b| &b.layer != l);
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        if let Some(b) = self.bars.iter_mut().find(|b| &b.layer == l) {
            b.w = c.new_size.0.max(1);
            b.configured = true;
            b.dirty = true;
            return;
        }
        if let Some(m) = self.menu.as_mut() {
            if &m.layer == l {
                m.configured = true;
                m.dirty = true;
            } else if &m.catcher == l {
                m.catcher_configured = true;
                // 1x1 transparent buffer stretched over the screen
                let (cw, ch) = (c.new_size.0.max(1) as i32, c.new_size.1.max(1) as i32);
                if let Some(vp) = &m._viewport {
                    vp.set_destination(cw, ch);
                    if let Ok((buf, data)) = self.pool.create_buffer(1, 1, 4, wl_shm::Format::Argb8888) {
                        data.copy_from_slice(&[0, 0, 0, 0]);
                        let s = m.catcher.wl_surface();
                        let _ = buf.attach_to(s);
                        s.damage_buffer(0, 0, 1, 1);
                    }
                }
                m.catcher.commit();
            }
        }
    }
}
impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
            self.cursor_dev = None;
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            self.pointer_event(e);
        }
    }
}
impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
wayland_client::delegate_noop!(App: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
wayland_client::delegate_noop!(App: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
wayland_client::delegate_noop!(App: ignore wp_viewporter::WpViewporter);
wayland_client::delegate_noop!(App: ignore wp_viewport::WpViewport);

pub fn tick_interval() -> Duration {
    Duration::from_millis(16)
}
