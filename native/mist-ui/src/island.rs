// Dynamic Island (Island.qml 1:1): a pure black shape at the top centre that springs between views.
//   idle    [art | date · time | live activities: minutnik, submap, Caps Lock, camera / mic / sharing dots, EQ]
//   glance  hover when nothing plays: weather, the next events, minutnik
//   player  hover when something plays: art, title, controls, seekable progress (volume while it changes)
//   osd     volume of the output / the microphone
//   call    ringing (answer / reject) or a call in progress (mute, audio here / on the phone, keypad, hang up)
//   notif   a notification (progress, "+N" waiting, hover = full text; phone notes: reply, actions, dismiss)
//   track   track change / play / pause from outside
//   info    one line about something that happened (devices, network, clipboard, Hyprland, calendar...)
// Transient events go through a small priority queue (same key = update in place).
use crate::anim::{Anim, Ease};
use crate::apps::{Apps, Icons};
use crate::data;
use crate::gfx::{wrap, Canvas, Rgba};
use crate::img::{self, Images};
use crate::media::Player;
use crate::sys::{Audio, BtDev};
use crate::text::{Style, Text};
use crate::theme::*;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// the equalizer of a settled frame: its view layer's transform and opacity, and the box (view coords,
// window pixels) that a frame of the bars alone repaints
#[derive(Clone, Copy)]
struct EqSpot {
    tf: tiny_skia::Transform,
    op: f32,
    run: bool,
    lx: f32, // the bars' origin in view coords
    ly: f32,
    bx: i32, // the small layer's origin in view coords (integer: same pixel phase as the view)
    by: i32,
    dst: (i32, i32, i32, i32), // window pixels repainted: x, y, w, h
}
const EQ_BOX: (i32, i32) = (24, 20); // 4 bars (5 px pitch, 3 wide, <= 14 high) + 3 px around

pub const WIN_W: u32 = 560;
pub const WIN_H: u32 = 240;
const ORANGE_I: Rgba = Rgba(0xef as f32 / 255.0, 0xb0 as f32 / 255.0, 0x8a as f32 / 255.0, 1.0);
const PURPLE: Rgba = Rgba(0xbb as f32 / 255.0, 0xa9 as f32 / 255.0, 0xee as f32 / 255.0, 1.0);
const INK: Rgba = Rgba(0x0e as f32 / 255.0, 0x14 as f32 / 255.0, 0x20 as f32 / 255.0, 1.0);
const RIM: Rgba = Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16);

// ------------------------------------------------------------------ springs (QQuickSpringAnimation)
#[derive(Clone, Copy)]
pub struct Spring {
    pub cur: f32,
    pub to: f32,
    vel: f32,
    spring: f32,
    damping: f32,
    eps: f32,
    last: Instant,
    pub moving: bool,
}
impl Spring {
    pub fn new(v: f32, spring: f32, damping: f32, eps: f32) -> Spring {
        Spring { cur: v, to: v, vel: 0.0, spring, damping, eps, last: Instant::now(), moving: false }
    }
    pub fn set(&mut self, to: f32) {
        if (to - self.to).abs() > 1e-4 {
            if !self.moving {
                self.last = Instant::now();
            }
            self.to = to;
            self.moving = true;
        }
    }
    // Qt steps in 16 ms ticks: v += spring * diff - damping * v; x += v * 16 / 1000
    pub fn step(&mut self) {
        if !self.moving {
            return;
        }
        let el = self.last.elapsed().as_millis() as u32;
        let n = el / 16;
        if n == 0 {
            return;
        }
        self.last += Duration::from_millis(n as u64 * 16);
        for _ in 0..n.min(200) {
            let diff = self.to - self.cur;
            self.vel += self.spring * diff - self.damping * self.vel;
            self.cur += self.vel * 16.0 / 1000.0 * 60.0 / 60.0;
            if self.vel.abs() < self.eps && (self.to - self.cur).abs() < self.eps {
                self.cur = self.to;
                self.vel = 0.0;
                self.moving = false;
                break;
            }
        }
    }
}

// ------------------------------------------------------------------ events
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Act {
    #[default]
    None,
    Nid(u32, Vec<String>, String), // notification id, app names to focus, desktop entry
    Focus(String),
    Open(String),
    Popup(String),
    Phone(String),
}

