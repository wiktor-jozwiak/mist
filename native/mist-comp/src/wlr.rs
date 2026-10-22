//! wlroots protocols Smithay does not implement, for the tools people already use with Hyprland:
//! - foreign-toplevel-management: task bars and window switchers (waybar wlr/taskbar, rofi, nwg-dock):
//!   the window list with title, app id, output and state; activate / close / (un)maximize / (un)minimize /
//!   (un)fullscreen. Kept in step by one diff pass after events (`ftm_sync`), not by hooks everywhere
//! - output-power-management: wlopm, swayidle-style DPMS
//! - virtual-pointer: wayvnc, remote desktop tools
//! - gamma-control: gammastep, wlsunset (replaces the night light's ramp on that output while bound)
//!
//! Sandboxed clients (security-context) do not get these globals.
use std::collections::HashMap;
use std::sync::Mutex;

use smithay::{
    backend::input::{Axis, AxisSource, ButtonState},
    desktop::Window,
    input::pointer::AxisFrame,
    output::Output,
    reexports::{
        wayland_protocols_wlr::{
            foreign_toplevel::v1::server::{
                zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
                zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
            },
            gamma_control::v1::server::{
                zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
                zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
            },
            output_power_management::v1::server::{
                zwlr_output_power_manager_v1::{self, ZwlrOutputPowerManagerV1},
                zwlr_output_power_v1::{self, ZwlrOutputPowerV1},
            },
            virtual_pointer::v1::server::{
                zwlr_virtual_pointer_manager_v1::{self, ZwlrVirtualPointerManagerV1},
                zwlr_virtual_pointer_v1::{self, ZwlrVirtualPointerV1},
            },
        },
        wayland_server::{
            backend::{ClientId, ObjectId},
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
        },
    },
    utils::{Logical, Point},
};

use crate::{wm::MINIMIZED, MistComp};

pub fn trusted(client: &Client) -> bool {
    client.get_data::<crate::state::ClientState>().is_none_or(|c| c.security_context.is_none())
}

/// What a task bar was last told about a window.
#[derive(Clone, Default, PartialEq)]
struct Shown {
    title: String,
    app_id: String,
    output: Option<String>,
    states: Vec<u32>,
}

#[derive(Default)]
pub struct Wlr {
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    /// window address -> one handle per manager, with what that handle was told
    handles: HashMap<u64, Vec<(ObjectId, ZwlrForeignToplevelHandleV1, Shown)>>,
    /// something changed since the last sync
    pub dirty: bool,
    power: Vec<ZwlrOutputPowerV1>,
    /// output name -> the client controlling its gamma
    gamma: HashMap<String, ZwlrGammaControlV1>,
}

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, ZwlrForeignToplevelManagerV1, _>(3, ());
    dh.create_global::<MistComp, ZwlrOutputPowerManagerV1, _>(1, ());
    dh.create_global::<MistComp, ZwlrVirtualPointerManagerV1, _>(2, ());
    dh.create_global::<MistComp, ZwlrGammaControlManagerV1, _>(1, ());
}

// ------------------------------------------------------------------ foreign toplevel management
impl GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for MistComp {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrForeignToplevelManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        let m = init.init(res, ());
        state.wlr.managers.push(m);
        state.wlr.dirty = true;
        state.ftm_sync();
    }
    fn can_view(client: Client, _: &()) -> bool {
        trusted(&client)
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, m: &ZwlrForeignToplevelManagerV1, req: zwlr_foreign_toplevel_manager_v1::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = req {
            m.finished();
            state.ftm_forget_manager(&m.id());
        }
    }
    fn destroyed(state: &mut Self, _: ClientId, m: &ZwlrForeignToplevelManagerV1, _: &()) {
        state.ftm_forget_manager(&m.id());
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, u64> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &ZwlrForeignToplevelHandleV1, req: zwlr_foreign_toplevel_handle_v1::Request, addr: &u64, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        use zwlr_foreign_toplevel_handle_v1::Request;
        let Some(w) = state.window_by_addr(&format!("{addr:x}")) else { return };
        let maxed = crate::wm::data(&w).restore.borrow().is_some();
        let full = is_fullscreen(&w);
        match req {
            Request::Activate { .. } => {
                state.restore(&w, None);
                state.focus_window(Some(&w));
            }
            Request::Close => state.close_window(&w),
            Request::SetMaximized if !maxed => state.toggle_max(&w, false),
            Request::UnsetMaximized if maxed && !full => state.toggle_max(&w, false),
            Request::SetMinimized if crate::wm::data(&w).ws.get() != MINIMIZED => state.minimize(&w),
            Request::UnsetMinimized if crate::wm::data(&w).ws.get() == MINIMIZED => state.restore(&w, None),
            Request::SetFullscreen { .. } if !full => {
                if maxed {
                    state.toggle_max(&w, false);
                }
                state.toggle_max(&w, true);
            }
            Request::UnsetFullscreen if full => state.toggle_max(&w, true),
            _ => {}
        }
        state.wlr.dirty = true;
        state.dirty = true;
    }
}

