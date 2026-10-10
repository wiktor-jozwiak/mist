// System tray (StatusNotifierItem) for the native bar: a host that lists the items registered with the
// org.kde.StatusNotifierWatcher (Quickshell runs one today; when nobody owns the name, this thread becomes
// the watcher itself), reads their icons (pixmap data or an icon name), activates them and fetches their
// com.canonical.dbusmenu menus. Runs on its own thread with a blocking zbus connection; the bar gets
// Msg::Tray lists and sends Req back (activate, open a menu, click a menu entry).
use crate::sys::Msg;
use calloop::channel::Sender;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

#[derive(Clone, Debug, Default)]
pub struct Item {
    pub key: String,        // "service/path"
    pub id: String,
    pub icon_name: String,
    pub theme_path: String,
    pub pixmap: Option<(u32, u32, Vec<u8>)>, // largest pixmap, premultiplied RGBA
    pub menu: Option<String>,
    pub item_is_menu: bool,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub toggle: Option<bool>, // checkmark / radio state
    pub submenu: Vec<MenuEntry>,
}

pub enum Req {
    Activate(String, i32, i32),
    Secondary(String),
    Menu(String),            // fetch the menu of an item -> Msg::TrayMenu via the callback
    Click(String, i32),      // menu entry clicked
}

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

// a minimal watcher, used only when no one else provides it
struct Watcher {
    items: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: String, #[zbus(header)] h: zbus::message::Header<'_>) {
        let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
        let key = if service.starts_with('/') { format!("{sender}{service}") } else { service };
        let mut v = self.items.lock().unwrap();
        if !v.contains(&key) {
            v.push(key);
        }
    }
    fn register_status_notifier_host(&self, _service: String) {}
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

fn split_key(k: &str) -> (String, String) {
    match k.find('/') {
        Some(i) => (k[..i].to_string(), k[i..].to_string()),
        None => (k.to_string(), "/StatusNotifierItem".to_string()),
    }
}

fn prop(c: &Connection, dest: &str, path: &str, iface: &str, name: &str) -> Option<OwnedValue> {
    c.call_method(Some(dest), path, Some("org.freedesktop.DBus.Properties"), "Get", &(iface, name)).ok()?.body().deserialize::<OwnedValue>().ok()
}

fn read_item(c: &Connection, key: &str) -> Option<Item> {
    let (svc, path) = split_key(key);
    let m = c.call_method(Some(svc.as_str()), path.as_str(), Some("org.freedesktop.DBus.Properties"), "GetAll", &("org.kde.StatusNotifierItem",)).ok()?;
    let p: HashMap<String, OwnedValue> = m.body().deserialize().ok()?;
    let s = |k: &str| p.get(k).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default();
    let mut it = Item { key: key.to_string(), id: s("Id"), icon_name: s("IconName"), theme_path: s("IconThemePath"), status: s("Status"), ..Default::default() };
    it.item_is_menu = p.get("ItemIsMenu").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
    it.menu = p.get("Menu").and_then(|v| OwnedObjectPath::try_from(v.clone()).ok()).map(|o| o.to_string());
    if let Some(v) = p.get("IconPixmap") {
        if let Ok(list) = <Vec<(i32, i32, Vec<u8>)>>::try_from(v.clone()) {
            // the largest one; ARGB32 in network byte order
            if let Some((w, h, d)) = list.into_iter().filter(|(w, h, d)| *w > 0 && *h > 0 && d.len() >= (*w * *h * 4) as usize).max_by_key(|(w, _, _)| *w) {
                let mut rgba = vec![0u8; (w * h * 4) as usize];
                for (o, i) in rgba.chunks_exact_mut(4).zip(d.chunks_exact(4)) {
                    let a = i[0] as u32;
                    o[0] = (i[1] as u32 * a / 255) as u8;
                    o[1] = (i[2] as u32 * a / 255) as u8;
                    o[2] = (i[3] as u32 * a / 255) as u8;
                    o[3] = a as u8;
                }
                it.pixmap = Some((w as u32, h as u32, rgba));
            }
        }
    }
    Some(it)
}

fn menu_layout(c: &Connection, dest: &str, path: &str) -> Vec<MenuEntry> {
    let _ = c.call_method(Some(dest), path, Some("com.canonical.dbusmenu"), "AboutToShow", &(0i32,));
    let props: Vec<&str> = vec!["label", "enabled", "visible", "type", "toggle-type", "toggle-state", "children-display"];
    let Ok(m) = c.call_method(Some(dest), path, Some("com.canonical.dbusmenu"), "GetLayout", &(0i32, -1i32, props)) else { return Vec::new() };
    let body = m.body();
    let Ok((_rev, root)): Result<(u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)), _> = body.deserialize() else { return Vec::new() };
    fn conv(children: &[OwnedValue]) -> Vec<MenuEntry> {
        let mut out = Vec::new();
        for ch in children {
            let v: &Value = ch;
            let inner = match v {
                Value::Value(b) => b.as_ref(),
                other => other,
            };
            let Value::Structure(st) = inner else { continue };
            let f = st.fields();
            if f.len() < 3 {
                continue;
            }
            let id = i32::try_from(&f[0]).unwrap_or(0);
            let mut label = String::new();
            let (mut enabled, mut visible, mut sep, mut toggle) = (true, true, false, None);
            if let Value::Dict(d) = &f[1] {
                for (k, val) in d.iter() {
                    let Value::Str(k) = k else { continue };
                    let val = match val { Value::Value(b) => b.as_ref(), o => o };
                    match k.as_str() {
                        "label" => if let Value::Str(s) = val { label = s.replace('_', "") },
                        "enabled" => if let Value::Bool(b) = val { enabled = *b },
                        "visible" => if let Value::Bool(b) = val { visible = *b },
                        "type" => if let Value::Str(s) = val { sep = s.as_str() == "separator" },
                        "toggle-state" => if let Value::I32(i) = val { toggle = Some(*i == 1) },
                        _ => {}
                    }
                }
            }
            let sub = if let Value::Array(a) = &f[2] { conv(&a.iter().map(|x| OwnedValue::try_from(x.try_clone().unwrap()).unwrap()).collect::<Vec<_>>()) } else { Vec::new() };
            if visible {
                out.push(MenuEntry { id, label, enabled, separator: sep, toggle, submenu: sub });
            }
        }
        out
    }
    conv(&root.2)
}

