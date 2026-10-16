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
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
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
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

const FADE: f32 = 120.0; // ms, BarItem opacity Behaviors
pub use crate::paths::primary_monitor;

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

struct IslandSurf {
    layer: LayerSurface,
    configured: bool,
    input: Option<(i32, i32, i32, i32)>,
    focus_kbd: bool,
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

// window snapping (snap.rs): the layout of the next "snapped" event, Snap Assist between picks, the drag's cursor
// poll, the flyout's hover delay
#[derive(Default)]
pub struct SnapSt {
    layout: i32,
    assist: Option<crate::snap::Assist>,
    assist_at: Option<Instant>,
    poll: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    btn: bool,
    fly_at: Option<(Instant, String, f32, f32)>,
    fly_check: Option<Instant>,
}

pub struct PaneSurf {
    p: Box<dyn crate::pane::Pane>,
    layer: LayerSurface,
    screen: String,
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
    island_surf: Option<IslandSurf>,
    pub island: crate::island::Island,
    pub imgs: crate::img::Images,
    pub phone: crate::phone::Phone,
    pub notif_tx: std::sync::mpsc::Sender<crate::notif::Cmd>,
    pub media_tx: std::sync::mpsc::Sender<crate::media::Cmd>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    island_dirty: bool,
    island_eq_dirty: bool, // only the equalizer moved (Island::eq_frame)
    last_minute: i64,
    pos_tick: Instant,
    last_call_key: String,
    pub qh: QueueHandle<App>,
    pub test: bool,
    pub wall: crate::wall::Wall,
    pub desk: crate::desktop::Desk,
    pub idle: crate::idle::Idle,
    seat: Option<wl_seat::WlSeat>,
    pub tx: Option<calloop::channel::Sender<Msg>>,
    polkit: Option<(crate::polkit::Sheet, LayerSurface, bool, bool, Canvas)>,
    polkit_queue: Vec<crate::polkit::Begin>,
    pane: Option<PaneSurf>,
    pane_ptr: (f32, f32),
    notif_hidden: std::collections::HashMap<String, i64>,
    ctrl: bool,
    clip_watch: Option<std::process::Child>,
    shot_opts: (&'static str, u32, bool),
    thumb: Option<(crate::shot::Thumb, LayerSurface, bool, bool, Canvas, Instant)>,
    snap: SnapSt,
    clip_retry: Instant,
    desk_serial: u32,
    desk_cursor: &'static str,
}

impl App {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<App>, tray_req: std::sync::mpsc::Sender<Req>, test: bool, phone: crate::phone::Phone, notif_tx: std::sync::mpsc::Sender<crate::notif::Cmd>, media_tx: std::sync::mpsc::Sender<crate::media::Cmd>) -> App {
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
            island_surf: None,
            island: crate::island::Island::new(),
            imgs: crate::img::Images::new(),
            phone,
            notif_tx,
            media_tx,
            keyboard: None,
            island_dirty: true,
            island_eq_dirty: false,
            last_minute: 0,
            pos_tick: Instant::now(),
            last_call_key: String::new(),
            qh: qh.clone(),
            test,
            desk: crate::desktop::Desk::new(),
            idle: { let mut i = crate::idle::Idle::default(); if !test { i.bind(globals, qh); } i },
            seat: None,
            tx: None,
            polkit: None,
            polkit_queue: Vec::new(),
            pane: None,
            pane_ptr: (0.0, 0.0),
            notif_hidden: std::collections::HashMap::new(),
            ctrl: false,
            clip_watch: None,
            shot_opts: ("region", 0, true),
            thumb: None,
            snap: SnapSt::default(),
            clip_retry: Instant::now(),
            desk_serial: 0,
            desk_cursor: "",
            wall: { let mut w = crate::wall::Wall::new(); w.off = test || std::env::var("MIST_UI_NO_WALL").is_ok(); w },
        }
    }

