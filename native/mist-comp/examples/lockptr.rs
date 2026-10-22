//! Test client (tests/t-lockptr.sh): a 300x200 window that asks for a persistent pointer lock as soon as
//! it is configured, wherever the pointer is; prints "locked" / "unlocked" as the compositor reports them.
use std::{io::Write, os::fd::AsFd};

use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::{
    wp::pointer_constraints::zv1::client::{zwp_locked_pointer_v1, zwp_pointer_constraints_v1},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Default)]
struct App {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    constraints: Option<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1>,
    surface: Option<wl_surface::WlSurface>,
    pointer: Option<wl_pointer::WlPointer>,
    lock: Option<zwp_locked_pointer_v1::ZwpLockedPointerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(app: &mut Self, reg: &wl_registry::WlRegistry, ev: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = ev {
            match interface.as_str() {
                "wl_compositor" => app.compositor = Some(reg.bind(name, 4, qh, ())),
                "wl_shm" => app.shm = Some(reg.bind(name, 1, qh, ())),
                "xdg_wm_base" => app.wm = Some(reg.bind(name, 1, qh, ())),
                "wl_seat" => app.seat = Some(reg.bind(name, 5, qh, ())),
                "zwp_pointer_constraints_v1" => app.constraints = Some(reg.bind(name, 1, qh, ())),
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
    fn event(app: &mut Self, xs: &xdg_surface::XdgSurface, ev: xdg_surface::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let xdg_surface::Event::Configure { serial } = ev else { return };
        xs.ack_configure(serial);
        let surface = app.surface.clone().unwrap();
        if app.lock.is_none() {
            let (w, h) = (300i32, 200i32);
            let mut f = tempfile();
            f.write_all(&vec![0x80u8; (w * h * 4) as usize]).unwrap();
            let pool = app.shm.as_ref().unwrap().create_pool(f.as_fd(), w * h * 4, qh, ());
            let buf = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, qh, ());
            surface.attach(Some(&buf), 0, 0);
            surface.damage(0, 0, w, h);
            if let (Some(c), Some(p)) = (app.constraints.as_ref(), app.pointer.as_ref()) {
                app.lock = Some(c.lock_pointer(&surface, p, None, zwp_pointer_constraints_v1::Lifetime::Persistent, qh, ()));
                println!("lock requested");
            }
        }
        surface.commit();
    }
}

impl Dispatch<zwp_locked_pointer_v1::ZwpLockedPointerV1, ()> for App {
    fn event(_: &mut Self, _: &zwp_locked_pointer_v1::ZwpLockedPointerV1, ev: zwp_locked_pointer_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            zwp_locked_pointer_v1::Event::Locked => println!("locked"),
            zwp_locked_pointer_v1::Event::Unlocked => println!("unlocked"),
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(_: &mut Self, _: &wl_pointer::WlPointer, _: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
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
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: ignore zwp_pointer_constraints_v1::ZwpPointerConstraintsV1);

fn tempfile() -> std::fs::File {
    let fd = unsafe { libc::memfd_create(c"lockptr".as_ptr(), libc::MFD_CLOEXEC) };
    assert!(fd >= 0, "memfd");
    unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) }
}

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App::default();
    q.roundtrip(&mut app).unwrap();
    app.pointer = app.seat.as_ref().map(|s| s.get_pointer(&qh, ()));
    let surface = app.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xs = app.wm.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let top = xs.get_toplevel(&qh, ());
    top.set_title("mist-lockptr".into());
    top.set_app_id("mist-lockptr".into());
    app.surface = Some(surface.clone());
    surface.commit();
    loop {
        q.blocking_dispatch(&mut app).unwrap();
        let _ = std::io::stdout().flush();
    }
}