pub fn spawn(tx: Sender<Msg>, reqs: Receiver<Req>, menus: Sender<(String, Vec<MenuEntry>)>) {
    let _ = std::thread::Builder::new().name("tray".into()).stack_size(512 * 1024).spawn(move || {
        let Ok(c) = Connection::session() else { return };
        let host = format!("org.kde.StatusNotifierHost-{}-1", std::process::id());
        let _ = c.request_name(host.as_str());
        let own_items = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let mut own_watcher = false;
        let list = |c: &Connection, own: bool, own_items: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| -> Vec<String> {
            if own {
                return own_items.lock().unwrap().clone();
            }
            prop(c, WATCHER, WATCHER_PATH, WATCHER, "RegisteredStatusNotifierItems").and_then(|v| <Vec<String>>::try_from(v).ok()).unwrap_or_default()
        };
        let mut become_watcher = |c: &Connection| -> bool {
            if c.object_server().at(WATCHER_PATH, Watcher { items: own_items.clone() }).is_err() {
                return false;
            }
            c.request_name(WATCHER).is_ok()
        };
        // register as host with the watcher (or become it)
        if c.call_method(Some(WATCHER), WATCHER_PATH, Some(WATCHER), "RegisterStatusNotifierHost", &(host.as_str(),)).is_err() {
            own_watcher = become_watcher(&c);
        }
        for r in [
            "type='signal',interface='org.kde.StatusNotifierWatcher'",
            "type='signal',interface='org.kde.StatusNotifierItem'",
            "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged'",
        ] {
            let _ = c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(r,));
        }
        let mut items: Vec<Item> = Vec::new();
        let refresh = |c: &Connection, own: bool, items: &mut Vec<Item>| {
            let keys = list(c, own, &own_items);
            *items = keys.iter().filter_map(|k| read_item(c, k)).filter(|i| i.status != "Passive").collect();
            let _ = tx.send(Msg::Tray(items.clone()));
        };
        refresh(&c, own_watcher, &mut items);

        // signals arrive on the connection's message stream; requests from the bar on `reqs`
        let stream = zbus::blocking::MessageIterator::from(&c);
        let (sig_tx, sig_rx) = std::sync::mpsc::channel::<String>();
        let _ = std::thread::Builder::new().name("tray-sig".into()).stack_size(256 * 1024).spawn(move || {
            for m in stream {
                let Ok(m) = m else { continue };
                let h = m.header();
                if h.message_type() != zbus::message::Type::Signal {
                    continue;
                }
                let member = h.member().map(|x| x.to_string()).unwrap_or_default();
                if member == "NameOwnerChanged" {
                    if let Ok((name, _old, new)) = m.body().deserialize::<(String, String, String)>() {
                        if name == WATCHER && new.is_empty() {
                            let _ = sig_tx.send("watcher-gone".into());
                        } else if name.starts_with(':') && new.is_empty() {
                            let _ = sig_tx.send(format!("gone {name}"));
                        }
                    }
                } else {
                    let _ = sig_tx.send(member);
                }
            }
        });
        loop {
            match reqs.recv_timeout(Duration::from_millis(250)) {
                Ok(Req::Activate(k, x, y)) => {
                    let (s, p) = split_key(&k);
                    let _ = c.call_method(Some(s.as_str()), p.as_str(), Some("org.kde.StatusNotifierItem"), "Activate", &(x, y));
                }
                Ok(Req::Secondary(k)) => {
                    let (s, p) = split_key(&k);
                    let _ = c.call_method(Some(s.as_str()), p.as_str(), Some("org.kde.StatusNotifierItem"), "SecondaryActivate", &(0i32, 0i32));
                }
                Ok(Req::Menu(k)) => {
                    let (s, _) = split_key(&k);
                    let entries = items.iter().find(|i| i.key == k).and_then(|i| i.menu.clone()).map(|mp| menu_layout(&c, &s, &mp)).unwrap_or_default();
                    let _ = menus.send((k, entries));
                }
                Ok(Req::Click(k, id)) => {
                    let (s, _) = split_key(&k);
                    if let Some(mp) = items.iter().find(|i| i.key == k).and_then(|i| i.menu.clone()) {
                        let ts = 0u32;
                        let _ = c.call_method(Some(s.as_str()), mp.as_str(), Some("com.canonical.dbusmenu"), "Event", &(id, "clicked", Value::from(0i32), ts));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            // coalesce the signals that arrived meanwhile into one refresh
            let mut dirty = false;
            while let Ok(s) = sig_rx.try_recv() {
                if s == "watcher-gone" && !own_watcher {
                    own_watcher = become_watcher(&c);
                    // items re-register with the new watcher
                }
                if let Some(name) = s.strip_prefix("gone ") {
                    if own_watcher {
                        own_items.lock().unwrap().retain(|k| !k.starts_with(name));
                    }
                    if !items.iter().any(|i| i.key.starts_with(name)) {
                        continue;
                    }
                }
                dirty = true;
            }
            if dirty {
                refresh(&c, own_watcher, &mut items);
            }
        }
    });
}
