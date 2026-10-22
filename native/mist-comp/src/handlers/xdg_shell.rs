use smithay::wayland::seat::WaylandFocus as _;
use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab,
        PopupUngrabStrategy, Space, Window,
    },
    input::{
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    grabs::ResizeSurfaceGrab,
    MistComp,
};

impl XdgShellHandler for MistComp {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // joins the Space at its first buffer (rules.rs: size from the window rules, centred then)
        self.pending.push(Window::new_wayland_window(surface));
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.pending.retain(|w| w.toplevel() != Some(&surface));
        let gone = self
            .space
            .elements()
            .chain(self.stash.iter().map(|(w, _)| w))
            .find(|w| w.toplevel() == Some(&surface))
            .cloned();
        if let Some(w) = gone {
            self.drop_window(&w);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat = Seat::from_resource(&seat).unwrap();

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let window = self.space.elements().find(|w| w.wl_surface().as_deref() == Some(wl_surface)).cloned();
            if let Some(window) = window {
                self.start_move(window, start_data.button, serial);
            }
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat = Seat::from_resource(&seat).unwrap();

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat.get_pointer().unwrap();

            // only a window on screen (one on a hidden workspace or not mapped yet has no place to resize from)
            let Some(window) = self.space.elements().find(|w| w.wl_surface().as_deref() == Some(wl_surface)).cloned() else { return };
            let Some(initial_window_location) = self.space.element_location(&window) else { return };
            let initial_window_size = window.geometry().size;

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });

            surface.send_pending_configure();

            let grab = ResizeSurfaceGrab::start(
                start_data,
                window,
                edges.into(),
                Rectangle::new(initial_window_location, initial_window_size),
            );

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        let found = self.win_of(&surface);
        if let Some(w) = found {
            self.minimize(&w);
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if self.wish_at_map(surface.wl_surface(), Some(crate::rules::Wish::Max)) {
            return;
        }
        let found = self.win_of(&surface);
        if let Some(w) = found {
            // the client's own maximize (CSD double click, green dot): Mist's maximize (work area, gaps)
            if surface.is_initial_configure_sent() {
                self.mist_maximize(&w);
            }
        }
        if !surface.is_initial_configure_sent() {
            return;
        }
        surface.send_pending_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if self.wish_at_map(surface.wl_surface(), None) {
            return;
        }
        let found = self.win_of(&surface);
        if let Some(w) = found {
            if crate::wm::data(&w).restore.borrow().is_some() {
                self.toggle_max(&w, false);
            } else {
                self.snap_key_on(&w, crate::snap::Key::Unsnap);
            }
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>) {
        if self.wish_at_map(surface.wl_surface(), Some(crate::rules::Wish::Full)) {
            return;
        }
        let found = self.win_of(&surface);
        if let Some(w) = found {
            if crate::wm::data(&w).restore.borrow().is_none() {
                self.toggle_max(&w, true);
            }
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if self.wish_at_map(surface.wl_surface(), None) {
            return;
        }
        let found = self.win_of(&surface);
        if let Some(w) = found {
            if crate::wm::data(&w).restore.borrow().is_some() {
                self.toggle_max(&w, true);
            }
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_info_changed(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_info_changed(&surface);
    }

    /// A menu (right click, menu bar): keys and pointer stay with it and its children until a click lands
    /// outside them, which closes the whole chain (as on every desktop).
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::<MistComp>::from_resource(&seat) else { return };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else { return };
        let Ok(mut grab) = self.popups.grab_popup(crate::focus::KbFocus::Surface(root), kind, &seat, serial) else { return };
        let prev = grab.previous_serial().unwrap_or(serial);
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed() && !(keyboard.has_grab(serial) || keyboard.has_grab(prev)) {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed() && !(pointer.has_grab(serial) || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial()))) {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

// Xdg Shell
delegate_xdg_shell!(MistComp);

fn check_grab(
    seat: &Seat<MistComp>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<MistComp>> {
    let pointer = seat.get_pointer()?;

    // Check that this surface has a click grab.
    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    // If the focus was for a different surface, ignore the request.
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    // Handle toplevel commits.
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().unwrap().send_configure();
        }
    }

    // Handle popup commits.
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    // NOTE: This should never fail as the initial configure is always
                    // allowed.
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl MistComp {
    /// Keep a menu on the monitor its window (or panel) is on: flipped / slid inside that output, as the
    /// client's positioner allows.
    pub(crate) fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        // where the popup's coordinates start: the window's geometry origin, or the layer surface's corner
        let win = self.space.elements().find(|w| w.wl_surface().as_deref() == Some(&root));
        let (origin, output) = if let Some(w) = win {
            let Some(loc) = self.space.element_location(w) else { return };
            (loc, self.output_of(w))
        } else {
            let layer = self.space.outputs().find_map(|o| {
                let map = smithay::desktop::layer_map_for_output(o);
                let l = map.layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL)?;
                Some((o.clone(), map.layer_geometry(l)?.loc))
            });
            let Some((o, at)) = layer else { return };
            let Some(og) = self.space.output_geometry(&o) else { return };
            (og.loc + at, Some(o))
        };
        let Some(output_geo) = output.or_else(|| self.space.outputs().next().cloned()).and_then(|o| self.space.output_geometry(&o)) else {
            return;
        };

        // The target geometry for the positioner is relative to the popup's parent.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&kind);
        target.loc -= origin;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

impl MistComp {
    pub fn win_of(&self, surface: &ToplevelSurface) -> Option<Window> {
        self.space.elements().chain(self.stash.iter().map(|(w, _)| w)).find(|w| w.toplevel() == Some(surface)).cloned()
    }

    /// Title / app_id changed: tell IPC listeners (mist-ui re-reads the client list).
    fn toplevel_info_changed(&mut self, surface: &ToplevelSurface) {
        let w = self.space.elements().chain(self.stash.iter().map(|(w, _)| w)).find(|w| w.toplevel() == Some(surface)).cloned();
        if let Some(w) = w {
            let a = crate::wm::data(&w).addr.get();
            let t = crate::hyprcompat::title(&w);
            self.hypr_event("windowtitlev2", &format!("{a:x},{t}"));
            self.list_update(&w);
        }
    }
}
