// MPRIS players for the island (was Quickshell.Services.Mpris): every org.mpris.MediaPlayer2.* on the session
// bus, their metadata / state / capabilities, kept up to date from PropertiesChanged + Seeked + NameOwnerChanged.
// Remote cover art (http) is downloaded once into ~/.cache/mist/art. Commands come back on a channel.
use crate::sys::Msg;
use calloop::channel::Sender;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

#[derive(Clone, Debug, Default)]
pub struct Player {
    pub bus: String,
    pub identity: String,
    pub entry: String,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: String, // local file path or ""
    pub length: f64, // s
    pub position: f64,
    pub pos_at: Option<Instant>,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_toggle: bool,
    pub can_seek: bool,
    pub shuffle: Option<bool>,
    pub loop_status: Option<String>,
    pub track_id: String,
}
impl Player {
    // the position now (extrapolated while playing)
    pub fn pos(&self) -> f64 {
        let p = self.position + if self.playing { self.pos_at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0) } else { 0.0 };
        if self.length > 0.0 { p.min(self.length) } else { p }
    }
}

pub enum Cmd {
    Toggle(String),
    Next(String),
    Prev(String),
    Seek(String, String, f64),
    Shuffle(String, bool),
    Loop(String, String),
    Refresh,
}

const P: &str = "org.mpris.MediaPlayer2.Player";

fn s(v: Option<&OwnedValue>) -> String {
    v.and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
}
fn b(v: Option<&OwnedValue>) -> bool {
    v.and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
}
fn art_local(url: &str) -> String {
    if let Some(p) = url.strip_prefix("file://") {
        return p.to_string();
    }
    if url.starts_with("http") {
        let h = url.bytes().fold(1469598103934665603u64, |h, c| (h ^ c as u64).wrapping_mul(1099511628211));
        let dir = format!("{}/.cache/mist/art", std::env::var("HOME").unwrap_or_default());
        let f = format!("{dir}/{h:016x}");
        if !std::path::Path::new(&f).exists() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("curl").args(["-sfL", "--max-time", "8", "-o", &f, url]).status();
        }
        return if std::path::Path::new(&f).exists() { f } else { String::new() };
    }
    String::new()
}

fn read(c: &Connection, bus: &str) -> Option<Player> {
    let m = c.call_method(Some(bus), "/org/mpris/MediaPlayer2", Some("org.freedesktop.DBus.Properties"), "GetAll", &(P,)).ok()?;
    let p: HashMap<String, OwnedValue> = m.body().deserialize().ok()?;
    let root: HashMap<String, OwnedValue> = c
        .call_method(Some(bus), "/org/mpris/MediaPlayer2", Some("org.freedesktop.DBus.Properties"), "GetAll", &("org.mpris.MediaPlayer2",))
        .ok()
        .and_then(|m| m.body().deserialize().ok())
        .unwrap_or_default();
    let mut pl = Player { bus: bus.into(), identity: s(root.get("Identity")), entry: s(root.get("DesktopEntry")), ..Default::default() };
    pl.playing = s(p.get("PlaybackStatus")) == "Playing";
    if let Some(md) = p.get("Metadata").and_then(|v| <HashMap<String, OwnedValue>>::try_from(v.clone()).ok()) {
        pl.title = s(md.get("xesam:title"));
        pl.album = s(md.get("xesam:album"));
        pl.artist = md.get("xesam:artist").and_then(|v| <Vec<String>>::try_from(v.clone()).ok()).map(|v| v.join(", ")).unwrap_or_default();
        pl.length = md.get("mpris:length").and_then(|v| { let v: &Value = v; match v { Value::I64(x) => Some(*x as f64), Value::U64(x) => Some(*x as f64), Value::I32(x) => Some(*x as f64), _ => None } }).unwrap_or(0.0) / 1e6;
        pl.art = art_local(&s(md.get("mpris:artUrl")));
        pl.track_id = md.get("mpris:trackid").and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok()).map(|o| o.to_string()).unwrap_or_default();
    }
    pl.position = p.get("Position").and_then(|v| i64::try_from(v.clone()).ok()).unwrap_or(0) as f64 / 1e6;
    pl.pos_at = Some(Instant::now());
    pl.can_next = b(p.get("CanGoNext"));
    pl.can_prev = b(p.get("CanGoPrevious"));
    pl.can_toggle = b(p.get("CanPlay")) || b(p.get("CanPause"));
    pl.can_seek = b(p.get("CanSeek"));
    pl.shuffle = p.get("Shuffle").and_then(|v| bool::try_from(v.clone()).ok());
    pl.loop_status = p.get("LoopStatus").and_then(|v| String::try_from(v.clone()).ok());
    Some(pl)
}

