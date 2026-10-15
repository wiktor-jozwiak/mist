//! wlr-screencopy-unstable-v1 (grim, screenshot.sh, shot.sh), shm buffers only.
//!
//! A copy re-renders that output into an offscreen texture and reads it back once: no cost unless a client
//! asks. Locked sessions refuse (never leak the desktop behind the lock).
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            gles::{GlesRenderer, GlesTexture},
            Bind, ExportMem, Offscreen,
        },
    },
    output::Output,
    reexports::{
        wayland_protocols_wlr::screencopy::v1::server::{
            zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
            zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
        },
        wayland_server::{
            protocol::{wl_buffer::WlBuffer, wl_shm},
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
        },
    },
    utils::{Physical, Rectangle, Transform},
    wayland::shm::with_buffer_contents_mut,
};

use crate::MistComp;

pub struct FrameData {
    output: Option<Output>,
    /// physical, output-relative
    region: Rectangle<i32, Physical>,
}

pub struct Pending {
    frame: ZwlrScreencopyFrameV1,
    buffer: WlBuffer,
}

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, ZwlrScreencopyManagerV1, _>(3, ());
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrScreencopyManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for MistComp {
    fn request(
        state: &mut Self,
        _: &Client,
        mgr: &ZwlrScreencopyManagerV1,
        req: zwlr_screencopy_manager_v1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_screencopy_manager_v1::Request;
        let (frame, output, region) = match req {
            Request::CaptureOutput { frame, output, .. } => (frame, output, None),
            Request::CaptureOutputRegion { frame, output, x, y, width, height, .. } => {
                (frame, output, Some(Rectangle::<i32, Physical>::new((x, y).into(), (width, height).into())))
            }
            Request::Destroy => return,
            _ => return,
        };
        let Some(output) = Output::from_resource(&output) else {
            let f = init.init(frame, FrameData { output: None, region: Default::default() });
            f.failed();
            return;
        };
        let size = output.current_mode().map(|m| m.size).unwrap_or_default();
        let full = Rectangle::from_size(size);
        let region = region.and_then(|r| r.intersection(full)).unwrap_or(full);
        let f = init.init(frame, FrameData { output: Some(output.clone()), region });
        f.buffer(wl_shm::Format::Xrgb8888, region.size.w as u32, region.size.h as u32, region.size.w as u32 * 4);
        if mgr.version() >= 3 {
            f.buffer_done();
        }
        let _ = state;
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameData> for MistComp {
    fn request(
        state: &mut Self,
        _: &Client,
        frame: &ZwlrScreencopyFrameV1,
        req: zwlr_screencopy_frame_v1::Request,
        _: &FrameData,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        use zwlr_screencopy_frame_v1::Request;
        match req {
            Request::Copy { buffer } | Request::CopyWithDamage { buffer } => {
                state.screencopy.push(Pending { frame: frame.clone(), buffer });
                state.dirty = true;
            }
            _ => {}
        }
    }
}

/// Serve queued copies (called from the main loop after rendering).
pub fn process(state: &mut MistComp) {
    if state.screencopy.is_empty() {
        return;
    }
    for p in std::mem::take(&mut state.screencopy) {
        let Some(fd) = p.frame.data::<FrameData>() else { continue };
        if state.locked {
            p.frame.failed();
            continue;
        }
        let Some(output) = fd.output.clone() else { p.frame.failed(); continue };
        let region = fd.region;
        let MistComp { space, gles, udev, headless, anim, .. } = state;
        let pixels = {
            let grab = |r: &mut GlesRenderer| grab(r, space, anim, &output, region);
            if let Some(h) = headless.as_mut() {
                grab(&mut h.renderer)
            } else if let Some(u) = udev.as_mut() {
                grab(&mut u.renderer)
            } else if let Some(g) = gles.as_ref() {
                grab(g.borrow_mut().renderer())
            } else {
                None
            }
        };
        let Some(px) = pixels else {
            p.frame.failed();
            continue;
        };
        let ok = with_buffer_contents_mut(&p.buffer, |ptr, len, d| {
            if d.width != region.size.w || d.height != region.size.h || d.stride < d.width * 4 {
                return false;
            }
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            for y in 0..d.height as usize {
                let row = &px[y * d.width as usize * 4..(y + 1) * d.width as usize * 4];
                let off = d.offset as usize + y * d.stride as usize;
                let out = &mut dst[off..off + d.width as usize * 4];
                // RGBA bytes -> XRGB8888 little endian (B, G, R, X)
                for (o, i) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                    o[0] = i[2];
                    o[1] = i[1];
                    o[2] = i[0];
                    o[3] = 255;
                }
            }
            true
        })
        .unwrap_or(false);
        if !ok {
            p.frame.failed();
            continue;
        }
        p.frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
        let t = smithay::reexports::rustix::time::clock_gettime(smithay::reexports::rustix::time::ClockId::Monotonic);
        p.frame.ready((t.tv_sec as u64 >> 32) as u32, t.tv_sec as u32, t.tv_nsec as u32);
    }
}

/// Render one output's space (windows + layers) offscreen and read `region` back as RGBA rows.
fn grab(r: &mut GlesRenderer, space: &smithay::desktop::Space<smithay::desktop::Window>, anim: &crate::anim::Anims, output: &Output, region: Rectangle<i32, Physical>) -> Option<Vec<u8>> {
    let size = output.current_mode()?.size;
    let mut tex: GlesTexture = r.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into()).ok()?;
    let elements = crate::scene::elements(r, space, None, false, output, anim);
    let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
    let mut fb = r.bind(&mut tex).ok()?;
    tracker.render_output(r, &mut fb, 0, &elements, [0.0, 0.0, 0.0, 1.0]).ok()?;
    let map = r.copy_framebuffer(&fb, Rectangle::new((region.loc.x, region.loc.y).into(), (region.size.w, region.size.h).into()), Fourcc::Abgr8888).ok()?;
    let px = r.map_texture(&map).ok()?.to_vec();
    Some(px)
}
