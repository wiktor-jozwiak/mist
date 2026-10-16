// The session lock (was Lock.qml + lock/MacLock.qml, removed; see git history): `mist-ui-bin lock [preview N]`, its own process that
// lives only while the screen is locked (ext-session-lock; if it dies the compositor keeps the session
// locked and the next `mist-ui-bin lock` takes over). PAM "login" checks the password in a thread.
// macOS look: the photo wallpaper, blurred and dimmed once awake (first key / mouse move, back after 30 s
// idle with an empty field), the date above big glass numerals at the top, avatar + name + password
// capsule at the bottom of the main monitor; a wrong password shakes the capsule and clears it.
use crate::gfx::{Canvas, Rgba};
use crate::text::{Style, Text};
use calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_session_lock, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    session_lock::{SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface, SessionLockSurfaceConfigure},
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use std::time::{Duration, Instant};
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use wayland_client::{globals::registry_queue_init, protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface}, Connection, QueueHandle};

const PRIMARY: &str = "DP-3";

// ------------------------------------------------------------------ PAM
mod pam {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    #[repr(C)]
    struct Message {
        style: c_int,
        msg: *const c_char,
    }
    #[repr(C)]
    struct Response {
        resp: *mut c_char,
        retcode: c_int,
    }
    #[repr(C)]
    struct Conv {
        conv: extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
        data: *mut c_void,
    }
    #[link(name = "pam")]
    unsafe extern "C" {
        fn pam_start(service: *const c_char, user: *const c_char, conv: *const Conv, h: *mut *mut c_void) -> c_int;
        fn pam_authenticate(h: *mut c_void, flags: c_int) -> c_int;
        fn pam_end(h: *mut c_void, status: c_int) -> c_int;
    }
    struct Data {
        password: CString,
        info: String,
    }
    extern "C" fn conv(n: c_int, msg: *mut *const Message, resp: *mut *mut Response, data: *mut c_void) -> c_int {
        unsafe {
            let d = &mut *(data as *mut Data);
            let out = libc::calloc(n as usize, std::mem::size_of::<Response>()) as *mut Response;
            if out.is_null() {
                return 5; // PAM_BUF_ERR
            }
            for i in 0..n as isize {
                let m = &**msg.offset(i);
                match m.style {
                    1 | 2 => (*out.offset(i)).resp = libc::strdup(d.password.as_ptr()), // PROMPT_ECHO_OFF / ON
                    _ => {
                        if !m.msg.is_null() {
                            d.info = CStr::from_ptr(m.msg).to_string_lossy().to_string();
                        }
                    }
                }
            }
            *resp = out;
            0
        }
    }
    // Ok(()) or the message to show
    pub fn check(user: &str, password: &str) -> Result<(), String> {
        let mut d = Data { password: CString::new(password).unwrap_or_default(), info: String::new() };
        let c = Conv { conv, data: &mut d as *mut Data as *mut c_void };
        let (svc, u) = (CString::new("login").unwrap(), CString::new(user).unwrap_or_default());
        let mut h: *mut c_void = std::ptr::null_mut();
        unsafe {
            let r = pam_start(svc.as_ptr(), u.as_ptr(), &c, &mut h);
            if r != 0 {
                return Err("Nie można uruchomić PAM".into());
            }
            let r = pam_authenticate(h, 0);
            pam_end(h, r);
            // wipe the copy of the password
            let b = d.password.as_bytes().len();
            std::ptr::write_bytes(d.password.as_ptr() as *mut u8, 0, b);
            match r {
                0 => Ok(()),
                11 => Err("Zbyt wiele prób, poczekaj chwilę".into()), // PAM_MAXTRIES
                7 => Err("Nieprawidłowe hasło".into()),               // PAM_AUTH_ERR
                _ if !d.info.is_empty() => Err(d.info),
                _ => Err("Nieprawidłowe hasło".into()),
            }
        }
    }
}