fn is_fullscreen(w: &Window) -> bool {
    match (w.toplevel(), w.x11_surface()) {
        (Some(t), _) => t.with_pending_state(|s| s.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen)),
        (_, Some(x)) => x.is_fullscreen(),
        _ => false,
    }
}

impl MistComp {
    fn ftm_forget_manager(&mut self, id: &ObjectId) {
        self.wlr.managers.retain(|m| m.id() != *id);
        for hs in self.wlr.handles.values_mut() {
            hs.retain(|(m, _, _)| m != id);
        }
    }

    /// Bring every task bar up to date: new windows announced, changes sent, gone ones closed.
    pub fn ftm_sync(&mut self) {
        if !std::mem::take(&mut self.wlr.dirty) || self.wlr.managers.is_empty() {
            return;
        }
        let dh = self.display_handle.clone();
        let focused = self.focused_window();
        let wins: Vec<Window> = self.space.elements().chain(self.stash.iter().map(|(w, _)| w)).filter(|w| crate::wm::data(w).addr.get() != 0).cloned().collect();
        let mut seen = Vec::with_capacity(wins.len());
        for w in &wins {
            let addr = crate::wm::data(w).addr.get();
            seen.push(addr);
            let d = crate::wm::data(w);
            let mut states = Vec::new();
            let full = is_fullscreen(w);
            if d.restore.borrow().is_some() && !full {
                states.push(zwlr_foreign_toplevel_handle_v1::State::Maximized as u32);
            }
            if d.ws.get() == MINIMIZED {
                states.push(zwlr_foreign_toplevel_handle_v1::State::Minimized as u32);
            }
            if focused.as_ref() == Some(w) {
                states.push(zwlr_foreign_toplevel_handle_v1::State::Activated as u32);
            }
            if full {
                states.push(zwlr_foreign_toplevel_handle_v1::State::Fullscreen as u32);
            }
            let now = Shown { title: crate::hyprcompat::title(w), app_id: crate::hyprcompat::app_id(w), output: self.output_of(w).map(|o| o.name()), states };
            let output = now.output.as_ref().and_then(|n| self.space.outputs().find(|o| o.name() == *n).cloned());
            let managers = self.wlr.managers.clone();
            let entry = self.wlr.handles.entry(addr).or_default();
            // a handle for each manager that has none for this window yet
            for m in &managers {
                if entry.iter().any(|(id, _, _)| *id == m.id()) {
                    continue;
                }
                let Some(client) = m.client() else { continue };
                let Ok(h) = client.create_resource::<ZwlrForeignToplevelHandleV1, u64, MistComp>(&dh, m.version(), addr) else { continue };
                m.toplevel(&h);
                entry.push((m.id(), h, Shown::default()));
            }
            for (_, h, shown) in entry.iter_mut() {
                if *shown == now {
                    continue;
                }
                if shown.title != now.title {
                    h.title(now.title.clone());
                }
                if shown.app_id != now.app_id {
                    h.app_id(now.app_id.clone());
                }
                if shown.output != now.output {
                    if let Some(client) = h.client() {
                        if let Some(old) = shown.output.as_ref().and_then(|n| self.space.outputs().find(|o| o.name() == *n)) {
                            for o in old.client_outputs(&client) {
                                h.output_leave(&o);
                            }
                        }
                        if let Some(new) = &output {
                            for o in new.client_outputs(&client) {
                                h.output_enter(&o);
                            }
                        }
                    }
                }
                if shown.states != now.states {
                    h.state(now.states.iter().flat_map(|s| s.to_ne_bytes()).collect());
                }
                h.done();
                *shown = now.clone();
            }
        }
        self.wlr.handles.retain(|addr, hs| {
            let keep = seen.contains(addr);
            if !keep {
                for (_, h, _) in hs.iter() {
                    h.closed();
                }
            }
            keep
        });
    }
}

