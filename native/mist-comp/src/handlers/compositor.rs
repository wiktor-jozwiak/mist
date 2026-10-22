use smithay::wayland::seat::WaylandFocus as _;
use crate::{grabs::resize_grab, state::ClientState, MistComp};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states, BufferAssignment,
            CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes,
        },
        dmabuf::get_dmabuf,
        drm_syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState},
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;
use smithay::reexports::{
    calloop::{
        timer::{TimeoutAction, Timer},
        Interest,
    },
    wayland_server::Resource,
};
use std::time::Duration;

impl CompositorHandler for MistComp {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(x) = client.get_data::<smithay::xwayland::XWaylandClientData>() {
            return &x.compositor_state;
        }
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    // a buffer is used only once the client's GPU work on it is done: the explicit-sync acquire point
    // (linux-drm-syncobj, NVIDIA) or the dmabuf's implicit fence; until then the commit waits, not the screen
    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, |state, _dh, surface| {
            let mut acquire = None;
            let dmabuf = with_states(surface, |st| {
                acquire.clone_from(&st.cached_state.get::<DrmSyncobjCachedState>().pending().acquire_point);
                st.cached_state.get::<SurfaceAttributes>().pending().buffer.as_ref().and_then(|b| match b {
                    BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                    _ => None,
                })
            });
            let Some(dmabuf) = dmabuf else { return };
            let Some(client) = surface.client() else { return };
            let cleared = move |data: &mut MistComp| {
                let dh = data.display_handle.clone();
                data.client_compositor_state(&client).blocker_cleared(data, &dh);
            };
            if let Some((blocker, source)) = acquire.and_then(|p| p.generate_blocker().ok()) {
                let c = cleared.clone();
                // a GPU fence that never signals freezes the window: watch_fences says so in the log
                let done = std::rc::Rc::new(std::cell::Cell::new(false));
                let d = done.clone();
                if state.loop_handle.insert_source(source, move |_, _, data| { d.set(true); c(data); Ok(()) }).is_ok() {
                    add_blocker(surface, blocker);
                    state.fence_waits.push((surface.downgrade(), std::time::Instant::now(), done));
                    state.watch_fences();
                    return;
                }
            }
            if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                if state.loop_handle.insert_source(source, move |_, _, data| { cleared(data); Ok(()) }).is_ok() {
                    add_blocker(surface, blocker);
                }
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.commits += 1;
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.wl_surface().as_deref() == Some(&root))
            {
                window.on_commit();
            }
        };

        // the drag icon moves by what the client offsets it (wl_surface.offset / attach x, y)
        if let Some((icon, off)) = self.dnd_icon.as_mut().filter(|(i, _)| i == surface) {
            let _ = icon;
            *off += with_states(surface, |s| s.cached_state.get::<SurfaceAttributes>().current().buffer_delta.take().unwrap_or_default());
        }
        self.pending_commit(surface);
        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
        resize_grab::handle_commit(&mut self.space, surface);
        super::layer_shell::handle_commit(self, surface);
        self.dirty = true;
    }
}

impl BufferHandler for MistComp {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for MistComp {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(MistComp);

impl DrmSyncobjHandler for MistComp {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        self.syncobj.as_mut()
    }
}
smithay::delegate_drm_syncobj!(MistComp);
delegate_shm!(MistComp);

impl MistComp {
    /// Who a surface belongs to, for the log: the window's app id, the layer's namespace.
    pub fn surface_name(&self, surface: &WlSurface) -> String {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if let Some(w) = self.space.elements().find(|w| w.wl_surface().as_deref() == Some(&root)) {
            return format!("window {}", crate::hyprcompat::app_id(w));
        }
        for o in self.space.outputs() {
            if let Some(l) = smithay::desktop::layer_map_for_output(o).layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL) {
                return format!("layer {}", l.namespace());
            }
        }
        "a surface".into()
    }
}

/// Explicit-sync commits waiting for their GPU fence: (surface, since, signalled).
pub type FenceWaits = Vec<(smithay::reexports::wayland_server::Weak<WlSurface>, std::time::Instant, std::rc::Rc<std::cell::Cell<bool>>)>;

impl MistComp {
    /// Once a second while commits wait for a GPU fence: one that waits over a second is a window frozen by
    /// the driver, logged once (`explicit_sync off` in comp.conf works around a driver that loses fences).
    pub fn watch_fences(&mut self) {
        if self.fence_timer {
            return;
        }
        self.fence_timer = true;
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(1)), |_, _, data: &mut MistComp| {
            let mut stuck = Vec::new();
            data.fence_waits.retain(|(s, since, done)| {
                if done.get() {
                    return false;
                }
                if since.elapsed() < Duration::from_secs(1) {
                    return true;
                }
                if let Ok(s) = s.upgrade() {
                    stuck.push((s, since.elapsed()));
                }
                false
            });
            for (s, t) in stuck {
                tracing::warn!("{}: a frame waited {:.1} s for its GPU fence (explicit sync); still waiting", data.surface_name(&s), t.as_secs_f32());
            }
            if data.fence_waits.is_empty() {
                data.fence_timer = false;
                TimeoutAction::Drop
            } else {
                TimeoutAction::ToDuration(Duration::from_secs(1))
            }
        });
    }
}