// ------------------------------------------------------------------ greetd (`mist-ui-bin greeter`)
// IPC: u32 length (native endian) + JSON. create_session -> answer the auth prompts -> start_session.
// Without GREETD_SOCK (dry run, tests) the password "test" logs in.
mod greetd {
    use serde_json::{json, Value};
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
    };

    fn rpc(s: &mut UnixStream, v: Value) -> Result<Value, String> {
        let b = v.to_string();
        let io = |e: std::io::Error| format!("greetd: {e}");
        s.write_all(&(b.len() as u32).to_ne_bytes()).map_err(io)?;
        s.write_all(b.as_bytes()).map_err(io)?;
        let mut l = [0u8; 4];
        s.read_exact(&mut l).map_err(io)?;
        let mut r = vec![0u8; u32::from_ne_bytes(l) as usize];
        s.read_exact(&mut r).map_err(io)?;
        serde_json::from_slice(&r).map_err(|e| format!("greetd: {e}"))
    }

    fn fail(s: &mut UnixStream, r: &Value) -> String {
        let _ = rpc(s, json!({ "type": "cancel_session" }));
        match r["error_type"].as_str() {
            Some("auth_error") => "Nieprawidłowe hasło".into(),
            _ => r["description"].as_str().unwrap_or("Błąd greetd").to_string(),
        }
    }

    pub fn login(user: &str, pw: &str, cmd: &[String], env: &[String]) -> Result<(), String> {
        let Ok(sock) = std::env::var("GREETD_SOCK") else {
            return if pw == "test" { Ok(()) } else { Err("Nieprawidłowe hasło".into()) };
        };
        let mut s = UnixStream::connect(sock).map_err(|e| format!("greetd: {e}"))?;
        let mut r = rpc(&mut s, json!({ "type": "create_session", "username": user }))?;
        loop {
            match r["type"].as_str() {
                Some("auth_message") => {
                    let answer = match r["auth_message_type"].as_str() {
                        Some("secret") | Some("visible") => json!(pw),
                        _ => Value::Null,
                    };
                    r = rpc(&mut s, json!({ "type": "post_auth_message_response", "response": answer }))?;
                }
                Some("success") => break,
                _ => return Err(fail(&mut s, &r)),
            }
        }
        r = rpc(&mut s, json!({ "type": "start_session", "cmd": cmd, "env": env }))?;
        if r["type"] == "success" {
            Ok(())
        } else {
            Err(fail(&mut s, &r))
        }
    }
}

/// greeter.json (install-greeter.sh): { user, name, avatar, wallpaper, session: [cmd...], env: [K=V...] },
/// avatar / wallpaper relative to the file.
struct Greeter {
    session: Vec<String>,
    env: Vec<String>,
}

// ------------------------------------------------------------------ pictures
fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
fn photo_path() -> String {
    let c: serde_json::Value = std::fs::read_to_string(format!("{}/.config/mist/wallpaper.json", home())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    c["image"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or(format!("{}/glow-ember.png", crate::paths::wallpaper_dir()))
}
// the photo cropped to w x h, premultiplied RGBA (opaque)
fn photo(path: &str, w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w, h).unwrap();
    pm.fill(tiny_skia::Color::from_rgba8(0x0b, 0x0f, 0x18, 255));
    if let Some(im) = image::ImageReader::open(path).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.decode().ok()) {
        let im = im.resize_to_fill(w, h, image::imageops::FilterType::Triangle).to_rgba8();
        for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(im.pixels()) {
            d[0] = s[0];
            d[1] = s[1];
            d[2] = s[2];
            d[3] = 255;
        }
    }
    pm
}
fn box_blur(buf: &mut [f32], w: usize, h: usize, r: usize) {
    let mut tmp = vec![0f32; w.max(h)];
    let win = (2 * r + 1) as f32;
    for pass in 0..2 {
        let (n_lines, len) = if pass == 0 { (h, w) } else { (w, h) };
        for l in 0..n_lines {
            let idx = |i: usize| if pass == 0 { l * w + i } else { i * w + l };
            // clamped edges (MultiEffect does not darken the borders)
            let get = |i: isize, b: &[f32]| b[idx(i.clamp(0, len as isize - 1) as usize)];
            let mut sum = 0.0;
            for i in -(r as isize)..=(r as isize) {
                sum += get(i, buf);
            }
            for i in 0..len {
                tmp[i] = sum / win;
                sum += get(i as isize + r as isize + 1, buf) - get(i as isize - r as isize, buf);
            }
            for i in 0..len {
                buf[idx(i)] = tmp[i];
            }
        }
    }
}
// MultiEffect blur .85 (of 64), saturation +.12, brightness -.08: at half size, 3 box passes, back up
fn frosted(src: &Pixmap) -> Pixmap {
    let (w, h) = (src.width(), src.height());
    let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
    let mut small = Pixmap::new(hw, hh).unwrap();
    small.draw_pixmap(0, 0, src.as_ref(), &PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, Transform::from_scale(0.5, 0.5), None);
    let n = (hw * hh) as usize;
    let mut ch: Vec<Vec<f32>> = (0..3).map(|c| small.data().chunks_exact(4).map(|p| p[c] as f32).collect()).collect();
    for c in ch.iter_mut() {
        for _ in 0..3 {
            box_blur(c, hw as usize, hh as usize, 9);
        }
    }
    let d = small.data_mut();
    for i in 0..n {
        let (r, g, b) = (ch[0][i] / 255.0, ch[1][i] / 255.0, ch[2][i] / 255.0);
        let l = 0.299 * r + 0.587 * g + 0.114 * b;
        let s = 1.12;
        let f = |v: f32| ((l + (v - l) * s) - 0.08).clamp(0.0, 1.0) * 255.0;
        d[i * 4] = f(r) as u8;
        d[i * 4 + 1] = f(g) as u8;
        d[i * 4 + 2] = f(b) as u8;
        d[i * 4 + 3] = 255;
    }
    let mut out = Pixmap::new(w, h).unwrap();
    out.draw_pixmap(0, 0, small.as_ref(), &PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, Transform::from_scale(w as f32 / hw as f32, h as f32 / hh as f32), None);
    out
}

