// Window thumbnails (the dock's window picker): hyprland_toplevel_export_v1 into wl_shm, on a thread with its
// own Wayland connection, only while the picker opens. Each window: one frame (its full-size buffer, freed at
// once), box-filtered down to the thumbnail and sent to the app as it arrives. Nothing runs between pickers.
use crate::sys::Msg;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use tiny_skia::Pixmap;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

#[allow(non_upper_case_globals, non_camel_case_types, dead_code, clippy::all)]
pub mod proto {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-export-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-export-v1.xml");
}
use proto::hyprland_toplevel_export_frame_v1::{self as frame, HyprlandToplevelExportFrameV1};
use proto::hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;

#[derive(Default)]
struct St {
    buf: Option<(u32, u32, u32, u32)>, // format, w, h, stride (wl_shm)
    done: bool,
    ready: bool,
    failed: bool,
    y_invert: bool,
}

// capture `addrs` (hyprctl addresses), each scaled to fit tw x th, sent as Msg::WinThumb(address, pixmap)
pub fn capture(addrs: Vec<String>, tw: u32, th: u32, tx: calloop::channel::Sender<Msg>) {
    std::thread::spawn(move || {
        if let Err(e) = run(&addrs, tw, th, &tx) {
            eprintln!("mist-ui: window thumbnails: {e}");
        }
    });
}

fn run(addrs: &[String], tw: u32, th: u32, tx: &calloop::channel::Sender<Msg>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut q) = registry_queue_init::<St>(&conn).map_err(|e| e.to_string())?;
    let qh = q.handle();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let mgr: HyprlandToplevelExportManagerV1 = globals.bind(&qh, 1..=2, ()).map_err(|e| e.to_string())?;
    for a in addrs {
        let Ok(full) = u64::from_str_radix(a.trim_start_matches("0x"), 16) else { continue };
        let mut st = St::default();
        let fr = mgr.capture_toplevel(0, (full & 0xffff_ffff) as u32, &qh, ());
        // v2 ends the buffer list with buffer_done; a v1 server stops after its buffer events
        while !st.done && !st.failed {
            q.blocking_dispatch(&mut st).map_err(|e| e.to_string())?;
            if st.buf.is_some() && mgr.version() < 2 {
                break;
            }
        }
        let Some((fmt, w, h, stride)) = st.buf.filter(|_| !st.failed) else {
            fr.destroy();
            continue;
        };
        let size = (stride * h) as usize;
        let fd = unsafe { libc::memfd_create(c"mist-wincap".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            fr.destroy();
            continue;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } != 0 {
            fr.destroy();
            continue;
        }
        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        let fmt_e = wl_shm::Format::try_from(fmt).unwrap_or(wl_shm::Format::Xrgb8888);
        let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, fmt_e, &qh, ());
        fr.copy(&buffer, 1);
        while !st.ready && !st.failed {
            q.blocking_dispatch(&mut st).map_err(|e| e.to_string())?;
        }
        if st.ready {
            let map = unsafe { libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
            if map != libc::MAP_FAILED {
                let px = unsafe { std::slice::from_raw_parts(map as *const u8, size) };
                let alpha = matches!(fmt_e, wl_shm::Format::Argb8888);
                if let Some(pm) = shrink(px, w, h, stride, tw, th, st.y_invert, alpha) {
                    let _ = tx.send(Msg::WinThumb(a.clone(), pm));
                }
                unsafe { libc::munmap(map, size) };
            }
        }
        fr.destroy();
        buffer.destroy();
        pool.destroy();
    }
    mgr.destroy();
    let _ = conn.flush();
    Ok(())
}

