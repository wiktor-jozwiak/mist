// Mist Settings data: what the pages show and the commands they run. Slow reads (pactl, BlueZ, nmcli,
// checkupdates, xdg-mime, the system summary) go to one worker thread; its answers come back on the app's
// channel (appwin Cx::tx) as `Res`. Small config files (settings.json, idle.json, wallpaper.json, comp.conf,
// outputs) are read and written here directly, every write atomic (tmp + rename) so mist-ui's inotify sees one
// finished file; the shell is told with `mist-ui-bin msg ui settings`, the compositor with `action reload`.
use crate::control::{Bt, Net, Snd};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};

pub enum Req {
    Snd,
    Bt,
    Net,
    Info,
    Updates,
    Mime,
    Time,
    Tzs,
    Profile,
    Mons,
    // a command line, then the named state again
    Run(Vec<String>, &'static str),
    BtPower(bool),
    BtScan(bool),
    BtConnect(String, bool),
    BtPair(String),
    // join a network: a saved one by name, a new one with the password on nmcli's stdin (--ask), never in argv
    Wifi(String, Option<String>),
    // places matching the text (`mistd geo`)
    Geo(String),
}

pub enum Res {
    Snd(Snd),
    Bt(Bt),
    Net(Net, Vec<Wifi>),
    Info(Vec<(String, String)>),
    Updates(Result<Vec<String>, String>, String),
    Mime(Vec<(String, Vec<String>)>),
    Time(String, bool),
    Tzs(Vec<String>),
    Profile(String, Vec<String>),
    Mons(Vec<Mon>),
    BtDone,
    // the join failed: nmcli's message
    WifiErr(String),
    // the asked text, the places found
    Geo(String, Vec<Value>),
}

#[derive(Clone, Default, Debug)]
pub struct Wifi {
    pub on: bool,
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub known: bool,
}

#[derive(Clone, Default, Debug)]
pub struct Mon {
    pub name: String,
    pub desc: String,
    pub w: i64,
    pub h: i64,
    pub hz: f64,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
}

// the categories of Aplikacje > Domyślne aplikacje: label, icon, mime types (the first one is asked)
pub const MIME_CATS: [(&str, &str, &[&str]); 8] = [
    ("Przeglądarka", "world", &["x-scheme-handler/http", "x-scheme-handler/https", "text/html", "application/xhtml+xml"]),
    ("Pliki", "folder", &["inode/directory"]),
    ("Poczta", "mail", &["x-scheme-handler/mailto"]),
    ("Muzyka", "music", &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav", "audio/mp4"]),
    ("Wideo", "video", &["video/mp4", "video/x-matroska", "video/webm", "video/quicktime"]),
    ("Zdjęcia", "photo", &["image/png", "image/jpeg", "image/webp", "image/gif"]),
    ("Edytor tekstu", "file", &["text/plain"]),
    ("Dokumenty PDF", "file", &["application/pdf"]),
];

// where the answers go: the app's calloop channel, or a plain mpsc one for `--png`
pub type Out = std::sync::Arc<dyn Fn(Box<dyn std::any::Any + Send>) -> bool + Send + Sync>;
pub fn start(out: Out) -> Sender<Req> {
    let (tx, rx) = channel();
    let _ = std::thread::Builder::new().name("settings".into()).stack_size(512 * 1024).spawn(move || worker(rx, out));
    tx
}

fn sh(cmd: &str) -> String {
    Command::new("sh").args(["-c", cmd]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

fn worker(rx: Receiver<Req>, out: Out) {
    let bus = zbus::blocking::Connection::system().ok();
    let send = |r: Res| out(Box::new(r));
    while let Ok(first) = rx.recv() {
        // coalesce: a dragged slider sends many volumes, the same state is asked once
        let mut reqs = vec![first];
        while let Ok(r) = rx.try_recv() {
            reqs.push(r);
        }
        let mut want: Vec<&'static str> = Vec::new();
        let mut runs: Vec<Vec<String>> = Vec::new();
        for r in reqs {
            let w = match r {
                Req::Snd => "snd",
                Req::Bt => "bt",
                Req::Net => "net",
                Req::Info => "info",
                Req::Updates => "updates",
                Req::Mime => "mime",
                Req::Time => "time",
                Req::Tzs => "tzs",
                Req::Profile => "profile",
                Req::Mons => "mons",
                Req::Run(c, then) => {
                    // only the last of the same command with another value (pactl set-sink-volume X 40%)
                    runs.retain(|p| p.len() != c.len() || p[..p.len() - 1] != c[..c.len() - 1]);
                    runs.push(c);
                    then
                }
                Req::BtPower(on) => {
                    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(crate::control::bt_adapter)) {
                        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &("org.bluez.Adapter1", "Powered", zbus::zvariant::Value::from(on)));
                    }
                    "bt"
                }
                Req::BtScan(on) => {
                    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(crate::control::bt_adapter)) {
                        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.bluez.Adapter1"), if on { "StartDiscovery" } else { "StopDiscovery" }, &());
                    }
                    "bt"
                }
                Req::BtConnect(path, on) => {
                    if let Some(c) = bus.clone() {
                        let out = out.clone();
                        let _ = std::thread::Builder::new().name("bt-connect".into()).spawn(move || {
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.bluez.Device1"), if on { "Connect" } else { "Disconnect" }, &());
                            let _ = out(Box::new(Res::BtDone));
                            let _ = out(Box::new(Res::Bt(crate::control::bt_state(&c))));
                        });
                    }
                    ""
                }
                Req::Wifi(ssid, pwd) => {
                    use std::io::Write;
                    let mut c = Command::new("nmcli");
                    match &pwd {
                        None => c.args(["connection", "up", "id", &ssid]),
                        Some(_) => c.args(["--ask", "device", "wifi", "connect", &ssid]),
                    };
                    c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
                    if let Ok(mut ch) = c.spawn() {
                        if let (Some(p), Some(mut i)) = (&pwd, ch.stdin.take()) {
                            let _ = writeln!(i, "{p}");
                        }
                        if let Ok(o) = ch.wait_with_output() {
                            if !o.status.success() {
                                let e = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("nmcli").trim_start_matches("Error: ").to_string();
                                let _ = out(Box::new(Res::WifiErr(e)));
                            }
                        }
                    }
                    "net"
                }
                Req::Geo(q) => {
                    let out = out.clone();
                    let _ = std::thread::Builder::new().name("geo".into()).spawn(move || {
                        let lang = if crate::i18n::setting() == "pl" || std::env::var("LANG").unwrap_or_default().starts_with("pl") { "pl" } else { "en" };
                        let o = Command::new(crate::paths::mistd()).args(["geo", &q, lang]).stderr(std::process::Stdio::null()).output();
                        let v: Vec<Value> = o.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or_default();
                        let _ = out(Box::new(Res::Geo(q, v)));
                    });
                    ""
                }
                Req::BtPair(path) => {
                    if let Some(c) = bus.clone() {
                        let out = out.clone();
                        let _ = std::thread::Builder::new().name("bt-pair".into()).spawn(move || {
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &("org.bluez.Device1", "Trusted", zbus::zvariant::Value::from(true)));
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.bluez.Device1"), "Pair", &());
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.bluez.Device1"), "Connect", &());
                            let _ = out(Box::new(Res::BtDone));
                            let _ = out(Box::new(Res::Bt(crate::control::bt_state(&c))));
                        });
                    }
                    ""
                }
            };
            if !w.is_empty() && !want.contains(&w) {
                want.push(w);
            }
        }
        for c in runs {
            let _ = Command::new(&c[0]).args(&c[1..]).status();
        }
        for w in want {
            let ok = match w {
                "snd" => send(Res::Snd(crate::control::snd_state())),
                "bt" => bus.as_ref().map(|c| send(Res::Bt(crate::control::bt_state(c)))).unwrap_or(true),
                "net" => send(Res::Net(crate::control::net_state(), wifi_list())),
                "info" => send(Res::Info(info())),
                "updates" => {
                    let o = Command::new("checkupdates").output();
                    let r = match o {
                        // 2 = nothing to update
                        Ok(o) if o.status.success() || o.status.code() == Some(2) => Ok(String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()),
                        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("checkupdates").to_string()),
                        Err(e) => Err(e.to_string()),
                    };
                    send(Res::Updates(r, last_upgrade()))
                }
                "mime" => send(Res::Mime(mime_state())),
                "time" => {
                    let s = sh("timedatectl show -p Timezone -p NTP");
                    let get = |k: &str| s.lines().find_map(|l| l.strip_prefix(k)).unwrap_or("").to_string();
                    send(Res::Time(get("Timezone="), get("NTP=") == "yes"))
                }
                "tzs" => send(Res::Tzs(sh("timedatectl list-timezones").lines().map(String::from).collect())),
                "profile" => {
                    let cur = sh("powerprofilesctl get").trim().to_string();
                    let all = sh("powerprofilesctl list").lines().filter_map(|l| l.trim_start_matches(['*', ' ']).strip_suffix(':').map(String::from)).collect();
                    send(Res::Profile(cur, all))
                }
                "mons" => send(Res::Mons(monitors())),
                _ => true,
            };
            if !ok {
                return;
            }
        }
    }
    // the window closed: stop a discovery we started
    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(crate::control::bt_adapter)) {
        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.bluez.Adapter1"), "StopDiscovery", &());
    }
}

