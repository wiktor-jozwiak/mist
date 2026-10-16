// Mist Player, native (was the Quickshell app ~/.config/quickshell/mist-player): libmpv decodes (hardware
// decoding) and draws the picture with OpenGL on a subsurface under the window; the controls are this CPU
// canvas on top (transparent over the video). Video edge to edge, auto-hiding controls capsule, queue built
// from the file's folder, audio / subtitle tracks (external .srt next to the file too), speed, resume where
// you stopped (~/.config/mist/player.json, same format as the QML app).
// `mist-ui-bin player [--add] [file | file:// uri ...]`, one process (later launches hand their files over).
pub mod ffi;

use crate::appwin::{self, Cmd, Cx, Ev, Logic, VideoSurface, WinId, LEFT, RIGHT};
use crate::gfx::{stroke_rrect, Canvas, Rgba};
use crate::text::{Style, Text};
use ffi::*;
use smithay_client_toolkit::seat::keyboard::Keysym;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use std::any::Any;
use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

type Tx = calloop::channel::Sender<Box<dyn Any + Send>>;
enum Wake {
    Mpv,
    Render,
    Picked(Vec<String>, bool),
}
static MPV_PENDING: AtomicBool = AtomicBool::new(false);
static RENDER_PENDING: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn on_mpv_wake(ctx: *mut c_void) {
    if !MPV_PENDING.swap(true, Ordering::AcqRel) {
        let tx = unsafe { &*(ctx as *const Tx) };
        let _ = tx.send(Box::new(Wake::Mpv));
    }
}
unsafe extern "C" fn on_render_wake(ctx: *mut c_void) {
    if !RENDER_PENDING.swap(true, Ordering::AcqRel) {
        let tx = unsafe { &*(ctx as *const Tx) };
        let _ = tx.send(Box::new(Wake::Render));
    }
}
unsafe extern "C" fn get_proc(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let egl = unsafe { &*(ctx as *const Egl) };
    unsafe { (egl.eglGetProcAddress)(name) }
}