fn glyph(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, cp: u32, size: f32, col: Rgba) {
    let s = char::from_u32(cp).unwrap_or(' ').to_string();
    let st = Style::icon(size);
    let w = t.width(&s, &st);
    c.text(t, cx - w / 2.0, cy, &s, &st, col);
}

// ------------------------------------------------------------------ the locker
struct LSurf {
    output: wl_output::WlOutput,
    name: String,
    surface: SessionLockSurface,
    w: u32,
    h: u32,
    sharp: Option<Pixmap>,
    blurred: Option<Pixmap>,
    clock: Option<(i64, Pixmap)>, // date + glass time, per minute
    dirty: bool,
}

struct L {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    seats: SeatState,
    lock_state: SessionLockState,
    lock: Option<SessionLock>,
    surfs: Vec<LSurf>,
    pool: SlotPool,
    text: Text,
    qh: QueueHandle<L>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    // state
    user: String,
    full_name: String,
    face: Option<Pixmap>,
    pw: String,
    busy: bool,
    message: String,
    awake: crate::anim::Anim,
    hint: crate::anim::Anim,
    go: crate::anim::Anim,
    last_input: Instant,
    last_ptr: (f64, f64),
    shake: Option<Instant>,
    spin: Instant,
    blink: Instant,
    go_hover: bool,
    unlocked: bool,
    exit: bool,
    tx: calloop::channel::Sender<Result<(), String>>,
    pending_len: usize,
    wall: String,
    greeter: Option<std::sync::Arc<Greeter>>,
}

impl L {
    fn wake(&mut self) {
        self.awake.set(1.0, 450.0, crate::anim::Ease::OutCubic);
        self.last_input = Instant::now();
        self.blink = Instant::now();
        self.dirty_all();
    }
    fn dirty_all(&mut self) {
        for s in &mut self.surfs {
            s.dirty = true;
        }
    }
    fn animating(&self) -> bool {
        self.awake.running() || self.hint.running() || self.go.running() || self.shake.map(|s| s.elapsed().as_millis() < 400).unwrap_or(false) || self.busy
    }
    fn submit(&mut self) {
        if self.pw.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.message.clear();
        let (user, pw, tx) = (self.user.clone(), std::mem::take(&mut self.pw), self.tx.clone());
        self.pending_len = pw.chars().count();
        let greeter = self.greeter.clone();
        std::thread::spawn(move || {
            let r = match greeter {
                Some(g) => greetd::login(&user, &pw, &g.session, &g.env),
                None => pam::check(&user, &pw),
            };
            let _ = tx.send(r);
        });
        self.dirty_all();
    }

    fn add_output(&mut self, o: wl_output::WlOutput) {
        let Some(lock) = &self.lock else { return };
        let name = self.outputs.info(&o).and_then(|i| i.name.clone()).unwrap_or_default();
        if self.surfs.iter().any(|s| s.output == o) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let ls = lock.create_lock_surface(surface, &o, &self.qh);
        self.surfs.push(LSurf { output: o, name, surface: ls, w: 0, h: 0, sharp: None, blurred: None, clock: None, dirty: true });
    }