pub(crate) fn wifi_list() -> Vec<Wifi> {
    let known: Vec<String> = sh("nmcli -t -f NAME,TYPE connection show").lines().filter(|l| l.ends_with(":802-11-wireless")).map(|l| l.rsplit_once(':').map(|x| x.0).unwrap_or(l).replace("\\:", ":")).collect();
    let mut v: Vec<Wifi> = Vec::new();
    for l in sh("nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list --rescan no").lines() {
        // fields split on unescaped ':'
        let mut f: Vec<String> = vec![String::new()];
        let mut esc = false;
        for ch in l.chars() {
            match (esc, ch) {
                (false, '\\') => esc = true,
                (false, ':') => f.push(String::new()),
                (_, c) => {
                    esc = false;
                    f.last_mut().unwrap().push(c);
                }
            }
        }
        if f.len() < 4 || f[1].is_empty() || v.iter().any(|w| w.ssid == f[1]) {
            continue;
        }
        v.push(Wifi { on: f[0] == "*", known: known.contains(&f[1]), ssid: f[1].clone(), signal: f[2].parse().unwrap_or(0), secure: !f[3].is_empty() && f[3] != "--" });
    }
    v.sort_by(|a, b| b.on.cmp(&a.on).then(b.signal.cmp(&a.signal)));
    v.truncate(12);
    v
}

