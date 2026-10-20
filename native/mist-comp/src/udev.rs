//! Real session backend: libseat + udev + DRM/GBM (single GPU) + libinput.
//!
//! Damage driven like the nested backend: an output renders only when the state is dirty and its previous
//! frame has been scanned out (vblank). Idle = no renders, no timers. NVIDIA: no overlay planes (they break),
//! cursor plane and direct scan-out stay on.
use std::{collections::HashMap, path::PathBuf, time::Duration};

use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode,
        },
        egl::{context::ContextPriority, EGLContext, EGLDevice, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::{
                memory::MemoryRenderBufferRenderElement,
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind,
            },
            gles::GlesRenderer,
            ImportDma, ImportMemWl,
        },
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
    },
    desktop::{
        layer_map_for_output,
    },
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::EventLoop,
        drm::{
            control::{connector, crtc, Device as _, ModeTypeFlags},
            Device as _,
        },
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::backend::GlobalId,
    },
    utils::{DeviceFd, IsAlive, Scale, Transform},
    wayland::{
        compositor::with_states,
        dmabuf::{DmabufFeedbackBuilder, DmabufState},
    },
};

use crate::{cursor::Cursors, CalloopData, MistComp};

smithay::backend::renderer::element::render_elements! {
    pub OutElem<=GlesRenderer>;
    Pointer = MemoryRenderBufferRenderElement<GlesRenderer>,
    CursorSurface = WaylandSurfaceRenderElement<GlesRenderer>,
    Scene = crate::scene::SceneElem,
}

type Mgr = DrmOutputManager<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;
type Out = DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

struct Surf {
    output: Output,
    drm_output: Out,
    global: GlobalId,
    /// a frame is queued: wait for its vblank before rendering again
    pending: bool,
    /// adaptive sync on now; None = the output refused it (never retried)
    vrr: Option<bool>,
    /// the connector's modes (wlr-output-management picks among them)
    modes: Vec<smithay::reexports::drm::control::Mode>,
}

pub struct Udev {
    pub session: LibSeatSession,
    node: DrmNode,
    mgr: Mgr,
    pub renderer: GlesRenderer,
    surfaces: HashMap<crtc::Handle, Surf>,
    pub cursors: Cursors,
    nvidia: bool,
    dpms_off: bool,
    /// night light: warm gamma ramp on every crtc (hardware LUT, no per-frame shader pass)
    pub night: bool,
}

impl Udev {
    /// DPMS: off clears every crtc (planes disabled, no renders until on); on renders a full frame.
    pub fn set_active(&mut self, on: bool) {
        self.dpms_off = !on;
        for s in self.surfaces.values_mut() {
            if !on {
                if let Err(e) = s.drm_output.with_compositor(|c| c.clear()) {
                    tracing::warn!("dpms off {}: {e}", s.output.name());
                }
            }
            s.pending = false;
        }
    }

    /// Night light on/off: rewrite each crtc's gamma ramp (~4000 K: green x0.84, blue x0.66, as the old shader).
    pub fn set_night(&mut self, on: bool) {
        self.night = on;
        let crtcs: Vec<crtc::Handle> = self.surfaces.keys().copied().collect();
        for c in crtcs {
            self.apply_gamma(c);
        }
    }

    fn apply_gamma(&self, c: crtc::Handle) {
        let dev = self.mgr.device();
        let Ok(info) = dev.get_crtc(c) else { return };
        let n = info.gamma_length() as usize;
        if n < 2 {
            return;
        }
        let (g, b) = if self.night { (0.84, 0.66) } else { (1.0, 1.0) };
        let ramp = |k: f64| -> Vec<u16> { (0..n).map(|i| (i as f64 / (n - 1) as f64 * k * 65535.0).round() as u16).collect() };
        if let Err(e) = dev.set_gamma(c, &ramp(1.0), &ramp(g), &ramp(b)) {
            tracing::warn!("gamma {c:?}: {e}");
        }
    }

    fn crtc_of(&self, output: &str) -> Option<crtc::Handle> {
        self.surfaces.iter().find(|(_, s)| s.output.name() == output).map(|(c, _)| *c)
    }

