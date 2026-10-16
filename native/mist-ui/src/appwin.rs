// Runtime for Mist's own apps (Mist Files, Mist Player): ordinary xdg toplevel windows drawn on the CPU like
// the shell surfaces (tiny-skia canvas into wl_shm, no GPU context), one process per app with any number of
// windows. The app is a `Logic`: it paints a window into a Canvas and gets input as `Ev`; what it wants done
// with the windows (new, close, redraw, move, cursor, drag, timers) it queues in `Cx`, applied after it returns.
// Single instance: `$XDG_RUNTIME_DIR/mist/<name>.sock`; a second launch sends its arguments there and exits.
use crate::gfx::Canvas;
use calloop::{generic::Generic, EventLoop, Interest, LoopHandle, Mode, PostAction};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::{
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{DataSourceHandler, DragSource},
        DataDeviceManagerState, WritePipe,
    },
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, delegate_subcompositor, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::client::protocol::wl_data_device_manager::DndAction,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
    subcompositor::SubcompositorState,
};
use std::any::Any;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_data_device, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

pub type WinId = u32;

pub const LEFT: u32 = 0x110;
pub const RIGHT: u32 = 0x111;
pub const MIDDLE: u32 = 0x112;

#[derive(Clone, Copy, Default, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug)]
pub enum Ev {
    Motion(f32, f32),
    Leave,
    // button, clicks (1 or 2 = double click)
    Press(f32, f32, u32, u8),
    Release(f32, f32, u32),
    // wheel / touchpad in px (positive = down), at the pointer
    Scroll(f32, f32, f32),
    Key(Keysym, Option<String>),
    Focus(bool),
    // drag and drop onto the window (text/uri-list)
    DragMotion(f32, f32),
    DragLeave,
    Drop(f32, f32, Vec<String>),
    // the size changed (already applied)
    Resized,
    // the compositor made the window fullscreen (true) or not
    Fullscreen(bool),
}

pub enum Cmd {
    NewWindow { title: String, w: u32, h: u32, min: (u32, u32), token: u64 },
    Close(WinId),
    Title(WinId, String),
    Redraw(WinId),
    // keep redrawing every frame while true (animations)
    Animate(WinId, bool),
    Move(WinId),
    Minimize(WinId),
    Maximize(WinId),
    Fullscreen(WinId, bool),
    HideCursor(bool),
    IdleInhibit(WinId, bool),
    Cursor(CursorIcon),
    WakeAt(Instant),
    // drag these uris out of the window (started by the last press), with an icon
    Drag(WinId, Vec<String>, Option<(u32, u32, Vec<u8>)>),
    Exit,
}

pub struct Cx {
    pub cmds: Vec<Cmd>,
    pub mods: Mods,
    pub tx: calloop::channel::Sender<Box<dyn Any + Send>>,
    next_token: u64,
}
impl Cx {
    pub fn redraw(&mut self, id: WinId) {
        self.cmds.push(Cmd::Redraw(id));
    }
    pub fn wake_at(&mut self, t: Instant) {
        self.cmds.push(Cmd::WakeAt(t));
    }
    pub fn cursor(&mut self, c: CursorIcon) {
        self.cmds.push(Cmd::Cursor(c));
    }
    // a new window; its id arrives in Logic::opened with the token returned here
    pub fn new_window(&mut self, title: &str, w: u32, h: u32, min: (u32, u32)) -> u64 {
        self.next_token += 1;
        self.cmds.push(Cmd::NewWindow { title: title.into(), w, h, min, token: self.next_token });
        self.next_token
    }
}

pub trait Logic {
    fn app_id(&self) -> &str;
    // a window asked for with Cx::new_window exists (not yet configured)
    fn opened(&mut self, id: WinId, token: u64, cx: &mut Cx);
    fn paint(&mut self, id: WinId, c: &mut Canvas, cx: &mut Cx);
    fn event(&mut self, id: WinId, ev: Ev, cx: &mut Cx);
    // the compositor (or the user) asked to close the window
    fn close_requested(&mut self, id: WinId, cx: &mut Cx);
    // a WakeAt time passed
    fn tick(&mut self, cx: &mut Cx);
    // arguments from another launch (single instance)
    fn message(&mut self, args: Vec<String>, cx: &mut Cx);
    // anything sent on Cx::tx by worker threads
    fn async_msg(&mut self, m: Box<dyn Any + Send>, cx: &mut Cx);
    // optional video plane under the canvas: where it goes in the window (x, y, w, h); None = no plane
    fn video_rect(&mut self, _id: WinId) -> Option<(i32, i32, i32, i32)> {
        None
    }
    // the plane's wl_surface was made (Some) or is about to go (None): the app draws it itself (EGL)
    fn video_surface(&mut self, _id: WinId, _s: Option<VideoSurface>, _cx: &mut Cx) {}
}