fn monitors() -> Vec<Mon> {
    let Some(Value::Array(ms)) = crate::sys::hypr_request("j/monitors") else { return Vec::new() };
    let mut v: Vec<Mon> = ms
        .iter()
        .map(|m| Mon {
            name: m["name"].as_str().unwrap_or("").into(),
            desc: [m["make"].as_str().unwrap_or(""), m["model"].as_str().unwrap_or("")].iter().filter(|s| !s.is_empty() && **s != "Unknown").cloned().collect::<Vec<_>>().join(" "),
            w: m["width"].as_i64().unwrap_or(0),
            h: m["height"].as_i64().unwrap_or(0),
            hz: m["refreshRate"].as_f64().unwrap_or(60.0),
            x: m["x"].as_i64().unwrap_or(0),
            y: m["y"].as_i64().unwrap_or(0),
            scale: m["scale"].as_f64().unwrap_or(1.0),
        })
        .collect();
    v.sort_by_key(|m| (m.x, m.y));
    v
}

fn last_upgrade() -> String {
    // the last "starting full system upgrade" in pacman's log: [2026-10-21T19:02:11+0200]
    let log = std::fs::read_to_string("/var/log/pacman.log").unwrap_or_default();
    log.lines().rev().find(|l| l.contains("starting full system upgrade")).and_then(|l| l.get(1..17)).map(|d| d.replace('T', " ")).unwrap_or_default()
}

fn mime_state() -> Vec<(String, Vec<String>)> {
    let mut caches = String::new();
    for d in crate::apps::data_dirs() {
        caches.push_str(&std::fs::read_to_string(format!("{d}/applications/mimeinfo.cache")).unwrap_or_default());
        caches.push('\n');
    }
    MIME_CATS
        .iter()
        .map(|(_, _, mimes)| {
            let cur = sh(&format!("xdg-mime query default {}", mimes[0])).trim().trim_end_matches(".desktop").to_string();
            let mut cands: Vec<String> = Vec::new();
            for m in mimes.iter() {
                for l in caches.lines() {
                    if let Some(r) = l.strip_prefix(m).and_then(|r| r.strip_prefix('=')) {
                        for id in r.split(';').filter(|s| !s.is_empty()) {
                            let id = id.trim_end_matches(".desktop").to_string();
                            if !cands.contains(&id) {
                                cands.push(id);
                            }
                        }
                    }
                }
            }
            if !cur.is_empty() && !cands.contains(&cur) {
                cands.insert(0, cur.clone());
            }
            (cur, cands)
        })
        .collect()
}

