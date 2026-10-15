//! XWayland (Steam, games, old X11 apps): Smithay's X11Wm as the window manager. Windows become ordinary
//! `Window`s in the Space (workspaces, focus, Hyprland IPC, bars all apply); override-redirect windows
//! (menus, tooltips, splash) map where they ask, unmanaged. X clipboard <-> Wayland selection bridged.
//! Started at launch in a real session (MIST_COMP_XWAYLAND=0 skips it; nested/headless only with =1).
use std::os::fd::OwnedFd;

use smithay::{
    desktop::{layer_map_for_output, Window},
    utils::{Logical, Point, Rectangle},
    wayland::{
        selection::{
            data_device::{clear_data_device_selection, current_data_device_selection_userdata, request_data_device_client_selection, set_data_device_selection},
            primary_selection::{clear_primary_selection, current_primary_selection_userdata, request_primary_client_selection, set_primary_selection},
            SelectionTarget,
        },
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId},
        X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler,
    },
};

use crate::{focus::KbFocus, CalloopData, MistComp};

pub fn start(state: &mut MistComp, handle: &smithay::reexports::calloop::LoopHandle<'static, CalloopData>) {
    let spawned = XWayland::spawn(
        &state.display_handle,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        std::process::Stdio::null(),
        std::process::Stdio::null(),
        |_| (),
    );
    let (xwayland, client) = match spawned {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("xwayland: {e}");
            return;
        }
    };
    // the X sockets listen already: children started from now on (autostart too) get DISPLAY
    std::env::set_var("DISPLAY", format!(":{}", xwayland.display_number()));
    println!("DISPLAY=:{}", xwayland.display_number());
    let h = handle.clone();
    let r = handle.insert_source(xwayland, move |event, _, data: &mut CalloopData| match event {
        XWaylandEvent::Ready { x11_socket, display_number } => {
            match X11Wm::start_wm(h.clone(), x11_socket, client.clone()) {
                Ok(wm) => {
                    data.xwm = Some(wm);
                    tracing::info!("xwayland ready on :{display_number}");
                }
                Err(e) => tracing::warn!("xwm: {e}"),
            }
        }
        XWaylandEvent::Error => tracing::warn!("xwayland failed to start"),
    });
    if let Err(e) = r {
        tracing::warn!("xwayland source: {e}");
    }
}

impl MistComp {
    fn x11_window(&self, x: &X11Surface) -> Option<Window> {
        self.space
            .elements()
            .chain(self.stash.iter().map(|(w, _)| w))
            .find(|w| w.x11_surface() == Some(x))
            .cloned()
    }
}

impl XWaylandShellHandler for MistComp {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell
    }
}
smithay::delegate_xwayland_shell!(MistComp);

impl XwmHandler for MistComp {
    fn xwm_state(&mut self, _: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().unwrap()
    }

    fn new_window(&mut self, _: XwmId, _: X11Surface) {}
    fn new_override_redirect_window(&mut self, _: XwmId, _: X11Surface) {}

    fn map_window_request(&mut self, _: XwmId, x: X11Surface) {
        if x.set_mapped(true).is_err() {
            return;
        }
        // its own size (or the rule's), centred in the usable area of the rule's output (games: fullscreen)
        let w = Window::new_x11_window(x.clone());
        let r = self.rule_of(&w);
        let mut geo = x.geometry();
        if let Some(o) = self.rule_output(&r) {
            let og = self.space.output_geometry(&o).unwrap_or_default();
            let z = layer_map_for_output(&o).non_exclusive_zone();
            let remembered = self.sizes.get(&crate::hyprcompat::app_id(&w).to_lowercase()).map(|s| (s.w, s.h));
            if let Some((sw, sh)) = r.size.map(|s| s.on(og)).or(remembered) {
                geo.size = (sw, sh).into();
            } else if geo.size.w <= 1 || geo.size.h <= 1 {
                geo.size = (z.size.w * 2 / 3, z.size.h * 2 / 3).into();
            }
            geo.size.w = geo.size.w.min(z.size.w);
            geo.size.h = geo.size.h.min(z.size.h);
            geo.loc = og.loc + z.loc + Point::from(((z.size.w - geo.size.w) / 2, (z.size.h - geo.size.h) / 2));
        }
        let _ = x.configure(geo);
        self.adopt_ruled(w, geo.loc, r);
    }