pub fn spawn(tx: Sender<Msg>, cmds: std::sync::mpsc::Receiver<Cmd>) {
    let _ = std::thread::Builder::new().name("mpris".into()).stack_size(512 * 1024).spawn(move || {
        let Ok(c) = Connection::session() else { return };
        for r in ["type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'", "type='signal',interface='org.mpris.MediaPlayer2.Player',member='Seeked'", "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged'"] {
            let _ = c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(r,));
        }
        let names = |c: &Connection| -> Vec<String> {
            c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListNames", &())
                .ok()
                .and_then(|m| m.body().deserialize::<Vec<String>>().ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
                .collect()
        };
        let refresh = |c: &Connection| {
            let list: Vec<Player> = names(c).iter().filter_map(|n| read(c, n)).collect();
            let _ = tx.send(Msg::Players(list));
        };
        refresh(&c);
        let stream = zbus::blocking::MessageIterator::from(&c);
        let (stx, srx) = std::sync::mpsc::channel::<()>();
        let _ = std::thread::Builder::new().name("mpris-sig".into()).stack_size(256 * 1024).spawn(move || {
            for m in stream {
                let Ok(m) = m else { continue };
                let h = m.header();
                if h.message_type() != zbus::message::Type::Signal {
                    continue;
                }
                let member = h.member().map(|x| x.to_string()).unwrap_or_default();
                if member == "NameOwnerChanged" {
                    if let Ok((name, _, _)) = m.body().deserialize::<(String, String, String)>() {
                        if !name.starts_with("org.mpris.MediaPlayer2.") {
                            continue;
                        }
                    }
                }
                let _ = stx.send(());
            }
        });
        let call = |bus: &str, method: &str| {
            let _ = c.call_method(Some(bus), "/org/mpris/MediaPlayer2", Some(P), method, &());
        };
        let set = |bus: &str, prop: &str, v: Value| {
            let _ = c.call_method(Some(bus), "/org/mpris/MediaPlayer2", Some("org.freedesktop.DBus.Properties"), "Set", &(P, prop, v));
        };
        loop {
            let mut dirty = false;
            match cmds.recv_timeout(Duration::from_millis(120)) {
                Ok(Cmd::Toggle(b)) => call(&b, "PlayPause"),
                Ok(Cmd::Next(b)) => call(&b, "Next"),
                Ok(Cmd::Prev(b)) => call(&b, "Previous"),
                Ok(Cmd::Seek(b, track, pos)) => {
                    if let Ok(o) = zbus::zvariant::ObjectPath::try_from(track.as_str()) {
                        let _ = c.call_method(Some(b.as_str()), "/org/mpris/MediaPlayer2", Some(P), "SetPosition", &(o, (pos * 1e6) as i64));
                    }
                    dirty = true;
                }
                Ok(Cmd::Shuffle(b, on)) => set(&b, "Shuffle", Value::from(on)),
                Ok(Cmd::Loop(b, l)) => set(&b, "LoopStatus", Value::from(l.as_str())),
                Ok(Cmd::Refresh) => dirty = true,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            while srx.try_recv().is_ok() {
                dirty = true;
            }
            if dirty {
                refresh(&c);
            }
        }
    });
}