// B,G,R,A/X bytes (wl_shm, little endian) -> a premultiplied pixmap fitting tw x th: each target pixel is the
// average of its whole source box (no aliasing at 1/8 scale, unlike a bilinear draw)
pub fn shrink(px: &[u8], w: u32, h: u32, stride: u32, tw: u32, th: u32, y_invert: bool, alpha: bool) -> Option<Pixmap> {
    if w == 0 || h == 0 {
        return None;
    }
    let s = (tw as f32 / w as f32).min(th as f32 / h as f32).min(1.0);
    let (ow, oh) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
    let mut pm = Pixmap::new(ow, oh)?;
    let out = pm.data_mut();
    for oy in 0..oh {
        let y0 = (oy as u64 * h as u64 / oh as u64) as u32;
        let y1 = (((oy + 1) as u64 * h as u64 / oh as u64) as u32).max(y0 + 1).min(h);
        for ox in 0..ow {
            let x0 = (ox as u64 * w as u64 / ow as u64) as u32;
            let x1 = (((ox + 1) as u64 * w as u64 / ow as u64) as u32).max(x0 + 1).min(w);
            let (mut b, mut g, mut r, mut a) = (0u32, 0u32, 0u32, 0u32);
            for y in y0..y1 {
                let sy = if y_invert { h - 1 - y } else { y };
                let row = (sy * stride) as usize;
                for x in x0..x1 {
                    let i = row + x as usize * 4;
                    let pa = if alpha { px[i + 3] as u32 } else { 255 };
                    // wl_shm ARGB is premultiplied already
                    b += px[i] as u32;
                    g += px[i + 1] as u32;
                    r += px[i + 2] as u32;
                    a += pa;
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            let o = ((oy * ow + ox) * 4) as usize;
            let a8 = (a / n) as u8;
            out[o] = ((r / n) as u8).min(a8);
            out[o + 1] = ((g / n) as u8).min(a8);
            out[o + 2] = ((b / n) as u8).min(a8);
            out[o + 3] = a8;
        }
    }
    Some(pm)
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for St {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_shm::WlShm, ()> for St {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for St {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_buffer::WlBuffer, ()> for St {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandToplevelExportManagerV1, ()> for St {
    fn event(_: &mut Self, _: &HyprlandToplevelExportManagerV1, _: proto::hyprland_toplevel_export_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandToplevelExportFrameV1, ()> for St {
    fn event(st: &mut Self, _: &HyprlandToplevelExportFrameV1, ev: frame::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            // the first 8-bit RGB shm format offered
            frame::Event::Buffer { format, width, height, stride } => {
                if st.buf.is_none() && (format == wl_shm::Format::Xrgb8888 as u32 || format == wl_shm::Format::Argb8888 as u32) {
                    st.buf = Some((format, width, height, stride));
                }
            }
            frame::Event::BufferDone => st.done = true,
            frame::Event::Flags { flags } => {
                st.y_invert = flags & 1 != 0; // y_invert
            }
            frame::Event::Ready { .. } => st.ready = true,
            frame::Event::Failed => st.failed = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_box_averages_and_flips() {
        // 4x2 XRGB: left half white, right half black; y_invert swaps rows (top row red here)
        let mut px = vec![0u8; 4 * 4 * 2];
        for y in 0..2 {
            for x in 0..4 {
                let i = (y * 16 + x * 4) as usize;
                let v = if x < 2 { 255 } else { 0 };
                px[i..i + 4].copy_from_slice(&[v, v, v, 0]);
            }
        }
        px[0..4].copy_from_slice(&[0, 0, 255, 0]); // (0,0) red
        let pm = shrink(&px, 4, 2, 16, 2, 1, false, false).unwrap();
        assert_eq!((pm.width(), pm.height()), (2, 1));
        let d = pm.data();
        // left box: red + 3 white -> r 255, g/b 191; opaque (XRGB)
        assert_eq!(&d[0..4], &[255, 191, 191, 255]);
        assert_eq!(&d[4..8], &[0, 0, 0, 255]);
        let flipped = shrink(&px, 4, 2, 16, 4, 2, true, false).unwrap();
        // row 1 of the result is source row 0: the red pixel
        assert_eq!(&flipped.data()[16..20], &[255, 0, 0, 255]);
    }
}
