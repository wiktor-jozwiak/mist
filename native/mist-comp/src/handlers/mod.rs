mod compositor;
pub mod layer_shell;
pub mod protocols;
mod xdg_shell;

use crate::MistComp;

//
// Wl Seat
//

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl SeatHandler for MistComp {
    type KeyboardFocus = crate::focus::KbFocus;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<MistComp> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        self.cursor_status = image;
        self.dirty = true;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&crate::focus::KbFocus>) {
        use smithay::wayland::seat::WaylandFocus;
        let dh = &self.display_handle;
        let client = focused.and_then(|f| f.wl_surface()).and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        smithay::wayland::selection::primary_selection::set_primary_focus(dh, seat, client);
    }
}

delegate_seat!(MistComp);

//
// Wl Data Device
//

impl SelectionHandler for MistComp {
    type SelectionUserData = ();

    // Wayland clipboard -> X11 clients
    fn new_selection(&mut self, ty: smithay::wayland::selection::SelectionTarget, source: Option<smithay::wayland::selection::SelectionSource>, _seat: Seat<Self>) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(e) = xwm.new_selection(ty, source.map(|s| s.mime_types())) {
                tracing::debug!("x11 selection: {e}");
            }
        }
    }

    fn send_selection(&mut self, ty: smithay::wayland::selection::SelectionTarget, mime: String, fd: std::os::fd::OwnedFd, _seat: Seat<Self>, _: &()) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(e) = xwm.send_selection(ty, mime, fd, self.loop_handle.clone()) {
                tracing::debug!("x11 send selection: {e}");
            }
        }
    }
}

impl DataDeviceHandler for MistComp {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for MistComp {}
impl ServerDndGrabHandler for MistComp {}

delegate_data_device!(MistComp);

//
// Wl Output & Xdg Output
//

impl OutputHandler for MistComp {}
delegate_output!(MistComp);
