//! `--headless`: render offscreen on the GPU render node, no window, no DRM master. For tests: the host
//! compositor never sees it (a nested window on a hidden workspace stops getting its buffers back and blocks
//! eglSwapBuffers). Output size MIST_COMP_SIZE=WxH (1920x1080), name MIST_COMP_OUTPUT; several monitors side
//! by side: MIST_COMP_OUTPUTS="DP-3:1920x1080,HDMI-A-1:1920x1080".
use std::time::Duration;

use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            gbm::GbmDevice,
            Fourcc,
        },
        drm::DrmDeviceFd,
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            damage::OutputDamageTracker,
            gles::{GlesRenderbuffer, GlesRenderer},
            Bind, ImportDma, ImportMemWl, Offscreen,
        },
    },
    desktop::layer_map_for_output,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{calloop::EventLoop, rustix::fs::OFlags},
    utils::{DeviceFd, Transform},
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufState},
};

use crate::{CalloopData, MistComp};

struct Out {
    buffer: GlesRenderbuffer,
    output: Output,
    damage: OutputDamageTracker,
    size: (i32, i32),
}

pub struct Headless {
    pub renderer: GlesRenderer,
    outs: Vec<Out>,
    /// 60 Hz frame clock: a frame waits for the next tick instead of rendering on every commit
    last: Option<std::time::Instant>,
    tick_armed: bool,
}

impl Headless {
    pub fn import_dmabuf(&mut self, d: &Dmabuf) -> bool {
        self.renderer.import_dmabuf(d, None).is_ok()
    }
}

pub fn init(_event_loop: &mut EventLoop<'static, CalloopData>, state: &mut MistComp) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("MIST_COMP_RENDER_NODE").unwrap_or_else(|_| "/dev/dri/renderD128".into());
    let open = || -> Result<EGLDisplay, Box<dyn std::error::Error>> {
        let fd = smithay::reexports::rustix::fs::open(path.as_str(), OFlags::RDWR | OFlags::CLOEXEC, smithay::reexports::rustix::fs::Mode::empty())?;
        let gbm = GbmDevice::new(DrmDeviceFd::new(DeviceFd::from(fd)))?;
        Ok(unsafe { EGLDisplay::new(gbm)? })
    };
    // no GPU (CI, containers) or MIST_COMP_SOFTWARE=1: Mesa's software device (llvmpipe), shm clients only
    let soft = std::env::var_os("MIST_COMP_SOFTWARE").is_some();
    let egl = match if soft { Err("software requested".into()) } else { open() } {
        Ok(e) => e,
        Err(e) => {
            let dev = EGLDevice::enumerate()?.find(|d| d.is_software()).ok_or(e)?;
            tracing::info!("headless: software rendering (llvmpipe)");
            unsafe { EGLDisplay::new(dev)? }
        }
    };
    let node = EGLDevice::device_for_display(&egl).ok().and_then(|d| d.try_get_render_node().ok().flatten());
    let ctx = EGLContext::new(&egl)?;
    let mut renderer = unsafe { GlesRenderer::new(ctx)? };

    let dh = state.display_handle.clone();
    state.shm_state.update_formats(renderer.shm_formats());
    let formats = renderer.dmabuf_formats();
    let mut dmabuf_state = DmabufState::new();
    let global = match node.and_then(|n| DmabufFeedbackBuilder::new(n.dev_id(), formats.clone()).build().ok()) {
        Some(fb) => dmabuf_state.create_global_with_default_feedback::<MistComp>(&dh, &fb),
        None => dmabuf_state.create_global::<MistComp>(&dh, formats),
    };
    state.dmabuf = Some((dmabuf_state, global));

    let size = |v: &str| v.split_once('x').and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?)));
    let list: Vec<(String, (i32, i32))> = match std::env::var("MIST_COMP_OUTPUTS") {
        Ok(v) => v.split(',').filter_map(|o| o.split_once(':').and_then(|(n, sz)| Some((n.to_string(), size(sz)?)))).collect(),
        Err(_) => vec![(
            std::env::var("MIST_COMP_OUTPUT").unwrap_or_else(|_| "HEADLESS-1".into()),
            std::env::var("MIST_COMP_SIZE").ok().and_then(|v| size(&v)).unwrap_or((1920, 1080)),
        )],
    };
    let mut outs = Vec::new();
    let mut x = 0;
    for (name, (w, h)) in list {
        let output = Output::new(name, PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Mist".into(), model: "Headless".into() });
        let _global = output.create_global::<MistComp>(&dh);
        let mode = Mode { size: (w, h).into(), refresh: 60_000 };
        output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((x, 0).into()));
        output.set_preferred(mode);
        state.space.map_output(&output, (x, 0));
        x += w;
        let buffer: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, (w, h).into())?;
        let damage = OutputDamageTracker::from_output(&output);
        outs.push(Out { buffer, output, damage, size: (w, h) });
    }
    state.headless = Some(Headless { renderer, outs, last: None, tick_armed: false });
    state.fix_ws();
    println!("WAYLAND_DISPLAY={}", state.socket_name.to_string_lossy());
    Ok(())
}