    fn add_output(&mut self, output: wl_output::WlOutput) {
        let Some(info) = self.outputs.info(&output) else { return };
        let name = info.name.clone().unwrap_or_default();
        let (ow, oh) = info.logical_size.unwrap_or((1920, 1080));
        if self.test && name != "HDMI-A-1" {
            return;
        }
        self.wall.add_output(&output, &name, &self.compositor, &self.layer_shell, &self.qh);
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
        let island_here = if self.test { name == "HDMI-A-1" } else { name == primary_monitor() };
        if island_here && self.island_surf.is_none() && std::env::var("MIST_UI_NO_ISLAND").is_err() {
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("mist-island"), Some(&output));
            layer.set_anchor(Anchor::TOP);
            layer.set_size(crate::island::WIN_W, crate::island::WIN_H);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.island_surf = Some(IslandSurf { layer, configured: false, input: None, focus_kbd: false, canvas: Canvas::new(crate::island::WIN_W, crate::island::WIN_H) });
        }
        if name == primary_monitor() && !self.test && self.desk.stage.is_none() && std::env::var("MIST_UI_NO_DESK").is_err() {
            self.desk.out_w = ow;
            let h = crate::desktop::stage_h(&mut self.text) + 60.0;
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Bottom, Some("mist-desktop"), Some(&output));
            layer.set_anchor(Anchor::TOP);
            layer.set_margin(crate::desktop::STAGE_TOP, 0, 0, 0);
            layer.set_size(crate::desktop::STAGE_W, h as u32);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            // input only over the bento row (mask: Region { item: bento })
            if let Ok(r) = Region::new(&self.compositor) {
                r.add(40, (h - 60.0 - crate::desktop::BENTO_H) as i32, crate::desktop::BENTO_W as i32, crate::desktop::BENTO_H as i32);
                layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
            layer.commit();
            self.desk.stage = Some(layer);
            // the weather card's own window above (no blur: its sky is mist-wall's)
            let (x, y) = self.desk.card_at(&mut self.text);
            let p = crate::desktop::CARD_PAD;
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Bottom, Some("mist-desktop-card"), Some(&output));
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_margin(y - p as i32, 0, 0, x - p as i32);
            layer.set_size((crate::desktop::WX_W + 2.0 * p) as u32, (crate::desktop::BENTO_H + 2.0 * p) as u32);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            if let Ok(r) = Region::new(&self.compositor) {
                r.add(p as i32, p as i32, crate::desktop::WX_W as i32, crate::desktop::BENTO_H as i32);
                layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
            layer.commit();
            self.desk.card = Some(layer);
            self.desk_relayout();
        }
        let dock_here = if self.test { name == "HDMI-A-1" } else { name == primary_monitor() };
        if dock_here && self.dock_surf.is_none() && std::env::var("MIST_UI_NO_DOCK").is_err() {
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("mist-dock"), Some(&output));
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
        s.primary = b.name == primary_monitor();
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
        s.control_open = self.popup_name() == "control";
        s.tray = self.tray_items.iter().map(|t| TrayItem { id: t.key.clone() }).collect();
        s
    }
    fn primary_bare(&self) -> bool {
        let mon = self.hypr.monitors.iter().find(|m| m.1 == primary_monitor());
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
        self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE) || self.dock.animating() || self.dock_hide_at.is_some() || self.dock_show_at.is_some() || self.island.animating() || self.wall.busy() || self.desk.animating() || self.polkit.as_ref().map(|p| p.0.animating()).unwrap_or(false) || self.pane.as_ref().map(|p| p.p.animating()).unwrap_or(false) || self.snap.assist_at.is_some() || self.snap.fly_at.is_some() || self.snap.fly_check.is_some()
    }

    // timers (dock hide 400 ms / reveal 80 ms) and animation frames
    // once a second (and every animation frame): island timers, minute boundary, phone call timeout
    fn outs(&self) -> Vec<(wl_output::WlOutput, String)> {
        self.outputs.outputs().filter_map(|o| self.outputs.info(&o).and_then(|i| i.name.clone()).map(|n| (o, n))).collect()
    }

    // when a desktop card's hover intent settles (the loop wakes then, no polling)
    pub fn intent_due(&self) -> Option<Instant> {
        [self.desk.intent.due(), self.desk.wx.intent.due()].into_iter().flatten().min()
    }
    pub fn intent_tick(&mut self) {
        if self.desk.intent.settle() {
            self.desk.dirty = true;
        }
        if self.desk.wx.settle() {
            self.desk.card_dirty = true;
        }
    }

    // the slow tick's period: 250 ms while something it times is pending, else 1 s (minute, watchers, carousel)
    pub fn slow_period(&self) -> Duration {
        let ringing = { let s = self.phone.st.lock().unwrap(); s.ringing() || s.call().is_some() };
        let busy = self.thumb.is_some() || self.polkit.is_some() || self.pane.is_some() || self.island.pending() || self.desk.wx.pending() || ringing;
        Duration::from_millis(if busy { 250 } else { 1000 })
    }

    pub fn slow_tick(&mut self) {
        self.idle.reap();
        // the screenshot thumbnail goes after 6 s (not while hovered)
        if self.thumb.as_ref().map(|t| t.0.ptr.0 < 0.0 && t.5.elapsed().as_secs() >= 6).unwrap_or(false) {
            self.thumb = None;
        }
        // the clipboard history watcher (wl-paste --watch mistd clipstore), restarted when it exits
        if !self.test && std::env::var("MIST_UI_NO_SERVICES").is_err() {
            let dead = self.clip_watch.as_mut().map(|c| matches!(c.try_wait(), Ok(Some(_)))).unwrap_or(true);
            if dead && self.clip_retry.elapsed().as_secs() >= 3 {
                self.clip_retry = Instant::now();
                self.clip_watch = crate::clipboard::watch();
            }
        }
        if let Some(p) = self.polkit.as_mut() {
            p.3 = true; // the cursor blinks
        }
        if let Some(p) = self.pane.as_mut() {
            if p.p.slow() {
                p.dirty = true;
            }
            let a = p.p.timer();
            self.pane_act(a);
        }
        let outs = self.outs();
        self.wall.slow_tick(&self.compositor, &self.layer_shell, &self.qh, &outs);
        let n = crate::wxcard::Wx::n_insights(&self.desk.weather, &self.desk.garmin, &self.desk.cfg);
        if self.desk.wx.tick(n) {
            self.desk.card_dirty = true;
        }
        let (ringing, call) = { let s = self.phone.st.lock().unwrap(); (s.ringing(), s.call().is_some()) };
        if self.island.tick(ringing, call) {
            self.island_dirty = true;
        }
        if self.phone.tick() {
            self.island_dirty = true;
        }
        let m = crate::data::now() / 60;
        if m != self.last_minute {
            self.last_minute = m;
            self.island.minute();
            self.island_dirty = true;
            self.desk.dirty = true;
            self.desk.card_dirty = true;
            self.mark_all();
        }
        let mode = self.island.mode(ringing, call);
        if self.island.timer_on() || (call && mode == "call") || (mode == "player" && self.island.playing()) {
            self.island_dirty = true;
        }
        if mode == "player" && self.pos_tick.elapsed().as_secs() >= 5 {
            self.pos_tick = Instant::now();
            let _ = self.media_tx.send(crate::media::Cmd::Refresh);
        }
        // the equalizer's next heights: its box alone when the frame is at rest, nothing while hidden
        if self.island.playing() && matches!(mode, "idle" | "track") && !self.island.is_hidden() {
            if self.island.eq_only() {
                self.island_eq_dirty = true;
            } else {
                self.island_dirty = true;
            }
        }
    }
    pub fn tick(&mut self) {
        self.snap_timers();
        DRAWS.with(|d| {
            let mut d = d.borrow_mut();
            *d.entry("tick").or_insert(0) += 1;
            if self.island.animating() { *d.entry("a_island").or_insert(0) += 1; }
            if self.island.eq_running() { *d.entry("a_eq").or_insert(0) += 1; }
            if self.dock.animating() { *d.entry("a_dock").or_insert(0) += 1; }
            if self.desk.animating() { *d.entry("a_desk").or_insert(0) += 1; }
            if self.wall.busy() { *d.entry("a_wall").or_insert(0) += 1; }
            if self.dock_hide_at.is_some() || self.dock_show_at.is_some() { *d.entry("a_dockt").or_insert(0) += 1; }
        });
        let now = Instant::now();
        if self.wall.busy() {
            self.wall.tick(&self.shm);
        }
        if self.desk.fx.running() {
            self.desk.dirty = true;
        }
        if let Some(p) = self.polkit.as_mut() {
            if p.0.animating() {
                p.3 = true;
            }
        }
        if let Some(p) = self.pane.as_mut() {
            if p.p.animating() {
                p.dirty = true;
            }
        }
        if self.desk.wx.fx.running() {
            self.desk.card_dirty = true;
        }
        if self.island.eq_only() {
            if self.island.eq_running() {
                self.island_eq_dirty = true;
            }
        } else if self.island.animating() {
            self.island_dirty = true;
        }
        if self.dock_hide_at.map(|t| now >= t).unwrap_or(false) {
            self.dock_hide_at = None;
            if !self.dock.menu_open && !self.dock.start_open {
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
        if self.island_surf.as_ref().map(|i| i.configured).unwrap_or(false) {
            if self.island_dirty {
                self.draw_island();
            } else if self.island_eq_dirty {
                self.draw_island_eq();
            }
        }
        self.island_eq_dirty = false;
        let t0 = Instant::now();
        self.draw_desk();
        count("wake_us", t0.elapsed().as_micros() as u64);
        self.draw_polkit();
        self.draw_pane();
        self.draw_thumb();
    }

    // ---------------------------------------------------------------- popups (Ui.qml)
    fn focused_screen(&self) -> String {
        self.hypr.monitors.iter().find(|m| m.2 == self.hypr.focused_ws).map(|m| m.1.clone()).unwrap_or(primary_monitor().into())
    }
    // which popup is open (native first, else Quickshell's)
    pub fn popup_name(&self) -> String {
        self.pane.as_ref().map(|p| p.p.name().to_string()).unwrap_or(self.ui_popup.0.clone())
    }
    pub fn ui_toggle(&mut self, name: &str, screen: &str) {
        let scr = if screen.is_empty() { self.focused_screen() } else { screen.to_string() };
        if self.pane.as_ref().map(|p| p.p.name() == name).unwrap_or(false) {
            self.pane_close();
            return;
        }
        if crate::pane::native(name) {
            self.pane_open(name, "", &scr);
        } else {
            self.pane_close();
        }
    }
    pub fn ui_control(&mut self, page: &str, screen: &str) {
        let scr = if screen.is_empty() { self.focused_screen() } else { screen.to_string() };
        // open on this monitor: another page switches, the same one (or none) closes
        if let Some(p) = self.pane.as_mut().filter(|p| p.p.name() == "control" && p.screen == scr) {
            if !page.is_empty() && p.p.set_page(page) {
                p.dirty = true;
            } else {
                self.pane_close();
            }
            return;
        }
        self.pane_open("control", page, &scr);
    }
    fn pane_open(&mut self, name: &str, page: &str, scr: &str) {
        self.pane = None;
        let p: Box<dyn crate::pane::Pane> = if name == "phone" {
            Box::new(crate::phonepanel::PhonePanel::new(self.phone.clone()))
        } else if name == "start" {
            Box::new(crate::start::Start::new(self.phone.clone()))
        } else {
            let Some(p) = crate::pane::make(name, page) else { return };
            p
        };
        self.pane_open_box(p, scr);
    }
    // Cmd+Tab: open on the focused monitor, or step when open
    fn switcher_step(&mut self, d: i32) {
        if let Some(sw) = self.pane.as_mut().and_then(|p| p.p.as_switcher()) {
            sw.step(d);
            self.pane.as_mut().unwrap().dirty = true;
            return;
        }
        let mut cs = self.hypr.clients.clone();
        cs.retain(|c| !c.class.is_empty());
        cs.sort_by_key(|c| c.focus);
        let mut apps: Vec<crate::switcher::App> = Vec::new();
        for c in cs {
            match apps.iter_mut().find(|a| a.cls == c.class) {
                Some(a) => a.wins.push((c.address.clone(), c.ws_name.clone())),
                None => {
                    let e = self.apps.lookup(&c.class);
                    apps.push(crate::switcher::App { cls: c.class.clone(), name: e.map(|e| e.name.clone()).unwrap_or(c.class.clone()), icon: e.map(|e| e.icon.clone()).unwrap_or(c.class.clone()), wins: vec![(c.address.clone(), c.ws_name.clone())] });
                }
            }
        }
        if apps.is_empty() {
            return;
        }
        let scr = self.focused_screen();
        self.pane_open_box(Box::new(crate::switcher::Switcher::new(apps, d)), &scr);
    }
    fn pane_open_box(&mut self, p: Box<dyn crate::pane::Pane>, scr: &str) {
        // replacing the window picker: the dock is not held up by it any more
        if self.pane.as_ref().map(|p| p.p.name() == "winpick").unwrap_or(false) {
            self.dock.menu_open = false;
            self.dock.update_lift();
        }
        self.pane = None;
        let outs = self.outs();
        let Some((o, sname)) = outs.iter().find(|(_, n)| n == scr).or(outs.first()).cloned() else { return };
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some(p.namespace()), Some(&o));
        match p.rect() {
            Some((x, y, w, h)) => {
                layer.set_anchor(Anchor::TOP | Anchor::LEFT);
                layer.set_margin(y, 0, 0, x);
                layer.set_size(w, h);
            }
            None => layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT),
        }
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(if p.keyboard() { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::None });
        // the snap overlay lets the pointer through
        if !p.input() {
            if let Ok(r) = Region::new(&self.compositor) {
                layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
        }
        // Start stops above the dock (shown while it is open)
        let start = p.name() == "start";
        if start && sname == primary_monitor() {
            layer.set_margin(0, 0, 88, 0);
        }
        layer.commit();
        let mut p = p;
        p.opened(&sname);
        self.pane = Some(PaneSurf { p, layer, screen: sname, configured: false, dirty: true, canvas: Canvas::new(1, 1) });
        self.set_start_open(start);
        self.mark_all();
        self.dock_rebuild();
    }
    fn set_start_open(&mut self, on: bool) {
        if self.dock.start_open == on {
            return;
        }
        self.dock.start_open = on;
        if on {
            self.dock_hide_at = None;
        }
        self.dock.update_y();
        if let Some(d) = &mut self.dock_surf {
            d.dirty = true;
        }
    }
    pub fn pane_close(&mut self) {
        if let Some(p) = self.pane.take() {
            // the window picker held the dock up like its menu does
            if p.p.name() == "winpick" {
                self.dock.menu_open = false;
                self.dock.update_lift();
                if !self.dock.start_open {
                    self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
                }
            }
            self.set_start_open(false);
            self.mark_all();
            self.dock_rebuild();
            // the full-monitor buffers (~8 MB each) and the pane's data go back to the OS, not glibc's arena
            unsafe { libc::malloc_trim(0) };
        }
    }
    fn pane_act(&mut self, a: crate::pane::Act) {
        use crate::pane::Act;
        match a {
            Act::None => {}
            Act::Redraw => {
                if let Some(p) = self.pane.as_mut() {
                    p.dirty = true;
                }
            }
            Act::Close => self.pane_close(),
            Act::Run(cmd) => {
                self.pane_close();
                let v: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
                spawn(&v);
            }
            Act::Exec(cmd) => {
                let v: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
                spawn(&v);
                if let Some(p) = self.pane.as_mut() {
                    p.dirty = true;
                }
            }
            Act::Media(c) => {
                let _ = self.media_tx.send(c);
            }
            Act::Snap(c) => self.snap_cmd(c),
            Act::DeskCfg => {
                self.desk.cfg = crate::desktop::Cfg::load();
                self.desk_relayout();
                self.desk.dirty = true;
                self.desk.card_dirty = true;
            }
            Act::Launch(i) => {
                self.pane_close();
                if let Some(e) = self.apps.entries.get(i) {
                    let exec: Vec<&str> = e.exec.split_whitespace().filter(|a| !(a.len() == 2 && a.starts_with('%'))).collect();
                    let cmd = exec.join(" ");
                    if e.terminal { sh(&format!("kitty -e {cmd}")) } else { sh(&cmd) }
                }
            }
            Act::Capture => {
                let o = self.pane.as_mut().and_then(|p| p.p.as_shot()).map(|t| (t.o.mode, t.o.delay, t.o.copy));
                self.pane_close();
                if let Some(o) = o {
                    self.shot_opts = o;
                    crate::shot::capture(&crate::shot::Opts { mode: o.0, delay: o.1, copy: o.2 }, self.tx.clone());
                }
            }
            Act::Toggle(name) => {
                let scr = self.pane.as_ref().map(|p| p.screen.clone()).unwrap_or_default();
                self.ui_toggle(name, &scr);
            }
        }
    }
    fn draw_pane(&mut self) {
        let Some(p) = self.pane.as_mut() else { return };
        if !p.configured || !p.dirty {
            return;
        }
        p.dirty = false;
        let mut c = std::mem::replace(&mut p.canvas, Canvas::new(1, 1));
        let mut cx = crate::pane::Ctx { t: &mut self.text, cal: &self.island.calendar, icons: &mut self.icons, imgs: &mut self.imgs, apps: &self.apps, notif_hidden: &mut self.notif_hidden, ctrl: self.ctrl, st: &self.st, players: &self.island.players, phone_name: &self.island.phone_name, wx: &self.desk.weather };
        let t0 = Instant::now();
        p.p.paint(&mut c, &mut cx);
        let ms = t0.elapsed().as_millis() as u64;
        DRAWS.with(|d| {
            let mut d = d.borrow_mut();
            *d.entry("pane").or_insert(0) += 1;
            *d.entry("pane_ms").or_insert(0) += ms;
        });
        Self::commit(&mut self.pool, &p.layer, &c);
        p.canvas = c;
    }
    fn pane_pointer(&mut self, e: &PointerEvent) {
        use crate::pane::Ptr;
        let (x, y) = (e.position.0 as f32, e.position.1 as f32);
        self.pane_ptr = (x, y);
        let ev = match e.kind {
            PointerEventKind::Enter { serial } => {
                self.desk_serial = serial;
                self.desk_cursor = "?";
                Ptr::Enter
            }
            PointerEventKind::Leave { .. } => Ptr::Leave,
            PointerEventKind::Motion { .. } => Ptr::Motion,
            PointerEventKind::Press { button, .. } => Ptr::Press(button),
            PointerEventKind::Release { button, .. } => Ptr::Release(button),
            PointerEventKind::Axis { vertical, .. } => {
                let dy = if vertical.discrete != 0 { vertical.discrete as f32 } else { vertical.absolute as f32 / 10.0 };
                if dy == 0.0 {
                    return;
                }
                Ptr::Wheel(dy)
            }
        };
        let Some(p) = self.pane.as_mut() else { return };
        let mut cx = crate::pane::Ctx { t: &mut self.text, cal: &self.island.calendar, icons: &mut self.icons, imgs: &mut self.imgs, apps: &self.apps, notif_hidden: &mut self.notif_hidden, ctrl: self.ctrl, st: &self.st, players: &self.island.players, phone_name: &self.island.phone_name, wx: &self.desk.weather };
        let a = p.p.pointer(ev, x, y, &mut cx);
        let hand = p.p.hand();
        self.desk_cursor(if hand { "hand" } else { "" });
        self.pane_act(a);
    }

    // ---------------------------------------------------------------- window snapping (snap.rs)
    fn snap_event(&mut self, n: &str, d: &str) {
        use crate::snap::*;
        let p: Vec<&str> = d.split(',').collect();
        let num = |i: usize| p.get(i).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(f32::NAN);
        let arg = |i: usize| p.get(i).copied().unwrap_or("").to_string();
        match (n, p.first().copied().unwrap_or("")) {
            ("mistdrag", "begin") => {
                self.snap_close_extras();
                let g = [num(2), num(3), num(4), num(5)];
                let grab = (!g.iter().any(|v| v.is_nan())).then_some(g);
                let (x, y) = cursor().unwrap_or((num(2), num(3)));
                let drag = Drag::new(&arg(1), grab, monitors(), x, y);
                if drag.mons.is_empty() {
                    return;
                }
                let scr = drag.mon().name.clone();
                self.pane_close();
                self.pane_open_box(Box::new(DragPane(drag)), &scr);
                self.snap_poll(true);
            }
            ("mistdrag", "end") => {
                self.snap_poll(false);
                let Some(dr) = self.pane.as_mut().and_then(|p| p.p.as_drag()) else { return };
                if let Some((z, li)) = dr.end(num(1), num(2)) {
                    self.snap.layout = li;
                    apply(&dr.addr, &z, dr.grab, "", 0);
                }
                self.pane_close();
            }
            ("mistsnap", "snapped") => {
                let li = std::mem::replace(&mut self.snap.layout, -1);
                let a = norm(&arg(1));
                // a window picked in Snap Assist
                if self.snap.assist.as_ref().map(|s| s.group.contains(&a) && s.group[0] != a).unwrap_or(false) {
                    return;
                }
                let z = [num(3), num(4), num(5), num(6)];
                if z.iter().any(|v| v.is_nan()) {
                    return;
                }
                let rest = remaining(&z, li);
                if rest.is_empty() {
                    self.snap_end_assist();
                    return;
                }
                self.snap.assist = Some(Assist { mon: arg(2), zones: rest, group: vec![a] });
                self.snap.assist_at = Some(Instant::now() + Duration::from_millis(160));
            }
            ("mistsnap", "maxhover") => {
                self.snap.btn = true;
                let name = self.pane.as_ref().map(|p| p.p.name()).unwrap_or("");
                match name {
                    "snapdrag" => {}
                    "snapfly" => {
                        self.pane.as_mut().unwrap().p.data("maxhover");
                    }
                    _ => self.snap.fly_at = Some((Instant::now() + Duration::from_millis(420), arg(1), num(2), num(3))),
                }
            }
            ("mistsnap", "maxleave") => {
                self.snap.btn = false;
                self.snap.fly_at = None;
                if let Some(p) = self.pane.as_mut().filter(|p| p.p.name() == "snapfly") {
                    p.p.data("maxleave");
                    self.snap.fly_check = Some(Instant::now() + Duration::from_millis(330));
                }
            }
            ("mistsnap", "layouts") => self.snap_open_fly(&arg(1), num(2), num(3), true),
            _ => {}
        }
    }
    fn snap_open_fly(&mut self, addr: &str, x: f32, y: f32, pinned: bool) {
        if self.pane.as_ref().map(|p| p.p.name() == "snapdrag").unwrap_or(false) || x.is_nan() || y.is_nan() {
            return;
        }
        let f = crate::snap::Fly::new(addr, x, y, pinned, &crate::snap::monitors());
        let r = f.surface(x, y);
        let scr = f.screen().to_string();
        self.pane_close();
        self.pane_open_box(Box::new(crate::snap::FlyPane(f, r)), &scr);
    }
    // the drag overlay follows the cursor (Msg::Cursor from the poll thread)
    fn snap_cursor(&mut self, x: f32, y: f32) {
        let Some(p) = self.pane.as_mut() else { return };
        let Some(d) = p.p.as_drag() else { return };
        if d.preview {
            return;
        }
        let before = (d.zone(), d.pill, d.mode, d.hot_layout);
        if d.track(x, y) {
            // onto another monitor: the overlay moves there
            d.moved();
            let scr = d.mon().name.clone();
            if let Some(ps) = self.pane.take() {
                self.pane_open_box(ps.p, &scr);
            }
            return;
        }
        if (d.zone(), d.pill, d.mode, d.hot_layout) != before {
            p.dirty = true;
        }
    }
    // j/cursorpos every 16 ms, only while a window is dragged
    fn snap_poll(&mut self, on: bool) {
        use std::sync::atomic::{AtomicBool, Ordering};
        if let Some(f) = self.snap.poll.take() {
            f.store(false, Ordering::Relaxed);
        }
        let Some(tx) = self.tx.clone().filter(|_| on) else { return };
        let flag = std::sync::Arc::new(AtomicBool::new(true));
        self.snap.poll = Some(flag.clone());
        let _ = std::thread::Builder::new().name("snap-cursor".into()).stack_size(128 * 1024).spawn(move || {
            let (t0, mut last) = (Instant::now(), (f32::NAN, f32::NAN));
            // a drag end that never arrived (plugin unloaded mid-drag...): give up after 2 min
            while flag.load(Ordering::Relaxed) && t0.elapsed().as_secs() < 120 {
                if let Some(c) = crate::snap::cursor() {
                    if c != last {
                        last = c;
                        if tx.send(Msg::Cursor(c.0, c.1)).is_err() {
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        });
    }
    fn snap_cmd(&mut self, c: crate::snap::Cmd) {
        use crate::snap::*;
        match c {
            Cmd::Fly(addr, z, li) => {
                self.pane_close();
                self.snap.layout = li as i32;
                apply(&addr, &z, None, "", 90);
            }
            Cmd::AssistPick(addr, ws) => {
                self.pane_close();
                let Some(a) = self.snap.assist.as_mut() else { return };
                if a.zones.is_empty() {
                    return self.snap_end_assist();
                }
                let z = a.zones.remove(0);
                // minimized (special:minimized): back to this monitor's workspace first
                let pre = if ws.starts_with("special:") {
                    monitors().iter().find(|m| m.name == a.mon).map(|m| format!("MistRestore(\"{addr}\", {}); ", m.ws)).unwrap_or_default()
                } else {
                    String::new()
                };
                a.group.push(addr.clone());
                let more = !a.zones.is_empty();
                self.snap.layout = -1;
                apply(&addr, &z, None, &pre, 90);
                if more {
                    self.snap.assist_at = Some(Instant::now() + Duration::from_millis(90 + 200));
                } else {
                    self.snap_end_assist();
                }
            }
            Cmd::AssistEnd => {
                self.snap_end_assist();
                self.pane_close();
            }
        }
    }
    fn snap_end_assist(&mut self) {
        self.snap.assist = None;
        self.snap.assist_at = None;
    }
    // a drag starts: the flyout and Snap Assist go
    fn snap_close_extras(&mut self) {
        self.snap_end_assist();
        self.snap.fly_at = None;
        if self.pane.as_ref().map(|p| matches!(p.p.name(), "snapfly" | "snapassist")).unwrap_or(false) {
            self.pane_close();
        }
    }
    fn snap_timers(&mut self) {
        let now = Instant::now();
        // the hover flyout: closed unless the pointer went onto it
        if self.snap.fly_check.map(|t| now >= t).unwrap_or(false) {
            self.snap.fly_check = None;
            if let Some(a) = self.pane.as_mut().filter(|p| p.p.name() == "snapfly").map(|p| p.p.timer()) {
                self.pane_act(a);
            }
        }
        if let Some((t, a, x, y)) = self.snap.fly_at.clone() {
            if now >= t {
                self.snap.fly_at = None;
                if self.snap.btn {
                    self.snap_open_fly(&a, x, y, false);
                }
            }
        }
        if self.snap.assist_at.map(|t| now >= t).unwrap_or(false) {
            self.snap.assist_at = None;
            // not over a popup the user opened meanwhile
            if self.pane.as_ref().map(|p| p.p.name() != "snapassist").unwrap_or(false) {
                return self.snap_end_assist();
            }
            let Some(mut a) = self.snap.assist.take() else { return };
            if let Some((m, wins)) = a.candidates() {
                let (z, scr) = (a.zones[0], m.name.clone());
                self.snap.assist = Some(a);
                self.pane_open_box(Box::new(crate::snap::AssistPane::new(m, z, wins)), &scr);
            } else {
                self.pane_close();
            }
        }
    }

    // ---------------------------------------------------------------- the screenshot thumbnail
    fn thumb_show(&mut self, path: String) {
        self.imgs.forget(&path);
        let ih = self.imgs.get(&path, 480, &mut self.icons).map(|p| (224.0 * p.height() as f32 / p.width().max(1) as f32).min(150.0)).unwrap_or(120.0);
        let th = crate::shot::Thumb { path, copy: self.shot_opts.2, ptr: (-1.0, -1.0), img_h: ih };
        let outs = self.outs();
        let scr = self.focused_screen();
        let Some((o, _)) = outs.iter().find(|(_, n)| *n == scr).or(outs.first()).cloned() else { return };
        self.thumb = None;
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("mist-popup"), Some(&o));
        layer.set_anchor(Anchor::BOTTOM | Anchor::RIGHT);
        layer.set_margin(0, 24, 24, 0);
        layer.set_size(crate::shot::Thumb::W, th.height());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        let (w, h) = (crate::shot::Thumb::W, th.height());
        self.thumb = Some((th, layer, false, true, Canvas::new(w, h), Instant::now()));
    }
    fn draw_thumb(&mut self) {
        let Some(t) = self.thumb.as_mut() else { return };
        if !t.2 || !t.3 {
            return;
        }
        t.3 = false;
        t.0.paint(&mut t.4, &mut self.text, &mut self.imgs, &mut self.icons);
        Self::commit(&mut self.pool, &t.1, &t.4);
    }

    // ---------------------------------------------------------------- polkit sheet
    fn on_polkit(&mut self, ev: crate::polkit::Ev) {
        use crate::polkit::Ev;
        let Some(tx) = self.tx.clone() else { return };
        match ev {
            Ev::Begin(b) => {
                if self.polkit.is_some() {
                    self.polkit_queue.push(b);
                } else {
                    self.polkit_open(b);
                }
            }
            Ev::Cancel(cookie) => {
                if self.polkit.as_ref().map(|p| p.0.req.cookie == cookie).unwrap_or(false) {
                    if let Some(p) = self.polkit.as_mut() {
                        p.0.cancel();
                    }
                    self.polkit_close();
                } else {
                    self.polkit_queue.retain(|b| b.cookie != cookie);
                }
            }
            Ev::Prompt(p, echo) => {
                if let Some(s) = self.polkit.as_mut() {
                    s.0.prompt = p;
                    s.0.echo = echo;
                    s.0.busy = false;
                    s.3 = true;
                }
            }
            Ev::Info(m, err) => {
                if let Some(s) = self.polkit.as_mut() {
                    s.0.info = m;
                    s.0.info_err = err;
                    s.3 = true;
                }
            }
            Ev::Done(ok) => {
                let close = self.polkit.as_mut().map(|s| { s.3 = true; s.0.done(ok, tx.clone()) }).unwrap_or(false);
                if close {
                    self.polkit_close();
                }
            }
        }
    }
    fn polkit_open(&mut self, b: crate::polkit::Begin) {
        let Some(tx) = self.tx.clone() else { return };
        // the focused monitor
        let name = self.hypr.monitors.iter().find(|m| m.2 == self.hypr.focused_ws).map(|m| m.1.clone()).unwrap_or(primary_monitor().into());
        let outs = self.outs();
        let Some((o, _)) = outs.iter().find(|(_, n)| *n == name).or(outs.first()) else { return };
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("mist-session"), Some(o));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        self.polkit = Some((crate::polkit::Sheet::new(b, tx), layer, false, true, Canvas::new(1, 1)));
    }
    fn polkit_close(&mut self) {
        self.polkit = None;
        if !self.polkit_queue.is_empty() {
            let b = self.polkit_queue.remove(0);
            self.polkit_open(b);
        }
    }
    fn draw_polkit(&mut self) {
        let Some(p) = self.polkit.as_mut() else { return };
        if !p.2 || !p.3 {
            return;
        }
        p.3 = false;
        let mut c = std::mem::replace(&mut p.4, Canvas::new(1, 1));
        p.0.paint(&mut c, &mut self.text);
        Self::commit(&mut self.pool, &p.1, &c);
        p.4 = c;
    }
    fn polkit_key(&mut self, ev: &KeyEvent) {
        let Some(p) = self.polkit.as_mut() else { return };
        p.3 = true;
        p.0.blink = Instant::now();
        match ev.keysym {
            Keysym::Return | Keysym::KP_Enter => p.0.submit(),
            Keysym::Escape => {
                p.0.cancel();
                self.polkit_close();
            }
            Keysym::BackSpace => {
                if !p.0.busy {
                    p.0.pw.pop();
                }
            }
            _ => {
                if let Some(t) = &ev.utf8 {
                    if !p.0.busy && !t.chars().any(|c| c.is_control()) {
                        p.0.pw.push_str(t);
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- desktop
    fn draw_desk(&mut self) {
        DRAWS.with(|d| *d.borrow_mut().entry("wake").or_insert(0) += 1); // every loop wakeup, not a desktop paint (stage/card count those)
        if self.desk.dirty && self.desk.stage_cfg {
            self.desk.dirty = false;
            let t0 = Instant::now();
            self.desk.paint_stage(&mut self.text);
            count("stage", 1);
            count("stage_us", t0.elapsed().as_micros() as u64);
            if let Some(l) = &self.desk.stage {
                Self::commit(&mut self.pool, l, &self.desk.canvas);
            }
        }
        if self.desk.card_dirty && self.desk.card_cfg {
            self.desk.card_dirty = false;
            let p = crate::desktop::CARD_PAD;
            let (w, h) = ((crate::desktop::WX_W + 2.0 * p) as u32, (crate::desktop::BENTO_H + 2.0 * p) as u32);
            if self.desk.card_canvas.pm.width() != w {
                self.desk.card_canvas = crate::gfx::Canvas::new(w, h);
            }
            let mut c = std::mem::replace(&mut self.desk.card_canvas, crate::gfx::Canvas::new(1, 1));
            let t0 = Instant::now();
            let d = &mut self.desk;
            // not among the shown cards: an empty window (its sky is off too)
            let rows = if d.weather_on() {
                d.wx.paint_cached(&mut c, &mut self.text, &d.weather, &d.garmin, &d.cfg)
            } else {
                c.clear();
                None
            };
            count("card", 1);
            count("card_us", t0.elapsed().as_micros() as u64);
            self.desk.card_canvas = c;
            if let Some(l) = &self.desk.card {
                let cw = self.desk.card_canvas.pm.width() as i32;
                let (y, h) = rows.unwrap_or((0, self.desk.card_canvas.pm.height() as i32));
                Self::commit_box(&mut self.pool, l, &self.desk.card_canvas, (0, y, cw, h));
            }
        }
    }
    fn desk_cursor(&mut self, want: &'static str) {
        if want != self.desk_cursor {
            self.desk_cursor = want;
            let shape = match want {
                "hand" => wp_cursor_shape_device_v1::Shape::Pointer,
                "ew" => wp_cursor_shape_device_v1::Shape::EwResize,
                _ => wp_cursor_shape_device_v1::Shape::Default,
            };
            self.set_cursor(self.desk_serial, shape);
        }
    }
    fn desk_pointer(&mut self, e: &PointerEvent, card: bool) {
        let (x, y) = (e.position.0 as f32, e.position.1 as f32);
        let p = crate::desktop::CARD_PAD;
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.desk_serial = serial;
                self.desk_cursor = "?";
                self.desk_motion(Some((x, y)), card);
            }
            PointerEventKind::Leave { .. } => self.desk_motion(None, card),
            PointerEventKind::Motion { .. } => self.desk_motion(Some((x, y)), card),
            PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                if card {
                    let wx = self.desk.weather.clone();
                    if self.desk.wx.click(&mut self.desk.cfg, &wx) {
                        self.desk.card_dirty = true;
                        self.desk.dirty = true;
                    }
                } else if self.desk.stage_click() {
                    self.desk.dirty = true;
                }
                // the order may have changed: the weather card and its sky move
                self.desk_relayout();
            }
            // right click on a card or the clock: Dostosuj pulpit, on that card's tab
            PointerEventKind::Release { button, .. } if button == BTN_RIGHT => {
                let k = if card { "weather".to_string() } else { self.desk.zone() };
                self.pane_open_box(Box::new(crate::desksettings::DeskSettings::new(&k)), primary_monitor());
            }
            PointerEventKind::Axis { vertical, .. } if card => {
                let dy = if vertical.discrete != 0 { vertical.discrete as f64 } else { vertical.absolute };
                if dy != 0.0 {
                    let wx = self.desk.weather.clone();
                    if self.desk.wx.wheel(dy > 0.0, &wx) {
                        self.desk.card_dirty = true;
                    }
                }
            }
            _ => {}
        }
        let _ = p;
    }
    fn desk_motion(&mut self, pos: Option<(f32, f32)>, card: bool) {
        let p = crate::desktop::CARD_PAD;
        if card {
            let before = self.desk.wx.sig();
            self.desk.wx.motion(pos.map(|q| (q.0 - p, q.1 - p)));
            if self.desk.wx.sig() != before {
                self.desk.card_dirty = true;
            }
            let c = self.desk.wx.cursor;
            self.desk_cursor(c);
        } else {
            let before = self.desk.sig();
            self.desk.stage_motion(pos);
            if self.desk.sig() != before {
                self.desk.dirty = true;
            }
            let c = if self.desk.cursor_hand { "hand" } else { "" };
            self.desk_cursor(c);
        }
    }
    // the weather card follows its slot in the bento row
    // (and the input regions: the shown cards + the clock; the weather card takes no input while it is hidden)
    fn desk_relayout(&mut self) {
        let h = crate::desktop::stage_h(&mut self.text) + 60.0;
        if let (Some(l), Ok(r)) = (&self.desk.stage, Region::new(&self.compositor)) {
            let (rx, rw) = self.desk.row();
            if rw > 0.0 {
                r.add(rx as i32, (h - 60.0 - crate::desktop::BENTO_H) as i32, rw as i32, crate::desktop::BENTO_H as i32);
            }
            // the clock: right click there opens the settings (with no cards shown too)
            r.add(crate::desktop::STAGE_W as i32 / 2 - 320, 0, 640, (h - 60.0 - crate::desktop::BENTO_H - 40.0) as i32);
            l.wl_surface().set_input_region(Some(r.wl_region()));
            l.commit();
        }
        let Some(l) = &self.desk.card else { return };
        let (x, y) = self.desk.card_at(&mut self.text);
        let p = crate::desktop::CARD_PAD as i32;
        l.set_margin(y - p, 0, 0, x - p);
        if let Ok(r) = Region::new(&self.compositor) {
            if self.desk.weather_on() {
                r.add(p, p, crate::desktop::WX_W as i32, crate::desktop::BENTO_H as i32);
            }
            l.wl_surface().set_input_region(Some(r.wl_region()));
        }
        l.commit();
        self.desk.write_sky(&mut self.text);
    }

    // ---------------------------------------------------------------- island
    fn draw_island(&mut self) {
        DRAWS.with(|d| *d.borrow_mut().entry("island").or_insert(0) += 1);
        self.island_dirty = false;
        let st = self.phone.st.lock().unwrap();
        let (ringing, call) = (st.ringing(), st.call().is_some());
        self.island.phone_name = st.name();
        let Some(isf) = self.island_surf.as_mut() else { return };
        let mut cx = crate::island::Ctx { t: &mut self.text, icons: &mut self.icons, imgs: &mut self.imgs, phone: &st, now: crate::data::now() };
        let t0 = Instant::now();
        self.island.draw(&mut isf.canvas, &mut cx, ringing, call);
        let us = t0.elapsed().as_micros() as u64;
        DRAWS.with(|d| *d.borrow_mut().entry("island_us").or_insert(0) += us);
        drop(cx);
        drop(st);
        // input region: the shape (nothing while it hides under a fullscreen window)
        let mode = self.island.mode(ringing, call);
        let hidden = self.island.fullscreen && mode == "idle";
        let w = self.island.w.cur;
        let r = if hidden { (0, 0, 0, 0) } else { (((crate::island::WIN_W as f32 - w) / 2.0) as i32, self.island.y.cur.max(0.0) as i32, w.ceil() as i32, self.island.h.cur.ceil() as i32) };
        if isf.input != Some(r) {
            if let Ok(reg) = Region::new(&self.compositor) {
                if r.2 > 0 {
                    reg.add(r.0, r.1, r.2, r.3);
                }
                isf.layer.wl_surface().set_input_region(Some(reg.wl_region()));
            }
            isf.input = Some(r);
        }
        // the keyboard only while a reply is typed
        if isf.focus_kbd != self.island.replying {
            isf.focus_kbd = self.island.replying;
            isf.layer.set_keyboard_interactivity(if self.island.replying { KeyboardInteractivity::OnDemand } else { KeyboardInteractivity::None });
        }
        Self::commit(&mut self.pool, &isf.layer, &isf.canvas);
    }

    // a frame of the equalizer alone: its box repainted over the last frame, only that box damaged
    fn draw_island_eq(&mut self) {
        let Some(isf) = self.island_surf.as_mut() else { return };
        let t0 = Instant::now();
        let Some(b) = self.island.eq_frame(&mut isf.canvas) else {
            self.draw_island();
            return;
        };
        count("island_eq", 1);
        count("island_eq_us", t0.elapsed().as_micros() as u64);
        Self::commit_box(&mut self.pool, &isf.layer, &isf.canvas, b);
    }

    fn island_hit(&self, x: f32, y: f32) -> Option<crate::island::Hit> {
        self.island.hits.iter().rev().find(|h| x >= h.0 && x < h.0 + h.2 && y >= h.1 && y < h.1 + h.3).map(|h| h.4.clone())
    }
    fn island_in_shape(&self, x: f32, y: f32) -> bool {
        let w = self.island.w.cur;
        let x0 = (crate::island::WIN_W as f32 - w) / 2.0;
        x >= x0 && x < x0 + w && y >= self.island.y.cur && y < self.island.y.cur + self.island.h.cur
    }
    fn island_mode(&self) -> &'static str {
        let s = self.phone.st.lock().unwrap();
        self.island.mode(s.ringing(), s.call().is_some())
    }

    fn island_pointer(&mut self, e: &PointerEvent) {
        use crate::island::Hit as IH;
        let (x, y) = (e.position.0 as f32, e.position.1 as f32);
        let mode = self.island_mode();
        match e.kind {
            PointerEventKind::Enter { serial } => {
                let shape = if mode == "idle" || mode == "glance" || self.island.ev.as_ref().map(|e| e.act != crate::island::Act::None).unwrap_or(false) { wp_cursor_shape_device_v1::Shape::Pointer } else { wp_cursor_shape_device_v1::Shape::Default };
                self.set_cursor(serial, shape);
                self.island.set_hover(true, mode);
                self.island.hover_hit = self.island_hit(x, y);
            }
            PointerEventKind::Leave { .. } => {
                self.island.set_hover(false, mode);
                self.island.hover_hit = None;
            }
            PointerEventKind::Motion { .. } => {
                let inside = self.island_in_shape(x, y);
                self.island.set_hover(inside, mode);
                self.island.hover_hit = self.island_hit(x, y);
                if self.island.seek_drag.is_some() {
                    let (a, b) = self.island.seek_track(self.island.w.to);
                    self.island.seek_drag = Some(((x - a) / (b - a)).clamp(0.0, 1.0));
                }
            }
            PointerEventKind::Press { button, .. } => {
                if button == BTN_LEFT && self.island_hit(x, y) == Some(IH::Seek) {
                    let (a, b) = self.island.seek_track(self.island.w.to);
                    self.island.seek_drag = Some(((x - a) / (b - a)).clamp(0.0, 1.0));
                }
            }
            PointerEventKind::Release { button, .. } => {
                if let Some(f) = self.island.seek_drag.take() {
                    if let Some(p) = self.island.player().cloned() {
                        let _ = self.media_tx.send(crate::media::Cmd::Seek(p.bus.clone(), p.track_id.clone(), f as f64 * p.length));
                    }
                    self.island_dirty = true;
                    return;
                }
                self.island_click(button, x, y, mode);
            }
            PointerEventKind::Axis { vertical, .. } => {
                let dy = if vertical.discrete != 0 { vertical.discrete as f64 } else { vertical.absolute };
                if dy != 0.0 {
                    let up = dy < 0.0;
                    let mut cmd = String::new();
                    if self.island.audio.mute && up {
                        cmd.push_str("wpctl set-mute @DEFAULT_AUDIO_SINK@ 0; ");
                    }
                    cmd.push_str(if up { "wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+" } else { "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-" });
                    sh(&cmd);
                }
            }
        }
        self.island_dirty = true;
    }

    fn island_click(&mut self, button: u32, x: f32, y: f32, mode: &str) {
        use crate::island::Hit as IH;
        if button == BTN_MIDDLE {
            if let Some(p) = self.island.player().cloned() {
                self.island.mine();
                let _ = self.media_tx.send(crate::media::Cmd::Toggle(p.bus));
            }
            return;
        }
        if button == BTN_RIGHT {
            if self.island.ev.is_some() || !self.island.queue.is_empty() {
                self.island.clear_all();
            } else if self.island.timer_on() {
                self.island.timer_stop();
            }
            return;
        }
        if let Some(h) = self.island_hit(x, y) {
            let p = self.island.player().cloned();
            let note = { let s = self.phone.st.lock().unwrap(); self.island.ev.clone().and_then(|e| crate::island::phone_note(e.phone.as_ref(), &s)) };
            match h {
                IH::Ctl(c) => {
                    if let Some(p) = p {
                        self.island.mine();
                        let cmd = match c {
                            "prev" => crate::media::Cmd::Prev(p.bus),
                            "next" => crate::media::Cmd::Next(p.bus),
                            "play" => crate::media::Cmd::Toggle(p.bus),
                            "shuffle" => crate::media::Cmd::Shuffle(p.bus, !p.shuffle.unwrap_or(false)),
                            _ => crate::media::Cmd::Loop(p.bus, match p.loop_status.as_deref() { Some("None") => "Playlist", Some("Playlist") => "Track", _ => "None" }.into()),
                        };
                        let _ = self.media_tx.send(cmd);
                    }
                }
                IH::Chip => self.island.next_player(),
                IH::Seek => {}
                IH::CallAnswer => self.phone.answer(),
                IH::CallHangup => self.phone.hangup(),
                IH::CallMute => self.phone.tel("mute", None),
                IH::CallAudio => self.phone.tel("pc", None),
                IH::Keypad => self.island.keypad = !self.island.keypad,
                IH::Key(k) => self.phone.tel("tones", Some(serde_json::json!(k))),
                IH::NoteReply => {
                    self.island.replying = true;
                    self.island.reply.clear();
                }
                IH::NoteAction(a) => {
                    if let Some(n) = note {
                        self.phone.send("action", Some(serde_json::json!({ "id": n["id"], "action": a })));
                        self.phone_done(&format!("wysłano · {a}"));
                    }
                }
                IH::NoteDismiss => {
                    if let Some(n) = note {
                        self.phone.send("dismiss", Some(n["id"].clone()));
                        self.phone_done("");
                    }
                }
                IH::ReplySend => self.reply_send(),
                IH::CopyCode(code) => {
                    spawn(&["wl-copy", "--", &code]);
                    self.island_copied(&format!("skopiowano kod · {code}"));
                }
                IH::CopyLink(link) => {
                    spawn(&["wl-copy", "--", &link]);
                    let preview = if link.len() > 30 { format!("{}…", &link[..28]) } else { link.clone() };
                    self.island_copied(&format!("skopiowano link · {preview}"));
                }
                IH::OpenLink(link) => {
                    let url = if link.starts_with("www.") { format!("https://{link}") } else { link.clone() };
                    spawn(&["xdg-open", &url]);
                    self.island_copied("otwieram link w przeglądarce…");
                }
                IH::DesktopAction(id, key) => {
                    let _ = self.notif_tx.send(crate::notif::Cmd::Invoke(id, key));
                    self.island.clear_ev();
                    self.island.next();
                }
            }
            return;
        }
        if !self.island_in_shape(x, y) {
            return;
        }
        if mode == "idle" || mode == "glance" {
            self.ui_toggle("calendar", primary_monitor());
        } else if mode != "player" && mode != "call" {
            if let Some(e) = self.island.ev.clone() {
                self.island_act(&e);
            }
            self.island.next();
        }
    }

    fn island_act(&mut self, e: &crate::island::Ev) {
        use crate::island::Act;
        match &e.act {
            Act::None => {}
            Act::Nid(id, names, entry) => {
                let _ = self.notif_tx.send(crate::notif::Cmd::Invoke(*id, "default".into()));
                // the app's window (by class), else start the app
                let names: Vec<String> = names.iter().filter(|n| !n.is_empty()).map(|n| n.to_lowercase()).collect();
                let w = self.hypr.clients.iter().find(|c| {
                    let cl = c.class.to_lowercase();
                    !cl.is_empty() && names.iter().any(|n| cl == *n || cl.ends_with(&format!(".{n}")) || n.ends_with(&format!(".{cl}")) || n.replace(' ', "-") == cl)
                });
                if let Some(w) = w {
                    focus_win(&w.address);
                } else if let Some(en) = self.apps.entries.iter().find(|x| &x.id == entry) {
                    launch(&en.exec.clone());
                }
            }
            Act::Focus(a) => focus_win(&format!("0x{}", a.trim_start_matches("0x"))),
            Act::Open(p) => spawn(&["xdg-open", p]),
            Act::Popup(p) => self.ui_toggle(p, primary_monitor()),
            Act::Phone(f) => self.phone.send(f, None),
        }
    }
    fn phone_done(&mut self, msg: &str) {
        self.island.replying = false;
        if msg.is_empty() {
            self.island.next();
            return;
        }
        self.island.clear_ev();
        let name = { let s = self.phone.st.lock().unwrap(); s.name() };
        self.island.push(crate::island::Ev { view: "info", key: "phoneMsg".into(), prio: 2, dur: 2000, icon: "check".into(), color: Some(crate::theme::GREEN), title: if name.is_empty() { "Telefon".into() } else { name }, sub: msg.into(), value: -1, ..Default::default() });
    }
    fn island_copied(&mut self, msg: &str) {
        self.island.replying = false;
        self.island.clear_ev();
        self.island.push(crate::island::Ev {
            view: "info",
            key: "clipDone".into(),
            prio: 3,
            dur: 2400,
            icon: "clipOk".into(),
            color: Some(crate::theme::GREEN),
            title: "Schowek".into(),
            sub: msg.into(),
            value: -1,
            ..Default::default()
        });
    }
    fn reply_send(&mut self) {
        let t = self.island.reply.trim().to_string();
        let note = { let s = self.phone.st.lock().unwrap(); self.island.ev.clone().and_then(|e| crate::island::phone_note(e.phone.as_ref(), &s)) };
        if let (false, Some(n)) = (t.is_empty(), note) {
            self.phone.send("reply", Some(serde_json::json!({ "id": n["id"], "text": t })));
            self.phone_done("odpowiedź wysłana");
        }
    }

    // ---------------------------------------------------------------- IPC
    fn on_ipc(&mut self, r: crate::ipc::Request) {
        let a = |i: usize| r.args.get(i).cloned().unwrap_or_default();
        let out: String = match (r.target.as_str(), r.func.as_str()) {
            ("island", "show") => { self.island.push(crate::island::Ev { view: "info", prio: 2, dur: 3500, icon: a(0), color: Some(crate::theme::ACCENT), title: a(1), sub: a(2), value: -1, ..Default::default() }); String::new() }
            ("island", "demo") => { self.island.demo(&a(0)); String::new() }
            // tests: a left click on the dock icon of app <key> (desktop id or class)
            ("dock", "click") => match self.dock.slots.iter().position(|s| matches!(s, Slot::App(k) if *k == a(0))) {
                Some(i) => { self.dock_click(i, BTN_LEFT); "ok".into() }
                None => format!("no dock slot {}", a(0)),
            },
            ("island", "timer") => { self.island.timer_start(a(0).parse().unwrap_or(0.0), &a(1)); String::new() }
            ("island", "timerStop") => { self.island.timer_stop(); String::new() }
            ("island", "dismiss") => { self.island.clear_all(); String::new() }
            ("island", "click") => { if let Some(e) = self.island.ev.clone() { self.island_act(&e); self.island.next(); } String::new() }
            ("island", "state") => {
                let mode = self.island_mode();
                serde_json::json!({ "mode": mode, "ev": self.island.ev.as_ref().map(|e| format!("{} {} {}", e.view, e.title, e.sub)), "queue": self.island.queue.len(), "timer": self.island.timer_left(), "expanded": self.island.expanded, "w": self.island.w.cur, "h": self.island.h.cur }).to_string()
            }
            ("island", "grab") => {
                if let Some(i) = &self.island_surf { let _ = i.canvas.pm.save_png(a(0)); }
                String::new()
            }
            ("notif", "dnd") => { let on = a(0) == "true" || a(0) == "1"; let _ = self.notif_tx.send(crate::notif::Cmd::Dnd(on)); self.st.dnd = on; self.mark_all(); String::new() }
            ("notif", "clear") => { let _ = self.notif_tx.send(crate::notif::Cmd::Clear); String::new() }
            ("notif", "invoke") => { let _ = self.notif_tx.send(crate::notif::Cmd::Invoke(a(0).parse().unwrap_or(0), if a(1).is_empty() { "default".into() } else { a(1) })); String::new() }
            ("phone", "send") => { let v: serde_json::Value = serde_json::from_str(&a(0)).unwrap_or_default(); self.phone.send(v["c"].as_str().unwrap_or(""), v.get("a").cloned()); String::new() }
            ("phone", "tel") => { let v: serde_json::Value = serde_json::from_str(&a(0)).unwrap_or_default(); self.phone.tel(v["c"].as_str().unwrap_or(""), v.get("a").cloned()); String::new() }
            ("phone", "answer") => { self.phone.answer(); String::new() }
            ("phone", "hangup") => { self.phone.hangup(); String::new() }
            ("phone", "call") => { self.phone.call_number(&a(0)); String::new() }
            ("mist", "media") => {
                let ps = &self.island.players;
                let p = ps.iter().find(|p| p.playing).or_else(|| ps.iter().find(|p| !p.title.is_empty())).or(ps.first()).cloned();
                if let Some(p) = p {
                    let _ = self.media_tx.send(match a(0).as_str() { "next" => crate::media::Cmd::Next(p.bus), "prev" => crate::media::Cmd::Prev(p.bus), _ => crate::media::Cmd::Toggle(p.bus) });
                }
                String::new()
            }
            ("ui", "ping") => "pong".into(),
            // window snapping debug hooks (were Snap.qml's)
            ("snap", "state") => match self.pane.as_mut().and_then(|p| p.p.as_drag()) {
                Some(d) => serde_json::json!([d.cx, d.cy, d.addr, d.grab, format!("{:?}", d.mode), d.zone(), true]).to_string(),
                None => "[null,null,null,null,\"\",null,false]".into(),
            },
            ("snap", "assist") => {
                let titles = self.pane.as_mut().and_then(|p| p.p.as_assist()).map(|a| a.titles()).unwrap_or_default();
                let s = self.snap.assist.clone().unwrap_or_default();
                serde_json::json!([s.mon, s.zones, s.group, titles]).to_string()
            }
            ("snap", "assistPick") => {
                let i: usize = a(0).parse().unwrap_or(0);
                let act = self.pane.as_mut().and_then(|p| p.p.as_assist()).map(|p| p.pick(i));
                if let Some(act) = act {
                    self.pane_act(act);
                }
                String::new()
            }
            // tests: a plugin event / the dragged cursor (`msg snap ev mistdrag begin,0x..,x,y,w,h`, `msg snap cursor x y`)
            ("snap", "ev") => {
                self.snap_event(&a(0), &a(1));
                String::new()
            }
            ("snap", "cursor") => {
                self.snap_cursor(a(0).parse().unwrap_or(0.0), a(1).parse().unwrap_or(0.0));
                String::new()
            }
            ("snap", "fly") => {
                self.snap_open_fly(&a(0), 900.0, 80.0, true);
                String::new()
            }
            ("snap", "close") => {
                self.snap_close_extras();
                String::new()
            }
            ("snap", "preview") | ("snap", "edge") => {
                self.pane_close();
                if a(0) == "true" {
                    let mut d = crate::snap::Drag::new("", None, crate::snap::monitors(), 960.0, 540.0);
                    if let Some(i) = d.mons.iter().position(|m| m.name == primary_monitor()) {
                        d.scr = i;
                    }
                    d.set_preview(true, r.func == "edge");
                    self.pane_open_box(Box::new(crate::snap::DragPane(d)), primary_monitor());
                }
                String::new()
            }
            // tests: a key into the open popup (`msg ui key Right|Left|Up|Down|Tab|BackTab|Return|Escape|space|<char>`)
            ("ui", "key") => {
                let n = a(0);
                let keysym = match n.as_str() {
                    "Right" => Keysym::Right,
                    "Left" => Keysym::Left,
                    "Up" => Keysym::Up,
                    "Down" => Keysym::Down,
                    "Tab" => Keysym::Tab,
                    "BackTab" => Keysym::ISO_Left_Tab,
                    "Return" => Keysym::Return,
                    "Escape" => Keysym::Escape,
                    "space" => Keysym::space,
                    _ => Keysym::from_char(n.chars().next().unwrap_or(' ')),
                };
                let utf8 = if n.chars().count() == 1 { Some(n.clone()) } else if n == "space" { Some(" ".into()) } else { None };
                let ev = KeyEvent { time: 0, raw_code: 0, keysym, utf8 };
                match self.pane.as_mut() {
                    Some(p) => {
                        let mut cx = crate::pane::Ctx { t: &mut self.text, cal: &self.island.calendar, icons: &mut self.icons, imgs: &mut self.imgs, apps: &self.apps, notif_hidden: &mut self.notif_hidden, ctrl: self.ctrl, st: &self.st, players: &self.island.players, phone_name: &self.island.phone_name, wx: &self.desk.weather };
                        let act = p.p.key(&ev, &mut cx);
                        self.pane_act(act);
                        String::new()
                    }
                    None => "no popup".into(),
                }
            }
            // test hook: a left click (or `wheel <d>`) at x y in the popup's surface coordinates
            ("ui", "click") | ("ui", "wheel") => {
                let (x, y) = (a(0).parse::<f32>().unwrap_or(0.0), a(1).parse::<f32>().unwrap_or(0.0));
                let evs = if r.func == "wheel" { vec![crate::pane::Ptr::Motion, crate::pane::Ptr::Wheel(a(2).parse().unwrap_or(1.0))] } else { vec![crate::pane::Ptr::Motion, crate::pane::Ptr::Press(0x110), crate::pane::Ptr::Release(0x110)] };
                match self.pane.is_some() {
                    true => {
                        for ev in evs {
                            let Some(p) = self.pane.as_mut() else { break };
                            let mut cx = crate::pane::Ctx { t: &mut self.text, cal: &self.island.calendar, icons: &mut self.icons, imgs: &mut self.imgs, apps: &self.apps, notif_hidden: &mut self.notif_hidden, ctrl: self.ctrl, st: &self.st, players: &self.island.players, phone_name: &self.island.phone_name, wx: &self.desk.weather };
                            let act = p.p.pointer(ev, x, y, &mut cx);
                            self.pane_act(act);
                        }
                        String::new()
                    }
                    false => "no popup".into(),
                }
            }
            ("mist", "toggle") => {
                let n = a(0);
                if n == "control" { self.ui_control("", "") } else { self.ui_toggle(&n, "") }
                String::new()
            }
            ("mist", "toggleOn") => {
                self.ui_toggle(&a(0), &a(1));
                String::new()
            }
            ("mist", "control") => {
                self.ui_control(&a(0), "");
                String::new()
            }
            ("mist", "controlOn") => {
                self.ui_control(&a(0), &a(1));
                String::new()
            }
            ("mist", "close") => {
                self.pane_close();
                if self.dock.start_open {
                    self.dock.start_open = false;
                    self.dock.update_y();
                    if let Some(d) = &mut self.dock_surf {
                        d.dirty = true;
                    }
                }
                String::new()
            }
            ("ui", "popup") => self.popup_name(),
            ("shot", "toolbar") => {
                if self.pane.as_ref().map(|p| p.p.name() == "shot").unwrap_or(false) {
                    self.pane_close();
                } else {
                    let o = self.shot_opts;
                    let scr = self.focused_screen();
                    self.pane_open_box(Box::new(crate::shot::Toolbar::new(crate::shot::Opts { mode: o.0, delay: o.1, copy: o.2 })), &scr);
                }
                String::new()
            }
            // test hook: the thumbnail for an existing picture
            ("shot", "thumb") => {
                self.thumb_show(a(0));
                String::new()
            }
            ("shot", m @ ("screen" | "window" | "region")) => {
                let m: &'static str = match m { "screen" => "screen", "window" => "window", _ => "region" };
                self.pane_close();
                self.shot_opts.0 = m;
                crate::shot::capture(&crate::shot::Opts { mode: m, delay: self.shot_opts.1, copy: self.shot_opts.2 }, self.tx.clone());
                String::new()
            }
            ("switcher", "next") => {
                self.switcher_step(1);
                String::new()
            }
            ("switcher", "prev") => {
                self.switcher_step(-1);
                String::new()
            }
            ("lock", "lock") => {
                self.idle.lock(None);
                String::new()
            }
            ("lock", "preview") => {
                self.idle.lock(Some(r.args.first().and_then(|s| s.parse().ok()).unwrap_or(8)));
                String::new()
            }
            ("lock", "isLocked") => crate::idle::Idle::locked().to_string(),
            ("polkit", "state") => self.polkit.as_ref().map(|p| format!("open {} queue {}", p.0.req.action, self.polkit_queue.len())).unwrap_or("closed".into()),
            ("polkit", "cancel") => {
                if let Some(p) = self.polkit.as_mut() {
                    p.0.cancel();
                }
                self.polkit_close();
                String::new()
            }
            ("ui", "stats") => DRAWS.with(|d| format!("{:?} fades {} dock {} hide {} show {} island {} wall {} desk {} {}", d.borrow(), self.fades.iter().any(|f| f.4.elapsed().as_secs_f32() * 1000.0 < FADE), self.dock.animating(), self.dock_hide_at.is_some(), self.dock_show_at.is_some(), self.island.animating(), self.wall.busy(), self.desk.fx.running(), self.desk.wx.fx.running())),
            _ => "unknown".into(),
        };
        let _ = r.reply.send(out);
        self.island_dirty = true;
    }


    fn commit(pool: &mut SlotPool, layer: &LayerSurface, canvas: &Canvas) {
        let (wi, hi) = (canvas.pm.width() as i32, canvas.pm.height() as i32);
        Self::commit_box(pool, layer, canvas, (0, 0, wi, hi));
    }
    // the whole canvas into a new buffer, `damage` (buffer pixels) marked as changed
    fn commit_box(pool: &mut SlotPool, layer: &LayerSurface, canvas: &Canvas, damage: (i32, i32, i32, i32)) {
        let (wi, hi) = (canvas.pm.width() as i32, canvas.pm.height() as i32);
        let t0 = Instant::now();
        let Ok((buffer, data)) = pool.create_buffer(wi, hi, wi * 4, wl_shm::Format::Argb8888) else { return };
        canvas.to_bgra(data);
        count("commit_us", t0.elapsed().as_micros() as u64);
        count("commits", 1);
        let s = layer.wl_surface();
        s.damage_buffer(damage.0, damage.1, damage.2, damage.3);
        let _ = buffer.attach_to(s);
        layer.commit();
    }

    // ---------------------------------------------------------------- bar
    fn draw_bar(&mut self, i: usize) {
        DRAWS.with(|d| *d.borrow_mut().entry("bar").or_insert(0) += 1);
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
        DRAWS.with(|d| *d.borrow_mut().entry("dock").or_insert(0) += 1);
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
        // the wallpaper gallery covers the main monitor: the dock slides away (it is Overlay, above the preview)
        let gallery = self.pane.as_ref().map(|p| p.p.name() == "wallpaper" && p.screen == primary_monitor()).unwrap_or(false);
        if gallery {
            self.dock.hovered = false;
        }
        let bare = self.primary_bare() && !gallery;
        if bare != self.dock.bare {
            self.dock.bare = bare;
            if !bare && !self.dock.start_open {
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
                if !self.dock.start_open {
                    self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
                }
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
            Slot::Start => self.ui_toggle("start", primary_monitor()),
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
                // more than one window: pick one (Windows taskbar thumbnails)
                if app.wins.len() > 1 {
                    self.open_winpick(i, &app);
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
                // the app is the active one: a click hides it (all its windows, back with the next click).
                // Asked now: the cached state can trail a restore's focus changes
                let active = crate::sys::hypr_request("j/activewindow").and_then(|a| a["address"].as_str().map(String::from)).unwrap_or_default();
                if !active.is_empty() && app.wins.iter().any(|w| w.address == active) {
                    for w in &app.wins {
                        hypr(&format!("MistMinimize('{}')", w.address));
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

    // the window picker above the dock icon (winpick.rs), thumbnails captured once on a thread (wincap.rs)
    fn open_winpick(&mut self, i: usize, app: &dock::App) {
        // the dock stays up while it is open (its target position, also when opened by `msg dock click`)
        self.dock.menu_open = true;
        self.dock.update_y();
        self.dock.update_lift();
        let Some(d) = self.dock_surf.as_ref() else { return };
        let anchor_x = (d.out_w - d.w as i32) as f32 / 2.0 + self.dock.slot_x(d.w as f32)[i] + dock::SIZE / 2.0;
        let bottom = (d.out_h - dock::WIN_H as i32) as f32 + self.dock.y.target() - 6.0;
        let output = d.output.clone();
        let Some(scr) = self.outs().into_iter().find(|(o, _)| *o == output).map(|o| o.1) else { return };
        let wins: Vec<crate::winpick::Win> = app.wins.iter().map(|w| crate::winpick::Win { address: w.address.clone(), title: w.title.clone(), minimized: w.ws_name == "special:minimized" }).collect();
        let p = crate::winpick::WinPick::new(app.icon.clone(), wins, anchor_x, bottom, self.hypr.focused_ws);
        let addrs = p.addresses();
        self.pane_open_box(Box::new(p), &scr);
        self.dock.menu_open = true;
        if let Some(d) = &mut self.dock_surf {
            d.dirty = true;
        }
        if let Some(tx) = self.tx.clone() {
            crate::wincap::capture(addrs, crate::winpick::THUMB_W, crate::winpick::THUMB_H, tx);
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
        if self.island_surf.as_ref().map(|i| i.layer.wl_surface() == &e.surface).unwrap_or(false) {
            self.island_pointer(e);
            return;
        }
        if self.pane.as_ref().map(|p| p.layer.wl_surface() == &e.surface).unwrap_or(false) {
            self.pane_pointer(e);
            return;
        }
        if self.thumb.as_ref().map(|t| t.1.wl_surface() == &e.surface).unwrap_or(false) {
            let t = self.thumb.as_mut().unwrap();
            t.0.ptr = (e.position.0 as f32, e.position.1 as f32);
            t.3 = true;
            t.5 = Instant::now(); // hovered: stays
            match e.kind {
                PointerEventKind::Enter { serial } => self.set_cursor(serial, wp_cursor_shape_device_v1::Shape::Pointer),
                PointerEventKind::Leave { .. } => t.0.ptr = (-1.0, -1.0),
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    let cmd = t.0.click();
                    self.thumb = None;
                    if let Some(c) = cmd {
                        let v: Vec<&str> = c.iter().map(|s| s.as_str()).collect();
                        spawn(&v);
                    }
                }
                _ => {}
            }
            return;
        }
        if self.polkit.as_ref().map(|p| p.1.wl_surface() == &e.surface).unwrap_or(false) {
            let (x, y) = (e.position.0 as f32, e.position.1 as f32);
            let p = self.polkit.as_mut().unwrap();
            match e.kind {
                PointerEventKind::Enter { serial } => {
                    self.desk_serial = serial;
                    self.set_cursor(serial, wp_cursor_shape_device_v1::Shape::Default);
                }
                PointerEventKind::Motion { .. } => {
                    let h = p.0.hit(x, y);
                    if h != p.0.hover {
                        p.0.hover = h;
                        p.3 = true;
                    }
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => match p.0.hit(x, y) {
                    Some(0) => {
                        p.0.cancel();
                        self.polkit_close();
                    }
                    Some(1) => {
                        p.0.submit();
                        p.3 = true;
                    }
                    _ => {}
                },
                _ => {}
            }
            return;
        }
        if self.desk.stage.as_ref().map(|l| l.wl_surface() == &e.surface).unwrap_or(false) {
            self.desk_pointer(e, false);
            return;
        }
        if self.desk.card.as_ref().map(|l| l.wl_surface() == &e.surface).unwrap_or(false) {
            self.desk_pointer(e, true);
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
            Hit::Start => self.ui_toggle("start", &scr),
            Hit::Stats => spawn(&["kitty", "-e", "btop"]),
            Hit::Phone => if mid { self.phone.send("ring", None) } else { self.ui_toggle("phone", &scr) },
            Hit::Bluetooth => self.ui_control("bluetooth", &scr),
            Hit::Network => self.ui_control("network", &scr),
            Hit::Volume => if mid { sh("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle") } else { self.ui_control("sound", &scr) },
            Hit::Bell => if right { let on = !self.st.dnd; let _ = self.notif_tx.send(crate::notif::Cmd::Dnd(on)); self.st.dnd = on; self.mark_all(); } else { self.ui_toggle("notifications", &scr) },
            Hit::Adjust => self.ui_control("", &scr),
            Hit::Power => self.ui_toggle("session", &scr),
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
                if !self.dock.start_open {
                    self.dock_hide_at = Some(Instant::now() + Duration::from_millis(400));
                }
            }
        }
    }
    fn draw_popup(&mut self) {
        DRAWS.with(|d| *d.borrow_mut().entry("popup").or_insert(0) += 1);
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
pub fn hypr(lua: &str) {
    spawn(&["hyprctl", "eval", lua]);
}
fn focus_win(addr: &str) {
    let a = if addr.starts_with("0x") { addr.to_string() } else { format!("0x{addr}") };
    hypr(&format!("hl.dispatch(hl.dsp.focus({{ window = \"address:{a}\" }})); hl.dispatch(hl.dsp.window.bring_to_top())"));
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
        // the bars and the dock draw only st, hypr, the tray and the popup: these messages touch none of it
        // (with music on, Audio + Players alone redrew both bars and the dock ~8 times a second)
        let bars_too = !matches!(&m, Msg::Players(..) | Msg::Audio(..) | Msg::Data(..) | Msg::Cursor(..) | Msg::HyprEv(..) | Msg::Note(..) | Msg::Phone2(..) | Msg::Profile(..) | Msg::Sys(..) | Msg::NmLine(..) | Msg::BtDevs(..) | Msg::WinThumb(..));
        let k: &'static str = match &m {
            Msg::Hypr(..) => "Hypr",
            Msg::Net(..) => "Net",
            Msg::Bt(..) => "Bt",
            Msg::Volume(..) => "Volume",
            Msg::Stats(..) => "Stats",
            Msg::Phone(..) => "Phone",
            Msg::Ui(..) => "Ui",
            Msg::Dnd(..) => "Dnd",
            Msg::Tray(..) => "Tray",
            Msg::Pins => "Pins",
            Msg::Ipc(..) => "Ipc",
            Msg::Note(..) => "Note",
            Msg::Phone2(..) => "Phone2",
            Msg::Players(..) => "Players",
            Msg::Data(..) => "Data",
            Msg::HyprEv(..) => "HyprEv",
            Msg::Audio(..) => "Audio",
            Msg::BtDevs(..) => "BtDevs",
            Msg::Profile(..) => "Profile",
            Msg::Sys(..) => "Sys",
            Msg::NmLine(..) => "NmLine",
            Msg::Cursor(..) => "Cursor",
            Msg::Wall(..) => "Wall",
            Msg::Lock => "Lock",
            Msg::IdleCfg => "IdleCfg",
            Msg::Polkit(..) => "Polkit",
            Msg::Shot(..) => "Shot",
            Msg::WinThumb(..) => "WinThumb",
        };
        DRAWS.with(|d| *d.borrow_mut().entry(k).or_insert(0) += 1);
        match m {
            Msg::Hypr(h) => {
                self.hypr = h;
                self.dock_rebuild();
                // only real fullscreen (mode 2) on the island's monitor hides the idle pill
                let mon = self.hypr.monitors.iter().find(|m| m.1 == primary_monitor()).map(|m| m.2);
                self.island.fullscreen = self.hypr.clients.iter().any(|c| Some(c.ws_id) == mon && c.fullscreen == 2);
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
            Msg::Ui(p, s) => {
                let was_start = self.dock.start_open;
                let is_start = p == "start" || self.pane.as_ref().map(|q| q.p.name() == "start").unwrap_or(false);
                self.dock.start_open = is_start;
                self.ui_popup = (p, s);
                if was_start != is_start {
                    if is_start {
                        self.dock_hide_at = None;
                    }
                    self.dock.update_y();
                    if let Some(d) = &mut self.dock_surf {
                        d.dirty = true;
                    }
                }
            }
            Msg::Dnd(d) => {
                self.st.dnd = d;
                if let Some(p) = self.pane.as_mut() {
                    if p.p.data("notifications") {
                        p.dirty = true;
                    }
                }
            }
            Msg::Lock => self.idle.lock(None),
            Msg::Polkit(ev) => self.on_polkit(ev),
            Msg::Shot(f) => self.thumb_show(f),
            Msg::WinThumb(a, pm) => {
                if let Some(p) = self.pane.as_mut() {
                    if p.p.as_winpick().map(|w| w.thumb(a, pm)).unwrap_or(false) {
                        p.dirty = true;
                    }
                }
            }
            Msg::IdleCfg => {
                if self.idle.changed() {
                    if let Some(s) = self.seat.clone() {
                        let qh = self.qh.clone();
                        self.idle.arm(&s, &qh);
                    }
                }
            }
            Msg::Wall(live, image) => {
                let outs = self.outs();
                self.wall.set(live, image, &self.compositor, &self.layer_shell, &self.qh, &outs);
            }
            Msg::Tray(t) => self.tray_items = t,
            Msg::Ipc(r) => self.on_ipc(r),
            Msg::Note(n) => {
                let notes = self.phone.st.lock().unwrap().notes();
                self.island.on_note(&n, &self.apps, &notes);
            }
            Msg::Phone2(ev) => {
                let (call, bat) = { let s = self.phone.st.lock().unwrap(); (s.call(), s.battery()) };
                let key = call.as_ref().map(|c| format!("{}|{}", c["path"], c["state"])).unwrap_or_default();
                if key != self.last_call_key {
                    self.last_call_key = key;
                    self.island.on_call(call.as_ref());
                }
                if let Some(p) = self.pane.as_mut() {
                    let a = p.p.data("phone");
                    let b = ev.as_ref().map(|e| p.p.phone_ev(e)).unwrap_or(false);
                    if a || b {
                        p.dirty = true;
                    }
                }
                if let Some(ev) = ev {
                    self.island.on_phone_event(&ev, bat);
                }
            }
            Msg::Players(ps) => self.island.set_players(ps),
            Msg::Data(name, v) => match name.as_str() {
                "weather" => {
                    self.desk.weather = v.clone();
                    self.desk.dirty = true;
                    self.desk.card_dirty = true;
                    self.island.on_weather(v)
                }
                "garmin" => {
                    self.desk.garmin = v;
                    self.desk.dirty = true;
                    self.desk.card_dirty = true;
                }
                "running" => {
                    self.desk.running = v;
                    self.desk.dirty = true;
                }
                "calendar" => self.island.calendar = v,
                _ => {}
            },
            Msg::Cursor(x, y) => self.snap_cursor(x, y),
            Msg::HyprEv(n, d) if n == "mistdrag" || n == "mistsnap" => self.snap_event(&n, &d),
            Msg::HyprEv(n, d) => {
                let active = self.hypr.clients.iter().find(|c| c.focus == 0).map(|c| c.address.clone()).unwrap_or_default();
                self.island.on_hypr(&n, &d, &self.apps, &self.hypr.clients, &active);
            }
            Msg::Audio(a) => self.island.on_audio(a),
            Msg::BtDevs(d) => { let on = self.st.bt_on; self.island.on_bt(on, Some(d)); }
            Msg::Profile(p) => self.island.on_profile(p),
            Msg::Sys(v) => self.island.on_sys(&v),
            Msg::NmLine(l) => self.island.on_nm(&l),
            Msg::Pins => {
                self.dock.load_pins();
                self.dock.forget_tiles();
                self.dock_rebuild();
            }
        }
        if bars_too {
            self.mark_all();
        }
        self.island_dirty = true;
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
        self.wall.remove_output(&o);
        if self.dock_surf.as_ref().map(|d| d.output == o).unwrap_or(false) {
            self.dock_surf = None;
        }
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface) {
        self.bars.retain(|b| &b.layer != l);
        self.wall.closed(l);
        if self.pane.as_ref().map(|p| &p.layer == l).unwrap_or(false) {
            self.pane = None;
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        if self.wall.configure(l, c.new_size.0, c.new_size.1) {
            return;
        }
        if let Some(t) = self.thumb.as_mut() {
            if &t.1 == l {
                t.2 = true;
                t.3 = true;
                return;
            }
        }
        if let Some(p) = self.pane.as_mut() {
            if &p.layer == l {
                let (w, h) = (c.new_size.0.max(1), c.new_size.1.max(1));
                if p.canvas.pm.width() != w || p.canvas.pm.height() != h {
                    p.canvas = Canvas::new(w, h);
                }
                p.configured = true;
                p.dirty = true;
                return;
            }
        }
        if let Some(p) = self.polkit.as_mut() {
            if &p.1 == l {
                let (w, h) = (c.new_size.0.max(1), c.new_size.1.max(1));
                if p.4.pm.width() != w || p.4.pm.height() != h {
                    p.4 = Canvas::new(w, h);
                }
                p.2 = true;
                p.3 = true;
                return;
            }
        }
        if self.desk.stage.as_ref() == Some(l) {
            self.desk.stage_cfg = true;
            self.desk.dirty = true;
            return;
        }
        if self.desk.card.as_ref() == Some(l) {
            self.desk.card_cfg = true;
            self.desk.card_dirty = true;
            return;
        }
        if let Some(b) = self.bars.iter_mut().find(|b| &b.layer == l) {
            b.w = c.new_size.0.max(1);
            b.configured = true;
            b.dirty = true;
            return;
        }
        if let Some(i) = self.island_surf.as_mut() {
            if &i.layer == l {
                i.configured = true;
                i.input = None;
                self.island_dirty = true;
                return;
            }
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
                        data.fill(0);
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
        if self.seat.is_none() {
            self.idle.arm(&seat, qh);
            self.seat = Some(seat.clone());
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
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
impl KeyboardHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        if self.polkit.is_some() {
            self.polkit_key(&ev);
            return;
        }
        if let Some(p) = self.pane.as_mut() {
            let mut cx = crate::pane::Ctx { t: &mut self.text, cal: &self.island.calendar, icons: &mut self.icons, imgs: &mut self.imgs, apps: &self.apps, notif_hidden: &mut self.notif_hidden, ctrl: self.ctrl, st: &self.st, players: &self.island.players, phone_name: &self.island.phone_name, wx: &self.desk.weather };
            let a = p.p.key(&ev, &mut cx);
            self.pane_act(a);
            return;
        }
        if !self.island.replying {
            return;
        }
        match ev.keysym {
            Keysym::Return | Keysym::KP_Enter => self.reply_send(),
            Keysym::Escape => self.island.replying = false,
            Keysym::BackSpace => { self.island.reply.pop(); }
            _ => {
                if let Some(t) = ev.utf8 {
                    if !t.chars().any(|c| c.is_control()) {
                        self.island.reply.push_str(&t);
                    }
                }
            }
        }
        self.island_dirty = true;
    }
    fn repeat_key(&mut self, c: &Connection, q: &QueueHandle<Self>, k: &wl_keyboard::WlKeyboard, s: u32, ev: KeyEvent) {
        self.press_key(c, q, k, s, ev);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        if let Some(p) = self.pane.as_mut() {
            let a = p.p.key_release(&ev);
            self.pane_act(a);
        }
    }
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: Modifiers, _: RawModifiers, _: u32) {
        self.ctrl = m.ctrl;
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
delegate_keyboard!(App);
delegate_layer!(App);
delegate_registry!(App);
wayland_client::delegate_noop!(App: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
wayland_client::delegate_noop!(App: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
wayland_client::delegate_noop!(App: ignore wp_viewporter::WpViewporter);
wayland_client::delegate_noop!(App: ignore wp_viewport::WpViewport);

pub fn count(k: &'static str, v: u64) {
    DRAWS.with(|d| *d.borrow_mut().entry(k).or_insert(0) += v);
}
thread_local! { static DRAWS: std::cell::RefCell<std::collections::BTreeMap<&'static str, u64>> = Default::default(); }

pub fn tick_interval() -> Duration {
    Duration::from_millis(16)
}
