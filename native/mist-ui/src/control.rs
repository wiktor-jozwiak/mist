// macOS-style Control centre (was Control.qml; the bar's sliders icon, the volume / network / Bluetooth icons
// open it on their page):
//   home       tiles: network · Bluetooth · phone | Do Not Disturb · night light, sound slider, now playing
//   sound      output + input volume and devices, per-app volume (grouped by application.name)
//   network    active connection details and live throughput
//   bluetooth  power switch, paired devices (click = connect / disconnect), discovery + pairing
// Pages slide in from the right; the glass sheet animates to the page height. The data comes from a worker
// thread (pactl json, BlueZ over D-Bus, nmcli), refreshed every second while open and after each command.
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

const SHEET_W: f32 = 400.0;
const INK: Rgba = Rgba(14.0 / 255.0, 20.0 / 255.0, 32.0 / 255.0, 1.0); // Theme.ink
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const BODY_MAX: f32 = 620.0;

// ------------------------------------------------------------------ data (worker thread)
#[derive(Clone, Default)]
struct Dev {
    name: String,
    desc: String,
    vol: f32,
    mute: bool,
}
#[derive(Clone, Default)]
struct AppSnd {
    name: String,
    media: String,
    icon: String,
    inputs: Vec<u64>,
    vol: f32,
    mute: bool,
}
#[derive(Clone, Default)]
struct Snd {
    sinks: Vec<Dev>,
    sources: Vec<Dev>,
    sink: String,
    source: String,
    apps: Vec<AppSnd>,
}
#[derive(Clone, Default)]
struct BtD {
    path: String,
    address: String,
    name: String,
    icon: String,
    paired: bool,
    connected: bool,
    battery: Option<f32>,
}
#[derive(Clone, Default)]
struct Bt {
    on: bool,
    discovering: bool,
    devs: Vec<BtD>,
}
#[derive(Clone, Default)]
struct Net {
    dev: String,
    kind: String,
    conn: String,
    ip: String,
    gw: String,
    dns: String,
    speed: i64,
    up: bool,
    has_wifi: bool,
}

enum Req {
    Snd,
    Bt,
    Net,
    // a command, then the sound state again
    Pactl(Vec<String>),
    BtPower(bool),
    BtDiscover(bool),
    BtConnect(String, bool),
    BtPair(String),
}
enum Res {
    Snd(Snd),
    Bt(Bt),
    Net(Net),
    // the device's connect / disconnect / pair finished
    BtDone(String),
}

fn pactl_json(args: &[&str]) -> Value {
    Command::new("pactl").arg("-f").arg("json").args(args).output().ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(Value::Null)
}
fn vol_of(v: &Value) -> f32 {
    let ch: Vec<f32> = v["volume"].as_object().map(|o| o.values().filter_map(|c| c["value"].as_f64()).map(|x| x as f32 / 65536.0).collect()).unwrap_or_default();
    if ch.is_empty() { 0.0 } else { ch.iter().sum::<f32>() / ch.len() as f32 }
}
fn snd_state() -> Snd {
    let info = pactl_json(&["info"]);
    // the Mist EQ filter-chain nodes (60-mist-eq.conf) have no description
    let desc = |v: &Value| -> String {
        let (n, d) = (v["name"].as_str().unwrap_or(""), v["description"].as_str().unwrap_or(""));
        match (n, d) {
            ("mist_eq_speakers", _) => "Głośniki · EQ".into(),
            ("mist_eq_mic", _) => "Mikrofon · EQ".into(),
            (n, "" | "(null)") => v["properties"]["node.nick"].as_str().unwrap_or(n).into(),
            (_, d) => d.into(),
        }
    };
    let dev = |v: &Value| Dev { name: v["name"].as_str().unwrap_or("").into(), desc: desc(v), vol: vol_of(v), mute: v["mute"].as_bool().unwrap_or(false) };
    let sinks = pactl_json(&["list", "sinks"]).as_array().map(|a| a.iter().map(dev).collect()).unwrap_or_default();
    let sources = pactl_json(&["list", "sources"])
        .as_array()
        .map(|a| a.iter().filter(|s| s["properties"]["device.class"].as_str() != Some("monitor") && !s["name"].as_str().unwrap_or("").ends_with(".monitor")).map(dev).collect())
        .unwrap_or_default();
    // playback streams grouped per application (Spotify opens two); filter-chain streams have no application.name
    let mut apps: Vec<AppSnd> = Vec::new();
    for s in pactl_json(&["list", "sink-inputs"]).as_array().cloned().unwrap_or_default() {
        let p = &s["properties"];
        let Some(a) = p["application.name"].as_str() else { continue };
        if p["media.class"].as_str().map(|c| c != "Stream/Output/Audio").unwrap_or(false) {
            continue;
        }
        let i = match apps.iter().position(|x| x.name == a) {
            Some(i) => i,
            None => {
                let icon = p["application.icon_name"].as_str().or(p["application.process.binary"].as_str()).unwrap_or(a).to_string();
                apps.push(AppSnd { name: a.into(), icon, vol: vol_of(&s), mute: s["mute"].as_bool().unwrap_or(false), ..Default::default() });
                apps.len() - 1
            }
        };
        let m = p["media.name"].as_str().unwrap_or("");
        if apps[i].media.is_empty() && !m.is_empty() && m != a {
            apps[i].media = m.into();
        }
        if let Some(ix) = s["index"].as_u64() {
            apps[i].inputs.push(ix);
        }
    }
    Snd { sinks, sources, sink: info["default_sink_name"].as_str().unwrap_or("").into(), source: info["default_source_name"].as_str().unwrap_or("").into(), apps }
}