#[derive(Clone, Debug, Default)]
pub struct Ev {
    pub view: &'static str,
    pub key: String,
    pub prio: i32,
    pub dur: u64,
    pub icon: String,
    pub color: Option<Rgba>,
    pub title: String,
    pub sub: String,
    pub img: String,
    pub photo: bool,
    pub swatch: Option<Rgba>,
    pub hold: bool,
    pub pulse: bool,
    pub app: String,
    pub body: String,
    pub urgent: bool,
    pub value: i32,
    pub mic: bool,
    pub state: String,
    pub act: Act,
    pub phone: Option<(String, String)>, // phone app, raw body
    pub links: Vec<String>,
    pub codes: Vec<String>,
    pub desktop_actions: Vec<(String, String)>,
    pub note_id: u32,
}
fn info(key: &str, prio: i32, dur: u64, icon: &str, color: Rgba, title: &str, sub: &str) -> Ev {
    Ev { view: "info", key: key.into(), prio, dur, icon: icon.into(), color: Some(color), title: title.into(), sub: sub.into(), value: -1, ..Default::default() }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Ctl(&'static str),      // shuffle prev play next loop
    Chip,
    Seek,
    CallAnswer,
    CallHangup,
    CallMute,
    CallAudio,
    Keypad,
    Key(String),
    NoteReply,
    NoteAction(String),
    NoteDismiss,
    ReplySend,
    CopyCode(String),
    CopyLink(String),
    OpenLink(String),
    DesktopAction(u32, String),
}

pub struct Island {
    pub ev: Option<Ev>,
    pub queue: Vec<Ev>,
    last: HashMap<&'static str, Ev>,
    ev_until: Option<Instant>,
    start: Instant,
    pub armed: bool,
    pub expanded: bool,
    glance_demo: bool,
    pub hover: bool,
    open_at: Option<Instant>,
    close_at: Option<Instant>,
    pub replying: bool,
    pub reply: String,
    // media
    pub players: Vec<Player>,
    pick: Option<usize>,
    self_until: Option<Instant>,
    last_track: String,
    last_playing: bool,
    track_at: Option<Instant>,
    state_at: Option<Instant>,
    // audio
    pub audio: Audio,
    audio_seen: bool,
    dev_armed_at: Option<Instant>,
    vol_recent_until: Option<Instant>,
    // Hyprland
    layout: String,
    pub sharing: bool,
    cast_on: bool,
    share_since: Instant,
    share_at: Option<Instant>,
    submap: String,
    shot_at: Option<Instant>,
    pub fullscreen: bool,
    // devices
    bt_on: Option<bool>,
    bt_devs: Vec<BtDev>,
    bt_low: HashMap<String, u8>,
    profile: String,
    caps: bool,
    num: bool,
    cam_busy: bool,
    cam_apps: Vec<String>,
    conn: String,
    primary: String,
    // phone
    last_call: Option<Value>,
    pub keypad: bool,
    call_at: Option<Instant>,
    last_note: (String, Option<Instant>),
    // data
    pub weather: Value,
    pub calendar: Value,
    reminded: HashMap<String, u8>,
    rain_at: Option<Instant>,
    // minutnik
    timer_end: f64, // unix seconds
    timer_total: f64,
    timer_label: String,
    // look
    pub w: Spring,
    pub h: Spring,
    pub y: Spring,
    bump_at: Option<Instant>,
    panes: HashMap<&'static str, (Anim, Spring, Option<Instant>)>, // opacity, scale, fade-in start (after 110 ms)
    shadow: Anim,
    eq: [Anim; 4],
    eq_at: Instant,
    eq_pos: Option<(f32, f32, bool)>, // where the view being drawn put the equalizer (view coords), running
    eq_spot: Option<EqSpot>,          // the settled frame's equalizer: redrawn alone (see eq_frame)
    hidden: bool,                     // last frame was hidden under a fullscreen window
    pub hits: Vec<(f32, f32, f32, f32, Hit)>,
    pub hover_hit: Option<Hit>,
    pub seek_drag: Option<f32>,
    pub phone_name: String,
    pub track_h: Anim,
    note_bar: Anim,
}

impl Island {
    pub fn new() -> Island {
        let now = Instant::now();
        Island {
            ev: None, queue: Vec::new(), last: HashMap::new(), ev_until: None, start: now, armed: false, expanded: false, glance_demo: false, hover: false,
            open_at: None, close_at: None, replying: false, reply: String::new(), players: Vec::new(), pick: None, self_until: None, last_track: String::new(),
            last_playing: false, track_at: None, state_at: None, audio: Audio::default(), audio_seen: false, dev_armed_at: Some(now), vol_recent_until: None,
            layout: String::new(), sharing: false, cast_on: false, share_since: now, share_at: None, submap: String::new(), shot_at: None, fullscreen: false,
            bt_on: None, bt_devs: Vec::new(), bt_low: HashMap::new(), profile: String::new(), caps: false, num: true, cam_busy: false, cam_apps: Vec::new(),
            conn: "full".into(), primary: String::new(), last_call: None, keypad: false, call_at: None, last_note: (String::new(), None),
            weather: Value::Null, calendar: Value::Null, reminded: HashMap::new(), rain_at: None, timer_end: 0.0, timer_total: 0.0, timer_label: String::new(),
            w: Spring::new(120.0, 3.4, 0.3, 0.25), h: Spring::new(30.0, 3.4, 0.3, 0.25), y: Spring::new(5.0, 3.0, 0.32, 0.25), bump_at: None,
            panes: HashMap::new(), shadow: Anim::new(0.0), eq: [Anim::new(4.0); 4], eq_at: now, eq_pos: None, eq_spot: None, hidden: false, hits: Vec::new(), hover_hit: None, seek_drag: None,
            phone_name: String::new(), track_h: Anim::new(5.0), note_bar: Anim::new(0.0),
        }
    }

    // ------------------------------------------------------------ queue
    pub fn push(&mut self, mut e: Ev) {
        if e.prio == 0 {
            e.prio = 1;
        }
        if e.dur == 0 {
            e.dur = 3000;
        }
        if self.replying && !(self.ev.as_ref().map(|v| !e.key.is_empty() && v.key == e.key).unwrap_or(false)) {
            self.queue.push(e);
            let n = self.queue.len();
            if n > 8 {
                self.queue.drain(0..n - 8);
            }
            return;
        }
        if let Some(cur) = &self.ev {
            if !e.key.is_empty() && cur.key == e.key {
                self.ev_until = Some(Instant::now() + Duration::from_millis(e.dur));
                self.set_ev(Some(e));
                return;
            }
        }
        if self.ev.as_ref().map(|c| e.prio >= c.prio).unwrap_or(true) {
            if let Some(cur) = self.ev.clone() {
                if cur.view == "notif" && e.view != "notif" {
                    self.queue.retain(|q| q.key != cur.key || cur.key.is_empty());
                    self.queue.insert(0, cur);
                    self.queue.truncate(8);
                }
            }
            self.ev_until = Some(Instant::now() + Duration::from_millis(e.dur));
            if e.pulse {
                self.pulse();
            }
            self.set_ev(Some(e));
            return;
        }
        self.queue.retain(|q| e.key.is_empty() || q.key != e.key);
        self.queue.push(e);
        let n = self.queue.len();
        if n > 8 {
            self.queue.drain(0..n - 8);
        }
    }
    fn set_ev(&mut self, e: Option<Ev>) {
        if let Some(e) = &e {
            self.last.insert(e.view, e.clone());
        }
        self.ev = e;
    }
    pub fn next(&mut self) {
        self.replying = false;
        if !self.queue.is_empty() {
            let e = self.queue.remove(0);
            let d = if e.view == "notif" { e.dur.clamp(3000, 4500) } else { e.dur.min(3000) };
            self.ev_until = Some(Instant::now() + Duration::from_millis(d));
            self.set_ev(Some(e));
        } else {
            self.set_ev(None);
        }
    }
    pub fn clear_ev(&mut self) {
        self.set_ev(None);
    }
    pub fn clear_all(&mut self) {
        self.replying = false;
        self.queue.clear();
        self.set_ev(None);
    }
    pub fn waiting(&self) -> usize {
        self.queue.iter().filter(|q| q.view == "notif").count()
    }
    pub fn pulse(&mut self) {
        self.bump_at = Some(Instant::now());
    }
    fn bump_scale(&self) -> f32 {
        // 1 -> 1.07 (140 ms OutQuad) -> 0.98 (160 ms InOutQuad) -> 1 (260 ms OutBack)
        let Some(t0) = self.bump_at else { return 1.0 };
        let t = t0.elapsed().as_secs_f32() * 1000.0;
        if t < 140.0 {
            let k = t / 140.0;
            1.0 + 0.07 * (1.0 - (1.0 - k) * (1.0 - k))
        } else if t < 300.0 {
            let k = (t - 140.0) / 160.0;
            let e = if k < 0.5 { 2.0 * k * k } else { 1.0 - (-2.0 * k + 2.0).powi(2) / 2.0 };
            1.07 - 0.09 * e
        } else if t < 560.0 {
            let k = (t - 300.0) / 260.0 - 1.0;
            let s = 1.70158;
            let e = 1.0 + (s + 1.0) * k.powi(3) + s * k.powi(2);
            0.98 + 0.02 * e
        } else {
            1.0
        }
    }

    // ------------------------------------------------------------ media
    pub fn player(&self) -> Option<&Player> {
        if let Some(i) = self.pick {
            if i < self.players.len() {
                return self.players.get(i);
            }
        }
        self.players.iter().find(|p| p.playing).or_else(|| self.players.iter().find(|p| !p.title.is_empty()))
    }
    pub fn playing(&self) -> bool {
        self.player().map(|p| p.playing).unwrap_or(false)
    }
    pub fn next_player(&mut self) {
        let n = self.players.len();
        if n > 1 {
            let cur = self.player().map(|p| p.bus.clone()).unwrap_or_default();
            let i = self.players.iter().position(|p| p.bus == cur).unwrap_or(0);
            self.pick = Some((i + 1) % n);
        }
    }
    pub fn mine(&mut self) {
        self.self_until = Some(Instant::now() + Duration::from_millis(1200));
    }
    pub fn set_players(&mut self, ps: Vec<Player>) {
        let old_key = self.player().map(|p| (p.title.clone() + "|" + &p.artist, p.playing));
        self.players = ps;
        let new = self.player().map(|p| (p.title.clone() + "|" + &p.artist, p.playing));
        if old_key.as_ref().map(|o| o.0.clone()) != new.as_ref().map(|n| n.0.clone()) {
            self.track_at = Some(Instant::now() + Duration::from_millis(450));
        }
        if old_key.map(|o| o.1) != new.map(|n| n.1) {
            self.state_at = Some(Instant::now() + Duration::from_millis(400));
        }
    }

    // ------------------------------------------------------------ the shown mode
    pub fn mode(&self, ringing: bool, call: bool) -> &'static str {
        if ringing {
            "call"
        } else if self.expanded && self.player().is_some() && !self.glance_demo {
            "player"
        } else if let Some(e) = &self.ev {
            e.view
        } else if call {
            "call"
        } else if self.expanded {
            "glance"
        } else {
            "idle"
        }
    }

    // ------------------------------------------------------------ timers (called every frame / tick)
    pub fn tick(&mut self, ringing: bool, call: bool) -> bool {
        let now = Instant::now();
        let mut changed = false;
        if !self.armed && self.start.elapsed().as_secs() >= 3 {
            self.armed = true;
        }
        if self.ev_until.map(|t| now >= t).unwrap_or(false) {
            let keep = (self.hover || self.replying) && self.ev.as_ref().map(|e| e.view == "notif" || e.hold).unwrap_or(false);
            if keep {
                self.ev_until = Some(now + Duration::from_millis(self.ev.as_ref().map(|e| e.dur).unwrap_or(3000)));
            } else {
                self.ev_until = None;
                self.next();
                changed = true;
            }
        }
        let mode = self.mode(ringing, call);
        if self.open_at.map(|t| now >= t).unwrap_or(false) {
            self.open_at = None;
            if self.hover && (mode == "idle" || mode == "track") {
                if self.player().is_some() {
                    self.set_ev(None);
                }
                self.expanded = true;
                changed = true;
            }
        }
        if self.close_at.map(|t| now >= t).unwrap_or(false) {
            self.close_at = None;
            self.expanded = false;
            changed = true;
        }
        if self.track_at.map(|t| now >= t).unwrap_or(false) {
            self.track_at = None;
            if let Some(p) = self.player().cloned() {
                let k = p.title.clone() + "|" + &p.artist;
                if !p.title.is_empty() && k != self.last_track {
                    self.last_track = k;
                    if self.armed && p.playing && !self.expanded {
                        self.push(Ev { view: "track", key: "track".into(), prio: 1, dur: 3500, value: -1, ..Default::default() });
                        changed = true;
                    }
                }
            }
        }
        if self.state_at.map(|t| now >= t).unwrap_or(false) {
            self.state_at = None;
            let p = self.playing();
            if p != self.last_playing {
                self.last_playing = p;
                let mine = self.self_until.map(|t| now < t).unwrap_or(false);
                if self.armed && !mine && !self.expanded && self.player().map(|p| !p.title.is_empty()).unwrap_or(false) {
                    self.push(Ev { view: "track", key: "track".into(), prio: 1, dur: 1800, state: if p { "play".into() } else { "pause".into() }, value: -1, ..Default::default() });
                    changed = true;
                }
            }
        }
        if self.share_at.map(|t| now >= t).unwrap_or(false) {
            self.share_at = None;
            if self.cast_on && !self.sharing {
                self.sharing = true;
                self.share_since = now;
                self.push(info("share", 2, 2600, "screenshot", PURPLE, "Udostępniasz ekran", ""));
                changed = true;
            }
        }
        // minutnik end
        if self.timer_end > 0.0 && data::now() as f64 >= self.timer_end {
            let l = self.timer_label.clone();
            self.timer_end = 0.0;
            let mut e = info("timer", 4, 15000, "alarm", ORANGE_I, if l.is_empty() { "Minutnik" } else { &l }, &format!("czas minął · {}", fmt_dur(self.timer_total)));
            e.pulse = true;
            e.hold = true;
            self.push(e);
            crate::app::spawn(&["pw-play", "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga"]);
            changed = true;
        }
        changed
    }

    // ------------------------------------------------------------ inputs from the data sources
    pub fn on_audio(&mut self, a: Audio) {
        let old = std::mem::replace(&mut self.audio, a.clone());
        if !self.audio_seen {
            self.audio_seen = true;
            return;
        }
        let dev_armed = self.dev_armed_at.map(|t| t.elapsed().as_millis() >= 1500).unwrap_or(true);
        if a.sink != old.sink {
            if self.armed && !a.sink.is_empty() {
                let hp = a.sink_desc.to_lowercase();
                let icon = if ["head", "słuch", "buds", "airpods"].iter().any(|k| hp.contains(k)) { "headphones" } else { "volume" };
                self.push(info("out", 2, 2600, icon, ACCENT, if a.sink_desc.is_empty() { &a.sink } else { &a.sink_desc }, "wyjście audio"));
            }
            self.dev_armed_at = Some(Instant::now());
            return;
        }
        if a.source != old.source {
            if self.armed && !a.source.is_empty() {
                self.push(info("in", 2, 2600, "mic", ACCENT, if a.src_desc.is_empty() { &a.source } else { &a.src_desc }, "wejście audio"));
            }
            self.dev_armed_at = Some(Instant::now());
            return;
        }
        if self.armed && dev_armed && ((a.vol - old.vol).abs() > 0.001 || a.mute != old.mute) {
            self.vol_recent_until = Some(Instant::now() + Duration::from_millis(1600));
            if !self.expanded || self.player().is_none() {
                self.push(Ev { view: "osd", key: "vol".into(), prio: 3, dur: 1600, mic: false, value: -1, ..Default::default() });
            }
        }
        if self.armed && dev_armed && a.src_mute != old.src_mute {
            let m = a.src_mute;
            self.push(info("mic", 3, 2200, if m { "micOff" } else { "mic" }, if m { DANGER } else { GREEN }, if m { "Mikrofon wyciszony" } else { "Mikrofon włączony" }, ""));
        }
        if self.armed && dev_armed && (a.src_vol - old.src_vol).abs() > 0.001 && a.mic_users.is_empty() {
            self.push(Ev { view: "osd", key: "vol".into(), prio: 3, dur: 1600, mic: true, value: -1, ..Default::default() });
        }
        if a.mic_users.is_empty() != old.mic_users.is_empty() && self.armed {
            let busy = !a.mic_users.is_empty();
            self.push(info("busy", 2, 2600, if busy { "mic" } else { "micOff" }, ORANGE_I, if busy { "Mikrofon w użyciu" } else { "Mikrofon zwolniony" }, &a.mic_users.join(", ")));
        }
    }
    pub fn mic_busy(&self) -> bool {
        !self.audio.mic_users.is_empty()
    }
    pub fn vol_recent(&self) -> bool {
        self.vol_recent_until.map(|t| Instant::now() < t).unwrap_or(false)
    }

    pub fn on_hypr(&mut self, n: &str, d: &str, apps: &Apps, clients: &[crate::sys::Client], active: &str) {
        match n {
            "activelayout" => {
                let l = d.splitn(2, ',').nth(1).unwrap_or("").to_string();
                if l == self.layout {
                    return;
                }
                let first = self.layout.is_empty();
                self.layout = l.clone();
                if !first && self.armed {
                    self.push(info("kbd", 2, 1800, "keyboard", ACCENT, &l, "układ klawiatury"));
                }
            }
            "screencast" => {
                // screenshots fire this too: sharing only after 1.5 s
                self.cast_on = d.starts_with('1');
                if self.cast_on {
                    self.share_at = Some(Instant::now() + Duration::from_millis(1500));
                } else if self.sharing {
                    self.sharing = false;
                    let s = fmt_dur(self.share_since.elapsed().as_secs_f64());
                    self.push(info("share", 2, 2600, "screenshot", PURPLE, "Koniec udostępniania", &s));
                } else {
                    self.share_at = None;
                    self.shot_at = Some(Instant::now());
                }
            }
            "submap" => self.submap = d.to_string(),
            "urgent" => {
                if !self.armed {
                    return;
                }
                let a = d.trim_start_matches("0x");
                let Some(c) = clients.iter().find(|c| c.address.trim_start_matches("0x") == a) else { return };
                if c.address.trim_start_matches("0x") == active.trim_start_matches("0x") {
                    return;
                }
                let e = apps.lookup(&c.class);
                let mut ev = info(&format!("urg{d}"), 2, 5000, "", ORANGE_I, &e.map(|e| e.name.clone()).unwrap_or(if c.class.is_empty() { "Okno".into() } else { c.class.clone() }), "prosi o uwagę · kliknij");
                ev.img = e.map(|e| e.icon.clone()).unwrap_or("application-x-executable".into());
                ev.act = Act::Focus(a.to_string());
                ev.hold = true;
                self.push(ev);
            }
            "activespecial" => {
                if !self.armed {
                    return;
                }
                let ws = d.split(',').next().unwrap_or("");
                // special:minimized is where minimizing parks windows, not a place the user goes to
                if !ws.is_empty() && ws != "special:minimized" {
                    let t = ws.trim_start_matches("special").trim_start_matches(':');
                    self.push(info("special", 1, 1500, "layers", PURPLE, if t.is_empty() { "Brudnopis" } else { t }, "obszar specjalny"));
                }
            }
            "configreloaded" => {
                if !self.armed {
                    return;
                }
                let out = std::process::Command::new("hyprctl").args(["configerrors", "-j"]).output().ok();
                let errs: Vec<String> = out.and_then(|o| serde_json::from_slice::<Vec<String>>(&o.stdout).ok()).unwrap_or_default().into_iter().filter(|x| !x.trim().is_empty()).collect();
                if !errs.is_empty() {
                    let mut e = info("cfg", 3, 7000, "alert", DANGER, &format!("Błędy konfiguracji Hyprland ({})", errs.len()), &errs[0].split_whitespace().collect::<Vec<_>>().join(" "));
                    e.hold = true;
                    self.push(e);
                } else {
                    self.push(info("cfg", 1, 1800, "refresh", GREEN, "Konfiguracja przeładowana", "Hyprland"));
                }
            }
            "monitoraddedv2" | "monitorremovedv2" => {
                if !self.armed {
                    return;
                }
                let p: Vec<&str> = d.split(',').collect();
                let add = n.starts_with("monitoradded");
                let title = if p.len() > 2 { p[2..].join(",") } else { p.get(1).or(p.first()).unwrap_or(&"").to_string() };
                self.push(info(&format!("mon{}", p.get(1).unwrap_or(&"")), 2, 3000, if add { "screenOn" } else { "screenOff" }, if add { ACCENT } else { T3 }, &title, if add { "monitor podłączony" } else { "monitor odłączony" }));
            }
            "pin" => {
                if !self.armed {
                    return;
                }
                let on = d.split(',').nth(1) == Some("1");
                self.push(info("pin", 1, 1500, "pinned", if on { ACCENT } else { T3 }, if on { "Okno przypięte" } else { "Okno odpięte" }, ""));
            }
            _ => {}
        }
    }

    pub fn on_bt(&mut self, on: bool, devs: Option<Vec<BtDev>>) {
        if let Some(was) = self.bt_on {
            if self.armed && was != on {
                self.push(info("btA", 2, 2000, if on { "bluetooth" } else { "btOff" }, if on { ACCENT } else { T3 }, if on { "Bluetooth włączony" } else { "Bluetooth wyłączony" }, ""));
            }
        }
        self.bt_on = Some(on);
        let Some(devs) = devs else { return };
        for d in &devs {
            let old = self.bt_devs.iter().find(|o| o.address == d.address);
            if let Some(o) = old {
                if o.connected != d.connected && self.armed {
                    let sub = if d.connected { d.battery.map(|b| format!("połączono · {}%", (b * 100.0).round())).unwrap_or("połączono".into()) } else { "rozłączono".into() };
                    self.push(info(&format!("bt{}", d.address), 2, 3000, if d.connected { "btOn" } else { "btOff" }, if d.connected { ACCENT } else { T3 }, &d.name, &sub));
                }
            }
            if let (Some(b), true) = (d.battery, d.connected) {
                if b > 0.3 {
                    self.bt_low.remove(&d.address);
                } else {
                    let lvl = if b <= 0.1 { 2 } else if b <= 0.2 { 1 } else { 0 };
                    if self.armed && lvl > 0 && *self.bt_low.get(&d.address).unwrap_or(&0) < lvl {
                        self.bt_low.insert(d.address.clone(), lvl);
                        self.push(info(&format!("btLow{}", d.address), 3, 5000, "battLow", if lvl == 2 { DANGER } else { ORANGE_I }, &d.name, &format!("słaba bateria · {}%", (b * 100.0).round())));
                    }
                }
            }
        }
        self.bt_devs = devs;
    }

    pub fn on_profile(&mut self, p: String) {
        if !self.profile.is_empty() && p != self.profile && self.armed {
            let (icon, col, t) = match p.as_str() { "performance" => ("rocket", ORANGE_I, "Wydajność"), "power-saver" => ("leaf", GREEN, "Oszczędzanie energii"), _ => ("gauge", ACCENT, "Zrównoważony") };
            self.push(info("profile", 2, 2000, icon, col, t, "profil zasilania"));
        }
        self.profile = p;
    }

    pub fn on_nm(&mut self, l: &str) {
        if let Some(c) = l.split("Connectivity is now '").nth(1).and_then(|r| r.split('\'').next()) {
            let prev = std::mem::replace(&mut self.conn, c.to_string());
            if !self.armed || prev == c {
                return;
            }
            if c == "full" {
                let p = self.primary.clone();
                self.push(info("net", 2, 2600, "worldOk", GREEN, "Połączono z internetem", &p));
            } else if prev == "full" {
                self.push(info("net", 2, 4000, "worldOff", DANGER, "Brak internetu", match c { "limited" => "ograniczone", "portal" => "wymaga logowania (portal)", _ => "" }));
            }
            return;
        }
        if let Some(rest) = l.strip_prefix('\'') {
            if let Some(name) = rest.split("' is now the primary connection").next().filter(|_| rest.contains("' is now the primary connection")) {
                let was = std::mem::replace(&mut self.primary, name.to_string());
                if self.armed && !was.is_empty() && was != name && self.conn == "full" {
                    self.push(info("net", 1, 2400, "network", ACCENT, name, "główne połączenie"));
                }
                return;
            }
        }
        let low = l.to_lowercase();
        if ["wg", "tun", "tap", "vpn", "proton", "nord", "mullvad"].iter().any(|p| low.starts_with(p)) {
            if let Some((dev, st)) = l.split_once(": ") {
                let st = st.trim();
                if ["connected", "disconnected", "unavailable", "unmanaged"].contains(&st) && self.armed {
                    let on = st == "connected";
                    self.push(info(&format!("vpn{dev}"), 2, 2600, "shield", if on { GREEN } else { T3 }, if on { "VPN połączony" } else { "VPN rozłączony" }, dev));
                }
            }
        }
    }

    pub fn on_sys(&mut self, o: &Value) {
        let t = o["t"].as_str().unwrap_or("");
        let s = |k: &str| o[k].as_str().unwrap_or("").to_string();
        if t == "lock" {
            let (caps, num) = (o["caps"].as_bool().unwrap_or(false), o["num"].as_bool().unwrap_or(true));
            let (c, n) = (caps != self.caps, num != self.num);
            self.caps = caps;
            self.num = num;
            if o["init"].as_bool().unwrap_or(false) || !self.armed {
                return;
            }
            if c {
                self.push(info("lock", 2, 1400, "capsLock", if caps { ACCENT } else { T3 }, if caps { "Caps Lock włączony" } else { "Caps Lock wyłączony" }, ""));
            } else if n {
                self.push(info("lock", 2, 1400, "numbers", if num { ACCENT } else { T3 }, if num { "Num Lock włączony" } else { "Num Lock wyłączony" }, ""));
            }
            return;
        }
        if t == "cam" {
            self.cam_busy = o["on"].as_bool().unwrap_or(false);
            self.cam_apps = o["apps"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            if self.armed {
                let on = self.cam_busy;
                let apps = self.cam_apps.join(", ");
                self.push(info("cam", 2, 2600, if on { "video" } else { "videoOff" }, if on { GREEN } else { T3 }, if on { "Kamera w użyciu" } else { "Kamera wyłączona" }, if on { &apps } else { "" }));
            }
            return;
        }
        if !self.armed {
            return;
        }
        match t {
            "clip" => {
                let kind = s("kind");
                let shot = kind == "image" && self.shot_at.map(|t| t.elapsed().as_millis() < 4000).unwrap_or(false);
                let private = self.sharing && matches!(kind.as_str(), "text" | "link" | "files");
                let n = o["n"].as_i64().unwrap_or(1);
                let (icon, title, sub) = match kind.as_str() {
                    "text" => ("copy", "Skopiowano".to_string(), format!("„{}”", s("text"))),
                    "link" => ("link", "Skopiowano link".into(), s("text")),
                    "color" => ("palette", "Skopiowano kolor".into(), s("text").to_uppercase()),
                    "files" => ("files", if n == 1 { "Skopiowano plik".into() } else { format!("Skopiowano pliki ({n})") }, s("text")),
                    "image" => ("photo", if shot { "Zrzut ekranu w schowku".into() } else { "Skopiowano obraz".into() }, format!("{} KB", ((o["size"].as_f64().unwrap_or(0.0) / 1024.0).round() as i64).max(1))),
                    "secret" => ("lock", "Skopiowano hasło".into(), "ukryte · schowek".into()),
                    _ => return,
                };
                let mut e = info("clip", 1, if kind == "image" { 2800 } else { 2000 }, icon, if kind == "secret" { ORANGE_I } else { ACCENT }, &title, if private { "treść ukryta · udostępniasz ekran" } else { &sub });
                if kind == "image" && !self.sharing {
                    e.img = s("path");
                    e.photo = true;
                }
                if kind == "color" {
                    e.swatch = Some(Rgba::hex(&s("text")));
                }
                self.push(e);
            }
            "usb" => {
                let icon = match s("kind").as_str() { "keyboard" => "keyboard", "mouse" => "mouse", "storage" => "usb", "audio" => "headset", "video" => "camera", "phone" => "mobile", "pad" => "gamepad", "printer" => "printer", "net" => "network", _ => "usb" };
                let on = o["on"].as_bool().unwrap_or(false);
                self.push(info(&format!("usb{}", s("name")), 1, 2400, icon, if on { ACCENT } else { T3 }, &s("name"), if on { "podłączono" } else { "odłączono" }));
            }
            "mount" => {
                if o["on"].as_bool().unwrap_or(false) {
                    let free = o["free"].as_f64().unwrap_or(-1.0);
                    let sub = format!("zamontowano{} · otwórz", if free >= 0.0 { format!(" · wolne {}", gb(free)) } else { String::new() });
                    let mut e = info(&format!("mnt{}", s("path")), 2, 4500, "drive", ACCENT, &s("name"), &sub);
                    e.act = Act::Open(s("path"));
                    self.push(e);
                } else {
                    self.push(info(&format!("mnt{}", s("path")), 2, 2600, "eject", GREEN, &s("name"), "odmontowano · można odłączyć"));
                }
            }
            _ => {}
        }
    }

    // a notification from the server (Island.notify)
    pub fn on_note(&mut self, n: &crate::notif::Note, apps: &Apps, phone_notes: &[Value]) {
        let title = strip_tags(&n.summary);
        let body = strip_tags(&n.body);
        if title.is_empty() && body.is_empty() {
            return;
        }
        let ent = apps.lookup(if n.entry.is_empty() { &n.app } else { &n.entry });
        let app = ent.map(|e| e.name.clone()).unwrap_or(if n.app.is_empty() { "Powiadomienie".into() } else { n.app.clone() });
        let kde = format!("{} {}", n.app, n.entry).to_lowercase();
        if self.call_at.map(|t| t.elapsed().as_secs() < 5).unwrap_or(false) && (kde.contains("kdeconnect") || kde.contains("kde connect")) {
            return;
        }
        let urgent = n.urgency == 2;
        let icon = if !n.icon.is_empty() { n.icon.clone() } else { ent.map(|e| e.icon.clone()).unwrap_or("dialog-information".into()) };
        if kde.contains("kdeconnect") {
            self.phone_note(&n.summary, &n.body, &icon, urgent, phone_notes);
            return;
        }
        let sig = format!("{app}|{title}|{body}");
        if self.last_note.0 == sig && self.last_note.1.map(|t| t.elapsed().as_millis() < 2000).unwrap_or(false) && n.value < 0 {
            return;
        }
        self.last_note = (sig, Some(Instant::now()));
        let mut key = if !n.sync.is_empty() { format!("s:{app}{}", n.sync) } else if n.value >= 0 { format!("v:{app}{title}") } else { String::new() };
        if key.is_empty() && n.replaced {
            if let Some(e) = &self.ev {
                if e.view == "notif" && e.app == app {
                    key = e.key.clone();
                }
            }
            if key.is_empty() {
                key = format!("r:{app}{}", n.id);
            }
        }
        let full_text = format!("{title} {body}");
        let raw_links = extract_links(&full_text);
        let raw_codes = extract_codes(&full_text, &raw_links);
        let (links, codes) = if self.sharing { (Vec::new(), Vec::new()) } else { (raw_links, raw_codes) };
        let has_smart = !links.is_empty() || !codes.is_empty();

        let e = Ev {
            view: "notif",
            key,
            prio: if urgent { 3 } else { 2 },
            dur: if urgent { 9000 } else if has_smart { 7500 } else if n.value >= 0 { 6000 } else { 4500 },
            app: app.clone(),
            icon,
            title: if self.sharing { app.clone() } else if title.is_empty() { body.clone() } else { title.clone() },
            body: if self.sharing { "treść ukryta · udostępniasz ekran".into() } else if title.is_empty() { String::new() } else { body },
            urgent,
            value: n.value,
            pulse: urgent,
            act: Act::Nid(n.id, vec![n.entry.clone(), n.app.clone(), ent.map(|e| e.id.clone()).unwrap_or_default(), ent.map(|e| e.wm_class.clone()).unwrap_or_default(), app], ent.map(|e| e.id.clone()).unwrap_or_default()),
            phone: None,
            links,
            codes,
            desktop_actions: n.actions.clone(),
            note_id: n.id,
            ..Default::default()
        };
        if e.pulse {
            self.pulse();
        }
        self.push(e);
    }
    // KDE Connect: summary = phone app, body = "sender: text"; kdeconnectd's own messages: one info line
    fn phone_note(&mut self, app_name: &str, raw_body: &str, img: &str, urgent: bool, _notes: &[Value]) {
        let dev = self.phone_name.clone();
        let clean_app = strip_tags(app_name);
        if raw_body.is_empty() || (!dev.is_empty() && clean_app == dev) {
            let body_clean = strip_tags(raw_body);
            let mut e = info("phoneMsg", if urgent { 3 } else { 2 }, 4000, "mobile", if urgent { DANGER } else { ACCENT }, if dev.is_empty() { &clean_app } else { &dev }, if body_clean.is_empty() { &clean_app } else { &body_clean });
            e.act = Act::Popup("phone".into());
            self.push(e);
            return;
        }

        let (title, sender, last_msg) = parse_phone_message(app_name, raw_body);
        let display_body = if !sender.is_empty() && sender != title {
            format!("{sender}: {last_msg}")
        } else {
            last_msg
        };
        if title.is_empty() && display_body.is_empty() {
            return;
        }

        let sig = format!("{clean_app}|{title}|{display_body}");
        if self.last_note.0 == sig && self.last_note.1.map(|t| t.elapsed().as_millis() < 2000).unwrap_or(false) {
            return;
        }
        self.last_note = (sig, Some(Instant::now()));

        let stripped_body = strip_tags(&unescape_html(raw_body));
        let full_text = format!("{title} {display_body} {stripped_body}");
        let raw_links = extract_links(&full_text);
        let raw_codes = extract_codes(&full_text, &raw_links);
        let (links, codes) = if self.sharing { (Vec::new(), Vec::new()) } else { (raw_links, raw_codes) };
        let has_smart = !links.is_empty() || !codes.is_empty();

        let app_display = if clean_app.is_empty() { "Telefon".to_string() } else { clean_app.clone() };
        let e = Ev {
            view: "notif",
            key: format!("ph:{clean_app}|{title}"),
            prio: if urgent { 3 } else { 2 },
            dur: if urgent { 9000 } else if has_smart { 8000 } else { 7000 },
            app: format!("{app_display}{}", if dev.is_empty() { String::new() } else { format!(" · {dev}") }),
            phone: Some((clean_app.clone(), raw_body.to_string())),
            icon: [clean_app.to_lowercase().replace(' ', "-"), img.to_string(), "smartphone".into()].into_iter().find(|s| !s.is_empty()).unwrap_or_default(),
            title: if self.sharing { clean_app.clone() } else { title },
            body: if self.sharing { "treść ukryta · udostępniasz ekran".into() } else { display_body },
            urgent,
            value: -1,
            pulse: urgent,
            act: Act::Popup("phone".into()),
            links,
            codes,
            desktop_actions: Vec::new(),
            note_id: 0,
            ..Default::default()
        };
        self.push(e);
    }

    // phone events (Island.phoneEvent)
    pub fn on_phone_event(&mut self, o: &Value, battery: i64) {
        if !self.armed {
            return;
        }
        let s = |k: &str| o[k].as_str().unwrap_or("").to_string();
        let n = if !s("name").is_empty() { s("name") } else if !self.phone_name.is_empty() { self.phone_name.clone() } else { "Telefon".into() };
        let bat = if battery >= 0 { format!(" · {battery}%") } else { String::new() };
        let k = s("k");
        if k == "call" {
            let who = [s("who"), s("number")].into_iter().find(|x| !x.is_empty()).unwrap_or("Nieznany numer".into());
            self.call_at = Some(Instant::now());
            if s("what") == "missedCall" {
                let mut e = info("phoneCall", 3, 9000, "call", DANGER, &who, "nieodebrane połączenie");
                e.hold = true;
                self.push(e);
            }
            return;
        }
        let ob = o["bat"].as_i64().unwrap_or(battery);
        let mut e = match k.as_str() {
            "conn" => { let mut e = info("", 2, 2600, "mobile", ACCENT, &n, &format!("połączono{bat}")); e.act = Act::Popup("start".into()); e }
            "disc" => info("", 1, 2200, "mobileOff", T3, &n, "rozłączono"),
            "pairreq" => { let mut e = info("", 3, 15000, "mobileShare", YELLOW, &format!("{n} chce się sparować"), &format!("klucz {} · kliknij, aby zaakceptować", s("key"))); e.hold = true; e.pulse = true; e.act = Act::Phone("accept".into()); e }
            "paired" => info("", 2, 3000, "check", GREEN, &n, "sparowano · telefon i komputer połączone"),
            "unpaired" => info("", 2, 2600, "unlink", T3, &n, "rozparowano"),
            "plug" => info("", 1, 2200, "charging", GREEN, &n, &format!("ładowanie · {ob}%")),
            "full" => info("", 1, 3000, "battery", GREEN, &n, "naładowany · 100%"),
            "photo" | "shot" => {
                let mut e = info("", 2, if k == "photo" { 4500 } else { 4000 }, if k == "photo" { "camera" } else { "screenshot" }, ACCENT, if k == "photo" { "Zdjęcie z telefonu" } else { "Zrzut ekranu telefonu" }, if k == "photo" { "kliknij, aby otworzyć" } else { "w schowku · kliknij, aby otworzyć" });
                e.img = s("path");
                e.photo = true;
                e.act = Act::Open(s("path"));
                e
            }
            "done" => info("", 1, 2400, if s("icon").is_empty() { "check" } else { o["icon"].as_str().unwrap_or("check") }, GREEN, &n, &s("text")),
            "err" => info("", 2, 3000, "alert", ORANGE_I, &n, if s("text").is_empty() { "błąd" } else { o["text"].as_str().unwrap_or("") }),
            _ => return,
        };
        e.key = format!("phone{k}");
        if e.pulse {
            self.pulse();
        }
        self.push(e);
    }
    // call view bookkeeping (Island Connections on Phone.callChanged)
    pub fn on_call(&mut self, c: Option<&Value>) {
        let p = self.last_call.clone();
        if let Some(p) = &p {
            if p["state"].as_str() == Some("active") && p["since"].as_i64().unwrap_or(0) > 0 && c.map(|c| c["path"] != p["path"]).unwrap_or(true) {
                let since = p["since"].as_i64().unwrap_or(0);
                self.push(info("callEnd", 2, 3200, "callEnd", T2, p["who"].as_str().unwrap_or(""), &format!("rozmowa zakończona · {}", fmt((data::now() - since) as f64))));
            }
        }
        match c {
            Some(c) => {
                let ring = matches!(c["state"].as_str(), Some("incoming") | Some("waiting"));
                let same = p.as_ref().map(|p| p["path"] == c["path"] && p["state"] == c["state"]).unwrap_or(false);
                if ring && !same {
                    self.pulse();
                }
                self.last_call = Some(c.clone());
            }
            None => self.keypad = false,
        }
    }

    // calendar reminders + rain within the hour (Data.onNowChanged / onWeatherChanged)
    pub fn soon(&self) -> Option<Value> {
        let t = data::now();
        self.calendar["items"].as_array().and_then(|a| a.iter().find(|x| !x["allDay"].as_bool().unwrap_or(false) && !x["holiday"].as_bool().unwrap_or(false) && x["start"].as_i64().unwrap_or(0) > t - 300 && x["start"].as_i64().unwrap_or(0) - t <= 900).cloned())
    }
    pub fn minute(&mut self) {
        let Some(s) = self.soon() else { return };
        let start = s["start"].as_i64().unwrap_or(0);
        let left = start - data::now();
        let id = format!("{}{start}", s["title"].as_str().unwrap_or(""));
        let title = s["title"].as_str().unwrap_or("").to_string();
        let wh = s["where"].as_str().map(|w| format!(" · {w}")).unwrap_or_default();
        if left <= 30 && left > -120 && self.reminded.get(&id) != Some(&2) {
            self.reminded.insert(id, 2);
            let mut e = info("cal", 3, 9000, "calEvent", ACCENT, &title, &format!("zaczyna się teraz{wh}"));
            e.pulse = true;
            e.hold = true;
            e.act = Act::Popup("calendar".into());
            self.pulse();
            self.push(e);
        } else if left <= 300 && left > 30 && !self.reminded.contains_key(&id) {
            self.reminded.insert(id, 1);
            let mut e = info("cal", 2, 7000, "calEvent", ACCENT, &title, &format!("{}{wh}", soon_text(start)));
            e.act = Act::Popup("calendar".into());
            self.push(e);
        }
    }
    pub fn on_weather(&mut self, w: Value) {
        self.weather = w;
        let w = &self.weather;
        if !self.armed || w["precip"].as_f64().unwrap_or(0.0) >= 0.1 || self.rain_at.map(|t| t.elapsed().as_secs() < 3 * 3600).unwrap_or(false) {
            return;
        }
        let s: Vec<f64> = w["soon"].as_array().map(|a| a.iter().take(4).map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
        let Some(i) = s.iter().position(|x| *x >= 0.2) else { return };
        self.rain_at = Some(Instant::now());
        let snow = w["snow"].as_f64().unwrap_or(0.0) > 0.0;
        let t = if i == 0 { if snow { "Zaczyna padać śnieg".to_string() } else { "Zaczyna padać".into() } } else { format!("{} za ok. {} min", if snow { "Śnieg" } else { "Deszcz" }, i * 15) };
        let city = w["city"].as_str().unwrap_or("").to_string();
        self.push(info("rain", 2, 6000, if snow { "snow" } else { "umbrella" }, ACCENT, &t, &city));
    }

    // minutnik
    pub fn timer_on(&self) -> bool {
        self.timer_end > 0.0
    }
    pub fn timer_left(&self) -> f64 {
        if self.timer_on() { (self.timer_end - now_f()).max(0.0) } else { 0.0 }
    }
    pub fn timer_start(&mut self, s: f64, label: &str) {
        if s <= 0.0 {
            return;
        }
        self.timer_total = s;
        self.timer_label = label.into();
        self.timer_end = now_f() + s;
        let t = if label.is_empty() { "Minutnik" } else { label };
        self.push(info("timer", 2, 2000, "hourglass", ORANGE_I, t, &format!("start · {}", fmt_dur(s))));
    }
    pub fn timer_stop(&mut self) {
        if !self.timer_on() {
            return;
        }
        self.timer_end = 0.0;
        let l = self.timer_label.clone();
        self.push(info("timer", 2, 1500, "stop", T3, "Minutnik zatrzymany", &l));
    }

    // hover over the shape
    pub fn set_hover(&mut self, h: bool, mode: &str) {
        if h == self.hover {
            return;
        }
        self.hover = h;
        if h {
            self.close_at = None;
            self.open_at = Some(Instant::now() + Duration::from_millis(220));
        } else {
            self.open_at = None;
            if self.expanded {
                self.close_at = Some(Instant::now() + Duration::from_millis(320));
            }
        }
        let _ = mode;
    }

    pub fn demo(&mut self, kind: &str) {
        let a = ACCENT;
        let e = match kind {
            "volume" => Ev { view: "osd", key: "vol".into(), prio: 3, dur: 2500, value: -1, ..Default::default() },
            "mic-volume" => Ev { view: "osd", key: "vol".into(), prio: 3, dur: 2500, mic: true, value: -1, ..Default::default() },
            "notif" => Ev { view: "notif", prio: 2, dur: 5000, app: "Zen Browser".into(), icon: "zen-browser".into(), title: crate::i18n::tr("Nowa wiadomość od Kasi").into_owned(), body: crate::i18n::tr("Hej! Widzimy się dziś o 18 na bieganiu? Weź bidon, bo ma być ciepło.").into_owned(), value: -1, ..Default::default() },
            "notif-long" => Ev { view: "notif", prio: 2, dur: 8000, app: "Thunderbird".into(), icon: "thunderbird".into(), title: "Plan treningowy na październik".into(), body: "Cześć! W załączniku plan na najbliższe cztery tygodnie: dwa akcenty w tygodniu (interwały i tempo), jedno długie wybieganie w niedzielę, reszta spokojnie w drugiej strefie. Daj znać, jak się czujesz po sobotnim starcie.".into(), value: -1, ..Default::default() },
            "progress" => Ev { view: "notif", key: "v:demo".into(), prio: 2, dur: 6000, app: "Pobieranie".into(), icon: "folder-download".into(), title: "archlinux-2026.10.01-x86_64.iso".into(), body: "1,1 GB z 1,4 GB · 12 s".into(), value: 78, ..Default::default() },
            "urgent" => Ev { view: "notif", prio: 3, dur: 9000, app: "System".into(), icon: "dialog-warning".into(), title: "Mało miejsca na dysku".into(), body: "Na partycji / zostało 2,1 GB.".into(), urgent: true, value: -1, pulse: true, ..Default::default() },
            "track" => Ev { view: "track", key: "track".into(), prio: 1, dur: 4000, value: -1, ..Default::default() },
            "pause" => Ev { view: "track", key: "track".into(), prio: 1, dur: 2500, state: "pause".into(), value: -1, ..Default::default() },
            "mic" => info("mic", 3, 2500, "micOff", DANGER, "Mikrofon wyciszony", ""),
            "busy" => info("busy", 2, 2500, "mic", ORANGE_I, "Mikrofon w użyciu", "Discord"),
            "cam" => info("cam", 2, 2500, "video", GREEN, "Kamera w użyciu", "Zen Browser"),
            "share" => info("share", 2, 2500, "screenshot", PURPLE, "Udostępniasz ekran", ""),
            "bt" => info("bt", 2, 3000, "btOn", a, "Słuchawki", "połączono · 80%"),
            "bt-low" => info("btLow", 3, 4000, "battLow", DANGER, "Słuchawki", "słaba bateria · 9%"),
            "bt-off" => info("btA", 2, 2000, "btOff", T3, "Bluetooth wyłączony", ""),
            "net" => info("net", 2, 3000, "worldOff", DANGER, "Brak internetu", ""),
            "vpn" => info("vpn", 2, 2600, "shield", GREEN, "VPN połączony", "wg0"),
            "kbd" => info("kbd", 2, 2000, "keyboard", a, "Polish", "układ klawiatury"),
            "caps" => info("lock", 2, 1600, "capsLock", a, "Caps Lock włączony", ""),
            "num" => info("lock", 2, 1600, "numbers", T3, "Num Lock wyłączony", ""),
            "cal" => { let mut e = info("cal", 2, 5000, "calEvent", a, "Trening interwałowy", "za 5 min · Stadion"); e.act = Act::Popup("calendar".into()); e }
            "cal-now" => { let mut e = info("cal", 3, 6000, "calEvent", a, "Trening interwałowy", "zaczyna się teraz · Stadion"); e.pulse = true; e.act = Act::Popup("calendar".into()); e }
            "rain" => info("rain", 2, 5000, "umbrella", a, "Deszcz za ok. 30 min", "Kraków"),
            "clip" => info("clip", 2, 2500, "copy", a, "Skopiowano", "„Rozgrzewka 15 min, 5×800 m, przerwy 400 m truchtem”"),
            "link" => info("clip", 2, 2500, "link", a, "Skopiowano link", "https://github.com/quickshell-mirror/quickshell"),
            "color" => { let mut e = info("clip", 2, 2500, "palette", a, "Skopiowano kolor", "#A9C2EE"); e.swatch = Some(Rgba::hex("#a9c2ee")); e }
            "files" => info("clip", 2, 2500, "files", a, "Skopiowano pliki (3)", "plan.pdf, trasa.gpx, zdjęcie.jpg"),
            "image" => { let mut e = info("clip", 2, 3000, "photo", a, "Zrzut ekranu w schowku", "412 KB"); e.img = "/usr/share/pixmaps/archlinux-logo.png".into(); e.photo = true; e }
            "secret" => info("clip", 2, 2000, "lock", ORANGE_I, "Skopiowano hasło", "ukryte · schowek"),
            "usb" => info("usb", 1, 2400, "usb", a, "SanDisk Ultra Fit", "podłączono"),
            "mount" => info("mnt", 2, 4000, "drive", a, "PENDRIVE", "zamontowano · wolne 28,4 GB · otwórz"),
            "unmount" => info("mnt", 2, 2600, "eject", GREEN, "PENDRIVE", "odmontowano · można odłączyć"),
            "monitor" => info("mon", 2, 3000, "screenOn", a, "iiyama PL2770H", "monitor podłączony"),
            "urgent-window" => { let mut e = info("urg", 2, 4000, "", ORANGE_I, "kitty", "prosi o uwagę · kliknij"); e.img = "kitty".into(); e }
            "special" => info("special", 1, 1500, "layers", PURPLE, "magic", "obszar specjalny"),
            "cfg" => info("cfg", 1, 1800, "refresh", GREEN, "Konfiguracja przeładowana", "Hyprland"),
            "cfg-err" => info("cfg", 3, 5000, "alert", DANGER, "Błędy konfiguracji Hyprland (2)", "binds.lua:42: unknown dispatcher 'focuswindw'"),
            "profile" => info("profile", 2, 2000, "rocket", ORANGE_I, "Wydajność", "profil zasilania"),
            "player" => { self.set_ev(None); self.expanded = !self.expanded; return; }
            "glance" => { self.set_ev(None); self.expanded = !self.expanded; self.glance_demo = self.expanded; return; }
            "timer" => { self.timer_start(65.0, "Herbata"); return; }
            k if k.starts_with("phone") => {
                let kk = match k { "phone" => "conn", "phone-off" => "disc", "phone-pair" => "pairreq", "phone-charge" => "plug", _ => "call" };
                let v = serde_json::json!({ "k": kk, "name": "POCO X5 Pro 5G", "key": "A3E7A214", "bat": 64, "what": "ringing", "who": "Kasia" });
                self.on_phone_event(&v, 64);
                return;
            }
            _ => return,
        };
        if e.pulse {
            self.pulse();
        }
        self.push(e);
    }
}

fn now_f() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
pub fn fmt(s: f64) -> String {
    let s = s.max(0.0).floor() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}
pub fn fmt_dur(s: f64) -> String {
    let s = s.round() as i64;
    if s < 60 { format!("{s} s") } else if s < 3600 { format!("{} min {} s", s / 60, s % 60) } else { format!("{} h {} min", s / 3600, s % 3600 / 60) }
}
fn gb(b: f64) -> String {
    if b >= 1e9 { format!("{:.1} GB", b / 1e9).replace('.', ",") } else { format!("{} MB", (b / 1e6).round()) }
}
fn soon_text(start: i64) -> String {
    let left = ((start - data::now()) as f64 / 60.0).ceil() as i64;
    if left > 0 { format!("za {left} min") } else { "teraz".into() }
}

// ======================================================================================== drawing
pub struct Ctx<'a> {
    pub t: &'a mut Text,
    pub icons: &'a mut Icons,
    pub imgs: &'a mut Images,
    pub phone: &'a crate::phone::State,
    pub now: i64,
}

impl Island {
    fn pane(&mut self, name: &'static str, active: bool) -> (f32, f32) {
        let e = self.panes.entry(name).or_insert((Anim::new(if active { 1.0 } else { 0.0 }), Spring::new(if active { 1.0 } else { 0.92 }, 3.4, 0.34, 0.005), None));
        // IslandPane: content comes in a beat after the shape starts (110 ms pause, 220 ms), leaves at once (80 ms)
        if active && e.0.target() < 0.5 && e.2.is_none() {
            e.2 = Some(Instant::now() + Duration::from_millis(110));
        }
        if !active {
            e.2 = None;
            e.0.set(0.0, 80.0, Ease::OutCubic);
        }
        if let Some(t) = e.2 {
            if Instant::now() >= t {
                e.0.set(1.0, 220.0, Ease::OutCubic);
                e.2 = None;
            }
        }
        e.1.set(if active { 1.0 } else { 0.92 });
        e.1.step();
        (e.0.value(), e.1.cur)
    }
    pub fn eq_running(&self) -> bool {
        self.eq.iter().any(|a| a.running())
    }
    pub fn animating(&self) -> bool {
        self.animating_but_eq() || self.eq_running()
    }
    fn animating_but_eq(&self) -> bool {
        self.w.moving || self.h.moving || self.y.moving || self.bump_at.map(|t| t.elapsed().as_millis() < 600).unwrap_or(false) || self.shadow.running()
            || self.panes.values().any(|p| p.0.running() || p.1.moving || p.2.is_some())
            || self.note_bar.running() || self.track_h.running()
    }
    // only the equalizer changes: a frame of the bars alone will do (eq_frame)
    pub fn eq_only(&self) -> bool {
        self.eq_spot.is_some() && !self.animating_but_eq()
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
    // anything tick() still has to time (or a clock it shows): the slow tick stays at 250 ms
    pub fn pending(&self) -> bool {
        let at = [self.ev_until, self.open_at, self.close_at, self.self_until, self.track_at, self.state_at, self.dev_armed_at, self.vol_recent_until, self.share_at, self.shot_at, self.call_at, self.rain_at, self.bump_at, self.last_note.1];
        !self.armed || self.ev.is_some() || !self.queue.is_empty() || at.iter().any(|t| t.is_some()) || self.timer_on() || self.playing() || self.hover || self.expanded || self.replying
    }
    // the equalizer alone over the last full frame: the box goes back to the black shape, the bars are drawn
    // into a small layer at the view's pixel phase and composited like the view (same transform, opacity).
    // The box (window pixels) to commit, or None when a full frame is needed.
    pub fn eq_frame(&mut self, c: &mut Canvas) -> Option<(i32, i32, i32, i32)> {
        let s = self.eq_spot?;
        let mut layer = Canvas::new(EQ_BOX.0 as u32, EQ_BOX.1 as u32);
        self.eq_draw(&mut layer, s.lx - s.bx as f32, s.ly - s.by as f32, s.run);
        self.eq_spot = Some(s);
        let (dx, dy, dw, dh) = s.dst;
        let rect = tiny_skia::Rect::from_xywh(dx as f32, dy as f32, dw as f32, dh as f32)?;
        let mut black = tiny_skia::Paint::default();
        black.set_color_rgba8(0, 0, 0, 255);
        c.pm.fill_rect(rect, &black, tiny_skia::Transform::identity(), None);
        let tf = s.tf.pre_translate(s.bx as f32, s.by as f32);
        let shader = tiny_skia::Pattern::new(layer.pm.as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, s.op, tf);
        let paint = tiny_skia::Paint { shader, anti_alias: true, ..Default::default() };
        c.pm.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
        Some(s.dst)
    }

    // size of each view (implicitWidth / implicitHeight in Island.qml)
    fn size(&mut self, view: &str, cx: &mut Ctx) -> (f32, f32) {
        let t = &mut *cx.t;
        match view {
            "idle" => (self.idle_w(cx) + 28.0, 30.0),
            "glance" => {
                let evs = data::upcoming(&self.calendar, 2, 2);
                let mut h = 0.0;
                if self.timer_on() { h += 18.0; }
                h += evs.len() as f32 * 33.0;
                if evs.is_empty() && !self.timer_on() { h += 17.0; }
                let n = (self.timer_on() as usize + evs.len().max(if self.timer_on() { 0 } else { 1 })).max(1);
                h += 4.0 * (n.saturating_sub(1)) as f32;
                (420.0, (h + 28.0).max(76.0))
            }
            "player" => {
                let len = self.player().map(|p| p.length).unwrap_or(0.0);
                (452.0, if len > 0.0 || self.vol_recent() { 144.0 } else { 110.0 })
            }
            "osd" => (300.0, 40.0),
            "call" => {
                let c = cx.phone.call().or(self.last_call.clone()).unwrap_or(Value::Null);
                let st = c["state"].as_str().unwrap_or("");
                let ring = st == "incoming" || st == "waiting";
                let live = st == "active";
                let open = ring || self.hover || self.keypad || (!live && st != "held");
                if !open {
                    let who = elide_w(t, c["who"].as_str().unwrap_or(""), &Style::ui(13.0, 600.0), 180.0);
                    let status = self.call_status(&c, cx.phone);
                    let mut w = 14.0 + 8.0 + who + 8.0 + t.width(&status, &Style::ui(13.0, 650.0).tnum());
                    if cx.phone.get("callAudio")["mute"].as_bool().unwrap_or(false) { w += 8.0 + 13.0; }
                    (w + 30.0, 34.0)
                } else {
                    (452.0, if ring { 80.0 } else if self.keypad { 196.0 } else { 116.0 })
                }
            }
            "notif" => {
                let e = self.last.get("notif").cloned().unwrap_or_default();
                let open = self.hover || self.replying;
                let cw = 452.0 - (14.0 + 38.0 + 12.0) - 18.0;
                let tl = wrap(t, &e.title, &Style::ui(13.5, 600.0), cw, if open { 2 } else { 1 }).len() as f32;
                let bl = if e.body.is_empty() { 0.0 } else { wrap(t, &e.body, &Style::ui(12.0, 450.0), cw, if open { 6 } else { 1 }).len() as f32 };
                let mut h = 14.0 + 1.0 + tl * 17.0 + 1.0 + bl * 15.5;
                if e.value >= 0 { h += 13.0; }
                let pn = phone_note(e.phone.as_ref(), cx.phone);
                let has_phone = open && pn.as_ref().map(|n| n["reply"].as_bool().unwrap_or(false) || n["dismiss"].as_bool().unwrap_or(false) || n["actions"].as_array().map(|a| !a.is_empty()).unwrap_or(false)).unwrap_or(false);
                let has_smart = !e.codes.is_empty() || !e.links.is_empty();
                let has_desktop = open && !e.desktop_actions.is_empty();
                if self.replying || has_phone || has_smart || has_desktop {
                    h += 9.0 + if self.replying { 32.0 } else { 26.0 };
                }
                (452.0, (h + 24.0).max(56.0))
            }
            "track" => {
                let p = self.player();
                let tw = p.map(|p| t.width(&p.title, &Style::ui(13.0, 600.0)).max(t.width(if p.artist.is_empty() { &p.identity } else { &p.artist }, &Style::ui(11.5, 450.0)))).unwrap_or(0.0);
                ((tw + 112.0).clamp(300.0, 420.0), 50.0)
            }
            "info" => {
                let e = self.last.get("info").cloned().unwrap_or_default();
                let photo = e.photo && !e.img.is_empty();
                let stack = photo || e.title.chars().count() + e.sub.chars().count() > 38;
                let rw = if stack {
                    let a = t.width(&e.title, &Style::ui(13.0, 600.0)).min(380.0);
                    let b = t.width(&e.sub, &Style::ui(11.5, 450.0)).min(380.0);
                    (if photo { 52.0 } else { 28.0 }) + 11.0 + a.max(b)
                } else {
                    let a = t.width(&e.title, &Style::ui(13.0, 600.0)).min(260.0);
                    let b = if e.sub.is_empty() { 0.0 } else { 10.0 + t.width(&e.sub, &Style::ui(12.0, 450.0)).min(220.0) };
                    28.0 + 10.0 + a + b
                };
                ((rw + if photo { 30.0 } else { 24.0 }).clamp(230.0, if stack { 470.0 } else { 440.0 }), if photo { 58.0 } else if stack { 50.0 } else { 40.0 })
            }
            _ => (100.0, 30.0),
        }
    }

    fn idle_w(&self, cx: &mut Ctx) -> f32 {
        let t = &mut *cx.t;
        let soon = self.soon();
        let mut items: Vec<f32> = Vec::new();
        let art = self.player().map(|p| !p.art.is_empty()).unwrap_or(false);
        if art && soon.is_none() { items.push(18.0); }
        if soon.is_some() { items.push(Canvas::icon_w(t, "calEvent", 15.0)); }
        items.push(t.width(&self.idle_text(&soon), &Style::ui(13.0, 600.0)).min(240.0));
        items.push(t.width("·", &Style::ui(13.0, 450.0)));
        items.push(t.width(&data::hm(cx.now), &Style::ui(13.0, 600.0)));
        if self.timer_on() { items.push(15.0 + 5.0 + t.width(&fmt(self.timer_left().ceil()), &Style::ui(12.5, 650.0).tnum())); }
        if !self.submap.is_empty() { items.push(12.0 + Canvas::icon_w(t, "command", 11.0) + 3.0 + t.width(&self.submap, &Style::ui(11.0, 650.0))); }
        if self.caps { items.push(Canvas::icon_w(t, "capsLock", 14.0)); }
        if self.cam_busy { items.push(7.0); }
        if self.mic_busy() { items.push(7.0); }
        if self.sharing { items.push(7.0); }
        if self.playing() { items.push(18.0); }
        items.iter().sum::<f32>() + 8.0 * (items.len().saturating_sub(1)) as f32
    }
    fn idle_text(&self, soon: &Option<Value>) -> String {
        if let Some(s) = soon {
            return format!("{} · {}", s["title"].as_str().unwrap_or(""), soon_text(s["start"].as_i64().unwrap_or(0)));
        }
        let tm = data::local(data::now());
        format!("{} {} {}", data::DAYS_SHORT[tm.tm_wday as usize], tm.tm_mday, &data::MONTHS_GEN[tm.tm_mon as usize].chars().take(3).collect::<String>())
    }
    fn call_status(&self, c: &Value, ph: &crate::phone::State) -> String {
        match c["state"].as_str().unwrap_or("") {
            "incoming" | "waiting" => format!("{} · {}", if c["state"].as_str() == Some("waiting") { "druga rozmowa czeka" } else { "połączenie przychodzące" }, if ph.name().is_empty() { "telefon".into() } else { ph.name() }),
            "active" => fmt((data::now() - c["since"].as_i64().unwrap_or(data::now())) as f64),
            "held" => "wstrzymane".into(),
            "alerting" => "dzwonię…".into(),
            _ => "łączenie…".into(),
        }
    }

    // the whole window: the shape (springing to the size of the shown view) and the views fading in / out
    pub fn draw(&mut self, c: &mut Canvas, cx: &mut Ctx, ringing: bool, call: bool) {
        c.clear();
        self.hits.clear();
        let mode = self.mode(ringing, call);
        let (tw, th) = self.size(mode, cx);
        self.w.set(tw);
        self.h.set(th);
        let hidden = self.fullscreen && mode == "idle";
        self.hidden = hidden;
        self.eq_spot = None;
        self.y.set(if hidden { -self.h.to - 8.0 } else { 5.0 });
        self.w.step();
        self.h.step();
        self.y.step();
        self.shadow.set(if mode == "idle" { 0.0 } else { 1.0 }, 220.0, Ease::Linear);
        let (w, h) = (self.w.cur.max(1.0), self.h.cur.max(1.0));
        let x = (WIN_W as f32 - w) / 2.0;
        let y = self.y.cur;
        let r = (h / 2.0).min(30.0);
        let k = self.bump_scale();
        // shadow + black shape (scaled about the top centre while bumping)
        let (sx, sw, sh) = (WIN_W as f32 / 2.0 - w * k / 2.0, w * k, h * k);
        let so = self.shadow.value();
        if so > 0.01 {
            crate::glass::shadow(c, sx, y + 8.0, sw, sh, r * k, 24.0, -4.0, Rgba(0.0, 0.0, 0.0, 0.45 * so));
        }
        c.rrect(sx, y, sw, sh, r * k, Rgba(0.0, 0.0, 0.0, 1.0));
        // views (each drawn into its own layer, then faded / scaled into the shape, clipped to it)
        for view in ["idle", "glance", "player", "osd", "call", "notif", "track", "info"] {
            let active = view == mode;
            let (op, sc) = self.pane(view, active);
            if op <= 0.01 {
                continue;
            }
            let (vw, vh) = if active { (tw, th) } else { self.size(view, cx) };
            let mut layer = Canvas::new(vw.ceil().max(1.0) as u32, vh.ceil().max(1.0) as u32);
            let hits0 = self.hits.len();
            self.eq_pos = None;
            self.draw_view(view, &mut layer, cx, vw, vh, active);
            // pane placed centred in the shape
            let ox = WIN_W as f32 / 2.0 - vw / 2.0;
            let oy = y + h / 2.0 - vh / 2.0;
            for hb in &mut self.hits[hits0..] {
                hb.0 += ox;
                hb.1 += oy;
            }
            if !active {
                self.hits.truncate(hits0);
            }
            let s = sc * k;
            let tf = tiny_skia::Transform::from_translate(-vw / 2.0, -vh / 2.0).post_scale(s, s).post_translate(WIN_W as f32 / 2.0, oy + vh / 2.0);
            // the active view at rest with an equalizer: remember where, for frames of the bars alone
            if let (true, Some((lx, ly, run))) = (active && s == 1.0 && !hidden, self.eq_pos) {
                let (bx, by) = (lx.floor() as i32 - 3, ly.floor() as i32 - 3);
                let dst = ((tf.tx + bx as f32).floor() as i32, (tf.ty + by as f32).floor() as i32, EQ_BOX.0 + 1, EQ_BOX.1 + 1);
                // the box must sit on the opaque black shape, inside the window
                let rr = r * k;
                let inside = |px: f32, py: f32| {
                    let (qx, qy) = ((px - (sx + sw / 2.0)).abs() - (sw / 2.0 - rr), (py - (y + sh / 2.0)).abs() - (sh / 2.0 - rr));
                    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rr < -1.5
                };
                let (x0, y0, x1, y1) = (dst.0 as f32, dst.1 as f32, (dst.0 + dst.2) as f32, (dst.1 + dst.3) as f32);
                if dst.0 >= 0 && dst.1 >= 0 && dst.0 + dst.2 <= WIN_W as i32 && dst.1 + dst.3 <= WIN_H as i32 && inside(x0, y0) && inside(x1, y0) && inside(x0, y1) && inside(x1, y1) {
                    self.eq_spot = Some(EqSpot { tf, op, run, lx, ly, bx, by, dst });
                }
            }
            let shader = tiny_skia::Pattern::new(layer.pm.as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, op, tf);
            let paint = tiny_skia::Paint { shader, anti_alias: true, ..Default::default() };
            if let Some(clip) = tiny_skia::Rect::from_xywh(sx, y, sw, sh) {
                let inter = tiny_skia::Rect::from_ltrb(clip.left().max(ox + vw / 2.0 - vw * s / 2.0), clip.top().max(oy + vh / 2.0 - vh * s / 2.0), clip.right().min(ox + vw / 2.0 + vw * s / 2.0), clip.bottom().min(oy + vh / 2.0 + vh * s / 2.0));
                if let Some(rc) = inter {
                    c.pm.fill_rect(rc, &paint, tiny_skia::Transform::identity(), None);
                }
            }
        }
        let _ = x;
    }

    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, hit: Hit) {
        self.hits.push((x, y, w, h, hit));
    }
    fn hov(&self, h: &Hit) -> bool {
        self.hover_hit.as_ref() == Some(h)
    }

    fn draw_view(&mut self, view: &str, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32, active: bool) {
        match view {
            "idle" => self.v_idle(c, cx, w, h, active),
            "glance" => self.v_glance(c, cx, w, h),
            "player" => self.v_player(c, cx, w, h),
            "osd" => {
                let mic = self.last.get("osd").map(|e| e.mic).unwrap_or(false);
                self.volume_row(c, cx, 16.0, 0.0, w - 32.0, h, mic);
            }
            "call" => self.v_call(c, cx, w, h),
            "notif" => self.v_notif(c, cx, w, h),
            "track" => self.v_track(c, cx, w, h, active),
            "info" => self.v_info(c, cx, w, h),
            _ => {}
        }
    }

    fn v_idle(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32, active: bool) {
        let iw = self.idle_w(cx);
        let mut x = (w - iw) / 2.0;
        let cy = h / 2.0;
        let soon = self.soon();
        let art = self.player().map(|p| p.art.clone()).unwrap_or_default();
        if !art.is_empty() && soon.is_none() {
            c.rrect(x, cy - 9.0, 18.0, 18.0, 5.0, FILL);
            if let Some(pm) = cx.imgs.get(&art, 48, cx.icons) {
                img::draw(c, &pm, x, cy - 9.0, 18.0, 18.0, 5.0, true, 1.0);
            }
            x += 18.0 + 8.0;
        }
        if soon.is_some() {
            let iw = Canvas::icon_w(cx.t, "calEvent", 15.0);
            c.icon(cx.t, x + iw / 2.0, cy, "calEvent", 15.0, ACCENT);
            x += iw + 8.0;
        }
        let st = Style::ui(13.0, 600.0);
        let txt = crate::bar::elide(cx.t, &self.idle_text(&soon), &st, 240.0);
        x += c.text(cx.t, x, cy, &txt, &st, if soon.is_some() { ACCENT } else { T1 }) + 8.0;
        x += c.text(cx.t, x, cy, "·", &Style::ui(13.0, 450.0), T3) + 8.0;
        x += c.text(cx.t, x, cy, &data::hm(cx.now), &st, T1) + 8.0;
        if self.timer_on() {
            let left = self.timer_left();
            let col = if left < 10.0 { DANGER } else { ORANGE_I };
            ring(c, x + 7.5, cy, if self.timer_total > 0.0 { (left / self.timer_total) as f32 } else { 0.0 }, col);
            x += 15.0 + 5.0;
            x += c.text(cx.t, x, cy, &fmt(left.ceil()), &Style::ui(12.5, 650.0).tnum(), col) + 8.0;
        }
        if !self.submap.is_empty() {
            let tw = cx.t.width(&self.submap, &Style::ui(11.0, 650.0));
            let iw = Canvas::icon_w(cx.t, "command", 11.0);
            let pw = iw + 3.0 + tw + 12.0;
            c.rrect(x, cy - 9.0, pw, 18.0, 9.0, PURPLE.alpha(0.2));
            c.icon(cx.t, x + 6.0 + iw / 2.0, cy, "command", 11.0, PURPLE);
            c.text(cx.t, x + 6.0 + iw + 3.0, cy, &self.submap.clone(), &Style::ui(11.0, 650.0), PURPLE);
            x += pw + 8.0;
        }
        if self.caps {
            let iw = Canvas::icon_w(cx.t, "capsLock", 14.0);
            c.icon(cx.t, x + iw / 2.0, cy, "capsLock", 14.0, ACCENT);
            x += iw + 8.0;
        }
        for (on, col) in [(self.cam_busy, GREEN), (self.mic_busy(), ORANGE_I), (self.sharing, PURPLE)] {
            if on {
                c.rrect(x, cy - 3.5, 7.0, 7.0, 3.5, col);
                x += 7.0 + 8.0;
            }
        }
        if self.playing() {
            self.eq_draw(c, x, cy - 7.0, active);
        }
    }
    // 4-bar equalizer, new heights every 130 ms (120 ms Behavior)
    fn eq_draw(&mut self, c: &mut Canvas, x: f32, y: f32, run: bool) {
        self.eq_pos = Some((x, y, run));
        // nothing moves while the island hides under a fullscreen window
        if run && !self.hidden && self.eq_at.elapsed().as_millis() >= 130 {
            self.eq_at = Instant::now();
            let seed = self.eq_at.elapsed().as_nanos() as u64 ^ (data::now() as u64).wrapping_mul(2654435761);
            for (i, a) in self.eq.iter_mut().enumerate() {
                let r = ((seed.wrapping_add(i as u64 * 7919).wrapping_mul(6364136223846793005) >> 33) % 1000) as f32 / 1000.0;
                a.set(3.0 + r * 11.0, 120.0, Ease::Linear);
            }
        }
        for (i, a) in self.eq.iter().enumerate() {
            let bh = a.value();
            c.rrect(x + i as f32 * 5.0, y + 14.0 - bh, 3.0, bh, 1.5, ACCENT);
        }
    }

    fn v_glance(&mut self, c: &mut Canvas, cx: &mut Ctx, _w: f32, h: f32) {
        let wx = self.weather.clone();
        let cy = h / 2.0;
        let icon = data::wx_icon(wx["code"].as_i64().unwrap_or(3), wx["day"].as_i64().unwrap_or(1));
        let iw = Canvas::icon_w(cx.t, icon, 30.0);
        c.icon(cx.t, 18.0 + iw / 2.0, cy, icon, 30.0, T1);
        let tx = 18.0 + iw + 10.0;
        let temp = wx["temp"].as_f64().map(|t| format!("{}°", t.round())).unwrap_or("–".into());
        let d0 = wx["days"].get(0).cloned().unwrap_or(Value::Null);
        let lines = 1 + (!wx["desc"].as_str().unwrap_or("").is_empty()) as usize + (!d0.is_null()) as usize;
        let mut ty = cy - (28.0 + (lines - 1) as f32 * 14.0) / 2.0;
        c.text(cx.t, tx, ty + 14.0, &temp, &Style::ui(22.0, 600.0), T1);
        ty += 28.0;
        if let Some(d) = wx["desc"].as_str().filter(|d| !d.is_empty()) {
            let d = crate::bar::elide(cx.t, d, &Style::ui(11.0, 450.0), 96.0);
            c.text(cx.t, tx, ty + 7.0, &d, &Style::ui(11.0, 450.0), T3);
            ty += 14.0;
        }
        if !d0.is_null() {
            let pop = d0["pop"].as_i64().unwrap_or(0);
            let s = format!("{}° / {}°{}", d0["max"], d0["min"], if pop >= 30 { format!(" · {pop}%") } else { String::new() });
            c.text(cx.t, tx, ty + 7.0, &s, &Style::ui(11.0, 450.0), T3);
        }
        c.rrect(150.0, 14.0, 1.0, h - 28.0, 0.0, HAIR);
        let gx = 164.0;
        let gw = 420.0 - gx - 18.0;
        let evs = data::upcoming(&self.calendar, 2, 2);
        let n_rows = self.timer_on() as usize + evs.len() + (evs.is_empty() && !self.timer_on()) as usize;
        let total = self.timer_on() as u8 as f32 * 18.0 + evs.len() as f32 * 33.0 + if evs.is_empty() && !self.timer_on() { 17.0 } else { 0.0 } + 4.0 * n_rows.saturating_sub(1) as f32;
        let mut y = cy - total / 2.0;
        if self.timer_on() {
            let left = self.timer_left();
            ring(c, gx + 7.5, y + 9.0, if self.timer_total > 0.0 { (left / self.timer_total) as f32 } else { 0.0 }, ORANGE_I);
            let l = if self.timer_label.is_empty() { "Minutnik".to_string() } else { self.timer_label.clone() };
            c.text(cx.t, gx + 21.0, y + 9.0, &format!("{l} · {}", fmt(left.ceil())), &Style::ui(12.5, 600.0), ORANGE_I);
            y += 22.0;
        }
        for e in &evs {
            let t1 = crate::bar::elide(cx.t, e["title"].as_str().unwrap_or(""), &Style::ui(12.5, 600.0), gw);
            c.text(cx.t, gx, y + 8.0, &t1, &Style::ui(12.5, 600.0), T1);
            let wh = e["where"].as_str().map(|w| format!(" · {w}")).unwrap_or_default();
            let t2 = crate::bar::elide(cx.t, &format!("{} · {}{wh}", data::rel(e["start"].as_i64().unwrap_or(0)), data::when(e)), &Style::ui(11.0, 450.0), gw);
            c.text(cx.t, gx, y + 24.0, &t2, &Style::ui(11.0, 450.0), T3);
            y += 37.0;
        }
        if evs.is_empty() && !self.timer_on() {
            c.text(cx.t, gx, y + 8.0, "Brak wydarzeń do jutra", &Style::ui(12.5, 450.0), T3);
        }
    }

    fn v_player(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, _h: f32) {
        let Some(p) = self.player().cloned() else { return };
        c.rrect(16.0, 16.0, 78.0, 78.0, 16.0, FILL);
        if let Some(pm) = cx.imgs.get(&p.art, 180, cx.icons) {
            img::draw(c, &pm, 16.0, 16.0, 78.0, 78.0, 16.0, true, 1.0);
        } else {
            c.icon(cx.t, 55.0, 55.0, "music", 28.0, T3);
        }
        // player chip
        let n = self.players.len();
        let label = if n > 1 { format!("{}/{}", self.players.iter().position(|x| x.bus == p.bus).unwrap_or(0) + 1, n) } else { String::new() };
        let lw = if label.is_empty() { 0.0 } else { 4.0 + cx.t.width(&label, &Style::ui(10.5, 600.0)) };
        let chip_w = 14.0 + lw + 14.0;
        let chip_x = w - 16.0 - chip_w;
        c.rrect(chip_x, 18.0, chip_w, 24.0, 12.0, if self.hov(&Hit::Chip) && n > 1 { FILL2 } else { FILL });
        let entry = if p.entry.is_empty() { p.identity.to_lowercase() } else { p.entry.clone() };
        if let Some(pm) = cx.imgs.get(&entry, 28, cx.icons).or_else(|| cx.imgs.get("multimedia-player", 28, cx.icons)) {
            img::draw(c, &pm, chip_x + 7.0, 23.0, 14.0, 14.0, 0.0, false, 1.0);
        }
        if !label.is_empty() {
            c.text(cx.t, chip_x + 7.0 + 14.0 + 4.0, 30.0, &label, &Style::ui(10.5, 600.0), T2);
        }
        self.hit(chip_x, 18.0, chip_w, 24.0, Hit::Chip);
        let tx = 16.0 + 78.0 + 14.0;
        let tw = chip_x - tx - 10.0;
        let title = crate::bar::elide(cx.t, if p.title.is_empty() { &p.identity } else { &p.title }, &Style::ui(15.0, 650.0), tw);
        c.text(cx.t, tx, 19.0 + 10.0, &title, &Style::ui(15.0, 650.0), T1);
        let sub = [p.artist.clone(), p.album.clone(), p.identity.clone()].into_iter().find(|s| !s.is_empty()).unwrap_or_default();
        let sub = crate::bar::elide(cx.t, &sub, &Style::ui(12.5, 450.0), tw);
        c.text(cx.t, tx, 19.0 + 20.0 + 1.0 + 9.0, &sub, &Style::ui(12.5, 450.0), T2);
        // controls
        let mut x = 16.0 + 78.0 + 8.0;
        let ctl: Vec<(&'static str, &str, bool, bool, bool)> = vec![
            ("shuffle", "shuffle", p.shuffle.is_some(), p.shuffle == Some(true), false),
            ("prev", "prevF", true, false, !p.can_prev),
            ("play", if p.playing { "pauseF" } else { "playF" }, true, false, !p.can_toggle),
            ("next", "nextF", true, false, !p.can_next),
            ("loop", if p.loop_status.as_deref() == Some("Track") { "repeatOne" } else { "repeat" }, p.loop_status.is_some(), p.loop_status.as_deref().map(|l| l != "None").unwrap_or(false), false),
        ];
        for (id, icon, vis, lit, off) in ctl {
            if !vis {
                continue;
            }
            let big = id == "play";
            let bw = if big { 38.0 } else { 32.0 };
            let d = if big { 36.0 } else { 30.0 };
            let (bx, by) = (x + bw / 2.0, 58.0 + 19.0);
            let h = self.hov(&Hit::Ctl(id));
            let op = if off { 0.35 } else { 1.0 };
            if big {
                c.rrect(bx - d / 2.0, by - d / 2.0, d, d, d / 2.0, T1.alpha(op));
            } else if h {
                c.rrect(bx - d / 2.0, by - d / 2.0, d, d, d / 2.0, FILL2.alpha(op));
            }
            c.icon(cx.t, bx, by, icon, if big { 17.0 } else { 16.0 }, if big { INK } else if lit { ACCENT.alpha(op) } else { T1.alpha(op) });
            if !off {
                self.hit(x, 58.0, bw, 38.0, Hit::Ctl(id));
            }
            x += bw + 4.0;
        }
        // progress / volume while it changes
        let vol = self.vol_recent();
        if p.length > 0.0 && !vol {
            let frac = self.seek_drag.unwrap_or(if p.length > 0.0 { (p.pos() / p.length).min(1.0) as f32 } else { 0.0 });
            let (sx, sy, sw) = (18.0, 106.0, w - 36.0);
            let st = Style::ui(11.0, 500.0).tnum();
            c.text(cx.t, sx, sy + 10.0, &fmt(frac as f64 * p.length), &st, T3);
            let rem = format!("−{}", fmt(p.length - frac as f64 * p.length));
            let rw = cx.t.width(&rem, &st);
            c.text(cx.t, sx + sw - rw, sy + 10.0, &rem, &st, T3);
            let (tx0, tx1) = (sx + 38.0 + 6.0, sx + sw - 42.0 - 6.0);
            let big = self.hov(&Hit::Seek) || self.seek_drag.is_some();
            let th = if big { 7.0 } else { 5.0 };
            c.rrect(tx0, sy + 10.0 - th / 2.0, tx1 - tx0, th, th / 2.0, FILL2);
            c.rrect(tx0, sy + 10.0 - th / 2.0, ((tx1 - tx0) * frac).max(th), th, th / 2.0, T1);
            if p.can_seek {
                self.hit(tx0, sy + 3.0 - 7.0, tx1 - tx0, 14.0 + 14.0, Hit::Seek);
            }
        } else if vol {
            self.volume_row(c, cx, 18.0, 106.0, w - 36.0, 20.0, false);
        }
    }
    pub fn seek_track(&self, w: f32) -> (f32, f32) {
        // x range of the seek track inside the player view (for dragging)
        let ox = WIN_W as f32 / 2.0 - w / 2.0;
        (ox + 18.0 + 44.0, ox + w - 18.0 - 48.0)
    }

    fn volume_row(&mut self, c: &mut Canvas, cx: &mut Ctx, x: f32, y: f32, w: f32, h: f32, mic: bool) {
        let a = &self.audio;
        let (vol, mute) = if mic { (a.src_vol, a.src_mute) } else { (a.vol, a.mute) };
        let cy = y + h / 2.0;
        let icon = if mic { if mute || vol == 0.0 { "micOff" } else { "mic" } } else if mute || vol == 0.0 { "volumeOff" } else if vol < 0.4 { "volLow" } else { "volume" };
        c.icon(cx.t, x + 10.0, cy, icon, 17.0, if mute { T3 } else { T1 });
        let st = Style::ui(12.5, 600.0).tnum();
        let txt = if mute { "wycisz.".to_string() } else { format!("{}%", (vol * 100.0).round()) };
        let col = if mute { T3 } else if vol > 1.001 { ORANGE } else { T1 };
        let tw = cx.t.width(&txt, &st);
        c.text(cx.t, x + w - tw, cy, &txt, &st, col);
        let (bx0, bx1) = (x + 20.0 + 10.0, x + w - 46.0 - 8.0);
        c.rrect(bx0, cy - 3.0, bx1 - bx0, 6.0, 3.0, FILL2);
        c.rrect(bx0, cy - 3.0, (bx1 - bx0) * vol.min(1.0), 6.0, 3.0, col);
    }

    fn v_call(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32) {
        let call = cx.phone.call().or(self.last_call.clone()).unwrap_or(Value::Null);
        let st = call["state"].as_str().unwrap_or("").to_string();
        let ring = st == "incoming" || st == "waiting";
        let live = st == "active";
        let open = ring || self.hover || self.keypad || (!live && st != "held");
        let au = cx.phone.get("callAudio");
        let status = self.call_status(&call, cx.phone);
        let who = call["who"].as_str().unwrap_or("").to_string();
        let bt = call["bt"].as_bool().unwrap_or(false);
        if !open {
            let mut x = 15.0;
            let cy = h / 2.0;
            c.icon(cx.t, x + 7.0, cy, "call", 14.0, GREEN);
            x += Canvas::icon_w(cx.t, "call", 14.0) + 8.0;
            let wst = Style::ui(13.0, 600.0);
            let wv = crate::bar::elide(cx.t, &who, &wst, 180.0);
            x += c.text(cx.t, x, cy, &wv, &wst, T1) + 8.0;
            x += c.text(cx.t, x, cy, &status, &Style::ui(13.0, 650.0).tnum(), if live { GREEN } else { T3 }) + 8.0;
            if au["mute"].as_bool().unwrap_or(false) {
                c.icon(cx.t, x + 6.5, cy, "micOff", 13.0, ORANGE_I);
            }
            return;
        }
        // avatar
        c.rrect(14.0, 14.0, 50.0, 50.0, 25.0, GREEN.alpha(0.18));
        let first = who.chars().next().unwrap_or('?');
        if first.is_alphabetic() {
            let s = first.to_uppercase().to_string();
            let tw = cx.t.width(&s, &Style::ui(20.0, 650.0));
            c.text(cx.t, 39.0 - tw / 2.0, 39.0, &s, &Style::ui(20.0, 650.0), GREEN);
        } else {
            c.icon(cx.t, 39.0, 39.0, if ring { "callRing" } else { "call" }, 20.0, GREEN);
        }
        let tx = 14.0 + 50.0 + 12.0;
        let tw = w - tx - if ring { 128.0 } else { 16.0 };
        let audio_text = if !bt { if ring { "bez Bluetooth: dźwięk na telefonie".to_string() } else { String::new() } } else if let Some(e) = au["err"].as_str().filter(|e| !e.is_empty()) { e.to_string() } else if !au["pc"].as_bool().unwrap_or(true) { "dźwięk na telefonie".into() } else if au["on"].as_bool().unwrap_or(false) { format!("dźwięk przez komputer{}", if au["mute"].as_bool().unwrap_or(false) { " · mikrofon wyciszony" } else { "" }) } else if ring { String::new() } else { "łączę dźwięk…".into() };
        let num = call["number"].as_str().unwrap_or("");
        let line3 = [if !num.is_empty() && num != who { num.to_string() } else { String::new() }, audio_text].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
        let rows = 2 + (!line3.is_empty()) as usize;
        let mut y = 39.0 - (15.0 + 21.0 + if rows > 2 { 15.0 } else { 0.0 }) / 2.0;
        let s1 = Style::ui(11.5, if live { 650.0 } else { 450.0 }).tnum();
        let l1 = crate::bar::elide(cx.t, &status, &s1, tw);
        c.text(cx.t, tx, y + 7.5, &l1, &s1, if live { GREEN } else { T3 });
        y += 16.0;
        let s2 = Style::ui(16.0, 650.0);
        let l2 = crate::bar::elide(cx.t, &who, &s2, tw);
        c.text(cx.t, tx, y + 10.5, &l2, &s2, T1);
        y += 22.0;
        if !line3.is_empty() {
            let s3 = Style::ui(11.0, 450.0);
            let l3 = crate::bar::elide(cx.t, &line3, &s3, tw);
            c.text(cx.t, tx, y + 7.0, &l3, &s3, if au["err"].as_str().map(|e| !e.is_empty()).unwrap_or(false) { ORANGE_I } else { T3 });
        }
        if ring {
            let (b1, b2) = (w - 16.0 - 46.0 - 14.0 - 46.0, w - 16.0 - 46.0);
            for (bx, icon, col, hit) in [(b1, "callEnd", DANGER, Hit::CallHangup), (b2, "call", GREEN, Hit::CallAnswer)] {
                let hv = self.hov(&hit);
                c.rrect(bx, 39.0 - 23.0, 46.0, 46.0, 23.0, if hv { col.mix(Rgba(1.0, 1.0, 1.0, 1.0), 0.12) } else { col });
                c.icon(cx.t, bx + 23.0, 39.0, icon, 21.0, Rgba(1.0, 1.0, 1.0, 1.0));
                self.hit(bx, 16.0, 46.0, 46.0, hit);
            }
            return;
        }
        // in a call: mute, audio, keypad; hang up
        let mut x = 14.0;
        if bt {
            let mute = au["mute"].as_bool().unwrap_or(false);
            let pc = au["pc"].as_bool().unwrap_or(true);
            for (icon, text, primary, hit) in [
                (if mute { "micOff" } else { "mic" }, if mute { "Wyciszony" } else { "Mikrofon" }, mute, Hit::CallMute),
                (if pc { "speaker" } else { "mobile" }, if pc { "Komputer" } else { "Telefon" }, false, Hit::CallAudio),
                ("dialpad", "Klawiatura", self.keypad, Hit::Keypad),
            ] {
                x += self.act_btn(c, cx, x, 76.0, icon, text, primary, hit) + 6.0;
            }
        }
        let st = Style::ui(12.0, 650.0);
        let ew = Canvas::icon_w(cx.t, "callEnd", 14.0) + 5.0 + cx.t.width("Zakończ", &st) + 24.0;
        let ex = w - 14.0 - ew;
        let hv = self.hov(&Hit::CallHangup);
        c.rrect(ex, 74.0, ew, 30.0, 15.0, if hv { DANGER.mix(Rgba(1.0, 1.0, 1.0, 1.0), 0.1) } else { DANGER });
        c.icon(cx.t, ex + 12.0 + 7.0, 89.0, "callEnd", 14.0, Rgba(1.0, 1.0, 1.0, 1.0));
        let ciw = Canvas::icon_w(cx.t, "callEnd", 14.0);
        c.text(cx.t, ex + 12.0 + ciw + 5.0, 89.0, "Zakończ", &st, Rgba(1.0, 1.0, 1.0, 1.0));
        self.hit(ex, 74.0, ew, 30.0, Hit::CallHangup);
        if self.keypad {
            let kw = (452.0 - 28.0 - 30.0) / 6.0;
            for (i, k) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"].iter().enumerate() {
                let (kx, ky) = (14.0 + (i % 6) as f32 * (kw + 6.0), 116.0 + (i / 6) as f32 * 38.0);
                let hit = Hit::Key(k.to_string());
                c.rrect(kx, ky, kw, 32.0, 10.0, if self.hov(&hit) { FILL2 } else { FILL });
                let tw = cx.t.width(k, &Style::ui(15.0, 600.0));
                c.text(cx.t, kx + (kw - tw) / 2.0, ky + 16.0, k, &Style::ui(15.0, 600.0), T1);
                self.hit(kx, ky, kw, 32.0, hit);
            }
        }
    }

    // small text button (ActBtn): 26 px, primary = accent
    fn act_btn(&mut self, c: &mut Canvas, cx: &mut Ctx, x: f32, y: f32, icon: &str, text: &str, primary: bool, hit: Hit) -> f32 {
        let st = Style::ui(11.5, 600.0);
        let iw = if icon.is_empty() { 0.0 } else { Canvas::icon_w(cx.t, icon, 12.0) + 5.0 };
        let w = iw + cx.t.width(text, &st) + 20.0;
        let hv = self.hov(&hit);
        c.rrect(x, y, w, 26.0, 13.0, if primary { if hv { ACCENT.mix(Rgba(1.0, 1.0, 1.0, 1.0), 0.08) } else { ACCENT } } else if hv { FILL2 } else { FILL });
        let mut tx = x + 10.0;
        if !icon.is_empty() {
            c.icon(cx.t, tx + (iw - 5.0) / 2.0, y + 13.0, icon, 12.0, if primary { INK } else { T2 });
            tx += iw;
        }
        c.text(cx.t, tx, y + 13.0, text, &st, if primary { INK } else { T1 });
        self.hit(x, y, w, 26.0, hit);
        w
    }

    fn v_notif(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32) {
        let e = self.last.get("notif").cloned().unwrap_or_default();
        let open = self.hover || self.replying;
        let pn = phone_note(e.phone.as_ref(), cx.phone);
        let iy = if open { 14.0 } else { (h - 38.0) / 2.0 };
        c.rrect(14.0, iy, 38.0, 38.0, 11.0, FILL);
        let src = pn.as_ref().and_then(|n| n["icon"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())).unwrap_or(e.icon.clone());
        let pad = if pn.as_ref().map(|n| n["icon"].as_str().map(|s| !s.is_empty()).unwrap_or(false)).unwrap_or(false) { 0.0 } else { 3.0 };
        if let Some(pm) = cx.imgs.get(&src, 96, cx.icons) {
            img::draw(c, &pm, 14.0 + pad, iy + pad, 38.0 - 2.0 * pad, 38.0 - 2.0 * pad, 11.0, false, 1.0);
        }
        if e.phone.is_some() {
            c.rrect(14.0 + 38.0 - 13.0, iy + 38.0 - 13.0, 17.0, 17.0, 8.5, Rgba(0.0, 0.0, 0.0, 1.0));
            c.icon(cx.t, 14.0 + 38.0 - 4.5, iy + 38.0 - 4.5, "mobile", 10.0, ACCENT);
        }
        let x = 14.0 + 38.0 + 12.0;
        let cw = w - x - 18.0;
        let mut y = 12.0;
        let sa = Style::ui(11.0, 600.0);
        let app = crate::bar::elide(cx.t, &e.app, &sa, cw - 110.0);
        let aw = c.text(cx.t, x, y + 7.0, &app, &sa, T3);
        let nw = c.text(cx.t, x + aw + 6.0, y + 7.0, "teraz", &Style::ui(11.0, 450.0), T4);
        if e.urgent {
            c.rrect(x + aw + 6.0 + nw + 6.0, y + 3.5, 7.0, 7.0, 3.5, DANGER);
        }
        y += 14.0 + 1.0;
        let st = Style::ui(13.5, 600.0);
        for l in wrap(cx.t, &e.title, &st, cw, if open { 2 } else { 1 }) {
            c.text(cx.t, x, y + 8.5, &l, &st, T1);
            y += 17.0;
        }
        y += 1.0;
        if !e.body.is_empty() {
            let sb = Style::ui(12.0, 450.0);
            for l in wrap(cx.t, &e.body, &sb, cw, if open { 6 } else { 1 }) {
                c.text(cx.t, x, y + 7.75, &l, &sb, T2);
                y += 15.5;
            }
        }
        if e.value >= 0 {
            let pct = format!("{}%", e.value);
            let ps = Style::ui(10.5, 600.0).tnum();
            let pw = cx.t.width(&pct, &ps);
            c.text(cx.t, x + cw - pw, y + 6.0, &pct, &ps, T3);
            let bw = cw - pw - 8.0;
            self.note_bar.set(e.value.clamp(0, 100) as f32 / 100.0, 220.0, Ease::OutCubic);
            c.rrect(x, y + 3.5, bw, 5.0, 2.5, FILL2);
            c.rrect(x, y + 3.5, bw * self.note_bar.value(), 5.0, 2.5, ACCENT);
            y += 13.0;
        }
        let pn_actions = pn.as_ref().and_then(|n| {
            let acts: Vec<String> = n["actions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).take(3).collect()).unwrap_or_default();
            let reply = n["reply"].as_bool().unwrap_or(false);
            let dismiss = n["dismiss"].as_bool().unwrap_or(false);
            if reply || dismiss || !acts.is_empty() {
                Some((reply, acts, dismiss))
            } else {
                None
            }
        });

        let has_smart = !e.codes.is_empty() || !e.links.is_empty();
        let has_phone = open && pn_actions.is_some();
        let has_desktop = open && !e.desktop_actions.is_empty();

        if self.replying || has_smart || has_phone || has_desktop {
            y += 9.0;
            if self.replying {
                c.rrect(x, y, cw, 32.0, 16.0, FILL);
                crate::gfx::stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, 31.0, 15.5, Rgba(169.0 / 255.0, 194.0 / 255.0, 238.0 / 255.0, 0.45));
                let ts = Style::ui(13.0, 450.0);
                if self.reply.is_empty() {
                    let ph = crate::bar::elide(cx.t, &format!("Odpowiedź do {}", e.title), &Style::ui(12.5, 450.0), cw - 13.0 - 34.0);
                    c.text(cx.t, x + 13.0, y + 16.0, &ph, &Style::ui(12.5, 450.0), T4);
                } else {
                    // the end of the text stays visible
                    let mut s = self.reply.clone();
                    while cx.t.width(&s, &ts) > cw - 13.0 - 40.0 && !s.is_empty() {
                        s.remove(0);
                    }
                    let tw = c.text(cx.t, x + 13.0, y + 16.0, &s, &ts, T1);
                    c.rrect(x + 13.0 + tw + 1.0, y + 8.0, 1.5, 16.0, 0.0, T1.alpha(0.85));
                }
                let can = !self.reply.trim().is_empty();
                c.rrect(x + cw - 3.0 - 26.0, y + 3.0, 26.0, 26.0, 13.0, if can { ACCENT } else { FILL2 });
                c.icon(cx.t, x + cw - 3.0 - 13.0, y + 16.0, "send", 13.0, if can { INK } else { T3 });
                self.hit(x + cw - 29.0, y + 3.0, 26.0, 26.0, Hit::ReplySend);
            } else {
                let mut bx = x;
                let max_x = x + cw;
                let st = Style::ui(11.5, 600.0);
                let btn_w = |t: &mut crate::text::Text, icon: &str, text: &str| -> f32 {
                    let iw = if icon.is_empty() { 0.0 } else { Canvas::icon_w(t, icon, 12.0) + 5.0 };
                    iw + t.width(text, &st) + 20.0
                };

                // 1. Detected codes: prominent copy button
                for code in &e.codes {
                    let label = format!("Kopiuj kod: {code}");
                    let bw = btn_w(cx.t, "clipboard", &label);
                    if bx + bw <= max_x {
                        bx += self.act_btn(c, cx, bx, y, "clipboard", &label, true, Hit::CopyCode(code.clone())) + 6.0;
                    } else {
                        let short_label = "Kopiuj kod";
                        let sbw = btn_w(cx.t, "clipboard", short_label);
                        if bx + sbw <= max_x {
                            bx += self.act_btn(c, cx, bx, y, "clipboard", short_label, true, Hit::CopyCode(code.clone())) + 6.0;
                        }
                    }
                }

                // 2. Detected links: copy link and open in browser
                for link in e.links.iter().take(1) {
                    let bw_copy = btn_w(cx.t, "copy", "Kopiuj link");
                    if bx + bw_copy <= max_x {
                        bx += self.act_btn(c, cx, bx, y, "copy", "Kopiuj link", false, Hit::CopyLink(link.clone())) + 6.0;
                    }
                    let bw_open = btn_w(cx.t, "world", "Otwórz");
                    if bx + bw_open <= max_x {
                        bx += self.act_btn(c, cx, bx, y, "world", "Otwórz", e.codes.is_empty(), Hit::OpenLink(link.clone())) + 6.0;
                    }
                }

                // 3. Phone actions
                if let Some((reply, acts, dismiss)) = pn_actions {
                    if reply {
                        let bw = btn_w(cx.t, "message", "Odpowiedz");
                        if bx + bw <= max_x {
                            let is_pri = e.codes.is_empty() && e.links.is_empty();
                            bx += self.act_btn(c, cx, bx, y, "message", "Odpowiedz", is_pri, Hit::NoteReply) + 6.0;
                        }
                    }
                    for a in acts {
                        let bw = btn_w(cx.t, "", &a);
                        if bx + bw <= max_x {
                            bx += self.act_btn(c, cx, bx, y, "", &a, false, Hit::NoteAction(a.clone())) + 6.0;
                        }
                    }
                    if dismiss {
                        let bw = btn_w(cx.t, "x", "Odrzuć");
                        if bx + bw <= max_x {
                            self.act_btn(c, cx, bx, y, "x", "Odrzuć", false, Hit::NoteDismiss);
                        }
                    }
                }

                // 4. Desktop actions
                if open {
                    for (key, label) in &e.desktop_actions {
                        let bw = btn_w(cx.t, "", label);
                        if bx + bw <= max_x {
                            bx += self.act_btn(c, cx, bx, y, "", label, false, Hit::DesktopAction(e.note_id, key.clone())) + 6.0;
                        }
                    }
                }
            }
        }
        let waiting = self.waiting();
        if waiting > 0 {
            let s = format!("+{waiting}");
            let st = Style::ui(10.5, 650.0);
            let bw = cx.t.width(&s, &st) + 12.0;
            c.rrect(w - 14.0 - bw, 11.0, bw, 17.0, 8.5, FILL2);
            c.text(cx.t, w - 14.0 - bw + 6.0, 19.5, &s, &st, T2);
        }
    }

    fn v_track(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32, active: bool) {
        let st = self.last.get("track").map(|e| e.state.clone()).unwrap_or_default();
        let p = self.player().cloned();
        let cy = h / 2.0;
        c.rrect(9.0, cy - 17.0, 34.0, 34.0, 10.0, FILL);
        let art = p.as_ref().map(|p| p.art.clone()).unwrap_or_default();
        if let Some(pm) = cx.imgs.get(&art, 96, cx.icons) {
            img::draw(c, &pm, 9.0, cy - 17.0, 34.0, 34.0, 10.0, true, 1.0);
        } else {
            c.icon(cx.t, 26.0, cy, "music", 16.0, T3);
        }
        let tx = 9.0 + 34.0 + 11.0;
        let tw = w - tx - 52.0;
        if let Some(p) = &p {
            let a = crate::bar::elide(cx.t, &p.title, &Style::ui(13.0, 600.0), tw);
            c.text(cx.t, tx, cy - 8.0, &a, &Style::ui(13.0, 600.0), T1);
            let b = crate::bar::elide(cx.t, if p.artist.is_empty() { &p.identity } else { &p.artist }, &Style::ui(11.5, 450.0), tw);
            c.text(cx.t, tx, cy + 8.0, &b, &Style::ui(11.5, 450.0), T3);
        }
        if st.is_empty() {
            let run = active && self.playing();
            self.eq_draw(c, w - 18.0 - 18.0, cy - 7.0, run);
        } else {
            c.icon(cx.t, w - 16.0 - 9.0, cy, if st == "pause" { "pauseF" } else { "playF" }, 18.0, if st == "pause" { T2 } else { ACCENT });
        }
    }

    fn v_info(&mut self, c: &mut Canvas, cx: &mut Ctx, w: f32, h: f32) {
        let e = self.last.get("info").cloned().unwrap_or_default();
        let col = e.color.unwrap_or(ACCENT);
        let photo = e.photo && !e.img.is_empty();
        let stack = photo || e.title.chars().count() + e.sub.chars().count() > 38;
        let cy = h / 2.0;
        let x = if photo { 10.0 } else if stack { 11.0 } else { 7.0 };
        let (bw, bh) = if photo { (52.0, 38.0) } else { (28.0, 28.0) };
        if photo {
            c.rrect(x, cy - bh / 2.0, bw, bh, 9.0, FILL);
            if let Some(pm) = cx.imgs.get(&e.img, 160, cx.icons) {
                img::draw(c, &pm, x, cy - bh / 2.0, bw, bh, 9.0, true, 1.0);
            }
            crate::gfx::stroke_rrect(c, x + 0.5, cy - bh / 2.0 + 0.5, bw - 1.0, bh - 1.0, 8.5, RIM);
        } else if !e.img.is_empty() {
            if let Some(pm) = cx.imgs.get(&e.img, 48, cx.icons) {
                img::draw(c, &pm, x + 2.0, cy - 12.0, 24.0, 24.0, 0.0, false, 1.0);
            }
        } else {
            c.rrect(x, cy - 14.0, 28.0, 28.0, 14.0, e.swatch.unwrap_or(col.alpha(0.18)));
            if e.swatch.is_some() {
                crate::gfx::stroke_rrect(c, x + 0.5, cy - 13.5, 27.0, 27.0, 13.5, RIM);
            } else {
                c.icon(cx.t, x + 14.0, cy, if e.icon.is_empty() { "bell" } else { &e.icon }, 15.0, col);
            }
        }
        let tx = x + bw + if stack { 11.0 } else { 10.0 };
        if stack {
            let a = crate::bar::elide(cx.t, &e.title, &Style::ui(13.0, 600.0), 380.0);
            c.text(cx.t, tx, cy - 8.0, &a, &Style::ui(13.0, 600.0), T1);
            let b = crate::bar::elide(cx.t, &e.sub, &Style::ui(11.5, 450.0), 380.0);
            c.text(cx.t, tx, cy + 8.5, &b, &Style::ui(11.5, 450.0), T3);
        } else {
            let a = crate::bar::elide(cx.t, &e.title, &Style::ui(13.0, 600.0), 260.0);
            let aw = c.text(cx.t, tx, cy, &a, &Style::ui(13.0, 600.0), T1);
            if !e.sub.is_empty() {
                let b = crate::bar::elide(cx.t, &e.sub, &Style::ui(12.0, 450.0), 220.0);
                c.text(cx.t, tx + aw + 10.0, cy, &b, &Style::ui(12.0, 450.0), T3);
            }
        }
        let _ = w;
    }
}

