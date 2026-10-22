//! Hyprland protocols that Mist's own tools and the Hyprland portal use, which Smithay does not implement:
//! - hyprland-toplevel-export-v1: one window into a client's shm buffer (the dock's window picker thumbnails,
//!   xdg-desktop-portal-hyprland window sharing). Rendered offscreen only when asked, like screencopy.
//! - hyprland-global-shortcuts-v1: apps register shortcuts through the portal (Discord push-to-talk); a
//!   comp.conf bind `global APP_ID:ID` sends them pressed / released.
//!
//! Sandboxed clients (security-context) do not get these globals.
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{damage::OutputDamageTracker, gles::GlesTexture, Bind, ExportMem, Offscreen},
    },
    desktop::Window,
    reexports::wayland_server::{
        self,
        protocol::{wl_buffer::WlBuffer, wl_shm},
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    },
    utils::{Rectangle, Scale, Transform},
    wayland::shm::with_buffer_contents_mut,
};

use crate::MistComp;

macro_rules! protocol {
    ($name:ident, $xml:literal) => {
        #[allow(non_upper_case_globals, non_camel_case_types, dead_code, unused_imports, clippy::all)]
        pub mod $name {
            use smithay::reexports::wayland_server;
            use smithay::reexports::wayland_server::protocol::*;
            pub mod __interfaces {
                use smithay::reexports::wayland_server::protocol::__interfaces::*;
                wayland_scanner::generate_interfaces!($xml);
            }
            use self::__interfaces::*;
            wayland_scanner::generate_server_code!($xml);
        }
    };
}
protocol!(export, "protocols/hyprland-toplevel-export-v1.xml");
protocol!(shortcuts, "protocols/hyprland-global-shortcuts-v1.xml");
use shortcuts::hyprland_global_shortcut_v1::HyprlandGlobalShortcutV1;
use shortcuts::hyprland_global_shortcuts_manager_v1::{self as gsm, HyprlandGlobalShortcutsManagerV1};
use export::hyprland_toplevel_export_frame_v1::{self as frame, HyprlandToplevelExportFrameV1};
use export::hyprland_toplevel_export_manager_v1::{self as tem, HyprlandToplevelExportManagerV1};

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, HyprlandToplevelExportManagerV1, _>(2, ());
    dh.create_global::<MistComp, HyprlandGlobalShortcutsManagerV1, _>(1, ());
}

fn now() -> (u32, u32, u32) {
    let t = smithay::reexports::rustix::time::clock_gettime(smithay::reexports::rustix::time::ClockId::Monotonic);
    ((t.tv_sec as u64 >> 32) as u32, t.tv_sec as u32, t.tv_nsec as u32)
}

// ------------------------------------------------------------------ window capture

/// The window a frame captures (its hyprctl address), and the size announced for it.
pub struct FrameData {
    addr: u64,
    size: (i32, i32),
    used: std::sync::atomic::AtomicBool,
}

pub struct Pending {
    frame: HyprlandToplevelExportFrameV1,
    buffer: WlBuffer,
}

impl GlobalDispatch<HyprlandToplevelExportManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<HyprlandToplevelExportManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
    fn can_view(client: Client, _: &()) -> bool {
        crate::wlr::trusted(&client)
    }
}

impl MistComp {
    /// A window (on screen or on a hidden workspace) by the lower 32 bits of its address.
    fn window_by_handle(&self, handle: u32) -> Option<Window> {
        self.space
            .elements()
            .chain(self.stash.iter().map(|(w, _)| w))
            .find(|w| crate::wm::data(w).addr.get() & 0xffff_ffff == handle as u64)
            .cloned()
    }

    /// The scale a window is captured at: its output's (sharp thumbnails and shares on HiDPI).
    fn capture_scale(&self, w: &Window) -> f64 {
        self.output_of(w).or_else(|| self.active_output()).map(|o| o.current_scale().fractional_scale()).unwrap_or(1.0)
    }
}

impl Dispatch<HyprlandToplevelExportManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &HyprlandToplevelExportManagerV1, req: tem::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        let (frame, handle) = match req {
            tem::Request::CaptureToplevel { frame, handle, .. } => (frame, handle),
            _ => return,
        };
        let Some(w) = state.window_by_handle(handle).filter(|_| !state.locked) else {
            init.init(frame, FrameData { addr: 0, size: (0, 0), used: Default::default() }).failed();
            return;
        };
        let s: smithay::utils::Size<i32, smithay::utils::Physical> = w.geometry().size.to_f64().to_physical_precise_round(state.capture_scale(&w));
        let (cw, ch) = (s.w.max(1), s.h.max(1));
        let f = init.init(frame, FrameData { addr: crate::wm::data(&w).addr.get(), size: (cw, ch), used: Default::default() });
        f.buffer(wl_shm::Format::Xrgb8888 as u32, cw as u32, ch as u32, cw as u32 * 4);
        if f.version() >= 2 {
            f.buffer_done();
        }
    }
}

impl Dispatch<HyprlandToplevelExportFrameV1, FrameData> for MistComp {
    fn request(state: &mut Self, _: &Client, f: &HyprlandToplevelExportFrameV1, req: frame::Request, d: &FrameData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let frame::Request::Copy { buffer, .. } = req {
            if d.used.swap(true, std::sync::atomic::Ordering::Relaxed) {
                f.post_error(frame::Error::AlreadyUsed, "frame already used");
                return;
            }
            state.window_copies.push(Pending { frame: f.clone(), buffer });
            state.dirty = true;
        }
    }
}

