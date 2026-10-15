//! Line IPC on $XDG_RUNTIME_DIR/mist-comp/<wayland socket>.sock (one command per connection).
//! `layers` | `windows` | `shot <out.ppm>` (written after the next frame) | `quit`
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

use smithay::{
    desktop::{layer_map_for_output, WindowSurfaceType},
    reexports::calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
};

use smithay::backend::input::{ButtonState, KeyState};

use crate::{CalloopData, MistComp};

pub fn path(socket: &str) -> PathBuf {
    let dir = PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into())).join("mist-comp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{socket}.sock"))
}

pub fn serve(event_loop: &mut EventLoop<CalloopData>, socket: &str) {
    let p = path(socket);
    let _ = std::fs::remove_file(&p);
    let listener = UnixListener::bind(&p).expect("ipc socket");
    listener.set_nonblocking(true).unwrap();
    event_loop
        .handle()
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, l, data| {
            while let Ok((stream, _)) = l.accept() {
                handle(data, stream);
            }
            Ok(PostAction::Continue)
        })
        .unwrap();
}

fn handle(state: &mut MistComp, mut stream: UnixStream) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));
    let mut line = String::new();
    let _ = BufReader::new(&stream).read_line(&mut line);
    let mut it = line.split_whitespace();
    let reply = match it.next().unwrap_or("") {
        "layers" => {
            let mut s = String::new();
            for o in state.space.outputs() {
                let map = layer_map_for_output(o);
                for l in map.layers() {
                    let g = map.layer_geometry(l).unwrap_or_default();
                    s += &format!("{} {} {:?} {},{} {}x{}\n", o.name(), l.namespace(), l.layer(), g.loc.x, g.loc.y, g.size.w, g.size.h);
                }
            }
            s
        }
        "windows" => {
            let title = |w: &smithay::desktop::Window| {
                w.toplevel()
                    .and_then(|t| {
                        smithay::wayland::compositor::with_states(t.wl_surface(), |s| {
                            s.data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().unwrap().app_id.clone())
                        })
                    })
                    .unwrap_or_default()
            };
            let mut s: String = state
                .space
                .elements()
                .map(|w| {
                    let g = state.space.element_geometry(w).unwrap_or_default();
                    let bar = if state.bar_h(w) > 0 { " bar" } else { "" };
                    format!("{} ws{} {},{} {}x{}{bar}\n", title(w), crate::wm::data(w).ws.get(), g.loc.x, g.loc.y, g.size.w, g.size.h)
                })
                .collect();
            for (w, loc) in &state.stash {
                s += &format!("{} ws{} hidden {},{}\n", title(w), crate::wm::data(w).ws.get(), loc.x, loc.y);
            }
            s
        }
        "action" => {
            let rest = line.trim().splitn(2, char::is_whitespace).nth(1).unwrap_or("");
            match crate::config::parse_action(rest.trim()) {
                Some(a) => {
                    state.do_action(a);
                    "ok\n".into()
                }
                None => format!("bad action {rest:?}\n"),
            }
        }
        // synthetic input (tests): evdev key codes, BTN_LEFT/RIGHT
        // `scroll N`: N wheel notches (> 0 down), binds first
        "scroll" => {
            let n: f64 = it.next().and_then(|n| n.parse().ok()).unwrap_or(0.0);
            if !state.wheel_bind(n * 120.0) {
                let t = state.start_time.elapsed().as_millis() as u32;
                let frame = smithay::input::pointer::AxisFrame::new(t)
                    .source(smithay::backend::input::AxisSource::Wheel)
                    .value(smithay::backend::input::Axis::Vertical, n * 15.0)
                    .v120(smithay::backend::input::Axis::Vertical, (n * 120.0) as i32);
                let p = state.seat.get_pointer().unwrap();
                p.axis(state, frame);
                p.frame(state);
            }
            "ok".into()
        }
        "key" | "motion" | "button" => {
            let t = state.start_time.elapsed().as_millis() as u32;
            let args: Vec<&str> = it.collect();
            let press = |a: Option<&&str>| a.map(|a| *a == "press");
            let r = match (line.split_whitespace().next().unwrap_or(""), args.as_slice()) {
                ("key", [code, st]) => code.parse::<u32>().ok().map(|c| {
                    let ks = if press(Some(st)) == Some(true) { KeyState::Pressed } else { KeyState::Released };
                    state.handle_key((c + 8).into(), ks, t)
                }),
                ("motion", [x, y]) => x.parse().ok().zip(y.parse().ok()).map(|(x, y): (f64, f64)| state.pointer_to((x, y).into(), t)),
                ("button", [b, st]) => {
                    let code = match *b { "left" => Some(0x110), "right" => Some(0x111), "middle" => Some(0x112), _ => None };
                    code.map(|c| {
                        let bs = if press(Some(st)) == Some(true) { ButtonState::Pressed } else { ButtonState::Released };
                        state.handle_button(c, bs, t)
                    })
                }
                _ => None,
            };
            state.dirty = true;
            if r.is_some() { "ok\n".into() } else { "usage: key CODE press|release | motion X Y | button left|right press|release\n".into() }
        }
        "locked" => format!("{} surfaces={}\n", state.locked, state.lock.surfaces.len()),
        "stats" => format!("frames {} commits {} windows {} hidden {}\n", state.frames, state.commits, state.space.elements().count(), state.stash.len()),
        "night" => format!("{}\n", state.night),
        "workspaces" => state.ws_active.iter().map(|(o, w)| format!("{o} {w}\n")).collect(),
        "shot" => match it.next() {
            Some(p) => {
                state.shot = Some((p.into(), stream));
                state.dirty = true;
                state.force_full = true;
                return;
            }
            None => "usage: shot <out.ppm>\n".into(),
        },
        "focus" => {
            let f = state.seat.get_keyboard().unwrap().current_focus().and_then(|f| smithay::wayland::seat::WaylandFocus::wl_surface(&f).map(|c| c.into_owned()));
            let layer = f.as_ref().and_then(|f| {
                state.space.outputs().find_map(|o| {
                    layer_map_for_output(o)
                        .layer_for_surface(f, WindowSurfaceType::TOPLEVEL)
                        .map(|l| l.namespace().to_string())
                })
            });
            let lock = f.as_ref().is_some_and(|f| state.lock.surfaces.values().any(|l| l.wl_surface() == f));
            match (f, layer) {
                (None, _) => "none\n".into(),
                (Some(_), _) if lock => "lock\n".into(),
                (Some(_), Some(ns)) => format!("layer {ns}\n"),
                (Some(_), None) => "window\n".into(),
            }
        }
        "quit" => {
            state.loop_signal.stop();
            "ok\n".into()
        }
        c => format!("unknown command {c:?}\n"),
    };
    let _ = stream.write_all(reply.as_bytes());
}