fn ring(c: &mut Canvas, cx: f32, cy: f32, frac: f32, col: Rgba) {
    c.arc(cx, cy, 7.5 - 1.1, 1.0, 2.2, FILL2);
    c.arc(cx, cy, 7.5 - 1.1, frac.clamp(0.001, 1.0), 2.2, col);
}
fn elide_w(t: &mut Text, s: &str, st: &Style, max: f32) -> f32 {
    t.width(s, st).min(max)
}
// the phone note behind a KDE Connect notification (Island.phoneNote)
pub fn phone_note(p: Option<&(String, String)>, ph: &crate::phone::State) -> Option<Value> {
    let (app, raw) = p?;
    ph.notes().into_iter().find(|n| n["app"].as_str() == Some(app.as_str()) && {
        let t = n["text"].as_str().filter(|s| !s.is_empty()).or(n["title"].as_str()).unwrap_or("");
        raw.contains(&t.chars().take(48).collect::<String>())
    })
}

fn unescape_html(s: &str) -> String {
    let once = |t: &str| -> String {
        t.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
    };
    let s1 = once(s);
    if s1.contains('&') { once(&s1) } else { s1 }
}

fn strip_tags(s: &str) -> String {
    let u = unescape_html(s);
    let mut o = String::new();
    let mut tag = false;
    for ch in u.chars() {
        match ch {
            '<' => tag = true,
            '>' if tag => tag = false,
            c if !tag => o.push(c),
            _ => {}
        }
    }
    o.split_whitespace().collect::<Vec<_>>().join(" ")
}