    /// wlr-output-management: switch an output to one of its connector's modes (size and refresh).
    pub fn set_mode(&mut self, output: &str, mode: WlMode) -> bool {
        let Some(c) = self.crtc_of(output) else { return false };
        let Some(s) = self.surfaces.get_mut(&c) else { return false };
        let Some(m) = s.modes.iter().find(|m| WlMode::from(**m) == mode).copied() else { return false };
        match s.drm_output.use_mode::<_, OutElem>(m, &mut self.renderer, &DrmOutputRenderElements::default()) {
            Ok(()) => {
                s.output.change_current_state(Some(mode), None, None, None);
                true
            }
            Err(e) => {
                tracing::warn!("mode {output}: {e:?}");
                false
            }
        }
    }

    /// wlr-gamma-control: the output's gamma ramp length (None: no such output / no LUT).
    pub fn gamma_size(&self, output: &str) -> Option<u32> {
        let c = self.crtc_of(output)?;
        let n = self.mgr.device().get_crtc(c).ok()?.gamma_length();
        (n >= 2).then_some(n)
    }

    /// A client's ramps (red, green, blue, each gamma_size long) replace the night light's on this output.
    pub fn set_gamma(&self, output: &str, r: &[u16], g: &[u16], b: &[u16]) -> bool {
        let Some(c) = self.crtc_of(output) else { return false };
        self.mgr.device().set_gamma(c, r, g, b).is_ok()
    }

    /// The client is gone: back to Mist's own ramp (night light or linear).
    pub fn reset_gamma(&self, output: &str) {
        if let Some(c) = self.crtc_of(output) {
            self.apply_gamma(c);
        }
    }

    /// A frame is queued on some output (its vblank will come).
    pub fn any_pending(&self) -> bool {
        self.surfaces.values().any(|s| s.pending)
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> bool {
        self.renderer.import_dmabuf(dmabuf, None).is_ok()
    }
}

pub fn init(event_loop: &mut EventLoop<CalloopData>, data: &mut CalloopData) -> Result<(), Box<dyn std::error::Error>> {
    let (session, notifier) = LibSeatSession::new()?;
    let seat = session.seat();

    let path: PathBuf = match std::env::var("MIST_COMP_DRM_DEVICE") {
        Ok(p) => p.into(),
        Err(_) => primary_gpu(&seat)?
            .or_else(|| all_gpus(&seat).ok()?.into_iter().next())
            .ok_or("no GPU")?,
    };
    tracing::info!("gpu {}", path.display());
    let mut session_c = session.clone();
    let fd = session_c.open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let node = DrmNode::from_file(fd.clone())?;
    let (drm, drm_notifier) = DrmDevice::new(fd.clone(), true)?;
    // explicit sync (NVIDIA 555+, Mesa): clients hand acquire / release points instead of implicit fences
    if !data.config.no_explicit_sync && smithay::wayland::drm_syncobj::supports_syncobj_eventfd(&fd) {
        tracing::info!("explicit sync: linux-drm-syncobj-v1");
        data.syncobj = Some(smithay::wayland::drm_syncobj::DrmSyncobjState::new::<MistComp>(&data.display_handle, fd.clone()));
    }
    let gbm = GbmDevice::new(fd)?;

    let egl = unsafe { EGLDisplay::new(gbm.clone())? };
    let render_node = EGLDevice::device_for_display(&egl)
        .ok()
        .and_then(|d| d.try_get_render_node().ok().flatten())
        .unwrap_or(node);
    let ctx = EGLContext::new_with_priority(&egl, ContextPriority::High)?;
    let renderer = unsafe { GlesRenderer::new(ctx)? };

    let nvidia = drm.get_driver().map(|d| d.name().to_string_lossy().to_lowercase().contains("nvidia")).unwrap_or(false);
    let render_formats = renderer.egl_context().dmabuf_render_formats().clone();
    let mgr = DrmOutputManager::new(
        drm,
        GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT),
        GbmFramebufferExporter::new(gbm.clone(), Some(render_node)),
        Some(gbm),
        [Fourcc::Argb8888, Fourcc::Xrgb8888, Fourcc::Abgr8888],
        render_formats,
    );