const VIDEO_EXT: [&str; 14] = ["mkv", "mp4", "m4v", "webm", "avi", "mov", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "ogv", "3gp"];
const AUDIO_EXT: [&str; 11] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav", "wma", "alac", "ape", "mka"];
const SPEEDS: [f64; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0];

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
// "Prince_and_The_Revolution_-_Purple_Rain.mp3" -> "Prince and The Revolution - Purple Rain"
pub fn pretty(p: &str) -> String {
    let b = p.rsplit('/').next().unwrap_or("");
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
pub fn time(sec: f64) -> String {
    let s = if sec > 0.0 { sec as u64 } else { 0 };
    let (h, m, ss) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{ss:02}") } else { format!("{m}:{ss:02}") }
}
fn lang(l: &str) -> String {
    let k = l.to_lowercase();
    let m = match k.as_str() {
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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Panel {
    None,
    List,
    Tracks,
    Speed,
    Help,
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
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub lang: String,
    pub selected: bool,
    pub external: bool,
}
impl Track {
    fn name(&self, i: usize, kind: &str) -> String {
        let mut p: Vec<String> = vec![];
        if !self.title.is_empty() {
            p.push(self.title.clone());
        }
        let l = lang(&self.lang);
        if !l.is_empty() {
            p.push(l);
        }
        if self.external {
            p.push("plik".into());
        }
        if p.is_empty() { format!("{kind} {}", i + 1) } else { p.join(" · ") }
    }
}

#[derive(Clone)]
pub struct Recent {
    pub path: String,
    pub pos: f64, // ms (player.json)
    pub dur: f64,
    pub t: f64,
}

struct GlState {
    dpy: EGLDisplay,
    cfg: EGLConfig,
    ctx: EGLContext,
    ewin: *mut c_void,
    surf: EGLSurface,
    w: i32,
    h: i32,
}

// what the panel's rows do
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
    None,
}

pub struct Player {
    libs: Option<Box<Libs>>,
    pub err: String,
    mpv: Handle,
    rctx: Handle,
    gl: Option<GlState>,
    tx: Option<Tx>,
    win: Option<WinId>,
    pub w: f32,
    pub h: f32,
    pub queue: Vec<String>,
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
    pub volume: f64,
    pub muted: bool,
    pub recents: Vec<Recent>,
    pending_resume: f64,
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
    drawn: (u64, i32, bool),
    save_at: Option<Instant>,
    rows: Vec<Row>,
    stat: (u64, u64, u64),
    backdrop: Option<(u32, u32, bool, tiny_skia::Pixmap)>,
}

fn cfg_path() -> String {
    format!("{}/.config/mist/player.json", std::env::var("HOME").unwrap_or_default())
}

impl Player {
    pub fn new() -> Player {
        let (libs, err) = match ffi::load() {
            Ok(l) => (Some(Box::new(l)), String::new()),
            Err(e) => (None, e),
        };
        let mut p = Player {
            libs,
            err,
            mpv: std::ptr::null_mut(),
            rctx: std::ptr::null_mut(),
            gl: None,
            tx: None,
            win: None,
            w: 1280.0,
            h: 760.0,
            queue: vec![],
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
            volume: 0.8,
            muted: false,
            recents: vec![],
            pending_resume: 0.0,
            pending_load: false,
            fs: false,
            ui_until: Instant::now() + Duration::from_millis(2600),
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
            drawn: (0, 0, false),
            save_at: None,
            rows: vec![],
            stat: (0, 0, 0),
            backdrop: None,
        };
        if let Ok(s) = std::fs::read_to_string(cfg_path()) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                p.volume = v["volume"].as_f64().unwrap_or(0.8).clamp(0.0, 1.0);
                p.muted = v["muted"].as_bool().unwrap_or(false);
                if let Some(a) = v["recents"].as_array() {
                    p.recents = a
                        .iter()
                        .filter_map(|e| {
                            Some(Recent { path: e["path"].as_str()?.to_string(), pos: e["pos"].as_f64().unwrap_or(0.0), dur: e["dur"].as_f64().unwrap_or(0.0), t: e["t"].as_f64().unwrap_or(0.0) })
                        })
                        .collect();
                }
            }
        }
        p
    }
    fn save(&self) {
        let r: Vec<serde_json::Value> = self.recents.iter().map(|e| serde_json::json!({ "path": e.path, "pos": e.pos, "dur": e.dur, "t": e.t })).collect();
        let v = serde_json::json!({ "volume": self.volume, "muted": self.muted, "recents": r });
        let _ = std::fs::create_dir_all(std::path::Path::new(&cfg_path()).parent().unwrap());
        let _ = std::fs::write(cfg_path(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    fn remember(&mut self, path: &str, pos_s: f64, dur_s: f64) {
        let (pos, dur) = (pos_s * 1000.0, dur_s * 1000.0);
        self.recents.retain(|e| e.path != path);
        let p = if (dur > 0.0 && pos > dur - 15000.0) || pos < 5000.0 { 0.0 } else { pos.floor() };
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        self.recents.insert(0, Recent { path: path.into(), pos: p, dur: dur.floor(), t });
        self.recents.truncate(30);
        self.save();
    }
    fn resume_of(&self, path: &str) -> f64 {
        self.recents.iter().find(|e| e.path == path).map(|e| e.pos / 1000.0).unwrap_or(0.0)
    }

    // ------------------------------------------------------------ mpv
    fn init_mpv(&mut self, tx: &Tx) {
        let Some(l) = self.libs.as_ref() else { return };
        if !self.mpv.is_null() {
            return;
        }
        unsafe {
            let m = (l.mpv.mpv_create)();
            if m.is_null() {
                self.err = "mpv_create nie działa".into();
                return;
            }
            let opt = |k: &str, v: &str| {
                let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap());
                (l.mpv.mpv_set_option_string)(m, k.as_ptr(), v.as_ptr());
            };
            opt("config", "no");
            opt("vo", "libmpv");
            opt("hwdec", "auto-safe");
            opt("keep-open", "yes");
            opt("idle", "yes");
            opt("input-default-bindings", "no");
            opt("input-vo-keyboard", "no");
            opt("osc", "no");
            opt("ytdl", "no");
            opt("terminal", "no");
            opt("audio-client-name", "mist-player");
            opt("sub-auto", "fuzzy");
            opt("audio-file-auto", "no");
            opt("sub-font", "SF Pro Display");
            opt("sub-font-size", "46");
            opt("sub-border-size", "2.5");
            opt("sub-shadow-offset", "1");
            opt("slang", "pl,pol,en,eng");
            opt("alang", "pl,pol,en,eng,ja,jpn");
            opt("volume", &format!("{}", self.volume * 100.0));
            opt("mute", if self.muted { "yes" } else { "no" });
            if (l.mpv.mpv_initialize)(m) < 0 {
                self.err = "mpv_initialize nie działa".into();
                return;
            }
            self.mpv = m;
            for (i, (name, fmt)) in [("time-pos", FMT_DOUBLE), ("duration", FMT_DOUBLE), ("pause", FMT_FLAG), ("eof-reached", FMT_FLAG), ("vo-configured", FMT_FLAG), ("speed", FMT_DOUBLE), ("media-title", FMT_STRING), ("track-list/count", FMT_INT64), ("aid", FMT_STRING), ("sid", FMT_STRING), ("volume", FMT_DOUBLE), ("mute", FMT_FLAG)].iter().enumerate() {
                let c = CString::new(*name).unwrap();
                (l.mpv.mpv_observe_property)(m, i as u64 + 1, c.as_ptr(), *fmt);
            }
            let ctx = Box::into_raw(Box::new(tx.clone())) as *mut c_void;
            (l.mpv.mpv_set_wakeup_callback)(m, Some(on_mpv_wake), ctx);
        }
    }
    fn cmd(&self, args: &[&str]) {
        let Some(l) = self.libs.as_ref() else { return };
        if self.mpv.is_null() {
            return;
        }
        let cs: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap_or_default()).collect();
        let mut ptrs: Vec<*const c_char> = cs.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe { (l.mpv.mpv_command_async)(self.mpv, 0, ptrs.as_mut_ptr()) };
    }
    fn set(&self, k: &str, v: &str) {
        let Some(l) = self.libs.as_ref() else { return };
        if self.mpv.is_null() {
            return;
        }
        let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap());
        unsafe { (l.mpv.mpv_set_property_string)(self.mpv, k.as_ptr(), v.as_ptr()) };
    }
    fn get_str(&self, k: &str) -> Option<String> {
        let l = self.libs.as_ref()?;
        if self.mpv.is_null() {
            return None;
        }
        let k = CString::new(k).unwrap();
        unsafe {
            let p = (l.mpv.mpv_get_property_string)(self.mpv, k.as_ptr());
            if p.is_null() {
                return None;
            }
            let s = cstr(p);
            (l.mpv.mpv_free)(p as *mut c_void);
            Some(s)
        }
    }
    fn get_i64(&self, k: &str) -> Option<i64> {
        let l = self.libs.as_ref()?;
        let k = CString::new(k).unwrap();
        let mut v: i64 = 0;
        let r = unsafe { (l.mpv.mpv_get_property)(self.mpv, k.as_ptr(), FMT_INT64, &mut v as *mut i64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    fn refresh_tracks(&mut self) {
        let n = self.get_i64("track-list/count").unwrap_or(0);
        let mut v = vec![];
        for i in 0..n {
            let g = |k: &str| self.get_str(&format!("track-list/{i}/{k}"));
            v.push(Track {
                id: self.get_i64(&format!("track-list/{i}/id")).unwrap_or(0),
                kind: g("type").unwrap_or_default(),
                title: g("title").unwrap_or_default(),
                lang: g("lang").unwrap_or_default(),
                selected: g("selected").as_deref() == Some("yes"),
                external: g("external").as_deref() == Some("yes"),
            });
        }
        self.tracks = v;
    }
    fn of_kind(&self, k: &str) -> Vec<Track> {
        self.tracks.iter().filter(|t| t.kind == k).cloned().collect()
    }

    fn mpv_events(&mut self, cx: &mut Cx) {
        MPV_PENDING.store(false, Ordering::Release);
        let Some(l) = self.libs.as_ref() else { return };
        let wait = l.mpv.mpv_wait_event;
        let errs = l.mpv.mpv_error_string;
        if self.mpv.is_null() {
            return;
        }
        let mut redraw = false;
        loop {
            let e = unsafe { &*wait(self.mpv, 0.0) };
            match e.event_id {
                EV_NONE => break,
                EV_SHUTDOWN => break,
                EV_PROPERTY_CHANGE => {
                    let p = unsafe { &*(e.data as *const mpv_event_property) };
                    let has = !p.data.is_null() && p.format != 0;
                    let dbl = || if has && p.format == FMT_DOUBLE { unsafe { *(p.data as *const f64) } } else { 0.0 };
                    let flag = || has && p.format == FMT_FLAG && unsafe { *(p.data as *const i32) } != 0;
                    let s = || if has && p.format == FMT_STRING { cstr(unsafe { *(p.data as *const *const c_char) }) } else { String::new() };
                    match e.reply_userdata {
                        1 => {
                            self.pos = dbl();
                            // only redraw when the visible time or the bar moved
                            let bw = (self.w - 48.0).min(860.0) - 170.0;
                            let key = (self.pos as u64, (self.pos / self.dur.max(1.0) * bw as f64) as i32, self.chrome());
                            if key != self.drawn && self.chrome() {
                                self.drawn = key;
                                redraw = true;
                            }
                        }
                        2 => {
                            self.dur = dbl();
                            redraw = true;
                        }
                        3 => {
                            self.paused = flag();
                            self.inhibit(cx);
                            redraw = true;
                        }
                        4 => {
                            if flag() && !self.cur.is_empty() {
                                self.save_spot();
                                self.next(true, cx);
                                redraw = true;
                            }
                        }
                        5 => {
                            self.video = flag();
                            self.inhibit(cx);
                            redraw = true;
                        }
                        6 => {
                            self.speed = if has { dbl() } else { 1.0 };
                            redraw = true;
                        }
                        7 => {
                            self.title = s();
                            redraw = true;
                        }
                        8..=10 => {
                            self.refresh_tracks();
                            redraw = true;
                        }
                        11 => {
                            if has {
                                let v = (dbl() / 100.0).clamp(0.0, 1.0);
                                if (v - self.volume).abs() > 0.001 {
                                    self.volume = v;
                                }
                            }
                        }
                        12 => self.muted = flag(),
                        _ => {}
                    }
                }
                EV_FILE_LOADED => {
                    self.artist = self.get_str("metadata/by-key/artist").or_else(|| self.get_str("metadata/by-key/album_artist")).unwrap_or_default();
                    self.album = self.get_str("metadata/by-key/album").unwrap_or_default();
                    self.refresh_tracks();
                    let p = std::mem::take(&mut self.pending_resume);
                    if p > 5.0 && (self.dur <= 0.0 || p < self.dur - 15.0) {
                        self.cmd(&["seek", &format!("{p}"), "absolute"]);
                        self.resume_at = Some((p, Instant::now() + Duration::from_secs(6)));
                        cx.wake_at(Instant::now() + Duration::from_secs(6));
                    }
                    redraw = true;
                }
                EV_END_FILE => {
                    let ef = unsafe { &*(e.data as *const mpv_event_end_file) };
                    if ef.reason == END_ERROR {
                        let m = cstr(unsafe { errs(ef.error) });
                        self.say("x", &format!("Nie można odtworzyć: {m}"), cx);
                        redraw = true;
                    }
                }
                _ => {}
            }
        }
        if redraw {
            self.redraw(cx);
        }
    }

    // ------------------------------------------------------------ GL plane
    fn gl_init(&mut self, vs: VideoSurface) -> Result<(), String> {
        let l = self.libs.as_ref().ok_or("brak bibliotek")?;
        let (e, we) = (&l.egl, &l.wegl);
        unsafe {
            let dpy = (e.eglGetDisplay)(vs.display);
            if dpy.is_null() || (e.eglInitialize)(dpy, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                return Err("EGL: brak wyświetlacza".into());
            }
            (e.eglBindAPI)(EGL_OPENGL_API);
            let attrs = [EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 0, EGL_SURFACE_TYPE, EGL_WINDOW_BIT, EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT, EGL_NONE];
            let mut cfg: EGLConfig = std::ptr::null_mut();
            let mut n = 0;
            if (e.eglChooseConfig)(dpy, attrs.as_ptr(), &mut cfg, 1, &mut n) == 0 || n < 1 {
                return Err("EGL: brak konfiguracji".into());
            }
            let cattrs = [EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 3, EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT, EGL_NONE];
            let mut ctx = (e.eglCreateContext)(dpy, cfg, std::ptr::null_mut(), cattrs.as_ptr());
            if ctx.is_null() {
                ctx = (e.eglCreateContext)(dpy, cfg, std::ptr::null_mut(), [EGL_NONE].as_ptr());
            }
            if ctx.is_null() {
                return Err("EGL: brak kontekstu".into());
            }
            let (w, h) = (self.w.max(1.0) as i32, self.h.max(1.0) as i32);
            let ewin = (we.wl_egl_window_create)(vs.surface, w, h);
            let surf = (e.eglCreateWindowSurface)(dpy, cfg, ewin, std::ptr::null());
            if surf.is_null() {
                return Err("EGL: brak powierzchni".into());
            }
            (e.eglMakeCurrent)(dpy, surf, surf, ctx);
            // never block the loop on the compositor: frames are paced by mpv's update callback
            (e.eglSwapInterval)(dpy, 0);
            self.gl = Some(GlState { dpy, cfg, ctx, ewin, surf, w, h });
            if self.rctx.is_null() && !self.mpv.is_null() {
                let api = CString::new("opengl").unwrap();
                let mut init = mpv_opengl_init_params { get_proc_address: get_proc, get_proc_address_ctx: &l.egl as *const Egl as *mut c_void };
                let mut params = [
                    mpv_render_param { kind: RP_API_TYPE, data: api.as_ptr() as *mut c_void },
                    mpv_render_param { kind: RP_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
                    mpv_render_param { kind: 0, data: std::ptr::null_mut() },
                ];
                let mut rc: Handle = std::ptr::null_mut();
                if (l.mpv.mpv_render_context_create)(&mut rc, self.mpv, params.as_mut_ptr()) < 0 {
                    return Err("mpv: brak renderowania OpenGL".into());
                }
                self.rctx = rc;
                if let Some(tx) = &self.tx {
                    let c = Box::into_raw(Box::new(tx.clone())) as *mut c_void;
                    (l.mpv.mpv_render_context_set_update_callback)(rc, Some(on_render_wake), c);
                }
            }
        }
        Ok(())
    }
    fn gl_drop_surface(&mut self) {
        let (Some(l), Some(g)) = (self.libs.as_ref(), self.gl.take()) else { return };
        unsafe {
            (l.egl.eglMakeCurrent)(g.dpy, std::ptr::null_mut(), std::ptr::null_mut(), g.ctx);
            (l.egl.eglDestroySurface)(g.dpy, g.surf);
            (l.wegl.wl_egl_window_destroy)(g.ewin);
            let _ = g.cfg;
        }
        // keep the context for the next plane? the window is going: free everything
        unsafe {
            if !self.rctx.is_null() {
                (l.mpv.mpv_render_context_free)(self.rctx);
                self.rctx = std::ptr::null_mut();
            }
            (l.egl.eglMakeCurrent)(g.dpy, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (l.egl.eglDestroyContext)(g.dpy, g.ctx);
        }
    }
    // draw the current video frame (force: also when mpv has no new one, e.g. after a resize)
    fn render(&mut self, force: bool) {
        RENDER_PENDING.store(false, Ordering::Release);
        self.stat.0 += 1;
        let (Some(l), Some(g)) = (self.libs.as_ref(), self.gl.as_mut()) else { return };
        if self.rctx.is_null() {
            return;
        }
        unsafe {
            let flags = (l.mpv.mpv_render_context_update)(self.rctx);
            if flags & UPDATE_FRAME == 0 && !force {
                return;
            }
            self.stat.1 += 1;
            (l.egl.eglMakeCurrent)(g.dpy, g.surf, g.surf, g.ctx);
            let (w, h) = (self.w.max(1.0) as i32, self.h.max(1.0) as i32);
            if (w, h) != (g.w, g.h) {
                (l.wegl.wl_egl_window_resize)(g.ewin, w, h, 0, 0);
                g.w = w;
                g.h = h;
            }
            let mut fbo = mpv_opengl_fbo { fbo: 0, w, h, internal_format: 0 };
            let mut flip: i32 = 1;
            let mut params = [
                mpv_render_param { kind: RP_OPENGL_FBO, data: &mut fbo as *mut _ as *mut c_void },
                mpv_render_param { kind: RP_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
                mpv_render_param { kind: 0, data: std::ptr::null_mut() },
            ];
            (l.mpv.mpv_render_context_render)(self.rctx, params.as_mut_ptr());
            (l.egl.eglSwapBuffers)(g.dpy, g.surf);
            (l.mpv.mpv_render_context_report_swap)(self.rctx);
        }
    }
    fn shutdown(&mut self) {
        self.save_spot();
        self.gl_drop_surface();
        if let (Some(l), false) = (self.libs.as_ref(), self.mpv.is_null()) {
            unsafe { (l.mpv.mpv_terminate_destroy)(self.mpv) };
            self.mpv = std::ptr::null_mut();
        }
    }

    // ------------------------------------------------------------ playback
    fn redraw(&self, cx: &mut Cx) {
        if let Some(w) = self.win {
            cx.redraw(w);
        }
    }
    pub fn chrome(&self) -> bool {
        Instant::now() < self.ui_until || self.paused || self.panel != Panel::None || self.cur.is_empty() || !self.video || self.drag.is_some() || matches!(self.hover, Some(ref h) if *h != Hit::Picture && *h != Hit::Drag)
    }
    fn poke(&mut self, cx: &mut Cx) {
        let was = self.chrome();
        self.ui_until = Instant::now() + Duration::from_millis(2600);
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
        let paths: Vec<String> = paths.into_iter().filter(|p| std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)).collect();
        if paths.is_empty() {
            return;
        }
        if paths.len() == 1 {
            let f = paths[0].clone();
            // queue = media files in the same folder, natural order
            let d = crate::files::fs::parent(&f);
            let mut list: Vec<String> = std::fs::read_dir(&d).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| is_media(n) && !n.starts_with('.')).collect()).unwrap_or_default();
            list.sort_by(|a, b| crate::files::fs::natural_cmp(a, b));
            let list: Vec<String> = list.iter().map(|n| crate::files::fs::join(&d, n)).collect();
            match list.iter().position(|p| *p == f) {
                Some(i) => {
                    self.queue = list;
                    self.load(i, cx);
                }
                None => {
                    self.queue = vec![f];
                    self.load(0, cx);
                }
            }
        } else {
            self.queue = paths;
            self.load(0, cx);
        }
    }
    fn add(&mut self, paths: Vec<String>, cx: &mut Cx) {
        let paths: Vec<String> = paths.into_iter().filter(|p| !self.queue.contains(p) && std::fs::metadata(p).is_ok()).collect();
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
    fn load(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.queue.len() {
            return;
        }
        self.save_spot();
        self.idx = i as i32;
        self.cur = self.queue[i].clone();
        self.pending_resume = self.resume_of(&self.cur.clone());
        self.title.clear();
        self.artist.clear();
        self.album.clear();
        self.pos = 0.0;
        self.dur = 0.0;
        self.resume_at = None;
        if self.rctx.is_null() {
            // the GL plane is not there yet: the file waits for it (vo=libmpv needs the render context)
            self.pending_load = true;
        } else {
            self.cmd(&["loadfile", &self.cur.clone(), "replace"]);
            self.set("pause", "no");
        }
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
        if self.pos > 4.0 || self.queue.len() < 2 {
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
        self.cur.clear();
        self.queue.clear();
        self.idx = -1;
        self.panel = Panel::None;
        self.video = false;
        self.inhibit(cx);
        self.redraw(cx);
    }
    fn save_spot(&mut self) {
        if !self.cur.is_empty() && self.dur > 0.0 {
            let (c, p, d) = (self.cur.clone(), self.pos, self.dur);
            self.remember(&c, p, d);
        }
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
        let cur = subs.iter().position(|t| t.selected);
        let next = match cur {
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
                let n = subs[i].name(i, "Napisy");
                self.say("cc", &n, cx);
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
        let n = a[i].name(i, "Ścieżka");
        self.say("head", &n, cx);
    }
    fn toggle_fs(&mut self, cx: &mut Cx) {
        if let Some(w) = self.win {
            cx.cmds.push(Cmd::Fullscreen(w, !self.fs));
        }
    }
    fn set_panel(&mut self, p: Panel, cx: &mut Cx) {
        self.panel = if self.panel == p { Panel::None } else { p };
        self.redraw(cx);
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
                let filters: Vec<(String, Vec<(u32, String)>)> = vec![("Multimedia".into(), pats(&all)), ("Wideo".into(), pats(&VIDEO_EXT)), ("Audio".into(), pats(&AUDIO_EXT)), ("Wszystkie pliki".into(), vec![(0, "*".into())])];
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
}

pub fn rate(r: f64) -> String {
    format!("{}×", format!("{r}").replace('.', ","))
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
        let ch = utf8.as_deref().and_then(|s| s.chars().next()).map(|c| c.to_ascii_lowercase());
        match k {
            Keysym::w | Keysym::W | Keysym::q | Keysym::Q if m.ctrl => self.quit(cx),
            Keysym::o | Keysym::O if m.ctrl => self.pick(false),
            Keysym::space | Keysym::k | Keysym::K | Keysym::XF86_AudioPlay => self.toggle(cx),
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
            Keysym::n | Keysym::N | Keysym::XF86_AudioNext => self.next(false, cx),
            Keysym::p | Keysym::P | Keysym::XF86_AudioPrev => self.prev(cx),
            Keysym::bracketleft => self.step_speed(-1, cx),
            Keysym::bracketright => self.step_speed(1, cx),
            Keysym::BackSpace => self.set_speed(1.0, cx),
            Keysym::s | Keysym::S => self.cycle_subs(cx),
            Keysym::a | Keysym::A => self.cycle_audio(cx),
            Keysym::q | Keysym::Q => self.set_panel(Panel::List, cx),
            Keysym::o | Keysym::O => self.pick(false),
            Keysym::h | Keysym::H | Keysym::question => self.set_panel(Panel::Help, cx),
            Keysym::Home => self.cmd(&["seek", "0", "absolute"]),
            Keysym::End => self.cmd(&["seek", "-1", "absolute+exact"]),
            Keysym::comma => self.cmd(&["frame-back-step"]),
            Keysym::period => self.cmd(&["frame-step"]),
            Keysym::z | Keysym::Z => {
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
        if let Some(w) = self.win.take() {
            cx.cmds.push(Cmd::Close(w));
        }
        self.shutdown();
        cx.cmds.push(Cmd::Exit);
    }
    fn press(&mut self, x: f32, y: f32, b: u32, clicks: u8, cx: &mut Cx) {
        self.poke(cx);
        let h = self.hit_at(x, y);
        // a click outside an open panel closes it
        let in_panel = matches!(h, Some(Hit::Item(_)) | Some(Hit::Queue(_)) | Some(Hit::QueueRemove(_)) | Some(Hit::Shuffle) | Some(Hit::Repeat) | Some(Hit::Add) | Some(Hit::Card));
        let toggles = matches!(h, Some(Hit::Speed) | Some(Hit::Tracks) | Some(Hit::List) | Some(Hit::Help));
        if self.panel != Panel::None && !in_panel && !toggles {
            self.panel = Panel::None;
            self.redraw(cx);
            if matches!(h, Some(Hit::Picture) | None) {
                return;
            }
        }
        let Some(h) = h else { return };
        self.redraw(cx);
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
                    let t = Instant::now() + Duration::from_millis(230);
                    self.click_at = Some(t);
                    cx.wake_at(t);
                }
            }
            Hit::Drag => {
                if clicks >= 2 {
                    self.toggle_fs(cx);
                } else if !self.fs {
                    if let Some(w) = self.win {
                        cx.cmds.push(Cmd::Move(w));
                    }
                }
            }
            Hit::Light(0) => self.quit(cx),
            Hit::Light(1) => appwin::hypr("MistMinimize()"),
            Hit::Light(_) => appwin::hypr("MistMaximize()"),
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
        }
    }
}

// ---------------------------------------------------------------- the app
impl Logic for Player {
    fn app_id(&self) -> &str {
        "mist-player"
    }
    fn opened(&mut self, id: WinId, _token: u64, cx: &mut Cx) {
        self.win = Some(id);
        cx.redraw(id);
    }
    fn paint(&mut self, _id: WinId, c: &mut Canvas, _cx: &mut Cx) {
        self.w = c.w();
        self.h = c.h();
        self.paint_all(c);
    }
    fn video_rect(&mut self, _id: WinId) -> Option<(i32, i32, i32, i32)> {
        // the plane lives as long as the window (vo=libmpv needs the render context before a video starts)
        self.libs.as_ref().map(|_| (0, 0, self.w as i32, self.h as i32))
    }
    fn video_surface(&mut self, _id: WinId, s: Option<VideoSurface>, cx: &mut Cx) {
        match s {
            Some(vs) => {
                if let Err(e) = self.gl_init(vs) {
                    self.err = e;
                }
                self.render(true);
                if self.pending_load && !self.rctx.is_null() {
                    self.pending_load = false;
                    self.cmd(&["loadfile", &self.cur.clone(), "replace"]);
                    self.set("pause", "no");
                }
                self.redraw(cx);
            }
            None => self.gl_drop_surface(),
        }
    }
    fn event(&mut self, _id: WinId, ev: Ev, cx: &mut Cx) {
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
                        return;
                    }
                    Some(Hit::Vol) => {
                        let v = self.bar_frac(&Hit::Vol, x);
                        self.set_vol(v, cx);
                        return;
                    }
                    _ => {}
                }
                let h = self.hit_at(x, y);
                if h != self.hover || moved_ui || matches!(h, Some(Hit::Seek)) {
                    self.hover = h.clone();
                    self.redraw(cx);
                }
                let c = match h {
                    Some(Hit::Picture) | Some(Hit::Drag) | None => CursorIcon::Default,
                    _ => CursorIcon::Pointer,
                };
                cx.cursor(c);
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
                if self.panel == Panel::None {
                    self.set_vol(self.volume - dy as f64 / 15.0 * 0.05, cx);
                }
            }
            Ev::Resized => {
                self.render(true);
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
                if !self.cur.is_empty() && cx.mods.shift {
                    self.add(paths, cx);
                } else {
                    self.open(paths, cx);
                }
                self.redraw(cx);
            }
            Ev::Focus(_) => {}
        }
    }
    fn close_requested(&mut self, _id: WinId, cx: &mut Cx) {
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
        if let Some((_, t)) = &self.resume_at {
            if now >= *t {
                self.resume_at = None;
                self.redraw(cx);
            }
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
        // the controls hide after 2.6 s without input (playing video); the pointer too
        if now >= self.ui_until {
            if !self.chrome() {
                self.redraw(cx);
                if self.video && !self.cur.is_empty() {
                    cx.cmds.push(Cmd::HideCursor(true));
                }
            }
        } else {
            cx.wake_at(self.ui_until);
        }
        // the resume spot every 10 s while playing
        if !self.paused && !self.cur.is_empty() {
            let t = self.recents.iter().find(|r| r.path == self.cur).map(|r| r.t).unwrap_or(0.0);
            let nowms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
            if nowms - t > 10_000.0 {
                self.save_spot();
            }
            cx.wake_at(now + Duration::from_secs(10));
        }
    }
    fn message(&mut self, args: Vec<String>, cx: &mut Cx) {
        if self.tx.is_none() {
            self.tx = Some(cx.tx.clone());
            let tx = cx.tx.clone();
            self.init_mpv(&tx);
        }
        if args.first().map(|s| s.as_str()) == Some("--test") {
            self.test(&args[1..], cx);
            return;
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
                } else if a.starts_with('/') {
                    a.clone()
                } else {
                    format!("{}/{a}", std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default())
                }
            })
            .collect();
        if !paths.is_empty() {
            if add { self.add(paths, cx) } else { self.open(paths, cx) }
            if let Some(w) = self.win {
                let _ = w;
                appwin::hypr("hl.dispatch(hl.dsp.focus({ window = \"class:^(mist-player)$\" }))");
            }
        }
    }
    fn async_msg(&mut self, m: Box<dyn Any + Send>, cx: &mut Cx) {
        let Ok(w) = m.downcast::<Wake>() else { return };
        match *w {
            Wake::Mpv => self.mpv_events(cx),
            Wake::Render => self.render(false),
            Wake::Picked(v, add) => {
                if add { self.add(v, cx) } else { self.open(v, cx) }
            }
        }
        if let Some(w) = self.win {
            let t = match self.cur.is_empty() {
                true => "Odtwarzacz — Mist Player".to_string(),
                false => format!("Odtwarzacz — {}", self.nice_title()),
            };
            TITLE.with(|c| {
                if *c.borrow() != t {
                    *c.borrow_mut() = t.clone();
                    cx.cmds.push(Cmd::Title(w, t));
                }
            });
        }
    }
}
thread_local! {
    static TITLE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

impl Player {
    pub fn nice_title(&self) -> String {
        if self.title.chars().count() > 1 && self.title != self.cur.rsplit('/').next().unwrap_or("") { self.title.clone() } else { pretty(&self.cur) }
    }
    // test hooks (running instance): key NAME [shift ctrl] | click X Y [n] [right] | dump
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
            Some("dump") => {
                let tr: Vec<String> = self.tracks.iter().map(|t| format!("{}:{}:{}{}", t.kind, t.id, t.lang, if t.selected { "*" } else { "" })).collect();
                let v = serde_json::json!({
                    "cur": self.cur, "idx": self.idx, "queue": self.queue.len(), "paused": self.paused, "pos": self.pos, "dur": self.dur,
                    "video": self.video, "fs": self.fs, "panel": format!("{:?}", self.panel), "tracks": tr, "volume": self.volume,
                    "muted": self.muted, "speed": self.speed, "osd": self.osd.as_ref().map(|o| o.1.clone()), "err": self.err,
                    "hwdec": self.get_str("hwdec-current"), "vo": self.get_str("current-vo"), "gl": self.gl.is_some(), "stat": [self.stat.0, self.stat.1, self.stat.2], "rctx": !self.rctx.is_null(), "chrome": self.chrome(), "title": self.nice_title(),
                });
                let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
                let _ = std::fs::write(format!("{dir}/player-state.json"), v.to_string());
            }
            _ => {}
        }
        cx.mods = appwin::Mods::default();
    }
}

