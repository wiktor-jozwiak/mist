// Mist Player, native: `mist-ui-bin player [--add] [file | file:// uri | http(s) url ...]`, one process (later
// launches hand their files over the socket). libmpv decodes (hardware decoding) and draws the picture with GL
// on a subsurface under the window (engine.rs); the controls are the CPU canvas on top (ui.rs); MPRIS on the
// session bus (mpris.rs). Queue from the file's folder, resume where you stopped (~/.config/mist/player.json,
// the QML app's format), audio / subtitle tracks (external subtitles next to the file or dropped on the
// window), speed, shortcuts after mpv / IINA / YouTube.
// Cost rules: no mpv before the first file, no GL for audio, video frames only when the compositor shows
// them, the controls repaint on state changes only (the clock once a second), nothing while they are hidden.
pub mod engine;
pub mod ffi;
mod cover;
mod mpris;
mod thumbs;
mod ui;

use crate::appwin::{self, Cmd, Cx, Ev, Logic, VideoSurface, WinId, LEFT, MIDDLE, RIGHT};
use crate::gfx::Canvas;
use crate::text::Text;
use engine::{Engine, Event, Track, Tx};
use smithay_client_toolkit::seat::keyboard::Keysym;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use std::any::Any;
use std::time::{Duration, Instant};

pub enum Wake {
    Mpv,
    Render,
    Picked(Vec<String>, bool),
    Mpris(mpris::Req),
    // a seek-bar thumbnail: file, bucket, RGBA (None: ffmpeg failed)
    Thumb(String, u32, Option<Vec<u8>>),
    // album art for an audio file (None: none found)
    Cover(String, Option<tiny_skia::Pixmap>),
}

const VIDEO_EXT: [&str; 14] = ["mkv", "mp4", "m4v", "webm", "avi", "mov", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "ogv", "3gp"];
const AUDIO_EXT: [&str; 11] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav", "wma", "alac", "ape", "mka"];
const SUB_EXT: [&str; 6] = ["srt", "ass", "ssa", "vtt", "sub", "sup"];
// asynchronous position reads (Event::Reply ids)
const R_SPOT: u64 = 100;
const R_SAVE: u64 = 101;
const SPEEDS: [f64; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0];
// the controls stay this long after the last input (a playing video)
const UI_HOLD: Duration = Duration::from_millis(2600);