    let dh = data.display_handle.clone();
    let state = data;

    // shm formats + linux-dmabuf v4 from the real GPU
    let udev = Udev { session: session.clone(), node, mgr, renderer, surfaces: HashMap::new(), cursors: Cursors::load(), nvidia, dpms_off: false, night: false };
    state.shm_state.update_formats(udev.renderer.shm_formats());
    let formats = udev.renderer.dmabuf_formats();
    let feedback = DmabufFeedbackBuilder::new(render_node.dev_id(), formats).build()?;
    let mut dmabuf_state = DmabufState::new();
    let global = dmabuf_state.create_global_with_default_feedback::<MistComp>(&dh, &feedback);
    state.dmabuf = Some((dmabuf_state, global));

    // vblank: the queued frame is on screen
    event_loop.handle().insert_source(drm_notifier, move |event, _, data| match event {
        DrmEvent::VBlank(crtc) => vblank(data, crtc),
        DrmEvent::Error(e) => tracing::error!("drm: {e:?}"),
    })?;

    // input
    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput.udev_assign_seat(&seat).map_err(|_| "libinput seat")?;
    event_loop.handle().insert_source(LibinputInputBackend::new(libinput.clone()), |event, _, data| {
        if let smithay::backend::input::InputEvent::DeviceAdded { device } = &event {
            let mut d = device.clone();
            if let Some(flat) = data.config.pointer_flat {
                if d.config_accel_is_available() {
                    let p = if flat { smithay::reexports::input::AccelProfile::Flat } else { smithay::reexports::input::AccelProfile::Adaptive };
                    let _ = d.config_accel_set_profile(p);
                }
            }
            // touchpads: tap to click, like every desktop
            if d.config_tap_finger_count() > 0 {
                let _ = d.config_tap_set_enabled(true);
            }
        }
        data.process_input_event(event);
    })?;

    // VT switch away / back
    event_loop.handle().insert_source(notifier, move |event, _, data| match event {
        SessionEvent::PauseSession => {
            libinput.suspend();
            if let Some(u) = data.udev.as_mut() {
                u.mgr.pause();
            }
        }
        SessionEvent::ActivateSession => {
            let _ = libinput.resume();
            if let Some(u) = data.udev.as_mut() {
                let _ = u.mgr.activate(false);
                for s in u.surfaces.values_mut() {
                    s.pending = false;
                }
            }
            data.force_full = true;
            data.dirty = true;
        }
    })?;

    // monitors hot-plug
    let udev_backend = UdevBackend::new(&seat)?;
    event_loop.handle().insert_source(udev_backend, move |event, _, data| {
        if let UdevEvent::Changed { device_id } = event {
            if data.udev.as_ref().is_some_and(|u| u.node.dev_id() == device_id) {
                let dh = data.display_handle.clone();
                scan(data, &dh);
            }
        }
    })?;

    let mut vt_session = session;
    state.vt_switch = Some(Box::new(move |vt| {
        if let Err(e) = vt_session.change_vt(vt) {
            tracing::warn!("vt {vt}: {e}");
        }
    }));
    state.udev = Some(udev);
    scan(state, &dh);
    state.dirty = true;
    Ok(())
}