// raw pointers for wl_egl_window_create / eglGetDisplay (libwayland-client objects)
#[derive(Clone, Copy)]
pub struct VideoSurface {
    pub surface: *mut std::ffi::c_void,
    pub display: *mut std::ffi::c_void,
}

struct Win {
    id: WinId,
    window: Window,
    w: u32,
    h: u32,
    configured: bool,
    dirty: bool,
    animate: bool,
    frame_pending: bool,
    canvas: Canvas,
    // video plane: a subsurface the app draws with EGL (x, y, w, h)
    video: Option<(wl_surface::WlSurface, smithay_client_toolkit::reexports::client::protocol::wl_subsurface::WlSubsurface, (i32, i32, i32, i32))>,
    min: (u32, u32),
    fullscreen: bool,
    inhibitor: Option<zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1>,
}

pub struct Rt {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    subcompositor: SubcompositorState,
    viewporter: Option<wp_viewporter::WpViewporter>,
    inhibit_mgr: Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>,
    cursor_hidden: bool,
    shm: Shm,
    seats: SeatState,
    xdg: XdgShell,
    ddm: DataDeviceManagerState,
    device: Option<DataDevice>,
    drag_source: Option<DragSource>,
    drag_uris: Vec<String>,
    drag_icon: Option<wl_surface::WlSurface>,
    drag_win: Option<WinId>,
    pool: SlotPool,
    qh: QueueHandle<Rt>,
    conn: Connection,
    lh: LoopHandle<'static, Rt>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    seat: Option<wl_seat::WlSeat>,
    wins: Vec<Win>,
    next_id: WinId,
    logic: Box<dyn Logic>,
    cx: Cx,
    mods: Mods,
    kb_win: Option<WinId>,
    ptr_win: Option<WinId>,
    last_press: (u32, Instant, f32, f32, u32),
    press_serial: u32,
    enter_serial: u32,
    cursor: CursorIcon,
    wake: Option<Instant>,
    exit: bool,
}

fn sock_path(name: &str) -> String {
    let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let _ = std::fs::create_dir_all(&dir);
    format!("{dir}/{name}.sock")
}

// hand the arguments to a running instance; true when one took them
pub fn forward(name: &str, args: &[String]) -> bool {
    let Ok(mut s) = UnixStream::connect(sock_path(name)) else { return false };
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    let mut msg = String::new();
    for a in args {
        msg.push_str(&a.replace('\n', " "));
        msg.push('\n');
    }
    if s.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    let _ = s.shutdown(std::net::Shutdown::Write);
    // wait for the instance to read it (it closes the socket)
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut b = [0u8; 8];
    matches!(s.read(&mut b), Ok(_))
}