pub fn render(state: &mut MistComp) {
    const FRAME: Duration = Duration::from_micros(16_667);
    let Some(hl) = state.headless.as_mut() else { return };
    let since = hl.last.map(|t| t.elapsed()).unwrap_or(FRAME);
    if since < FRAME {
        // too early: render on the tick (one timer at a time; clients wait for their frame callback)
        if !hl.tick_armed {
            hl.tick_armed = true;
            let timer = smithay::reexports::calloop::timer::Timer::from_duration(FRAME - since);
            let _ = state.loop_handle.insert_source(timer, |_, _, st: &mut CalloopData| {
                if let Some(hl) = st.headless.as_mut() {
                    hl.tick_armed = false;
                }
                st.dirty = true;
                smithay::reexports::calloop::timer::TimeoutAction::Drop
            });
        }
        return;
    }
    hl.last = Some(std::time::Instant::now());
    state.frames += 1;
    let dnd = state.dnd_at();
    let MistComp { headless, space, popups, locked, lock, shot, shot_output: state_shot_output, force_full, start_time, anim, out_seq, frames, .. } = state;
    let Some(hl) = headless.as_mut() else { return };
    let age = if std::mem::take(force_full) { 0 } else { 1 };
    let now = start_time.elapsed();
    for (i, o) in hl.outs.iter_mut().enumerate() {
        let output = o.output.clone();
        let Ok(mut fb) = hl.renderer.bind(&mut o.buffer) else { continue };
        let elems = crate::scene::elements(&mut hl.renderer, space, lock.surfaces.get(&output.name()), *locked, &output, anim, dnd.as_ref());
        let clear = if *locked { [0.0, 0.0, 0.0, 1.0] } else { [0.1, 0.1, 0.1, 1.0] };
        match o.damage.render_output(&mut hl.renderer, &mut fb, age, &elems, clear) {
            Ok(r) if r.damage.is_some() => *out_seq.entry(output.name()).or_insert(0) += 1,
            Ok(_) => {}
            Err(e) => tracing::warn!("headless render: {e:?}"),
        }
        // IPC `shot`: the named output, else the first
        let want = state_shot_output.as_deref().map(|n| n == output.name()).unwrap_or(i == 0);
        if want {
            if let Some((path, mut reply)) = shot.take() {
                use std::io::Write;
                let r = crate::winit::write_shot(&mut hl.renderer, &fb, o.size.into(), &path, false);
                let _ = reply.write_all(match r { Ok(()) => "ok\n".to_string(), Err(e) => format!("error {e}\n") }.as_bytes());
            }
        }
        drop(fb);
        // wp_presentation: shown at this tick (no vblank here)
        crate::scene::take_feedback(space, lock.surfaces.get(&output.name()), &output).presented::<_, smithay::utils::Monotonic>(
            smithay::utils::Clock::<smithay::utils::Monotonic>::new().now(),
            smithay::wayland::presentation::Refresh::fixed(FRAME),
            *frames,
            smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind::empty(),
        );
        for l in layer_map_for_output(&output).layers() {
            l.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
        if let Some(l) = lock.surfaces.get(&output.name()) {
            smithay::desktop::utils::send_frames_surface_tree(l.wl_surface(), &output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
    }
    if let Some(first) = hl.outs.first().map(|o| o.output.clone()) {
        space.elements().for_each(|w| w.send_frame(&first, now, Some(Duration::ZERO), |_, _| Some(first.clone())));
    }
    if let (Some((icon, _)), Some(first)) = (dnd.as_ref(), hl.outs.first()) {
        smithay::desktop::utils::send_frames_surface_tree(icon, &first.output, now, Some(Duration::ZERO), |_, _| Some(first.output.clone()));
    }
    space.refresh();
    popups.cleanup();
    state.fifo_signal();
}

/// A frame tick is armed (FIFO waiters are released by it, not by the idle loop).
pub fn frame_pending(state: &MistComp) -> bool {
    state.headless.as_ref().is_some_and(|h| h.tick_armed)
}