/// Connected connectors <-> outputs.
fn scan(state: &mut MistComp, dh: &smithay::reexports::wayland_server::DisplayHandle) {
    let Some(udev) = state.udev.as_mut() else { return };
    // monitoraddedv2 / monitorremovedv2 for mist-ui's island, sent once the outputs are settled
    let mut events: Vec<(&str, String)> = Vec::new();
    let drm = udev.mgr.device();
    let Ok(res) = drm.resource_handles() else { return };
    let mut wanted: Vec<(connector::Info, crtc::Handle)> = Vec::new();
    let mut used = Vec::new();
    for &conn in res.connectors() {
        let Ok(info) = drm.get_connector(conn, false) else { continue };
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        // keep the crtc it already drives, else the first free one its encoders allow
        let existing = udev.surfaces.iter().find(|(_, s)| s.output.name() == conn_name(&info)).map(|(c, _)| *c);
        let crtc = existing.or_else(|| {
            info.encoders().iter().filter_map(|e| drm.get_encoder(*e).ok()).find_map(|enc| {
                res.filter_crtcs(enc.possible_crtcs())
                    .into_iter()
                    .find(|c| !used.contains(c) && !udev.surfaces.contains_key(c))
            })
        });
        if let Some(crtc) = crtc {
            used.push(crtc);
            wanted.push((info, crtc));
        }
    }

    // gone
    let gone: Vec<crtc::Handle> = udev.surfaces.keys().filter(|c| !used.contains(c)).copied().collect();
    for crtc in gone {
        if let Some(s) = udev.surfaces.remove(&crtc) {
            tracing::info!("output {} gone", s.output.name());
            events.push(("monitorremovedv2", format!("0,{},{}", s.output.name(), s.output.description())));
            state.space.unmap_output(&s.output);
            dh.remove_global::<MistComp>(s.global);
        }
    }

    // new
    let layout = crate::config::outputs(&state.config);
    for (info, crtc) in wanted {
        if udev.surfaces.contains_key(&crtc) {
            continue;
        }
        let name = conn_name(&info);
        let conf = layout.iter().find(|o| o.name == name).cloned();
        let mode_i = info.modes().iter().position(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)).unwrap_or(0);
        let mut drm_mode = info.modes()[mode_i];
        // the configured size (and refresh), else the preferred size; the highest refresh there (165 Hz panels
        // prefer 60)
        let want = conf.as_ref().and_then(|c| c.mode);
        let size = want.map(|(w, h, _)| (w as u16, h as u16)).filter(|sz| info.modes().iter().any(|m| m.size() == *sz)).unwrap_or(drm_mode.size());
        let refresh = |m: &smithay::reexports::drm::control::Mode| WlMode::from(*m).refresh;
        let pick = info.modes().iter().filter(|m| m.size() == size).min_by_key(|m| match want.and_then(|w| w.2) {
            Some(hz) => (refresh(m) - hz).abs(),
            None => -refresh(m),
        });
        if let Some(m) = pick {
            drm_mode = *m;
        }
        let wl_mode = WlMode::from(drm_mode);
        let (pw, ph) = info.size().unwrap_or((0, 0));
        let output = Output::new(
            name.clone(),
            PhysicalProperties { size: (pw as i32, ph as i32).into(), subpixel: info.subpixel().into(), make: "Unknown".into(), model: "Unknown".into() },
        );
        let global = output.create_global::<MistComp>(dh);
        let pos = conf.as_ref().map(|c| c.pos).unwrap_or_else(|| {
            let x = state.space.outputs().filter_map(|o| state.space.output_geometry(o)).map(|g| g.loc.x + g.size.w).max().unwrap_or(0);
            (x, 0)
        });
        for m in info.modes() {
            output.add_mode(WlMode::from(*m));
        }
        output.set_preferred(WlMode::from(info.modes()[mode_i]));
        let scale = conf.as_ref().and_then(|c| c.scale).map(smithay::output::Scale::Fractional);
        let transform = conf.as_ref().and_then(|c| c.transform).unwrap_or(Transform::Normal);
        output.change_current_state(Some(wl_mode), Some(transform), scale, Some(pos.into()));
        state.space.map_output(&output, pos);

        let drm = udev.mgr.device();
        let mut planes = drm.planes(&crtc).ok();
        if udev.nvidia {
            if let Some(p) = planes.as_mut() {
                p.overlay.clear();
            }
        }
        let drm_output = match udev.mgr.initialize_output::<_, OutElem>(
            crtc,
            drm_mode,
            &[info.handle()],
            &output,
            planes,
            &mut udev.renderer,
            &DrmOutputRenderElements::default(),
        ) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("output {name}: {e}");
                state.space.unmap_output(&output);
                dh.remove_global::<MistComp>(global);
                continue;
            }
        };
        tracing::info!("output {name} {}x{}@{} at {pos:?}", wl_mode.size.w, wl_mode.size.h, wl_mode.refresh);
        events.push(("monitoraddedv2", format!("0,{name},{}", output.description())));
        udev.surfaces.insert(crtc, Surf { output, drm_output, global, pending: false, vrr: Some(false), modes: info.modes().to_vec() });
        if udev.night {
            udev.apply_gamma(crtc);
        }
    }
    for o in state.space.outputs() {
        layer_map_for_output(o).arrange();
    }
    state.fix_ws();
    state.dirty = true;
    let changed = !events.is_empty();
    for (e, d) in events {
        state.hypr_event(e, &d);
    }
    if changed {
        state.outman_changed();
    }
}

