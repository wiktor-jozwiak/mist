// Mist's notification server (org.freedesktop.Notifications, was Notifs.qml): every notification goes to the
// island (Msg::Note) and to the history ~/.local/state/mist/notifications.json (same format as before:
// {dnd, entries[{id, app_name, summary, body, desktop_entry, icon, received_wall_ms, urgency, image, image_w}]},
// newest first, at most 300). Pictures sent as data (image-data) are saved as PNG in notif-img/.
// Do Not Disturb: only critical ones reach the island and make a sound. A notification stays alive (its
// actions can be invoked) until the app closes it or 10 minutes pass (then: expired), unless resident.
use crate::sys::Msg;
use calloop::channel::Sender;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value as ZV};

#[derive(Clone, Debug, Default)]
pub struct Note {
    pub id: u32,
    pub app: String,
    pub icon: String,     // app icon name / path / file:// image
    pub summary: String,
    pub body: String,
    pub urgency: u8,      // 0 low, 1 normal, 2 critical
    pub entry: String,
    pub value: i32,       // progress 0..100, -1 none
    pub sync: String,
    pub actions: Vec<(String, String)>,
    pub replaced: bool,
    pub inline_reply: bool,
}

pub enum Cmd {
    Invoke(u32, String),
    Dnd(bool),
    Clear,
}

struct Shared {
    next: u32,
    live: HashMap<u32, (Note, std::time::Instant, bool)>, // note, since, resident
    dnd: bool,
    history: Vec<Value>,
}

fn state_dir() -> String {
    format!("{}/.local/state/mist", std::env::var("HOME").unwrap_or_default())
}
fn save(sh: &Shared) {
    let d = state_dir();
    let tmp = format!("{d}/.notifications.json.tmp");
    if std::fs::write(&tmp, json!({ "dnd": sh.dnd, "entries": sh.history }).to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, format!("{d}/notifications.json"));
    }
}

struct Server {
    sh: Arc<Mutex<Shared>>,
    tx: Sender<Msg>,
}

fn hint_str(h: &HashMap<String, OwnedValue>, k: &str) -> String {
    h.get(k).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
}
fn hint_int(h: &HashMap<String, OwnedValue>, k: &str) -> Option<i64> {
    let v: &ZV = h.get(k)?;
    match v {
        ZV::U8(x) => Some(*x as i64),
        ZV::I32(x) => Some(*x as i64),
        ZV::U32(x) => Some(*x as i64),
        ZV::I64(x) => Some(*x),
        ZV::U16(x) => Some(*x as i64),
        ZV::I16(x) => Some(*x as i64),
        _ => None,
    }
}
fn hint_bool(h: &HashMap<String, OwnedValue>, k: &str) -> bool {
    h.get(k).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
}

