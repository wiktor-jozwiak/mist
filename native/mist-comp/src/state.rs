use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{layer_map_for_output, LayerSurface, PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Logical, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{wlr_layer::{Layer as WlrLayer, WlrLayerShellState}, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::CalloopData;

pub struct MistComp {
    pub start_time: std::time::Instant,
    pub loop_handle: smithay::reexports::calloop::LoopHandle<'static, crate::CalloopData>,
    /// X11 window manager once XWayland is ready
    pub xwm: Option<smithay::xwayland::X11Wm>,
    pub xwayland_shell: smithay::wayland::xwayland_shell::XWaylandShellState,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub protocols: crate::handlers::protocols::Protocols,
    pub extra: crate::handlers::extra::Extra,
    pub wlr: crate::wlr::Wlr,
    pub outman: crate::outman::OutMan,
    /// monitors on (DPMS), as output-power clients are told
    pub dpms_on: bool,
    /// windows or outputs changed: surfaces get their output's scale again (update_scales)
    pub scales_dirty: bool,
    /// linux-drm-syncobj (explicit sync): only on a DRM device that supports syncobj eventfd
    pub syncobj: Option<smithay::wayland::drm_syncobj::DrmSyncobjState>,
    /// explicit-sync commits waiting for their GPU fence, checked once a second while there are some
    pub fence_waits: crate::handlers::compositor::FenceWaits,
    pub fence_timer: bool,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<MistComp>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,
    /// something changed on screen: request a frame after this loop iteration
    pub dirty: bool,
    /// a client committed: only these outputs show it (per-output frames; `dirty` = all of them)
    pub dirty_outputs: std::collections::HashSet<String>,
    /// cover / ext-workspace / toplevel lists / scales bookkeeping: last run, due, its timer armed
    pub sync_at: Option<std::time::Instant>,
    pub sync_due: bool,
    pub sync_timer: bool,
    /// linux-dmabuf global + the renderer that imports client buffers (set by the backend)
    pub dmabuf: Option<(smithay::wayland::dmabuf::DmabufState, smithay::wayland::dmabuf::DmabufGlobal)>,
    pub gles: Option<crate::winit::Gles>,
    pub vt_switch: Option<Box<dyn FnMut(i32)>>,
    /// the DRM session backend (None when nested)
    pub udev: Option<crate::udev::Udev>,
    /// offscreen test backend
    pub headless: Option<crate::headless::Headless>,
    pub cursor_status: smithay::input::pointer::CursorImageStatus,
    /// drag and drop: the icon surface the client gave, and its offset from the pointer (wl_surface.offset)
    pub dnd_icon: Option<(WlSurface, Point<i32, Logical>)>,
    pub config: crate::config::Config,
    /// output name -> shown workspace id
    pub ws_active: std::collections::HashMap<String, u32>,
    /// windows of hidden workspaces, position relative to their output
    pub stash: Vec<(Window, Point<i32, Logical>)>,
    /// xdg toplevels before their first buffer (rules.rs)
    pub pending: Vec<Window>,
    /// last size per app id (windowrules.lua persistent_size), this session only
    pub sizes: std::collections::HashMap<String, smithay::utils::Size<i32, smithay::utils::Logical>>,
    /// a workspace was switched / a window moved between workspaces (IPC event pending)
    pub ws_changed: bool,
    /// the output showing the special workspace
    pub special_on: Option<String>,
    /// Super went down and nothing else since (release binds)
    pub super_alone: bool,
    /// a workspace swipe in progress
    pub swipe: Option<Swipe>,
    /// a button whose press the overview took: its release is the overview's too
    pub swallow_button: Option<u32>,
    /// the xkb layout name in use and the output under the pointer, as last announced (activelayout,
    /// focusedmon events)
    pub ev_layout: String,
    pub ev_mon: String,
    /// keys whose press a bind ate: eat their release too
    pub suppressed: std::collections::HashSet<u32>,
    /// keys holding a `global` bind down (keycode -> APP_ID:ID), released with the key
    pub globals_held: std::collections::HashMap<u32, String>,
    /// socket2 listeners (hyprcompat)
    pub hypr_listeners: Vec<std::os::unix::net::UnixStream>,
    /// cover.rs: the geometry the last coverage was computed from, and what was announced per (output, layer)
    /// watchdog.rs: commits per client since its last check
    pub commit_rate: std::collections::HashMap<smithay::reexports::wayland_server::backend::ClientId, u32>,
    /// session.rs: a write is on its way; windows to put back after a crash
    pub session_timer: bool,
    pub restore: Vec<crate::session::Slot>,
    pub cover_sig: u64,
    /// ext-workspace-v1 clients and the workspace list they were last told (extws.rs)
    pub ext_ws: Vec<crate::extws::Bound>,
    pub ext_ws_last: crate::extws::Snap,
    pub cover_sent: std::collections::HashMap<(String, String), (bool, bool)>,
    /// window addresses, most recently focused first
    pub focus_history: Vec<u64>,
    pub next_addr: u64,
    pub lock_manager: smithay::wayland::session_lock::SessionLockManagerState,
    pub lock: crate::lock::Lock,
    pub idle: smithay::wayland::idle_notify::IdleNotifierState<MistComp>,
    pub idle_inhibit: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    /// surfaces holding an idle inhibitor (video players)
    pub inhibitors: Vec<WlSurface>,
    /// wlr-screencopy copies waiting for the next loop iteration
    pub screencopy: Vec<crate::screencopy::Pending>,
    pub window_copies: Vec<crate::hyprproto::Pending>,
    pub shortcuts: Vec<crate::hyprproto::Shortcut>,
    /// frames with damage shown per output (copy_with_damage waits for the next one)
    pub out_seq: std::collections::HashMap<String, u64>,
    /// render passes per output, damage or not (IPC `passes`: a commit on one monitor leaves the others alone)
    pub out_passes: std::collections::HashMap<String, u64>,
    /// screen casting: the last damage-driven copy served (mist-ui's island: "sharing your screen"), and
    /// the output frame each client was last given
    pub cast_last: Option<std::time::Instant>,
    pub cast_seen: std::collections::HashMap<smithay::reexports::wayland_server::backend::ClientId, u64>,
    /// night light state (gamma ramp on DRM; nested/headless only remember it)
    pub night: bool,
    /// title bar text (loaded with the first server-side bar)
    pub keys_seen: bool,
    pub wakeups: u64, // event-loop iterations (idle cost; `stats`)
    pub redraw_pending: bool, // nested: request_redraw sent, Redraw not yet handled
    pub text: Option<Option<crate::text::Text>>, // looked up on first use, inner None = no font
    /// window whose bar buttons are under the pointer (shows their symbols)
    pub bar_hovered: Option<u64>,
    /// window whose green button is hovered (mistsnap>>maxhover sent)
    pub max_hovered: Option<u64>,
    /// IPC `stats`: frames rendered (all outputs) and surface commits since start
    pub frames: u64,
    pub commits: u64,
    /// IPC `top`: commits per client while counting (None = off, the usual case)
    pub top: Option<std::collections::HashMap<String, u32>>,
    /// traffic light pressed (acts on release over the same one)
    pub bar_press: Option<(u64, crate::decor::Hit)>,
    /// last bar click (double click maximizes)
    pub bar_click: Option<(u64, std::time::Instant)>,
    /// the pointer shows a resize cursor over a window frame
    pub edge_cursor: bool,
    pub anim: crate::anim::Anims,
    /// title bar pressed: the move starts once the pointer travels (a click keeps the snap state)
    pub bar_drag: Option<(smithay::desktop::Window, Point<f64, Logical>, u32)>,
    /// session locked: only `bindl` binds work
    pub locked: bool,
    /// something changed while an output waited for its vblank
    pub redraw_after_vblank: bool,
    /// redraw the whole output next frame (screenshots)
    pub force_full: bool,
    /// pending `shot` IPC request: written after the next frame
    pub shot: Option<(std::path::PathBuf, std::os::unix::net::UnixStream)>,
    /// headless: the output `shot` captures (None: the first)
    pub shot_output: Option<String>,
}

impl MistComp {
    pub fn new(event_loop: &mut EventLoop<'static, CalloopData>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let config = crate::config::load();
        let protocols = crate::handlers::protocols::Protocols::new(&dh, config.middle_click_paste);
        let extra = crate::handlers::extra::Extra::new(&dh, config.middle_click_paste.then_some(&protocols.primary_selection), config.fifo);
        let lock_manager = smithay::wayland::session_lock::SessionLockManagerState::new::<Self, _>(&dh, |_| true);
        let idle = smithay::wayland::idle_notify::IdleNotifierState::new(&dh, event_loop.handle());
        let idle_inhibit = smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(&dh);
        crate::screencopy::init(&dh);
        crate::hyprproto::init(&dh);
        crate::extws::init(&dh);
        crate::wlr::init(&dh);
        crate::outman::init(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        let home = std::env::var("HOME").unwrap_or_default();
        for (k, v) in &config.env {
            // `~/` expands like in a shell
            std::env::set_var(k, v.replace("~/", &format!("{home}/")));
        }
        let (layout, variant, options) = &config.keyboard;
        let xkb = smithay::input::keyboard::XkbConfig {
            layout,
            variant,
            options: (!options.is_empty()).then(|| options.clone()),
            ..Default::default()
        };
        let kb = seat.add_keyboard(xkb, 300, 30).unwrap();
        if config.numlock {
            kb.set_modifier_state(smithay::input::keyboard::ModifiersState { num_lock: true, ..Default::default() });
        }

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        Self {
            start_time,
            loop_handle: event_loop.handle(),
            xwm: None,
            xwayland_shell: smithay::wayland::xwayland_shell::XWaylandShellState::new::<Self>(&dh),
            display_handle: dh,

            space,
            loop_signal,
            socket_name,

            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            protocols,
            extra,
            wlr: Default::default(),
            outman: Default::default(),
            dpms_on: true,
            scales_dirty: true,
            syncobj: None,
            fence_waits: Vec::new(),
            fence_timer: false,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
            dirty: false,
            dirty_outputs: Default::default(),
            sync_at: None,
            sync_due: false,
            sync_timer: false,
            dmabuf: None,
            gles: None,
            vt_switch: None,
            udev: None,
            headless: None,
            cursor_status: smithay::input::pointer::CursorImageStatus::default_named(),
            dnd_icon: None,
            redraw_after_vblank: false,
            config,
            ws_active: Default::default(),
            stash: Vec::new(),
            pending: Vec::new(),
            sizes: Default::default(),
            special_on: None,
            ws_changed: false,
            super_alone: false,
            swipe: None,
            swallow_button: None,
            ev_layout: String::new(),
            ev_mon: String::new(),
            suppressed: Default::default(),
            globals_held: Default::default(),
            locked: false,
            screencopy: Vec::new(),
            window_copies: Vec::new(),
            shortcuts: Vec::new(),
            out_seq: Default::default(),
            out_passes: Default::default(),
            cast_last: None,
            cast_seen: Default::default(),
            night: false,
            keys_seen: false,
            wakeups: 0,
            redraw_pending: false,
            text: None,
            bar_hovered: None,
            max_hovered: None,
            frames: 0,
            commits: 0,
            top: None,
            bar_press: None,
            bar_click: None,
            edge_cursor: false,
            anim: Default::default(),
            bar_drag: None,
            hypr_listeners: Vec::new(),
            commit_rate: Default::default(),
            session_timer: false,
            restore: Vec::new(),
            cover_sig: 0,
            ext_ws: Vec::new(),
            ext_ws_last: Vec::new(),
            cover_sent: Default::default(),
            lock_manager,
            lock: Default::default(),
            idle,
            idle_inhibit,
            inhibitors: Vec::new(),
            focus_history: Vec::new(),
            next_addr: 0x5a0000,
            force_full: false,
            shot: None,
            shot_output: None,
        }
    }

    fn init_wayland_listener(
        display: Display<MistComp>,
        event_loop: &mut EventLoop<CalloopData>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    /// Ctrl+Alt+Fn: only a real session (DRM backend) can switch VTs.
    pub fn change_vt(&mut self, vt: i32) {
        if let Some(f) = self.vt_switch.as_mut() {
            f(vt);
        }
    }

    /// Raise, activate and give the keyboard to a window (None: nothing focused).
    pub fn focus_window(&mut self, window: Option<&Window>) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        if let Some(w) = window {
            self.space.raise_element(w, true);
        }
        for w in self.space.elements() {
            w.set_activated(Some(w) == window);
            if let Some(t) = w.toplevel() {
                if t.is_initial_configure_sent() {
                    t.send_pending_configure();
                }
            }
        }
        let surface = window.and_then(crate::focus::of_window);
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, surface, serial);
        self.dirty = true;
        match window {
            Some(w) => {
                let a = crate::wm::data(w).addr.get();
                self.focus_history.retain(|x| *x != a);
                self.focus_history.insert(0, a);
                let (class, title) = (crate::hyprcompat::app_id(w), crate::hyprcompat::title(w));
                self.hypr_event("activewindow", &format!("{class},{title}"));
                self.hypr_event("activewindowv2", &format!("{a:x}"));
            }
            None => {
                self.hypr_event("activewindow", ",");
                self.hypr_event("activewindowv2", "");
            }
        }
    }

    /// The top of the usable area (below the bar's exclusive zone) of the output under a point, else of the
    /// nearest one: windows are kept below it (title bar included) when they are moved or resized.
    pub fn usable_top(&self, pos: Point<f64, Logical>) -> Option<i32> {
        let o = self.space.outputs().min_by(|a, b| {
            let d = |o: &smithay::output::Output| {
                self.space.output_geometry(o).map(|g| {
                    let g = g.to_f64();
                    let dx = (g.loc.x - pos.x).max(pos.x - (g.loc.x + g.size.w)).max(0.0);
                    let dy = (g.loc.y - pos.y).max(pos.y - (g.loc.y + g.size.h)).max(0.0);
                    dx * dx + dy * dy
                }).unwrap_or(f64::MAX)
            };
            d(a).total_cmp(&d(b))
        })?;
        let og = self.space.output_geometry(o)?;
        Some(og.loc.y + layer_map_for_output(o).non_exclusive_zone().loc.y)
    }

    /// The drag icon and where it shows (None: no drag, or its surface is gone).
    pub fn dnd_at(&self) -> Option<crate::scene::Dnd> {
        let (s, off) = self.dnd_icon.as_ref().filter(|(s, _)| smithay::utils::IsAlive::alive(s))?;
        let ptr = self.seat.get_pointer()?.current_location().to_i32_round();
        Some((s.clone(), ptr + *off))
    }

    /// The Overlay/Top layer surface under a point (these sit above windows).
    pub fn layer_at(&self, pos: Point<f64, Logical>) -> Option<LayerSurface> {
        let output = self.space.outputs().find(|o| {
            self.space.output_geometry(o).is_some_and(|g| g.to_f64().contains(pos))
        })?;
        let og = self.space.output_geometry(output)?;
        let map = layer_map_for_output(output);
        let rel = pos - og.loc.to_f64();
        // only where the layer takes input (the island's surface is larger than its pill)
        let hit = |lvl: WlrLayer| {
            map.layer_under(lvl, rel).filter(|l| {
                map.layer_geometry(l).is_some_and(|g| l.surface_under(rel - g.loc.to_f64(), WindowSurfaceType::ALL).is_some())
            })
        };
        hit(WlrLayer::Overlay).or_else(|| hit(WlrLayer::Top)).cloned()
    }

    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        if self.locked {
            return self.lock_surface_under(pos);
        }
        let output = self.space.outputs().find(|o| {
            self.space.output_geometry(o).is_some_and(|g| g.to_f64().contains(pos))
        })?;
        let og = self.space.output_geometry(output)?;
        // before the layer map lock: bar_at / edge_at take it themselves (a Mutex, not re-entrant)
        let frame = self.bar_at(pos).is_some() || self.edge_at(pos).is_some();
        let map = layer_map_for_output(output);
        let layer_hit = |layer: WlrLayer| {
            map.layer_under(layer, pos - og.loc.to_f64()).and_then(|l| {
                let lg = map.layer_geometry(l)?;
                let loc = og.loc + lg.loc;
                l.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, (p + loc).to_f64()))
            })
        };
        if let Some(hit) = layer_hit(WlrLayer::Overlay).or_else(|| layer_hit(WlrLayer::Top)) {
            return Some(hit);
        }
        // a server-side title bar or a window frame: no client surface gets the pointer there
        if frame {
            return None;
        }
        None.or_else(|| {
                self.space.element_under(pos).and_then(|(window, location)| {
                    window
                        .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                        .map(|(s, p)| (s, (p + location).to_f64()))
                })
            })
            .or_else(|| layer_hit(WlrLayer::Bottom))
            .or_else(|| layer_hit(WlrLayer::Background))
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// connected through a sandbox's security-context listener (flatpak): no privileged globals
    pub security_context: Option<smithay::wayland::security_context::SecurityContext>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, reason: DisconnectReason) {
        // a protocol error closes the socket silently on the client side: log which request killed it
        if let DisconnectReason::ProtocolError(e) = reason {
            tracing::warn!("client killed by protocol error: {e}");
        }
    }
}

/// A touchpad workspace swipe: the next workspace follows the fingers (1:1), lifting them finishes or takes
/// it back.
#[derive(Default)]
pub struct Swipe {
    /// horizontal distance so far and of the last update
    pub dx: f64,
    pub last: f64,
    pub dy: f64,
    /// up / down: the overview, not workspaces
    pub vertical: bool,
    /// switched on this output, +1 / -1, from this workspace (None: not yet, or nothing to switch to)
    pub on: Option<(smithay::output::Output, i32, u32)>,
    pub tried: bool,
}
