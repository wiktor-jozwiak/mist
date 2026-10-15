use std::{cell::RefCell, rc::Rc, time::Duration};

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, gles::GlesRenderer,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    desktop::layer_map_for_output,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
};

use smithay::{
    backend::{allocator::dmabuf::Dmabuf, egl::EGLDevice, renderer::ImportDma},
    delegate_dmabuf,
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
};

use crate::{CalloopData, MistComp};

pub type Gles = Rc<RefCell<WinitGraphicsBackend<GlesRenderer>>>;

impl DmabufHandler for MistComp {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.as_mut().unwrap().0
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let ok = match (self.udev.as_mut(), self.gles.as_ref()) {
            _ if self.headless.is_some() => self.headless.as_mut().unwrap().import_dmabuf(&dmabuf),
            (Some(u), _) => u.import_dmabuf(&dmabuf),
            (None, Some(b)) => b.borrow_mut().renderer().import_dmabuf(&dmabuf, None).is_ok(),
            _ => false,
        };
        if ok {
            let _ = notifier.successful::<MistComp>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(MistComp);

pub fn init_winit(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<Gles, Box<dyn std::error::Error>> {
    let display_handle = &data.display_handle.clone();
    let state = data;

    let (backend, winit) = winit::init::<GlesRenderer>()?;
    let backend = Rc::new(RefCell::new(backend));
    let mut swap_interval_set = false;
    let ret = backend.clone();

    // linux-dmabuf v4 (feedback names the render node): GPU clients render on the GPU instead of Mesa llvmpipe
    {
        let mut b = backend.borrow_mut();
        let formats = b.renderer().dmabuf_formats();
        let node = EGLDevice::device_for_display(b.renderer().egl_context().display())
            .ok()
            .and_then(|d| d.try_get_render_node().ok().flatten());
        let mut dmabuf_state = DmabufState::new();
        let global = match node.and_then(|n| DmabufFeedbackBuilder::new(n.dev_id(), formats.clone()).build().ok()) {
            Some(fb) => dmabuf_state.create_global_with_default_feedback::<MistComp>(display_handle, &fb),
            None => {
                tracing::warn!("no render node, dmabuf v3");
                dmabuf_state.create_global::<MistComp>(display_handle, formats)
            }
        };
        state.dmabuf = Some((dmabuf_state, global));
        state.gles = Some(backend.clone());
    }

    let mode = Mode {
        size: backend.borrow().window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        std::env::var("MIST_COMP_OUTPUT").unwrap_or_else(|_| "winit".into()),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<MistComp>(display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    println!("WAYLAND_DISPLAY={}", state.socket_name.to_string_lossy());

    event_loop.handle().insert_source(winit, move |event, _, data| {
        let mut display = data.display_handle.clone();
        let state = data;

        match event {
            WinitEvent::Resized { size, .. } => {
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                layer_map_for_output(&output).arrange();
                state.dirty = true;
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Focus(_) => state.dirty = true,
            WinitEvent::Redraw => {
                state.frames += 1;
                // damage-driven: only commits/input/resize mark the state dirty and request a frame
                tracing::trace!("redraw start");
                let mut backend = backend.borrow_mut();
                let size = backend.window_size();
                let age = if std::mem::take(&mut state.force_full) { 0 } else { backend.buffer_age().unwrap_or(0) };
                let shot = state.shot.take();
                let damage = {
                    let (renderer, mut framebuffer) = backend.bind().unwrap();
                    if !swap_interval_set {
                        // the surface is current now: no vsync wait in eglSwapBuffers (the host stops frame
                        // callbacks while our window is hidden, which would block the whole loop)
                        let dpy = renderer.egl_context().display().get_display_handle();
                        let ok = unsafe { smithay::backend::egl::ffi::egl::SwapInterval(dpy.handle, 0) };
                        tracing::debug!("swap interval 0: {ok}");
                        swap_interval_set = true;
                    }
                    let elems = crate::scene::elements(renderer, &state.space, state.lock.surfaces.get(&output.name()), state.locked, &output, &state.anim);
                    let clear = if state.locked { [0.0, 0.0, 0.0, 1.0] } else { [0.1, 0.1, 0.1, 1.0] };
                    let damage = damage_tracker.render_output(renderer, &mut framebuffer, age, &elems, clear).unwrap().damage.cloned();
                    if let Some((path, mut reply)) = shot {
                        use std::io::Write;
                        let r = write_shot(renderer, &framebuffer, size, &path, true);
                        let _ = reply.write_all(match r { Ok(()) => "ok\n".to_string(), Err(e) => format!("error {e}\n") }.as_bytes());
                    }
                    damage
                };
                tracing::trace!("rendered, submit {}", damage.is_some());
                if let Some(damage) = damage {
                    // NVIDIA nested: swap can fail (BadAlloc) while the host hides the window; skip the frame
                    if let Err(e) = backend.submit(Some(&damage)) {
                        tracing::warn!("submit: {e:?}");
                        state.force_full = true;
                    }
                }

                let now = state.start_time.elapsed();
                state.space.elements().for_each(|window| {
                    window.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()))
                });
                for layer in layer_map_for_output(&output).layers() {
                    layer.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
                }
                if let Some(l) = state.lock.surfaces.get(&output.name()) {
                    smithay::desktop::utils::send_frames_surface_tree(l.wl_surface(), &output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
                }

                state.space.refresh();
                state.popups.cleanup();
                let _ = display.flush_clients();
            }
            WinitEvent::CloseRequested => {
                state.loop_signal.stop();
            }
        };
    })?;

    Ok(ret)
}

/// Read the framebuffer back into a binary PPM (test hook for `shot`).
pub fn write_shot(
    renderer: &mut GlesRenderer,
    fb: &<GlesRenderer as smithay::backend::renderer::RendererSuper>::Framebuffer<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    path: &std::path::Path,
    flip: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use smithay::backend::{allocator::Fourcc, renderer::ExportMem};
    let region = Rectangle::from_size((size.w, size.h).into());
    let map = renderer.copy_framebuffer(fb, region, Fourcc::Abgr8888)?;
    let px = renderer.map_texture(&map)?;
    let (w, h) = (size.w as usize, size.h as usize);
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    out.reserve(w * h * 3);
    // the window's default framebuffer reads bottom-up, offscreen buffers top-down
    let rows: Box<dyn Iterator<Item = &[u8]>> =
        if flip { Box::new(px.chunks_exact(w * 4).rev()) } else { Box::new(px.chunks_exact(w * 4)) };
    for row in rows {
        for p in row.chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
    }
    std::fs::write(path, out)?;
    Ok(())
}
