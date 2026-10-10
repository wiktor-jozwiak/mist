// mist-ui Wayland side: layer surfaces (bars on every monitor, the dock on DP-3, popup menus) drawn on the
// CPU into wl_shm buffers; pointer input with cursor shapes; actions (Quickshell still owns the popups
// that are not native yet and opens them over its IPC).
use crate::apps::{Apps, Icons};
use crate::bar::{self, BarState, Hit, Item, TrayItem};
use crate::dock::{self, Dock, Slot};
use crate::gfx::Canvas;
use crate::popup::{self, Look, Row};
use crate::sys::{Hypr, Msg};
use crate::text::Text;
use crate::tray::{self, MenuEntry, Req};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_MIDDLE, BTN_RIGHT},
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
    Connection, QueueHandle,
};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

const FADE: f32 = 120.0; // ms, BarItem opacity Behaviors
pub const PRIMARY: &str = "DP-3";

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

struct DockSurf {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    w: u32,
    out_w: i32,
    out_h: i32,
    configured: bool,
    dirty: bool,
    input_shown: Option<bool>,
    canvas: Canvas,
}

#[derive(Clone)]
enum DockAct {
    Focus(String),
    Minimize,
    NewWindow,
    TogglePin,
    Close,
}
enum Owner {
    Tray(String, Vec<MenuEntry>),
    Dock(String, Vec<Option<DockAct>>),
}
struct Popup {
    owner: Owner,
    look: Look,
    rows: Vec<Row>,
    layer: LayerSurface,
    catcher: LayerSurface,
    viewport: Option<wp_viewport::WpViewport>,
    w: u32,
    hover: Option<usize>,
    configured: bool,
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
    dock_surf: Option<DockSurf>,
    pub dock: Dock,
    popup: Option<Popup>,
    pub text: Text,
    pub icons: Icons,
    pub apps: Apps,
    pub st: BarState,
    pub hypr: Hypr,
    pub ui_popup: (String, String),
    pub tray_items: Vec<tray::Item>,
    pub tray_req: std::sync::mpsc::Sender<Req>,
    fades: Vec<(usize, Hit, f32, f32, Instant)>,
    pending_menu: Option<(String, usize, i32, i32)>,
    dock_hide_at: Option<Instant>,
    dock_show_at: Option<Instant>,
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
            dock_surf: None,
            dock: Dock::new(),
            popup: None,
            text: Text::new(),
            icons: Icons::new(),
            apps: Apps::load(),
            st: BarState { net: "ethernet".into(), ..Default::default() },
            hypr: Hypr::default(),
            ui_popup: (String::new(), String::new()),
            tray_items: Vec::new(),
            tray_req,
            fades: Vec::new(),
            pending_menu: None,
            dock_hide_at: None,
            dock_show_at: None,
            qh: qh.clone(),
            test,
        }
    }

    fn add_output(&mut self, output: wl_output::WlOutput) {
        let Some(info) = self.outputs.info(&output) else { return };
        let name = info.name.clone().unwrap_or_default();
        let (ow, oh) = info.logical_size.unwrap_or((1920, 1080));
        if self.test && name != "HDMI-A-1" {
            return;
        }
        if !self.bars.iter().any(|b| b.name == name) && std::env::var("MIST_UI_NO_BAR").is_err() {
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, if self.test { Layer::Overlay } else { Layer::Top }, Some("mist-bar"), Some(&output));
            layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
            layer.set_size(0, bar::HEIGHT);
            layer.set_exclusive_zone(if self.test { 0 } else { bar::HEIGHT as i32 });
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.bars.push(BarSurf { output: output.clone(), name: name.clone(), layer, w: 0, configured: false, dirty: true, items: Vec::new(), hover: None, pressed: None, stats_open: false, left_end: 0.0, canvas: Canvas::new(1, 1) });
        }
        let dock_here = if self.test { name == "HDMI-A-1" } else { name == PRIMARY };
        if dock_here && self.dock_surf.is_none() && std::env::var("MIST_UI_NO_DOCK").is_err() {
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, if self.test { Layer::Overlay } else { Layer::Top }, Some("mist-dock"), Some(&output));
            layer.set_anchor(Anchor::BOTTOM);
            let w = self.dock.win_w().max(200);
            layer.set_size(w, dock::WIN_H);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.dock_surf = Some(DockSurf { output, layer, w, out_w: ow, out_h: oh, configured: false, dirty: true, input_shown: None, canvas: Canvas::new(1, 1) });
        }
    }

    // the per-monitor view of the shared bar state
    fn bar_state(&self, b: &BarSurf) -> BarState {
        let mut s = self.st.clone();
        s.primary = b.name == PRIMARY;
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
        s.control_open = self.ui_popup.0 == "control";
        s.tray = self.tray_items.iter().map(|t| TrayItem { id: t.key.clone(), icon: None }).collect();
        s
    }
    fn primary_bare(&self) -> bool {
        let mon = self.hypr.monitors.iter().find(|m| m.1 == PRIMARY);
        mon.map(|m| self.hypr.ws_windows.iter().find(|w| w.0 == m.2).map(|w| w.1 == 0).unwrap_or(true)).unwrap_or(true)
    }

    pub fn mark_all(&mut self) {
        for b in &mut self.bars {
            b.dirty = true;
        }
        if let Some(d) = &mut self.dock_surf {
            d.dirty = true;
        }
    }

    pub fn animating(&self) -> bool {
        self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE) || self.dock.animating() || self.dock_hide_at.is_some() || self.dock_show_at.is_some()
    }

    // timers (dock hide 400 ms / reveal 80 ms) and animation frames
    pub fn tick(&mut self) {
        let now = Instant::now();
        if self.dock_hide_at.map(|t| now >= t).unwrap_or(false) {
            self.dock_hide_at = None;
            if !self.dock.menu_open {
                self.dock.hovered = false;
                self.dock.update_y();
            }
        }
        if self.dock_show_at.map(|t| now >= t).unwrap_or(false) {
            self.dock_show_at = None;
            self.dock.hovered = true;
            self.dock.update_y();
        }
        if self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE + 20.0) {
            for b in &mut self.bars {
                b.dirty = true;
            }
        }
        if self.dock.animating() {
            if let Some(d) = &mut self.dock_surf {
                d.dirty = true;
            }
        }
    }

    pub fn draw_all(&mut self) {
        for i in 0..self.bars.len() {
            if self.bars[i].configured && self.bars[i].dirty {
                self.draw_bar(i);
            }
        }
        self.fades.retain(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE + 50.0);
        if self.dock_surf.as_ref().map(|d| d.configured && d.dirty).unwrap_or(false) {
            self.draw_dock();
        }
        if self.popup.as_ref().map(|m| m.configured && m.dirty).unwrap_or(false) {
            self.draw_popup();
        }
    }

    fn commit(pool: &mut SlotPool, layer: &LayerSurface, canvas: &Canvas) {
        let (wi, hi) = (canvas.pm.width() as i32, canvas.pm.height() as i32);
        let Ok((buffer, data)) = pool.create_buffer(wi, hi, wi * 4, wl_shm::Format::Argb8888) else { return };
        canvas.to_bgra(data);
        let s = layer.wl_surface();
        s.damage_buffer(0, 0, wi, hi);
        let _ = buffer.attach_to(s);
        layer.commit();
    }

    // ---------------------------------------------------------------- bar
    fn draw_bar(&mut self, i: usize) {
        let st = self.bar_state(&self.bars[i]);
        let w = self.bars[i].w;
        if self.bars[i].canvas.pm.width() != w {
            self.bars[i].canvas = Canvas::new(w, bar::HEIGHT);
        }
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
        Self::commit(&mut self.pool, &self.bars[i].layer, &canvas);
        self.bars[i].canvas = canvas;
        self.bars[i].left_end = items.iter().filter(|it| it.hit == Hit::Start).map(|it| it.x + it.w).next().unwrap_or(40.0) + 500.0;
        self.bars[i].items = items;
        self.bars[i].dirty = false;
    }

    // ---------------------------------------------------------------- dock
    fn draw_dock(&mut self) {
        let Some(d) = self.dock_surf.as_mut() else { return };
        let want = self.dock.win_w().max(200);
        if want != d.w {
            // the slab grew / shrank: resize the window, draw after the configure
            d.w = want;
            d.layer.set_size(want, dock::WIN_H);
            d.layer.commit();
            d.configured = false;
            return;
        }
        if d.canvas.pm.width() != d.w {
            d.canvas = Canvas::new(d.w, dock::WIN_H);
        }
        // input: the slab while shown, else the 2 px strip at the bottom edge
        let shown = self.dock.shown();
        if d.input_shown != Some(shown) {
            if let Ok(r) = Region::new(&self.compositor) {
                let sw = self.dock.slab_w();
                let x = ((d.w as f32 - sw) / 2.0) as i32;
                let sh = (dock::SIZE + 2.0 * dock::PAD) as i32;
                if shown {
                    r.add(x, dock::WIN_H as i32 - sh - 14, sw as i32, sh + 14);
                } else {
                    r.add(x, dock::WIN_H as i32 - 2, sw as i32, 2);
                }
                d.layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
            d.input_shown = Some(shown);
        }
        self.dock.draw(&mut d.canvas, &mut self.text, &mut self.icons);
        Self::commit(&mut self.pool, &d.layer, &d.canvas);
        d.dirty = false;
    }

    fn dock_rebuild(&mut self) {
        self.dock.rebuild(&self.hypr, &self.apps);
        let bare = self.primary_bare();
        if bare != self.dock.bare {
            self.dock.bare = bare;
            if !bare {
                self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
            }
        }
        self.dock.update_y();
        self.dock.update_lift();
        if let Some(d) = &mut self.dock_surf {
            d.dirty = true;
        }
    }

    fn dock_pointer(&mut self, e: &PointerEvent) {
        let Some(d) = self.dock_surf.as_ref() else { return };
        let ww = d.w as f32;
        let (x, y) = (e.position.0 as f32, e.position.1 as f32);
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.set_cursor(serial, wp_cursor_shape_device_v1::Shape::Default);
                if !self.dock.shown() {
                    self.dock_show_at = Some(Instant::now() + Duration::from_millis(80));
                } else {
                    self.dock_hide_at = None;
                    self.dock.hovered = true;
                }
            }
            PointerEventKind::Leave { .. } => {
                self.dock_show_at = None;
                self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
                self.dock.hover_slot = None;
                self.dock.pressed_slot = None;
            }
            PointerEventKind::Motion { .. } => {
                if self.dock.shown() {
                    self.dock_hide_at = None;
                    self.dock.hovered = true;
                }
                self.dock.hover_slot = self.dock.slot_at(ww, x, y);
            }
            PointerEventKind::Press { .. } => self.dock.pressed_slot = self.dock.slot_at(ww, x, y),
            PointerEventKind::Release { button, .. } => {
                let at = self.dock.slot_at(ww, x, y);
                if let (Some(p), Some(a)) = (self.dock.pressed_slot.take(), at) {
                    if p == a {
                        self.dock_click(a, button);
                    }
                }
            }
            _ => {}
        }
        self.dock.update_lift();
        self.dock.update_y();
        if let Some(d) = &mut self.dock_surf {
            d.dirty = true;
        }
    }

    fn dock_click(&mut self, i: usize, button: u32) {
        let slot = self.dock.slots[i].clone();
        match slot {
            Slot::Start => ipc(&["toggleOn", "start", PRIMARY]),
            Slot::Trash => spawn(&[&format!("{}/.local/bin/mist-files", home()), "trash:///"]),
            Slot::Sep => {}
            Slot::App(key) => {
                let Some(app) = self.dock.app(&key).cloned() else { return };
                if button == BTN_RIGHT {
                    self.open_dock_menu(i, &app);
                    return;
                }
                if button == BTN_MIDDLE || app.wins.is_empty() {
                    launch(&app.exec);
                    return;
                }
                // minimized windows come back first (all of them, like Windows)
                let mins: Vec<_> = app.wins.iter().filter(|w| w.ws_name == "special:minimized").collect();
                if !mins.is_empty() {
                    for w in mins {
                        hypr(&format!("MistRestore('{}', {})", w.address, self.hypr.focused_ws));
                    }
                    return;
                }
                // cycle through the app's windows; the first click focuses the most recent one
                let cur = app.wins.iter().position(|w| w.focus == 0);
                let next = match cur {
                    Some(c) => (c + 1) % app.wins.len(),
                    None => app.wins.iter().enumerate().min_by_key(|(_, w)| w.focus).map(|(i, _)| i).unwrap_or(0),
                };
                focus_win(&app.wins[next].address);
            }
        }
    }

    fn open_dock_menu(&mut self, i: usize, app: &dock::App) {
        let mut rows = vec![Row::Header(app.name.clone())];
        let mut acts = vec![None];
        for w in &app.wins {
            rows.push(Row::item("desktop", &w.title));
            acts.push(Some(DockAct::Focus(w.address.clone())));
        }
        if !app.wins.is_empty() {
            rows.push(Row::Sep);
            acts.push(None);
            rows.push(Row::item("minus", "Minimalizuj"));
            acts.push(Some(DockAct::Minimize));
        }
        rows.push(Row::item("plus", "Nowe okno"));
        acts.push(Some(DockAct::NewWindow));
        rows.push(Row::item("pin", if app.pinned { "Odepnij od docka" } else { "Przypnij do docka" }));
        acts.push(Some(DockAct::TogglePin));
        if !app.wins.is_empty() {
            rows.push(Row::Item { icon: "x".into(), label: if app.wins.len() > 1 { "Zamknij wszystkie okna".into() } else { "Zamknij okno".into() }, enabled: true, danger: true, check: false, sub: false });
            acts.push(Some(DockAct::Close));
        }
        let Some(d) = self.dock_surf.as_ref() else { return };
        let (pw, ph) = popup::size(&mut self.text, Look::Sheet, &rows);
        let m = popup::margin(Look::Sheet);
        let anchor_x = self.dock.slot_x(d.w as f32)[i] + dock::SIZE / 2.0;
        let win_x = (d.out_w - d.w as i32) / 2;
        let win_y = d.out_h - dock::WIN_H as i32;
        let sheet_h = ph as f32 - 2.0 * m;
        let x = win_x as f32 + anchor_x - 150.0 - m;
        let y = win_y as f32 + self.dock.y.target() - sheet_h - 6.0 - m;
        let output = d.output.clone();
        self.dock.menu_open = true;
        self.dock.update_lift();
        self.open_popup(Owner::Dock(app.key.clone(), acts), Look::Sheet, rows, &output, x as i32, y as i32, (pw, ph));
    }

    // ---------------------------------------------------------------- pointer
    fn set_cursor(&mut self, serial: u32, shape: wp_cursor_shape_device_v1::Shape) {
        if let (Some(mgr), Some(p)) = (&self.cursor_mgr, &self.pointer) {
            if self.cursor_dev.is_none() {
                self.cursor_dev = Some(mgr.get_pointer(p, &self.qh, ()));
            }
            if let Some(d) = &self.cursor_dev {
                d.set_shape(serial, shape);
            }
        }
    }
    fn bar_at(&self, s: &wl_surface::WlSurface) -> Option<usize> {
        self.bars.iter().position(|b| b.layer.wl_surface() == s)
    }
    fn hit_at(b: &BarSurf, x: f64) -> Option<Hit> {
        b.items.iter().find(|it| x as f32 >= it.x && (x as f32) < it.x + it.w).map(|it| it.hit.clone())
    }
    fn fade_value(&self, bi: usize, hit: &Hit, target: f32) -> f32 {
        self.fades.iter().find(|f| f.0 == bi && &f.1 == hit).map(|f| {
            let t = (f.4.elapsed().as_secs_f32() * 1000.0 / FADE).min(1.0);
            f.2 + (f.3 - f.2) * t
        }).unwrap_or(target)
    }
    fn set_hover(&mut self, i: usize, h: Option<Hit>) {
        let old = self.bars[i].hover.clone();
        if old == h {
            return;
        }
        let pressed = self.bars[i].pressed.clone();
        for (hit, from, to) in [(old.clone(), 1.0, 0.85), (h.clone(), 0.85, 1.0)] {
            if let Some(hit) = hit {
                let cur = self.fade_value(i, &hit, from);
                if pressed.as_ref() != Some(&hit) {
                    self.fades.retain(|f| !(f.0 == i && f.1 == hit));
                    self.fades.push((i, hit, cur, to, Instant::now()));
                }
            }
        }
        self.bars[i].stats_open = matches!(h, Some(Hit::Stats));
        self.bars[i].hover = h;
        self.bars[i].dirty = true;
    }

    fn pointer_event(&mut self, e: &PointerEvent) {
        if let Some(m) = &self.popup {
            if &e.surface == m.layer.wl_surface() || &e.surface == m.catcher.wl_surface() {
                self.popup_pointer(e);
                return;
            }
        }
        if self.dock_surf.as_ref().map(|d| d.layer.wl_surface() == &e.surface).unwrap_or(false) {
            self.dock_pointer(e);
            return;
        }
        let Some(i) = self.bar_at(&e.surface) else { return };
        let (x, _y) = e.position;
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.set_cursor(serial, wp_cursor_shape_device_v1::Shape::Pointer);
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
            PointerEventKind::Press { .. } => {
                if let Some(h) = Self::hit_at(&self.bars[i], x) {
                    self.fades.retain(|f| !(f.0 == i && f.1 == h));
                    self.fades.push((i, h.clone(), 1.0, 0.6, Instant::now()));
                    self.bars[i].pressed = Some(h);
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
                        self.bar_action(i, &p, button, item);
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

    fn bar_action(&mut self, i: usize, h: &Hit, button: u32, item: (f32, f32)) {
        let scr = self.bars[i].name.clone();
        let mid = button == BTN_MIDDLE;
        let right = button == BTN_RIGHT;
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
                let Some(it) = self.tray_items.iter().find(|t| &t.key == key).cloned() else { return };
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

    // ---------------------------------------------------------------- popups
    pub fn open_tray_menu(&mut self, key: String, entries: Vec<MenuEntry>) {
        let Some((pk, bi, x, y)) = self.pending_menu.take() else { return };
        if pk != key || entries.is_empty() || bi >= self.bars.len() {
            return;
        }
        let rows = tray_rows(&entries);
        let (w, h) = popup::size(&mut self.text, Look::Ctx, &rows);
        let bw = self.bars[bi].w as i32;
        let x = x.min(bw - w as i32 - 6).max(6);
        let output = self.bars[bi].output.clone();
        self.open_popup(Owner::Tray(key, entries), Look::Ctx, rows, &output, x, y, (w, h));
    }
    fn open_popup(&mut self, owner: Owner, look: Look, rows: Vec<Row>, output: &wl_output::WlOutput, x: i32, y: i32, (w, h): (u32, u32)) {
        self.popup = None;
        let cs = self.compositor.create_surface(&self.qh);
        let catcher = self.layer_shell.create_layer_surface(&self.qh, cs, Layer::Overlay, Some("mist-menu-catcher"), Some(output));
        catcher.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        catcher.set_exclusive_zone(-1);
        catcher.set_keyboard_interactivity(KeyboardInteractivity::None);
        catcher.commit();
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(catcher.wl_surface(), &self.qh, ()));
        let ms = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, ms, Layer::Overlay, Some(if look == Look::Sheet { "mist-popup" } else { "mist-menu" }), Some(output));
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(y.max(0), 0, 0, x.max(0));
        layer.set_size(w, h);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        if look == Look::Sheet {
            // only the sheet takes input (not its shadow margin)
            if let Ok(r) = Region::new(&self.compositor) {
                let m = popup::margin(look) as i32;
                r.add(m, m, w as i32 - 2 * m, h as i32 - 2 * m);
                layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
        }
        layer.commit();
        self.popup = Some(Popup { owner, look, rows, layer, catcher, viewport, w, hover: None, configured: false, dirty: true, canvas: Canvas::new(w, h) });
    }
    fn close_popup(&mut self) {
        if let Some(p) = self.popup.take() {
            if matches!(p.owner, Owner::Dock(..)) {
                self.dock.menu_open = false;
                self.dock.update_lift();
                self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
            }
        }
    }
    fn draw_popup(&mut self) {
        let Some(m) = self.popup.as_mut() else { return };
        popup::paint(&mut m.canvas, &mut self.text, m.look, &m.rows, m.hover);
        Self::commit(&mut self.pool, &m.layer, &m.canvas);
        m.dirty = false;
    }
    fn popup_pointer(&mut self, e: &PointerEvent) {
        let Some(m) = self.popup.as_mut() else { return };
        let on_menu = &e.surface == m.layer.wl_surface();
        match e.kind {
            PointerEventKind::Enter { serial } => self.set_cursor(serial, wp_cursor_shape_device_v1::Shape::Default),
            PointerEventKind::Motion { .. } | PointerEventKind::Leave { .. } => {
                let hv = if on_menu && !matches!(e.kind, PointerEventKind::Leave { .. }) { popup::row_at(m.look, &m.rows, e.position.0 as f32, e.position.1 as f32, m.w as f32) } else { None };
                if hv != m.hover {
                    m.hover = hv;
                    m.dirty = true;
                }
            }
            PointerEventKind::Release { .. } => {
                if !on_menu {
                    self.close_popup();
                    return;
                }
                let Some(i) = m.hover else { return };
                match &mut m.owner {
                    Owner::Tray(key, entries) => {
                        let Some(en) = entries.get(i).cloned() else { return };
                        if !en.submenu.is_empty() {
                            // the submenu replaces the list in place
                            *entries = en.submenu.clone();
                            m.rows = tray_rows(entries);
                            m.hover = None;
                            m.dirty = true;
                        } else {
                            let _ = self.tray_req.send(Req::Click(key.clone(), en.id));
                            self.close_popup();
                        }
                    }
                    Owner::Dock(key, acts) => {
                        let act = acts.get(i).cloned().flatten();
                        let key = key.clone();
                        self.close_popup();
                        if let Some(a) = act {
                            self.dock_menu_action(&key, a);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn dock_menu_action(&mut self, key: &str, a: DockAct) {
        let Some(app) = self.dock.app(key).cloned() else { return };
        match a {
            DockAct::Focus(addr) => focus_win(&addr),
            DockAct::Minimize => {
                for w in &app.wins {
                    hypr(&format!("MistMinimize('{}')", w.address));
                }
            }
            DockAct::NewWindow => launch(&app.exec),
            DockAct::TogglePin => {
                let pins: Vec<String> = if app.pinned {
                    self.dock.pins.iter().filter(|p| self.apps.lookup(p).map(|e| e.id != app.key).unwrap_or(true) && **p != app.key).cloned().collect()
                } else {
                    self.dock.pins.iter().cloned().chain(std::iter::once(app.key.clone())).collect()
                };
                self.dock.save_pins(&pins);
            }
            DockAct::Close => {
                for w in &app.wins {
                    hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"address:{}\" }})); hl.dispatch(hl.dsp.window.close())", w.address));
                }
            }
        }
    }
}

fn tray_rows(entries: &[MenuEntry]) -> Vec<Row> {
    entries.iter().map(|e| if e.separator { Row::Sep } else { Row::Item { icon: String::new(), label: e.label.clone(), enabled: e.enabled, danger: false, check: e.toggle == Some(true), sub: !e.submenu.is_empty() } }).collect()
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
fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
pub fn spawn(argv: &[&str]) {
    use std::os::unix::process::CommandExt;
    let mut c = std::process::Command::new(argv[0]);
    c.args(&argv[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).process_group(0);
    if let Ok(mut ch) = c.spawn() {
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}
pub fn sh(cmd: &str) {
    spawn(&["sh", "-c", cmd]);
}
pub fn ipc(args: &[&str]) {
    let qs = format!("{}/.local/bin/mist-qs", home());
    let dir = format!("{}/.config/quickshell/mist", home());
    let mut v: Vec<&str> = vec![&qs, "-p", &dir, "ipc", "call", "mist"];
    v.extend_from_slice(args);
    spawn(&v);
}
pub fn hypr(lua: &str) {
    spawn(&["hyprctl", "eval", lua]);
}
fn focus_win(addr: &str) {
    hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"address:{addr}\" }}))"));
}
// DesktopEntry.execute: the Exec line without the field codes
pub fn launch(exec: &str) {
    if exec.is_empty() {
        return;
    }
    let cmd: Vec<&str> = exec.split_whitespace().filter(|a| !(a.len() == 2 && a.starts_with('%'))).collect();
    sh(&cmd.join(" "));
}

// ---------------------------------------------------------------- messages from the data threads
impl App {
    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Hypr(h) => {
                self.hypr = h;
                self.dock_rebuild();
            }
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
            Msg::Ui(p, s) => self.ui_popup = (p, s),
            Msg::Dnd(d) => self.st.dnd = d,
            Msg::Tray(t) => self.tray_items = t,
            Msg::Pins => {
                self.dock.load_pins();
                self.dock.forget_tiles();
                self.dock_rebuild();
            }
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
        self.add_output(o);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.bars.retain(|b| b.output != o);
        if self.dock_surf.as_ref().map(|d| d.output == o).unwrap_or(false) {
            self.dock_surf = None;
        }
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
        if let Some(d) = self.dock_surf.as_mut() {
            if &d.layer == l {
                if c.new_size.0 > 0 {
                    d.w = c.new_size.0;
                }
                d.configured = true;
                d.dirty = true;
                d.input_shown = None;
                return;
            }
        }
        if let Some(m) = self.popup.as_mut() {
            if &m.layer == l {
                m.configured = true;
                m.dirty = true;
            } else if &m.catcher == l {
                // 1x1 transparent buffer stretched over the screen
                let (cw, ch) = (c.new_size.0.max(1) as i32, c.new_size.1.max(1) as i32);
                if let Some(vp) = &m.viewport {
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