// Parses a phone notification (e.g. from Messenger via KDE Connect) into:
// (chat_title, sender, latest_message)
fn parse_phone_message(raw_summary: &str, raw_body: &str) -> (String, String, String) {
    let s = unescape_html(raw_body);
    let (chat_title, rest) = match s.split_once(": ") {
        Some((w, r)) if !w.is_empty() && w.chars().count() <= 120 && !w.contains('\n') => {
            (strip_tags(w), r.trim())
        }
        _ => (String::new(), s.trim()),
    };

    // Split messages/lines by <br...>, \r\n, \n, \r
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    let bytes = rest.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 3 < bytes.len() {
            let rem = &rest[i..];
            if let Some(close) = rem.find('>') {
                let tag = &rem[..=close];
                if tag.len() >= 4 && tag[1..].trim_start().to_ascii_lowercase().starts_with("br") {
                    let trimmed = cur.trim();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                    cur.clear();
                    i += tag.len();
                    continue;
                }
            }
        }
        let ch = rest[i..].chars().next().unwrap();
        if ch == '\n' || ch == '\r' {
            let trimmed = cur.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
        i += ch.len_utf8();
    }
    let trimmed = cur.trim();
    if !trimmed.is_empty() {
        lines.push(trimmed.to_string());
    }

    if lines.is_empty() {
        return (
            if chat_title.is_empty() { strip_tags(raw_summary) } else { chat_title },
            String::new(),
            strip_tags(rest),
        );
    }

    let mut messages: Vec<(String, String)> = Vec::new();
    let mut current_sender = String::new();

    for line in &lines {
        let line_lower = line.to_ascii_lowercase();
        if let Some(b_start) = line_lower.find("<b>") {
            if let Some(b_end) = line_lower[b_start + 3..].find("</b>") {
                let sender_raw = &line[b_start + 3..b_start + 3 + b_end];
                current_sender = strip_tags(sender_raw);
                let after = &line[b_start + 3 + b_end + 4..];
                let after_clean = strip_tags(after.trim_start_matches(':').trim());
                if !after_clean.is_empty() {
                    messages.push((current_sender.clone(), after_clean));
                }
                continue;
            }
        }

        if current_sender.is_empty() {
            if let Some((sender_part, msg_part)) = line.split_once(": ") {
                if !sender_part.is_empty() && sender_part.chars().count() <= 40 && !sender_part.contains('\n') {
                    messages.push((strip_tags(sender_part), strip_tags(msg_part)));
                    continue;
                }
            }
        }

        messages.push((current_sender.clone(), strip_tags(line)));
    }

    let (last_sender, last_msg) = if let Some(last) = messages.last() {
        last.clone()
    } else {
        (String::new(), strip_tags(lines.last().unwrap()))
    };

    let title = if !chat_title.is_empty() {
        chat_title
    } else if !last_sender.is_empty() {
        last_sender.clone()
    } else {
        strip_tags(raw_summary)
    };

    (title, last_sender, last_msg)
}

