//! Hyprland-compatible IPC so mist-ui (and scripts calling `hyprctl`) run unchanged on mist-comp.
//!
//! $XDG_RUNTIME_DIR/hypr/<sig>/.socket.sock: `j/clients|monitors|workspaces|activewindow|activeworkspace|cursorpos`,
//! `/eval <lua>` (only the calls Mist makes: focus / close / bring_to_top / dpms / MistMinimize / MistRestore /
//! MistSnapApply), `/dispatch <classic>` (workspace, exec, killactive).
//! .socket2.sock: the events mist-ui listens to (openwindow, closewindow, activewindow(v2), workspace(v2),
//! focusedmon(v2), windowtitlev2, movewindowv2).
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

use serde_json::{json, Value};
use smithay::{
    desktop::{layer_map_for_output, Window},
    reexports::calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
    utils::{Logical, Rectangle},
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use crate::{
    config::{Action, WsArg},
    wm::{data, MINIMIZED},
    CalloopData, MistComp,
};

pub fn signature(socket: &str) -> String {
    format!("mist-comp-{socket}")
}

fn dir(sig: &str) -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into())).join("hypr").join(sig)
}

pub fn serve(event_loop: &mut EventLoop<CalloopData>, socket: &str) {
    let sig = signature(socket);
    let d = dir(&sig);
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::remove_file(d.join(".socket.sock"));
    let _ = std::fs::remove_file(d.join(".socket2.sock"));
    let req = UnixListener::bind(d.join(".socket.sock")).expect("hypr socket");
    let ev = UnixListener::bind(d.join(".socket2.sock")).expect("hypr socket2");
    req.set_nonblocking(true).unwrap();
    ev.set_nonblocking(true).unwrap();
    let h = event_loop.handle();
    h.insert_source(Generic::new(req, Interest::READ, Mode::Level), |_, l, data| {
        while let Ok((s, _)) = l.accept() {
            request(data, s);
        }
        Ok(PostAction::Continue)
    })
    .unwrap();
    h.insert_source(Generic::new(ev, Interest::READ, Mode::Level), |_, l, data| {
        while let Ok((s, _)) = l.accept() {
            let _ = s.set_nonblocking(true);
            data.hypr_listeners.push(s);
        }
        Ok(PostAction::Continue)
    })
    .unwrap();
    std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", &sig);
    println!("HYPRLAND_INSTANCE_SIGNATURE={sig}");
}

impl MistComp {
    /// Queue a socket2 event line (`name>>data`).
    pub fn hypr_event(&mut self, name: &str, data: &str) {
        if self.hypr_listeners.is_empty() {
            return;
        }
        let line = format!("{name}>>{data}\n");
        self.hypr_listeners.retain_mut(|s| s.write_all(line.as_bytes()).is_ok());
    }

    pub fn window_by_addr(&self, addr: &str) -> Option<Window> {
        let a = u64::from_str_radix(addr.trim().trim_start_matches("0x"), 16).ok()?;
        self.space.elements().chain(self.stash.iter().map(|(w, _)| w)).find(|w| data(w).addr.get() == a).cloned()
    }
}

pub fn app_id(w: &Window) -> String {
    if let Some(x) = w.x11_surface() {
        return x.class();
    }
    w.toplevel()
        .and_then(|t| with_states(t.wl_surface(), |s| s.data_map.get::<XdgToplevelSurfaceData>().and_then(|d| d.lock().unwrap().app_id.clone())))
        .unwrap_or_default()
}

pub fn title(w: &Window) -> String {
    if let Some(x) = w.x11_surface() {
        return x.title();
    }
    w.toplevel()
        .and_then(|t| with_states(t.wl_surface(), |s| s.data_map.get::<XdgToplevelSurfaceData>().and_then(|d| d.lock().unwrap().title.clone())))
        .unwrap_or_default()
}

pub fn addr(w: &Window) -> String {
    format!("0x{:x}", data(w).addr.get())
}

fn ws_json(id: u32) -> Value {
    if id == MINIMIZED {
        json!({ "id": -99, "name": "special:minimized" })
    } else if id == crate::wm::SPECIAL {
        json!({ "id": -98, "name": "special:special" })
    } else {
        json!({ "id": id, "name": id.to_string() })
    }
}

