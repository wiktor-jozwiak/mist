// Checks against the running compositor (mist-comp/tests/ci.sh, by hand on Hyprland):
//   --wincap ADDR OUT.ppm          one window through hyprland_toplevel_export_v1 (what the dock's picker does)
//   --global-shortcut APP ID       register a portal-style global shortcut, print `pressed` / `released`
//   --inhibit [APP]                keep the screen on (org.freedesktop.ScreenSaver.Inhibit) until stopped
//   --present [N]                  N frames with wp_presentation feedback; exit 1 when one never comes
use std::io::Write;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};

#[allow(non_upper_case_globals, non_camel_case_types, dead_code, unused_imports, clippy::all)]
mod proto {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-global-shortcuts-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/hyprland-global-shortcuts-v1.xml");
}
use proto::hyprland_global_shortcut_v1::{self as sc, HyprlandGlobalShortcutV1};
use proto::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1;

/// The window at full size as a binary PPM; exit 1 when the compositor refuses it.
pub fn wincap(addr: &str, out: &str) -> i32 {
    let (tx, rx) = calloop::channel::channel();
    crate::wincap::capture(vec![addr.to_string()], 16384, 16384, tx);
    let Ok(crate::sys::Msg::WinThumb(_, pm)) = rx.recv() else { return 1 };
    let mut ppm = format!("P6\n{} {}\n255\n", pm.width(), pm.height()).into_bytes();
    for p in pm.data().chunks_exact(4) {
        ppm.extend_from_slice(&p[..3]);
    }
    if std::fs::write(out, ppm).is_err() {
        return 1;
    }
    println!("{}x{}", pm.width(), pm.height());
    0
}