fn info() -> Vec<(String, String)> {
    let rd = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let os = rd("/etc/os-release").lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).unwrap_or("Linux").trim_matches('"').to_string();
    let cpuinfo = rd("/proc/cpuinfo");
    let cpu = cpuinfo.lines().find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')).map(|x| x.1.trim().to_string())).unwrap_or_default();
    let threads = cpuinfo.lines().filter(|l| l.starts_with("processor")).count();
    let mem_kb: u64 = rd("/proc/meminfo").lines().find_map(|l| l.strip_prefix("MemTotal:")).and_then(|r| r.split_whitespace().next()?.parse().ok()).unwrap_or(0);
    let mut gpu = sh("nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null").lines().next().map(|l| l.replace(", ", " · sterownik ")).unwrap_or_default();
    if gpu.is_empty() {
        gpu = sh("lspci 2>/dev/null | grep -iE 'vga|3d controller' | head -1").split_once(": ").map(|x| x.1.trim().to_string()).unwrap_or_default();
    }
    let comp = if std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default() == "mist" { "mist-comp".to_string() } else { "Hyprland".to_string() };
    let mist = sh(&format!("git -C {} log -1 --format='%h · %cs' 2>/dev/null", crate::paths::mist_path(""))).trim().to_string();
    vec![
        ("Nazwa urządzenia".into(), rd("/etc/hostname").trim().to_string()),
        ("System".into(), os),
        ("Jądro".into(), rd("/proc/sys/kernel/osrelease").trim().to_string()),
        ("Procesor".into(), if threads > 0 { format!("{cpu} · {threads} wątków") } else { cpu }),
        ("Karta graficzna".into(), gpu),
        ("Pamięć RAM".into(), format!("{:.1} GB", mem_kb as f64 / 1048576.0)),
        ("Kompozytor".into(), comp),
        ("Wersja Mist".into(), if mist.is_empty() { env!("CARGO_PKG_VERSION").to_string() } else { mist }),
    ]
}

// ------------------------------------------------------------------ config files
pub fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
pub fn cfg(name: &str) -> String {
    format!("{}/.config/mist/{name}", home())
}
pub fn write_atomic(path: &str, s: &str) {
    if let Some(d) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, s).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}
pub fn read_json(name: &str) -> Value {
    std::fs::read_to_string(cfg(name)).ok().and_then(|s| serde_json::from_str(&s).ok()).filter(|v: &Value| v.is_object()).unwrap_or_else(|| json!({}))
}
// merge `patch` into ~/.config/mist/<name>
pub fn patch_json(name: &str, patch: Value) -> Value {
    let mut v = read_json(name);
    if let Some(o) = patch.as_object() {
        for (k, x) in o {
            if x.is_null() {
                v.as_object_mut().map(|m| m.remove(k));
            } else {
                v[k] = x.clone();
            }
        }
    }
    write_atomic(&cfg(name), &v.to_string());
    v
}

pub fn bin() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "mist-ui-bin".into())
}
// a command for the running shell: `mist-ui-bin msg <target> <func> [args]`
pub fn shell_msg(args: &[&str]) {
    let b = bin();
    let mut v: Vec<&str> = vec![&b, "msg"];
    v.extend_from_slice(args);
    crate::appwin::spawn(&v);
}
// the shell (the bare `mist-ui-bin`, not this window or Mist Files) ends; its guard starts it again
pub fn restart_shell() {
    let me = std::process::id().to_string();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let pid = e.file_name().to_string_lossy().to_string();
        if pid == me || !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if std::fs::read_to_string(e.path().join("comm")).unwrap_or_default().trim() != "mist-ui-bin" {
            continue;
        }
        let cmd = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        if cmd.split(|b| *b == 0).filter(|a| !a.is_empty()).count() == 1 {
            let _ = Command::new("kill").arg(&pid).status();
        }
    }
}
pub fn under_mist_comp() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default() == "mist"
}
// one line on mist-comp's IPC socket ($XDG_RUNTIME_DIR/mist-comp/<wayland display>.sock)
pub fn comp_ipc(line: &str) -> Option<String> {
    use std::io::{Read, Write};
    let p = format!("{}/mist-comp/{}.sock", std::env::var("XDG_RUNTIME_DIR").ok()?, std::env::var("WAYLAND_DISPLAY").ok()?);
    let mut s = std::os::unix::net::UnixStream::connect(p).ok()?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    s.write_all(format!("{line}\n").as_bytes()).ok()?;
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    Some(out)
}

