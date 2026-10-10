// Data sources of mist-ui, each on its own thread, all event driven (no polling): Hyprland's event
// socket, NetworkManager and BlueZ over D-Bus, `pactl subscribe` for the default sink, inotify on the
// state files Quickshell still writes (phone, open popup, Do Not Disturb), `mistd stats` for CPU / RAM / net.
// Every change becomes one Msg on the main loop's channel.
use calloop::channel::Sender;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Win {
    pub class: String,
    pub title: String,
    pub monitor: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Hypr {
    pub active: Option<Win>,
    pub monitors: Vec<(i64, String, i64)>, // id, name, active workspace id
    pub ws_windows: Vec<(i64, i64)>,       // workspace id, window count
}

pub enum Msg {
    Hypr(Hypr),
    Net(String),
    Bt(bool, Option<Option<f32>>),
    Volume(f32, bool),
    Stats(i32, f32, f32, f64),
    Phone(Option<crate::bar::Phone>),
    Ui(String, String),
    Dnd(bool),
    Tray(Vec<crate::tray::Item>),
}

fn spawn<F: FnOnce() + Send + 'static>(name: &str, f: F) {
    let _ = std::thread::Builder::new().name(name.into()).stack_size(256 * 1024).spawn(f);
}

// ------------------------------------------------------------------ Hyprland
fn hypr_dir() -> String {
    format!("{}/hypr/{}", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(), std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default())
}
pub fn hypr_request(cmd: &str) -> Option<Value> {
    let mut s = UnixStream::connect(format!("{}/.socket.sock", hypr_dir())).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    serde_json::from_str(&out).ok()
}
fn hypr_state() -> Hypr {
    let mut h = Hypr::default();
    if let Some(a) = hypr_request("j/activewindow") {
        if a.get("address").is_some() {
            h.active = Some(Win {
                class: a["class"].as_str().unwrap_or("").into(),
                title: a["title"].as_str().unwrap_or("").into(),
                monitor: a["monitor"].as_i64().unwrap_or(-1),
            });
        }
    }
    if let Some(Value::Array(ms)) = hypr_request("j/monitors") {
        for m in ms {
            h.monitors.push((m["id"].as_i64().unwrap_or(0), m["name"].as_str().unwrap_or("").into(), m["activeWorkspace"]["id"].as_i64().unwrap_or(0)));
        }
    }
    if let Some(Value::Array(ws)) = hypr_request("j/workspaces") {
        for w in ws {
            h.ws_windows.push((w["id"].as_i64().unwrap_or(0), w["windows"].as_i64().unwrap_or(0)));
        }
    }
    h
}
pub fn hyprland(tx: Sender<Msg>) {
    spawn("hypr", move || loop {
        let _ = tx.send(Msg::Hypr(hypr_state()));
        let Ok(s) = UnixStream::connect(format!("{}/.socket2.sock", hypr_dir())) else {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        let mut r = BufReader::new(s);
        let mut line = String::new();
        loop {
            line.clear();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let ev = line.split(">>").next().unwrap_or("");
            if matches!(ev, "activewindow" | "windowtitlev2" | "workspacev2" | "focusedmonv2" | "openwindow" | "closewindow" | "movewindowv2"
                | "createworkspacev2" | "destroyworkspacev2" | "moveworkspacev2" | "monitoraddedv2" | "monitorremovedv2" | "activespecialv2")
            {
                // coalesce bursts (a workspace switch sends several events)
                std::thread::sleep(Duration::from_millis(15));
                let _ = tx.send(Msg::Hypr(hypr_state()));
            }
        }
    });
}

// ------------------------------------------------------------------ NetworkManager
pub fn network(tx: Sender<Msg>) {
    spawn("net", move || {
        let Ok(c) = zbus::blocking::Connection::system() else { return };
        let get = || -> String {
            let t: String = c
                .call_method(Some("org.freedesktop.NetworkManager"), "/org/freedesktop/NetworkManager", Some("org.freedesktop.DBus.Properties"), "Get", &("org.freedesktop.NetworkManager", "PrimaryConnectionType"))
                .ok()
                .and_then(|m| m.body().deserialize::<zbus::zvariant::OwnedValue>().ok())
                .and_then(|v| String::try_from(v).ok())
                .unwrap_or_default();
            if t.contains("ethernet") { "ethernet".into() } else if t.contains("wireless") { "wifi".into() } else if t.is_empty() { "none".into() } else { "ethernet".into() }
        };
        let _ = tx.send(Msg::Net(get()));
        let rule = "type='signal',sender='org.freedesktop.NetworkManager',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/freedesktop/NetworkManager'";
        let _ = c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(rule,));
        let mut last = String::new();
        for m in zbus::blocking::MessageIterator::from(&c) {
            let Ok(m) = m else { continue };
            if m.header().member().map(|x| x.as_str()) != Some("PropertiesChanged") {
                continue;
            }
            let n = get();
            if n != last {
                last = n.clone();
                let _ = tx.send(Msg::Net(n));
            }
        }
    });
}