/// Serve queued window copies (main loop, after rendering).
pub fn process(state: &mut MistComp) {
    if state.window_copies.is_empty() {
        return;
    }
    for p in std::mem::take(&mut state.window_copies) {
        if !p.frame.is_alive() {
            continue;
        }
        let Some(d) = p.frame.data::<FrameData>() else { continue };
        let w = state.window_by_handle(d.addr as u32).filter(|w| crate::wm::data(w).addr.get() == d.addr);
        let Some(w) = w.filter(|_| !state.locked) else {
            p.frame.failed();
            continue;
        };
        let scale = state.capture_scale(&w);
        let size = d.size;
        let px = {
            let MistComp { gles, udev, headless, .. } = state;
            if let Some(h) = headless.as_mut() {
                grab(&mut h.renderer, &w, scale, size)
            } else if let Some(u) = udev.as_mut() {
                grab(&mut u.renderer, &w, scale, size)
            } else if let Some(g) = gles.as_ref() {
                grab(g.borrow_mut().renderer(), &w, scale, size)
            } else {
                None
            }
        };
        let ok = px.is_some_and(|(px, swap)| write_shm(&p.buffer, &px, size, swap));
        if !ok {
            p.frame.failed();
            continue;
        }
        p.frame.flags(0);
        p.frame.damage(0, 0, size.0 as u32, size.1 as u32);
        let (hi, lo, ns) = now();
        p.frame.ready(hi, lo, ns);
    }
}

/// The window alone (surfaces and popups, rounded corners; not its server-side bar) rendered at `scale` and
/// read back: BGRA when the driver reads it so (`false`), else RGBA (`true`: bytes to swap).
fn grab(r: &mut smithay::backend::renderer::gles::GlesRenderer, w: &Window, scale: f64, (cw, ch): (i32, i32)) -> Option<(Vec<u8>, bool)> {
    let mut tex: GlesTexture = r.create_buffer(Fourcc::Abgr8888, (cw, ch).into()).ok()?;
    let elements = crate::scene::window_elements(r, w, (0, 0).into(), Scale::from(scale), 1.0, None);
    let mut tracker = OutputDamageTracker::new((cw, ch), 1.0, Transform::Normal);
    let mut fb = r.bind(&mut tex).ok()?;
    tracker.render_output(r, &mut fb, 0, &elements, [0.0, 0.0, 0.0, 1.0]).ok()?;
    let rect = Rectangle::from_size((cw, ch).into());
    for (format, swap) in [(Fourcc::Argb8888, false), (Fourcc::Abgr8888, true)] {
        if let Ok(map) = r.copy_framebuffer(&fb, rect, format) {
            if let Ok(px) = r.map_texture(&map) {
                return Some((px.to_vec(), swap));
            }
        }
    }
    None
}

/// Tightly packed rows into an XRGB8888 shm buffer of exactly that size.
fn write_shm(buffer: &WlBuffer, px: &[u8], (w, h): (i32, i32), swap: bool) -> bool {
    with_buffer_contents_mut(buffer, |ptr, len, d| {
        if d.width != w || d.height != h || d.stride < d.width * 4 || px.len() < (w * h * 4) as usize {
            return false;
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
        let w4 = w as usize * 4;
        for y in 0..h as usize {
            let row = &px[y * w4..(y + 1) * w4];
            let off = d.offset as usize + y * d.stride as usize;
            let Some(out) = dst.get_mut(off..off + w4) else { return false };
            if swap {
                for (o, i) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                    o.copy_from_slice(&[i[2], i[1], i[0], 255]);
                }
            } else {
                out.copy_from_slice(row);
            }
        }
        true
    })
    .unwrap_or(false)
}

// ------------------------------------------------------------------ global shortcuts

/// A registered shortcut: who asked, what it is called, what the app says it does.
pub struct Shortcut {
    pub app_id: String,
    pub id: String,
    pub description: String,
    pub res: HyprlandGlobalShortcutV1,
}

impl GlobalDispatch<HyprlandGlobalShortcutsManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<HyprlandGlobalShortcutsManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
    fn can_view(client: Client, _: &()) -> bool {
        crate::wlr::trusted(&client)
    }
}

impl Dispatch<HyprlandGlobalShortcutsManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, mgr: &HyprlandGlobalShortcutsManagerV1, req: gsm::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        if let gsm::Request::RegisterShortcut { shortcut, id, app_id, description, .. } = req {
            state.shortcuts.retain(|s| s.res.is_alive());
            if state.shortcuts.iter().any(|s| s.app_id == app_id && s.id == id) {
                mgr.post_error(gsm::Error::AlreadyTaken, format!("{app_id}:{id} is already registered"));
                return;
            }
            let res = init.init(shortcut, ());
            tracing::info!("global shortcut {app_id}:{id} ({description})");
            state.shortcuts.push(Shortcut { app_id, id, description, res });
        }
    }
}

impl Dispatch<HyprlandGlobalShortcutV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, res: &HyprlandGlobalShortcutV1, _: shortcuts::hyprland_global_shortcut_v1::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        // destroy: forget it
        state.shortcuts.retain(|s| s.res != *res && s.res.is_alive());
    }
    fn destroyed(state: &mut Self, _: wayland_server::backend::ClientId, res: &HyprlandGlobalShortcutV1, _: &()) {
        state.shortcuts.retain(|s| s.res != *res);
    }
}

impl MistComp {
    /// A `global APP_ID:ID` bind went down / up (Hyprland: `global` dispatcher, `:ID` matches any app).
    pub fn global_shortcut(&mut self, name: &str, pressed: bool) {
        let (app, id) = name.split_once(':').unwrap_or(("", name));
        let (hi, lo, ns) = now();
        for s in self.shortcuts.iter().filter(|s| s.id == id && (app.is_empty() || s.app_id == app) && s.res.is_alive()) {
            if pressed {
                s.res.pressed(hi, lo, ns);
            } else {
                s.res.released(hi, lo, ns);
            }
        }
    }
}