/// Hold a screensaver inhibit like a video player does, until killed (Ctrl+C).
pub fn inhibit(app: &str) -> i32 {
    let Ok(c) = zbus::blocking::Connection::session() else { return 1 };
    let r = c.call_method(Some("org.freedesktop.ScreenSaver"), "/org/freedesktop/ScreenSaver", Some("org.freedesktop.ScreenSaver"), "Inhibit", &(app, "mist-ui --inhibit"));
    let Ok(cookie) = r.and_then(|m| m.body().deserialize::<u32>().map_err(Into::into)) else {
        eprintln!("no org.freedesktop.ScreenSaver");
        return 1;
    };
    println!("inhibited ({cookie})");
    let _ = std::io::stdout().flush();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

struct St;

pub fn global_shortcut(app: &str, id: &str) -> i32 {
    let Ok(conn) = Connection::connect_to_env() else { return 1 };
    let Ok((globals, mut q)) = registry_queue_init::<St>(&conn) else { return 1 };
    let qh = q.handle();
    let Ok(mgr) = globals.bind::<HyprlandGlobalShortcutsManagerV1, _, _>(&qh, 1..=1, ()) else {
        eprintln!("no hyprland_global_shortcuts_manager_v1");
        return 1;
    };
    let _s = mgr.register_shortcut(id.into(), app.into(), "mist test".into(), "".into(), &qh, ());
    println!("registered");
    let _ = std::io::stdout().flush();
    loop {
        if q.blocking_dispatch(&mut St).is_err() {
            return 1;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for St {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandGlobalShortcutsManagerV1, ()> for St {
    fn event(_: &mut Self, _: &HyprlandGlobalShortcutsManagerV1, _: proto::hyprland_global_shortcuts_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandGlobalShortcutV1, ()> for St {
    fn event(_: &mut Self, _: &HyprlandGlobalShortcutV1, ev: sc::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            sc::Event::Pressed { .. } => println!("pressed"),
            sc::Event::Released { .. } => println!("released"),
        }
        let _ = std::io::stdout().flush();
    }
}

/// `--present N`: a window that draws N frames, each asking for wp_presentation feedback (what Chromium, Vulkan
/// FIFO and some EGL drivers wait on before the next frame). Prints `presented P discarded D`; exit 1 when a
/// feedback never comes (the client would hang there).
pub fn present(n: u32) -> i32 {
    use wayland_client::protocol::{wl_compositor::WlCompositor, wl_shm::WlShm};
    use wayland_protocols::wp::presentation_time::client::wp_presentation::WpPresentation;
    use wayland_protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
    let Ok(conn) = Connection::connect_to_env() else { return 1 };
    let Ok((globals, mut q)) = registry_queue_init::<Pr>(&conn) else { return 1 };
    let qh = q.handle();
    let (Ok(comp), Ok(shm), Ok(wm), Ok(pres)) = (
        globals.bind::<WlCompositor, _, _>(&qh, 4..=6, ()),
        globals.bind::<WlShm, _, _>(&qh, 1..=1, ()),
        globals.bind::<XdgWmBase, _, _>(&qh, 1..=6, ()),
        globals.bind::<WpPresentation, _, _>(&qh, 1..=2, ()),
    ) else {
        eprintln!("missing a global (wp_presentation?)");
        return 1;
    };
    let (w, h) = (200i32, 150i32);
    let size = (w * h * 4) as usize;
    let fd = unsafe { libc::memfd_create(c"mist-present".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 || unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
        return 1;
    }
    let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    let pool = shm.create_pool(std::os::fd::AsFd::as_fd(&fd), size as i32, &qh, ());
    let buf = pool.create_buffer(0, w, h, w * 4, wayland_client::protocol::wl_shm::Format::Xrgb8888, &qh, ());
    let surf = comp.create_surface(&qh, ());
    let xs = wm.get_xdg_surface(&surf, &qh, ());
    let top = xs.get_toplevel(&qh, ());
    top.set_title("mist-present".into());
    top.set_app_id("mist-present".into());
    surf.commit();
    let mut st = Pr::default();
    let start = std::time::Instant::now();
    let mut sent = 0;
    while start.elapsed() < std::time::Duration::from_secs(15) {
        if st.configured && st.ready && sent < n {
            st.ready = false;
            sent += 1;
            surf.attach(Some(&buf), 0, 0);
            surf.damage_buffer(0, 0, w, h);
            surf.frame(&qh, ());
            pres.feedback(&surf, &qh, ());
            surf.commit();
        }
        if st.presented + st.discarded >= n {
            break;
        }
        let _ = conn.flush();
        // wake at least every 100 ms to check the deadline
        if let Some(guard) = q.prepare_read() {
            use std::os::fd::AsRawFd;
            let mut pfd = libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut pfd, 1, 100) } > 0 {
                let _ = guard.read();
            }
        }
        if q.dispatch_pending(&mut st).is_err() {
            return 1;
        }
    }
    println!("presented {} discarded {} of {sent}", st.presented, st.discarded);
    i32::from(st.presented + st.discarded < n || st.presented == 0)
}

#[derive(Default)]
struct Pr {
    configured: bool,
    /// the last frame callback came: draw the next frame
    ready: bool,
    presented: u32,
    discarded: u32,
}

use std::os::fd::FromRawFd;
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Pr {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Pr {
    fn event(_: &mut Self, wm: &xdg_wm_base::XdgWmBase, ev: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = ev {
            wm.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for Pr {
    fn event(st: &mut Self, xs: &xdg_surface::XdgSurface, ev: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = ev {
            xs.ack_configure(serial);
            if !st.configured {
                st.configured = true;
                st.ready = true;
            }
        }
    }
}
impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, ()> for Pr {
    fn event(st: &mut Self, _: &wayland_client::protocol::wl_callback::WlCallback, _: wayland_client::protocol::wl_callback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        st.ready = true;
    }
}
impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, ()> for Pr {
    fn event(st: &mut Self, _: &wp_presentation_feedback::WpPresentationFeedback, ev: wp_presentation_feedback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            wp_presentation_feedback::Event::Presented { .. } => st.presented += 1,
            wp_presentation_feedback::Event::Discarded => st.discarded += 1,
            _ => {}
        }
    }
}
wayland_client::delegate_noop!(Pr: ignore wayland_client::protocol::wl_compositor::WlCompositor);
wayland_client::delegate_noop!(Pr: ignore wayland_client::protocol::wl_shm::WlShm);
wayland_client::delegate_noop!(Pr: ignore wayland_client::protocol::wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(Pr: ignore wayland_client::protocol::wl_buffer::WlBuffer);
wayland_client::delegate_noop!(Pr: ignore wayland_client::protocol::wl_surface::WlSurface);
wayland_client::delegate_noop!(Pr: ignore xdg_toplevel::XdgToplevel);
wayland_client::delegate_noop!(Pr: ignore wp_presentation::WpPresentation);