// ------------------------------------------------------------------ output power
impl GlobalDispatch<ZwlrOutputPowerManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrOutputPowerManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
    fn can_view(client: Client, _: &()) -> bool {
        trusted(&client)
    }
}

impl Dispatch<ZwlrOutputPowerManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &ZwlrOutputPowerManagerV1, req: zwlr_output_power_manager_v1::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        if let zwlr_output_power_manager_v1::Request::GetOutputPower { id, output } = req {
            let p = init.init(id, ());
            if Output::from_resource(&output).is_none() {
                p.failed();
                return;
            }
            p.mode(if state.dpms_on { zwlr_output_power_v1::Mode::On } else { zwlr_output_power_v1::Mode::Off });
            state.wlr.power.push(p);
        }
    }
}

impl Dispatch<ZwlrOutputPowerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &ZwlrOutputPowerV1, req: zwlr_output_power_v1::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let zwlr_output_power_v1::Request::SetMode { mode } = req {
            // DPMS is per session here (all monitors), as idle and the lock screen use it
            let on = matches!(mode.into_result(), Ok(zwlr_output_power_v1::Mode::On));
            state.set_dpms(on);
        }
    }
    fn destroyed(state: &mut Self, _: ClientId, p: &ZwlrOutputPowerV1, _: &()) {
        state.wlr.power.retain(|x| x.id() != p.id());
    }
}

impl MistComp {
    /// DPMS changed (idle, lock, a client): every output-power object hears it.
    pub fn power_note(&mut self) {
        let mode = if self.dpms_on { zwlr_output_power_v1::Mode::On } else { zwlr_output_power_v1::Mode::Off };
        for p in &self.wlr.power {
            p.mode(mode);
        }
    }
}

// ------------------------------------------------------------------ virtual pointer
/// The output a virtual pointer's absolute motion maps to (None: all of them).
pub struct VPointer {
    output: Option<String>,
    frame: Mutex<Option<AxisFrame>>,
}