// ------------------------------------------------------------------ BlueZ
pub fn bluetooth(tx: Sender<Msg>) {
    spawn("bt", move || {
        let Ok(c) = zbus::blocking::Connection::system() else { return };
        type Props = std::collections::HashMap<String, zbus::zvariant::OwnedValue>;
        type Objs = std::collections::HashMap<zbus::zvariant::OwnedObjectPath, std::collections::HashMap<String, Props>>;
        let get = || -> (bool, Option<Option<f32>>) {
            let Ok(m) = c.call_method(Some("org.bluez"), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &()) else { return (false, None) };
            let Ok(objs) = m.body().deserialize::<Objs>() else { return (false, None) };
            let b = |p: &Props, k: &str| p.get(k).and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
            let on = objs.values().any(|i| i.get("org.bluez.Adapter1").map(|p| b(p, "Powered")).unwrap_or(false));
            let dev = objs.values().find(|i| i.get("org.bluez.Device1").map(|p| b(p, "Connected")).unwrap_or(false)).map(|i| {
                i.get("org.bluez.Battery1").and_then(|p| p.get("Percentage")).and_then(|v| u8::try_from(v).ok()).map(|v| v as f32 / 100.0)
            });
            (on, dev)
        };
        let (on, dev) = get();
        let _ = tx.send(Msg::Bt(on, dev));
        let rule = "type='signal',sender='org.bluez'";
        let _ = c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(rule,));
        for m in zbus::blocking::MessageIterator::from(&c) {
            if m.is_err() {
                continue;
            }
            let (on, dev) = get();
            let _ = tx.send(Msg::Bt(on, dev));
        }
    });
}

// ------------------------------------------------------------------ default sink volume (PipeWire via pactl)
fn volume() -> (f32, bool) {
    let out = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    // "Volume: 0.50 [MUTED]"
    let v = out.split_whitespace().nth(1).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.0);
    (v, out.contains("MUTED"))
}
pub fn audio(tx: Sender<Msg>) {
    spawn("audio", move || loop {
        let (v, m) = volume();
        let _ = tx.send(Msg::Volume(v, m));
        let Ok(mut ch) = Command::new("pactl").arg("subscribe").stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return };
        let r = BufReader::new(ch.stdout.take().unwrap());
        let mut last = (v, m);
        for l in r.lines() {
            let Ok(l) = l else { break };
            if l.contains("on sink #") || l.contains("on server") {
                let now = volume();
                if now != last {
                    last = now;
                    let _ = tx.send(Msg::Volume(now.0, now.1));
                }
            }
        }
        let _ = ch.wait();
        std::thread::sleep(Duration::from_secs(2));
    });
}

// ------------------------------------------------------------------ mistd stats
pub fn stats(tx: Sender<Msg>) {
    spawn("stats", move || loop {
        let bin = format!("{}/.config/quickshell/mist/scripts/bin/mistd", std::env::var("HOME").unwrap_or_default());
        let Ok(mut ch) = Command::new(bin).arg("stats").stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return };
        for l in BufReader::new(ch.stdout.take().unwrap()).lines() {
            let Ok(l) = l else { break };
            let v: Vec<&str> = l.split(' ').collect();
            if v.len() >= 5 {
                let _ = tx.send(Msg::Stats(v[0].parse().unwrap_or(0), v[1].parse().unwrap_or(0.0), v[2].parse().unwrap_or(0.0), v[3].parse().unwrap_or(0.0)));
            }
        }
        let _ = ch.wait();
        std::thread::sleep(Duration::from_secs(2));
    });
}

// ------------------------------------------------------------------ state files written by Quickshell (inotify)
fn read_json(p: &str) -> Value {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}
pub fn state_files(tx: Sender<Msg>) {
    spawn("state", move || {
        let run = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default());
        let st = format!("{}/.local/state/mist", std::env::var("HOME").unwrap_or_default());
        let _ = std::fs::create_dir_all(&run);
        let send_all = |which: &str| {
            if which.is_empty() || which == "phone.json" {
                let p = read_json(&format!("{run}/phone.json"));
                let ph = if p["online"].as_bool().unwrap_or(false) || p["pair"].as_bool().unwrap_or(false) {
                    Some(crate::bar::Phone { pair: p["pair"].as_bool().unwrap_or(false), charging: p["charging"].as_bool().unwrap_or(false), battery: p["battery"].as_i64().unwrap_or(-1) as i32 })
                } else {
                    None
                };
                let _ = tx.send(Msg::Phone(ph));
            }
            if which.is_empty() || which == "ui.json" {
                let u = read_json(&format!("{run}/ui.json"));
                let _ = tx.send(Msg::Ui(u["popup"].as_str().unwrap_or("").into(), u["screen"].as_str().unwrap_or("").into()));
            }
            if which.is_empty() || which == "notifications.json" {
                // only "dnd" matters; the file also holds the history
                let n = read_json(&format!("{st}/notifications.json"));
                let _ = tx.send(Msg::Dnd(n["dnd"].as_bool().unwrap_or(false)));
            }
        };
        send_all("");
        unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC);
            for d in [&run, &st] {
                let c = std::ffi::CString::new(d.as_str()).unwrap();
                libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO);
            }
            let mut buf = [0u8; 4096];
            loop {
                let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                if n <= 0 {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let mut i = 0usize;
                while i < n as usize {
                    let ev = &*(buf.as_ptr().add(i) as *const libc::inotify_event);
                    let name_len = ev.len as usize;
                    let name = std::ffi::CStr::from_ptr(buf.as_ptr().add(i + std::mem::size_of::<libc::inotify_event>()) as *const libc::c_char).to_string_lossy().to_string();
                    if matches!(name.as_str(), "phone.json" | "ui.json" | "notifications.json") {
                        send_all(&name);
                    }
                    i += std::mem::size_of::<libc::inotify_event>() + name_len;
                }
            }
        }
    });
}