type Props = HashMap<String, zbus::zvariant::OwnedValue>;
type Objs = HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, Props>>;
fn bt_objs(c: &zbus::blocking::Connection) -> Objs {
    c.call_method(Some("org.bluez"), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &()).ok().and_then(|m| m.body().deserialize::<Objs>().ok()).unwrap_or_default()
}
fn bt_adapter(c: &zbus::blocking::Connection) -> Option<String> {
    bt_objs(c).into_iter().find(|(_, i)| i.contains_key("org.bluez.Adapter1")).map(|(p, _)| p.to_string())
}
fn bt_state(c: &zbus::blocking::Connection) -> Bt {
    let objs = bt_objs(c);
    let b = |p: &Props, k: &str| p.get(k).and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
    let s = |p: &Props, k: &str| p.get(k).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default();
    let ad = objs.values().find_map(|i| i.get("org.bluez.Adapter1"));
    let mut devs: Vec<BtD> = objs
        .iter()
        .filter_map(|(path, i)| {
            let d = i.get("org.bluez.Device1")?;
            Some(BtD {
                path: path.to_string(),
                address: s(d, "Address"),
                name: [s(d, "Alias"), s(d, "Name")].into_iter().find(|x| !x.is_empty()).unwrap_or_default(),
                icon: s(d, "Icon"),
                paired: b(d, "Paired"),
                connected: b(d, "Connected"),
                battery: i.get("org.bluez.Battery1").and_then(|p| p.get("Percentage")).and_then(|v| u8::try_from(v.clone()).ok()).map(|v| v as f32 / 100.0),
            })
        })
        .collect();
    devs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Bt { on: ad.map(|p| b(p, "Powered")).unwrap_or(false), discovering: ad.map(|p| b(p, "Discovering")).unwrap_or(false), devs }
}
fn net_state() -> Net {
    let out = Command::new("sh")
        .args(["-c", "nmcli -t -f GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS device show"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let mut devs: Vec<Net> = Vec::new();
    let mut dns: Vec<Vec<String>> = Vec::new();
    for l in out.lines() {
        let Some((k, v)) = l.split_once(':') else { continue };
        if k == "GENERAL.DEVICE" {
            devs.push(Net { dev: v.into(), ..Default::default() });
            dns.push(Vec::new());
            continue;
        }
        let Some(d) = devs.last_mut() else { continue };
        match k {
            "GENERAL.TYPE" => d.kind = v.into(),
            "GENERAL.STATE" => d.up = v.starts_with("100"),
            "GENERAL.CONNECTION" => d.conn = v.into(),
            "IP4.GATEWAY" => d.gw = v.into(),
            k if k.starts_with("IP4.ADDRESS") && d.ip.is_empty() => d.ip = v.into(),
            k if k.starts_with("IP4.DNS") => dns.last_mut().unwrap().push(v.into()),
            _ => {}
        }
    }
    for (d, n) in devs.iter_mut().zip(dns) {
        d.dns = n.join(", ");
    }
    let real: Vec<Net> = devs.into_iter().filter(|d| (d.kind == "ethernet" || d.kind == "wifi") && !["veth", "docker", "br-"].iter().any(|p| d.dev.starts_with(p))).collect();
    let has_wifi = real.iter().any(|d| d.kind == "wifi");
    let mut n = real.iter().find(|d| d.up).or(real.first()).cloned().unwrap_or_default();
    n.speed = std::fs::read_to_string(format!("/sys/class/net/{}/speed", n.dev)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    n.has_wifi = has_wifi;
    n
}

fn worker(rx: Receiver<Req>, tx: Sender<Res>) {
    let bus = zbus::blocking::Connection::system().ok();
    while let Ok(first) = rx.recv() {
        // coalesce: a dragged slider sends many volumes, only the last one per target matters
        let mut reqs = vec![first];
        while let Ok(r) = rx.try_recv() {
            reqs.push(r);
        }
        let mut last_pactl: Vec<Vec<String>> = Vec::new();
        let (mut snd, mut bt, mut net) = (false, false, false);
        for r in reqs {
            match r {
                Req::Snd => snd = true,
                Req::Bt => bt = true,
                Req::Net => net = true,
                Req::Pactl(c) => {
                    last_pactl.retain(|p| p[..p.len() - 1] != c[..c.len() - 1]);
                    last_pactl.push(c);
                    snd = true;
                }
                Req::BtPower(on) => {
                    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(bt_adapter)) {
                        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &("org.bluez.Adapter1", "Powered", zbus::zvariant::Value::from(on)));
                    }
                    bt = true;
                }
                Req::BtDiscover(on) => {
                    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(bt_adapter)) {
                        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.bluez.Adapter1"), if on { "StartDiscovery" } else { "StopDiscovery" }, &());
                    }
                    bt = true;
                }
                Req::BtConnect(path, on) => {
                    // seconds long: its own thread
                    if let Some(c) = bus.clone() {
                        let tx = tx.clone();
                        let _ = std::thread::Builder::new().name("bt-connect".into()).spawn(move || {
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.bluez.Device1"), if on { "Connect" } else { "Disconnect" }, &());
                            let _ = tx.send(Res::BtDone(path.clone()));
                            let _ = tx.send(Res::Bt(bt_state(&c)));
                        });
                    }
                }
                Req::BtPair(path) => {
                    if let Some(c) = bus.clone() {
                        let tx = tx.clone();
                        let _ = std::thread::Builder::new().name("bt-pair".into()).spawn(move || {
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &("org.bluez.Device1", "Trusted", zbus::zvariant::Value::from(true)));
                            let _ = c.call_method(Some("org.bluez"), path.as_str(), Some("org.bluez.Device1"), "Pair", &());
                            let _ = tx.send(Res::BtDone(path.clone()));
                            let _ = tx.send(Res::Bt(bt_state(&c)));
                        });
                    }
                }
            }
        }
        for c in last_pactl {
            let _ = Command::new(&c[0]).args(&c[1..]).status();
        }
        if snd && tx.send(Res::Snd(snd_state())).is_err() {
            return;
        }
        if bt {
            if let Some(c) = &bus {
                if tx.send(Res::Bt(bt_state(c))).is_err() {
                    return;
                }
            }
        }
        if net && tx.send(Res::Net(net_state())).is_err() {
            return;
        }
    }
    // the pane closed: stop a discovery we started (BlueZ also stops it when this connection goes)
    if let (Some(c), Some(a)) = (&bus, bus.as_ref().and_then(bt_adapter)) {
        let _ = c.call_method(Some("org.bluez"), a.as_str(), Some("org.bluez.Adapter1"), "StopDiscovery", &());
    }
}

// ------------------------------------------------------------------ the pane
#[derive(Clone, Copy, PartialEq, Debug)]
enum Slider {
    Sink,
    Source,
    App(usize),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Sheet,
    Link(u8),
    Orb(u8),
    Dnd,
    Night,
    SoundHead,
    Slider(Slider),
    Prev,
    Play,
    Next,
    Wallpaper,
    Lock,
    Power,
    Back,
    BtSwitch,
    Pavu,
    Sink(usize),
    Source(usize),
    Paired(usize),
    Found(usize),
    Wifi,
}

pub struct Control {
    screen: String,
    page: String,
    shown: String, // the detail page drawn (stays while it slides out)
    slide: Anim,   // 0 = home, 1 = a detail page
    sheet_h: Anim,
    snd: Snd,
    bt: Bt,
    net: Net,
    req: Sender<Req>,
    res: Receiver<Res>,
    busy: HashMap<String, Instant>, // Bluetooth devices connecting / pairing
    local: HashMap<String, (f32, Instant)>, // slider values set here, until the data catches up
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    ptr: (f32, f32),
    drag: Option<(Slider, f32, f32)>, // slider, track x, width
    pressed_outside: bool,
    scroll: f32,
    body_h: f32,
    tick: u32,
    rx: (u64, u64, Instant), // /proc/net/dev totals
    rate: (f64, f64),
    night: bool,
    uptime: String,
    phone: Value,
    discovering: bool,
    born: Instant,
}

fn rx_tx() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
    let mut t = (0u64, 0u64);
    for l in s.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else { continue };
        let n = name.trim();
        if n == "lo" || n.starts_with("veth") || n.starts_with("docker") || n.starts_with("br-") {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if f.len() >= 9 {
            t.0 += f[0];
            t.1 += f[8];
        }
    }
    t
}
fn uptime() -> String {
    let s: f64 = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| s.split_whitespace().next().and_then(|x| x.parse().ok())).unwrap_or(0.0);
    let m = (s / 60.0) as i64;
    let (d, h, mi) = (m / 1440, m / 60 % 24, m % 60);
    if d > 0 { format!("{d} d {h} godz.") } else if h > 0 { format!("{h} godz. {mi} min") } else { format!("{mi} min") }
}
fn night_path() -> String {
    format!("{}/.local/state/mist/nightlight", std::env::var("HOME").unwrap_or_default())
}
pub fn night_on() -> bool {
    std::fs::read_to_string(night_path()).map(|s| s.trim() == "1").unwrap_or(false)
}
// Hyprland screen shader shaders/nightlight.glsl (~4000 K)
pub fn night_cmd(on: bool) -> Vec<String> {
    let sh = if on { format!("{}/.config/quickshell/mist/shaders/nightlight.glsl", std::env::var("HOME").unwrap_or_default()) } else { String::new() };
    vec!["hyprctl".into(), "eval".into(), format!("hl.config({{ decoration = {{ screen_shader = \"{sh}\" }} }})")]
}
fn sink_icon(d: &Dev) -> &'static str {
    let s = format!("{} {}", d.desc, d.name).to_lowercase();
    if s.contains("headphone") || s.contains("headset") || s.contains("słuchawk") { "headphones" } else if s.contains("hdmi") || s.contains("displayport") { "tv" } else { "speaker" }
}
fn bt_icon(d: &BtD) -> &'static str {
    let i = d.icon.as_str();
    if i.contains("headset") || i.contains("headphone") || i.contains("audio-card") { "headphones" } else if i.contains("keyboard") { "keyboard" } else if i.contains("mouse") { "mouse" } else if i.contains("gaming") || i.contains("joystick") { "gamepad" } else if i.contains("phone") { "mobile" } else if i.contains("speaker") || i.contains("audio") { "speaker" } else { "bluetooth" }
}