// image-data (iiibiiay) -> PNG file, returns (file path, width)
fn save_image(v: &ZV, id: u32) -> Option<(String, u32)> {
    let ZV::Structure(s) = v else { return None };
    let f = s.fields();
    if f.len() < 7 {
        return None;
    }
    let w = i32::try_from(&f[0]).ok()? as u32;
    let h = i32::try_from(&f[1]).ok()? as u32;
    let stride = i32::try_from(&f[2]).ok()? as usize;
    let alpha = bool::try_from(&f[3]).ok()?;
    let ch = i32::try_from(&f[5]).ok()? as usize;
    let ZV::Array(a) = &f[6] else { return None };
    let data: Vec<u8> = a.iter().filter_map(|x| u8::try_from(x).ok()).collect();
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    let px = pm.data_mut();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * stride + x * ch;
            if i + ch > data.len() {
                continue;
            }
            let a = if alpha && ch >= 4 { data[i + 3] as u32 } else { 255 };
            let o = (y * w as usize + x) * 4;
            px[o] = (data[i] as u32 * a / 255) as u8;
            px[o + 1] = (data[i + 1] as u32 * a / 255) as u8;
            px[o + 2] = (data[i + 2] as u32 * a / 255) as u8;
            px[o + 3] = a as u8;
        }
    }
    let dir = format!("{}/notif-img", state_dir());
    let _ = std::fs::create_dir_all(&dir);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = format!("{dir}/n{ms}_{id}.png");
    pm.save_png(&path).ok()?;
    Some((path, w))
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<&str> {
        vec!["body", "body-markup", "actions", "icon-static", "persistence", "inline-reply", "x-kde-reply"]
    }
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Mist", "mist", "1.0", "1.2")
    }
    #[allow(clippy::too_many_arguments)]
    async fn notify(&self, app_name: String, replaces_id: u32, app_icon: String, summary: String, body: String, actions: Vec<String>, hints: HashMap<String, OwnedValue>, _expire_timeout: i32) -> u32 {
        let (id, replaced, dnd) = {
            let mut sh = self.sh.lock().unwrap();
            let replaced = replaces_id > 0 && sh.live.contains_key(&replaces_id);
            let id = if replaced { replaces_id } else { sh.next += 1; sh.next };
            (id, replaced, sh.dnd)
        };
        let mut n = Note {
            id,
            app: app_name.clone(),
            summary,
            body,
            urgency: hint_int(&hints, "urgency").unwrap_or(1).clamp(0, 2) as u8,
            entry: hint_str(&hints, "desktop-entry"),
            value: hint_int(&hints, "value").map(|v| v as i32).unwrap_or(-1),
            sync: [hint_str(&hints, "x-canonical-private-synchronous"), hint_str(&hints, "x-dunst-stack-tag"), hint_str(&hints, "synchronous")].into_iter().find(|s| !s.is_empty()).unwrap_or_default(),
            actions: actions.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
            replaced,
            ..Default::default()
        };
        n.inline_reply = n.actions.iter().any(|a| a.0 == "inline-reply");
        // picture: image-data (saved) > image-path > app_icon
        let mut img = String::new();
        let mut img_w = 0;
        for k in ["image-data", "image_data", "icon_data"] {
            if let Some(v) = hints.get(k) {
                if let Some((p, w)) = save_image(v, id) {
                    img = format!("file://{p}");
                    img_w = w;
                }
                break;
            }
        }
        let ip = hint_str(&hints, "image-path");
        n.icon = if !img.is_empty() { img.clone() } else if !ip.is_empty() { ip.clone() } else { app_icon.clone() };
        let resident = hint_bool(&hints, "resident");
        let transient = hint_bool(&hints, "transient");
        let urgent = n.urgency == 2;
        {
            let mut sh = self.sh.lock().unwrap();
            sh.live.insert(id, (n.clone(), std::time::Instant::now(), resident));
            if !transient && !replaced {
                let (hi, hw) = if !img.is_empty() {
                    (img.clone(), img_w)
                } else if ip.starts_with('/') {
                    (format!("file://{ip}"), 256)
                } else if ip.starts_with("file:") {
                    (ip.clone(), 256)
                } else {
                    (String::new(), 0)
                };
                let e = json!({ "id": id, "app_name": app_name, "summary": n.summary, "body": n.body, "desktop_entry": n.entry,
                    "icon": if app_icon.is_empty() && !ip.starts_with('/') { ip.clone() } else { app_icon.clone() },
                    "received_wall_ms": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
                    "urgency": (["low", "normal", "critical"][n.urgency as usize]), "image": hi, "image_w": hw });
                sh.history.insert(0, e);
                // pictures of entries that fall off the end
                let gone: Vec<String> = sh.history.iter().skip(300).filter_map(|e| e["image"].as_str().map(String::from)).filter(|s| s.contains("/notif-img/")).collect();
                for g in gone {
                    let _ = std::fs::remove_file(g.trim_start_matches("file://"));
                }
                sh.history.truncate(300);
                save(&sh);
            }
        }
        if !dnd || urgent {
            if !replaced && !hint_bool(&hints, "suppress-sound") && n.urgency > 0 {
                let home = std::env::var("HOME").unwrap_or_default();
                let custom = format!("{home}/.config/quickshell/mist/sounds/{}.wav", if urgent { "urgent" } else { "message" });
                let snd = if std::path::Path::new(&custom).exists() {
                    custom
                } else {
                    format!("/usr/share/sounds/freedesktop/stereo/{}.oga", if urgent { "dialog-warning" } else { "message" })
                };
                crate::app::spawn(&["pw-play", "--volume=0.7", &snd]);
            }
            let _ = self.tx.send(Msg::Note(n));
        }
        id
    }
    async fn close_notification(&self, id: u32, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        if self.sh.lock().unwrap().live.remove(&id).is_some() {
            let _ = Self::notification_closed(&em, id, 3).await;
        }
    }
    #[zbus(signal)]
    async fn notification_closed(em: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn action_invoked(em: &SignalEmitter<'_>, id: u32, action_key: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn notification_replied(em: &SignalEmitter<'_>, id: u32, text: &str) -> zbus::Result<()>;
}

