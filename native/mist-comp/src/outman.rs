//! wlr-output-management v4: monitor setup from outside (wlr-randr, kanshi, nwg-displays, wdisplays).
//!
//! Heads are the outputs with their modes; a configuration may set mode (DRM backend: any of the
//! connector's modes), position, scale, transform and adaptive sync on enabled outputs. Turning an output
//! off is not supported (the configuration fails). After a change every manager gets the heads anew
//! (finished + new heads, a new serial), which clients handle like a hotplug.
use std::sync::Mutex;

use smithay::{
    output::{Mode as WlMode, Output, Scale},
    reexports::{
        wayland_protocols_wlr::output_management::v1::server::{
            zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
            zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
            zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
            zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
            zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
        },
        wayland_server::{
            backend::ClientId, protocol::wl_output::Transform as WlTransform, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New,
            Resource, WEnum, Weak,
        },
    },
    utils::Transform,
};

use crate::MistComp;

#[derive(Default)]
pub struct OutMan {
    managers: Vec<ZwlrOutputManagerV1>,
    /// heads and modes sent, per manager (dropped and re-sent on a change)
    sent: Vec<(ZwlrOutputManagerV1, Vec<ZwlrOutputHeadV1>, Vec<ZwlrOutputModeV1>)>,
    serial: u32,
}

/// A configuration's wish for one head.
#[derive(Default, Clone)]
pub struct Wish {
    output: String,
    mode: Option<WlMode>,
    position: Option<(i32, i32)>,
    scale: Option<f64>,
    transform: Option<Transform>,
    vrr: Option<bool>,
}

pub struct Conf {
    serial: u32,
    wishes: Mutex<Vec<Wish>>,
    disabled: Mutex<Vec<String>>,
    used: Mutex<bool>,
}

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, ZwlrOutputManagerV1, _>(4, ());
}

fn trusted(client: &Client) -> bool {
    client.get_data::<crate::state::ClientState>().is_none_or(|c| c.security_context.is_none())
}

fn to_wl(t: Transform) -> WlTransform {
    match t {
        Transform::Normal => WlTransform::Normal,
        Transform::_90 => WlTransform::_90,
        Transform::_180 => WlTransform::_180,
        Transform::_270 => WlTransform::_270,
        Transform::Flipped => WlTransform::Flipped,
        Transform::Flipped90 => WlTransform::Flipped90,
        Transform::Flipped180 => WlTransform::Flipped180,
        Transform::Flipped270 => WlTransform::Flipped270,
    }
}
fn from_wl(t: WEnum<WlTransform>) -> Option<Transform> {
    Some(match t.into_result().ok()? {
        WlTransform::Normal => Transform::Normal,
        WlTransform::_90 => Transform::_90,
        WlTransform::_180 => Transform::_180,
        WlTransform::_270 => Transform::_270,
        WlTransform::Flipped => Transform::Flipped,
        WlTransform::Flipped90 => Transform::Flipped90,
        WlTransform::Flipped180 => Transform::Flipped180,
        WlTransform::Flipped270 => Transform::Flipped270,
        _ => return None,
    })
}