// run the app until it exits; `name` is the single-instance socket name
pub fn run(name: &str, logic: Box<dyn Logic>) -> i32 {
    let path = sock_path(name);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).ok();
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{name}: wayland: {e}");
            return 1;
        }
    };
    let (globals, queue) = registry_queue_init::<Rt>(&conn).expect("registry");
    let qh = queue.handle();
    let mut ev: EventLoop<'static, Rt> = EventLoop::try_new().unwrap();
    let lh = ev.handle();
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue).insert(lh.clone()).unwrap();
    let (tx, rx) = calloop::channel::channel::<Box<dyn Any + Send>>();
    lh.insert_source(rx, |e, _, rt: &mut Rt| {
        if let calloop::channel::Event::Msg(m) = e {
            rt.with_logic(|l, cx| l.async_msg(m, cx));
        }
    })
    .unwrap();
    if let Some(l) = listener {
        l.set_nonblocking(true).ok();
        lh.insert_source(Generic::new(l, Interest::READ, Mode::Level), |_, l, rt: &mut Rt| {
            while let Ok((mut s, _)) = l.accept() {
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                let _ = s.write_all(b"ok");
                let args: Vec<String> = buf.lines().map(String::from).collect();
                rt.with_logic(|lg, cx| lg.message(args, cx));
            }
            Ok(PostAction::Continue)
        })
        .unwrap();
    }
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let pool = SlotPool::new(1200 * 800 * 4, &shm).expect("pool");
    let compositor = CompositorState::bind(&globals, &qh).expect("compositor");
    let subcompositor = SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh).expect("subcompositor");
    let viewporter = globals.bind::<wp_viewporter::WpViewporter, _, _>(&qh, 1..=1, ()).ok();
    let mut rt = Rt {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        subcompositor,
        viewporter,
        inhibit_mgr: globals.bind::<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, _, _>(&qh, 1..=1, ()).ok(),
        cursor_hidden: false,
        seats: SeatState::new(&globals, &qh),
        xdg: XdgShell::bind(&globals, &qh).expect("xdg shell"),
        ddm: DataDeviceManagerState::bind(&globals, &qh).expect("data device"),
        device: None,
        drag_source: None,
        drag_uris: Vec::new(),
        drag_icon: None,
        drag_win: None,
        shm,
        pool,
        qh: qh.clone(),
        conn: conn.clone(),
        lh: lh.clone(),
        keyboard: None,
        pointer: None,
        seat: None,
        wins: Vec::new(),
        next_id: 1,
        logic,
        cx: Cx { cmds: Vec::new(), mods: Mods::default(), tx, next_token: 0 },
        mods: Mods::default(),
        kb_win: None,
        ptr_win: None,
        last_press: (0, Instant::now(), 0.0, 0.0, 0),
        press_serial: 0,
        enter_serial: 0,
        cursor: CursorIcon::Default,
        wake: None,
        exit: false,
    };
    // the app opens its first windows from here (message with the launch arguments)
    let args: Vec<String> = std::env::args().skip(2).collect();
    rt.with_logic(|l, cx| l.message(args, cx));
    while !rt.exit {
        let timeout = rt.wake.map(|t| t.saturating_duration_since(Instant::now()));
        if ev.dispatch(timeout, &mut rt).is_err() {
            break;
        }
        if let Some(t) = rt.wake {
            if Instant::now() >= t {
                rt.wake = None;
                rt.with_logic(|l, cx| l.tick(cx));
            }
        }
        rt.flush_draws();
    }
    let _ = std::fs::remove_file(&path);
    0
}

