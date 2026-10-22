//! ext-workspace-v1: the workspaces for bars and docks that are not Mist's own (waybar `ext/workspaces`,
//! ironbar, sfwbar, …; Mist's bar reads the Hyprland IPC). One group per monitor, one workspace per number that
//! is shown or holds windows (as `hyprctl workspaces` lists them), `activate` + `commit` switches. Kept in step
//! by `extws_sync` every loop turn: nothing is sent unless the list changed, nothing is computed without a
//! bound client.
use smithay::{
    output::Output,
    reexports::{
        wayland_protocols::ext::workspace::v1::server::{
            ext_workspace_group_handle_v1::{self as grp, ExtWorkspaceGroupHandleV1},
            ext_workspace_handle_v1::{self as wsh, ExtWorkspaceHandleV1},
            ext_workspace_manager_v1::{self as mgr, ExtWorkspaceManagerV1},
        },
        wayland_server::{backend::ClientId, protocol::wl_output::WlOutput, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource},
    },
};

use crate::{wm::data, MistComp};

/// One bound manager and what it has been told.
pub struct Bound {
    mgr: ExtWorkspaceManagerV1,
    /// a group per output name, and the client's wl_outputs it was told about
    groups: Vec<(String, ExtWorkspaceGroupHandleV1, Vec<WlOutput>)>,
    /// workspace number, its handle, its group's output, active
    ws: Vec<(u32, ExtWorkspaceHandleV1, String, bool)>,
    /// `activate` requests waiting for `commit`
    pending: Vec<u32>,
}

/// (workspace, output it belongs to, shown)
pub type Snap = Vec<(u32, String, bool)>;

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, ExtWorkspaceManagerV1, _>(1, ());
}

impl MistComp {
    fn extws_snapshot(&self) -> Snap {
        let mut ids: Vec<u32> = self.ws_active.values().copied().collect();
        for w in self.space.elements().chain(self.stash.iter().map(|(w, _)| w)) {
            ids.push(data(w).ws.get());
        }
        // not the scratchpad, not minimized windows (u32::MAX)
        ids.retain(|i| *i < crate::wm::SPECIAL);
        ids.sort();
        ids.dedup();
        let first = self.space.outputs().next().map(|o| o.name()).unwrap_or_default();
        ids.into_iter()
            .map(|id| {
                let shown = self.ws_active.iter().find(|(_, v)| **v == id).map(|(m, _)| m.clone());
                let out = shown.clone().or_else(|| self.ws_home(id).map(|o| o.name())).unwrap_or_else(|| first.clone());
                (id, out, shown.is_some())
            })
            .collect()
    }

    /// Tell every bound client what changed since the last time.
    pub fn extws_sync(&mut self) {
        if self.ext_ws.is_empty() {
            return;
        }
        let snap = self.extws_snapshot();
        // a client bound a wl_output since: its groups' output_enter is due
        let outs_due = self.ext_ws.iter().any(|b| {
            let Some(c) = b.mgr.client() else { return false };
            b.groups.iter().any(|(n, _, told)| self.space.outputs().find(|o| o.name() == *n).is_some_and(|o| o.client_outputs(&c).count() != told.len()))
        });
        if snap == self.ext_ws_last && !outs_due {
            return;
        }
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let dh = self.display_handle.clone();
        for b in &mut self.ext_ws {
            update(b, &dh, &outputs, &snap);
        }
        self.ext_ws_last = snap;
    }
}