impl Control {
    pub fn new(page: &str) -> Control {
        let (req, wrx) = channel();
        let (wtx, res) = channel();
        let _ = std::thread::Builder::new().name("control".into()).stack_size(512 * 1024).spawn(move || worker(wrx, wtx));
        let _ = req.send(Req::Snd);
        let _ = req.send(Req::Bt);
        let _ = req.send(Req::Net);
        let (r, t) = rx_tx();
        let phone = std::fs::read_to_string(format!("{}/mist/phone.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()))).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut c = Control {
            screen: String::new(),
            page: String::new(),
            shown: String::new(),
            slide: Anim::new(0.0),
            sheet_h: Anim::new(0.0),
            snd: Snd::default(),
            bt: Bt::default(),
            net: Net::default(),
            req,
            res,
            busy: HashMap::new(),
            local: HashMap::new(),
            hits: Vec::new(),
            ptr: (-1.0, -1.0),
            drag: None,
            pressed_outside: false,
            scroll: 0.0,
            body_h: 0.0,
            tick: 0,
            rx: (r, t, Instant::now()),
            rate: (0.0, 0.0),
            night: night_on(),
            uptime: uptime(),
            phone,
            discovering: false,
            born: Instant::now(),
        };
        c.go(page, false);
        // the first frame with data (a blank sheet would flash)
        let t0 = Instant::now();
        while t0.elapsed().as_millis() < 250 {
            match c.res.recv_timeout(std::time::Duration::from_millis(250) - t0.elapsed()) {
                Ok(r) => {
                    c.take(r);
                    if !c.snd.sink.is_empty() && !c.net.dev.is_empty() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        c
    }
    fn go(&mut self, p: &str, animate: bool) {
        self.page = p.to_string();
        if !p.is_empty() {
            self.shown = p.to_string();
            self.scroll = 0.0;
        }
        let to = if p.is_empty() { 0.0 } else { 1.0 };
        if animate {
            self.slide.set(to, 260.0, Ease::OutCubic);
        } else {
            self.slide = Anim::new(to);
        }
        // scan only while the Bluetooth page is open
        let want = p == "bluetooth" && self.bt.on;
        if want != self.discovering {
            self.discovering = want;
            let _ = self.req.send(Req::BtDiscover(want));
        }
        let _ = self.req.send(match p { "network" => Req::Net, "bluetooth" => Req::Bt, _ => Req::Snd });
    }
    fn take(&mut self, r: Res) {
        match r {
            Res::Snd(s) => self.snd = s,
            Res::Bt(b) => {
                self.bt = b;
                let want = self.page == "bluetooth" && self.bt.on;
                if want != self.discovering {
                    self.discovering = want;
                    let _ = self.req.send(Req::BtDiscover(want));
                }
            }
            Res::Net(n) => self.net = n,
            Res::BtDone(p) => {
                self.busy.remove(&p);
            }
        }
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1)
    }
    fn hov(&self, h: Hit) -> bool {
        self.hit() == Some(h)
    }
    fn sink(&self) -> Option<&Dev> {
        self.snd.sinks.iter().find(|d| d.name == self.snd.sink)
    }
    fn source(&self) -> Option<&Dev> {
        self.snd.sources.iter().find(|d| d.name == self.snd.source)
    }
    fn key_of(&self, s: Slider) -> String {
        match s {
            Slider::Sink => format!("sink:{}", self.snd.sink),
            Slider::Source => format!("source:{}", self.snd.source),
            Slider::App(i) => format!("app:{}", self.snd.apps.get(i).map(|a| a.name.as_str()).unwrap_or("")),
        }
    }
    // (volume, muted, present)
    fn slider_val(&self, s: Slider) -> (f32, bool, bool) {
        let (v, m, ok) = match s {
            Slider::Sink => self.sink().map(|d| (d.vol, d.mute, true)).unwrap_or((0.0, true, false)),
            Slider::Source => self.source().map(|d| (d.vol, d.mute, true)).unwrap_or((0.0, true, false)),
            Slider::App(i) => self.snd.apps.get(i).map(|a| (a.vol, a.mute, true)).unwrap_or((0.0, true, false)),
        };
        match self.local.get(&self.key_of(s)) {
            Some((lv, at)) if at.elapsed().as_millis() < 1500 => (*lv, if *lv > 0.0 { false } else { m }, ok),
            _ => (v.min(1.0), m, ok),
        }
    }
    fn set_vol(&mut self, s: Slider, v: f32) {
        let v = ((v.clamp(0.0, 1.0)) * 100.0).round() / 100.0;
        let (_, muted, ok) = self.slider_val(s);
        if !ok {
            return;
        }
        self.local.insert(self.key_of(s), (v, Instant::now()));
        let pct = format!("{}%", (v * 100.0).round() as i32);
        let mut cmds: Vec<Vec<String>> = Vec::new();
        let p = |a: &[&str]| -> Vec<String> { a.iter().map(|x| x.to_string()).collect() };
        match s {
            Slider::Sink => {
                let n = self.snd.sink.clone();
                if muted && v > 0.0 { cmds.push(p(&["pactl", "set-sink-mute", &n, "0"])); }
                cmds.push(p(&["pactl", "set-sink-volume", &n, &pct]));
            }
            Slider::Source => {
                let n = self.snd.source.clone();
                if muted && v > 0.0 { cmds.push(p(&["pactl", "set-source-mute", &n, "0"])); }
                cmds.push(p(&["pactl", "set-source-volume", &n, &pct]));
            }
            Slider::App(i) => {
                for ix in self.snd.apps.get(i).map(|a| a.inputs.clone()).unwrap_or_default() {
                    let ix = ix.to_string();
                    if muted && v > 0.0 { cmds.push(p(&["pactl", "set-sink-input-mute", &ix, "0"])); }
                    cmds.push(p(&["pactl", "set-sink-input-volume", &ix, &pct]));
                }
            }
        }
        for c in cmds {
            let _ = self.req.send(Req::Pactl(c));
        }
    }
    fn mute(&mut self, s: Slider) {
        let (_, muted, ok) = self.slider_val(s);
        if !ok {
            return;
        }
        let m = if muted { "0" } else { "1" }.to_string();
        let c = |a: &str, n: String| vec!["pactl".to_string(), a.to_string(), n, m.clone()];
        let cmds = match s {
            Slider::Sink => vec![c("set-sink-mute", self.snd.sink.clone())],
            Slider::Source => vec![c("set-source-mute", self.snd.source.clone())],
            Slider::App(i) => self.snd.apps.get(i).map(|a| a.inputs.iter().map(|ix| c("set-sink-input-mute", ix.to_string())).collect()).unwrap_or_default(),
        };
        // optimistic
        match s {
            Slider::Sink => if let Some(d) = self.snd.sinks.iter_mut().find(|d| d.name == self.snd.sink) { d.mute = !muted },
            Slider::Source => if let Some(d) = self.snd.sources.iter_mut().find(|d| d.name == self.snd.source) { d.mute = !muted },
            Slider::App(i) => if let Some(a) = self.snd.apps.get_mut(i) { a.mute = !muted },
        }
        self.local.remove(&self.key_of(s));
        for c in cmds {
            let _ = self.req.send(Req::Pactl(c));
        }
    }
    fn found(&self) -> Vec<&BtD> {
        self.bt.devs.iter().filter(|d| !d.paired && !d.name.is_empty() && d.name.replace(['-', ':'], "") != d.address.replace(['-', ':'], "")).collect()
    }
    fn paired(&self) -> Vec<&BtD> {
        self.bt.devs.iter().filter(|d| d.paired).collect()
    }

    // ---------------------------------------------------------------- drawing parts (local coords, hits absolute)
    fn tile(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, hover: bool) {
        crate::glass::Glass { radius: 22.0, tint: if hover { 0.095 } else { 0.06 }, tone: WHITE, shine: 0.6, shadow: false, ..Default::default() }.paint(c, x, y, w, h);
    }
    fn group(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
        crate::glass::Glass { radius: 18.0, tint: 0.22, tone: Rgba::rgb8(4, 7, 14, 1.0), shine: 0.45, shadow: false, ..Default::default() }.paint(c, x, y, w, h);
    }
    fn orb(c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, icon: &str, on: bool, hover: bool) {
        crate::desktop::rect(c, x, y, 34.0, 34.0, 17.0, if on { ACCENT } else if hover { WHITE.alpha(0.18) } else { FILL2 });
        c.icon(t, x + 17.0, y + 17.0, icon, 16.0, if on { WHITE } else { T1 });
    }
    fn label(c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, s: &str) -> f32 {
        let st = Style::ui(11.0, 600.0).spaced(1.2);
        crate::desktop::text(c, t, x + 4.0, y, &s.to_uppercase(), &st, T3, false, None);
        crate::desktop::line_h(t, &st)
    }
    fn two_lines(c: &mut Canvas, t: &mut crate::text::Text, x: f32, cy: f32, w: f32, a: &str, a_st: Style, a_col: Rgba, b: &str, b_st: Style, b_col: Rgba) {
        let (h1, h2) = (crate::desktop::line_h(t, &a_st), crate::desktop::line_h(t, &b_st));
        let y = cy - (h1 + h2) / 2.0;
        crate::desktop::text(c, t, x, y, a, &a_st, a_col, false, Some(w));
        crate::desktop::text(c, t, x, y + h1, b, &b_st, b_col, false, Some(w));
    }
    fn btn(&mut self, c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, icon: &str, size: f32, col: Rgba, hit: Hit, o: (f32, f32)) {
        let hov = self.hov(hit);
        crate::desktop::rect(c, x, y, 34.0, 34.0, 17.0, if hov { FILL2 } else { FILL });
        c.icon(t, x + 17.0, y + 17.0, icon, size, col);
        self.hits.push(((o.0 + x, o.1 + y, 34.0, 34.0), hit));
    }
    fn slider(&mut self, c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, w: f32, s: Slider, mic: bool, slim: bool, o: (f32, f32)) {
        let h = if slim { 24.0 } else { 30.0 };
        let (v, muted, ok) = self.slider_val(s);
        let a = if ok { 1.0 } else { 0.4 };
        crate::desktop::rect(c, x, y, w, h, h / 2.0, FILL2.alpha(FILL2.3 * a));
        let fw = (w * v).max(h);
        crate::desktop::rect(c, x, y, fw, h, h / 2.0, if muted { T3 } else { T1 }.alpha(a * if muted { T3.3 } else { T1.3 }));
        let icon = if mic { if muted { "micOff" } else { "mic" } } else if muted { "volumeOff" } else if v < 0.4 { "volLow" } else { "volume" };
        c.icon(t, x + h / 2.0 + 1.0, y + h / 2.0, icon, if slim { 12.0 } else { 15.0 }, INK.alpha(a));
        let txt = if muted { "wycisz.".to_string() } else { format!("{}%", (v * 100.0).round() as i32) };
        let st = Style::ui(if slim { 10.5 } else { 11.5 }, 600.0).tnum();
        let tw = t.width(&txt, &st);
        let lh = crate::desktop::line_h(t, &st);
        crate::desktop::text(c, t, x + w - (if slim { 9.0 } else { 12.0 }) - tw, y + (h - lh) / 2.0, &txt, &st, if v > 0.88 && !muted { INK } else { T2 }.alpha(a), false, None);
        self.hits.push(((o.0 + x, o.1 + y, w, h), Hit::Slider(s)));
    }
    fn pick_row(&mut self, c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, w: f32, icon: &str, title: &str, current: bool, hit: Hit, o: (f32, f32)) {
        if self.hov(hit) {
            crate::desktop::rect(c, x, y, w, 40.0, 12.0, FILL);
        }
        crate::desktop::rect(c, x + 6.0, y + 6.0, 28.0, 28.0, 14.0, if current { ACCENT } else { FILL2 });
        c.icon(t, x + 20.0, y + 20.0, icon, 14.0, if current { WHITE } else { T2 });
        let st = Style::ui(12.5, if current { 600.0 } else { 450.0 });
        let lh = crate::desktop::line_h(t, &st);
        crate::desktop::text(c, t, x + 44.0, y + (40.0 - lh) / 2.0, title, &st, T1, false, Some(w - 44.0 - 35.0));
        if current {
            c.icon(t, x + w - 12.0 - 7.5, y + 20.0, "tick", 15.0, ACCENT);
        }
        self.hits.push(((o.0 + x, o.1 + y, w, 40.0), hit));
    }
    fn dev_row(&mut self, c: &mut Canvas, t: &mut crate::text::Text, x: f32, y: f32, w: f32, d: &BtD, sub: &str, on: bool, busy: bool, hit: Hit, o: (f32, f32)) {
        if self.hov(hit) {
            crate::desktop::rect(c, x, y, w, 48.0, 12.0, FILL);
        }
        // busy: the orb pulses (700 ms)
        let a = if busy { 0.7 + 0.3 * (self.born.elapsed().as_millis() as f32 / 1000.0 * std::f32::consts::PI).cos() } else { 1.0 };
        crate::desktop::rect(c, x + 6.0, y + 8.0, 32.0, 32.0, 16.0, if on { ACCENT } else { FILL2 }.alpha(a * if on { 1.0 } else { FILL2.3 }));
        c.icon(t, x + 22.0, y + 24.0, bt_icon(d), 15.0, if on { WHITE } else { T2 }.alpha(a * if on { 1.0 } else { T2.3 }));
        Self::two_lines(c, t, x + 48.0, y + 24.0, w - 60.0, &d.name, Style::ui(12.5, if on { 600.0 } else { 500.0 }), T1, sub, Style::ui(11.0, 400.0), if on { GREEN } else { T3 });
        self.hits.push(((o.0 + x, o.1 + y, w, 48.0), hit));
    }

    // the home page into `c` (content width w), returns its height
    fn paint_home(&mut self, c: &mut Canvas, cx: &mut Ctx, o: (f32, f32)) -> f32 {
        let t = &mut *cx.t;
        let w = SHEET_W - 32.0;
        let half = (w - 10.0) / 2.0;
        let mut y = 0.0;
        // connectivity
        let tile_hov = inside(self.ptr, (o.0, o.1, half, 150.0));
        Self::tile(c, 0.0, y, half, 150.0, tile_hov);
        let bt_conn: Vec<&BtD> = self.bt.devs.iter().filter(|d| d.connected).collect();
        let net_title = if self.net.kind == "wifi" { if self.net.up { self.net.conn.clone() } else { "Wi-Fi".into() } } else { "Ethernet".into() };
        let net_sub = if self.net.dev.is_empty() { "…".into() } else if self.net.up { "Połączono".to_string() } else { "Rozłączono".into() };
        let bt_sub = if !self.bt.on { "Wyłączony".to_string() } else if bt_conn.len() == 1 { bt_conn[0].name.clone() } else if !bt_conn.is_empty() { format!("{} urządzenia", bt_conn.len()) } else { "Włączony".into() };
        let ph = &self.phone;
        let (ph_on, ph_bat) = (ph["online"].as_bool().unwrap_or(false), ph["battery"].as_i64().unwrap_or(-1));
        let ph_name = if cx.phone_name.is_empty() { "Telefon".to_string() } else { cx.phone_name.to_string() };
        let ph_sub = if ph_on { if ph_bat >= 0 { format!("Bateria {ph_bat}%") } else { "Połączony".into() } } else if ph.get("online").is_some() { "Poza zasięgiem".into() } else { "KDE Connect".into() };
        let rows: [(&str, bool, String, String); 3] = [
            (if self.net.kind == "wifi" { "wifi" } else { "ethernet" }, self.net.up, net_title, net_sub),
            (if bt_conn.is_empty() { "bluetooth" } else { "btOn" }, self.bt.on, "Bluetooth".into(), bt_sub),
            (if ph["charging"].as_bool().unwrap_or(false) { "charging" } else { "mobile" }, ph_on, ph_name, ph_sub),
        ];
        for (i, (icon, on, title, sub)) in rows.iter().enumerate() {
            let (rx, ry, rw) = (8.0, y + 8.0 + i as f32 * 44.6, half - 16.0);
            let link = Hit::Link(i as u8);
            if self.hov(link) {
                crate::desktop::rect(c, rx, ry, rw, 44.6, 13.0, FILL);
            }
            self.hits.push(((o.0 + rx, o.1 + ry, rw, 44.6), link));
            let orb_hov = self.hov(Hit::Orb(i as u8));
            Self::orb(c, t, rx + 4.0, ry + 5.3, icon, *on, orb_hov);
            self.hits.push(((o.0 + rx + 4.0, o.1 + ry + 5.3, 34.0, 34.0), Hit::Orb(i as u8)));
            Self::two_lines(c, t, rx + 47.0, ry + 22.3, rw - 51.0, title, Style::ui(12.5, 600.0), T1, sub, Style::ui(11.0, 400.0), T2);
        }
        // Do Not Disturb, night light
        let night = self.night;
        for (i, (icon, title, on, hit)) in [("bellZ", "Nie przeszkadzać", cx.st.dnd, Hit::Dnd), ("moon", "Nocne światło", night, Hit::Night)].into_iter().enumerate() {
            let (tx, ty) = (half + 10.0, y + i as f32 * 80.0);
            Self::tile(c, tx, ty, half, 70.0, self.hov(hit));
            Self::orb(c, t, tx + 10.0, ty + 18.0, icon, on, false);
            Self::two_lines(c, t, tx + 52.0, ty + 35.0, half - 58.0, title, Style::ui(12.0, 600.0), T1, if on { "Włączone" } else { "Wyłączone" }, Style::ui(11.0, 400.0), T2);
            self.hits.push(((o.0 + tx, o.1 + ty, half, 70.0), hit));
        }
        y += 160.0;
        // sound
        Self::tile(c, 0.0, y, w, 92.0, false);
        {
            let hy = y + 12.0;
            let st = Style::ui(13.0, 650.0);
            let lh = crate::desktop::line_h(t, &st);
            crate::desktop::text(c, t, 14.0, hy + (18.0 - lh) / 2.0, "Dźwięk", &st, T1, false, None);
            let hov = self.hov(Hit::SoundHead);
            let name = self.sink().map(|d| d.desc.clone()).unwrap_or_default();
            let ns = Style::ui(11.5, 400.0);
            let nw = t.width(&name, &ns).min(190.0);
            let a = if hov { 1.0 } else { 0.75 };
            c.icon(t, w - 14.0 - 7.0, hy + 9.0, "chevron", 14.0, T2.alpha(T2.3 * a));
            let nlh = crate::desktop::line_h(t, &ns);
            crate::desktop::text(c, t, w - 14.0 - 14.0 - 4.0 - nw, hy + (18.0 - nlh) / 2.0, &name, &ns, T2.alpha(T2.3 * a), false, Some(190.0));
            self.hits.push(((o.0 + 14.0, o.1 + hy, w - 28.0, 18.0), Hit::SoundHead));
            self.slider(c, t, 14.0, hy + 28.0, w - 28.0, Slider::Sink, false, false, o);
        }
        y += 102.0;
        // now playing
        let ps = cx.players;
        if let Some(p) = ps.iter().find(|p| p.playing).or_else(|| ps.iter().find(|p| !p.title.is_empty())) {
            let p = p.clone();
            Self::tile(c, 0.0, y, w, 76.0, false);
            crate::desktop::rect(c, 12.0, y + 12.0, 52.0, 52.0, 12.0, FILL2);
            match (!p.art.is_empty()).then(|| cx.imgs.get(&p.art, 104, cx.icons)).flatten() {
                Some(pm) => crate::img::draw(c, &pm, 12.0, y + 12.0, 52.0, 52.0, 12.0, true, 1.0),
                None => c.icon(t, 38.0, y + 38.0, "music", 22.0, T3),
            }
            let title = if p.title.is_empty() { p.identity.clone() } else { p.title.clone() };
            let artist = if p.artist.is_empty() { p.identity.clone() } else { p.artist.clone() };
            Self::two_lines(c, t, 76.0, y + 38.0, w - 76.0 - 112.0, &title, Style::ui(13.0, 600.0), T1, &artist, Style::ui(11.5, 400.0), T2);
            let bx = w - 10.0 - 100.0;
            for (i, (icon, size, hit, en)) in [("prevF", 18.0, Hit::Prev, p.can_prev), (if p.playing { "pauseF" } else { "playF" }, 22.0, Hit::Play, true), ("nextF", 18.0, Hit::Next, p.can_next)].into_iter().enumerate() {
                let gx = bx + i as f32 * 34.0;
                let a = if !en { 0.3 } else if self.hov(hit) { 1.0 } else { 0.85 };
                c.icon(t, gx + 16.0, y + 38.0, icon, size, T1.alpha(T1.3 * a));
                if en {
                    self.hits.push(((o.0 + gx, o.1 + y + 22.0, 32.0, 32.0), hit));
                }
            }
            y += 86.0;
        }
        // footer
        {
            let st = Style::ui(11.0, 400.0);
            let lh = crate::desktop::line_h(t, &st);
            crate::desktop::text(c, t, 4.0, y + (34.0 - lh) / 2.0, &format!("Działa {}", self.uptime), &st, T3, false, None);
            let bx = w - 3.0 * 34.0 - 2.0 * 6.0;
            self.btn(c, t, bx, y, "settings", 16.0, T1, Hit::Wallpaper, o);
            self.btn(c, t, bx + 40.0, y, "lock", 16.0, T1, Hit::Lock, o);
            self.btn(c, t, bx + 80.0, y, "power", 16.0, DANGER, Hit::Power, o);
            y += 34.0;
        }
        y
    }

    // the detail page's body (below the head), returns its height
    fn paint_body(&mut self, c: &mut Canvas, cx: &mut Ctx, o: (f32, f32)) -> f32 {
        let t = &mut *cx.t;
        let w = SHEET_W - 32.0;
        let mut y = 0.0;
        match self.shown.as_str() {
            "sound" => {
                y += Self::label(c, t, 0.0, y, "Wyjście") + 8.0;
                self.slider(c, t, 0.0, y, w, Slider::Sink, false, false, o);
                y += 38.0;
                let sinks = self.snd.sinks.clone();
                Self::group(c, 0.0, y, w, sinks.len() as f32 * 40.0 + 8.0);
                for (i, d) in sinks.iter().enumerate() {
                    self.pick_row(c, t, 4.0, y + 4.0 + i as f32 * 40.0, w - 8.0, sink_icon(d), &d.desc, d.name == self.snd.sink, Hit::Sink(i), o);
                }
                y += sinks.len() as f32 * 40.0 + 8.0 + 8.0 + 6.0 + 8.0;
                y += Self::label(c, t, 0.0, y, "Wejście") + 8.0;
                self.slider(c, t, 0.0, y, w, Slider::Source, true, false, o);
                y += 38.0;
                let sources = self.snd.sources.clone();
                if sources.len() > 1 {
                    Self::group(c, 0.0, y, w, sources.len() as f32 * 40.0 + 8.0);
                    for (i, d) in sources.iter().enumerate() {
                        let icon = if d.desc.to_lowercase().contains("camera") || d.desc.to_lowercase().contains("webcam") { "camera" } else { "mic" };
                        self.pick_row(c, t, 4.0, y + 4.0 + i as f32 * 40.0, w - 8.0, icon, &d.desc, d.name == self.snd.source, Hit::Source(i), o);
                    }
                    y += sources.len() as f32 * 40.0 + 8.0 + 8.0;
                }
                y += 6.0 + 8.0;
                y += Self::label(c, t, 0.0, y, "Aplikacje") + 8.0;
                let apps = self.snd.apps.clone();
                let gh = if apps.is_empty() { 44.0 } else { apps.len() as f32 * 64.0 } + 8.0;
                Self::group(c, 0.0, y, w, gh);
                if apps.is_empty() {
                    let st = Style::ui(12.0, 400.0);
                    let s = "Żadna aplikacja nie odtwarza dźwięku";
                    let lh = crate::desktop::line_h(t, &st);
                    let sw = t.width(s, &st);
                    crate::desktop::text(c, t, ((w - sw) / 2.0).round(), y + 4.0 + (44.0 - lh) / 2.0, s, &st, T3, false, None);
                }
                for (i, a) in apps.iter().enumerate() {
                    let (rx, ry) = (4.0, y + 4.0 + i as f32 * 64.0);
                    let icon = cx.apps.lookup(&a.icon).or_else(|| cx.apps.lookup(&a.name)).or_else(|| cx.apps.lookup(a.icon.trim_end_matches("-bin"))).map(|e| e.icon.clone()).unwrap_or(a.icon.to_lowercase());
                    match cx.imgs.get(&icon, 72, cx.icons) {
                        Some(pm) => crate::img::draw(c, &pm, rx + 8.0, ry + 14.0, 36.0, 36.0, 0.0, false, 1.0),
                        None => c.icon(t, rx + 26.0, ry + 32.0, "music", 20.0, T3),
                    }
                    let (nx, nw) = (rx + 54.0, w - 8.0 - 62.0);
                    let ns = Style::ui(12.5, 600.0);
                    let an = t.width(&a.name, &ns).min(nw * 0.5);
                    let (ty, lh) = (ry + 10.0, crate::desktop::line_h(t, &ns));
                    crate::desktop::text(c, t, nx, ty, &a.name, &ns, T1, false, Some(nw * 0.5));
                    let ms = Style::ui(11.5, 400.0);
                    let mlh = crate::desktop::line_h(t, &ms);
                    crate::desktop::text(c, t, nx + an + 6.0, ty + (lh - mlh) / 2.0, &a.media, &ms, T3, false, Some(nw - an - 6.0));
                    self.slider(c, t, nx, ty + lh + 6.0, nw, Slider::App(i), false, true, o);
                }
                y += gh;
            }
            "network" => {
                let n = self.net.clone();
                Self::tile(c, 0.0, y, w, 64.0, false);
                Self::orb(c, t, 12.0, y + 15.0, if n.kind == "wifi" { "wifi" } else { "ethernet" }, n.up, false);
                let title = if n.kind == "wifi" { if n.up { n.conn.clone() } else { "Wi-Fi".into() } } else { "Ethernet".into() };
                let sub = if n.dev.is_empty() { "…".to_string() } else if !n.up { "Rozłączono".into() } else if n.speed > 0 { format!("Połączono · {}", if n.speed >= 1000 { format!("{} Gb/s", (n.speed as f64 / 1000.0).to_string().replace('.', ",")) } else { format!("{} Mb/s", n.speed) }) } else { "Połączono".into() };
                Self::two_lines(c, t, 58.0, y + 32.0, w - 70.0, &title, Style::ui(13.5, 650.0), T1, &sub, Style::ui(11.5, 400.0), if n.up { GREEN } else { T3 });
                y += 64.0 + 8.0 + 4.0 + 8.0;
                y += Self::label(c, t, 0.0, y, "Ruch") + 8.0;
                let tw = (w - 8.0) / 2.0;
                for (i, (icon, label, v, col)) in [("down", "Pobieranie", self.rate.0, ACCENT), ("up", "Wysyłanie", self.rate.1, GREEN)].into_iter().enumerate() {
                    let tx = i as f32 * (tw + 8.0);
                    Self::tile(c, tx, y, tw, 62.0, false);
                    let (ls, vs) = (Style::ui(11.0, 400.0), Style::ui(17.0, 600.0).tnum());
                    let (lh, vh) = (crate::desktop::line_h(t, &ls), crate::desktop::line_h(t, &vs));
                    let ty = y + (62.0 - lh - 2.0 - vh) / 2.0;
                    c.icon(t, tx + 14.0 + 6.5, ty + lh / 2.0, icon, 13.0, col);
                    crate::desktop::text(c, t, tx + 14.0 + 13.0 + 5.0, ty, label, &ls, T2, false, None);
                    crate::desktop::text(c, t, tx + 14.0, ty + lh + 2.0, &format!("{}/s", crate::bar::bytes(v)), &vs, T1, false, None);
                }
                y += 62.0 + 8.0 + 4.0 + 8.0;
                y += Self::label(c, t, 0.0, y, "Szczegóły") + 8.0;
                let rows: Vec<(&str, String)> = [("Połączenie", n.conn.clone()), ("Interfejs", n.dev.clone()), ("Adres IP", n.ip.clone()), ("Brama", n.gw.clone()), ("DNS", n.dns.clone())].into_iter().filter(|r| !r.1.is_empty()).collect();
                Self::group(c, 0.0, y, w, rows.len() as f32 * 36.0 + 8.0);
                for (i, (k, v)) in rows.iter().enumerate() {
                    let ry = y + 4.0 + i as f32 * 36.0;
                    let (ks, vs) = (Style::ui(12.0, 400.0), Style::ui(12.0, 550.0));
                    let lh = crate::desktop::line_h(t, &ks);
                    crate::desktop::text(c, t, 16.0, ry + (36.0 - lh) / 2.0, k, &ks, T2, false, None);
                    let maxw = w - 8.0 - 120.0;
                    let vw = t.width(v, &vs).min(maxw);
                    crate::desktop::text(c, t, w - 4.0 - 12.0 - vw, ry + (36.0 - lh) / 2.0, v, &vs, T1, false, Some(maxw));
                }
                y += rows.len() as f32 * 36.0 + 8.0;
                if n.has_wifi {
                    y += 8.0;
                    Self::group(c, 0.0, y, w, 48.0);
                    self.pick_row(c, t, 4.0, y + 4.0, w - 8.0, "wifi", "Wi-Fi", false, Hit::Wifi, o);
                    y += 48.0;
                }
            }
            _ => {
                // bluetooth
                if !self.bt.on {
                    let st = Style::ui(13.0, 400.0);
                    let s = "Bluetooth jest wyłączony";
                    let sw = t.width(s, &st);
                    crate::desktop::text(c, t, ((w - sw) / 2.0).round(), y + 24.0, s, &st, T3, false, None);
                    return y + 24.0 + crate::desktop::line_h(t, &st) + 24.0;
                }
                let paired: Vec<BtD> = self.paired().into_iter().cloned().collect();
                if !paired.is_empty() {
                    y += Self::label(c, t, 0.0, y, "Moje urządzenia") + 8.0;
                    Self::group(c, 0.0, y, w, paired.len() as f32 * 48.0 + 8.0);
                    for (i, d) in paired.iter().enumerate() {
                        let busy = self.busy.contains_key(&d.path);
                        let sub = if busy { if d.connected { "Rozłączanie…".to_string() } else { "Łączenie…".into() } } else if !d.connected { "Nie połączono".into() } else if let Some(b) = d.battery { format!("Połączono · {}%", (b * 100.0).round()) } else { "Połączono".into() };
                        self.dev_row(c, t, 4.0, y + 4.0 + i as f32 * 48.0, w - 8.0, d, &sub, d.connected, busy, Hit::Paired(i), o);
                    }
                    y += paired.len() as f32 * 48.0 + 8.0 + 8.0;
                }
                y += 4.0 + 8.0;
                Self::label(c, t, 0.0, y, "Inne urządzenia");
                if self.bt.discovering {
                    let st = Style::ui(11.0, 400.0);
                    let s = "Wyszukiwanie…";
                    let sw = t.width(s, &st);
                    let a = 0.675 + 0.325 * (self.born.elapsed().as_millis() as f32 / 1400.0 * std::f32::consts::TAU).cos();
                    crate::desktop::text(c, t, w - 4.0 - sw, y, s, &st, T3.alpha(T3.3 * a), false, None);
                }
                y += 16.0 + 8.0;
                let found: Vec<BtD> = self.found().into_iter().cloned().collect();
                let gh = if found.is_empty() { 44.0 } else { found.len() as f32 * 48.0 } + 8.0;
                Self::group(c, 0.0, y, w, gh);
                if found.is_empty() {
                    let st = Style::ui(12.0, 400.0);
                    let s = "Nie znaleziono urządzeń w pobliżu";
                    let lh = crate::desktop::line_h(t, &st);
                    let sw = t.width(s, &st);
                    crate::desktop::text(c, t, ((w - sw) / 2.0).round(), y + 4.0 + (44.0 - lh) / 2.0, s, &st, T3, false, None);
                }
                for (i, d) in found.iter().enumerate() {
                    let busy = self.busy.contains_key(&d.path);
                    self.dev_row(c, t, 4.0, y + 4.0 + i as f32 * 48.0, w - 8.0, d, if busy { "Parowanie…" } else { "Kliknij, aby sparować" }, false, busy, Hit::Found(i), o);
                }
                y += gh;
            }
        }
        y
    }
    fn sheet_pos(c: &Canvas) -> (f32, f32) {
        // the QML window: top-right, margins 30 / 6, the sheet 10 px inside it
        (c.w() - 6.0 - 420.0 + 10.0, 40.0)
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        if self.discovering {
            let _ = self.req.send(Req::BtDiscover(false));
        }
    }
}

impl Pane for Control {
    fn name(&self) -> &'static str {
        "control"
    }
    fn opened(&mut self, screen: &str) {
        self.screen = screen.to_string();
    }
    fn animating(&self) -> bool {
        self.slide.running() || self.sheet_h.running() || !self.busy.is_empty() || (self.page == "bluetooth" && self.bt.discovering)
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(h) if h != Hit::Sheet)
    }
    fn set_page(&mut self, p: &str) -> bool {
        if p == self.page {
            return false;
        }
        self.go(p, true);
        true
    }
    fn slow(&mut self) -> bool {
        let mut changed = false;
        while let Ok(r) = self.res.try_recv() {
            self.take(r);
            changed = true;
        }
        // live throughput (Data.rxRate / txRate), every 250 ms
        let (r, tx) = rx_tx();
        let dt = self.rx.2.elapsed().as_secs_f64();
        if dt >= 0.9 {
            self.rate = ((r.saturating_sub(self.rx.0)) as f64 / dt, (tx.saturating_sub(self.rx.1)) as f64 / dt);
            self.rx = (r, tx, Instant::now());
            changed |= self.page == "network";
        }
        // the shown page's data every second
        self.tick += 1;
        if self.tick % 4 == 0 {
            let _ = self.req.send(match self.page.as_str() { "network" => Req::Net, "bluetooth" => Req::Bt, _ => Req::Snd });
            if self.page.is_empty() && self.tick % 8 == 0 {
                let _ = self.req.send(Req::Bt);
                self.uptime = uptime();
            }
        }
        changed
    }
    fn data(&mut self, what: &str) -> bool {
        matches!(what, "dnd" | "players" | "phone")
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hits.clear();
        let (sx, sy) = Self::sheet_pos(c);
        let k = self.slide.value();
        // both pages into layers (the sheet clips them), then slid / faded
        let mut home = Canvas::new(SHEET_W as u32, 900);
        let ho = (sx + 16.0 - 60.0 * k, sy + 16.0);
        let hits0 = self.hits.len();
        let home_h = if k < 0.999 { self.paint_home(&mut home, cx, ho) } else { 0.0 };
        if k > 0.5 {
            // the home page does not take clicks while a detail page is in
            self.hits.truncate(hits0);
        }
        let hits1 = self.hits.len();
        let mut detail = Canvas::new(SHEET_W as u32, 900);
        let mut det_h = 0.0;
        if k > 0.001 && !self.shown.is_empty() {
            let dx = sx + 16.0 + 60.0 * (1.0 - k);
            let t = &mut *cx.t;
            // head: back, title, Bluetooth switch / pavucontrol
            self.btn(&mut detail, t, 0.0, 0.0, "chevLeft", 17.0, T1, Hit::Back, (dx, sy + 16.0));
            let title = match self.shown.as_str() { "sound" => "Dźwięk", "network" => "Sieć", _ => "Bluetooth" };
            let st = Style::ui(17.0, 650.0);
            let lh = crate::desktop::line_h(t, &st);
            crate::desktop::text(&mut detail, t, 44.0, (34.0 - lh) / 2.0, title, &st, T1, false, None);
            let w = SHEET_W - 32.0;
            if self.shown == "bluetooth" {
                let on = self.bt.on;
                let (bx, by) = (w - 44.0, 4.0);
                crate::desktop::rect(&mut detail, bx, by, 44.0, 26.0, 13.0, if on { GREEN } else { FILL2 });
                crate::desktop::circle(&mut detail, if on { bx + 44.0 - 13.0 } else { bx + 13.0 }, by + 13.0, 10.0, WHITE);
                self.hits.push(((dx + bx, sy + 16.0 + by, 44.0, 26.0), Hit::BtSwitch));
            } else if self.shown == "sound" {
                self.btn(&mut detail, t, w - 34.0, 0.0, "adjust", 16.0, T1, Hit::Pavu, (dx, sy + 16.0));
            }
            // body, scrolled, clipped to 620 px
            let mut body = Canvas::new(w as u32, 1600);
            let bo = (dx, sy + 16.0 + 46.0 - self.scroll);
            let hb = self.hits.len();
            let bh = self.paint_body(&mut body, cx, bo);
            self.body_h = bh;
            let vis = bh.min(BODY_MAX);
            self.scroll = self.scroll.clamp(0.0, (bh - vis).max(0.0));
            // clicks only inside the visible part of the body
            let (top, bot) = (sy + 16.0 + 46.0, sy + 16.0 + 46.0 + vis);
            let mut kept = Vec::new();
            for h in self.hits.drain(hb..) {
                let (x, y, w2, h2) = h.0;
                let (y0, y1) = (y.max(top), (y + h2).min(bot));
                if y1 > y0 {
                    kept.push(((x, y0, w2, y1 - y0), h.1));
                }
            }
            self.hits.extend(kept);
            if let Some(view) = body.pm.clone_rect(tiny_skia::IntRect::from_xywh(0, self.scroll as i32, w as u32, vis.max(1.0) as u32).unwrap()) {
                detail.pm.draw_pixmap(0, 46, view.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
            }
            det_h = 46.0 + vis;
            if k < 0.5 {
                self.hits.truncate(hits1);
            }
        }
        // the sheet follows the page's height (260 ms)
        let want = if self.page.is_empty() { home_h.max(1.0) } else { det_h } + 32.0;
        if self.sheet_h.target() <= 0.0 {
            self.sheet_h = Anim::new(want);
        } else if (self.sheet_h.target() - want).abs() > 0.5 && want > 32.0 {
            self.sheet_h.set(want, 260.0, Ease::OutCubic);
        }
        let shh = self.sheet_h.value().round();
        glass(c, sx, sy, SHEET_W, shh, 26.0, 0.82);
        self.hits.insert(0, ((sx, sy, SHEET_W, shh), Hit::Sheet));
        // the layers, clipped to the sheet's inside
        let clip_h = (shh - 16.0).max(1.0) as u32;
        let ha = (1.0 - k / 0.7).clamp(0.0, 1.0); // opacity 180 ms of 260
        let da = (k / 0.7).clamp(0.0, 1.0);
        for (layer, a, x) in [(&home, ha, -60.0 * k), (&detail, da, 60.0 * (1.0 - k))] {
            if a <= 0.0 {
                continue;
            }
            let Some(view) = layer.pm.clone_rect(tiny_skia::IntRect::from_xywh(0, 0, (SHEET_W - 32.0) as u32, clip_h.min(layer.pm.height())).unwrap()) else { continue };
            // the sheet clips (clipContent): only what is inside its rect
            let left = (sx + 16.0 + x).round() as i32;
            let mut paint = tiny_skia::PixmapPaint::default();
            paint.opacity = a;
            let mask_rect = tiny_skia::Rect::from_xywh(sx, sy, SHEET_W, shh).unwrap();
            let mut mask = tiny_skia::Mask::new(c.pm.width(), c.pm.height()).unwrap();
            mask.fill_path(&tiny_skia::PathBuilder::from_rect(mask_rect), tiny_skia::FillRule::Winding, false, tiny_skia::Transform::identity());
            c.pm.draw_pixmap(left, (sy + 16.0) as i32, view.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&mask));
        }
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let hit = self.hit();
        if let Some((s, tx, tw)) = self.drag {
            match ev {
                Ptr::Motion => {
                    let h = if matches!(s, Slider::App(_)) { 24.0 } else { 30.0 };
                    if x - tx >= h {
                        self.set_vol(s, (x - tx) / tw);
                    }
                    return Act::Redraw;
                }
                Ptr::Release(_) => {
                    self.drag = None;
                    return Act::Redraw;
                }
                _ => {}
            }
        }
        match ev {
            Ptr::Enter | Ptr::Motion | Ptr::Leave => Act::Redraw,
            Ptr::Press(_) => {
                self.pressed_outside = hit.is_none();
                if let Some(Hit::Slider(s)) = hit {
                    let r = self.hits.iter().rev().find(|h| h.1 == Hit::Slider(s)).map(|h| h.0).unwrap();
                    if x - r.0 < r.3 {
                        self.mute(s);
                    } else {
                        self.drag = Some((s, r.0, r.2));
                        self.set_vol(s, (x - r.0) / r.2);
                    }
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Release(_) => {
                let bin = format!("{}/.local/bin/mist-ui-bin", std::env::var("HOME").unwrap_or_default());
                match hit {
                    None if self.pressed_outside => Act::Close,
                    Some(Hit::Link(0)) | Some(Hit::Orb(0)) => { self.go("network", true); Act::Redraw }
                    Some(Hit::Link(1)) => { self.go("bluetooth", true); Act::Redraw }
                    Some(Hit::Orb(1)) | Some(Hit::BtSwitch) => {
                        let on = !self.bt.on;
                        self.bt.on = on;
                        let _ = self.req.send(Req::BtPower(on));
                        Act::Redraw
                    }
                    Some(Hit::Link(2)) | Some(Hit::Orb(2)) => Act::Toggle("phone"),
                    Some(Hit::Dnd) => Act::Exec(vec![bin, "msg".into(), "notif".into(), "dnd".into(), (!_cx.st.dnd).to_string()]),
                    Some(Hit::Night) => {
                        self.night = !self.night;
                        let _ = std::fs::create_dir_all(format!("{}/.local/state/mist", std::env::var("HOME").unwrap_or_default()));
                        let _ = std::fs::write(night_path(), if self.night { "1" } else { "0" });
                        Act::Exec(night_cmd(self.night))
                    }
                    Some(Hit::SoundHead) => { self.go("sound", true); Act::Redraw }
                    Some(Hit::Prev) => Act::Exec(vec![bin, "msg".into(), "mist".into(), "media".into(), "prev".into()]),
                    Some(Hit::Play) => Act::Exec(vec![bin, "msg".into(), "mist".into(), "media".into(), "toggle".into()]),
                    Some(Hit::Next) => Act::Exec(vec![bin, "msg".into(), "mist".into(), "media".into(), "next".into()]),
                    Some(Hit::Wallpaper) => Act::Toggle("wallpaper"),
                    Some(Hit::Lock) => Act::Run(vec![bin, "lock".into()]),
                    Some(Hit::Power) => Act::Toggle("session"),
                    Some(Hit::Back) => { self.go("", true); Act::Redraw }
                    Some(Hit::Pavu) => Act::Run(vec!["pavucontrol".into()]),
                    Some(Hit::Sink(i)) => {
                        if let Some(d) = self.snd.sinks.get(i) {
                            let n = d.name.clone();
                            self.snd.sink = n.clone();
                            let _ = self.req.send(Req::Pactl(vec!["pactl".into(), "set-default-sink".into(), n]));
                        }
                        Act::Redraw
                    }
                    Some(Hit::Source(i)) => {
                        if let Some(d) = self.snd.sources.get(i) {
                            let n = d.name.clone();
                            self.snd.source = n.clone();
                            let _ = self.req.send(Req::Pactl(vec!["pactl".into(), "set-default-source".into(), n]));
                        }
                        Act::Redraw
                    }
                    Some(Hit::Paired(i)) => {
                        if let Some(d) = self.paired().get(i).map(|d| (*d).clone()) {
                            self.busy.insert(d.path.clone(), Instant::now());
                            let _ = self.req.send(Req::BtConnect(d.path, !d.connected));
                        }
                        Act::Redraw
                    }
                    Some(Hit::Found(i)) => {
                        if let Some(d) = self.found().get(i).map(|d| (*d).clone()) {
                            self.busy.insert(d.path.clone(), Instant::now());
                            let _ = self.req.send(Req::BtPair(d.path));
                        }
                        Act::Redraw
                    }
                    Some(Hit::Wifi) => Act::Exec(vec!["sh".into(), "-c".into(), "[ \"$(nmcli radio wifi)\" = enabled ] && nmcli radio wifi off || nmcli radio wifi on".into()]),
                    _ => Act::None,
                }
            }
            Ptr::Wheel(d) => {
                // a slider under the pointer: ±5 %; else the detail page scrolls
                if let Some(Hit::Slider(s)) = hit {
                    let (v, _, _) = self.slider_val(s);
                    self.set_vol(s, v - d.signum() * 0.05);
                } else if !self.page.is_empty() {
                    self.scroll = (self.scroll + d * 40.0).clamp(0.0, (self.body_h - self.body_h.min(BODY_MAX)).max(0.0));
                }
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape => {
                if self.page.is_empty() {
                    Act::Close
                } else {
                    self.go("", true);
                    Act::Redraw
                }
            }
            _ => Act::None,
        }
    }
}