    // date + glass numerals (MacLock clock column), cached per minute
    fn clock(&mut self, w: u32) -> Pixmap {
        let t = &mut self.text;
        let now = crate::data::now();
        let tm = crate::data::local(now);
        const DAYS: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];
        let date = format!("{}, {} {}", DAYS[tm.tm_wday as usize], tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]);
        let time = format!("{}:{:02}", tm.tm_hour, tm.tm_min);
        let ds = Style::ui(24.0, 600.0);
        let dh = crate::desktop::line_h(t, &ds);
        let ts = Style::ui(176.0, 700.0).spaced(-5.0);
        let th = crate::desktop::line_h(t, &ts) - 44.0;
        let pad = 60.0;
        let mut out = Canvas::new(w, (dh + th + 2.0 * pad) as u32);
        // date: shadow black .25, blur .5, 1 px down
        let mut l = Canvas::new(w, (dh + 2.0 * pad) as u32);
        let dw = t.width(&date, &ds);
        crate::desktop::text(&mut l, t, ((w as f32 - dw) / 2.0).round(), pad, &date, &ds, Rgba(1.0, 1.0, 1.0, 0.92), false, None);
        crate::desktop::put_with_shadow(&mut out, &l.pm, 0, 0, 6.4, Rgba(0.0, 0.0, 0.0, 1.0), 0.25, 1);
        // time: the glyphs' alpha painted with the glass gradient (glasstext.frag), shadow .16 blur 40, 6 px down
        let tw = t.width(&time, &ts);
        let mut g = Canvas::new(w, (th + 2.0 * pad) as u32);
        let ty = pad; // the item's top; the glyphs sit 20 px higher (topPadding -20)
        crate::desktop::text(&mut g, t, ((w as f32 - tw) / 2.0).round(), ty - 20.0, &time, &ts, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
        let (c_top, c_mid, c_bot) = ([1.0f32, 1.0, 1.0, 0.97], [1.0f32, 1.0, 1.0, 0.84], [220.0 / 255.0, 232.0 / 255.0, 1.0, 0.74]);
        let gw = g.pm.width() as usize;
        let d = g.pm.data_mut();
        for y in 0..(th + 2.0 * pad) as usize {
            let v = ((y as f32 - ty) / th).clamp(0.0, 1.0);
            let c = if v < 0.5 {
                let k = v / 0.5;
                [0, 1, 2, 3].map(|i| c_top[i] + (c_mid[i] - c_top[i]) * k)
            } else {
                let k = (v - 0.5) / 0.5;
                [0, 1, 2, 3].map(|i| c_mid[i] + (c_bot[i] - c_mid[i]) * k)
            };
            for x in 0..gw {
                let i = (y * gw + x) * 4;
                let a = d[i + 3] as f32 / 255.0 * c[3];
                d[i] = (c[0] * a * 255.0) as u8;
                d[i + 1] = (c[1] * a * 255.0) as u8;
                d[i + 2] = (c[2] * a * 255.0) as u8;
                d[i + 3] = (a * 255.0) as u8;
            }
        }
        let sh = crate::desktop::drop_shadow(&g.pm, 16.0, Rgba(2.0 / 255.0, 6.0 / 255.0, 16.0 / 255.0, 1.0), 0.16, 0, 6);
        let oy = dh as i32;
        out.pm.draw_pixmap(0, oy, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        out.pm.draw_pixmap(0, oy, g.pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        out.pm
    }

    fn draw(&mut self, i: usize) {
        let (w, h) = (self.surfs[i].w, self.surfs[i].h);
        if w == 0 {
            return;
        }
        self.surfs[i].dirty = false;
        if self.surfs[i].name.is_empty() {
            self.surfs[i].name = self.outputs.info(&self.surfs[i].output).and_then(|i| i.name.clone()).unwrap_or_default();
        }
        if self.surfs[i].sharp.is_none() {
            let p = photo(&self.wall, w, h);
            self.surfs[i].blurred = Some(frosted(&p));
            self.surfs[i].sharp = Some(p);
        }
        let minute = crate::data::now() / 60;
        if self.surfs[i].clock.as_ref().map(|c| c.0 != minute).unwrap_or(true) {
            let pm = self.clock(w);
            self.surfs[i].clock = Some((minute, pm));
        }
        let mut c = Canvas::new(w, h);
        let s = &self.surfs[i];
        let aw = self.awake.value();
        c.pm.draw_pixmap(0, 0, s.sharp.as_ref().unwrap().as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        if aw > 0.0 {
            c.pm.draw_pixmap(0, 0, s.blurred.as_ref().unwrap().as_ref(), &PixmapPaint { opacity: aw, ..Default::default() }, Transform::identity(), None);
        }
        // soft vignette top and bottom
        if let Some(p) = crate::gfx::rrect_path(0.0, 0.0, w as f32, h as f32, 0.0) {
            let k = Rgba(0.0, 0.0, 0.0, 1.0);
            crate::desktop::fill_lin(&mut c, &p, (0.0, 0.0), (0.0, h as f32), &[(0.0, k.alpha(0.22)), (0.32, k.alpha(0.0)), (0.7, k.alpha(0.0)), (1.0, k.alpha(0.30))]);
        }
        if let Some((_, pm)) = &s.clock {
            c.pm.draw_pixmap(0, (h as f32 * 0.075).round() as i32 - 60, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        if s.name == PRIMARY || self.surfs.len() == 1 {
            self.who(&mut c, w as f32, h as f32);
        }
        let Ok((buf, data)) = self.pool.create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888) else { return };
        c.to_bgra(data);
        let surf = self.surfs[i].surface.wl_surface();
        surf.damage_buffer(0, 0, w as i32, h as i32);
        let _ = buf.attach_to(surf);
        surf.commit();
    }

    // avatar, name, password capsule, hint
    fn who(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let dx = self.shake_x();
        let t = &mut self.text;
        let raised = Rgba(0.0, 0.0, 0.0, 0.18);
        let ns = Style::ui(17.0, 600.0);
        let hs = Style::ui(13.0, 500.0);
        let (nh, hh) = (crate::desktop::line_h(t, &ns), crate::desktop::line_h(t, &hs));
        let total = 72.0 + 12.0 + nh + 14.0 + 34.0 + 12.0 + hh;
        let top = h - (h * 0.09).round() - total;
        let cx = w / 2.0;
        // avatar (picture or initials on the grey gradient), shadow .28 blur .6, 2 px down
        let mut a = Canvas::new(72 + 60, 72 + 60);
        match &self.face {
            Some(f) => {
                let mut m = tiny_skia::Mask::new(a.pm.width(), a.pm.height()).unwrap();
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_circle(66.0, 66.0, 36.0);
                if let Some(p) = pb.finish() {
                    m.fill_path(&p, tiny_skia::FillRule::Winding, true, Transform::identity());
                }
                let k = 72.0 / f.width().min(f.height()) as f32;
                let (fw, fh) = (f.width() as f32 * k, f.height() as f32 * k);
                a.pm.draw_pixmap(0, 0, f.as_ref(), &PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() }, Transform::from_row(k, 0.0, 0.0, k, 30.0 + (72.0 - fw) / 2.0, 30.0 + (72.0 - fh) / 2.0), Some(&m));
            }
            None => {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_circle(66.0, 66.0, 36.0);
                if let Some(p) = pb.finish() {
                    crate::desktop::fill_lin(&mut a, &p, (0.0, 30.0), (0.0, 102.0), &[(0.0, Rgba::hex("#a9b1be")), (1.0, Rgba::hex("#6c7482"))]);
                }
                let ini: String = self.full_name.split(' ').filter(|s| !s.is_empty()).take(2).filter_map(|s| s.chars().next()).flat_map(|c| c.to_uppercase()).collect();
                let is = Style::ui(28.0, 600.0);
                let iw = t.width(&ini, &is);
                let ih = crate::desktop::line_h(t, &is);
                crate::desktop::text(&mut a, t, 66.0 - iw / 2.0, 66.0 - ih / 2.0, &ini, &is, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
            }
        }
        crate::desktop::put_with_shadow(c, &a.pm, (cx - 66.0) as i32, (top - 30.0) as i32, 7.7, Rgba(0.0, 0.0, 0.0, 1.0), 0.28, 2);
        // name
        let mut y = top + 72.0 + 12.0;
        let nw = t.width(&self.full_name, &ns);
        c.text(t, (cx - nw / 2.0).round(), y + nh / 2.0 + 1.0, &self.full_name, &ns, raised);
        crate::desktop::text(c, t, (cx - nw / 2.0).round(), y, &self.full_name, &ns, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
        y += nh + 14.0;
        // password capsule (shakes on a wrong password)
        let (bx, by, bw, bh) = (cx - 116.0 + dx, y, 232.0, 34.0);
        crate::desktop::rect(c, bx, by, bw, bh, 17.0, Rgba(1.0, 1.0, 1.0, 0.20));
        crate::desktop::border(c, bx, by, bw, bh, 17.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.34));
        crate::desktop::rect(c, bx + 17.0, by + 1.0, bw - 34.0, 1.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.22));
        let n = if self.busy { self.pending_len } else { self.pw.chars().count() };
        let fx = bx + 16.0;
        if n == 0 {
            let ps = Style::ui(14.0, 450.0);
            let ph = crate::desktop::line_h(t, &ps);
            crate::desktop::text(c, t, fx, by + (bh - ph) / 2.0, "Wpisz hasło", &ps, Rgba(1.0, 1.0, 1.0, 0.62), false, None);
        } else {
            let ds = Style::ui(11.0, 500.0).spaced(2.5);
            let dots: String = "●".repeat(n.min(18));
            let dh = crate::desktop::line_h(t, &ds);
            let dw = crate::desktop::text(c, t, fx, by + (bh - dh) / 2.0, &dots, &ds, Rgba(1.0, 1.0, 1.0, 1.0), false, Some(bw - 16.0 - 35.0));
            let _ = dw;
        }
        // blinking cursor
        if !self.busy {
            let ph = self.blink.elapsed().as_millis() % 1000;
            let o = if ph < 500 { 1.0 - (ph as f32 / 500.0).powi(2) } else { let k = (ph - 500) as f32 / 500.0; 1.0 - (1.0 - k) * (1.0 - k) };
            let tx = if n == 0 { fx } else { fx + t.width(&"●".repeat(n.min(18)), &Style::ui(11.0, 500.0).spaced(2.5)).min(bw - 51.0) };
            crate::desktop::rect(c, tx, by + 9.0, 1.5, 16.0, 0.0, Rgba(1.0, 1.0, 1.0, 0.85 * o));
        }
        // arrow / spinner on the right
        let (gx, gy) = (bx + bw - 5.0 - 12.0, by + bh / 2.0);
        let go = self.go.value();
        if go > 0.0 {
            crate::desktop::circle(c, gx, gy, 12.0, Rgba(1.0, 1.0, 1.0, (if self.go_hover { 0.34 } else { 0.24 }) * go));
            glyph(c, t, gx, gy, 0xea1f, 15.0, Rgba(1.0, 1.0, 1.0, go));
        }
        if self.busy {
            // loader glyph turning once per 900 ms
            let ang = (self.spin.elapsed().as_millis() % 900) as f32 / 900.0 * 360.0;
            let mut l = Canvas::new(40, 40);
            glyph(&mut l, t, 20.0, 20.0, 0xf226, 17.0, Rgba(1.0, 1.0, 1.0, 0.85));
            c.pm.draw_pixmap(0, 0, l.pm.as_ref(), &PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, Transform::from_translate(gx, gy).pre_rotate(ang).pre_translate(-20.0, -20.0), None);
        }
        y += bh + 12.0;
        // hint / message
        let msg = if !self.message.is_empty() { self.message.clone() } else { "Naciśnij Enter, aby odblokować".into() };
        let ho = self.hint.value();
        if ho > 0.0 {
            let mw = t.width(&msg, &hs);
            c.text(t, (cx - mw / 2.0).round(), y + hh / 2.0 + 1.0, &msg, &hs, raised.alpha(0.18 * ho));
            crate::desktop::text(c, t, (cx - mw / 2.0).round(), y, &msg, &hs, Rgba(1.0, 1.0, 1.0, ho), false, None);
        }
    }

    fn shake_x(&self) -> f32 {
        let Some(s) = self.shake else { return 0.0 };
        let ms = s.elapsed().as_millis() as f32;
        // -14 (55, OutQuad), 12 (80), -9 (75), 6 (70), -3 (60), 0 (55)
        let keys = [(0.0, 0.0), (55.0, -14.0), (135.0, 12.0), (210.0, -9.0), (280.0, 6.0), (340.0, -3.0), (395.0, 0.0)];
        for k in 1..keys.len() {
            if ms <= keys[k].0 {
                let (t0, v0) = keys[k - 1];
                let (t1, v1) = keys[k];
                let mut p = (ms - t0) / (t1 - t0);
                if k == 1 {
                    p = 1.0 - (1.0 - p) * (1.0 - p);
                }
                return v0 + (v1 - v0) * p;
            }
        }
        0.0
    }

    fn update_anims(&mut self) {
        let hint_to = if !self.message.is_empty() { 0.95 } else if self.awake.target() > 0.5 { 0.62 } else { 0.0 };
        self.hint.set(hint_to, 300.0, crate::anim::Ease::Linear);
        self.go.set(if !self.pw.is_empty() && !self.busy { 1.0 } else { 0.0 }, 160.0, crate::anim::Ease::Linear);
    }
}

// `mist-ui-bin lock pam-check`: one deliberately wrong password through PAM (proves the conversation works)
pub fn pam_check() -> i32 {
    let user = std::env::var("USER").unwrap_or_default();
    match pam::check(&user, "\u{1}mist-wrong-password\u{1}") {
        Ok(()) => { println!("unexpected success"); 1 }
        Err(m) => { println!("rejected: {m}"); 0 }
    }
}

pub fn run(args: &[String]) -> i32 {
    if args.first().map(|a| a == "pam-check").unwrap_or(false) {
        return pam_check();
    }
    // `greeter [greeter.json]`: the login screen under greetd (inside `mist-comp --session`)
    let greeter_cfg: Option<(String, serde_json::Value)> = (args.first().map(|a| a.as_str()) == Some("greeter")).then(|| {
        let path = args.get(1).cloned().unwrap_or("/usr/share/mist-greeter/greeter.json".into());
        let v = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        (path, v)
    });
    let rel = |path: &str, f: &str| -> String {
        if f.is_empty() || f.starts_with('/') {
            f.to_string()
        } else {
            std::path::Path::new(path).parent().map(|d| d.join(f).to_string_lossy().into_owned()).unwrap_or_default()
        }
    };
    if greeter_cfg.is_none() {
        // one locker at a time (the greeter has its own compositor)
        let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
        let _ = std::fs::create_dir_all(&dir);
        let lf = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(format!("{dir}/lock.lock")).expect("lock file");
        if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lf), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return 0; // already locked
        }
        std::mem::forget(lf);
    }
    let preview: Option<u64> = if args.first().map(|a| a == "preview").unwrap_or(false) { Some(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(2, 30)) } else { None };
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, queue) = registry_queue_init::<L>(&conn).expect("registry");
    let qh = queue.handle();
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let pool = SlotPool::new(1920 * 1080 * 4, &shm).expect("pool");
    let gs = |k: &str| greeter_cfg.as_ref().and_then(|(_, v)| v[k].as_str()).unwrap_or("").to_string();
    let user = if greeter_cfg.is_some() { gs("user") } else { std::env::var("USER").unwrap_or_default() };
    // full name from GECOS, word case normalised
    let full_name = {
        let u = std::ffi::CString::new(user.clone()).unwrap_or_default();
        let pw = unsafe { libc::getpwnam(u.as_ptr()) };
        let g = if pw.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().split(',').next().unwrap_or("").trim().to_string() } };
        let g = if !gs("name").is_empty() { gs("name") } else if g.is_empty() { user.clone() } else { g };
        g.split(' ').map(|s| { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
    };
    // ~/.face when there is one, else the initials
    let avatar = greeter_cfg.as_ref().map(|(p, _)| rel(p, &gs("avatar"))).filter(|a| !a.is_empty());
    let face = avatar.into_iter().chain([format!("{}/.face", home()), format!("{}/.face.icon", home()), format!("/var/lib/AccountsService/icons/{user}")]).find(|f| std::fs::metadata(f).map(|m| m.len() > 0).unwrap_or(false)).and_then(|f| {
        let im = image::ImageReader::open(&f).ok()?.with_guessed_format().ok()?.decode().ok()?.thumbnail(144, 144).to_rgba8();
        let (w, h) = im.dimensions();
        let mut data = im.into_raw();
        for p in data.chunks_exact_mut(4) {
            let a = p[3] as u32;
            for c in 0..3 {
                p[c] = (p[c] as u32 * a / 255) as u8;
            }
        }
        Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
    });
    let mut ev: calloop::EventLoop<L> = calloop::EventLoop::try_new().unwrap();
    let lh = ev.handle();
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue).insert(lh.clone()).unwrap();
    let (tx, rx) = calloop::channel::channel::<Result<(), String>>();
    let mut l = L {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).expect("compositor"),
        shm,
        seats: SeatState::new(&globals, &qh),
        lock_state: SessionLockState::new(&globals, &qh),
        lock: None,
        surfs: Vec::new(),
        pool,
        text: Text::new(),
        qh: qh.clone(),
        keyboard: None,
        pointer: None,
        user,
        full_name,
        face,
        pw: String::new(),
        busy: false,
        message: String::new(),
        awake: crate::anim::Anim::new(0.0),
        hint: crate::anim::Anim::new(0.0),
        go: crate::anim::Anim::new(0.0),
        last_input: Instant::now(),
        last_ptr: (-100.0, -100.0),
        shake: None,
        spin: Instant::now(),
        blink: Instant::now(),
        go_hover: false,
        unlocked: false,
        exit: false,
        tx,
        pending_len: 0,
        wall: match &greeter_cfg {
            Some((p, _)) => rel(p, &gs("wallpaper")),
            None => photo_path(),
        },
        greeter: greeter_cfg.as_ref().map(|(_, v)| {
            let list = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let session: Vec<String> = list("session");
            std::sync::Arc::new(Greeter { session: if session.is_empty() { vec!["start-mist".into()] } else { session }, env: list("env") })
        }),
    };
    match l.lock_state.lock(&qh) {
        Ok(lk) => l.lock = Some(lk),
        Err(_) => {
            eprintln!("lock: ext-session-lock is not available");
            return 1;
        }
    }
    let outs: Vec<wl_output::WlOutput> = l.outputs.outputs().collect();
    for o in outs {
        l.add_output(o);
    }
    lh.insert_source(rx, |e, _, l: &mut L| {
        let calloop::channel::Event::Msg(r) = e else { return };
        l.busy = false;
        match r {
            Ok(()) => l.unlock(),
            Err(m) => {
                l.message = m;
                l.pw.clear();
                l.shake = Some(Instant::now());
                l.wake();
            }
        }
        l.dirty_all();
    })
    .unwrap();
    let started = Instant::now();
    lh.insert_source(Timer::from_duration(Duration::from_millis(16)), move |_, _, l: &mut L| {
        if let Some(p) = preview {
            if started.elapsed().as_secs() >= p && !l.unlocked {
                l.unlock();
            }
        }
        // back to the sharp wallpaper after 30 s without input (empty field)
        if l.awake.target() > 0.5 && l.pw.is_empty() && !l.busy && l.last_input.elapsed().as_secs() >= 30 {
            l.awake.set(0.0, 450.0, crate::anim::Ease::OutCubic);
            l.dirty_all();
        }
        l.update_anims();
        let anim = l.animating();
        // the cursor blinks (primary only) and the minute turns: a slower beat when nothing moves
        let blink_due = l.blink.elapsed().as_millis() % 1000 < 520;
        if anim || blink_due || l.surfs.iter().any(|s| s.clock.as_ref().map(|c| c.0 != crate::data::now() / 60).unwrap_or(false)) {
            let one = l.surfs_len_one();
            for s in &mut l.surfs {
                if anim || s.name == PRIMARY || one {
                    s.dirty = true;
                }
            }
        }
        for i in 0..l.surfs.len() {
            if l.surfs[i].dirty {
                l.draw(i);
            }
        }
        TimeoutAction::ToDuration(Duration::from_millis(if anim { 16 } else { 100 }))
    })
    .unwrap();
    while !l.exit {
        if ev.dispatch(Some(Duration::from_millis(500)), &mut l).is_err() {
            break;
        }
        let _ = conn.flush();
    }
    let _ = conn.roundtrip();
    0
}