impl MistComp {
    /// Send every output as a head (with its modes) to one manager.
    fn outman_send(&mut self, m: &ZwlrOutputManagerV1) {
        let Some(client) = m.client() else { return };
        let dh = self.display_handle.clone();
        let (mut heads, mut modes) = (Vec::new(), Vec::new());
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        for o in &outputs {
            let Ok(h) = client.create_resource::<ZwlrOutputHeadV1, String, MistComp>(&dh, m.version(), o.name()) else { continue };
            m.head(&h);
            h.name(o.name());
            h.description(o.description());
            let p = o.physical_properties();
            if p.size.w > 0 && p.size.h > 0 {
                h.physical_size(p.size.w, p.size.h);
            }
            let cur = o.current_mode();
            let preferred = o.preferred_mode();
            let mut list = o.modes();
            if list.is_empty() {
                list.extend(cur);
            }
            for md in list {
                let Ok(mr) = client.create_resource::<ZwlrOutputModeV1, (String, WlMode), MistComp>(&dh, m.version(), (o.name(), md)) else { continue };
                h.mode(&mr);
                mr.size(md.size.w, md.size.h);
                mr.refresh(md.refresh);
                if Some(md) == preferred {
                    mr.preferred();
                }
                if Some(md) == cur {
                    h.current_mode(&mr);
                }
                modes.push(mr);
            }
            h.enabled(1);
            let loc = self.space.output_geometry(o).map(|g| g.loc).unwrap_or_default();
            h.position(loc.x, loc.y);
            h.transform(to_wl(o.current_transform()));
            h.scale(o.current_scale().fractional_scale());
            if h.version() >= 2 {
                h.make(p.make.clone());
                h.model(p.model.clone());
            }
            if h.version() >= 4 {
                let vrr = self.config.vrr && self.fullscreen_on(o);
                h.adaptive_sync(if vrr { zwlr_output_head_v1::AdaptiveSyncState::Enabled } else { zwlr_output_head_v1::AdaptiveSyncState::Disabled });
            }
            heads.push(h);
        }
        // clients (wlr-randr) wait for a non-zero serial
        if self.outman.serial == 0 {
            self.outman.serial = 1;
        }
        m.done(self.outman.serial);
        self.outman.sent.push((m.clone(), heads, modes));
    }

    /// Outputs changed (hotplug, a configuration): every manager gets the heads anew.
    pub fn outman_changed(&mut self) {
        self.outman.serial = self.outman.serial.wrapping_add(1).max(1);
        for (_, heads, modes) in std::mem::take(&mut self.outman.sent) {
            for md in modes {
                md.finished();
            }
            for h in heads {
                h.finished();
            }
        }
        for m in self.outman.managers.clone() {
            self.outman_send(&m);
        }
    }

    /// Check (and with `apply`, carry out) a configuration.
    fn outman_run(&mut self, conf: &Conf, apply: bool) -> Result<(), &'static str> {
        if conf.serial != self.outman.serial {
            return Err("cancelled");
        }
        if !conf.disabled.lock().unwrap().is_empty() {
            return Err("turning an output off is not supported");
        }
        let wishes = conf.wishes.lock().unwrap().clone();
        for w in &wishes {
            let Some(o) = self.space.outputs().find(|o| o.name() == w.output).cloned() else { return Err("no such output") };
            if let Some(md) = w.mode {
                let same = o.current_mode() == Some(md);
                if !same && (self.udev.is_none() || !o.modes().contains(&md)) {
                    return Err("mode not available");
                }
            }
            if w.scale.is_some_and(|s| !(0.25..=8.0).contains(&s)) {
                return Err("bad scale");
            }
        }
        if !apply {
            return Ok(());
        }
        for w in wishes {
            let Some(o) = self.space.outputs().find(|o| o.name() == w.output).cloned() else { continue };
            if let Some(md) = w.mode.filter(|m| o.current_mode() != Some(*m)) {
                let ok = self.udev.as_mut().is_some_and(|u| u.set_mode(&w.output, md));
                if !ok {
                    return Err("mode change failed");
                }
            }
            let scale = w.scale.map(Scale::Fractional);
            o.change_current_state(None, w.transform, scale, w.position.map(Into::into));
            if let Some(p) = w.position {
                self.space.map_output(&o, p);
            }
            if let Some(v) = w.vrr {
                self.config.vrr = v;
            }
            smithay::desktop::layer_map_for_output(&o).arrange();
        }
        self.fix_ws();
        self.force_full = true;
        self.dirty = true;
        Ok(())
    }
}