fn conn_name(info: &connector::Info) -> String {
    format!("{}-{}", info.interface().as_str(), info.interface_id())
}

/// `~/.config/mist/outputs`: lines `NAME X Y` (e.g. `DP-3 0 0`, `HDMI-A-1 1920 0`).
fn vblank(state: &mut MistComp, crtc: crtc::Handle) {
    let Some(udev) = state.udev.as_mut() else { return };
    let Some(s) = udev.surfaces.get_mut(&crtc) else { return };
    if let Err(e) = s.drm_output.frame_submitted() {
        tracing::warn!("frame_submitted: {e:?}");
    }
    s.pending = false;
    state.fifo_signal();
    if state.redraw_after_vblank {
        render_all(state);
    }
}

/// The pointer's elements on an output (`og`: its geometry), empty when the pointer is elsewhere or hidden.
/// Shared by the screen and screen casting (overlay_cursor).
pub fn pointer_elements(
    renderer: &mut GlesRenderer,
    cursors: &mut Cursors,
    cursor_status: &mut CursorImageStatus,
    ptr: smithay::utils::Point<f64, smithay::utils::Logical>,
    og: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    scale: Scale<f64>,
) -> Vec<OutElem> {
    let mut elements: Vec<OutElem> = Vec::new();
    if !og.to_f64().contains(ptr) {
        return elements;
    }
    let at = ptr - og.loc.to_f64();
    if let CursorImageStatus::Surface(ref surf) = *cursor_status {
        if !surf.alive() {
            *cursor_status = CursorImageStatus::default_named();
        }
    }
    match cursor_status {
        CursorImageStatus::Hidden => {}
        CursorImageStatus::Named(icon) => {
            if let Some((buf, hot)) = cursors.get_icon(*icon) {
                let loc = (at - hot.to_f64()).to_physical(scale);
                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(renderer, loc, &buf, None, None, None, Kind::Cursor) {
                    elements.push(OutElem::Pointer(e));
                }
            }
        }
        CursorImageStatus::Surface(surf) => {
            let hot = with_states(surf, |st| {
                st.data_map.get::<std::sync::Mutex<CursorImageAttributes>>().map(|a| a.lock().unwrap().hotspot).unwrap_or_default()
            });
            let loc = (at - hot.to_f64()).to_physical(scale).to_i32_round();
            elements.extend(render_elements_from_surface_tree(renderer, surf, loc, scale, 1.0, Kind::Cursor).into_iter().map(OutElem::CursorSurface));
        }
    }
    elements
}

/// Render every output that changed and is not waiting for a vblank.
pub fn render_all(state: &mut MistComp) {
    state.redraw_after_vblank = false;
    let Some(udev) = state.udev.as_mut() else { return };
    if !udev.session.is_active() || udev.dpms_off {
        return;
    }
    let crtcs: Vec<crtc::Handle> = udev.surfaces.keys().copied().collect();
    for crtc in crtcs {
        render_one(state, crtc);
    }
    state.force_full = false;
}

