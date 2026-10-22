//! Test client (tests/t-watchdog.sh): a 200x150 window that commits a new frame every BUSY_MS (5) ms without
//! waiting for frame callbacks, the way a broken client burns CPU/GPU. mist-comp's watchdog should name it.
//! BUSY_FRAME=1: a well-behaved animating client instead (tests/t-occlude.sh): draws on every frame callback,
//! marks itself opaque; BUSY_ID (app_id, mist-busy) and BUSY_SIZE (WxH, 200x150) for several of them.
use std::{io::Write, os::fd::AsFd, time::Duration};

use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[derive(Default)]
struct App {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm: Option<xdg_wm_base::XdgWmBase>,
    configured: bool,
    frame_done: bool,
}

impl Dispatch<wl_callback::WlCallback, ()> for App {
    fn event(app: &mut Self, _: &wl_callback::WlCallback, ev: wl_callback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { .. } = ev {
            app.frame_done = true;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(app: &mut Self, reg: &wl_registry::WlRegistry, ev: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = ev {
            match interface.as_str() {
                "wl_compositor" => app.compositor = Some(reg.bind(name, 4, qh, ())),
                "wl_shm" => app.shm = Some(reg.bind(name, 1, qh, ())),
                "xdg_wm_base" => app.wm = Some(reg.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(_: &mut Self, wm: &xdg_wm_base::XdgWmBase, ev: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = ev {
            wm.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for App {
    fn event(app: &mut Self, xs: &xdg_surface::XdgSurface, ev: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = ev {
            xs.ack_configure(serial);
            app.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for App {
    fn event(_: &mut Self, _: &xdg_toplevel::XdgToplevel, ev: xdg_toplevel::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_toplevel::Event::Close = ev {
            std::process::exit(0);
        }
    }
}
delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_region::WlRegion);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);

fn main() {
    let ms = std::env::var("BUSY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App::default();
    q.roundtrip(&mut app).unwrap();
    let surface = app.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xs = app.wm.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let top = xs.get_toplevel(&qh, ());
    let id = std::env::var("BUSY_ID").unwrap_or_else(|_| "mist-busy".into());
    let framed = std::env::var("BUSY_FRAME").is_ok_and(|v| v == "1");
    top.set_title(id.clone());
    top.set_app_id(id);
    surface.commit();
    while !app.configured {
        q.blocking_dispatch(&mut app).unwrap();
    }
    let size = std::env::var("BUSY_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))));
    let (w, h): (i32, i32) = size.unwrap_or((200, 150));
    let fd = unsafe { libc::memfd_create(c"busy".as_ptr(), libc::MFD_CLOEXEC) };
    let mut f = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) };
    f.write_all(&vec![0x60u8; (w * h * 4) as usize]).unwrap();
    let pool = app.shm.as_ref().unwrap().create_pool(f.as_fd(), w * h * 4, &qh, ());
    let buf = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, &qh, ());
    if framed {
        let region = app.compositor.as_ref().unwrap().create_region(&qh, ());
        region.add(0, 0, w, h);
        surface.set_opaque_region(Some(&region));
        println!("busy on frame callbacks");
        loop {
            surface.attach(Some(&buf), 0, 0);
            surface.damage_buffer(0, 0, w, h);
            surface.frame(&qh, ());
            surface.commit();
            app.frame_done = false;
            while !app.frame_done {
                q.blocking_dispatch(&mut app).unwrap();
            }
        }
    }
    println!("busy every {ms} ms");
    loop {
        surface.attach(Some(&buf), 0, 0);
        surface.damage_buffer(0, 0, w, h);
        surface.commit();
        conn.flush().unwrap();
        q.dispatch_pending(&mut app).unwrap();
        if let Some(g) = conn.prepare_read() {
            let _ = g.read();
        }
        q.dispatch_pending(&mut app).unwrap();
        std::thread::sleep(Duration::from_millis(ms));
    }
}
