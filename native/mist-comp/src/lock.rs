//! ext-session-lock (mist-ui-bin lock) and ext-idle-notify (mist-ui idle timers).
//!
//! Locked: outputs show only their lock surface (black until it has one, never the desktop), the pointer and
//! keyboard reach only lock surfaces, binds are limited to `bindl`.
use std::collections::HashMap;

use smithay::{
    delegate_idle_notify, delegate_session_lock,
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::{
        idle_notify::{IdleNotifierHandler, IdleNotifierState},
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
    },
};

use crate::MistComp;

#[derive(Default)]
pub struct Lock {
    /// output name -> its lock surface
    pub surfaces: HashMap<String, LockSurface>,
}

impl SessionLockHandler for MistComp {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock_manager
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        tracing::info!("session locked");
        self.locked = true;
        // nothing but black (or lock surfaces) is drawn from the next frame on
        self.force_full = true;
        self.dirty = true;
        confirmation.lock();
        let kb = self.seat.get_keyboard().unwrap();
        kb.set_focus(self, None, SERIAL_COUNTER.next_serial());
    }

    fn unlock(&mut self) {
        tracing::info!("session unlocked");
        self.locked = false;
        self.lock.surfaces.clear();
        let top = self.space.elements().last().cloned();
        self.focus_window(top.as_ref());
        self.force_full = true;
        self.dirty = true;
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let Some(output) = Output::from_resource(&wl_output) else { return };
        if let Some(mode) = output.current_mode() {
            let size = output.current_transform().transform_size(mode.size).to_logical(1);
            surface.with_pending_state(|s| s.size = Some((size.w as u32, size.h as u32).into()));
        }
        surface.send_configure();
        // the keyboard goes to the lock surface under the pointer (else the first one)
        let focus_here = self.active_output().as_ref() == Some(&output) || self.lock.surfaces.is_empty();
        let wl = surface.wl_surface().clone();
        self.lock.surfaces.insert(output.name(), surface);
        if focus_here {
            let kb = self.seat.get_keyboard().unwrap();
            kb.set_focus(self, Some(wl.into()), SERIAL_COUNTER.next_serial());
        }
        self.dirty = true;
    }
}
delegate_session_lock!(MistComp);

impl IdleNotifierHandler for MistComp {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.idle
    }
}
delegate_idle_notify!(MistComp);

impl MistComp {
    /// While locked: the lock surface under a point (output-relative surfaces start at the output origin).
    pub fn lock_surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        let o = self.space.outputs().find(|o| self.space.output_geometry(o).is_some_and(|g| g.to_f64().contains(pos)))?;
        let g = self.space.output_geometry(o)?;
        let s = self.lock.surfaces.get(&o.name())?;
        Some((s.wl_surface().clone(), g.loc.to_f64()))
    }
}

impl smithay::wayland::idle_inhibit::IdleInhibitHandler for MistComp {
    fn inhibit(&mut self, surface: WlSurface) {
        self.inhibitors.push(surface);
        self.idle.set_is_inhibited(true);
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.inhibitors.retain(|s| *s != surface);
        self.idle.set_is_inhibited(!self.inhibitors.is_empty());
    }
}
smithay::delegate_idle_inhibit!(MistComp);