// comp.conf: the flags this app sets. When the file is new it gets every one of them with the default's value
// (a user comp.conf replaces the defaults' plain flags, binds and autostart still come from the defaults).
pub const COMP_DEFAULT: &str = include_str!("../../../assets/comp-default.conf");
const COMP_KEYS: [&str; 7] = ["keyboard", "numlock", "pointer", "swipe", "middle_click_paste", "vrr", "swallow"];

fn directive<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().rev().map(str::trim).filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once(char::is_whitespace).filter(|(k, _)| *k == key).map(|(_, v)| v.trim())).next()
}
// the effective value of a comp.conf flag ("" when neither file has it)
pub fn comp_get(key: &str) -> String {
    let user = std::fs::read_to_string(cfg("comp.conf")).ok();
    match &user {
        Some(u) => directive(u, key).map(String::from).unwrap_or_default(),
        None => directive(COMP_DEFAULT, key).map(String::from).unwrap_or_default(),
    }
}
pub fn comp_set(key: &str, val: &str) {
    let path = cfg("comp.conf");
    let mut lines: Vec<String> = match std::fs::read_to_string(&path) {
        Ok(s) => s.lines().map(String::from).collect(),
        Err(_) => {
            let mut v = vec!["# mist-comp settings (Mist Settings writes the flags below; binds and autostart come from the defaults".to_string(), "# until this file has its own `bind` / `exec` lines). Directives: see native/assets/comp-default.conf".to_string()];
            for k in COMP_KEYS {
                if let Some(d) = directive(COMP_DEFAULT, k) {
                    v.push(format!("{k} {d}"));
                }
            }
            v
        }
    };
    let line = format!("{key} {val}");
    match lines.iter().rposition(|l| l.trim().split_once(char::is_whitespace).map(|(k, _)| k == key).unwrap_or(l.trim() == key)) {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    write_atomic(&path, &(lines.join("\n") + "\n"));
    comp_ipc("action reload");
}

// ~/.config/mist/outputs with new scales: monitors left to right, each starting where the previous one's logical
// width ends; then the compositor applies it (reload)
pub fn set_scale(mons: &[Mon], name: &str, scale: f64) {
    let mut x = 0i64;
    let mut out = String::from("# mist-comp output layout: NAME X Y (logical px) [scale S]\n");
    for m in mons {
        let s = if m.name == name { scale } else { m.scale };
        out.push_str(&format!("{} {} {}", m.name, x, m.y));
        if (s - 1.0).abs() > 0.001 {
            out.push_str(&format!(" scale {s}"));
        }
        out.push('\n');
        x += (m.w as f64 / s).round() as i64;
    }
    write_atomic(&cfg("outputs"), &out);
    comp_ipc("action reload");
}

// the shortcuts as the compositor has them: (keys, action), from comp.conf when it has binds, else the defaults
pub fn binds() -> Vec<(String, String)> {
    let user = std::fs::read_to_string(cfg("comp.conf")).unwrap_or_default();
    let has = user.lines().any(|l| l.trim_start().starts_with("bind"));
    let src = if has { user.as_str() } else { COMP_DEFAULT };
    src.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("bind ") || l.starts_with("bindr ") || l.starts_with("bindl "))
        .filter_map(|l| {
            let mut it = l.splitn(3, char::is_whitespace).map(str::trim);
            it.next();
            let rest = l.split_once(char::is_whitespace)?.1.trim();
            let (keys, act) = rest.split_once(char::is_whitespace)?;
            let _ = it;
            Some((keys.to_string(), act.trim().to_string()))
        })
        .collect()
}