fn request(state: &mut MistComp, mut s: UnixStream) {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(200)));
    let mut buf = [0u8; 8192];
    let n = s.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let reply = answer(state, req.trim());
    let _ = s.write_all(reply.as_bytes());
}

fn monitors_json(state: &mut MistComp) -> Value {
    let pointer = state.seat.get_pointer().unwrap().current_location();
    let outputs: Vec<_> = state.space.outputs().cloned().collect();
    let mut out = Vec::new();
    for (i, o) in outputs.iter().enumerate() {
        let g = state.space.output_geometry(o).unwrap_or_default();
        let mode = o.current_mode();
        let ws = state.active_ws(o);
        let zone = layer_map_for_output(o).non_exclusive_zone();
        let reserved = [zone.loc.x, zone.loc.y, g.size.w - zone.loc.x - zone.size.w, g.size.h - zone.loc.y - zone.size.h];
        out.push(json!({
            "id": i, "name": o.name(), "description": o.description(),
            "make": o.physical_properties().make, "model": o.physical_properties().model,
            "width": mode.map(|m| m.size.w).unwrap_or(g.size.w), "height": mode.map(|m| m.size.h).unwrap_or(g.size.h),
            "refreshRate": mode.map(|m| m.refresh as f64 / 1000.0).unwrap_or(60.0),
            "x": g.loc.x, "y": g.loc.y, "scale": o.current_scale().fractional_scale(), "transform": 0,
            "activeWorkspace": ws_json(ws), "specialWorkspace": { "id": 0, "name": "" },
            "reserved": reserved, "focused": g.to_f64().contains(pointer), "dpmsStatus": true, "vrr": state.config.vrr && state.fullscreen_on(o), "disabled": false,
        }));
    }
    Value::Array(out)
}

fn client_json(state: &MistComp, w: &Window, hidden_loc: Option<smithay::utils::Point<i32, Logical>>) -> Value {
    let mut geo = match hidden_loc {
        Some(l) => Rectangle::new(l, w.geometry().size),
        None => state.space.element_geometry(w).unwrap_or_default(),
    };
    // the Mist title bar is part of the window (hyprbars bar_part_of_window): coverage, window shots
    let bar = state.bar_h(w);
    geo.loc.y -= bar;
    geo.size.h += bar;
    let mon = state
        .output_of(w)
        .and_then(|o| state.space.outputs().position(|x| *x == o))
        .map(|i| i as i64)
        .unwrap_or(0);
    let d = data(w);
    let pid = state.window_pid(w).unwrap_or(-1);
    let focus = state.focus_history.iter().position(|a| *a == d.addr.get()).map(|i| i as i64).unwrap_or(99);
    let full = if d.restore.borrow().is_some() { 1 } else { 0 };
    let class = app_id(w);
    let t = title(w);
    json!({
        "address": addr(w), "mapped": true, "hidden": hidden_loc.is_some(),
        "at": [geo.loc.x, geo.loc.y], "size": [geo.size.w, geo.size.h],
        "workspace": ws_json(d.ws.get()), "floating": true, "pseudo": false, "monitor": mon,
        "class": class, "title": t, "initialClass": class, "initialTitle": t,
        "pid": pid, "xwayland": w.x11_surface().is_some(), "pinned": false, "fullscreen": full, "fullscreenClient": full,
        "grouped": [], "tags": [], "swallowing": "0x0", "focusHistoryID": focus,
    })
}

