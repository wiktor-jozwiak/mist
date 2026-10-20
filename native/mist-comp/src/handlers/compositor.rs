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
use smithay::reexports::{calloop::Interest, wayland_server::Resource};

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
                if state.loop_handle.insert_source(source, move |_, _, data| { c(data); Ok(()) }).is_ok() {
                    add_blocker(surface, blocker);
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
