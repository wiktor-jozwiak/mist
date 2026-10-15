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
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState,
        },
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;

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
delegate_shm!(MistComp);