// ---------------------------------------------------------------- painting
const FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);
const FG2: Rgba = Rgba(222.0 / 255.0, 230.0 / 255.0, 242.0 / 255.0, 0.86);
const MUTED: Rgba = Rgba(206.0 / 255.0, 216.0 / 255.0, 232.0 / 255.0, 0.74);
const FAINT: Rgba = Rgba(196.0 / 255.0, 208.0 / 255.0, 226.0 / 255.0, 0.6);
const ACC: Rgba = Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 1.0);
const ACC2: Rgba = Rgba(168.0 / 255.0, 228.0 / 255.0, 223.0 / 255.0, 1.0);
const RIM: Rgba = Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16);
const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.1);
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const INK: Rgba = Rgba(11.0 / 255.0, 17.0 / 255.0, 24.0 / 255.0, 1.0);
const SMOKE: Rgba = Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.78);

fn glyph(name: &str) -> Option<char> {
    let c = match name {
        "play" => 0xf691,
        "pause" => 0xf690,
        "next" => 0xf694,
        "prev" => 0xf693,
        "back10" => 0xfaba,
        "fwd10" => 0xfac2,
        "vol0" => 0xeb50,
        "vol1" => 0xeb4f,
        "vol2" => 0xeb51,
        "full" => 0xeaea,
        "unfull" => 0xeaf1,
        "list" => 0xeec0,
        "cc" => 0xf55a,
        "music" => 0xeafc,
        "open" => 0xfaf7,
        "x" => 0xeb55,
        "repeat" => 0xeb72,
        "repeat1" => 0xeb71,
        "shuffle" => 0xf000,
        "movie" => 0xeafa,
        "clock" => 0xea70,
        "head" => 0xeabd,
        "check" => 0xea5e,
        "plus" => 0xeaa0,
        "kbd" => 0xebd6,
        "crop" => 0xea85,
        "minus" => 0xeaf2,
        "close" => 0xeb55,
        "maxi" => 0xeb0b,
        _ => return None,
    };
    char::from_u32(c)
}
fn gl(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, name: &str, size: f32, col: Rgba) {
    let Some(ch) = glyph(name) else { return };
    let st = Style::icon(size);
    let s = ch.to_string();
    let w = t.width(&s, &st);
    c.text(t, cx - w / 2.0, cy, &s, &st, col);
}
fn sk(c: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.0.clamp(0.0, 1.0), c.1.clamp(0.0, 1.0), c.2.clamp(0.0, 1.0), c.3.clamp(0.0, 1.0)).unwrap()
}
fn elide(t: &mut Text, s: &str, st: &Style, max: f32) -> String {
    crate::bar::elide(t, s, st, max)
}
fn centered(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, s: &str, st: &Style, col: Rgba) {
    let w = t.width(s, st);
    c.text(t, cx - w / 2.0, cy, s, st, col);
}