fn ext(p: &str) -> String {
    let b = p.rsplit('/').next().unwrap_or("");
    b.rfind('.').map(|i| b[i + 1..].to_lowercase()).unwrap_or_default()
}
fn is_media(p: &str) -> bool {
    let e = ext(p);
    VIDEO_EXT.contains(&e.as_str()) || AUDIO_EXT.contains(&e.as_str())
}
fn is_audio(p: &str) -> bool {
    AUDIO_EXT.contains(&ext(p).as_str())
}
fn is_url(p: &str) -> bool {
    p.starts_with("http://") || p.starts_with("https://")
}
// "Prince_and_The_Revolution_-_Purple_Rain.mp3" -> "Prince and The Revolution - Purple Rain"
pub fn pretty(p: &str) -> String {
    let b = p.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let s = match b.rfind('.') {
        Some(i) if i > 0 => &b[..i],
        _ => b,
    };
    let mut s = s.replace('_', " ");
    if let Some(i) = s.rfind(" (") {
        let tail = s[i + 2..].trim_end_matches(')').to_lowercase();
        if tail.ends_with(".com") || tail.ends_with(".pl") || tail.ends_with(".net") || tail == "mp3.pm" {
            s.truncate(i);
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
// "Artist - Title" in a name without tags: (artist, title), the title without "(Official Video)" and the like
pub fn split_name(s: &str) -> Option<(String, String)> {
    let (a, t) = s.split_once(" - ").or_else(|| s.split_once(" – "))?;
    let (a, t) = (a.trim(), clean_title(t));
    (!a.is_empty() && !t.is_empty() && a.chars().count() <= 60).then(|| (a.to_string(), t))
}
fn clean_title(t: &str) -> String {
    const JUNK: [&str; 12] = ["official", "video", "lyric", "audio", "visuali", "hd", "hq", "4k", "kbps", "download", "teledysk", "tekst"];
    let junk = |g: &str| {
        let g = g.to_lowercase();
        g.split(|c: char| !c.is_alphanumeric()).any(|w| JUNK.iter().any(|j| w.starts_with(j)))
    };
    let mut out = t.to_string();
    // (…) / […] groups that only say what kind of upload it is
    for (o, c) in [('(', ')'), ('[', ']')] {
        while let Some(i) = out.find(o) {
            let Some(j) = out[i..].find(c).map(|j| i + j) else { break };
            if !junk(&out[i + 1..j]) {
                break;
            }
            out.replace_range(i..=j, "");
        }
    }
    // a trailing " - Official Video"
    if let Some((head, tail)) = out.rsplit_once(" - ")
        && junk(tail)
    {
        out = head.to_string();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn time(sec: f64) -> String {
    let s = if sec > 0.0 { sec as u64 } else { 0 };
    let (h, m, ss) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{ss:02}") } else { format!("{m}:{ss:02}") }
}
pub fn rate(r: f64) -> String {
    format!("{}×", format!("{r}").replace('.', crate::i18n::dec_sep()))
}
fn lang(l: &str) -> String {
    let m = match l.to_lowercase().as_str() {
        "pol" | "pl" | "polish" => "Polski",
        "eng" | "en" | "english" => "Angielski",
        "ger" | "deu" | "de" | "german" => "Niemiecki",
        "fre" | "fra" | "fr" | "french" => "Francuski",
        "spa" | "es" | "spanish" => "Hiszpański",
        "ita" | "it" | "italian" => "Włoski",
        "jpn" | "ja" | "japanese" => "Japoński",
        "rus" | "ru" | "russian" => "Rosyjski",
        "ukr" | "uk" | "ukrainian" => "Ukraiński",
        "cze" | "ces" | "cs" | "czech" => "Czeski",
        "kor" | "ko" | "korean" => "Koreański",
        "und" | "unknown" | "" => "",
        _ => return l.to_string(),
    };
    m.to_string()
}
fn track_name(t: &Track, i: usize, kind: &str) -> String {
    let mut p: Vec<String> = vec![];
    if !t.title.is_empty() {
        p.push(t.title.clone());
    }
    let l = lang(&t.lang);
    if !l.is_empty() {
        p.push(l);
    }
    if t.external {
        p.push("plik".into());
    }
    if p.is_empty() { format!("{kind} {}", i + 1) } else { p.join(" · ") }
}
// MIST_PLAYER_TRACE=1: start-up steps on stderr (ms since the process started)
pub(crate) fn trace(what: &str) {
    use std::sync::OnceLock;
    static ON: OnceLock<Option<Instant>> = OnceLock::new();
    if let Some(t0) = ON.get_or_init(|| std::env::var_os("MIST_PLAYER_TRACE").map(|_| Instant::now())) {
        eprintln!("player {:7.1} ms  {what}", t0.elapsed().as_secs_f64() * 1000.0);
    }
}
fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}
// where a stopped file resumes (ms): not in the first 5 s nor the last 15 s
// the entries of an M3U / M3U8 playlist: paths relative to its folder, file:// URIs and URLs; # lines skipped
fn read_m3u(p: &str) -> Vec<String> {
    let dir = p.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if l.starts_with("file://") {
                appwin::uri_to_path(l)
            } else if l.starts_with('/') || is_url(l) {
                l.to_string()
            } else {
                format!("{dir}/{l}")
            }
        })
        .collect()
}

fn resume_spot(pos_ms: f64, dur_ms: f64) -> f64 {
    if (dur_ms > 0.0 && pos_ms > dur_ms - 15000.0) || pos_ms < 5000.0 { 0.0 } else { pos_ms.floor() }
}
// the queue for one opened file: the media files of its folder, natural order, and where it is in it
fn folder_queue(f: &str) -> (Vec<String>, usize) {
    let d = crate::files::fs::parent(f);
    let mut list: Vec<String> = std::fs::read_dir(&d).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| is_media(n) && !n.starts_with('.')).collect()).unwrap_or_default();
    list.sort_by(|a, b| crate::files::fs::natural_cmp(a, b));
    let list: Vec<String> = list.iter().map(|n| crate::files::fs::join(&d, n)).collect();
    match list.iter().position(|p| p == f) {
        Some(i) => (list, i),
        None => (vec![f.to_string()], 0),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Panel {
    None,
    List,
    Tracks,
    Speed,
    Help,
    Info,
    Chapters,
}
#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    Picture,
    Drag,
    Light(u8),
    Open,
    Help,
    Play,
    Prev,
    Next,
    Back10,
    Fwd10,
    Mute,
    Vol,
    Seek,
    Remaining,
    Speed,
    Tracks,
    List,
    Full,
    Item(usize),
    Queue(usize),
    QueueRemove(usize),
    Shuffle,
    Repeat,
    Add,
    Recent(usize),
    RecentForget(usize),
    FromStart,
    Card,
    Chapters,
    Pip,
    ResumeQueue,
    // the Picture-in-Picture window's own buttons
    PipPlay,
    PipBack,
    PipClose,
}

#[derive(Clone)]
pub struct Recent {
    pub path: String,
    pub pos: f64, // ms (player.json)
    pub dur: f64,
    pub t: f64,
}

// what the popover's rows do
#[derive(Clone)]
enum Row {
    Head(String),
    Pick(String, bool, RowAct),
}
#[derive(Clone)]
enum RowAct {
    Speed(f64),
    Aid(i64),
    Sid(i64),
    Chapter(f64),
    None,
}

pub struct Player {
    eng: Option<Engine>,
    pub err: String,
    no_libs: bool,
    mpris: Option<mpris::Mpris>,
    tx: Option<Tx>,
    display: *mut std::ffi::c_void,
    win: Option<WinId>,
    pub w: f32,
    pub h: f32,
    pub queue: Vec<String>,
    pub queue_scroll: usize,
    pub idx: i32,
    pub cur: String,
    pub repeat: u8, // 0 off, 1 all, 2 one
    pub shuffle: bool,
    pub panel: Panel,
    remaining: bool,
    pub pos: f64,
    pub dur: f64,
    pub paused: bool,
    pub video: bool,
    pub speed: f64,
    pub title: String,
    artist: String,
    album: String,
    pub tracks: Vec<Track>,
    pub chapters: Vec<(f64, String)>,
    pub volume: f64,
    pub muted: bool,
    pub recents: Vec<Recent>,
    pending_resume: f64,
    // the file waits for the GL plane (EGL without surfaceless contexts)
    pending_load: bool,
    pub fs: bool,
    ui_until: Instant,
    hits: Vec<(f32, f32, f32, f32, Hit)>,
    hover: Option<Hit>,
    mouse: (f32, f32),
    pub osd: Option<(&'static str, String, Instant)>,
    flash: Option<(&'static str, Instant)>,
    resume_at: Option<(f64, Instant)>,
    drag: Option<Hit>,
    seek_preview: Option<f64>,
    drop_hover: bool,
    click_at: Option<Instant>,
    text: Text,
    // the second the controls show (they repaint when it changes)
    shown_sec: i64,
    save_at: Option<Instant>,
    spot_at: Instant,
    rows: Vec<Row>,
    paints: u64,
    paint_us: u64,
    backdrop: Option<(u32, u32, bool, tiny_skia::Pixmap)>,
    // album art of the current audio track and its backdrop colours (cover_for: the track it was asked for)
    cover_for: String,
    cover: Option<std::rc::Rc<tiny_skia::Pixmap>>,
    cover_cols: Option<[crate::gfx::Rgba; 3]>,
    // the art scaled and rounded at the size last drawn (drawn 1:1 after that)
    cover_px: Option<(u32, tiny_skia::Pixmap)>,
    png_req: Option<String>,
    launched: Instant,
    first_frame_ms: Option<f64>,
    title_shown: String,
    thumbs: thumbs::Thumbs,
    // the queue of the last session (player.json), offered on the empty screen
    pub saved_queue: Vec<String>,
    pub saved_idx: usize,
    // Picture-in-Picture: a second window (mist-comp pins "Picture-in-Picture…" titles, bottom right) that
    // takes the video plane while it is open; the main window shows a placeholder
    pip: Option<WinId>,
    pip_token: u64,
    pip_closing: bool,
    video_win: Option<WinId>,
    pub pip_size: (f32, f32),
    pub pip_hover: bool,
    pip_hits: Vec<(f32, f32, f32, f32, Hit)>,
    pip_mouse: (f32, f32),
}

fn cfg_path() -> String {
    format!("{}/.config/mist/player.json", std::env::var("HOME").unwrap_or_default())
}

impl Player {
    pub fn new() -> Player {
        trace("start");
        let mut p = Player::bare();
        if let Ok(s) = std::fs::read_to_string(cfg_path())
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
        {
            p.volume = v["volume"].as_f64().unwrap_or(0.8).clamp(0.0, 1.0);
            p.muted = v["muted"].as_bool().unwrap_or(false);
            if let Some(a) = v["recents"].as_array() {
                p.recents = a
                    .iter()
                    .filter_map(|e| Some(Recent { path: e["path"].as_str()?.to_string(), pos: e["pos"].as_f64().unwrap_or(0.0), dur: e["dur"].as_f64().unwrap_or(0.0), t: e["t"].as_f64().unwrap_or(0.0) }))
                    .collect();
            }
            if let Some(q) = v["queue"].as_array() {
                p.saved_queue = q.iter().filter_map(|e| e.as_str().map(String::from)).filter(|e| is_url(e) || std::path::Path::new(e).exists()).collect();
                p.saved_idx = (v["queue_idx"].as_u64().unwrap_or(0) as usize).min(p.saved_queue.len().saturating_sub(1));
            }
        }
        p
    }
    // no settings (the offline --png renders)
    fn bare() -> Player {
        Player {
            eng: None,
            err: String::new(),
            no_libs: false,
            mpris: None,
            tx: None,
            display: std::ptr::null_mut(),
            win: None,
            w: 1280.0,
            h: 760.0,
            queue: vec![],
            queue_scroll: 0,
            idx: -1,
            cur: String::new(),
            repeat: 0,
            shuffle: false,
            panel: Panel::None,
            remaining: false,
            pos: 0.0,
            dur: 0.0,
            paused: true,
            video: false,
            speed: 1.0,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            tracks: vec![],
            chapters: vec![],
            volume: 0.8,
            muted: false,
            recents: vec![],
            pending_resume: 0.0,
            pending_load: false,
            fs: false,
            ui_until: Instant::now() + UI_HOLD,
            hits: vec![],
            hover: None,
            mouse: (0.0, 0.0),
            osd: None,
            flash: None,
            resume_at: None,
            drag: None,
            seek_preview: None,
            drop_hover: false,
            click_at: None,
            text: Text::new(),
            shown_sec: -1,
            save_at: None,
            spot_at: Instant::now(),
            rows: vec![],
            paints: 0,
            paint_us: 0,
            backdrop: None,
            cover_for: String::new(),
            cover: None,
            cover_cols: None,
            cover_px: None,
            png_req: None,
            launched: Instant::now(),
            first_frame_ms: None,
            title_shown: String::new(),
            thumbs: thumbs::Thumbs::new(),
            saved_queue: vec![],
            saved_idx: 0,
            pip: None,
            pip_token: 0,
            pip_closing: false,
            video_win: None,
            pip_size: (480.0, 270.0),
            pip_hover: false,
            pip_hits: vec![],
            pip_mouse: (-1.0, -1.0),
        }
    }
    fn save(&self) {
        let r: Vec<serde_json::Value> = self.recents.iter().map(|e| serde_json::json!({ "path": e.path, "pos": e.pos, "dur": e.dur, "t": e.t })).collect();
        // the queue playing now, else the one kept from before (closing a file does not forget it)
        let (q, qi) = if self.queue.len() > 1 { (&self.queue, self.idx.max(0) as usize) } else { (&self.saved_queue, self.saved_idx) };
        let v = serde_json::json!({ "volume": self.volume, "muted": self.muted, "recents": r, "queue": q, "queue_idx": qi });
        let _ = std::fs::create_dir_all(std::path::Path::new(&cfg_path()).parent().unwrap());
        let _ = std::fs::write(cfg_path(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    fn remember(&mut self, path: &str, pos_s: f64, dur_s: f64) {
        let (pos, dur) = (pos_s * 1000.0, dur_s * 1000.0);
        self.recents.retain(|e| e.path != path);
        self.recents.insert(0, Recent { path: path.into(), pos: resume_spot(pos, dur), dur: dur.floor(), t: now_ms() });
        self.recents.truncate(30);
        self.save();
    }
    fn resume_of(&self, path: &str) -> f64 {
        self.recents.iter().find(|e| e.path == path).map(|e| e.pos / 1000.0).unwrap_or(0.0)
    }

    // ------------------------------------------------------------ engine
    fn cmd(&self, args: &[&str]) {
        if let Some(e) = &self.eng {
            e.cmd(args);
        }
    }
    fn set(&self, k: &str, v: &str) {
        if let Some(e) = &self.eng {
            e.set(k, v);
        }
    }
    fn get_str(&self, k: &str) -> Option<String> {
        self.eng.as_ref()?.get_str(k)
    }
    // the position now (time-pos is not observed while the controls are hidden)
    fn now_pos(&self) -> f64 {
        self.eng.as_ref().and_then(|e| e.get_f64("time-pos")).unwrap_or(self.pos)
    }
    fn of_kind(&self, k: &str) -> Vec<Track> {
        self.tracks.iter().filter(|t| t.kind == k).cloned().collect()
    }
    // libmpv is loaded (dlopen), started (and MPRIS with it) for the first file: an empty window maps no
    // mpv, ffmpeg or GL driver
    fn engine(&mut self) -> bool {
        if self.eng.is_some() {
            return true;
        }
        let Some(tx) = self.tx.clone() else { return false };
        trace("load libmpv");
        let libs = match ffi::load() {
            Ok(l) => Box::new(l),
            Err(e) => {
                self.err = e;
                self.no_libs = true;
                return false;
            }
        };
        match Engine::new(libs, tx.clone(), self.volume, self.muted) {
            Ok(e) => {
                trace("mpv up");
                self.eng = Some(e);
                if std::env::var_os("MIST_PLAYER_NO_MPRIS").is_none() {
                    self.mpris = Some(mpris::Mpris::start(tx));
                }
                true
            }
            Err(e) => {
                self.err = e;
                false
            }
        }
    }

    fn mpv_events(&mut self, cx: &mut Cx) {
        let Some(e) = self.eng.as_mut() else { return };
        let evs = e.events();
        let mut redraw = false;
        for ev in evs {
            match ev {
                Event::Pos(p) => {
                    self.pos = p;
                    // the clock and the seek bar move once a second
                    if self.chrome() && p as i64 != self.shown_sec {
                        redraw = true;
                    }
                }
                Event::Dur(d) => {
                    self.dur = d;
                    redraw = true;
                }
                Event::Pause(p) => {
                    self.paused = p;
                    self.inhibit(cx);
                    self.spot();
                    redraw = true;
                }
                Event::Eof => {
                    if !self.cur.is_empty() {
                        self.pos = self.dur;
                        self.save_spot_at(self.dur);
                        self.next(true, cx);
                        redraw = true;
                    }
                }
                Event::Video(v) => {
                    trace("vo-configured");
                    self.video = v;
                    self.inhibit(cx);
                    redraw = true;
                }
                Event::Speed(s) => {
                    self.speed = s;
                    self.spot();
                    redraw = true;
                }
                Event::Title(t) => {
                    self.title = t;
                    redraw = true;
                }
                Event::Tracks(t) => {
                    self.tracks = t;
                    redraw = true;
                }
                Event::Chapters(c) => {
                    self.chapters = c;
                    redraw = true;
                }
                Event::Meta(a, b) => {
                    self.artist = a;
                    self.album = b;
                    redraw = true;
                }
                Event::Reply(id, p) => {
                    self.pos = p;
                    if id == R_SPOT {
                        self.spot();
                        if let Some(m) = &self.mpris {
                            m.seeked(p);
                        }
                    } else if id == R_SAVE {
                        self.save_spot_at(p);
                    }
                }
                Event::Volume(v) => {
                    if (v - self.volume).abs() > 0.001 {
                        self.volume = v;
                    }
                }
                Event::Mute(m) => self.muted = m,
                Event::Loaded => {
                    trace("file loaded");
                    let p = std::mem::take(&mut self.pending_resume);
                    if p > 5.0 && (self.dur <= 0.0 || p < self.dur - 15.0) {
                        self.cmd(&["seek", &format!("{p}"), "absolute"]);
                        self.resume_at = Some((p, Instant::now() + Duration::from_secs(6)));
                        cx.wake_at(Instant::now() + Duration::from_secs(6));
                    }
                    redraw = true;
                }
                Event::Restart => {
                    // after a seek or a load: the clock and MPRIS start from where it answers
                    if let Some(e) = &self.eng {
                        e.ask_pos(R_SPOT);
                    }
                }
                Event::Failed(m) => {
                    self.say("x", &format!("Nie można odtworzyć: {m}"), cx);
                    redraw = true;
                }
            }
        }
        if redraw {
            self.redraw(cx);
        }
    }
    // MPRIS's Position counts on from here
    fn spot(&self) {
        if let Some(m) = &self.mpris {
            m.spot(self.pos, !self.paused && !self.cur.is_empty(), self.speed);
        }
    }

    // ------------------------------------------------------------ playback
    fn redraw(&self, cx: &mut Cx) {
        if let Some(w) = self.win {
            cx.redraw(w);
        }
    }
    pub fn chrome(&self) -> bool {
        Instant::now() < self.ui_until || self.paused || self.panel != Panel::None || self.cur.is_empty() || !self.video || self.pip.is_some() || self.drag.is_some() || matches!(self.hover, Some(ref h) if *h != Hit::Picture && *h != Hit::Drag)
    }
    fn poke(&mut self, cx: &mut Cx) {
        let was = self.chrome();
        self.ui_until = Instant::now() + UI_HOLD;
        cx.wake_at(self.ui_until);
        cx.cmds.push(Cmd::HideCursor(false));
        if !was {
            self.redraw(cx);
        }
    }
    pub fn say(&mut self, g: &'static str, t: &str, cx: &mut Cx) {
        let until = Instant::now() + Duration::from_millis(1100);
        self.osd = Some((g, t.into(), until));
        cx.wake_at(until);
        self.redraw(cx);
    }
    fn flash(&mut self, g: &'static str, cx: &mut Cx) {
        self.flash = Some((g, Instant::now()));
        if let Some(w) = self.win {
            cx.cmds.push(Cmd::Animate(w, true));
        }
        cx.wake_at(Instant::now() + Duration::from_millis(660));
    }
    fn inhibit(&self, cx: &mut Cx) {
        if let Some(w) = self.win {
            cx.cmds.push(Cmd::IdleInhibit(w, self.video && !self.paused && !self.cur.is_empty()));
        }
    }
    pub fn open(&mut self, paths: Vec<String>, cx: &mut Cx) {
        let paths: Vec<String> = paths.into_iter().filter(|p| is_url(p) || std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)).collect();
        if paths.is_empty() {
            return;
        }
        if paths.len() == 1 && !is_url(&paths[0]) {
            let (q, i) = folder_queue(&paths[0]);
            self.queue = q;
            self.load(i, cx);
        } else {
            self.queue = paths;
            self.load(0, cx);
        }
    }
    fn add(&mut self, paths: Vec<String>, cx: &mut Cx) {
        let paths: Vec<String> = paths.into_iter().filter(|p| !self.queue.contains(p) && (is_url(p) || std::fs::metadata(p).is_ok())).collect();
        if paths.is_empty() {
            return;
        }
        if self.cur.is_empty() {
            self.open(paths, cx);
            return;
        }
        let n = paths.len();
        self.queue.extend(paths);
        self.say("plus", &if n == 1 { "Dodano do kolejki".into() } else { format!("Dodano {n} pliki") }, cx);
    }
    fn queue_rows(&self) -> usize {
        let (_, cy, _, _) = self.controls_rect();
        let sh = (cy - 12.0 - 64.0).max(120.0);
        ((sh - 68.0) / 46.0).floor().max(1.0) as usize
    }
    fn load(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.queue.len() || !self.engine() {
            self.redraw(cx);
            return;
        }
        self.save_spot();
        self.idx = i as i32;
        self.cur = self.queue[i].clone();
        let thumb_src = if is_url(&self.cur) || is_audio(&self.cur) { String::new() } else { self.cur.clone() };
        self.thumbs.reset(&thumb_src);
        self.want_cover();
        // the backdrop takes the track's colours
        self.backdrop = None;
        if self.queue.len() > 1 {
            self.saved_queue = self.queue.clone();
            self.saved_idx = i;
            self.save();
        }
        self.pending_resume = self.resume_of(&self.cur.clone());
        self.title.clear();
        self.artist.clear();
        self.album.clear();
        self.tracks.clear();
        self.chapters.clear();
        self.pos = 0.0;
        self.dur = 0.0;
        // video after video: mpv keeps its VO configured and sends no vo-configured change, so the plane stays
        self.video = self.video && !is_audio(&self.cur);
        self.resume_at = None;
        let vis = self.queue_rows();
        if i < self.queue_scroll {
            self.queue_scroll = i;
        } else if i >= self.queue_scroll + vis {
            self.queue_scroll = i + 1 - vis;
        }
        let picture = !is_audio(&self.cur);
        if !picture {
            self.close_pip(cx);
        }
        let display = self.display;
        let e = self.eng.as_mut().unwrap();
        if picture {
            if let Err(m) = e.gl_open(display) {
                self.err = m;
            }
            trace("gl up");
        } else {
            // audio: no GL plane (the render context, CUDA and EGL go: ~250 MB on NVIDIA)
            e.gl_close();
        }
        let e = self.eng.as_ref().unwrap();
        if picture && !e.gl_ready() {
            // EGL without surfaceless contexts: the render context comes with the plane
            self.pending_load = true;
        } else {
            self.pending_load = false;
            e.cmd(&["loadfile", &self.cur, "replace"]);
            e.set("pause", "no");
            trace("loadfile");
        }
        self.spot_at = Instant::now();
        self.spot();
        self.poke(cx);
        self.redraw(cx);
    }
    fn next(&mut self, auto: bool, cx: &mut Cx) {
        if self.queue.is_empty() {
            return;
        }
        if auto && self.repeat == 2 {
            self.cmd(&["seek", "0", "absolute"]);
            self.set("pause", "no");
            return;
        }
        let n = self.queue.len();
        if self.shuffle && n > 1 {
            let mut j = self.idx as usize;
            let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as usize).unwrap_or(7);
            while j == self.idx as usize {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                j = (seed >> 8) % n;
            }
            self.load(j, cx);
            return;
        }
        if (self.idx as usize) + 1 < n {
            self.load(self.idx as usize + 1, cx);
        } else if self.repeat == 1 || !auto {
            self.load(0, cx);
        } else {
            self.set("pause", "yes");
            self.poke(cx);
        }
    }
    fn prev(&mut self, cx: &mut Cx) {
        if self.now_pos() > 4.0 || self.queue.len() < 2 {
            self.cmd(&["seek", "0", "absolute"]);
            return;
        }
        let i = if self.idx > 0 { self.idx as usize - 1 } else { self.queue.len() - 1 };
        self.load(i, cx);
    }
    fn remove_at(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.queue.len() {
            return;
        }
        if i as i32 == self.idx && self.queue.len() == 1 {
            self.close_file(cx);
            return;
        }
        self.queue.remove(i);
        if (i as i32) < self.idx {
            self.idx -= 1;
        } else if i as i32 == self.idx {
            let j = (self.idx as usize).min(self.queue.len() - 1);
            self.load(j, cx);
        }
        self.redraw(cx);
    }
    fn close_file(&mut self, cx: &mut Cx) {
        self.save_spot();
        self.cmd(&["stop"]);
        if let Some(e) = self.eng.as_mut() {
            e.gl_close();
        }
        self.close_pip(cx);
        self.cur.clear();
        self.queue.clear();
        self.thumbs.reset("");
        self.idx = -1;
        self.panel = Panel::None;
        self.video = false;
        self.inhibit(cx);
        self.redraw(cx);
    }
    // the resume spot of the file now playing (a direct read: called before a new file, not in mpv's events)
    fn save_spot(&mut self) {
        let p = self.now_pos();
        self.save_spot_at(p);
    }
    fn save_spot_at(&mut self, p: f64) {
        if !self.cur.is_empty() && !is_url(&self.cur) && self.dur > 0.0 {
            let (c, d) = (self.cur.clone(), self.dur);
            self.remember(&c, p, d);
        }
        self.spot_at = Instant::now();
    }
    fn toggle(&mut self, cx: &mut Cx) {
        if self.cur.is_empty() {
            self.pick(false);
            return;
        }
        self.cmd(&["cycle", "pause"]);
        self.flash(if self.paused { "play" } else { "pause" }, cx);
    }
    fn seek_by(&mut self, s: f64, cx: &mut Cx) {
        if self.cur.is_empty() {
            return;
        }
        self.cmd(&["seek", &format!("{s}"), "relative+exact"]);
        self.say(if s > 0.0 { "fwd10" } else { "back10" }, &format!("{}{} s", if s > 0.0 { "+" } else { "−" }, s.abs()), cx);
    }
    fn set_vol(&mut self, v: f64, cx: &mut Cx) {
        self.volume = v.clamp(0.0, 1.0);
        self.muted = false;
        self.set("volume", &format!("{}", self.volume * 100.0));
        self.set("mute", "no");
        let g = self.vol_glyph();
        self.say(g, &format!("Głośność {}%", (self.volume * 100.0).round()), cx);
        self.save_at = Some(Instant::now() + Duration::from_millis(600));
        cx.wake_at(Instant::now() + Duration::from_millis(600));
    }
    fn toggle_mute(&mut self, cx: &mut Cx) {
        self.muted = !self.muted;
        self.set("mute", if self.muted { "yes" } else { "no" });
        let g = self.vol_glyph();
        self.say(g, &if self.muted { "Wyciszono".into() } else { format!("Głośność {}%", (self.volume * 100.0).round()) }, cx);
        self.save();
    }
    pub fn vol_glyph(&self) -> &'static str {
        if self.muted || self.volume == 0.0 {
            "vol0"
        } else if self.volume < 0.5 {
            "vol1"
        } else {
            "vol2"
        }
    }
    fn set_speed(&mut self, r: f64, cx: &mut Cx) {
        let r = r.clamp(0.25, 3.0);
        self.set("speed", &format!("{r}"));
        self.say("clock", &if r == 1.0 { "Normalna prędkość".into() } else { rate(r) }, cx);
    }
    fn step_speed(&mut self, d: i32, cx: &mut Cx) {
        let i = SPEEDS.iter().position(|s| (s - self.speed).abs() < 0.01).unwrap_or(3) as i32;
        let r = SPEEDS[(i + d).clamp(0, SPEEDS.len() as i32 - 1) as usize];
        self.set_speed(r, cx);
    }
    fn cycle_subs(&mut self, cx: &mut Cx) {
        let subs = self.of_kind("sub");
        if subs.is_empty() {
            self.say("cc", "Brak napisów", cx);
            return;
        }
        let next = match subs.iter().position(|t| t.selected) {
            Some(i) if i + 1 >= subs.len() => None,
            Some(i) => Some(i + 1),
            None => Some(0),
        };
        match next {
            None => {
                self.set("sid", "no");
                self.say("cc", "Napisy wyłączone", cx);
            }
            Some(i) => {
                self.set("sid", &subs[i].id.to_string());
                self.say("cc", &track_name(&subs[i], i, "Napisy"), cx);
            }
        }
    }
    fn cycle_audio(&mut self, cx: &mut Cx) {
        let a = self.of_kind("audio");
        if a.len() < 2 {
            self.say("head", "Jedna ścieżka audio", cx);
            return;
        }
        let i = (a.iter().position(|t| t.selected).unwrap_or(0) + 1) % a.len();
        self.set("aid", &a[i].id.to_string());
        self.say("head", &track_name(&a[i], i, "Ścieżka"), cx);
    }
    fn sub_delay(&mut self, d: f64, cx: &mut Cx) {
        let v = self.eng.as_ref().and_then(|e| e.get_f64("sub-delay")).unwrap_or(0.0) + d;
        let v = (v * 10.0).round() / 10.0;
        self.set("sub-delay", &format!("{v}"));
        self.say("cc", &format!("Opóźnienie napisów {} s", format!("{v:+.1}").replace('.', crate::i18n::dec_sep())), cx);
    }
    fn toggle_sub_visible(&mut self, cx: &mut Cx) {
        let on = self.get_str("sub-visibility").as_deref() != Some("no");
        self.set("sub-visibility", if on { "no" } else { "yes" });
        self.say("cc", if on { "Napisy ukryte" } else { "Napisy widoczne" }, cx);
    }
    fn toggle_fs(&mut self, cx: &mut Cx) {
        if let Some(w) = self.win {
            cx.cmds.push(Cmd::Fullscreen(w, !self.fs));
        }
    }
    // Picture-in-Picture: a small pinned window takes the picture; again (or its own buttons) brings it back
    fn toggle_pip(&mut self, cx: &mut Cx) {
        if self.pip.is_some() {
            return self.close_pip(cx);
        }
        if !self.video || self.cur.is_empty() || self.pip_token != 0 {
            self.say("pip", "Obraz w obrazie: tylko dla wideo", cx);
            return;
        }
        if self.fs {
            self.toggle_fs(cx);
        }
        self.pip_token = cx.new_window("Picture-in-Picture — Mist Player", 480, 270, (240, 135));
        self.panel = Panel::None;
        self.redraw(cx);
    }
    // the plane goes back to the main window first (both redraw: the PiP plane goes, the main one comes), then
    // the window closes (video_surface None); a PiP window that never got the plane closes at once
    fn close_pip(&mut self, cx: &mut Cx) {
        let Some(p) = self.pip else { return };
        if self.video_win != Some(p) {
            self.pip = None;
            self.pip_closing = false;
            cx.cmds.push(Cmd::Close(p));
        } else {
            self.pip_closing = true;
            cx.redraw(p);
        }
        self.pip_hover = false;
        self.redraw(cx);
    }
    fn resume_queue(&mut self, cx: &mut Cx) {
        if self.saved_queue.is_empty() {
            return;
        }
        let i = self.saved_idx.min(self.saved_queue.len() - 1);
        self.queue = self.saved_queue.clone();
        self.load(i, cx);
    }
    // the chapter now playing (index into chapters)
    pub fn chapter_now(&self) -> Option<usize> {
        self.chapters.iter().rposition(|(t, _)| *t <= self.pos + 0.25)
    }
    fn set_panel(&mut self, p: Panel, cx: &mut Cx) {
        self.panel = if self.panel == p { Panel::None } else { p };
        if self.panel == Panel::List {
            let vis = self.queue_rows();
            self.queue_scroll = if (self.idx.max(0) as usize) >= vis { self.idx as usize + 1 - vis } else { 0 };
        }
        self.redraw(cx);
    }
    fn add_subs(&mut self, paths: &[String], cx: &mut Cx) {
        for p in paths {
            self.cmd(&["sub-add", p, "select"]);
        }
        self.say("cc", if paths.len() == 1 { "Dodano napisy" } else { "Dodano napisy (kilka plików)" }, cx);
    }

    // "Open…" through the FileChooser portal (that is Mist Files); the answer comes back as Wake::Picked
    fn pick(&self, add: bool) {
        let Some(tx) = self.tx.clone() else { return };
        std::thread::spawn(move || {
            let r = (|| -> zbus::Result<Vec<String>> {
                use std::collections::HashMap;
                use zbus::zvariant::{OwnedValue, Value};
                let conn = zbus::blocking::Connection::session()?;
                let me = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
                let token = format!("mistplayer{}", std::process::id());
                let path = format!("/org/freedesktop/portal/desktop/request/{me}/{token}");
                let req = zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", path.as_str(), "org.freedesktop.portal.Request")?;
                let mut sig = req.receive_signal("Response")?;
                let pats = |v: &[&str]| -> Vec<(u32, String)> { v.iter().map(|e| (0u32, format!("*.{e}"))).collect() };
                let all: Vec<&str> = VIDEO_EXT.iter().chain(AUDIO_EXT.iter()).copied().collect();
                let filters: Vec<(String, Vec<(u32, String)>)> = vec![("Multimedia".into(), pats(&all)), ("Wideo".into(), pats(&VIDEO_EXT)), ("Audio".into(), pats(&AUDIO_EXT)), ("Napisy".into(), pats(&SUB_EXT)), ("Playlisty".into(), pats(&["m3u", "m3u8"])), ("Wszystkie pliki".into(), vec![(0, "*".into())])];
                let home = std::env::var("HOME").unwrap_or_default();
                let mut folder = format!("{home}/Wideo").into_bytes();
                folder.push(0);
                let mut o: HashMap<&str, Value> = HashMap::new();
                o.insert("handle_token", Value::from(token.clone()));
                o.insert("multiple", Value::from(true));
                o.insert("filters", Value::from(filters));
                o.insert("current_folder", Value::from(folder));
                let fc = zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser")?;
                let _: zbus::zvariant::OwnedObjectPath = fc.call("OpenFile", &("", if add { "Dodaj do kolejki" } else { "Otwórz multimedia" }, o))?;
                let Some(m) = sig.next() else { return Ok(vec![]) };
                let (code, res): (u32, HashMap<String, OwnedValue>) = m.body().deserialize()?;
                if code != 0 {
                    return Ok(vec![]);
                }
                let uris: Vec<String> = res.get("uris").and_then(|v| Vec::<String>::try_from(v.clone()).ok()).unwrap_or_default();
                Ok(uris.iter().map(|u| appwin::uri_to_path(u)).collect())
            })();
            match r {
                Ok(v) if !v.is_empty() => {
                    let _ = tx.send(Box::new(Wake::Picked(v, add)));
                }
                Err(e) => eprintln!("player: portal: {e}"),
                _ => {}
            }
        });
    }
    // files from anywhere (portal, drop, MPRIS OpenUri): subtitles go onto the playing file
    fn take(&mut self, paths: Vec<String>, add: bool, cx: &mut Cx) {
        // a playlist opens as its entries
        let paths: Vec<String> = paths.into_iter().flat_map(|p| if !is_url(&p) && matches!(ext(&p).as_str(), "m3u" | "m3u8") { read_m3u(&p) } else { vec![p] }).collect();
        let (subs, media): (Vec<String>, Vec<String>) = paths.into_iter().partition(|p| SUB_EXT.contains(&ext(p).as_str()));
        if !subs.is_empty() && !self.cur.is_empty() {
            self.add_subs(&subs, cx);
        }
        if media.is_empty() {
            return;
        }
        if add { self.add(media, cx) } else { self.open(media, cx) }
    }

    fn mpris_req(&mut self, r: mpris::Req, cx: &mut Cx) {
        use mpris::Req;
        match r {
            Req::Raise => raise(),
            Req::Quit => self.quit(cx),
            Req::PlayPause => self.toggle(cx),
            Req::Play if self.paused && !self.cur.is_empty() => self.set("pause", "no"),
            Req::Pause | Req::Stop if !self.paused => self.set("pause", "yes"),
            Req::Next => self.next(false, cx),
            Req::Prev => self.prev(cx),
            Req::Seek(s) => self.cmd(&["seek", &format!("{s}"), "relative+exact"]),
            Req::SetPosition(p) => self.cmd(&["seek", &format!("{p}"), "absolute"]),
            Req::Open(u) => {
                let p = if u.starts_with("file://") { appwin::uri_to_path(&u) } else { u };
                self.take(vec![p], false, cx);
            }
            Req::Volume(v) => self.set_vol(v, cx),
            Req::Rate(r) => self.set_speed(r, cx),
            Req::Loop(l) => self.repeat = l,
            Req::Shuffle(s) => self.shuffle = s,
            _ => {}
        }
        self.redraw(cx);
    }

    // after every entry point: time-pos only while shown, the bus and the title up to date
    fn settle(&mut self, cx: &mut Cx) {
        let show = self.chrome() && !self.cur.is_empty();
        if let Some(e) = self.eng.as_mut() {
            e.watch_pos(show);
        }
        if self.mpris.is_some() {
            let status = if self.cur.is_empty() { "Stopped" } else if self.paused { "Paused" } else { "Playing" };
            let url = if self.cur.is_empty() || is_url(&self.cur) { self.cur.clone() } else { appwin::path_to_uri(&self.cur) };
            let n = self.queue.len();
            let snap = mpris::Snap {
                status,
                title: if self.cur.is_empty() { String::new() } else { self.nice_title() },
                artist: self.artist.clone(),
                album: self.album.clone(),
                url,
                length: self.dur,
                track: self.idx,
                volume: if self.muted { 0.0 } else { self.volume },
                rate: self.speed,
                repeat: self.repeat,
                shuffle: self.shuffle,
                can_next: n > 1,
                can_prev: n > 0,
            };
            if let Some(m) = self.mpris.as_mut() {
                m.update(snap);
            }
        }
        if let Some(w) = self.win {
            let t = if self.cur.is_empty() { "Odtwarzacz — Mist Player".to_string() } else { format!("Odtwarzacz — {}", self.nice_title()) };
            if t != self.title_shown {
                self.title_shown = t.clone();
                cx.cmds.push(Cmd::Title(w, t));
            }
        }
    }
    // what the audio view shows: (title, artist), the artist taken from the name when there is no tag
    pub fn track_meta(&self) -> (String, String) {
        let t = self.nice_title();
        if !self.artist.is_empty() {
            return (t, self.artist.clone());
        }
        match split_name(&t) {
            Some((a, t)) => (t, a),
            None => (t, String::new()),
        }
    }
    pub fn nice_title(&self) -> String {
        if self.title.chars().count() > 1 && self.title != self.cur.rsplit('/').next().unwrap_or("") { self.title.clone() } else { pretty(&self.cur) }
    }
}

fn raise() {
    appwin::hypr("hl.dispatch(hl.dsp.focus({ window = \"class:^(mist-player)$\" }))");
}

// ---------------------------------------------------------------- input
impl Player {
    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|(hx, hy, hw, hh, _)| x >= *hx && x < hx + hw && y >= *hy && y < hy + hh).map(|h| h.4.clone())
    }
    fn bar_frac(&self, which: &Hit, x: f32) -> f64 {
        match self.hits.iter().rev().find(|h| &h.4 == which) {
            Some((bx, _, bw, _, _)) => ((x - bx - 6.0) / (bw - 12.0).max(1.0)).clamp(0.0, 1.0) as f64,
            None => 0.0,
        }
    }
    fn key(&mut self, k: Keysym, utf8: Option<String>, cx: &mut Cx) {
        let m = cx.mods;
        self.poke(cx);
        let ch = utf8.as_deref().and_then(|s| s.chars().next());
        // the characters first (layout-independent: [ ] { } < > # ? with or without Shift)
        match ch {
            Some('[') => return self.step_speed(-1, cx),
            Some(']') => return self.step_speed(1, cx),
            Some('{') => return self.set_speed(self.speed / 2.0, cx),
            Some('}') => return self.set_speed(self.speed * 2.0, cx),
            Some('<') => return self.prev(cx),
            Some('>') => return self.next(false, cx),
            Some('#') => return self.cycle_audio(cx),
            Some('?') => return self.set_panel(Panel::Help, cx),
            Some(',') => return self.cmd(&["frame-back-step"]),
            Some('.') => return self.cmd(&["frame-step"]),
            _ => {}
        }
        match k {
            Keysym::w | Keysym::W | Keysym::q | Keysym::Q if m.ctrl => self.quit(cx),
            Keysym::o | Keysym::O if m.ctrl => self.pick(false),
            Keysym::space | Keysym::k | Keysym::K | Keysym::XF86_AudioPlay | Keysym::XF86_AudioPause => self.toggle(cx),
            Keysym::Left => self.seek_by(if m.shift { -30.0 } else { -5.0 }, cx),
            Keysym::Right => self.seek_by(if m.shift { 30.0 } else { 5.0 }, cx),
            Keysym::j | Keysym::J => self.seek_by(-10.0, cx),
            Keysym::l | Keysym::L => self.seek_by(10.0, cx),
            Keysym::Up => self.set_vol(self.volume + 0.05, cx),
            Keysym::Down => self.set_vol(self.volume - 0.05, cx),
            Keysym::m | Keysym::M => self.toggle_mute(cx),
            Keysym::f | Keysym::F | Keysym::F11 => self.toggle_fs(cx),
            Keysym::Escape => {
                if self.panel != Panel::None {
                    self.panel = Panel::None;
                    self.redraw(cx);
                } else if self.fs {
                    self.toggle_fs(cx);
                }
            }
            Keysym::Prior if !self.chapters.is_empty() => self.cmd(&["add", "chapter", "-1"]),
            Keysym::Next if !self.chapters.is_empty() => self.cmd(&["add", "chapter", "1"]),
            Keysym::n | Keysym::N | Keysym::XF86_AudioNext => self.next(false, cx),
            Keysym::p | Keysym::P if m.shift => self.toggle_pip(cx),
            Keysym::p | Keysym::P | Keysym::XF86_AudioPrev => self.prev(cx),
            Keysym::c | Keysym::C if !self.chapters.is_empty() => self.set_panel(Panel::Chapters, cx),
            Keysym::BackSpace => self.set_speed(1.0, cx),
            Keysym::s | Keysym::S if m.shift => {
                self.cmd(&["screenshot", "video"]);
                self.say("crop", "Zrzut klatki w Obrazach", cx);
            }
            Keysym::s | Keysym::S => self.cycle_subs(cx),
            Keysym::v | Keysym::V => self.toggle_sub_visible(cx),
            Keysym::z | Keysym::Z => self.sub_delay(-0.1, cx),
            Keysym::x | Keysym::X => self.sub_delay(0.1, cx),
            Keysym::a | Keysym::A => self.cycle_audio(cx),
            Keysym::q | Keysym::Q => self.set_panel(Panel::List, cx),
            Keysym::o | Keysym::O => self.pick(false),
            Keysym::h | Keysym::H => self.set_panel(Panel::Help, cx),
            Keysym::i | Keysym::I => self.set_panel(Panel::Info, cx),
            Keysym::Home => self.cmd(&["seek", "0", "absolute"]),
            Keysym::End => self.cmd(&["seek", "-1", "absolute+exact"]),
            Keysym::w | Keysym::W => {
                let fill = self.get_str("panscan").map(|v| v.parse::<f64>().unwrap_or(0.0) > 0.5).unwrap_or(false);
                self.set("panscan", if fill { "0" } else { "1" });
                self.say("crop", if fill { "Cały obraz" } else { "Wypełnij okno" }, cx);
            }
            _ => {
                if let Some(c) = ch.filter(|c| c.is_ascii_digit()) {
                    let pc = (c as u8 - b'0') as f64 * 10.0;
                    self.cmd(&["seek", &format!("{pc}"), "absolute-percent"]);
                }
            }
        }
    }
    fn quit(&mut self, cx: &mut Cx) {
        self.save_spot();
        if let Some(p) = self.pip.take() {
            cx.cmds.push(Cmd::Close(p));
        }
        if let Some(w) = self.win.take() {
            cx.cmds.push(Cmd::Close(w));
        }
        if let Some(e) = self.eng.as_mut() {
            e.shutdown();
        }
        self.eng = None;
        cx.cmds.push(Cmd::Exit);
    }
    fn press(&mut self, x: f32, y: f32, b: u32, clicks: u8, cx: &mut Cx) {
        self.poke(cx);
        let h = self.hit_at(x, y);
        // a click outside an open panel closes it
        let in_panel = matches!(h, Some(Hit::Item(_)) | Some(Hit::Queue(_)) | Some(Hit::QueueRemove(_)) | Some(Hit::Shuffle) | Some(Hit::Repeat) | Some(Hit::Add) | Some(Hit::Card));
        let toggles = matches!(h, Some(Hit::Speed) | Some(Hit::Tracks) | Some(Hit::List) | Some(Hit::Help) | Some(Hit::Chapters));
        if self.panel != Panel::None && !in_panel && !toggles {
            self.panel = Panel::None;
            self.redraw(cx);
            if matches!(h, Some(Hit::Picture) | None) {
                return;
            }
        }
        let Some(h) = h else { return };
        self.redraw(cx);
        if b == MIDDLE {
            self.toggle_mute(cx);
            return;
        }
        if b == RIGHT {
            if matches!(h, Hit::Picture) && !self.cur.is_empty() {
                self.panel = Panel::Tracks;
            }
            return;
        }
        if b != LEFT {
            return;
        }
        match h {
            Hit::Picture => {
                if clicks >= 2 {
                    self.click_at = None;
                    self.toggle_fs(cx);
                } else if !self.cur.is_empty() {
                    // a single click plays / pauses, unless a second one makes it a double click
                    let t = Instant::now() + Duration::from_millis(230);
                    self.click_at = Some(t);
                    cx.wake_at(t);
                }
            }
            Hit::Drag => {
                if clicks >= 2 {
                    if let Some(w) = self.win {
                        cx.cmds.push(Cmd::Maximize(w));
                    }
                } else if !self.fs
                    && let Some(w) = self.win
                {
                    cx.cmds.push(Cmd::Move(w));
                }
            }
            Hit::Light(0) => self.quit(cx),
            Hit::Light(1) => {
                if let Some(w) = self.win {
                    cx.cmds.push(Cmd::Minimize(w));
                }
            }
            Hit::Light(_) => {
                if let Some(w) = self.win {
                    cx.cmds.push(Cmd::Maximize(w));
                }
            }
            Hit::Open => self.pick(false),
            Hit::Help => self.set_panel(Panel::Help, cx),
            Hit::Play => self.toggle(cx),
            Hit::Prev => self.prev(cx),
            Hit::Next => self.next(false, cx),
            Hit::Back10 => self.seek_by(-10.0, cx),
            Hit::Fwd10 => self.seek_by(10.0, cx),
            Hit::Mute => self.toggle_mute(cx),
            Hit::Vol => {
                self.drag = Some(Hit::Vol);
                let v = self.bar_frac(&Hit::Vol, x);
                self.set_vol(v, cx);
            }
            Hit::Seek => {
                self.drag = Some(Hit::Seek);
                self.seek_preview = Some(self.bar_frac(&Hit::Seek, x));
            }
            Hit::Remaining => self.remaining = !self.remaining,
            Hit::Speed => self.set_panel(Panel::Speed, cx),
            Hit::Tracks => self.set_panel(Panel::Tracks, cx),
            Hit::List => self.set_panel(Panel::List, cx),
            Hit::Full => self.toggle_fs(cx),
            Hit::Item(i) => {
                if let Some(Row::Pick(_, _, a)) = self.rows.get(i).cloned() {
                    match a {
                        RowAct::Speed(r) => {
                            self.set_speed(r, cx);
                            self.panel = Panel::None;
                        }
                        RowAct::Aid(id) => self.set("aid", &id.to_string()),
                        RowAct::Sid(id) => self.set("sid", &if id < 0 { "no".into() } else { id.to_string() }),
                        RowAct::Chapter(t) => {
                            self.cmd(&["seek", &format!("{t}"), "absolute+exact"]);
                            self.pos = t;
                            self.panel = Panel::None;
                        }
                        RowAct::None => {}
                    }
                }
            }
            Hit::Queue(i) => self.load(i, cx),
            Hit::QueueRemove(i) => self.remove_at(i, cx),
            Hit::Shuffle => {
                self.shuffle = !self.shuffle;
                self.say("shuffle", if self.shuffle { "Losowo" } else { "Po kolei" }, cx);
            }
            Hit::Repeat => {
                self.repeat = (self.repeat + 1) % 3;
                let (g, t) = match self.repeat {
                    0 => ("repeat", "Bez powtarzania"),
                    1 => ("repeat", "Powtarzaj kolejkę"),
                    _ => ("repeat1", "Powtarzaj ten plik"),
                };
                self.say(g, t, cx);
            }
            Hit::Add => self.pick(true),
            Hit::Recent(i) => {
                if let Some(r) = self.recents.get(i).map(|r| r.path.clone()) {
                    self.open(vec![r], cx);
                }
            }
            Hit::RecentForget(i) => {
                if i < self.recents.len() {
                    self.recents.remove(i);
                    self.save();
                }
            }
            Hit::FromStart => {
                self.cmd(&["seek", "0", "absolute"]);
                self.resume_at = None;
            }
            Hit::Card => {}
            Hit::Chapters => self.set_panel(Panel::Chapters, cx),
            Hit::Pip => self.toggle_pip(cx),
            Hit::ResumeQueue => self.resume_queue(cx),
            Hit::PipPlay | Hit::PipBack | Hit::PipClose => {}
        }
    }
    // the PiP window: hover shows its buttons, a drag moves it, a double click (or ⤶) brings the picture back
    fn pip_event(&mut self, pid: WinId, ev: Ev, cx: &mut Cx) {
        let hit = |s: &Self, x: f32, y: f32| s.pip_hits.iter().rev().find(|(hx, hy, hw, hh, _)| x >= *hx && x < hx + hw && y >= *hy && y < hy + hh).map(|h| h.4.clone());
        match ev {
            Ev::Motion(x, y) => {
                let was = (self.pip_hover, hit(self, self.pip_mouse.0, self.pip_mouse.1));
                self.pip_mouse = (x, y);
                self.pip_hover = true;
                let h = hit(self, x, y);
                cx.cursor(if h.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
                if was != (true, h) {
                    cx.redraw(pid);
                }
            }
            Ev::Leave => {
                self.pip_hover = false;
                self.pip_mouse = (-1.0, -1.0);
                cx.redraw(pid);
            }
            Ev::Press(x, y, b, n) if b == LEFT => match hit(self, x, y) {
                Some(Hit::PipPlay) => {
                    self.toggle(cx);
                    cx.redraw(pid);
                }
                Some(Hit::PipBack) => self.close_pip(cx),
                Some(Hit::PipClose) => {
                    self.set("pause", "yes");
                    self.close_pip(cx);
                }
                _ if n >= 2 => self.close_pip(cx),
                _ => cx.cmds.push(Cmd::Move(pid)),
            },
            Ev::Key(Keysym::Escape, _) => self.close_pip(cx),
            Ev::Key(k, u) => {
                self.key(k, u, cx);
                cx.redraw(pid);
            }
            Ev::Scroll(_, _, dy) => self.set_vol(self.volume - dy as f64 / 15.0 * 0.05, cx),
            Ev::Resized => {
                if self.video_win == Some(pid) {
                    let (w, h) = (self.pip_size.0 as i32, self.pip_size.1 as i32);
                    if let Some(e) = self.eng.as_mut() {
                        e.resize(w, h);
                    }
                }
            }
            _ => {}
        }
        self.settle(cx);
    }
}

// ---------------------------------------------------------------- the app
impl Logic for Player {
    fn app_id(&self) -> &str {
        "mist-player"
    }
    // the backdrop is cached per size: drawn again in the new theme's colours
    fn theme_changed(&mut self, _cx: &mut Cx) {
        self.backdrop = None;
    }
    fn opened(&mut self, id: WinId, token: u64, cx: &mut Cx) {
        if token != 0 && token == self.pip_token {
            trace("pip window");
            self.pip_token = 0;
            self.pip = Some(id);
            cx.redraw(id);
            return;
        }
        trace("window");
        self.win = Some(id);
        self.inhibit(cx);
        cx.redraw(id);
    }
    fn paint(&mut self, id: WinId, c: &mut Canvas, _cx: &mut Cx) {
        if Some(id) == self.pip {
            self.pip_size = (c.w(), c.h());
            return self.paint_pip(c);
        }
        let t = Instant::now();
        self.w = c.w();
        self.h = c.h();
        self.paint_all(c);
        self.paints += 1;
        self.paint_us += t.elapsed().as_micros() as u64;
        if let Some(p) = self.png_req.take() {
            let _ = c.pm.save_png(p);
        }
    }
    fn video_rect(&mut self, id: WinId) -> Option<(i32, i32, i32, i32)> {
        // a plane only while GL is up (a file with a picture), in the PiP window while that is open
        self.eng.as_ref().filter(|e| e.gl_up())?;
        let in_pip = self.pip.is_some() && !self.pip_closing;
        if Some(id) == self.pip {
            in_pip.then_some((0, 0, self.pip_size.0 as i32, self.pip_size.1 as i32))
        } else {
            (!in_pip).then_some((0, 0, self.w as i32, self.h as i32))
        }
    }
    fn video_surface(&mut self, id: WinId, s: Option<VideoSurface>, cx: &mut Cx) {
        let is_pip = Some(id) == self.pip;
        let (w, h) = if is_pip { (self.pip_size.0 as i32, self.pip_size.1 as i32) } else { (self.w as i32, self.h as i32) };
        if s.is_none() && is_pip && self.pip_closing {
            self.pip = None;
            self.pip_closing = false;
            cx.cmds.push(Cmd::Close(id));
        }
        let Some(e) = self.eng.as_mut() else { return };
        match s {
            Some(vs) => {
                trace("video plane");
                // attach drops the EGL surface of the other window's plane first
                self.video_win = Some(id);
                e.size = (w, h);
                if let Err(m) = e.attach(vs) {
                    self.err = m;
                }
                if self.pending_load && e.gl_ready() {
                    self.pending_load = false;
                    e.cmd(&["loadfile", &self.cur, "replace"]);
                    e.set("pause", "no");
                }
                self.redraw(cx);
            }
            // the other window's plane going after this one took the picture: nothing to drop
            None if self.video_win == Some(id) => {
                self.video_win = None;
                e.detach();
            }
            None => {}
        }
    }
    fn video_frame(&mut self, _id: WinId, _cx: &mut Cx) {
        if let Some(e) = self.eng.as_mut() {
            e.frame_done();
            if self.first_frame_ms.is_none() && e.stat.frames > 0 && self.video {
                trace("first frame shown");
                self.first_frame_ms = Some(self.launched.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    fn event(&mut self, id: WinId, ev: Ev, cx: &mut Cx) {
        if Some(id) == self.pip {
            return self.pip_event(id, ev, cx);
        }
        match ev {
            Ev::Key(k, u) => self.key(k, u, cx),
            Ev::Motion(x, y) => {
                self.mouse = (x, y);
                let moved_ui = !self.chrome();
                self.poke(cx);
                match self.drag {
                    Some(Hit::Seek) => {
                        self.seek_preview = Some(self.bar_frac(&Hit::Seek, x));
                        self.redraw(cx);
                        return self.settle(cx);
                    }
                    Some(Hit::Vol) => {
                        let v = self.bar_frac(&Hit::Vol, x);
                        self.set_vol(v, cx);
                        return self.settle(cx);
                    }
                    _ => {}
                }
                let h = self.hit_at(x, y);
                if h != self.hover || moved_ui || matches!(h, Some(Hit::Seek)) {
                    self.hover = h.clone();
                    self.redraw(cx);
                }
                cx.cursor(match h {
                    Some(Hit::Picture) | Some(Hit::Drag) | None => CursorIcon::Default,
                    _ => CursorIcon::Pointer,
                });
            }
            Ev::Leave => {
                self.hover = None;
                self.redraw(cx);
            }
            Ev::Press(x, y, b, n) => self.press(x, y, b, n, cx),
            Ev::Release(x, _, _) => {
                if self.drag.take() == Some(Hit::Seek) {
                    let f = self.bar_frac(&Hit::Seek, x);
                    if self.dur > 0.0 {
                        self.cmd(&["seek", &format!("{}", f * self.dur), "absolute"]);
                        self.pos = f * self.dur;
                    }
                    self.seek_preview = None;
                }
                self.redraw(cx);
            }
            Ev::Scroll(_, _, dy) => {
                if self.panel == Panel::List {
                    let max_first = self.queue.len().saturating_sub(self.queue_rows());
                    if dy > 0.0 {
                        self.queue_scroll = (self.queue_scroll + 1).min(max_first);
                    } else if dy < 0.0 {
                        self.queue_scroll = self.queue_scroll.saturating_sub(1);
                    }
                    self.redraw(cx);
                } else if self.panel == Panel::None {
                    if matches!(self.hover, Some(Hit::Seek)) || cx.mods.shift {
                        self.seek_by(if dy > 0.0 { -5.0 } else { 5.0 }, cx);
                    } else {
                        self.set_vol(self.volume - dy as f64 / 15.0 * 0.05, cx);
                    }
                }
            }
            Ev::Resized => {
                let (w, h) = (self.w as i32, self.h as i32);
                if (self.pip.is_none() || self.pip_closing) && let Some(e) = self.eng.as_mut() {
                    e.resize(w, h);
                }
                self.redraw(cx);
            }
            Ev::Fullscreen(f) => {
                self.fs = f;
                self.redraw(cx);
            }
            Ev::DragMotion(..) => {
                if !self.drop_hover {
                    self.drop_hover = true;
                    self.redraw(cx);
                }
            }
            Ev::DragLeave => {
                self.drop_hover = false;
                self.redraw(cx);
            }
            Ev::Drop(_, _, uris) => {
                self.drop_hover = false;
                let paths: Vec<String> = uris.iter().filter(|u| u.starts_with("file://")).map(|u| appwin::uri_to_path(u)).collect();
                let add = !self.cur.is_empty() && cx.mods.shift;
                self.take(paths, add, cx);
                self.redraw(cx);
            }
            Ev::Focus(_) => {}
        }
        self.settle(cx);
    }
    fn close_requested(&mut self, id: WinId, cx: &mut Cx) {
        if Some(id) == self.pip {
            return self.close_pip(cx);
        }
        self.quit(cx);
    }
    fn tick(&mut self, cx: &mut Cx) {
        let now = Instant::now();
        if let Some(t) = self.click_at {
            if now >= t {
                self.click_at = None;
                self.toggle(cx);
            } else {
                cx.wake_at(t);
            }
        }
        if let Some((_, _, t)) = &self.osd {
            if now >= *t {
                self.osd = None;
                self.redraw(cx);
            } else {
                cx.wake_at(*t);
            }
        }
        if let Some((_, t)) = &self.resume_at
            && now >= *t
        {
            self.resume_at = None;
            self.redraw(cx);
        }
        if let Some((_, t0)) = &self.flash {
            if now.duration_since(*t0) >= Duration::from_millis(650) {
                self.flash = None;
                if let Some(w) = self.win {
                    cx.cmds.push(Cmd::Animate(w, false));
                }
                self.redraw(cx);
            } else {
                cx.wake_at(*t0 + Duration::from_millis(660));
            }
        }
        if let Some(t) = self.save_at {
            if now >= t {
                self.save_at = None;
                self.save();
            } else {
                cx.wake_at(t);
            }
        }
        // the controls hide after 2.6 s without input (a playing video); the pointer too
        if now >= self.ui_until {
            if !self.chrome() {
                self.redraw(cx);
                if self.video && !self.cur.is_empty() && self.pip.is_none() {
                    cx.cmds.push(Cmd::HideCursor(true));
                }
            }
        } else {
            cx.wake_at(self.ui_until);
        }
        // the resume spot every 10 s while playing (the position comes back as an event)
        if !self.paused && !self.cur.is_empty() {
            let due = self.spot_at + Duration::from_secs(10);
            if now >= due {
                self.spot_at = now;
                if let Some(e) = &self.eng {
                    e.ask_pos(R_SAVE);
                }
                cx.wake_at(now + Duration::from_secs(10));
            } else {
                cx.wake_at(due);
            }
        }
        self.settle(cx);
    }
    fn message(&mut self, args: Vec<String>, cx: &mut Cx) {
        if self.tx.is_none() {
            self.tx = Some(cx.tx.clone());
            self.display = cx.display;
        }
        if args.first().map(|s| s.as_str()) == Some("--test") {
            self.test(&args[1..], cx);
            return self.settle(cx);
        }
        if self.win.is_none() {
            cx.new_window("Odtwarzacz — Mist Player", 1280, 760, (640, 400));
        }
        let add = args.iter().any(|a| a == "--add");
        let paths: Vec<String> = args
            .iter()
            .filter(|a| !a.starts_with("--") && !a.is_empty())
            .map(|a| {
                if a.starts_with("file://") {
                    appwin::uri_to_path(a)
                } else if a.starts_with('/') || is_url(a) {
                    a.clone()
                } else {
                    format!("{}/{a}", std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default())
                }
            })
            .collect();
        if !paths.is_empty() {
            self.take(paths, add, cx);
            raise();
        }
        self.settle(cx);
    }
    fn async_msg(&mut self, m: Box<dyn Any + Send>, cx: &mut Cx) {
        let Ok(w) = m.downcast::<Wake>() else { return };
        match *w {
            // frames only: nothing else to settle
            Wake::Render => {
                if let Some(e) = self.eng.as_mut() {
                    e.on_update();
                }
                return;
            }
            Wake::Mpv => self.mpv_events(cx),
            Wake::Picked(v, add) => self.take(v, add, cx),
            Wake::Mpris(r) => self.mpris_req(r, cx),
            Wake::Cover(path, pm) => {
                if path == self.cover_for {
                    self.cover_cols = pm.as_ref().map(cover::palette);
                    self.cover = pm.map(std::rc::Rc::new);
                    self.cover_px = None;
                    self.backdrop = None;
                    self.redraw(cx);
                }
            }
            Wake::Thumb(path, b, px) => {
                let tx = self.tx.clone();
                if self.thumbs.done(&path, b, px, tx.as_ref()) && (matches!(self.hover, Some(Hit::Seek)) || matches!(self.drag, Some(Hit::Seek))) {
                    self.redraw(cx);
                }
            }
        }
        self.settle(cx);
    }
}

impl Player {
    // the album art of a new audio track, found on a worker thread (Wake::Cover); nothing for video / URLs
    fn want_cover(&mut self) {
        let want = if is_audio(&self.cur) && !is_url(&self.cur) { self.cur.clone() } else { String::new() };
        if want == self.cover_for {
            return;
        }
        self.cover_for = want.clone();
        self.cover_px = None;
        if self.cover.take().is_some() || self.cover_cols.take().is_some() {
            self.backdrop = None;
        }
        let Some(tx) = self.tx.clone() else { return };
        if want.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let pm = cover::find(&want);
            let _ = tx.send(Box::new(Wake::Cover(want, pm)));
        });
    }

    // test hooks (running instance): key NAME [shift ctrl] | click X Y [n] [right] | poke | minimize |
    // png OUT (the controls layer at its next paint) | dump -> $XDG_RUNTIME_DIR/mist/player-state.json
    fn test(&mut self, a: &[String], cx: &mut Cx) {
        let Some(wid) = self.win else { return };
        let f = |i: usize| a.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        let has = |s: &str| a.iter().any(|x| x == s);
        match a.first().map(|s| s.as_str()) {
            Some("key") => {
                let name = a.get(1).cloned().unwrap_or_default();
                let k = match name.as_str() {
                    "space" => Keysym::space,
                    "Left" => Keysym::Left,
                    "Right" => Keysym::Right,
                    "Up" => Keysym::Up,
                    "Down" => Keysym::Down,
                    "Escape" => Keysym::Escape,
                    "Home" => Keysym::Home,
                    "BackSpace" => Keysym::BackSpace,
                    s if s.chars().count() == 1 => Keysym::from_char(s.chars().next().unwrap()),
                    _ => return,
                };
                cx.mods = appwin::Mods { ctrl: has("ctrl"), shift: has("shift"), alt: false };
                let u = (name.chars().count() == 1).then(|| name.clone());
                self.event(wid, Ev::Key(k, u), cx);
            }
            Some("click") => {
                let n = a.get(3).and_then(|s| s.parse::<u8>().ok()).unwrap_or(1);
                let b = if has("right") { RIGHT } else { LEFT };
                self.event(wid, Ev::Motion(f(1), f(2)), cx);
                for k in 1..=n {
                    self.event(wid, Ev::Press(f(1), f(2), b, k), cx);
                    self.event(wid, Ev::Release(f(1), f(2), b), cx);
                }
            }
            Some("poke") => self.poke(cx),
            Some("pip") => self.toggle_pip(cx),
            Some("chapters") => self.set_panel(Panel::Chapters, cx),
            Some("resume-queue") => self.resume_queue(cx),
            // the pointer over the seek bar at fraction F (asks for that thumbnail)
            Some("hover-seek") => {
                if let Some((bx, by, bw, bh, _)) = self.hits.iter().rev().find(|h| h.4 == Hit::Seek).cloned() {
                    let x = bx + 6.0 + (bw - 12.0) * f(1).clamp(0.0, 1.0);
                    self.event(wid, Ev::Motion(x, by + bh / 2.0), cx);
                }
            }
            Some("minimize") => cx.cmds.push(Cmd::Minimize(wid)),
            Some("png") => {
                self.png_req = a.get(1).cloned();
                cx.redraw(wid);
            }
            Some("dump") => {
                let tr: Vec<String> = self.tracks.iter().map(|t| format!("{}:{}:{}{}", t.kind, t.id, t.lang, if t.selected { "*" } else { "" })).collect();
                let e = self.eng.as_ref();
                let st = e.map(|e| e.stat).unwrap_or_default();
                let v = serde_json::json!({
                    "cur": self.cur, "idx": self.idx, "queue": self.queue.len(), "paused": self.paused, "pos": self.now_pos(), "dur": self.dur,
                    "video": self.video, "fs": self.fs, "panel": format!("{:?}", self.panel), "tracks": tr, "volume": self.volume,
                    "muted": self.muted, "speed": self.speed, "osd": self.osd.as_ref().map(|o| o.1.clone()), "err": self.err,
                    "hwdec": self.get_str("hwdec-current"), "vo": self.get_str("current-vo"), "sid": self.get_str("sid"), "sub_delay": self.get_str("sub-delay"),
                    "engine": e.is_some(), "gl": e.is_some_and(|e| e.gl_up()), "rctx": e.is_some_and(|e| e.gl_ready()), "watch_pos": e.is_some_and(|e| e.watching_pos()),
                    "mpris": self.mpris.is_some(), "stat": [st.updates, st.frames, self.paints], "callbacks": st.callbacks,
                    "paint_us": self.paint_us.checked_div(self.paints).unwrap_or(0), "first_frame_ms": self.first_frame_ms, "gl_ms": e.map(|e| e.gl_ms),
                    "chrome": self.chrome(), "title": self.nice_title(), "repeat": self.repeat, "shuffle": self.shuffle,
                });
                let more = serde_json::json!({
                    "chapters": self.chapters.len(), "chapter": self.chapter_now(), "pip": self.pip.is_some(), "pip_closing": self.pip_closing,
                    "video_win": self.video_win.map(|w| if Some(w) == self.pip { "pip" } else { "main" }), "thumbs": self.thumbs.made,
                    "thumbs_bytes": self.thumbs.bytes(), "thumbs_off": self.thumbs.off, "saved_queue": self.saved_queue.len(), "saved_idx": self.saved_idx,
                    "cover": self.cover.as_ref().map(|c| [c.width(), c.height()]), "cover_cols": self.cover_cols.is_some(),
                });
                let mut v = v;
                if let (Some(o), Some(m)) = (v.as_object_mut(), more.as_object()) {
                    o.extend(m.clone());
                }
                let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
                let _ = std::fs::write(format!("{dir}/player-state.json"), v.to_string());
            }
            _ => {}
        }
        cx.mods = appwin::Mods::default();
    }
}

// `mist-ui-bin player --png OUT [empty|video|audio|noart|plain|queue|tracks|help|info|chapters|seek|pip|pipmain|resume] [WxH] [frame]`: the controls drawn with a
// fixed demo state, no Wayland, no mpv (checks of the look)
fn png(args: &[String]) -> i32 {
    let Some(out) = args.get(1) else {
        eprintln!("player --png OUT [empty|video|audio|noart|plain|queue|tracks|help|info|chapters|seek|pip|pipmain|resume] [WxH] [frame]");
        return 2;
    };
    let what = args.get(2).map(|s| s.as_str()).unwrap_or("video");
    let (w, h) = args.get(3).and_then(|s| s.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))).unwrap_or((1280u32, 760u32));
    let mut p = Player::bare();
    p.w = w as f32;
    p.h = h as f32;
    p.recents = (0..3).map(|i| Recent { path: format!("/home/demo/Wideo/Film {}.mkv", i + 1), pos: 600_000.0 * i as f64, dur: 5_400_000.0, t: 0.0 }).collect();
    if what == "resume" {
        p.saved_queue = (1..=8).map(|i| format!("/home/demo/Wideo/Serial S01E{i:02}.mkv")).collect();
        p.saved_idx = 3;
    }
    if what != "empty" && what != "resume" {
        let audio = what == "audio" || what == "noart" || what == "plain";
        p.queue = (1..=12).map(|i| if audio { format!("/home/demo/Muzyka/{i:02} Utwór_{i}.flac") } else { format!("/home/demo/Wideo/Serial S01E{i:02}.mkv") }).collect();
        p.idx = 2;
        p.cur = p.queue[2].clone();
        p.pos = 1234.0;
        p.dur = 2700.0;
        p.paused = true;
        p.video = !audio;
        p.artist = if audio { "Artysta".into() } else { String::new() };
        p.album = if audio { "Album".into() } else { String::new() };
        p.tracks = vec![
            Track { id: 1, kind: "audio".into(), title: String::new(), lang: "pol".into(), codec: "eac3".into(), selected: true, external: false },
            Track { id: 2, kind: "audio".into(), title: "Komentarz".into(), lang: "eng".into(), codec: "aac".into(), selected: false, external: false },
            Track { id: 1, kind: "sub".into(), title: String::new(), lang: "pol".into(), codec: "subrip".into(), selected: true, external: true },
        ];
        p.chapters = vec![(0.0, "Początek".into()), (600.0, "Rozdział 2".into()), (1500.0, "Rozdział 3".into()), (2200.0, "Napisy końcowe".into())];
        p.panel = match what {
            "queue" => Panel::List,
            "tracks" => Panel::Tracks,
            "help" => Panel::Help,
            "info" => Panel::Info,
            "chapters" => Panel::Chapters,
            _ => Panel::None,
        };
        if what == "seek" {
            // the pointer over the bar at 60 %, a stand-in thumbnail (a gradient) for that spot
            p.paint_all(&mut Canvas::new(w, h));
            if let Some((bx, by, bw, bh, _)) = p.hits.iter().rev().find(|h| h.4 == Hit::Seek).cloned() {
                p.mouse = (bx + 6.0 + (bw - 12.0) * 0.6, by + bh / 2.0);
                p.hover = Some(Hit::Seek);
            }
            let mut pm = tiny_skia::Pixmap::new(thumbs::TW, thumbs::TH).unwrap();
            for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
                let (x, y) = ((i as u32 % thumbs::TW) as f32 / thumbs::TW as f32, (i as u32 / thumbs::TW) as f32 / thumbs::TH as f32);
                *px = tiny_skia::ColorU8::from_rgba((60.0 + 180.0 * x) as u8, (90.0 + 120.0 * y) as u8, 200, 255).premultiply();
            }
            let (b, _) = thumbs::bucket(0.6 * p.dur, p.dur);
            p.thumbs.reset(&p.cur.clone());
            p.thumbs.insert(b, pm);
        }
        if what == "pipmain" {
            p.pip = Some(7);
        }
        if what == "plain" {
            // a download without tags: the artist from the name, one audio track, no chapters, just resumed
            p.queue = vec!["/home/demo/Pobrane/Prince_and_The_Revolution_-_Purple_Rain_Album_12_edit_SV_Sound_(mp3.pm).mp3".into()];
            p.idx = 0;
            p.cur = p.queue[0].clone();
            (p.artist, p.album) = (String::new(), String::new());
            p.tracks.retain(|t| t.kind == "audio" && t.id == 1);
            p.chapters.clear();
            p.resume_at = Some((317.0, Instant::now()));
        }
    }
    let mut c = Canvas::new(w, h);
    if what == "pip" {
        p.pip = Some(7);
        p.pip_hover = true;
        p.pip_mouse = (w as f32 / 2.0, h as f32 / 2.0);
        p.paint_pip(&mut c);
        return match c.pm.save_png(out) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("player --png: {e}");
                1
            }
        };
    }
    if what == "audio" {
        // a stand-in album cover: warm diagonal bands
        let mut pm = tiny_skia::Pixmap::new(400, 400).unwrap();
        for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 400) as f32 / 400.0, (i / 400) as f32 / 400.0);
            let band = (((x + y) * 6.0).sin() * 0.5 + 0.5) * 0.35;
            *px = tiny_skia::ColorU8::from_rgba((230.0 - 90.0 * y + 25.0 * band) as u8, (110.0 + 60.0 * x * band) as u8, (60.0 + 120.0 * y) as u8, 255).premultiply();
        }
        p.cover_cols = Some(cover::palette(&pm));
        p.cover = Some(std::rc::Rc::new(pm));
    }
    p.paint_all(&mut c);
    // `frame`: the controls over a stand-in picture (the video plane under the surface in the real window)
    if args.get(4).map(|s| s.as_str()) == Some("frame") {
        let mut f = tiny_skia::Pixmap::new(w, h).unwrap();
        for (i, px) in f.pixels_mut().iter_mut().enumerate() {
            let (x, y) = ((i as u32 % w) as f32 / w as f32, (i as u32 / w) as f32 / h as f32);
            *px = tiny_skia::ColorU8::from_rgba((40.0 + 150.0 * x * (1.0 - y)) as u8, (70.0 + 110.0 * y) as u8, (90.0 + 80.0 * (1.0 - x)) as u8, 255).premultiply();
        }
        f.draw_pixmap(0, 0, c.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        c.pm = f;
    }
    match c.pm.save_png(out) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("player --png: {e}");
            1
        }
    }
}

