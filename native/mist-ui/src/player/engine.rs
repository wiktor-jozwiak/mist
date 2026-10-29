// The playback engine of Mist Player: libmpv and the GL plane it draws the picture on.
// - mpv starts with the first file (an empty window runs no mpv at all).
// - The EGL context and mpv's render context exist only while a file with a picture plays: audio runs with
//   no GL (on NVIDIA the GL context plus the CUDA decoder are ~250 MB RSS). The context is made surfaceless,
//   before the window's video plane exists, so mpv opens the file while the window maps.
// - Frames are paced by the plane's frame callbacks: at most one per compositor refresh, a covered window
//   once a second (mist-comp), a minimized one none (mpv drops those frames, the sound goes on).
// - time-pos is observed only while something shows it (the controls): a playing video with hidden controls
//   wakes this thread for its frames only.
// - No synchronous property reads on the way of mpv's events: this thread renders, and while mpv reconfigures
//   its VO (a new file) it waits for a render with its core locked, so a mpv_get_property here stalls both
//   until mpv's 200 ms timeout (the old player lost those 200 ms at every start). Track list and metadata are
//   observed (as JSON), the position is read with mpv_get_property_async.
// vo=dmabuf-wayland (zero copy, no GL) is no option here: it needs mpv's own window, and with the NVIDIA
// VA-API driver vaExportSurfaceHandle fails ("Video: no video"), measured 2026-10-29.
use super::ffi::*;
use super::Wake;
use crate::appwin::VideoSurface;
use std::any::Any;
use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub type Tx = calloop::channel::Sender<Box<dyn Any + Send>>;

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

// observed properties (reply ids)
const P_POS: u64 = 1;
const PROPS: [(&str, i32); 10] = [
    ("duration", FMT_DOUBLE),
    ("pause", FMT_FLAG),
    ("eof-reached", FMT_FLAG),
    ("vo-configured", FMT_FLAG),
    ("speed", FMT_DOUBLE),
    ("media-title", FMT_STRING),
    ("track-list", FMT_STRING),
    ("metadata", FMT_STRING),
    ("volume", FMT_DOUBLE),
    ("mute", FMT_FLAG),
];

pub enum Event {
    Pos(f64),
    Dur(f64),
    Pause(bool),
    Eof,
    Video(bool),
    Speed(f64),
    Title(String),
    Tracks(Vec<Track>),
    // artist, album
    Meta(String, String),
    // an asynchronous read came back (its id, the value)
    Reply(u64, f64),
    Volume(f64),
    Mute(bool),
    Loaded,
    Restart,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub lang: String,
    pub codec: String,
    pub selected: bool,
    pub external: bool,
}

#[derive(Default, Clone, Copy)]
pub struct Stat {
    // render-update callbacks, frames drawn, frame callbacks
    pub updates: u64,
    pub frames: u64,
    pub callbacks: u64,
}

struct Gl {
    dpy: EGLDisplay,
    cfg: EGLConfig,
    ctx: EGLContext,
    surfaceless: bool,
    // the plane: wl_egl_window, EGL surface, its size
    win: Option<(*mut c_void, EGLSurface, i32, i32)>,
    vs: Option<VideoSurface>,
}

// track-list as mpv gives it in a string (JSON)
pub fn parse_tracks(json: &str) -> Vec<Track> {
    let Ok(serde_json::Value::Array(a)) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    a.iter()
        .map(|t| {
            let s = |k: &str| t[k].as_str().unwrap_or("").to_string();
            Track { id: t["id"].as_i64().unwrap_or(0), kind: s("type"), title: s("title"), lang: s("lang"), codec: s("codec"), selected: t["selected"].as_bool().unwrap_or(false), external: t["external"].as_bool().unwrap_or(false) }
        })
        .collect()
}
// (artist, album) from metadata (JSON; the keys' case depends on the container)
pub fn parse_meta(json: &str) -> (String, String) {
    let Ok(serde_json::Value::Object(m)) = serde_json::from_str::<serde_json::Value>(json) else { return Default::default() };
    let get = |k: &str| m.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).and_then(|(_, v)| v.as_str()).unwrap_or("").to_string();
    let artist = Some(get("artist")).filter(|s| !s.is_empty()).unwrap_or_else(|| get("album_artist"));
    (artist, get("album"))
}

pub struct Engine {
    libs: Box<Libs>,
    mpv: Handle,
    rctx: Handle,
    gl: Option<Gl>,
    tx: Tx,
    pos_watch: bool,
    // a frame callback is out (the compositor has not shown the last frame yet)
    frame_wait: bool,
    // mpv has a frame not drawn yet
    dirty: bool,
    pub size: (i32, i32),
    pub stat: Stat,
    // how long the last GL bring-up took (ms)
    pub gl_ms: f64,
}