pub fn extract_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    for token in text.split_whitespace() {
        let mut t = token.trim_start_matches(|c: char| matches!(c, '(' | '[' | '{' | '<' | '"' | '\'' | '`' | '«' | '“'));
        let open_parens = t.chars().filter(|&c| c == '(').count();
        let close_parens = t.chars().filter(|&c| c == ')').count();
        if close_parens > open_parens {
            t = t.trim_end_matches(')');
        }
        let clean = t.trim_end_matches(|c: char| matches!(c, ']' | '}' | '>' | '"' | '\'' | '`' | ',' | ';' | '.' | '!' | '?' | '»' | '”'));
        if (clean.starts_with("https://") || clean.starts_with("http://")) && clean.len() > 10 && clean[8..].contains('.') {
            if !links.contains(&clean.to_string()) {
                links.push(clean.to_string());
            }
        } else if clean.starts_with("www.") && clean.len() > 7 && clean[4..].contains('.') {
            if !links.contains(&clean.to_string()) {
                links.push(clean.to_string());
            }
        }
    }
    links
}

pub fn extract_codes(text: &str, links: &[String]) -> Vec<String> {
    let mut clean_text = text.to_string();
    for l in links {
        clean_text = clean_text.replace(l, " ");
    }

    let lower = clean_text.to_lowercase();
    let keywords = [
        "kod", "code", "blik", "hasło", "haslo", "otp", "2fa", "pin", "token",
        "weryfik", "verif", "autoryz", "authoriz", "odbior", "security", "potwierdz", "potwierdź"
    ];
    let has_kw = keywords.iter().any(|k| lower.contains(k));

    let mut codes = Vec::new();

    // 1. Prefix codes like G-123456 or AB-123456
    for word in clean_text.split_whitespace() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if let Some((prefix, rest)) = w.split_once('-') {
            if (1..=3).contains(&prefix.len()) && prefix.chars().all(|c| c.is_ascii_alphabetic())
                && (4..=8).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_digit())
            {
                if !codes.contains(&rest.to_string()) {
                    codes.push(rest.to_string());
                }
            }
        }
    }

    // 2. Tokenized words
    let words: Vec<&str> = clean_text.split_whitespace().collect();

    // Split codes: "123-456" or consecutive "123" "456"
    for (i, &w) in words.iter().enumerate() {
        let cw = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if let Some((a, b)) = cw.split_once('-') {
            if (3..=4).contains(&a.len()) && a.chars().all(|c| c.is_ascii_digit())
                && (3..=4).contains(&b.len()) && b.chars().all(|c| c.is_ascii_digit())
            {
                let code = format!("{a}{b}");
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }
        if i + 1 < words.len() {
            let next_w = words[i + 1].trim_matches(|c: char| !c.is_alphanumeric());
            let a = cw;
            let b = next_w;
            if a.len() == 3 && a.chars().all(|c| c.is_ascii_digit())
                && b.len() == 3 && b.chars().all(|c| c.is_ascii_digit())
            {
                let start_idx = i.saturating_sub(3);
                let end_idx = (i + 4).min(words.len());
                let local_has_kw = words[start_idx..end_idx].iter().any(|token| {
                    let tl = token.to_lowercase();
                    keywords.iter().any(|k| tl.contains(k))
                });
                if local_has_kw || has_kw || words.len() <= 4 {
                    let code = format!("{a}{b}");
                    if !codes.contains(&code) {
                        codes.push(code);
                    }
                }
            }
        }
    }

    // 3. 4-8 digit numbers or uppercase alphanumeric codes near keyword
    for (i, &w) in words.iter().enumerate() {
        let cw = w.trim_matches(|c: char| !c.is_ascii_digit());
        if (4..=8).contains(&cw.len()) && cw.chars().all(|c| c.is_ascii_digit()) {
            let is_year = if cw.len() == 4 {
                if let Ok(y) = cw.parse::<u32>() {
                    (1990..=2035).contains(&y)
                } else { false }
            } else { false };

            let start_idx = i.saturating_sub(4);
            let end_idx = (i + 5).min(words.len());
            let local_has_kw = words[start_idx..end_idx].iter().any(|token| {
                let tl = token.to_lowercase();
                keywords.iter().any(|k| tl.contains(k))
            });

            if (local_has_kw && !is_year) || (local_has_kw && is_year && i > 0 && {
                let prev = words[i - 1].to_lowercase();
                prev.contains("kod") || prev.contains("code") || prev.contains("pin")
            }) || (!is_year && words.len() <= 2) {
                let code = cw.to_string();
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }

        // Alphanumeric uppercase codes (e.g. Steam Guard: 8HX92)
        let cw_upper = w.trim_matches(|c: char| !c.is_alphanumeric());
        if (4..=8).contains(&cw_upper.len())
            && cw_upper.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && cw_upper.chars().any(|c| c.is_ascii_digit())
            && cw_upper.chars().any(|c| c.is_ascii_uppercase())
        {
            let start_idx = i.saturating_sub(3);
            let end_idx = (i + 4).min(words.len());
            let local_has_kw = words[start_idx..end_idx].iter().any(|token| {
                let tl = token.to_lowercase();
                keywords.iter().any(|k| tl.contains(k))
            });
            if local_has_kw {
                let code = cw_upper.to_string();
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }
    }

    // 4. Standalone line with 4-8 digits
    for line in clean_text.lines() {
        let cl = line.trim().trim_matches(|c: char| !c.is_ascii_digit());
        if (4..=8).contains(&cl.len()) && cl.chars().all(|c| c.is_ascii_digit()) {
            let code = cl.to_string();
            if !codes.contains(&code) {
                codes.push(code);
            }
        }
    }

    codes
}

#[cfg(test)]
mod special_tests {
    use super::*;
    // minimizing parks windows on special:minimized: no "special workspace" event for it, others still shown
    #[test]
    fn minimized_is_quiet() {
        let apps = Apps { entries: Vec::new() };
        let mut isl = Island::new();
        isl.armed = true;
        isl.on_hypr("activespecial", "special:minimized,DP-3", &apps, &[], "");
        assert!(isl.ev.is_none() && isl.queue.is_empty());
        isl.on_hypr("activespecial", "special:scratch,DP-3", &apps, &[], "");
        assert_eq!(isl.ev.as_ref().map(|e| e.key.as_str()), Some("special"));
    }
}