fn update(b: &mut Bound, dh: &DisplayHandle, outputs: &[Output], snap: &Snap) {
    let Some(client) = b.mgr.client() else { return };
    // groups: one per monitor
    for o in outputs {
        if !b.groups.iter().any(|(n, ..)| *n == o.name()) {
            let Ok(g) = client.create_resource::<ExtWorkspaceGroupHandleV1, _, MistComp>(dh, b.mgr.version(), ()) else { continue };
            b.mgr.workspace_group(&g);
            g.capabilities(grp::GroupCapabilities::empty());
            b.groups.push((o.name(), g, Vec::new()));
        }
        // the client may bind wl_output after the manager: tell the group once it has
        let Some((_, g, told)) = b.groups.iter_mut().find(|(n, ..)| *n == o.name()) else { continue };
        for wo in o.client_outputs(&client) {
            if !told.contains(&wo) {
                g.output_enter(&wo);
                told.push(wo);
            }
        }
    }
    b.groups.retain(|(n, g, _)| {
        let keep = outputs.iter().any(|o| o.name() == *n);
        if !keep {
            g.removed();
        }
        keep
    });
    let group = |b: &Bound, out: &str| b.groups.iter().find(|(n, ..)| n == out).map(|(_, g, _)| g.clone());
    // workspaces gone
    let mut i = 0;
    while i < b.ws.len() {
        if snap.iter().any(|(id, _, _)| *id == b.ws[i].0) {
            i += 1;
            continue;
        }
        let (_, h, out, _) = b.ws.remove(i);
        if let Some(g) = group(b, &out) {
            g.workspace_leave(&h);
        }
        h.removed();
    }
    // new and changed
    for (id, out, active) in snap {
        let state = if *active { wsh::State::Active } else { wsh::State::empty() };
        if let Some(k) = b.ws.iter().position(|(n, ..)| n == id) {
            let (_, h, old, was) = b.ws[k].clone();
            if old != *out {
                if let Some(g) = group(b, &old) {
                    g.workspace_leave(&h);
                }
                if let Some(g) = group(b, out) {
                    g.workspace_enter(&h);
                }
            }
            if was != *active {
                h.state(state);
            }
            b.ws[k] = (*id, h, out.clone(), *active);
            continue;
        }
        let Ok(h) = client.create_resource::<ExtWorkspaceHandleV1, _, MistComp>(dh, b.mgr.version(), ()) else { continue };
        b.mgr.workspace(&h);
        h.id(id.to_string());
        h.name(id.to_string());
        h.coordinates(id.saturating_sub(1).to_ne_bytes().to_vec());
        h.state(state);
        h.capabilities(wsh::WorkspaceCapabilities::Activate);
        if let Some(g) = group(b, out) {
            g.workspace_enter(&h);
        }
        b.ws.push((*id, h, out.clone(), *active));
    }
    b.mgr.done();
}

impl GlobalDispatch<ExtWorkspaceManagerV1, ()> for MistComp {
    fn bind(state: &mut Self, dh: &DisplayHandle, _: &Client, res: New<ExtWorkspaceManagerV1>, _: &(), init: &mut DataInit<'_, Self>) {
        let m = init.init(res, ());
        let mut b = Bound { mgr: m, groups: Vec::new(), ws: Vec::new(), pending: Vec::new() };
        let snap = state.extws_snapshot();
        let outputs: Vec<Output> = state.space.outputs().cloned().collect();
        update(&mut b, dh, &outputs, &snap);
        state.ext_ws.push(b);
        state.ext_ws_last = snap;
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, res: &ExtWorkspaceManagerV1, req: mgr::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        match req {
            mgr::Request::Commit => {
                let Some(b) = state.ext_ws.iter_mut().find(|b| b.mgr == *res) else { return };
                let todo = std::mem::take(&mut b.pending);
                for id in todo {
                    let out = state.ext_ws_last.iter().find(|(n, ..)| *n == id).map(|(_, o, _)| o.clone());
                    let o = out.and_then(|n| state.space.outputs().find(|o| o.name() == n).cloned()).or_else(|| state.active_output());
                    if let Some(o) = o {
                        state.switch_ws(&o, id);
                    }
                }
            }
            mgr::Request::Stop => {
                res.finished();
                state.ext_ws.retain(|b| b.mgr != *res);
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _: ClientId, res: &ExtWorkspaceManagerV1, _: &()) {
        state.ext_ws.retain(|b| b.mgr != *res);
    }
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for MistComp {
    // no create_workspace (capabilities say so); destroy has nothing to free
    fn request(_: &mut Self, _: &Client, _: &ExtWorkspaceGroupHandleV1, _: grp::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for MistComp {
    fn request(state: &mut Self, _: &Client, res: &ExtWorkspaceHandleV1, req: wsh::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let wsh::Request::Activate = req {
            for b in &mut state.ext_ws {
                if let Some((id, ..)) = b.ws.iter().find(|(_, h, ..)| h == res) {
                    b.pending.push(*id);
                }
            }
        }
    }

    fn destroyed(state: &mut Self, _: ClientId, res: &ExtWorkspaceHandleV1, _: &()) {
        for b in &mut state.ext_ws {
            b.ws.retain(|(_, h, ..)| h != res);
        }
    }
}