fn render_one(state: &mut MistComp, crtc: crtc::Handle) {
    // Hyprland misc.vrr = 3: adaptive sync only while a fullscreen window (game / video) is on the output
    let out = state.udev.as_ref().and_then(|u| u.surfaces.get(&crtc)).map(|s| s.output.clone());
    let vrr_want = state.config.vrr && out.is_some_and(|o| state.fullscreen_on(&o));
    let MistComp { udev, space, seat, cursor_status, start_time, force_full, redraw_after_vblank, locked, lock, frames, anim, out_seq, .. } = state;
    let Some(udev) = udev.as_mut() else { return };
    let Some(s) = udev.surfaces.get_mut(&crtc) else { return };
    if s.pending {
        *redraw_after_vblank = true;
        return;
    }
    let output = s.output.clone();
    let Some(og) = space.output_geometry(&output) else { return };
    let scale = Scale::from(output.current_scale().fractional_scale());

    // pointer on top
    let ptr = seat.get_pointer().unwrap().current_location();
    let mut elements = pointer_elements(&mut udev.renderer, &mut udev.cursors, cursor_status, ptr, og, scale);
    let scene = crate::scene::elements(&mut udev.renderer, space, lock.surfaces.get(&output.name()), *locked, &output, anim);
    elements.extend(scene.into_iter().map(OutElem::Scene));

    if s.vrr.is_some_and(|v| v != vrr_want) {
        s.vrr = match s.drm_output.with_compositor(|c| c.use_vrr(vrr_want)) {
            Ok(()) => {
                tracing::info!("vrr {}: {vrr_want}", output.name());
                Some(vrr_want)
            }
            Err(e) => {
                tracing::warn!("vrr {}: {e:?}, off for good", output.name());
                None
            }
        };
    }
    if *force_full {
        s.drm_output.reset_buffers();
    }
    let rendered = match s.drm_output.render_frame(&mut udev.renderer, &elements, [0.0, 0.0, 0.0, 1.0], FrameFlags::DEFAULT) {
        Ok(r) => !r.is_empty,
        Err(e) => {
            tracing::warn!("render {}: {e:?}", output.name());
            false
        }
    };
    if rendered {
        *out_seq.entry(output.name()).or_insert(0) += 1;
        match s.drm_output.queue_frame(()) {
            Ok(()) => {
                s.pending = true;
                *frames += 1;
            }
            Err(e) => tracing::warn!("queue {}: {e:?}", output.name()),
        }
    }

    // frame callbacks for what is on this output
    let now = start_time.elapsed();
    for w in space.elements_for_output(&output) {
        w.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
    for l in layer_map_for_output(&output).layers() {
        l.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
    if let Some(l) = lock.surfaces.get(&output.name()) {
        smithay::desktop::utils::send_frames_surface_tree(l.wl_surface(), &output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
}


/// `mist-comp --probe`: what the DRM backend would light up (read-only, no master, no modeset).
pub fn probe() -> Result<(), Box<dyn std::error::Error>> {
    use smithay::reexports::drm::control::Device as _;
    struct Card(std::fs::File);
    impl std::os::fd::AsFd for Card {
        fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
            self.0.as_fd()
        }
    }
    impl smithay::reexports::drm::Device for Card {}
    impl smithay::reexports::drm::control::Device for Card {}
    let path: PathBuf = match std::env::var("MIST_COMP_DRM_DEVICE") {
        Ok(p) => p.into(),
        Err(_) => primary_gpu("seat0")?.ok_or("no GPU")?,
    };
    let card = Card(std::fs::OpenOptions::new().read(true).write(true).open(&path)?);
    let drv = smithay::reexports::drm::Device::get_driver(&card)?;
    println!("{} driver={}", path.display(), drv.name().to_string_lossy());
    let res = card.resource_handles()?;
    let layout = crate::config::outputs(&crate::config::load());
    for &c in res.connectors() {
        let info = card.get_connector(c, false)?;
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        let pref = info.modes().iter().position(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)).unwrap_or(0);
        let p = info.modes()[pref];
        let best = info.modes().iter().filter(|m| m.size() == p.size()).max_by_key(|m| m.vrefresh()).unwrap();
        let crtcs: Vec<_> = info
            .encoders()
            .iter()
            .filter_map(|e| card.get_encoder(*e).ok())
            .flat_map(|e| res.filter_crtcs(e.possible_crtcs()))
            .collect();
        println!(
            "{} preferred {}x{}@{} -> {}x{}@{} crtcs {} pos {:?}",
            conn_name(&info),
            p.size().0, p.size().1, p.vrefresh(),
            best.size().0, best.size().1, best.vrefresh(),
            crtcs.len(),
            layout.iter().find(|o| o.name == conn_name(&info))
        );
    }
    Ok(())
}