impl Rt {
    fn with_logic(&mut self, f: impl FnOnce(&mut dyn Logic, &mut Cx)) {
        self.cx.mods = self.mods;
        f(self.logic.as_mut(), &mut self.cx);
        self.apply();
    }
    fn win(&mut self, id: WinId) -> Option<&mut Win> {
        self.wins.iter_mut().find(|w| w.id == id)
    }
    fn id_of(&self, s: &wl_surface::WlSurface) -> Option<WinId> {
        self.wins.iter().find(|w| w.window.wl_surface() == s).map(|w| w.id)
    }
    fn apply(&mut self) {
        loop {
            let cmds = std::mem::take(&mut self.cx.cmds);
            if cmds.is_empty() {
                break;
            }
            self.apply_batch(cmds);
        }
    }
    fn apply_batch(&mut self, cmds: Vec<Cmd>) {
        for c in cmds {
            match c {
                Cmd::NewWindow { title, w, h, min, token } => {
                    let surface = self.compositor.create_surface(&self.qh);
                    let window = self.xdg.create_window(surface, WindowDecorations::RequestClient, &self.qh);
                    window.set_title(title);
                    window.set_app_id(self.logic.app_id().to_string());
                    // min size only after the first configure (Hyprland opens a floating window at its min size)
                    window.commit();
                    let id = self.next_id;
                    self.next_id += 1;
                    self.wins.push(Win { id, window, w, h, configured: false, dirty: true, animate: false, frame_pending: false, canvas: Canvas::new(1, 1), video: None, min, fullscreen: false, inhibitor: None });
                    self.cx.mods = self.mods;
                    // its commands are applied after this batch (apply loops)
                    self.logic.opened(id, token, &mut self.cx);
                }
                Cmd::Close(id) => {
                    self.wins.retain(|w| w.id != id);
                    if self.kb_win == Some(id) {
                        self.kb_win = None;
                    }
                    if self.ptr_win == Some(id) {
                        self.ptr_win = None;
                    }
                }
                Cmd::Title(id, t) => {
                    if let Some(w) = self.win(id) {
                        w.window.set_title(t);
                    }
                }
                Cmd::Redraw(id) => {
                    if let Some(w) = self.win(id) {
                        w.dirty = true;
                    }
                }
                Cmd::Animate(id, on) => {
                    if let Some(w) = self.win(id) {
                        w.animate = on;
                        w.dirty = true;
                    }
                }
                Cmd::Move(id) => {
                    let (seat, serial) = (self.seat.clone(), self.press_serial);
                    if let (Some(w), Some(seat)) = (self.win(id), seat) {
                        w.window.move_(&seat, serial);
                    }
                }
                Cmd::Minimize(id) => {
                    if let Some(w) = self.win(id) {
                        w.window.set_minimized();
                    }
                }
                Cmd::Maximize(id) => {
                    if let Some(w) = self.win(id) {
                        w.window.set_maximized();
                    }
                }
                Cmd::Fullscreen(id, on) => {
                    if let Some(w) = self.win(id) {
                        if on {
                            w.window.set_fullscreen(None);
                        } else {
                            w.window.unset_fullscreen();
                        }
                    }
                }
                Cmd::HideCursor(h) => {
                    if h != self.cursor_hidden {
                        self.cursor_hidden = h;
                        if let Some(p) = &self.pointer {
                            let _ = if h { p.hide_cursor() } else { p.set_cursor(&self.conn, self.cursor) };
                        }
                    }
                }
                Cmd::IdleInhibit(id, on) => {
                    let (mgr, qh) = (self.inhibit_mgr.clone(), self.qh.clone());
                    if let Some(w) = self.win(id) {
                        match (on, w.inhibitor.take(), mgr) {
                            (true, None, Some(m)) => w.inhibitor = Some(m.create_inhibitor(w.window.wl_surface(), &qh, ())),
                            (true, Some(i), _) => w.inhibitor = Some(i),
                            (false, Some(i), _) => i.destroy(),
                            _ => {}
                        }
                    }
                }
                Cmd::Cursor(c) => {
                    if c != self.cursor {
                        self.cursor = c;
                        if let (Some(p), false) = (&self.pointer, self.cursor_hidden) {
                            let _ = p.set_cursor(&self.conn, c);
                        }
                    }
                }
                Cmd::WakeAt(t) => {
                    self.wake = Some(self.wake.map(|w| w.min(t)).unwrap_or(t));
                }
                Cmd::Drag(id, uris, icon) => self.start_drag(id, uris, icon),
                Cmd::Exit => self.exit = true,
            }
        }
    }

