// MPRIS for Mist Player (org.mpris.MediaPlayer2.mistplayer): the island, the media keys and other remote
// controls see it. A thread owns the bus connection and sleeps on a channel: the player sends it what changed
// (no polling), the methods come back to the player's loop as Wake::Mpris. Position is the last known spot
// moved on by the clock while playing (MPRIS clients interpolate the same way), so mpv's time-pos is not
// observed for it.
use super::Wake;
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

pub type Tx = calloop::channel::Sender<Box<dyn std::any::Any + Send>>;
const PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Debug)]
pub enum Req {
    Raise,
    Quit,
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Prev,
    Seek(f64),
    SetPosition(f64),
    Open(String),
    Volume(f64),
    Rate(f64),
    Loop(u8),
    Shuffle(bool),
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Snap {
    pub status: &'static str,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub url: String,
    pub length: f64,
    pub track: i32,
    pub volume: f64,
    pub rate: f64,
    pub repeat: u8,
    pub shuffle: bool,
    pub can_next: bool,
    pub can_prev: bool,
}

#[derive(Clone, Copy)]
struct Spot {
    pos: f64,
    at: Instant,
    rate: f64,
    playing: bool,
}
impl Spot {
    fn now(&self) -> f64 {
        self.pos + if self.playing { self.at.elapsed().as_secs_f64() * self.rate } else { 0.0 }
    }
}

enum Up {
    State(Box<Snap>),
    Seeked(f64),
}

pub struct Mpris {
    up: mpsc::Sender<Up>,
    spot: Arc<Mutex<Spot>>,
    last: Snap,
}

fn send(tx: &Tx, r: Req) {
    let _ = tx.send(Box::new(Wake::Mpris(r)));
}

struct Root(Tx);
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        send(&self.0, Req::Raise)
    }
    fn quit(&self) {
        send(&self.0, Req::Quit)
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "Mist Player".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "mist-player".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".into(), "http".into(), "https".into()]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        ["video/mp4", "video/x-matroska", "video/webm", "audio/mpeg", "audio/flac", "audio/ogg", "audio/mp4"].iter().map(|s| s.to_string()).collect()
    }
}

