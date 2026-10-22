//! wlr-screencopy-unstable-v1: screenshots (grim, screenshot.sh, shot.sh) and screen casting through
//! xdg-desktop-portal (-wlr / -hyprland: OBS, Discord, browsers).
//!
//! A copy re-renders that output offscreen: no cost unless a client asks. shm buffers are read back in the
//! buffer's own byte order (BGRA) when the driver can; dmabuf buffers (`screencopy_dmabuf on` in comp.conf)
//! are rendered into directly on the GPU, nothing read back. `copy_with_damage` (casting) waits until the
//! output shows a new frame: a still screen costs nothing. Casting is announced to mist-ui ("screencast"
//! events). Locked sessions refuse (never leak the desktop behind the lock).
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
    wayland::{dmabuf::get_dmabuf, shm::with_buffer_contents_mut},
};

use crate::{udev::OutElem, MistComp};

pub struct FrameData {
    output: Option<Output>,
    /// physical, output-relative
    region: Rectangle<i32, Physical>,
    /// the pointer drawn in (screen casting usually asks for it)
    cursor: bool,
}

pub struct Pending {
    frame: ZwlrScreencopyFrameV1,
    buffer: WlBuffer,
    /// copy_with_damage: served once the output shows a frame newer than this one
    after: Option<u64>,
    damage: bool,
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
        let (frame, output, region, cursor) = match req {
            Request::CaptureOutput { frame, output, overlay_cursor } => (frame, output, None, overlay_cursor != 0),
            Request::CaptureOutputRegion { frame, output, x, y, width, height, overlay_cursor } => {
                (frame, output, Some(Rectangle::<i32, Physical>::new((x, y).into(), (width, height).into())), overlay_cursor != 0)
            }
            Request::Destroy => return,
            _ => return,
        };
        let Some(output) = Output::from_resource(&output) else {
            let f = init.init(frame, FrameData { output: None, region: Default::default(), cursor: false });
            f.failed();
            return;
        };
        let size = output.current_mode().map(|m| m.size).unwrap_or_default();
        let full = Rectangle::from_size(size);
        let region = region.and_then(|r| r.intersection(full)).unwrap_or(full);
        let f = init.init(frame, FrameData { output: Some(output.clone()), region, cursor });
        f.buffer(wl_shm::Format::Xrgb8888, region.size.w as u32, region.size.h as u32, region.size.w as u32 * 4);
        if mgr.version() >= 3 {
            // a whole output can go straight into a client's dmabuf (opt-in until tested on each driver)
            if state.config.screencopy_dmabuf && region == full {
                f.linux_dmabuf(Fourcc::Xrgb8888 as u32, region.size.w as u32, region.size.h as u32);
            }
            f.buffer_done();
        }
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
            Request::Copy { buffer } => {
                state.screencopy.push(Pending { frame: frame.clone(), buffer, after: None, damage: false });
                state.dirty = true;
            }
            Request::CopyWithDamage { buffer } => {
                // the client's first frame at once, then one per new output frame
                let out = frame.data::<FrameData>().and_then(|d| d.output.as_ref().map(|o| o.name())).unwrap_or_default();
                let now = state.out_seq.get(&out).copied().unwrap_or(0);
                let after = frame.client().and_then(|c| state.cast_seen.get(&c.id()).copied()).map(|seen| seen.max(now.saturating_sub(1)));
                state.screencopy.push(Pending { frame: frame.clone(), buffer, after, damage: true });
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
    let mut waiting = Vec::new();
    for p in std::mem::take(&mut state.screencopy) {
        if !smithay::reexports::wayland_server::Resource::is_alive(&p.frame) {
            continue;
        }
        let Some(fd) = p.frame.data::<FrameData>() else { continue };
        if state.locked {
            p.frame.failed();
            continue;
        }
        let Some(output) = fd.output.clone() else { p.frame.failed(); continue };
        let seq = state.out_seq.get(&output.name()).copied().unwrap_or(0);
        if p.after.is_some_and(|a| seq <= a) {
            waiting.push(p);
            continue;
        }
        let (region, cursor) = (fd.region, fd.cursor);
        let ptr = state.seat.get_pointer().map(|p| p.current_location()).unwrap_or_default();
        let og = state.space.output_geometry(&output).unwrap_or_default();
        let scale = smithay::utils::Scale::from(output.current_scale().fractional_scale());
        let MistComp { space, gles, udev, headless, anim, cursor_status, .. } = state;
        let ok = {
            let copy = |r: &mut GlesRenderer, extra: Vec<OutElem>| copy_into(r, space, anim, &output, region, &p.buffer, extra);
            if let Some(h) = headless.as_mut() {
                copy(&mut h.renderer, Vec::new())
            } else if let Some(u) = udev.as_mut() {
                let extra = if cursor { crate::udev::pointer_elements(&mut u.renderer, &mut u.cursors, cursor_status, ptr, og, scale) } else { Vec::new() };
                copy(&mut u.renderer, extra)
            } else if let Some(g) = gles.as_ref() {
                copy(g.borrow_mut().renderer(), Vec::new())
            } else {
                false
            }
        };
        if !ok {
            p.frame.failed();
            continue;
        }
        if p.damage {
            if let Some(c) = p.frame.client() {
                state.cast_seen.insert(c.id(), seq);
            }
            p.frame.damage(0, 0, region.size.w as u32, region.size.h as u32);
            cast_note(state);
        }
        p.frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
        let t = smithay::reexports::rustix::time::clock_gettime(smithay::reexports::rustix::time::ClockId::Monotonic);
        p.frame.ready((t.tv_sec as u64 >> 32) as u32, t.tv_sec as u32, t.tv_nsec as u32);
    }
    state.screencopy = waiting;
}

/// Render `region` of one output into the client's buffer: dmabuf on the GPU, shm read back.
fn copy_into(r: &mut GlesRenderer, space: &smithay::desktop::Space<smithay::desktop::Window>, anim: &crate::anim::Anims, output: &Output, region: Rectangle<i32, Physical>, buffer: &WlBuffer, extra: Vec<OutElem>) -> bool {
    if let Ok(dmabuf) = get_dmabuf(buffer) {
        let mut dmabuf = dmabuf.clone();
        let size = smithay::backend::allocator::Buffer::size(&dmabuf);
        if size.w != region.size.w || size.h != region.size.h {
            return false;
        }
        let mut elements = extra;
        elements.extend(crate::scene::elements(r, space, None, false, output, anim, None).into_iter().map(OutElem::Scene));
        let mut tracker = OutputDamageTracker::new((size.w, size.h), 1.0, Transform::Normal);
        let Ok(mut fb) = r.bind(&mut dmabuf) else { return false };
        let Ok(res) = tracker.render_output(r, &mut fb, 0, &elements, [0.0, 0.0, 0.0, 1.0]) else { return false };
        // the client reads it as soon as `ready` arrives
        let _ = res.sync.wait();
        return true;
    }
    // shm: BGRA rows (the buffer's byte order) when the driver reads them back so, else RGBA swapped here
    let Some((px, swap)) = grab(r, space, anim, output, region, extra) else { return false };
    with_buffer_contents_mut(buffer, |ptr, len, d| {
        if d.width != region.size.w || d.height != region.size.h || d.stride < d.width * 4 {
            return false;
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
        let w4 = d.width as usize * 4;
        for y in 0..d.height as usize {
            let row = &px[y * w4..(y + 1) * w4];
            let off = d.offset as usize + y * d.stride as usize;
            let out = &mut dst[off..off + w4];
            if swap {
                // RGBA bytes -> XRGB8888 little endian (B, G, R, X)
                for (o, i) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                    o[0] = i[2];
                    o[1] = i[1];
                    o[2] = i[0];
                    o[3] = 255;
                }
            } else {
                out.copy_from_slice(row);
            }
        }
        true
    })
    .unwrap_or(false)
}

/// A damage-driven copy was served: tell mist-ui casting started; a timer says when it stops.
fn cast_note(state: &mut MistComp) {
    let first = state.cast_last.is_none();
    state.cast_last = Some(std::time::Instant::now());
    if !first {
        return;
    }
    state.hypr_event("screencast", "1,0");
    let t = smithay::reexports::calloop::timer::Timer::from_duration(std::time::Duration::from_secs(1));
    let _ = state.loop_handle.insert_source(t, |_, _, st| {
        use smithay::reexports::calloop::timer::TimeoutAction;
        // no frame asked for 3 s (the stream ended or the client is gone)
        if st.cast_last.is_some_and(|t| t.elapsed().as_secs() >= 3) {
            st.cast_last = None;
            st.cast_seen.clear();
            st.hypr_event("screencast", "0,0");
            return TimeoutAction::Drop;
        }
        TimeoutAction::ToDuration(std::time::Duration::from_secs(1))
    });
}

/// Render one output's space (windows + layers, `extra` on top) offscreen once and read `region` back: BGRA when
/// the driver reads it so (`false`), else RGBA (`true`: bytes to swap).
fn grab(r: &mut GlesRenderer, space: &smithay::desktop::Space<smithay::desktop::Window>, anim: &crate::anim::Anims, output: &Output, region: Rectangle<i32, Physical>, extra: Vec<OutElem>) -> Option<(Vec<u8>, bool)> {
    let size = output.current_mode()?.size;
    let mut tex: GlesTexture = r.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into()).ok()?;
    let mut elements = extra;
    elements.extend(crate::scene::elements(r, space, None, false, output, anim, None).into_iter().map(OutElem::Scene));
    let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
    let mut fb = r.bind(&mut tex).ok()?;
    tracker.render_output(r, &mut fb, 0, &elements, [0.0, 0.0, 0.0, 1.0]).ok()?;
    let rect = Rectangle::new((region.loc.x, region.loc.y).into(), (region.size.w, region.size.h).into());
    for (format, swap) in [(Fourcc::Argb8888, false), (Fourcc::Abgr8888, true)] {
        if let Ok(map) = r.copy_framebuffer(&fb, rect, format) {
            if let Ok(px) = r.map_texture(&map) {
                return Some((px.to_vec(), swap));
            }
        }
    }
    None
}
