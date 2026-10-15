//! Small protocols clients expect from a desktop compositor (mostly plain delegates).
use smithay::{
    delegate_cursor_shape, delegate_data_control, delegate_fractional_scale, delegate_keyboard_shortcuts_inhibit,
    delegate_pointer_constraints, delegate_pointer_gestures, delegate_presentation, delegate_primary_selection,
    delegate_relative_pointer, delegate_single_pixel_buffer, delegate_viewporter, delegate_xdg_activation,
    delegate_xdg_decoration,
    input::{pointer::PointerHandle, Seat},
    reexports::{
        wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
        wayland_server::{protocol::wl_surface::WlSurface, DisplayHandle},
    },
    utils::{Logical, Point},
    wayland::{
        compositor::with_states,
        cursor_shape::CursorShapeManagerState,
        fractional_scale::{with_fractional_scale, FractionalScaleHandler, FractionalScaleManagerState},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
        },
        pointer_constraints::{with_pointer_constraint, PointerConstraintsHandler, PointerConstraintsState},
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        selection::{
            primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
            wlr_data_control::{DataControlHandler, DataControlState},
        },
        shell::xdg::{
            decoration::{XdgDecorationHandler, XdgDecorationState},
            ToplevelSurface,
        },
        single_pixel_buffer::SinglePixelBufferState,
        viewporter::ViewporterState,
        xdg_activation::{XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData},
    },
};

use crate::MistComp;

pub struct Protocols {
    pub primary_selection: PrimarySelectionState,
    pub data_control: DataControlState,
    pub xdg_activation: XdgActivationState,
    pub xdg_decoration: XdgDecorationState,
    pub shortcuts_inhibit: KeyboardShortcutsInhibitState,
    _viewporter: ViewporterState,
    _presentation: PresentationState,
    _fractional: FractionalScaleManagerState,
    _single_pixel: SinglePixelBufferState,
    _relative: RelativePointerManagerState,
    _constraints: PointerConstraintsState,
    _gestures: PointerGesturesState,
    _cursor_shape: CursorShapeManagerState,
}

impl Protocols {
    /// `primary`: middle-click paste (Hyprland misc.middle_click_paste); off = no primary selection global
    pub fn new(dh: &DisplayHandle, primary: bool) -> Self {
        let primary_selection = PrimarySelectionState::new_with_filter::<MistComp, _>(dh, move |_| primary);
        Self {
            data_control: DataControlState::new::<MistComp, _>(dh, primary.then_some(&primary_selection), |_| true),
            primary_selection,
            xdg_activation: XdgActivationState::new::<MistComp>(dh),
            xdg_decoration: XdgDecorationState::new::<MistComp>(dh),
            shortcuts_inhibit: KeyboardShortcutsInhibitState::new::<MistComp>(dh),
            _viewporter: ViewporterState::new::<MistComp>(dh),
            // CLOCK_MONOTONIC
            _presentation: PresentationState::new::<MistComp>(dh, 1),
            _fractional: FractionalScaleManagerState::new::<MistComp>(dh),
            _single_pixel: SinglePixelBufferState::new::<MistComp>(dh),
            _relative: RelativePointerManagerState::new::<MistComp>(dh),
            _constraints: PointerConstraintsState::new::<MistComp>(dh),
            _gestures: PointerGesturesState::new::<MistComp>(dh),
            _cursor_shape: CursorShapeManagerState::new::<MistComp>(dh),
        }
    }
}

impl PrimarySelectionHandler for MistComp {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.protocols.primary_selection
    }
}
delegate_primary_selection!(MistComp);

impl DataControlHandler for MistComp {
    fn data_control_state(&self) -> &DataControlState {
        &self.protocols.data_control
    }
}
delegate_data_control!(MistComp);

delegate_viewporter!(MistComp);
delegate_presentation!(MistComp);
delegate_single_pixel_buffer!(MistComp);
delegate_relative_pointer!(MistComp);
delegate_pointer_gestures!(MistComp);
// cursor-shape needs the tablet seat hooks (defaults: the shape lands in cursor_image)
impl smithay::wayland::tablet_manager::TabletSeatHandler for MistComp {}
delegate_cursor_shape!(MistComp);

impl FractionalScaleHandler for MistComp {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.space.outputs().next().map(|o| o.current_scale().fractional_scale()).unwrap_or(1.0);
        with_states(&surface, |states| with_fractional_scale(states, |f| f.set_preferred_scale(scale)));
    }
}
delegate_fractional_scale!(MistComp);

impl KeyboardShortcutsInhibitHandler for MistComp {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.protocols.shortcuts_inhibit
    }
    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // games / VMs / remote desktops get all keys
        inhibitor.activate();
    }
}
delegate_keyboard_shortcuts_inhibit!(MistComp);

impl PointerConstraintsHandler for MistComp {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        if pointer.current_focus().as_ref() == Some(surface) {
            with_pointer_constraint(surface, pointer, |c| c.unwrap().activate());
        }
    }
    fn cursor_position_hint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>, location: Point<f64, Logical>) {
        if with_pointer_constraint(surface, pointer, |c| c.is_some_and(|c| c.is_active())) {
            let origin = self
                .space
                .elements()
                .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
                .and_then(|w| self.space.element_location(w))
                .unwrap_or_default()
                .to_f64();
            pointer.set_location(origin + location);
        }
    }
}
delegate_pointer_constraints!(MistComp);

impl XdgActivationHandler for MistComp {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.protocols.xdg_activation
    }
    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let Some((serial, seat)) = data.serial else { return false };
        let keyboard = self.seat.get_keyboard().unwrap();
        Seat::from_resource(&seat) == Some(self.seat.clone())
            && keyboard.last_enter().is_some_and(|last| serial.is_no_older_than(&last))
    }
    fn request_activation(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData, surface: WlSurface) {
        if data.timestamp.elapsed().as_secs() < 10 {
            let w = self.space.elements().find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &surface)).cloned();
            if let Some(w) = w {
                self.space.raise_element(&w, true);
                self.dirty = true;
            }
        }
    }
}
delegate_xdg_activation!(MistComp);

impl XdgDecorationHandler for MistComp {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // Mist draws the macOS bar (decor.rs) unless the client asks to draw its own
        toplevel.with_pending_state(|s| s.decoration_mode = Some(DecorationMode::ServerSide));
        self.dirty = true;
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        // like Hyprland + hyprbars: every window that negotiates gets the Mist bar (kitty asks client-side)
        toplevel.with_pending_state(|s| s.decoration_mode = Some(DecorationMode::ServerSide));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        self.dirty = true;
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.request_mode(toplevel, DecorationMode::ServerSide);
    }
}
delegate_xdg_decoration!(MistComp);