struct Player {
    tx: Tx,
    s: Snap,
    spot: Arc<Mutex<Spot>>,
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        send(&self.tx, Req::Next)
    }
    fn previous(&self) {
        send(&self.tx, Req::Prev)
    }
    fn pause(&self) {
        send(&self.tx, Req::Pause)
    }
    fn play_pause(&self) {
        send(&self.tx, Req::PlayPause)
    }
    fn stop(&self) {
        send(&self.tx, Req::Stop)
    }
    fn play(&self) {
        send(&self.tx, Req::Play)
    }
    fn seek(&self, offset: i64) {
        send(&self.tx, Req::Seek(offset as f64 / 1e6))
    }
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        if track_id.as_str() == track_path(self.s.track) {
            send(&self.tx, Req::SetPosition(position as f64 / 1e6))
        }
    }
    fn open_uri(&self, uri: String) {
        send(&self.tx, Req::Open(uri))
    }
    #[zbus(signal)]
    async fn seeked(em: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.s.status.into()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        ["None", "Playlist", "Track"][self.s.repeat.min(2) as usize].into()
    }
    #[zbus(property)]
    fn set_loop_status(&mut self, v: String) {
        send(&self.tx, Req::Loop(match v.as_str() {
            "Playlist" => 1,
            "Track" => 2,
            _ => 0,
        }))
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.s.rate
    }
    #[zbus(property)]
    fn set_rate(&mut self, r: f64) {
        if r > 0.0 {
            send(&self.tx, Req::Rate(r.clamp(0.25, 3.0)))
        }
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.s.shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&mut self, on: bool) {
        send(&self.tx, Req::Shuffle(on))
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            if let Ok(o) = OwnedValue::try_from(v) {
                m.insert(k.to_string(), o);
            }
        };
        if let Ok(p) = ObjectPath::try_from(track_path(self.s.track)) {
            put("mpris:trackid", Value::from(p));
        }
        if self.s.url.is_empty() {
            return m;
        }
        put("xesam:title", Value::from(self.s.title.clone()));
        if !self.s.artist.is_empty() {
            put("xesam:artist", Value::from(vec![self.s.artist.clone()]));
        }
        if !self.s.album.is_empty() {
            put("xesam:album", Value::from(self.s.album.clone()));
        }
        put("xesam:url", Value::from(self.s.url.clone()));
        if self.s.length > 0.0 {
            put("mpris:length", Value::from((self.s.length * 1e6) as i64));
        }
        m
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.s.volume
    }
    #[zbus(property)]
    fn set_volume(&mut self, v: f64) {
        send(&self.tx, Req::Volume(v.clamp(0.0, 1.0)))
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        (self.spot.lock().map(|s| s.now()).unwrap_or(0.0) * 1e6) as i64
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.25
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        3.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.s.can_next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.s.can_prev
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        !self.s.url.is_empty()
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        !self.s.url.is_empty()
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.s.length > 0.0
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

fn track_path(i: i32) -> String {
    format!("/org/mist/player/track/{}", i.max(0))
}

impl Mpris {
    pub fn start(tx: Tx) -> Mpris {
        let (up, rx) = mpsc::channel::<Up>();
        let spot = Arc::new(Mutex::new(Spot { pos: 0.0, at: Instant::now(), rate: 1.0, playing: false }));
        let sp = spot.clone();
        let _ = std::thread::Builder::new().name("mpris".into()).stack_size(256 * 1024).spawn(move || {
            let conn = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.serve_at(PATH, Root(tx.clone())))
                .and_then(|b| b.serve_at(PATH, Player { tx, s: Snap { status: "Stopped", rate: 1.0, ..Default::default() }, spot: sp }))
                .and_then(|b| b.build());
            let Ok(conn) = conn else { return };
            if let Err(e) = conn.request_name("org.mpris.MediaPlayer2.mistplayer") {
                eprintln!("player: mpris: {e}");
                return;
            }
            let Ok(iface) = conn.object_server().interface::<_, Player>(PATH) else { return };
            for u in rx {
                match u {
                    Up::Seeked(p) => {
                        let _ = zbus::block_on(Player::seeked(iface.signal_emitter(), (p * 1e6) as i64));
                    }
                    Up::State(s) => {
                        let old = std::mem::replace(&mut iface.get_mut().s, *s);
                        let p = iface.get();
                        let em = iface.signal_emitter();
                        zbus::block_on(async {
                            if old.status != p.s.status {
                                let _ = p.playback_status_changed(em).await;
                            }
                            if (old.title.as_str(), old.artist.as_str(), old.album.as_str(), old.url.as_str(), old.length, old.track) != (p.s.title.as_str(), p.s.artist.as_str(), p.s.album.as_str(), p.s.url.as_str(), p.s.length, p.s.track) {
                                let _ = p.metadata_changed(em).await;
                                let _ = p.can_seek_changed(em).await;
                                let _ = p.can_play_changed(em).await;
                                let _ = p.can_pause_changed(em).await;
                            }
                            if old.volume != p.s.volume {
                                let _ = p.volume_changed(em).await;
                            }
                            if old.rate != p.s.rate {
                                let _ = p.rate_changed(em).await;
                            }
                            if old.repeat != p.s.repeat {
                                let _ = p.loop_status_changed(em).await;
                            }
                            if old.shuffle != p.s.shuffle {
                                let _ = p.shuffle_changed(em).await;
                            }
                            if (old.can_next, old.can_prev) != (p.s.can_next, p.s.can_prev) {
                                let _ = p.can_go_next_changed(em).await;
                                let _ = p.can_go_previous_changed(em).await;
                            }
                        });
                    }
                }
            }
        });
        Mpris { up, spot, last: Snap::default() }
    }
    // the state the bus shows; a no-op when nothing changed
    pub fn update(&mut self, s: Snap) {
        if s != self.last {
            self.last = s.clone();
            let _ = self.up.send(Up::State(Box::new(s)));
        }
    }
    // the position right now (after a seek, a pause, a new file)
    pub fn spot(&self, pos: f64, playing: bool, rate: f64) {
        if let Ok(mut s) = self.spot.lock() {
            *s = Spot { pos, at: Instant::now(), rate, playing };
        }
    }
    pub fn seeked(&self, pos: f64) {
        let _ = self.up.send(Up::Seeked(pos));
    }
}