    fn start_drag(&mut self, id: WinId, uris: Vec<String>, icon: Option<(u32, u32, Vec<u8>)>) {
        let Some(dev) = &self.device else { return };
        let Some(origin) = self.wins.iter().find(|w| w.id == id).map(|w| w.window.wl_surface().clone()) else { return };
        let src = self.ddm.create_drag_and_drop_source(&self.qh, vec!["text/uri-list", "text/plain;charset=utf-8"], DndAction::Copy | DndAction::Move);
        let icon_surf = icon.and_then(|(w, h, bgra)| {
            let s = self.compositor.create_surface(&self.qh);
            let (buf, data) = self.pool.create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888).ok()?;
            let n = bgra.len().min(data.len());
            data[..n].copy_from_slice(&bgra[..n]);
            buf.attach_to(&s).ok()?;
            s.damage_buffer(0, 0, w as i32, h as i32);
            s.commit();
            Some(s)
        });
        src.start_drag(dev, &origin, icon_surf.as_ref(), self.press_serial);
        self.drag_source = Some(src);
        self.drag_uris = uris;
        self.drag_icon = icon_surf;
        self.drag_win = Some(id);
    }

    fn flush_draws(&mut self) {
        let ids: Vec<WinId> = self.wins.iter().filter(|w| w.configured && (w.dirty || w.animate) && !w.frame_pending).map(|w| w.id).collect();
        for id in ids {
            self.draw(id);
        }
    }

    fn draw(&mut self, id: WinId) {
        let Some(i) = self.wins.iter().position(|w| w.id == id) else { return };
        let (w, h) = (self.wins[i].w, self.wins[i].h);
        if self.wins[i].canvas.pm.width() != w || self.wins[i].canvas.pm.height() != h {
            self.wins[i].canvas = Canvas::new(w, h);
        } else {
            self.wins[i].canvas.clear();
        }
        let mut canvas = std::mem::replace(&mut self.wins[i].canvas, Canvas::new(1, 1));
        self.cx.mods = self.mods;
        self.logic.paint(id, &mut canvas, &mut self.cx);
        let video = self.logic.video_rect(id);
        let Some(i) = self.wins.iter().position(|w| w.id == id) else { return };
        let win = &mut self.wins[i];
        win.dirty = false;
        if let Ok((buf, data)) = self.pool.create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888) {
            canvas.to_bgra(data);
            let surf = win.window.wl_surface();
            surf.damage_buffer(0, 0, w as i32, h as i32);
            let _ = buf.attach_to(surf);
        }
        win.canvas = canvas;
        // the video plane (below the window surface, drawn by the app through EGL)
        let mut made = None;
        let mut gone = false;
        match video {
            Some(r) => {
                if win.video.is_none() {
                    let (sub, s) = self.subcompositor.create_subsurface(win.window.wl_surface().clone(), &self.qh);
                    sub.place_below(win.window.wl_surface());
                    sub.set_desync();
                    sub.set_position(r.0, r.1);
                    made = Some(VideoSurface { surface: wayland_client::Proxy::id(&s).as_ptr() as *mut _, display: self.conn.backend().display_ptr() as *mut _ });
                    win.video = Some((s, sub, r));
                }
                let (_, sub, rect) = win.video.as_mut().unwrap();
                if *rect != r {
                    *rect = r;
                    sub.set_position(r.0, r.1);
                }
            }
            None => gone = win.video.is_some(),
        }
        let surf = win.window.wl_surface();
        surf.frame(&self.qh, surf.clone());
        win.frame_pending = true;
        surf.commit();
        if let Some(v) = made {
            self.with_logic(|l, cx| l.video_surface(id, Some(v), cx));
        }
        if gone {
            self.with_logic(|l, cx| l.video_surface(id, None, cx));
            if let Some(w) = self.wins.iter_mut().find(|w| w.id == id) {
                if let Some((s, sub, _)) = w.video.take() {
                    sub.destroy();
                    s.destroy();
                }
            }
        }
    }

    fn emit(&mut self, id: WinId, e: Ev) {
        self.with_logic(|l, cx| l.event(id, e, cx));
    }
}