impl Player {
    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, k: Hit) -> bool {
        let on = self.hover.as_ref() == Some(&k);
        self.hits.push((x, y, w, h, k));
        on
    }
    // round glyph button (RBtn.qml): 38 px, or 52 px white for play
    fn rbtn(&mut self, c: &mut Canvas, x: f32, cy: f32, g: &str, k: Hit, on: bool, big: bool) -> f32 {
        let d = if big { 52.0 } else { 38.0 };
        let hov = self.hit(x, cy - d / 2.0, d, d, k);
        if big {
            c.rrect(x, cy - d / 2.0, d, d, d / 2.0, WHITE.alpha(if hov { 0.96 } else { 0.9 }));
        } else if hov || on {
            c.rrect(x, cy - d / 2.0, d, d, d / 2.0, WHITE.alpha(if on { 0.12 } else { 0.10 }));
        }
        let col = if big { INK } else if on { ACC2 } else if hov { FG } else { FG2 };
        gl(c, &mut self.text, x + d / 2.0 + if big && g == "play" { 1.5 } else { 0.0 }, cy, g, if big { 24.0 } else { 20.0 }, col);
        d
    }
    fn bar(&mut self, c: &mut Canvas, x: f32, cy: f32, w: f32, v: f64, k: Hit, hover_v: Option<f64>) {
        let hov = self.hit(x - 6.0, cy - 10.0, w + 12.0, 20.0, k.clone()) || self.drag.as_ref() == Some(&k);
        let h = if hov { 6.0 } else { 4.0 };
        c.rrect(x, cy - h / 2.0, w, h, h / 2.0, WHITE.alpha(0.16));
        if let (true, Some(hv)) = (hov, hover_v) {
            c.rrect(x, cy - h / 2.0, (w * hv as f32).max(h), h, h / 2.0, WHITE.alpha(0.22));
        }
        c.rrect(x, cy - h / 2.0, (w * v.clamp(0.0, 1.0) as f32).max(h), h, h / 2.0, FG);
        if hov {
            c.rrect(x + w * v.clamp(0.0, 1.0) as f32 - 7.0, cy - 7.0, 14.0, 14.0, 7.0, WHITE);
        }
    }

    fn paint_all(&mut self, c: &mut Canvas) {
        self.stat.2 += 1;
        self.hits.clear();
        let (w, h) = (self.w, self.h);
        c.clear();
        let show_video = self.video && !self.cur.is_empty();
        if !show_video {
            self.paint_backdrop(c);
        } else {
            self.backdrop = None;
        }
        self.hit(0.0, 0.0, w, h, Hit::Picture);
        let chrome = self.chrome();
        if !self.err.is_empty() && self.libs.is_none() {
            let st = Style::ui(15.0, 600.0);
            centered(c, &mut self.text, w / 2.0, h / 2.0, &format!("Mist Player: {}", self.err), &st, FG);
            self.paint_lights(c);
            return;
        }
        if self.cur.is_empty() {
            self.paint_empty(c);
        } else if !show_video {
            self.paint_artwork(c);
        }
        // big play / pause flash
        if let Some((g, t0)) = self.flash {
            let k = (t0.elapsed().as_secs_f32() / 0.65).min(1.0);
            let a = 1.0 - k * k * k;
            let s = 96.0 * (0.8 + 0.55 * (1.0 - (1.0 - k).powi(3)));
            c.rrect(w / 2.0 - s / 2.0, h / 2.0 - s / 2.0, s, s, s / 2.0, Rgba(0.0, 0.0, 0.0, 0.42 * a));
            gl(c, &mut self.text, w / 2.0 + if g == "play" { 3.0 } else { 0.0 }, h / 2.0, g, 40.0 * s / 96.0, WHITE.alpha(a));
        }
        if chrome {
            self.paint_top(c);
            if !self.cur.is_empty() {
                self.paint_controls(c);
            }
        }
        match self.panel {
            Panel::Speed | Panel::Tracks => self.paint_pop(c),
            Panel::List if !self.cur.is_empty() => self.paint_queue(c),
            Panel::Help => self.paint_help(c),
            _ => {}
        }
        if let Some((g, s, _)) = self.osd.clone() {
            let st = Style::ui(13.0, 600.0);
            let tw = self.text.width(&s, &st) + 17.0 + 9.0;
            let bw = tw + 30.0;
            let (x, y) = (w / 2.0 - bw / 2.0, 76.0);
            c.rrect(x, y, bw, 40.0, 20.0, SMOKE);
            stroke_rrect(c, x + 0.5, y + 0.5, bw - 1.0, 39.0, 19.5, RIM);
            gl(c, &mut self.text, x + 15.0 + 8.5, y + 20.0, g, 17.0, FG);
            c.text(&mut self.text, x + 15.0 + 26.0, y + 20.0, &s, &st, FG);
        }
        if let Some((p, _)) = self.resume_at {
            let st = Style::ui(13.0, 520.0);
            let sb = Style::ui(12.0, 600.0);
            let msg = format!("Wznowiono od {}", time(p));
            let tw = self.text.width(&msg, &st);
            let bw2 = self.text.width("Od początku", &sb) + 18.0;
            let bw = 12.0 + 16.0 + 10.0 + tw + 10.0 + bw2 + 12.0;
            let y = h - 22.0 - 106.0 - 14.0 - 42.0;
            c.rrect(24.0, y, bw, 42.0, 21.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.82));
            stroke_rrect(c, 24.5, y + 0.5, bw - 1.0, 41.0, 20.5, RIM);
            gl(c, &mut self.text, 24.0 + 12.0 + 8.0, y + 21.0, "clock", 16.0, ACC2);
            c.text(&mut self.text, 24.0 + 38.0, y + 21.0, &msg, &st, FG);
            let bx = 24.0 + 38.0 + tw + 10.0;
            let hov = self.hit(bx, y + 7.0, bw2, 28.0, Hit::FromStart);
            c.rrect(bx, y + 7.0, bw2, 28.0, 14.0, WHITE.alpha(if hov { 0.16 } else { 0.09 }));
            c.text(&mut self.text, bx + 9.0, y + 21.0, "Od początku", &sb, FG);
        }
        if self.drop_hover {
            c.rrect(14.0, 14.0, w - 28.0, h - 28.0, 22.0, Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.12));
            stroke_rrect(c, 15.0, 15.0, w - 30.0, h - 30.0, 21.0, Rgba::hex("#0A84FF"));
            gl(c, &mut self.text, w / 2.0, h / 2.0 - 16.0, "plus", 40.0, Rgba::hex("#409CFF"));
            let st = Style::ui(15.0, 600.0);
            let s = if self.cur.is_empty() { "Upuść, aby odtworzyć" } else { "Upuść, aby odtworzyć  ·  Shift: dodaj do kolejki" };
            centered(c, &mut self.text, w / 2.0, h / 2.0 + 24.0, s, &st, FG);
        }
    }

    fn paint_backdrop(&mut self, c: &mut Canvas) {
        // the gradients are the costly part of a repaint: drawn once per size, then copied
        let key = (c.pm.width(), c.pm.height(), self.cur.is_empty());
        if let Some((w, h, e, pm)) = &self.backdrop {
            if (*w, *h, *e) == key {
                c.pm.data_mut().copy_from_slice(pm.data());
                return;
            }
        }
        self.paint_backdrop_fresh(c);
        self.backdrop = Some((key.0, key.1, key.2, c.pm.clone()));
    }
    fn paint_backdrop_fresh(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        c.rrect(0.0, 0.0, w, h, 0.0, Rgba::rgb8(14, 19, 29, 1.0));
        // soft colour blobs (static: no per-frame work)
        let a = if self.cur.is_empty() { 0.18 } else { 0.32 };
        let m = w.max(h);
        for (col, fx, fy, fs) in [(Rgba::hex("#5E5CE6"), 0.10, 0.15, 0.55), (Rgba::hex("#BF5AF2"), 0.75, 0.25, 0.45), (Rgba::hex("#64D2FF"), 0.45, 0.85, 0.4)] {
            let r = m * fs / 2.0;
            let (cx, cy) = (w * fx, h * fy);
            let stops = vec![tiny_skia::GradientStop::new(0.0, sk(col.alpha(a))), tiny_skia::GradientStop::new(1.0, sk(col.alpha(0.0)))];
            if let Some(sh) = tiny_skia::RadialGradient::new(tiny_skia::Point::from_xy(cx, cy), tiny_skia::Point::from_xy(cx, cy), r * 1.4, stops, tiny_skia::SpreadMode::Pad, tiny_skia::Transform::identity()) {
                let p = tiny_skia::Paint { shader: sh, anti_alias: true, ..Default::default() };
                if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
                    c.pm.fill_rect(rect, &p, tiny_skia::Transform::identity(), None);
                }
            }
        }
    }
    fn paint_lights(&mut self, c: &mut Canvas) {
        if self.fs {
            return;
        }
        let cols = [Rgba::hex("#ff5f57"), Rgba::hex("#febc2e"), Rgba::hex("#28c840")];
        let any = matches!(self.hover, Some(Hit::Light(_)));
        for k in 0..3 {
            let x = 18.0 + k as f32 * 21.0;
            self.hit(x - 3.0, 15.0, 19.0, 19.0, Hit::Light(k as u8));
            c.rrect(x, 18.0, 13.0, 13.0, 6.5, cols[k]);
            if any {
                gl(c, &mut self.text, x + 6.5, 24.5, ["close", "minus", "maxi"][k], 10.0, Rgba::rgb8(60, 20, 10, 0.75));
            }
        }
    }
    fn paint_top(&mut self, c: &mut Canvas) {
        let w = self.w;
        if self.video && !self.cur.is_empty() {
            let path = tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w, 104.0).unwrap());
            crate::desktop::fill_lin(c, &path, (0.0, 0.0), (0.0, 104.0), &[(0.0, Rgba(0.0, 0.0, 0.0, 0.55)), (1.0, Rgba(0.0, 0.0, 0.0, 0.0))]);
        }
        self.hit(0.0, 0.0, w, 64.0, Hit::Drag);
        self.paint_lights(c);
        if !self.cur.is_empty() && !(self.video == false) {
            let st = Style::ui(14.0, 600.0);
            let nt = self.nice_title();
            let t = elide(&mut self.text, &nt, &st, w - 260.0);
            let sub = if self.queue.len() > 1 { format!("{} z {}", self.idx + 1, self.queue.len()) } else { String::new() };
            let cy = if sub.is_empty() { 30.0 } else { 24.0 };
            centered(c, &mut self.text, w / 2.0, cy, &t, &st, FG);
            if !sub.is_empty() {
                centered(c, &mut self.text, w / 2.0, 42.0, &sub, &Style::ui(11.0, 450.0), MUTED);
            }
        }
        let help = self.panel == Panel::Help;
        self.rbtn(c, w - 14.0 - 38.0, 32.0, "kbd", Hit::Help, help, false);
        self.rbtn(c, w - 14.0 - 80.0, 32.0, "open", Hit::Open, false, false);
    }

    fn paint_empty(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        let cw = (w - 60.0).min(560.0);
        let rec: Vec<Recent> = self.recents.iter().take(6).cloned().collect();
        let list_h = if rec.is_empty() { 0.0 } else { 8.0 + 26.0 + rec.len() as f32 * 48.0 + 12.0 };
        let total = 92.0 + 18.0 + 34.0 + 8.0 + 20.0 + 18.0 + 40.0 + list_h;
        let mut y = ((h - total) / 2.0).max(60.0);
        let x0 = (w - cw) / 2.0;
        // icon
        let ix = w / 2.0 - 46.0;
        if let Some(p) = crate::gfx::rrect_path(ix, y, 92.0, 92.0, 24.0) {
            crate::desktop::fill_lin(c, &p, (0.0, y), (0.0, y + 92.0), &[(0.0, Rgba::hex("#3a8dff")), (1.0, Rgba::hex("#5E5CE6"))]);
        }
        gl(c, &mut self.text, w / 2.0 + 3.0, y + 46.0, "play", 42.0, WHITE);
        y += 92.0 + 18.0;
        centered(c, &mut self.text, w / 2.0, y + 17.0, "Mist Player", &Style::ui(28.0, 700.0), FG);
        y += 34.0 + 8.0;
        centered(c, &mut self.text, w / 2.0, y + 10.0, "Upuść tu film lub muzykę, albo wybierz plik", &Style::ui(14.0, 450.0), MUTED);
        y += 20.0 + 18.0;
        let st = Style::ui(14.0, 600.0);
        let bw = self.text.width("Otwórz plik…", &st) + 17.0 + 8.0 + 36.0;
        let hov = self.hit(w / 2.0 - bw / 2.0, y, bw, 40.0, Hit::Open);
        c.rrect(w / 2.0 - bw / 2.0, y, bw, 40.0, 20.0, if hov { Rgba::hex("#409CFF") } else { Rgba::hex("#0A84FF") });
        gl(c, &mut self.text, w / 2.0 - bw / 2.0 + 18.0 + 8.5, y + 20.0, "open", 17.0, WHITE);
        c.text(&mut self.text, w / 2.0 - bw / 2.0 + 18.0 + 25.0, y + 20.0, "Otwórz plik…", &st, WHITE);
        y += 40.0 + 8.0;
        if rec.is_empty() {
            return;
        }
        c.text(&mut self.text, x0 + 6.0, y + 13.0, "Ostatnio odtwarzane", &Style::ui(12.0, 600.0), FAINT);
        y += 26.0;
        let lh = rec.len() as f32 * 48.0 + 12.0;
        c.rrect(x0, y, cw, lh, 18.0, WHITE.alpha(0.05));
        stroke_rrect(c, x0 + 0.5, y + 0.5, cw - 1.0, lh - 1.0, 17.5, HAIR);
        let name = Style::ui(13.0, 520.0);
        let tm = Style::ui(12.0, 450.0).tnum();
        for (i, r) in rec.iter().enumerate() {
            let ry = y + 6.0 + i as f32 * 48.0;
            let hov = self.hit(x0 + 6.0, ry, cw - 12.0, 48.0, Hit::Recent(i));
            let xh = self.hover == Some(Hit::RecentForget(i));
            if hov || xh {
                c.rrect(x0 + 6.0, ry, cw - 12.0, 48.0, 12.0, WHITE.alpha(0.07));
            }
            gl(c, &mut self.text, x0 + 6.0 + 24.0, ry + 24.0, if is_audio(&r.path) { "music" } else { "movie" }, 20.0, FG2);
            let tw = cw - 12.0 - 46.0 - 90.0;
            let s = elide(&mut self.text, &pretty(&r.path), &name, tw);
            let has_bar = r.dur > 0.0 && r.pos > 0.0;
            c.text(&mut self.text, x0 + 6.0 + 46.0, ry + if has_bar { 19.0 } else { 24.0 }, &s, &name, FG);
            if has_bar {
                c.rrect(x0 + 52.0, ry + 31.0, tw * 0.6, 3.0, 1.5, WHITE.alpha(0.12));
                c.rrect(x0 + 52.0, ry + 31.0, tw * 0.6 * (r.pos / r.dur).min(1.0) as f32, 3.0, 1.5, ACC);
            }
            let ts = if r.pos > 0.0 { format!("{} / {}", time(r.pos / 1000.0), time(r.dur / 1000.0)) } else { time(r.dur / 1000.0) };
            let tww = self.text.width(&ts, &tm);
            let right = x0 + cw - 6.0 - if hov || xh { 44.0 } else { 14.0 };
            c.text(&mut self.text, right - tww, ry + 24.0, &ts, &tm, FAINT);
            if hov || xh {
                let bx = x0 + cw - 6.0 - 36.0;
                if self.hit(bx, ry + 9.0, 30.0, 30.0, Hit::RecentForget(i)) {
                    c.rrect(bx, ry + 9.0, 30.0, 30.0, 15.0, WHITE.alpha(0.10));
                }
                gl(c, &mut self.text, bx + 15.0, ry + 24.0, "x", 15.0, FG2);
            }
        }
    }

    fn paint_artwork(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        let s = (h * 0.34).min(260.0);
        let cy = h / 2.0 - 40.0;
        let y = cy - (s + 22.0 + 30.0 + 20.0) / 2.0;
        let x = w / 2.0 - s / 2.0;
        let k = if self.paused { 0.9 } else { 1.0 };
        let (sx, sy, ss) = (x + s * (1.0 - k) / 2.0, y + s * (1.0 - k) / 2.0, s * k);
        crate::glass::shadow(c, sx, sy + 12.0, ss, ss, ss * 0.18, 40.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.4));
        if let Some(p) = crate::gfx::rrect_path(sx, sy, ss, ss, ss * 0.18) {
            crate::desktop::fill_lin(c, &p, (0.0, sy), (0.0, sy + ss), &[(0.0, Rgba::hex("#c46af4")), (1.0, Rgba::hex("#5E5CE6"))]);
        }
        gl(c, &mut self.text, w / 2.0, sy + ss / 2.0, "music", ss * 0.38, WHITE.alpha(0.92));
        let mut ty = y + s + 22.0 + 15.0;
        let st = Style::ui(24.0, 650.0);
        let nt = self.nice_title();
        let t = elide(&mut self.text, &nt, &st, w - 80.0);
        centered(c, &mut self.text, w / 2.0, ty, &t, &st, FG);
        ty += 30.0;
        let sub = [self.artist.clone(), self.album.clone()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" — ");
        if !sub.is_empty() {
            let st = Style::ui(15.0, 450.0);
            let s2 = elide(&mut self.text, &sub, &st, w - 80.0);
            centered(c, &mut self.text, w / 2.0, ty, &s2, &st, MUTED);
        }
    }

    fn controls_rect(&self) -> (f32, f32, f32, f32) {
        let cw = (self.w - 48.0).min(860.0);
        ((self.w - cw) / 2.0, self.h - 22.0 - 106.0, cw, 106.0)
    }
    fn paint_controls(&mut self, c: &mut Canvas) {
        let (x, y, cw, ch) = self.controls_rect();
        crate::glass::shadow(c, x, y + 8.0, cw, ch, 28.0, 30.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.30));
        c.rrect(x, y, cw, ch, 28.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.72));
        stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 27.5, RIM);
        self.hit(x, y, cw, ch, Hit::Card);
        // seek row
        let tm = Style::ui(12.0, 550.0).tnum();
        let sy = y + 14.0 + 11.0;
        let cur = self.seek_preview.map(|f| f * self.dur).unwrap_or(self.pos);
        c.text(&mut self.text, x + 22.0, sy, &time(cur), &tm, FG2);
        let end = if self.remaining { format!("−{}", time(self.dur - self.pos)) } else { time(self.dur) };
        let ew = self.text.width(&end, &tm);
        c.text(&mut self.text, x + cw - 22.0 - ew, sy, &end, &tm, FG2);
        self.hit(x + cw - 22.0 - 58.0, sy - 11.0, 58.0, 22.0, Hit::Remaining);
        let (bx, bw) = (x + 22.0 + 52.0 + 8.0, cw - 44.0 - 52.0 - 58.0 - 16.0);
        let frac = if self.dur > 0.0 { cur / self.dur } else { 0.0 };
        let hover_v = if self.hover == Some(Hit::Seek) || self.drag == Some(Hit::Seek) { Some(((self.mouse.0 - bx) / bw).clamp(0.0, 1.0) as f64) } else { None };
        self.bar(c, bx, sy, bw, frac, Hit::Seek, hover_v);
        if let (Some(hv), true) = (self.seek_preview.or(hover_v), self.dur > 0.0) {
            let s = time(hv * self.dur);
            let st = Style::ui(12.0, 600.0).tnum();
            let tw = self.text.width(&s, &st) + 16.0;
            let tx = (bx + bw * hv as f32 - tw / 2.0).clamp(bx - 20.0, bx + bw - tw + 20.0);
            c.rrect(tx, sy - 34.0 - 12.0, tw, 24.0, 12.0, Rgba(20.0 / 255.0, 26.0 / 255.0, 38.0 / 255.0, 0.95));
            stroke_rrect(c, tx + 0.5, sy - 46.0 + 0.5, tw - 1.0, 23.0, 11.5, RIM);
            c.text(&mut self.text, tx + 8.0, sy - 34.0, &s, &st, FG);
        }
        // buttons row
        let cy = y + 44.0 + 27.0;
        let mut lx = x + 14.0;
        let vg = self.vol_glyph();
        lx += self.rbtn(c, lx, cy, vg, Hit::Mute, false, false) + 6.0;
        let vw = (cw * 0.12).min(110.0);
        let v = if self.muted { 0.0 } else { self.volume };
        self.bar(c, lx, cy, vw, v, Hit::Vol, None);
        let widths = 38.0 + 6.0 + 38.0 + 6.0 + 52.0 + 6.0 + 38.0 + 6.0 + 38.0;
        let mut mx = x + cw / 2.0 - widths / 2.0;
        mx += self.rbtn(c, mx, cy, "prev", Hit::Prev, false, false) + 6.0;
        mx += self.rbtn(c, mx, cy, "back10", Hit::Back10, false, false) + 6.0;
        let pg = if self.paused { "play" } else { "pause" };
        mx += self.rbtn(c, mx, cy, pg, Hit::Play, false, true) + 6.0;
        mx += self.rbtn(c, mx, cy, "fwd10", Hit::Fwd10, false, false) + 6.0;
        self.rbtn(c, mx, cy, "next", Hit::Next, false, false);
        // right: speed, tracks, queue, fullscreen
        let mut rx = x + cw - 14.0 - 38.0;
        let fg = if self.fs { "unfull" } else { "full" };
        self.rbtn(c, rx, cy, fg, Hit::Full, false, false);
        rx -= 40.0;
        let on = self.panel == Panel::List;
        self.rbtn(c, rx, cy, "list", Hit::List, on, false);
        rx -= 40.0;
        let on = self.panel == Panel::Tracks || self.of_kind("sub").iter().any(|t| t.selected);
        self.rbtn(c, rx, cy, "cc", Hit::Tracks, on, false);
        let st = Style::ui(14.0, 600.0).tnum();
        let label = rate(self.speed);
        let lw = self.text.width(&label, &st) + 22.0;
        rx -= lw + 2.0;
        let on = self.panel == Panel::Speed || (self.speed - 1.0).abs() > 0.01;
        let hov = self.hit(rx, cy - 19.0, lw, 38.0, Hit::Speed);
        if hov || on {
            c.rrect(rx, cy - 19.0, lw, 38.0, 19.0, WHITE.alpha(if on { 0.12 } else { 0.10 }));
        }
        c.text(&mut self.text, rx + 11.0, cy, &label, &st, if on { ACC2 } else if hov { FG } else { FG2 });
    }

    fn paint_pop(&mut self, c: &mut Canvas) {
        let (x, y, cw, _) = self.controls_rect();
        let mut rows: Vec<Row> = vec![];
        if self.panel == Panel::Speed {
            for r in SPEEDS {
                rows.push(Row::Pick(if r == 1.0 { "Normalna".into() } else { rate(r) }, (self.speed - r).abs() < 0.01, RowAct::Speed(r)));
            }
        } else {
            rows.push(Row::Head("AUDIO".into()));
            let a = self.of_kind("audio");
            if a.is_empty() {
                rows.push(Row::Pick("Brak".into(), false, RowAct::None));
            }
            for (i, t) in a.iter().enumerate() {
                rows.push(Row::Pick(t.name(i, "Ścieżka"), t.selected || a.len() == 1, RowAct::Aid(t.id)));
            }
            rows.push(Row::Head("NAPISY".into()));
            let s = self.of_kind("sub");
            rows.push(Row::Pick("Wyłączone".into(), !s.iter().any(|t| t.selected), RowAct::Sid(-1)));
            for (i, t) in s.iter().enumerate() {
                rows.push(Row::Pick(t.name(i, "Napisy"), t.selected, RowAct::Sid(t.id)));
            }
        }
        let ph: f32 = rows.iter().map(|r| if matches!(r, Row::Head(_)) { 28.0 } else { 34.0 }).sum::<f32>() + 16.0;
        let pw = 270.0;
        let (px, py) = (x + cw - pw, (y - 10.0 - ph).max(70.0));
        c.rrect(px, py, pw, ph, 20.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.92));
        stroke_rrect(c, px + 0.5, py + 0.5, pw - 1.0, ph - 1.0, 19.5, RIM);
        self.hit(px, py, pw, ph, Hit::Card);
        let mut ry = py + 8.0;
        let st = Style::ui(13.0, 500.0);
        for (i, r) in rows.iter().enumerate() {
            match r {
                Row::Head(t) => {
                    c.text(&mut self.text, px + 18.0, ry + 15.0, t, &Style::ui(11.0, 650.0), FAINT);
                    ry += 28.0;
                }
                Row::Pick(t, on, _) => {
                    let hov = self.hit(px + 8.0, ry, pw - 16.0, 34.0, Hit::Item(i));
                    if hov {
                        c.rrect(px + 8.0, ry, pw - 16.0, 34.0, 10.0, WHITE.alpha(0.08));
                    }
                    if *on {
                        gl(c, &mut self.text, px + 8.0 + 18.0, ry + 17.0, "check", 15.0, ACC2);
                    }
                    let s = elide(&mut self.text, t, &st, pw - 16.0 - 44.0);
                    c.text(&mut self.text, px + 8.0 + 36.0, ry + 17.0, &s, &st, if *on { FG } else { FG2 });
                    ry += 34.0;
                }
            }
        }
        self.rows = rows;
    }

    fn paint_queue(&mut self, c: &mut Canvas) {
        let (_, cy, _, _) = self.controls_rect();
        let sw = (self.w * 0.42).min(360.0);
        let (x, y) = (self.w - sw - 22.0, 64.0);
        let sh = (cy - 12.0 - y).max(120.0);
        c.rrect(x, y, sw, sh, 24.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.90));
        stroke_rrect(c, x + 0.5, y + 0.5, sw - 1.0, sh - 1.0, 23.5, RIM);
        self.hit(x, y, sw, sh, Hit::Card);
        c.text(&mut self.text, x + 18.0, y + 20.0, "Kolejka", &Style::ui(16.0, 700.0), FG);
        let n = self.queue.len();
        let cnt = format!("{n} {}", if n == 1 { "plik" } else if (2..5).contains(&(n % 10)) && !(12..15).contains(&(n % 100)) { "pliki" } else { "plików" });
        c.text(&mut self.text, x + 18.0, y + 38.0, &cnt, &Style::ui(11.0, 450.0), MUTED);
        let mut bx = x + sw - 10.0 - 38.0;
        self.rbtn(c, bx, y + 27.0, "plus", Hit::Add, false, false);
        bx -= 40.0;
        let rg = if self.repeat == 2 { "repeat1" } else { "repeat" };
        let ron = self.repeat != 0;
        self.rbtn(c, bx, y + 27.0, rg, Hit::Repeat, ron, false);
        bx -= 40.0;
        let son = self.shuffle;
        self.rbtn(c, bx, y + 27.0, "shuffle", Hit::Shuffle, son, false);
        c.rrect(x + 14.0, y + 54.0, sw - 28.0, 1.0, 0.0, HAIR);
        // rows (scrolled so the current one is visible)
        let top = y + 60.0;
        let avail = sh - 68.0;
        let vis = (avail / 46.0).floor().max(1.0) as usize;
        let first = if (self.idx.max(0) as usize) >= vis { self.idx as usize + 1 - vis } else { 0 };
        let st = Style::ui(13.0, 450.0);
        let stn = Style::ui(13.0, 600.0);
        let small = Style::ui(10.0, 450.0);
        for (k, i) in (first..n.min(first + vis)).enumerate() {
            let ry = top + k as f32 * 46.0;
            let now = i as i32 == self.idx;
            let hov = self.hit(x + 8.0, ry, sw - 16.0, 46.0, Hit::Queue(i));
            let xh = self.hover == Some(Hit::QueueRemove(i));
            if now || hov || xh {
                c.rrect(x + 8.0, ry, sw - 16.0, 46.0, 12.0, WHITE.alpha(if now { 0.11 } else { 0.06 }));
            }
            if now {
                gl(c, &mut self.text, x + 8.0 + 21.0, ry + 23.0, if self.paused { "pause" } else { "vol2" }, 16.0, ACC2);
            } else {
                centered(c, &mut self.text, x + 8.0 + 21.0, ry + 23.0, &(i + 1).to_string(), &Style::ui(12.0, 450.0).tnum(), FAINT);
            }
            let p = self.queue[i].clone();
            let s = elide(&mut self.text, &pretty(&p), if now { &stn } else { &st }, sw - 16.0 - 40.0 - 40.0);
            c.text(&mut self.text, x + 8.0 + 40.0, ry + 17.0, &s, if now { &stn } else { &st }, if now { FG } else { FG2 });
            let res = self.resume_of(&p);
            let meta = format!("{}{}", ext(&p).to_uppercase(), if res > 0.0 && !now { format!(" · wznów od {}", time(res)) } else { String::new() });
            c.text(&mut self.text, x + 8.0 + 40.0, ry + 33.0, &meta, &small, FAINT);
            if hov || xh {
                let bx = x + sw - 8.0 - 4.0 - 30.0;
                if self.hit(bx, ry + 8.0, 30.0, 30.0, Hit::QueueRemove(i)) {
                    c.rrect(bx, ry + 8.0, 30.0, 30.0, 15.0, WHITE.alpha(0.10));
                }
                gl(c, &mut self.text, bx + 15.0, ry + 23.0, "x", 14.0, FG2);
            }
        }
    }

    fn paint_help(&mut self, c: &mut Canvas) {
        let rows = [
            ("Spacja / K", "Odtwarzaj / pauza"),
            ("← / →", "−5 s / +5 s  (Shift: 30 s)"),
            ("J / L", "−10 s / +10 s"),
            ("↑ / ↓ / kółko", "Głośność"),
            ("M", "Wycisz"),
            ("F / dwuklik", "Pełny ekran"),
            ("N / P", "Następny / poprzedni"),
            ("[ / ] / Backspace", "Wolniej / szybciej / normalnie"),
            ("S / A", "Napisy / ścieżka audio"),
            ("0 – 9", "Skok do 0–90%"),
            (", / .", "Klatka wstecz / dalej"),
            ("Z", "Wypełnij okno / cały obraz"),
            ("Q", "Kolejka"),
            ("O", "Otwórz plik"),
            ("Ctrl+W", "Zamknij"),
        ];
        let (cw, ch) = (520.0, 20.0 + 34.0 + rows.len() as f32 * 28.0 + 20.0);
        let (x, y) = ((self.w - cw) / 2.0, ((self.h - ch) / 2.0).max(10.0));
        c.rrect(x, y, cw, ch, 26.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.92));
        stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 25.5, RIM);
        self.hit(x, y, cw, ch, Hit::Card);
        c.text(&mut self.text, x + 24.0, y + 32.0, "Skróty klawiszowe", &Style::ui(17.0, 700.0), FG);
        let st = Style::ui(13.0, 450.0);
        let ks = Style::ui(12.0, 600.0);
        for (i, (k, d)) in rows.iter().enumerate() {
            let ry = y + 20.0 + 34.0 + i as f32 * 28.0 + 14.0;
            c.text(&mut self.text, x + 24.0, ry, d, &st, FG2);
            let kw = self.text.width(k, &ks) + 16.0;
            let kx = x + cw - 24.0 - kw;
            c.rrect(kx, ry - 11.0, kw, 22.0, 7.0, WHITE.alpha(0.08));
            stroke_rrect(c, kx + 0.5, ry - 10.5, kw - 1.0, 21.0, 6.5, HAIR);
            c.text(&mut self.text, kx + 8.0, ry, k, &ks, FG);
        }
    }
}

pub fn run(_args: &[String]) -> i32 {
    let args: Vec<String> = std::env::args().skip(2).collect();
    if appwin::forward("player", &args) {
        return 0;
    }
    if args.first().map(|s| s.as_str()) == Some("--test") {
        return 1;
    }
    appwin::run("player", Box::new(Player::new()))
}