impl L {
    fn surfs_len_one(&self) -> bool {
        self.surfs.len() == 1
    }
    fn unlock(&mut self) {
        if self.unlocked {
            return;
        }
        self.unlocked = true;
        if let Some(lk) = self.lock.take() {
            lk.unlock();
        }
        self.surfs.clear();
        self.exit = true;
    }
}

impl SessionLockHandler for L {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {}
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // the compositor refused (another locker): nothing to do
        self.exit = true;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: SessionLockSurface, c: SessionLockSurfaceConfigure, _: u32) {
        let Some(i) = self.surfs.iter().position(|s| s.surface.wl_surface() == surface.wl_surface()) else { return };
        let (w, h) = c.new_size;
        if self.surfs[i].w != w || self.surfs[i].h != h {
            self.surfs[i].sharp = None;
            self.surfs[i].blurred = None;
            self.surfs[i].clock = None;
        }
        self.surfs[i].w = w;
        self.surfs[i].h = h;
        self.draw(i);
    }
}
impl CompositorHandler for L {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}
impl OutputHandler for L {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.add_output(o);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.surfs.retain(|s| s.output != o);
    }
}
impl ShmHandler for L {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl SeatHandler for L {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl KeyboardHandler for L {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, ev: KeyEvent) {
        self.wake();
        if self.busy {
            return;
        }
        match ev.keysym {
            Keysym::Return | Keysym::KP_Enter => self.submit(),
            Keysym::Escape => self.pw.clear(),
            Keysym::BackSpace => {
                self.pw.pop();
            }
            _ => {
                if let Some(t) = ev.utf8 {
                    if !t.chars().any(|c| c.is_control()) {
                        self.pw.push_str(&t);
                        if !self.message.is_empty() {
                            self.message.clear();
                        }
                    }
                }
            }
        }
        self.dirty_all();
    }
    fn repeat_key(&mut self, c: &Connection, q: &QueueHandle<Self>, k: &wl_keyboard::WlKeyboard, s: u32, ev: KeyEvent) {
        self.press_key(c, q, k, s, ev);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: RawModifiers, _: u32) {}
}
impl PointerHandler for L {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            match e.kind {
                PointerEventKind::Motion { .. } => {
                    let (x, y) = e.position;
                    if (x - self.last_ptr.0).abs() + (y - self.last_ptr.1).abs() > 6.0 {
                        self.wake();
                    }
                    self.last_ptr = (x, y);
                    // over the arrow button (primary)
                    let on = self.surfs.iter().find(|s| s.surface.wl_surface() == &e.surface).map(|s| {
                        let (w, h) = (s.w as f64, s.h as f64);
                        let hh = 17.0; // close enough: the capsule row
                        let _ = hh;
                        let cx = w / 2.0 + 116.0 - 17.0;
                        let by = h - (h * 0.09).round() - 30.0 - 34.0;
                        (x - cx).abs() < 14.0 && y > by - 10.0 && y < by + 40.0
                    }).unwrap_or(false);
                    if on != self.go_hover {
                        self.go_hover = on;
                        self.dirty_all();
                    }
                }
                PointerEventKind::Press { .. } => {
                    self.wake();
                    if self.go_hover {
                        self.submit();
                    }
                }
                _ => {}
            }
        }
    }
}
impl ProvidesRegistryState for L {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
delegate_compositor!(L);
delegate_output!(L);
delegate_shm!(L);
delegate_seat!(L);
delegate_keyboard!(L);
delegate_pointer!(L);
delegate_session_lock!(L);
delegate_registry!(L);
