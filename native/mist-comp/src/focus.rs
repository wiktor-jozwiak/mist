//! Keyboard focus target: a Wayland surface (toplevel, layer, lock) or an X11 window, which also needs the
//! WM to set X input focus / send WM_TAKE_FOCUS (X11Surface's KeyboardTarget does both, then forwards to
//! its wl_surface).
use std::borrow::Cow;

use smithay::{
    backend::input::KeyState,
    input::{
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
        Seat,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
    xwayland::X11Surface,
};

use crate::MistComp;

#[derive(Clone, PartialEq, Debug)]
pub enum KbFocus {
    Surface(WlSurface),
    X11(X11Surface),
}

impl From<WlSurface> for KbFocus {
    fn from(s: WlSurface) -> Self {
        KbFocus::Surface(s)
    }
}

impl KbFocus {
    /// Is this focus that surface (an X11 window: its wl_surface)?
    pub fn is(&self, s: &WlSurface) -> bool {
        self.wl_surface().is_some_and(|w| &*w == s)
    }
}

impl IsAlive for KbFocus {
    fn alive(&self) -> bool {
        match self {
            KbFocus::Surface(s) => s.alive(),
            KbFocus::X11(x) => x.alive(),
        }
    }
}

impl WaylandFocus for KbFocus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            KbFocus::Surface(s) => Some(Cow::Borrowed(s)),
            KbFocus::X11(x) => x.wl_surface().map(Cow::Owned),
        }
    }
}

impl KeyboardTarget<MistComp> for KbFocus {
    fn enter(&self, seat: &Seat<MistComp>, data: &mut MistComp, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
        match self {
            KbFocus::Surface(s) => KeyboardTarget::enter(s, seat, data, keys, serial),
            KbFocus::X11(x) => KeyboardTarget::enter(x, seat, data, keys, serial),
        }
    }
    fn leave(&self, seat: &Seat<MistComp>, data: &mut MistComp, serial: Serial) {
        match self {
            KbFocus::Surface(s) => KeyboardTarget::leave(s, seat, data, serial),
            KbFocus::X11(x) => KeyboardTarget::leave(x, seat, data, serial),
        }
    }
    fn key(&self, seat: &Seat<MistComp>, data: &mut MistComp, key: KeysymHandle<'_>, state: KeyState, serial: Serial, time: u32) {
        match self {
            KbFocus::Surface(s) => KeyboardTarget::key(s, seat, data, key, state, serial, time),
            KbFocus::X11(x) => KeyboardTarget::key(x, seat, data, key, state, serial, time),
        }
    }
    fn modifiers(&self, seat: &Seat<MistComp>, data: &mut MistComp, modifiers: ModifiersState, serial: Serial) {
        match self {
            KbFocus::Surface(s) => KeyboardTarget::modifiers(s, seat, data, modifiers, serial),
            KbFocus::X11(x) => KeyboardTarget::modifiers(x, seat, data, modifiers, serial),
        }
    }
}

/// The keyboard target for a window (xdg toplevel or X11).
pub fn of_window(w: &smithay::desktop::Window) -> Option<KbFocus> {
    if let Some(t) = w.toplevel() {
        return Some(KbFocus::Surface(t.wl_surface().clone()));
    }
    w.x11_surface().map(|x| KbFocus::X11(x.clone()))
}
