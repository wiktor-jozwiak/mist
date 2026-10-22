//! Test client (tests/t-extws.sh): binds ext_workspace_manager_v1, prints the workspaces at every `done`
//! ("done: 1@HDMI-A-1* 2@HDMI-A-1"), and with an argument N activates workspace N once, then commits.
use wayland_client::{event_created_child, protocol::wl_registry, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self as grp, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self as wsh, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self as mgr, ExtWorkspaceManagerV1},
};

#[derive(Default)]
struct Ws {
    h: Option<ExtWorkspaceHandleV1>,
    name: String,
    group: Option<u32>,
    active: bool,
    gone: bool,
}

#[derive(Default)]
struct App {
    mgr: Option<ExtWorkspaceManagerV1>,
    ws: Vec<Ws>,
    groups: Vec<(u32, String)>,
    want: Option<String>,
    dones: usize,
    /// wl_output proxy key, its name
    names: Vec<(String, String)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(app: &mut Self, reg: &wl_registry::WlRegistry, ev: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = ev {
            if interface == "ext_workspace_manager_v1" {
                app.mgr = Some(reg.bind(name, 1, qh, ()));
            }
            if interface == "wl_output" {
                reg.bind::<wayland_client::protocol::wl_output::WlOutput, _, _>(name, 4, qh, ());
            }
        }
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for App {
    fn event(app: &mut Self, m: &ExtWorkspaceManagerV1, ev: mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            mgr::Event::WorkspaceGroup { workspace_group } => app.groups.push((workspace_group.id().protocol_id(), String::new())),
            mgr::Event::Workspace { workspace } => app.ws.push(Ws { h: Some(workspace), ..Default::default() }),
            mgr::Event::Done => {
                app.dones += 1;
                let list: Vec<String> = app
                    .ws
                    .iter()
                    .filter(|w| !w.gone)
                    .map(|w| format!("{}@{}{}", w.name, app.groups.iter().find(|(g, _)| Some(*g) == w.group).map(|(_, n)| n.as_str()).unwrap_or("?"), if w.active { "*" } else { "" }))
                    .collect();
                println!("done: {}", list.join(" "));
                if let Some(n) = app.want.take() {
                    if let Some(h) = app.ws.iter().find(|w| w.name == n && !w.gone).and_then(|w| w.h.clone()) {
                        h.activate();
                        m.commit();
                        println!("activate {n}");
                    }
                }
            }
            _ => {}
        }
    }
    event_created_child!(App, ExtWorkspaceManagerV1, [
        mgr::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        mgr::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for App {
    fn event(app: &mut Self, g: &ExtWorkspaceGroupHandleV1, ev: grp::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let gid = g.id().protocol_id();
        match ev {
            grp::Event::OutputEnter { output } => {
                if let Some(e) = app.groups.iter_mut().find(|(i, _)| *i == gid) {
                    let key = format!("out{}", output.id().protocol_id());
                    e.1 = app.names.iter().find(|(k, _)| *k == key).map(|(_, n)| n.clone()).unwrap_or(key);
                }
            }
            grp::Event::WorkspaceEnter { workspace } => {
                if let Some(w) = app.ws.iter_mut().find(|w| w.h.as_ref() == Some(&workspace)) {
                    w.group = Some(gid);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for App {
    fn event(app: &mut Self, h: &ExtWorkspaceHandleV1, ev: wsh::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let Some(w) = app.ws.iter_mut().find(|w| w.h.as_ref() == Some(h)) else { return };
        match ev {
            wsh::Event::Name { name } => w.name = name,
            wsh::Event::State { state } => w.active = state.into_result().is_ok_and(|s| s.contains(wsh::State::Active)),
            wsh::Event::Removed => w.gone = true,
            _ => {}
        }
    }
}

impl Dispatch<wayland_client::protocol::wl_output::WlOutput, ()> for App {
    fn event(app: &mut Self, o: &wayland_client::protocol::wl_output::WlOutput, ev: wayland_client::protocol::wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        // group names: the output's name, once it is known
        if let wayland_client::protocol::wl_output::Event::Name { name } = ev {
            let key = format!("out{}", o.id().protocol_id());
            for g in &mut app.groups {
                if g.1 == key {
                    g.1 = name.clone();
                }
            }
            app.names.push((key, name));
        }
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App { want: std::env::args().nth(1), ..Default::default() };
    q.roundtrip(&mut app).unwrap();
    assert!(app.mgr.is_some(), "no ext_workspace_manager_v1");
    loop {
        q.blocking_dispatch(&mut app).unwrap();
        // output names arrive after the groups: rename
        for g in &mut app.groups {
            if let Some((_, n)) = app.names.iter().find(|(k, _)| *k == g.1) {
                g.1 = n.clone();
            }
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}