fn answer(state: &mut MistComp, req: &str) -> String {
    // hyprctl may send flags first: "j/clients", "[[BATCH]]..."
    let (json_mode, cmd) = match req.split_once('/') {
        Some((flags, rest)) => (flags.contains('j'), rest),
        None => (false, req),
    };
    let (word, rest) = cmd.split_once(char::is_whitespace).unwrap_or((cmd, ""));
    let v = match word {
        "monitors" => monitors_json(state),
        "clients" => {
            let mut v: Vec<Value> = state.space.elements().filter(|w| data(w).addr.get() != 0).map(|w| client_json(state, w, None)).collect();
            v.extend(state.stash.iter().map(|(w, l)| client_json(state, w, Some(*l))));
            Value::Array(v)
        }
        "workspaces" => {
            let mut ids: Vec<u32> = state.ws_active.values().copied().collect();
            for w in state.space.elements().chain(state.stash.iter().map(|(w, _)| w)) {
                ids.push(data(w).ws.get());
            }
            ids.sort();
            ids.dedup();
            let outputs: Vec<_> = state.space.outputs().cloned().collect();
            Value::Array(
                ids.into_iter()
                    .map(|id| {
                        let n = state.space.elements().chain(state.stash.iter().map(|(w, _)| w)).filter(|w| data(w).ws.get() == id).count();
                        let mon = state.ws_active.iter().find(|(_, v)| **v == id).map(|(m, _)| m.clone()).unwrap_or_default();
                        let mid = outputs.iter().position(|o| o.name() == mon).map(|i| i as i64).unwrap_or(0);
                        let mut j = ws_json(id);
                        j["monitor"] = json!(mon);
                        j["monitorID"] = json!(mid);
                        j["windows"] = json!(n);
                        j["hasfullscreen"] = json!(false);
                        j
                    })
                    .collect(),
            )
        }
        "activeworkspace" => {
            let o = state.active_output();
            let id = o.map(|o| state.active_ws(&o)).unwrap_or(1);
            ws_json(id)
        }
        "activewindow" => match state.focused_window() {
            Some(w) => client_json(state, &w, None),
            None => json!({}),
        },
        "cursorpos" => {
            let p = state.seat.get_pointer().unwrap().current_location();
            json!({ "x": p.x as i64, "y": p.y as i64 })
        }
        "eval" => return eval(state, rest),
        "dispatch" => return dispatch(state, rest),
        "version" => json!({ "branch": "mist-comp", "tag": env!("CARGO_PKG_VERSION") }),
        "configerrors" => json!([]),
        _ => return "unknown request\n".into(),
    };
    if json_mode {
        v.to_string()
    } else {
        format!("{v:#}")
    }
}

/// Quoted / bare argument after `key =` in a Lua table (`window = "address:0x1"`).
fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let i = s.find(&format!("{key} ="))? + key.len() + 2;
    let r = s[i..].trim_start();
    if let Some(r) = r.strip_prefix('"') {
        return r.split('"').next();
    }
    r.split([',', '}', ')', ' ']).next()
}

/// The arguments of a Lua call `Name(a, b, ...)` (quotes stripped).
fn call_args(s: &str) -> Vec<String> {
    let inner = s.split_once('(').map(|x| x.1).unwrap_or("");
    let inner = inner.rsplit_once(')').map(|x| x.0).unwrap_or(inner);
    inner.split(',').map(|a| a.trim().trim_matches(['"', '\'']).to_string()).filter(|a| !a.is_empty()).collect()
}

fn find_target(state: &MistComp, sel: &str) -> Option<Window> {
    if let Some(a) = sel.strip_prefix("address:") {
        return state.window_by_addr(a);
    }
    if let Some(c) = sel.strip_prefix("class:") {
        let c = c.trim_start_matches("(?i)").to_lowercase();
        return state
            .space
            .elements()
            .chain(state.stash.iter().map(|(w, _)| w))
            .find(|w| app_id(w).to_lowercase() == c)
            .cloned();
    }
    None
}

