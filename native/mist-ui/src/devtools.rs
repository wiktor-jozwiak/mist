// Checks against the running compositor (mist-comp/tests/ci.sh, by hand on Hyprland):
//   --wincap ADDR OUT.ppm          one window through hyprland_toplevel_export_v1 (what the dock's picker does)
//   --global-shortcut APP ID       register a portal-style global shortcut, print `pressed` / `released`
//   --inhibit [APP]                keep the screen on (org.freedesktop.ScreenSaver.Inhibit) until stopped
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