pub fn run(_args: &[String]) -> i32 {
    let args: Vec<String> = std::env::args().skip(2).collect();
    if args.first().map(|s| s.as_str()) == Some("--png") {
        return png(&args);
    }
    if appwin::forward("player", &args) {
        return 0;
    }
    if args.first().map(|s| s.as_str()) == Some("--test") {
        return 1;
    }
    appwin::run("player", Box::new(Player::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_times() {
        assert_eq!(pretty("/m/Prince_and_The_Revolution_-_Purple_Rain (mp3.pm).mp3"), "Prince and The Revolution - Purple Rain");
        assert_eq!(pretty("/v/Film.2024.mkv"), "Film.2024");
        assert_eq!(time(59.9), "0:59");
        assert_eq!(time(3725.0), "1:02:05");
        assert_eq!(time(-3.0), "0:00");
        assert_eq!(rate(1.25).replace(',', "."), "1.25×"); // the decimal point follows the UI language
        let n = pretty("/m/Prince_and_The_Revolution_-_Purple_Rain_Album_12_edit_SV_Sound_(mp3.pm).mp3");
        assert_eq!(split_name(&n), Some(("Prince and The Revolution".into(), "Purple Rain Album 12 edit SV Sound".into())));
        assert_eq!(split_name("Daft Punk - Get Lucky (Official Video) [HD]"), Some(("Daft Punk".into(), "Get Lucky".into())));
        assert_eq!(split_name("Artysta - Utwór (Remix) - Official Audio"), Some(("Artysta".into(), "Utwór (Remix)".into())));
        assert_eq!(split_name("Bez myślnika"), None);
    }
    #[test]
    fn kinds() {
        assert!(is_audio("/a/b.FLAC"));
        assert!(!is_audio("/a/b.mkv"));
        assert!(is_media("x.webm") && !is_media("x.srt"));
        assert!(is_url("https://example.org/v.m3u8"));
        assert_eq!(ext("/a.b/c"), "");
    }
    #[test]
    fn playlist_m3u() {
        let d = std::env::temp_dir().join(format!("mist-player-m3u-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("lista.m3u8");
        std::fs::write(&f, "\u{feff}#EXTM3U\n#EXTINF:123,Utwór\na.mp3\n\n/abs/b.flac\nfile:///x/c%20d.ogg\nhttps://radio.example/stream\n").unwrap();
        let d = d.to_string_lossy().into_owned();
        assert_eq!(read_m3u(&f.to_string_lossy()), [format!("{d}/a.mp3"), "/abs/b.flac".into(), "/x/c d.ogg".into(), "https://radio.example/stream".into()]);
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn chapters_json() {
        let c = super::engine::parse_chapters(r#"[{"title":"Intro","time":0.0},{"time":90.5}]"#);
        assert_eq!(c, [(0.0, "Intro".to_string()), (90.5, String::new())]);
        assert!(super::engine::parse_chapters("").is_empty());
    }
    #[test]
    fn resume_rule() {
        // not in the first 5 s, not in the last 15 s, whole ms otherwise
        assert_eq!(resume_spot(4000.0, 600_000.0), 0.0);
        assert_eq!(resume_spot(590_000.0, 600_000.0), 0.0);
        assert_eq!(resume_spot(123_456.7, 600_000.0), 123_456.0);
        assert_eq!(resume_spot(123_456.0, 0.0), 123_456.0);
    }
    #[test]
    fn queue_from_folder() {
        let d = std::env::temp_dir().join(format!("mist-player-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for n in ["Ep 10.mkv", "Ep 2.mkv", "Ep 1.mkv", "notes.txt", "Ep 2.srt", ".hidden.mkv", "song.mp3"] {
            std::fs::write(d.join(n), b"").unwrap();
        }
        let f = d.join("Ep 2.mkv").to_string_lossy().into_owned();
        let (q, i) = folder_queue(&f);
        let names: Vec<&str> = q.iter().map(|p| p.rsplit('/').next().unwrap()).collect();
        assert_eq!(names, ["Ep 1.mkv", "Ep 2.mkv", "Ep 10.mkv", "song.mp3"]);
        assert_eq!(i, 1);
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn track_names() {
        let t = Track { id: 3, kind: "sub".into(), title: String::new(), lang: "pol".into(), codec: String::new(), selected: false, external: true };
        assert_eq!(track_name(&t, 0, "Napisy"), "Polski · plik");
        let t = Track { lang: String::new(), external: false, ..t };
        assert_eq!(track_name(&t, 1, "Napisy"), "Napisy 2");
    }
    #[test]
    fn demo_renders() {
        for what in ["empty", "video", "audio", "queue", "tracks", "help", "info", "chapters", "seek", "pip", "pipmain", "resume"] {
            let out = std::env::temp_dir().join(format!("mist-player-{what}-{}.png", std::process::id()));
            let args: Vec<String> = ["--png", &out.to_string_lossy(), what, "960x600"].iter().map(|s| s.to_string()).collect();
            assert_eq!(png(&args), 0, "{what}");
            let pm = tiny_skia::Pixmap::load_png(&out).unwrap();
            assert_eq!((pm.width(), pm.height()), (960, 600));
            // something was drawn
            assert!(pm.pixels().iter().filter(|p| p.alpha() > 0).count() > 2000, "{what}");
            let _ = std::fs::remove_file(out);
        }
    }
}