impl CompositorHandler for Rt {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, s: &wl_surface::WlSurface, _: u32) {
        if let Some(w) = self.wins.iter_mut().find(|w| w.window.wl_surface() == s) {
            w.frame_pending = false;
            // first frame shown: now the min size (set earlier, Hyprland opens the window at it)
            if w.min.0 > 0 {
                w.window.set_min_size(Some(w.min));
                w.min = (0, 0);
            }
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}
impl OutputHandler for Rt {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl ShmHandler for Rt {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl WindowHandler for Rt {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(id) = self.id_of(window.wl_surface()) {
            self.with_logic(|l, cx| l.close_requested(id, cx));
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, c: WindowConfigure, _: u32) {
        let Some(id) = self.id_of(window.wl_surface()) else { return };
        let w = self.win(id).unwrap();
        let (nw, nh) = (c.new_size.0.map(|v| v.get()).unwrap_or(w.w), c.new_size.1.map(|v| v.get()).unwrap_or(w.h));
        let changed = nw != w.w || nh != w.h;
        let fs = c.state.contains(smithay_client_toolkit::reexports::csd_frame::WindowState::FULLSCREEN);
        let fs_changed = fs != w.fullscreen;
        w.fullscreen = fs;
        w.w = nw.max(1);
        w.h = nh.max(1);
        w.configured = true;
        w.dirty = true;
        w.frame_pending = false;
        if changed {
            self.emit(id, Ev::Resized);
        }
        if fs_changed {
            self.emit(id, Ev::Fullscreen(fs));
        }
        self.draw(id);
    }
}
impl SeatHandler for Rt {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.device.is_none() {
            self.device = Some(self.ddm.get_data_device(qh, &seat));
        }
        self.seat = Some(seat);
    }
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if self.device.is_none() {
            self.device = Some(self.ddm.get_data_device(qh, &seat));
        }
        self.seat = Some(seat.clone());
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            let surf = self.compositor.create_surface(qh);
            self.pointer = self.seats.get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surf, ThemeSpec::default()).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl KeyboardHandler for Rt {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, s: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        if let Some(id) = self.id_of(s) {
            self.kb_win = Some(id);
            self.emit(id, Ev::Focus(true));
        }
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, s: &wl_surface::WlSurface, _: u32) {
        if let Some(id) = self.id_of(s) {
            if self.kb_win == Some(id) {
                self.kb_win = None;
            }
            self.emit(id, Ev::Focus(false));
        }
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        if let Some(id) = self.kb_win {
            self.emit(id, Ev::Key(ev.keysym, ev.utf8));
        }
    }
    fn repeat_key(&mut self, c: &Connection, q: &QueueHandle<Self>, k: &wl_keyboard::WlKeyboard, s: u32, ev: KeyEvent) {
        self.press_key(c, q, k, s, ev);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: Modifiers, _: RawModifiers, _: u32) {
        self.mods = Mods { ctrl: m.ctrl, shift: m.shift, alt: m.alt };
    }
}
impl PointerHandler for Rt {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            let Some(id) = self.id_of(&e.surface) else { continue };
            let (x, y) = (e.position.0 as f32, e.position.1 as f32);
            match e.kind {
                PointerEventKind::Enter { serial } => {
                    self.ptr_win = Some(id);
                    self.enter_serial = serial;
                    // the shape is per enter: set it again
                    if let Some(p) = &self.pointer {
                        let _ = if self.cursor_hidden { p.hide_cursor() } else { p.set_cursor(&self.conn, self.cursor) };
                    }
                    self.emit(id, Ev::Motion(x, y));
                }
                PointerEventKind::Leave { .. } => {
                    if self.ptr_win == Some(id) {
                        self.ptr_win = None;
                    }
                    self.emit(id, Ev::Leave);
                }
                PointerEventKind::Motion { .. } => self.emit(id, Ev::Motion(x, y)),
                PointerEventKind::Press { button, serial, .. } => {
                    self.press_serial = serial;
                    let (b, t, px, py, n) = self.last_press;
                    let clicks = if b == button && t.elapsed() < Duration::from_millis(400) && (px - x).abs() < 5.0 && (py - y).abs() < 5.0 && n == 1 { 2 } else { 1 };
                    self.last_press = (button, Instant::now(), x, y, clicks);
                    self.emit(id, Ev::Press(x, y, button, clicks as u8));
                }
                PointerEventKind::Release { button, .. } => self.emit(id, Ev::Release(x, y, button)),
                PointerEventKind::Axis { vertical, .. } => {
                    let dy = if vertical.absolute != 0.0 { vertical.absolute as f32 } else { vertical.discrete as f32 * 15.0 };
                    if dy != 0.0 {
                        self.emit(id, Ev::Scroll(x, y, dy));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- drag and drop
impl DataDeviceHandler for Rt {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, dev: &wl_data_device::WlDataDevice, x: f64, y: f64, s: &wl_surface::WlSurface) {
        let Some(id) = self.id_of(s) else { return };
        if let Some(offer) = self.device.as_ref().and_then(|d| (d.inner() == dev).then(|| d.data().drag_offer()).flatten()) {
            let ok = offer.with_mime_types(|m| m.iter().any(|t| t == "text/uri-list"));
            offer.accept_mime_type(offer.serial, ok.then(|| "text/uri-list".to_string()));
            if ok {
                offer.set_actions(DndAction::Copy | DndAction::Move, DndAction::Move);
            }
        }
        self.emit(id, Ev::DragMotion(x as f32, y as f32));
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {
        for id in self.wins.iter().map(|w| w.id).collect::<Vec<_>>() {
            self.emit(id, Ev::DragLeave);
        }
    }
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, dev: &wl_data_device::WlDataDevice, x: f64, y: f64) {
        let Some(offer) = self.device.as_ref().and_then(|d| (d.inner() == dev).then(|| d.data().drag_offer()).flatten()) else { return };
        if let Some(id) = self.id_of(&offer.surface) {
            self.emit(id, Ev::DragMotion(x as f32, y as f32));
        }
    }
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, dev: &wl_data_device::WlDataDevice) {
        let Some(offer) = self.device.as_ref().and_then(|d| (d.inner() == dev).then(|| d.data().drag_offer()).flatten()) else { return };
        let Some(id) = self.id_of(&offer.surface) else { return };
        let (x, y) = (offer.x as f32, offer.y as f32);
        // our own drag: no pipe round trip through the compositor
        if self.drag_source.is_some() && !self.drag_uris.is_empty() {
            let uris = self.drag_uris.clone();
            offer.finish();
            offer.destroy();
            self.emit(id, Ev::Drop(x, y, uris));
            return;
        }
        let Ok(pipe) = offer.receive("text/uri-list".into()) else { return };
        let mut buf = Vec::new();
        let off = offer.clone();
        let _ = self.lh.insert_source(pipe, move |_, f, rt: &mut Rt| {
            let mut chunk = [0u8; 8192];
            loop {
                match unsafe { f.get_mut() }.read(&mut chunk) {
                    Ok(0) => {
                        let s = String::from_utf8_lossy(&buf).to_string();
                        let uris: Vec<String> = s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
                        off.finish();
                        off.destroy();
                        rt.emit(id, Ev::Drop(x, y, uris));
                        return PostAction::Remove;
                    }
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return PostAction::Continue,
                    Err(_) => return PostAction::Remove,
                }
            }
        });
    }
}
impl DataOfferHandler for Rt {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}
impl DataSourceHandler for Rt {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: Option<String>) {}
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, mime: String, mut pipe: WritePipe) {
        let body = if mime == "text/uri-list" { self.drag_uris.iter().map(|u| format!("{u}\r\n")).collect::<String>() } else { self.drag_uris.iter().map(|u| uri_to_path(u)).collect::<Vec<_>>().join("\n") };
        // small payload: write it straight (the pipe buffer holds 64 KiB)
        let _ = pipe.write_all(body.as_bytes());
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {
        self.end_drag();
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {
        self.end_drag();
    }
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: DndAction) {}
}
impl Rt {
    fn end_drag(&mut self) {
        self.drag_source = None;
        self.drag_uris.clear();
        if let Some(s) = self.drag_icon.take() {
            s.destroy();
        }
        if let Some(id) = self.drag_win.take() {
            self.emit(id, Ev::DragLeave);
        }
    }
}

impl ProvidesRegistryState for Rt {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
impl Dispatch<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, ()> for Rt {
    fn event(_: &mut Self, _: &zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, _: zwp_idle_inhibit_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1, ()> for Rt {
    fn event(_: &mut Self, _: &zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1, _: zwp_idle_inhibitor_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wp_viewporter::WpViewporter, ()> for Rt {
    fn event(_: &mut Self, _: &wp_viewporter::WpViewporter, _: wp_viewporter::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wp_viewport::WpViewport, ()> for Rt {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
delegate_compositor!(Rt);
delegate_subcompositor!(Rt);
delegate_output!(Rt);
delegate_shm!(Rt);
delegate_seat!(Rt);
delegate_keyboard!(Rt);
delegate_pointer!(Rt);
delegate_xdg_shell!(Rt);
delegate_xdg_window!(Rt);
delegate_data_device!(Rt);
delegate_registry!(Rt);

// ---------------------------------------------------------------- helpers shared by the apps
pub fn uri_to_path(u: &str) -> String {
    let s = u.strip_prefix("file://").unwrap_or(u);
    let s = s.strip_prefix("localhost").unwrap_or(s);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
pub fn path_to_uri(p: &str) -> String {
    let mut s = String::from("file://");
    for &c in p.as_bytes() {
        if c.is_ascii_alphanumeric() || b"/-_.~!$&'()*+,;=:@".contains(&c) {
            s.push(c as char);
        } else {
            s.push_str(&format!("%{c:02X}"));
        }
    }
    s
}
// run detached (own session, no zombie: double fork through sh)
pub fn spawn(cmd: &[&str]) {
    use std::os::unix::process::CommandExt;
    if cmd.is_empty() {
        return;
    }
    let mut c = std::process::Command::new(cmd[0]);
    c.args(&cmd[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Ok(mut ch) = c.spawn() {
        // reap in a thread so it never stays a zombie
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}
pub fn hypr(lua: &str) {
    spawn(&["hyprctl", "eval", lua]);
}