fn opt(l: &Libs, m: Handle, k: &str, v: &str) {
    let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap_or_default());
    unsafe { (l.mpv.mpv_set_option_string)(m, k.as_ptr(), v.as_ptr()) };
}

impl Engine {
    pub fn new(libs: Box<Libs>, tx: Tx, volume: f64, muted: bool) -> Result<Engine, String> {
        let l = &libs;
        let m = unsafe { (l.mpv.mpv_create)() };
        if m.is_null() {
            return Err("mpv_create nie działa".into());
        }
        let home = std::env::var("HOME").unwrap_or_default();
        for (k, v) in [
            ("config", "no"),
            ("vo", "libmpv"),
            ("hwdec", "auto-safe"),
            ("keep-open", "yes"),
            ("idle", "yes"),
            ("input-default-bindings", "no"),
            ("input-builtin-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("terminal", "no"),
            ("ytdl", "no"),
            // no Lua at all: each built-in script is its own VM and thread
            ("osc", "no"),
            ("load-scripts", "no"),
            ("load-stats-overlay", "no"),
            ("load-console", "no"),
            ("load-auto-profiles", "no"),
            ("load-select", "no"),
            ("load-positioning", "no"),
            ("load-commands", "no"),
            ("load-context-menu", "no"),
            ("osd-bar", "no"),
            ("audio-client-name", "mist-player"),
            // a cover in an audio file is no video track: audio needs no GL plane
            ("audio-display", "no"),
            ("sub-auto", "fuzzy"),
            ("sub-file-paths", "subs:Subs:napisy:Napisy"),
            ("audio-file-auto", "no"),
            ("sub-font", "SF Pro Display"),
            ("sub-font-size", "46"),
            ("sub-border-size", "2.5"),
            ("sub-shadow-offset", "1"),
            ("slang", "pl,pol,en,eng"),
            ("alang", "pl,pol,en,eng,ja,jpn"),
            // streams: a bounded read-ahead (local files have no cache)
            ("demuxer-max-bytes", "64MiB"),
            ("demuxer-max-back-bytes", "16MiB"),
            ("screenshot-directory", &format!("{home}/Obrazy")),
            ("screenshot-template", "Mist Player %F %wH-%wM-%wS"),
        ] {
            opt(l, m, k, v);
        }
        if let Ok(v) = std::env::var("MIST_PLAYER_MPV") {
            // experiments: MIST_PLAYER_MPV="hwdec=nvdec msg-level=all=v"
            for kv in v.split_whitespace() {
                if let Some((k, v)) = kv.split_once('=') {
                    opt(l, m, k, v);
                }
            }
        }
        opt(l, m, "volume", &format!("{}", volume * 100.0));
        opt(l, m, "mute", if muted { "yes" } else { "no" });
        if unsafe { (l.mpv.mpv_initialize)(m) } < 0 {
            unsafe { (l.mpv.mpv_terminate_destroy)(m) };
            return Err("mpv_initialize nie działa".into());
        }
        for (i, (name, fmt)) in PROPS.iter().enumerate() {
            let c = CString::new(*name).unwrap();
            unsafe { (l.mpv.mpv_observe_property)(m, i as u64 + 2, c.as_ptr(), *fmt) };
        }
        // the callbacks keep their own sender (one engine per process: never freed)
        let ctx = Box::into_raw(Box::new(tx.clone())) as *mut c_void;
        unsafe { (l.mpv.mpv_set_wakeup_callback)(m, Some(on_mpv_wake), ctx) };
        Ok(Engine { libs, mpv: m, rctx: std::ptr::null_mut(), gl: None, tx, pos_watch: false, frame_wait: false, dirty: false, size: (1, 1), stat: Stat::default(), gl_ms: 0.0 })
    }

    // ------------------------------------------------------------ commands and properties
    pub fn cmd(&self, args: &[&str]) {
        let cs: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap_or_default()).collect();
        let mut ptrs: Vec<*const c_char> = cs.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe { (self.libs.mpv.mpv_command_async)(self.mpv, 0, ptrs.as_mut_ptr()) };
    }
    pub fn set(&self, k: &str, v: &str) {
        let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap_or_default());
        unsafe { (self.libs.mpv.mpv_set_property_string)(self.mpv, k.as_ptr(), v.as_ptr()) };
    }
    pub fn get_str(&self, k: &str) -> Option<String> {
        let k = CString::new(k).unwrap();
        unsafe {
            let p = (self.libs.mpv.mpv_get_property_string)(self.mpv, k.as_ptr());
            if p.is_null() {
                return None;
            }
            let s = cstr(p);
            (self.libs.mpv.mpv_free)(p as *mut c_void);
            Some(s)
        }
    }
    pub fn get_i64(&self, k: &str) -> Option<i64> {
        let k = CString::new(k).unwrap();
        let mut v: i64 = 0;
        let r = unsafe { (self.libs.mpv.mpv_get_property)(self.mpv, k.as_ptr(), FMT_INT64, &mut v as *mut i64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    pub fn get_f64(&self, k: &str) -> Option<f64> {
        let k = CString::new(k).unwrap();
        let mut v: f64 = 0.0;
        let r = unsafe { (self.libs.mpv.mpv_get_property)(self.mpv, k.as_ptr(), FMT_DOUBLE, &mut v as *mut f64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    // time-pos events only while the position is on screen
    pub fn watch_pos(&mut self, on: bool) {
        if on == self.pos_watch {
            return;
        }
        self.pos_watch = on;
        unsafe {
            if on {
                let c = CString::new("time-pos").unwrap();
                (self.libs.mpv.mpv_observe_property)(self.mpv, P_POS, c.as_ptr(), FMT_DOUBLE);
            } else {
                (self.libs.mpv.mpv_unobserve_property)(self.mpv, P_POS);
            }
        }
    }
    pub fn watching_pos(&self) -> bool {
        self.pos_watch
    }
    // the position, answered as Event::Reply(id, seconds)
    pub fn ask_pos(&self, id: u64) {
        let c = CString::new("time-pos").unwrap();
        unsafe { (self.libs.mpv.mpv_get_property_async)(self.mpv, id, c.as_ptr(), FMT_DOUBLE) };
    }

    pub fn events(&mut self) -> Vec<Event> {
        MPV_PENDING.store(false, Ordering::Release);
        let mut out = vec![];
        loop {
            let e = unsafe { &*(self.libs.mpv.mpv_wait_event)(self.mpv, 0.0) };
            match e.event_id {
                EV_NONE | EV_SHUTDOWN => break,
                EV_PROPERTY_CHANGE => {
                    let p = unsafe { &*(e.data as *const mpv_event_property) };
                    let has = !p.data.is_null() && p.format != 0;
                    let dbl = || if has && p.format == FMT_DOUBLE { unsafe { *(p.data as *const f64) } } else { 0.0 };
                    let flag = || has && p.format == FMT_FLAG && unsafe { *(p.data as *const i32) } != 0;
                    let s = || if has && p.format == FMT_STRING { cstr(unsafe { *(p.data as *const *const c_char) }) } else { String::new() };
                    out.push(match e.reply_userdata {
                        P_POS if has => Event::Pos(dbl()),
                        2 => Event::Dur(dbl()),
                        3 => Event::Pause(flag()),
                        4 if flag() => Event::Eof,
                        5 => Event::Video(flag()),
                        6 => Event::Speed(if has { dbl() } else { 1.0 }),
                        7 => Event::Title(s()),
                        8 => Event::Tracks(parse_tracks(&s())),
                        9 => {
                            let (a, b) = parse_meta(&s());
                            Event::Meta(a, b)
                        }
                        10 if has => Event::Volume((dbl() / 100.0).clamp(0.0, 1.0)),
                        11 => Event::Mute(flag()),
                        _ => continue,
                    });
                }
                EV_GET_PROPERTY_REPLY => {
                    let p = unsafe { &*(e.data as *const mpv_event_property) };
                    if e.error >= 0 && !p.data.is_null() && p.format == FMT_DOUBLE {
                        out.push(Event::Reply(e.reply_userdata, unsafe { *(p.data as *const f64) }));
                    }
                }
                EV_FILE_LOADED => out.push(Event::Loaded),
                EV_PLAYBACK_RESTART => out.push(Event::Restart),
                EV_END_FILE => {
                    let ef = unsafe { &*(e.data as *const mpv_event_end_file) };
                    if ef.reason == END_ERROR {
                        out.push(Event::Failed(cstr(unsafe { (self.libs.mpv.mpv_error_string)(ef.error) })));
                    }
                }
                _ => {}
            }
        }
        out
    }

    // ------------------------------------------------------------ the GL plane
    pub fn gl_ready(&self) -> bool {
        !self.rctx.is_null()
    }
    pub fn gl_up(&self) -> bool {
        self.gl.is_some()
    }
    // EGL context (+ mpv's render context when EGL can be current without a surface)
    pub fn gl_open(&mut self, display: *mut c_void) -> Result<(), String> {
        if self.gl.is_some() {
            return Ok(());
        }
        let t = Instant::now();
        let e = &self.libs.egl;
        let gles = std::env::var_os("MIST_PLAYER_GLES").is_some();
        unsafe {
            let dpy = (e.eglGetDisplay)(display);
            if dpy.is_null() || (e.eglInitialize)(dpy, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                return Err("EGL: brak wyświetlacza".into());
            }
            super::trace("egl initialized");
            (e.eglBindAPI)(if gles { EGL_OPENGL_ES_API } else { EGL_OPENGL_API });
            let rt = if gles { EGL_OPENGL_ES3_BIT } else { EGL_OPENGL_BIT };
            let attrs = [EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 0, EGL_SURFACE_TYPE, EGL_WINDOW_BIT, EGL_RENDERABLE_TYPE, rt, EGL_NONE];
            let mut cfg: EGLConfig = std::ptr::null_mut();
            let mut n = 0;
            if (e.eglChooseConfig)(dpy, attrs.as_ptr(), &mut cfg, 1, &mut n) == 0 || n < 1 {
                return Err("EGL: brak konfiguracji".into());
            }
            let cattrs = if gles { vec![EGL_CONTEXT_MAJOR_VERSION, 3, EGL_NONE] } else { vec![EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 3, EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT, EGL_NONE] };
            let mut ctx = (e.eglCreateContext)(dpy, cfg, std::ptr::null_mut(), cattrs.as_ptr());
            if ctx.is_null() {
                ctx = (e.eglCreateContext)(dpy, cfg, std::ptr::null_mut(), [EGL_NONE].as_ptr());
            }
            if ctx.is_null() {
                return Err("EGL: brak kontekstu".into());
            }
            super::trace("egl context");
            let surfaceless = cstr((e.eglQueryString)(dpy, EGL_EXTENSIONS)).contains("EGL_KHR_surfaceless_context");
            self.gl = Some(Gl { dpy, cfg, ctx, surfaceless, win: None, vs: None });
            if surfaceless && (e.eglMakeCurrent)(dpy, std::ptr::null_mut(), std::ptr::null_mut(), ctx) != 0 {
                self.render_ctx()?;
            }
        }
        self.gl_ms = t.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }
    fn render_ctx(&mut self) -> Result<(), String> {
        if !self.rctx.is_null() {
            return Ok(());
        }
        let l = &self.libs;
        let api = CString::new("opengl").unwrap();
        let mut init = mpv_opengl_init_params { get_proc_address: get_proc, get_proc_address_ctx: &l.egl as *const Egl as *mut c_void };
        let mut params = [
            mpv_render_param { kind: RP_API_TYPE, data: api.as_ptr() as *mut c_void },
            mpv_render_param { kind: RP_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
            mpv_render_param { kind: 0, data: std::ptr::null_mut() },
        ];
        let mut rc: Handle = std::ptr::null_mut();
        if unsafe { (l.mpv.mpv_render_context_create)(&mut rc, self.mpv, params.as_mut_ptr()) } < 0 {
            return Err("mpv: brak renderowania OpenGL".into());
        }
        self.rctx = rc;
        let c = Box::into_raw(Box::new(self.tx.clone())) as *mut c_void;
        unsafe { (l.mpv.mpv_render_context_set_update_callback)(rc, Some(on_render_wake), c) };
        Ok(())
    }
    // the window's video plane appeared: an EGL window surface on it, then the current frame
    pub fn attach(&mut self, vs: VideoSurface) -> Result<(), String> {
        self.detach();
        let (w, h) = (self.size.0.max(1), self.size.1.max(1));
        let Some(g) = self.gl.as_mut() else { return Ok(()) };
        let (e, we) = (&self.libs.egl, &self.libs.wegl);
        unsafe {
            let ewin = (we.wl_egl_window_create)(vs.surface, w, h);
            let surf = (e.eglCreateWindowSurface)(g.dpy, g.cfg, ewin, std::ptr::null());
            if surf.is_null() {
                (we.wl_egl_window_destroy)(ewin);
                return Err("EGL: brak powierzchni".into());
            }
            (e.eglMakeCurrent)(g.dpy, surf, surf, g.ctx);
            // never block the loop in eglSwapBuffers: frame callbacks pace the frames
            (e.eglSwapInterval)(g.dpy, 0);
            g.win = Some((ewin, surf, w, h));
            g.vs = Some(vs);
        }
        self.render_ctx()?;
        self.frame_wait = false;
        self.dirty = true;
        self.draw();
        Ok(())
    }
    // the plane goes (the window closes or no longer shows video)
    pub fn detach(&mut self) {
        let Some(g) = self.gl.as_mut() else { return };
        let Some((ewin, surf, _, _)) = g.win.take() else { return };
        g.vs = None;
        let (e, we) = (&self.libs.egl, &self.libs.wegl);
        unsafe {
            if g.surfaceless {
                (e.eglMakeCurrent)(g.dpy, std::ptr::null_mut(), std::ptr::null_mut(), g.ctx);
            } else {
                (e.eglMakeCurrent)(g.dpy, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            }
            (e.eglDestroySurface)(g.dpy, surf);
            (we.wl_egl_window_destroy)(ewin);
        }
        self.frame_wait = false;
    }
    // no more picture: the render context (mpv drops its VO and the hardware decoder), EGL, all of it
    pub fn gl_close(&mut self) {
        let Some((dpy, ctx, surf)) = self.gl.as_ref().map(|g| (g.dpy, g.ctx, g.win.map(|w| w.1))) else { return };
        let e = &self.libs.egl;
        unsafe {
            if !self.rctx.is_null() {
                match surf {
                    Some(s) => (e.eglMakeCurrent)(dpy, s, s, ctx),
                    None => (e.eglMakeCurrent)(dpy, std::ptr::null_mut(), std::ptr::null_mut(), ctx),
                };
                (self.libs.mpv.mpv_render_context_free)(self.rctx);
                self.rctx = std::ptr::null_mut();
            }
        }
        self.detach();
        let e = &self.libs.egl;
        unsafe {
            (e.eglMakeCurrent)(dpy, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (e.eglDestroyContext)(dpy, ctx);
            (e.eglTerminate)(dpy);
        }
        self.gl = None;
        self.dirty = false;
    }

    // ------------------------------------------------------------ frames
    // mpv's render update callback
    pub fn on_update(&mut self) {
        RENDER_PENDING.store(false, Ordering::Release);
        if self.rctx.is_null() {
            return;
        }
        self.stat.updates += 1;
        if unsafe { (self.libs.mpv.mpv_render_context_update)(self.rctx) } & UPDATE_FRAME != 0 {
            self.dirty = true;
        }
        if self.dirty && !self.frame_wait {
            self.draw();
        }
    }
    // the compositor showed the last frame
    pub fn frame_done(&mut self) {
        self.stat.callbacks += 1;
        self.frame_wait = false;
        if self.dirty {
            self.draw();
        }
    }
    // a new size: draw now, a frame callback may still be out
    pub fn resize(&mut self, w: i32, h: i32) {
        if (w, h) == self.size {
            return;
        }
        self.size = (w, h);
        self.dirty = true;
        self.draw();
    }
    fn draw(&mut self) {
        if self.rctx.is_null() {
            return;
        }
        let (w, h) = (self.size.0.max(1), self.size.1.max(1));
        let Some(g) = self.gl.as_mut() else { return };
        let Some((ewin, surf, gw, gh)) = g.win.as_mut() else { return };
        let (e, we, m) = (&self.libs.egl, &self.libs.wegl, &self.libs.mpv);
        unsafe {
            (e.eglMakeCurrent)(g.dpy, *surf, *surf, g.ctx);
            if (w, h) != (*gw, *gh) {
                (we.wl_egl_window_resize)(*ewin, w, h, 0, 0);
                (*gw, *gh) = (w, h);
            }
            if let Some(vs) = &g.vs {
                (vs.request_frame)();
            }
            let mut fbo = mpv_opengl_fbo { fbo: 0, w, h, internal_format: 0 };
            let mut flip: i32 = 1;
            let mut params = [
                mpv_render_param { kind: RP_OPENGL_FBO, data: &mut fbo as *mut _ as *mut c_void },
                mpv_render_param { kind: RP_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
                mpv_render_param { kind: 0, data: std::ptr::null_mut() },
            ];
            (m.mpv_render_context_render)(self.rctx, params.as_mut_ptr());
            (e.eglSwapBuffers)(g.dpy, *surf);
            (m.mpv_render_context_report_swap)(self.rctx);
        }
        self.frame_wait = true;
        self.dirty = false;
        if self.stat.frames < 4 {
            super::trace("drawn");
        }
        self.stat.frames += 1;
    }

    pub fn shutdown(&mut self) {
        self.gl_close();
        if !self.mpv.is_null() {
            unsafe { (self.libs.mpv.mpv_terminate_destroy)(self.mpv) };
            self.mpv = std::ptr::null_mut();
        }
    }
}