    fn mapped_override_redirect_window(&mut self, _: XwmId, x: X11Surface) {
        let loc = x.geometry().loc;
        let w = Window::new_x11_window(x);
        crate::wm::data(&w).addr.set(0);
        self.space.map_element(w, loc, false);
        self.dirty = true;
    }

    fn unmapped_window(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            if x.is_override_redirect() {
                self.space.unmap_elem(&w);
                self.dirty = true;
            } else {
                self.drop_window(&w);
            }
        }
        if !x.is_override_redirect() {
            let _ = x.set_mapped(false);
        }
    }

    fn destroyed_window(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            self.drop_window(&w);
        }
    }

    fn configure_request(&mut self, _: XwmId, x: X11Surface, nx: Option<i32>, ny: Option<i32>, w: Option<u32>, h: Option<u32>, _: Option<Reorder>) {
        let mut geo = x.geometry();
        let managed = self.x11_window(&x).is_some_and(|w| self.space.element_location(&w).is_some());
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        // unmapped (still placing itself) may move; mapped windows are placed by Mist
        if !managed {
            if let Some(nx) = nx {
                geo.loc.x = nx;
            }
            if let Some(ny) = ny {
                geo.loc.y = ny;
            }
        }
        let _ = x.configure(geo);
    }

    fn configure_notify(&mut self, _: XwmId, x: X11Surface, geometry: Rectangle<i32, Logical>, _: Option<u32>) {
        let Some(w) = self.x11_window(&x) else { return };
        if self.space.element_location(&w).is_some() && x.is_override_redirect() {
            self.space.map_element(w, geometry.loc, false);
        }
        self.dirty = true;
    }

    fn maximize_request(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            self.mist_maximize(&w);
        }
    }

    fn unmaximize_request(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            self.snap_key_on(&w, crate::snap::Key::Unsnap);
        }
    }

    fn fullscreen_request(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            if !x.is_fullscreen() {
                self.toggle_max(&w, true);
            }
        }
    }

    fn unfullscreen_request(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            if x.is_fullscreen() {
                self.toggle_max(&w, true);
            }
        }
    }

    fn minimize_request(&mut self, _: XwmId, x: X11Surface) {
        if let Some(w) = self.x11_window(&x) {
            self.minimize(&w);
        }
    }

    fn resize_request(&mut self, _: XwmId, _: X11Surface, _: u32, _: X11ResizeEdge) {
        // Super + right drag resizes any window; client-initiated X11 resize is not supported
    }

    fn move_request(&mut self, _: XwmId, x: X11Surface, button: u32) {
        let Some(w) = self.x11_window(&x) else { return };
        self.start_move(w, button, smithay::utils::SERIAL_COUNTER.next_serial());
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _: SelectionTarget) -> bool {
        // only the focused X11 client reads the clipboard
        matches!(self.seat.get_keyboard().and_then(|k| k.current_focus()), Some(KbFocus::X11(x)) if x.xwm_id() == Some(xwm))
    }

    fn send_selection(&mut self, _: XwmId, sel: SelectionTarget, mime: String, fd: OwnedFd) {
        let r = match sel {
            SelectionTarget::Clipboard => request_data_device_client_selection(&self.seat, mime, fd).map_err(|e| e.to_string()),
            SelectionTarget::Primary => request_primary_client_selection(&self.seat, mime, fd).map_err(|e| e.to_string()),
        };
        if let Err(e) = r {
            tracing::debug!("x11 selection request: {e}");
        }
    }

    fn new_selection(&mut self, _: XwmId, sel: SelectionTarget, mimes: Vec<String>) {
        match sel {
            SelectionTarget::Clipboard => set_data_device_selection(&self.display_handle, &self.seat, mimes, ()),
            SelectionTarget::Primary => set_primary_selection(&self.display_handle, &self.seat, mimes, ()),
        }
    }

    fn cleared_selection(&mut self, _: XwmId, sel: SelectionTarget) {
        match sel {
            SelectionTarget::Clipboard => {
                if current_data_device_selection_userdata(&self.seat).is_some() {
                    clear_data_device_selection(&self.display_handle, &self.seat)
                }
            }
            SelectionTarget::Primary => {
                if current_primary_selection_userdata(&self.seat).is_some() {
                    clear_primary_selection(&self.display_handle, &self.seat)
                }
            }
        }
    }
}