impl GlobalDispatch<ZwlrVirtualPointerManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrVirtualPointerManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
    fn can_view(client: Client, _: &()) -> bool {
        trusted(&client)
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for MistComp {
    fn request(_: &mut Self, _: &Client, _: &ZwlrVirtualPointerManagerV1, req: zwlr_virtual_pointer_manager_v1::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        use zwlr_virtual_pointer_manager_v1::Request;
        match req {
            Request::CreateVirtualPointer { id, .. } => {
                init.init(id, VPointer { output: None, frame: Mutex::new(None) });
            }
            Request::CreateVirtualPointerWithOutput { id, output, .. } => {
                let output = output.and_then(|o| Output::from_resource(&o)).map(|o| o.name());
                init.init(id, VPointer { output, frame: Mutex::new(None) });
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrVirtualPointerV1, VPointer> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &ZwlrVirtualPointerV1, req: zwlr_virtual_pointer_v1::Request, vp: &VPointer, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        use zwlr_virtual_pointer_v1::Request;
        // remote input (wayvnc) keeps the session awake like a real mouse
        state.idle.notify_activity(&state.seat);
        let axis_of = |a: smithay::reexports::wayland_server::WEnum<smithay::reexports::wayland_server::protocol::wl_pointer::Axis>| match a.into_result() {
            Ok(smithay::reexports::wayland_server::protocol::wl_pointer::Axis::HorizontalScroll) => Axis::Horizontal,
            _ => Axis::Vertical,
        };
        match req {
            Request::Motion { time, dx, dy } => {
                let p = state.seat.get_pointer().unwrap().current_location() + Point::<f64, Logical>::from((dx, dy));
                let p = state.clamp_to_outputs(p);
                state.pointer_to(p, time);
            }
            Request::MotionAbsolute { time, x, y, x_extent, y_extent } => {
                if x_extent == 0 || y_extent == 0 {
                    return;
                }
                let area = match &vp.output {
                    Some(n) => state.space.outputs().find(|o| o.name() == *n).and_then(|o| state.space.output_geometry(o)),
                    None => state.space.outputs().filter_map(|o| state.space.output_geometry(o)).reduce(|a, b| a.merge(b)),
                };
                let Some(g) = area else { return };
                let p = Point::<f64, Logical>::from((g.loc.x as f64 + x as f64 / x_extent as f64 * g.size.w as f64, g.loc.y as f64 + y as f64 / y_extent as f64 * g.size.h as f64));
                let p = state.clamp_to_outputs(p);
                state.pointer_to(p, time);
            }
            Request::Button { time, button, state: s } => {
                let pressed = matches!(s.into_result(), Ok(smithay::reexports::wayland_server::protocol::wl_pointer::ButtonState::Pressed));
                state.handle_button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, time);
            }
            Request::Axis { time, axis, value } => {
                let mut f = vp.frame.lock().unwrap();
                let fr = f.take().unwrap_or_else(|| AxisFrame::new(time));
                *f = Some(fr.value(axis_of(axis), value));
            }
            Request::AxisDiscrete { time, axis, value, discrete } => {
                let mut f = vp.frame.lock().unwrap();
                let fr = f.take().unwrap_or_else(|| AxisFrame::new(time));
                *f = Some(fr.value(axis_of(axis), value).v120(axis_of(axis), discrete * 120));
            }
            Request::AxisSource { axis_source } => {
                let src = match axis_source.into_result() {
                    Ok(smithay::reexports::wayland_server::protocol::wl_pointer::AxisSource::Finger) => AxisSource::Finger,
                    Ok(smithay::reexports::wayland_server::protocol::wl_pointer::AxisSource::Continuous) => AxisSource::Continuous,
                    Ok(smithay::reexports::wayland_server::protocol::wl_pointer::AxisSource::WheelTilt) => AxisSource::WheelTilt,
                    _ => AxisSource::Wheel,
                };
                let mut f = vp.frame.lock().unwrap();
                let fr = f.take().unwrap_or_else(|| AxisFrame::new(0));
                *f = Some(fr.source(src));
            }
            Request::AxisStop { time, axis } => {
                let mut f = vp.frame.lock().unwrap();
                let fr = f.take().unwrap_or_else(|| AxisFrame::new(time));
                *f = Some(fr.stop(axis_of(axis)));
            }
            Request::Frame => {
                let fr = vp.frame.lock().unwrap().take();
                let p = state.seat.get_pointer().unwrap();
                if let Some(fr) = fr {
                    p.axis(state, fr);
                }
                p.frame(state);
            }
            _ => {}
        }
        state.dirty = true;
    }
}

// ------------------------------------------------------------------ gamma control
impl GlobalDispatch<ZwlrGammaControlManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrGammaControlManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        init.init(res, ());
    }
    fn can_view(client: Client, _: &()) -> bool {
        trusted(&client)
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, _: &ZwlrGammaControlManagerV1, req: zwlr_gamma_control_manager_v1::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        if let zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } = req {
            let name = Output::from_resource(&output).map(|o| o.name()).unwrap_or_default();
            let g = init.init(id, name.clone());
            // one client per output; no LUT (nested, headless): failed
            let size = state.udev.as_ref().and_then(|u| u.gamma_size(&name));
            match size {
                Some(n) if !state.wlr.gamma.contains_key(&name) => {
                    g.gamma_size(n);
                    state.wlr.gamma.insert(name, g);
                }
                _ => g.failed(),
            }
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, String> for MistComp {
    fn request(state: &mut Self, _: &Client, g: &ZwlrGammaControlV1, req: zwlr_gamma_control_v1::Request, name: &String, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let zwlr_gamma_control_v1::Request::SetGamma { fd } = req {
            if state.wlr.gamma.get(name).map(|x| x.id()) != Some(g.id()) {
                return;
            }
            let Some(n) = state.udev.as_ref().and_then(|u| u.gamma_size(name)) else { return g.failed() };
            let n = n as usize;
            let mut buf = vec![0u8; n * 3 * 2];
            let mut f = std::fs::File::from(fd);
            use std::io::{Read, Seek};
            let _ = f.seek(std::io::SeekFrom::Start(0));
            if f.read_exact(&mut buf).is_err() {
                return g.failed();
            }
            let ramp: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect();
            let ok = state.udev.as_ref().is_some_and(|u| u.set_gamma(name, &ramp[..n], &ramp[n..2 * n], &ramp[2 * n..]));
            if !ok {
                g.failed();
            }
        }
    }
    fn destroyed(state: &mut Self, _: ClientId, g: &ZwlrGammaControlV1, name: &String) {
        if state.wlr.gamma.get(name).map(|x| x.id()) == Some(g.id()) {
            state.wlr.gamma.remove(name);
            if let Some(u) = state.udev.as_ref() {
                u.reset_gamma(name);
            }
        }
    }
}