impl GlobalDispatch<ZwlrOutputManagerV1, ()> for MistComp {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, res: New<ZwlrOutputManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        let m = init.init(res, ());
        state.outman.managers.push(m.clone());
        state.outman_send(&m);
    }
    fn can_view(client: Client, _: &()) -> bool {
        trusted(&client)
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, m: &ZwlrOutputManagerV1, req: zwlr_output_manager_v1::Request, _: &(), _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        match req {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                init.init(id, Conf { serial, wishes: Mutex::new(Vec::new()), disabled: Mutex::new(Vec::new()), used: Mutex::new(false) });
            }
            zwlr_output_manager_v1::Request::Stop => {
                m.finished();
                state.outman.managers.retain(|x| x.id() != m.id());
                state.outman.sent.retain(|(x, _, _)| x.id() != m.id());
            }
            _ => {}
        }
    }
    fn destroyed(state: &mut Self, _: ClientId, m: &ZwlrOutputManagerV1, _: &()) {
        state.outman.managers.retain(|x| x.id() != m.id());
        state.outman.sent.retain(|(x, _, _)| x.id() != m.id());
    }
}

impl Dispatch<ZwlrOutputHeadV1, String> for MistComp {
    fn request(_: &mut Self, _: &Client, _: &ZwlrOutputHeadV1, _: zwlr_output_head_v1::Request, _: &String, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<ZwlrOutputModeV1, (String, WlMode)> for MistComp {
    fn request(_: &mut Self, _: &Client, _: &ZwlrOutputModeV1, _: zwlr_output_mode_v1::Request, _: &(String, WlMode), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<ZwlrOutputConfigurationV1, Conf> for MistComp {
    fn request(state: &mut Self, _: &Client, c: &ZwlrOutputConfigurationV1, req: zwlr_output_configuration_v1::Request, conf: &Conf, _: &DisplayHandle, init: &mut DataInit<'_, Self>) {
        use zwlr_output_configuration_v1::Request;
        match req {
            Request::EnableHead { id, head } => {
                let name = head.data::<String>().cloned().unwrap_or_default();
                conf.wishes.lock().unwrap().push(Wish { output: name.clone(), ..Default::default() });
                init.init(id, (name, c.downgrade()));
            }
            Request::DisableHead { head } => {
                conf.disabled.lock().unwrap().push(head.data::<String>().cloned().unwrap_or_default());
            }
            Request::Apply | Request::Test => {
                let apply = matches!(req, Request::Apply);
                if std::mem::replace(&mut *conf.used.lock().unwrap(), true) {
                    return;
                }
                match state.outman_run(conf, apply) {
                    Ok(()) => {
                        c.succeeded();
                        if apply {
                            state.outman_changed();
                        }
                    }
                    Err("cancelled") => c.cancelled(),
                    Err(e) => {
                        tracing::info!("output configuration refused: {e}");
                        c.failed();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationHeadV1, (String, Weak<ZwlrOutputConfigurationV1>)> for MistComp {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &ZwlrOutputConfigurationHeadV1,
        req: zwlr_output_configuration_head_v1::Request,
        (name, conf): &(String, Weak<ZwlrOutputConfigurationV1>),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        use zwlr_output_configuration_head_v1::Request;
        let Ok(conf) = conf.upgrade() else { return };
        let Some(c) = conf.data::<Conf>() else { return };
        let mut wishes = c.wishes.lock().unwrap();
        let Some(w) = wishes.iter_mut().rev().find(|w| w.output == *name) else { return };
        match req {
            Request::SetMode { mode } => w.mode = mode.data::<(String, WlMode)>().map(|d| d.1),
            Request::SetCustomMode { width, height, refresh } => w.mode = Some(WlMode { size: (width, height).into(), refresh }),
            Request::SetPosition { x, y } => w.position = Some((x, y)),
            Request::SetTransform { transform } => w.transform = from_wl(transform),
            Request::SetScale { scale } => w.scale = Some(scale),
            Request::SetAdaptiveSync { state: s } => w.vrr = Some(matches!(s.into_result(), Ok(zwlr_output_head_v1::AdaptiveSyncState::Enabled))),
            _ => {}
        }
    }
}