/// The small Lua subset Mist sends through `hyprctl eval`; statements split on `;`.
fn eval(state: &mut MistComp, code: &str) -> String {
    for stmt in code.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if stmt.starts_with("MistMinimize") {
            let a = call_args(stmt);
            let w = a.first().and_then(|a| state.window_by_addr(a)).or_else(|| state.focused_window());
            if let Some(w) = w {
                state.minimize(&w);
            }
        } else if stmt.starts_with("MistRestore") {
            let a = call_args(stmt);
            if let Some(w) = a.first().and_then(|a| state.window_by_addr(a)) {
                let ws = a.get(1).and_then(|n| n.parse().ok());
                state.restore(&w, ws);
            }
        } else if stmt.starts_with("MistSnapApply") {
            let a = call_args(stmt);
            let f: Vec<f64> = a.iter().skip(1).filter_map(|x| x.parse().ok()).collect();
            if let (Some(w), true) = (a.first().and_then(|a| state.window_by_addr(a)), f.len() >= 4) {
                let grab = (f.len() >= 8).then(|| [f[4], f[5], f[6], f[7]]);
                state.snap_apply(&w, [f[0], f[1], f[2], f[3]], grab);
            }
        } else if stmt.starts_with("MistMaximize") || stmt.starts_with("MistUnsnap") || stmt.starts_with("MistLayouts") {
            // windows-like.lua functions (addr optional: else the focused window)
            let a = call_args(stmt);
            let w = a.first().and_then(|a| state.window_by_addr(a)).or_else(|| state.focused_window());
            if let Some(w) = w {
                match stmt.as_bytes()[4] {
                    b'M' => state.mist_maximize(&w),
                    b'U' => state.snap_key_on(&w, crate::snap::Key::Unsnap),
                    _ => state.snap_key_on(&w, crate::snap::Key::Layouts),
                }
            }
        } else if stmt.contains("hl.dsp.focus") {
            if let Some(sel) = field(stmt, "window") {
                if let Some(w) = find_target(state, sel) {
                    state.restore(&w, None);
                }
            } else if let Some(ws) = field(stmt, "workspace") {
                let arg = match ws {
                    "m+1" | "+1" => Some(WsArg::Rel(1)),
                    "m-1" | "-1" => Some(WsArg::Rel(-1)),
                    n => n.parse().ok().map(WsArg::Id),
                };
                if let Some(a) = arg {
                    state.do_action(Action::Workspace(a));
                }
            }
        } else if stmt.contains("hl.dsp.window.close") {
            state.do_action(Action::Close);
        } else if stmt.contains("hl.dsp.window.bring_to_top") {
            if let Some(w) = state.focused_window() {
                state.space.raise_element(&w, true);
            }
        } else if stmt.contains("hl.dsp.dpms") {
            let on = field(stmt, "action") != Some("off");
            state.set_dpms(on);
        } else if stmt.contains("hl.dsp.send_shortcut") {
            // Spotlight emoji: paste into the active window
            let mods = field(stmt, "mods").unwrap_or("").to_string();
            if let Some(key) = field(stmt, "key").map(str::to_string) {
                state.send_shortcut(&mods, &key);
            }
        } else if stmt.starts_with("hl.config") {
            // mist-ui night light sets screen_shader = ".../nightlight.glsl" (or ""): a gamma ramp here
            if let Some(sh) = field(stmt, "screen_shader") {
                state.set_night(sh.contains("nightlight"));
            }
        } else {
            tracing::debug!("eval ignored: {stmt}");
        }
    }
    state.dirty = true;
    "ok".into()
}

fn dispatch(state: &mut MistComp, args: &str) -> String {
    // the Lua form (`hyprctl dispatch "hl.dsp.…"`): same as eval
    if args.trim_start().starts_with("hl.") {
        return eval(state, args);
    }
    let (d, rest) = args.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((args, ""));
    let action = match d {
        "workspace" => rest.parse().ok().map(|n| Action::Workspace(WsArg::Id(n))),
        "exec" => Some(Action::Exec(rest.into())),
        "killactive" => Some(Action::Close),
        "fullscreen" => Some(Action::Fullscreen),
        "movetoworkspace" => rest.parse().ok().map(|n| Action::MoveToWorkspace(WsArg::Id(n))),
        _ => None,
    };
    match action {
        Some(a) => {
            state.do_action(a);
            "ok".into()
        }
        None => format!("unknown dispatcher {d}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_bits() {
        assert_eq!(field(r#"hl.dispatch(hl.dsp.focus({ window = "address:0x1f" }))"#, "window"), Some("address:0x1f"));
        assert_eq!(field(r#"hl.dispatch(hl.dsp.dpms({ action = "off" }))"#, "action"), Some("off"));
        assert_eq!(call_args("MistRestore('0x12', 3)"), vec!["0x12", "3"]);
        assert_eq!(call_args(r#"MistSnapApply("0x5", 0, 0, 0.5, 1, 10, 20, 300, 400)"#).len(), 9);
    }
}