pub fn spawn(tx: Sender<Msg>, cmds: std::sync::mpsc::Receiver<Cmd>) {
    let _ = std::thread::Builder::new().name("notif".into()).stack_size(512 * 1024).spawn(move || {
        let st: Value = std::fs::read_to_string(format!("{}/notifications.json", state_dir())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        let history = st["entries"].as_array().cloned().unwrap_or_default();
        let next = history.iter().filter_map(|e| e["id"].as_u64()).max().unwrap_or(0) as u32;
        let sh = Arc::new(Mutex::new(Shared { next, live: HashMap::new(), dnd: st["dnd"].as_bool().unwrap_or(false), history }));
        let _ = std::fs::create_dir_all(format!("{}/notif-img", state_dir()));
        let server = Server { sh: sh.clone(), tx: tx.clone() };
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
            .and_then(|b| b.build());
        let Ok(conn) = conn else { eprintln!("notif: no session bus"); return };
        // the name may still belong to Quickshell for a moment: retry
        let mut owned = false;
        for _ in 0..50 {
            if conn.request_name("org.freedesktop.Notifications").is_ok() {
                owned = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if !owned {
            eprintln!("notif: org.freedesktop.Notifications is taken");
        }
        let iface = || conn.object_server().interface::<_, Server>("/org/freedesktop/Notifications").ok();
        loop {
            match cmds.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok(Cmd::Invoke(id, key)) => {
                    let n = sh.lock().unwrap().live.get(&id).cloned();
                    if let (Some((n, _, resident)), Some(i)) = (n, iface()) {
                        if n.actions.iter().any(|a| a.0 == key) {
                            let _ = zbus::block_on(Server::action_invoked(i.signal_emitter(), id, &key));
                            if !resident {
                                sh.lock().unwrap().live.remove(&id);
                                let _ = zbus::block_on(Server::notification_closed(i.signal_emitter(), id, 2));
                            }
                        }
                    }
                }
                Ok(Cmd::Dnd(on)) => {
                    let mut s = sh.lock().unwrap();
                    s.dnd = on;
                    save(&s);
                }
                Ok(Cmd::Clear) => {
                    let mut s = sh.lock().unwrap();
                    for e in &s.history {
                        if let Some(p) = e["image"].as_str() {
                            if p.contains("/notif-img/") {
                                let _ = std::fs::remove_file(p.trim_start_matches("file://"));
                            }
                        }
                    }
                    s.history.clear();
                    save(&s);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            // expire the old ones (10 min), unless resident
            let old: Vec<u32> = sh.lock().unwrap().live.iter().filter(|(_, v)| !v.2 && v.1.elapsed().as_secs() > 600).map(|(k, _)| *k).collect();
            for id in old {
                sh.lock().unwrap().live.remove(&id);
                if let Some(i) = iface() {
                    let _ = zbus::block_on(Server::notification_closed(i.signal_emitter(), id, 1));
                }
            }
        }
    });
}
