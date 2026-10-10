// Polkit authentication agent (was Polkit.qml): an app asks for administrator rights (pkexec, GParted,
// systemd units, package managers) -> a macOS-style sheet on the focused monitor: what asks, the user,
// the password field. Enter confirms, Esc cancels; a wrong password shakes the card and asks again.
// D-Bus: org.freedesktop.PolicyKit1.AuthenticationAgent at AGENT_PATH, registered for our session.
// The password goes to polkit's helper (socket /run/polkit/agent-helper.socket, else the suid helper):
// "user\ncookie\n", then PAM lines (PAM_PROMPT_ECHO_OFF …, PAM_ERROR_MSG …, SUCCESS / FAILURE).
use crate::gfx::{Canvas, Rgba};
use crate::sys::Msg;
use crate::text::{Style, Text};
use crate::theme::*;
use calloop::channel::Sender;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::time::Instant;
use zbus::zvariant::{OwnedValue, Value};

const AGENT_PATH: &str = "/org/mist/PolicyKit1/AuthenticationAgent";

pub struct Begin {
    pub cookie: String,
    pub action: String,
    pub message: String,
    pub users: Vec<String>,
    pub reply: mpsc::Sender<bool>, // true = authenticated, false = cancelled
}

pub enum Ev {
    Begin(Begin),
    Cancel(String),
    Prompt(String, bool), // text, echo
    Info(String, bool),   // supplementary message, is error
    Done(bool),           // helper finished: SUCCESS / FAILURE
}

struct Agent {
    tx: Sender<Msg>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(&self, action_id: String, message: String, _icon_name: String, _details: HashMap<String, String>, cookie: String, identities: Vec<(String, HashMap<String, OwnedValue>)>) -> zbus::fdo::Result<()> {
        // the users we may ask for: unix-user identities -> names
        let mut users = Vec::new();
        for (kind, d) in &identities {
            if kind == "unix-user" {
                if let Some(uid) = d.get("uid").and_then(|v| u32::try_from(v).ok()) {
                    let pw = unsafe { libc::getpwuid(uid) };
                    if !pw.is_null() {
                        users.push(unsafe { std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().to_string() });
                    }
                }
            }
        }
        let (rtx, rrx) = mpsc::channel();
        let _ = self.tx.send(Msg::Polkit(Ev::Begin(Begin { cookie, action: action_id, message, users, reply: rtx })));
        // wait for the sheet without blocking zbus' executor
        loop {
            match rrx.try_recv() {
                Ok(true) => return Ok(()),
                Ok(false) | Err(mpsc::TryRecvError::Disconnected) => return Err(zbus::fdo::Error::Failed("cancelled".into())),
                Err(mpsc::TryRecvError::Empty) => async_io::Timer::after(std::time::Duration::from_millis(100)).await,
            };
        }
    }
    async fn cancel_authentication(&self, cookie: String) {
        let _ = self.tx.send(Msg::Polkit(Ev::Cancel(cookie)));
    }
}

// serve the agent and register it for this session (retries while another agent holds the session)
pub fn spawn(tx: Sender<Msg>) {
    let _ = std::thread::Builder::new().name("polkit".into()).stack_size(256 * 1024).spawn(move || loop {
        let run = || -> zbus::Result<()> {
            let c = zbus::blocking::connection::Builder::system()?.serve_at(AGENT_PATH, Agent { tx: tx.clone() })?.build()?;
            let sid = std::env::var("XDG_SESSION_ID").unwrap_or_default();
            let mut d: HashMap<&str, Value> = HashMap::new();
            let subject = if sid.is_empty() {
                d.insert("pid", Value::U32(std::process::id()));
                d.insert("start-time", Value::U64(0));
                ("unix-process", d)
            } else {
                d.insert("session-id", Value::from(sid));
                ("unix-session", d)
            };
            let locale = std::env::var("LANG").unwrap_or("pl_PL.UTF-8".into());
            c.call_method(Some("org.freedesktop.PolicyKit1"), "/org/freedesktop/PolicyKit1/Authority", Some("org.freedesktop.PolicyKit1.Authority"), "RegisterAuthenticationAgent", &(subject, locale, AGENT_PATH))?;
            // keep the connection (and the served object) alive
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        };
        if let Err(e) = run() {
            eprintln!("polkit agent: {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    });
}

// one helper conversation: prompts go to the sheet, the password comes back through `pw`
fn session(user: String, cookie: String, tx: Sender<Msg>, pw: mpsc::Receiver<String>) {
    std::thread::spawn(move || {
        let io: Option<(Box<dyn std::io::Read + Send>, Box<dyn Write + Send>, Option<std::process::Child>)> = match std::os::unix::net::UnixStream::connect("/run/polkit/agent-helper.socket") {
            Ok(s) => {
                let r = s.try_clone().ok();
                r.map(|r| (Box::new(r) as Box<dyn std::io::Read + Send>, Box::new(s) as Box<dyn Write + Send>, None))
            }
            Err(_) => std::process::Command::new("/usr/lib/polkit-1/polkit-agent-helper-1").arg(&user).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().ok().map(|mut c| {
                let (i, o) = (c.stdin.take().unwrap(), c.stdout.take().unwrap());
                (Box::new(o) as Box<dyn std::io::Read + Send>, Box::new(i) as Box<dyn Write + Send>, Some(c))
            }),
        };
        let Some((r, mut w, child)) = io else {
            let _ = tx.send(Msg::Polkit(Ev::Info("Nie można uruchomić pomocnika polkit".into(), true)));
            let _ = tx.send(Msg::Polkit(Ev::Done(false)));
            return;
        };
        if child.is_none() {
            let _ = writeln!(w, "{user}");
        }
        let _ = writeln!(w, "{cookie}");
        let _ = w.flush();
        let mut ok = false;
        for line in BufReader::new(r).lines() {
            let Ok(l) = line else { break };
            if let Some(p) = l.strip_prefix("PAM_PROMPT_ECHO_OFF ") {
                let _ = tx.send(Msg::Polkit(Ev::Prompt(p.to_string(), false)));
                let Ok(s) = pw.recv() else { break };
                let _ = writeln!(w, "{s}");
                let _ = w.flush();
            } else if let Some(p) = l.strip_prefix("PAM_PROMPT_ECHO_ON ") {
                let _ = tx.send(Msg::Polkit(Ev::Prompt(p.to_string(), true)));
                let Ok(s) = pw.recv() else { break };
                let _ = writeln!(w, "{s}");
                let _ = w.flush();
            } else if let Some(p) = l.strip_prefix("PAM_ERROR_MSG ") {
                let _ = tx.send(Msg::Polkit(Ev::Info(p.to_string(), true)));
            } else if let Some(p) = l.strip_prefix("PAM_TEXT_INFO ") {
                let _ = tx.send(Msg::Polkit(Ev::Info(p.to_string(), false)));
            } else if l.starts_with("SUCCESS") {
                ok = true;
                break;
            } else if l.starts_with("FAILURE") {
                break;
            }
        }
        if let Some(mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = tx.send(Msg::Polkit(Ev::Done(ok)));
    });
}

// ------------------------------------------------------------------ the sheet
pub struct Sheet {
    pub req: Begin,
    pub user: String,
    pub pw: String,
    pub prompt: String,
    pub echo: bool,
    pub info: String,
    pub info_err: bool,
    pub busy: bool,
    pw_tx: Option<mpsc::Sender<String>>,
    pub shake: Option<Instant>,
    pub hover: Option<u8>, // 0 cancel, 1 ok
    pub hits: Vec<(f32, f32, f32, f32, u8)>,
    pub blink: Instant,
}

impl Sheet {
    pub fn new(req: Begin, tx: Sender<Msg>) -> Sheet {
        let me = std::env::var("USER").unwrap_or_default();
        let user = if req.users.iter().any(|u| *u == me) || req.users.is_empty() { me } else { req.users[0].clone() };
        let mut s = Sheet { req, user, pw: String::new(), prompt: String::new(), echo: false, info: String::new(), info_err: false, busy: false, pw_tx: None, shake: None, hover: None, hits: Vec::new(), blink: Instant::now() };
        s.start(tx);
        s
    }
    // a new helper conversation (at open and after a wrong password)
    pub fn start(&mut self, tx: Sender<Msg>) {
        let (ptx, prx) = mpsc::channel();
        self.pw_tx = Some(ptx);
        session(self.user.clone(), self.req.cookie.clone(), tx, prx);
    }
    pub fn submit(&mut self) {
        if self.busy || self.pw_tx.is_none() {
            return;
        }
        self.busy = true;
        if let Some(t) = &self.pw_tx {
            let _ = t.send(std::mem::take(&mut self.pw));
        }
    }
    // the helper finished: true = the sheet closes
    pub fn done(&mut self, ok: bool, tx: Sender<Msg>) -> bool {
        if ok {
            let _ = self.req.reply.send(true);
            return true;
        }
        self.busy = false;
        self.pw.clear();
        self.shake = Some(Instant::now());
        self.start(tx);
        false
    }
    pub fn cancel(&mut self) {
        self.pw_tx = None; // the helper sees EOF
        let _ = self.req.reply.send(false);
    }
    pub fn animating(&self) -> bool {
        self.shake.map(|s| s.elapsed().as_millis() < 260).unwrap_or(false)
    }
    fn shake_x(&self) -> f32 {
        let Some(s) = self.shake else { return 0.0 };
        let ms = s.elapsed().as_millis() as f32;
        let keys = [(0.0, 0.0), (50.0, -14.0), (120.0, 12.0), (190.0, -8.0), (250.0, 0.0)];
        for k in 1..keys.len() {
            if ms <= keys[k].0 {
                let (t0, v0) = keys[k - 1];
                let (t1, v1) = keys[k];
                return v0 + (v1 - v0) * (ms - t0) / (t1 - t0);
            }
        }
        0.0
    }

    // the whole monitor: dim + the card in the middle
    pub fn paint(&mut self, c: &mut Canvas, t: &mut Text) {
        let (w, h) = (c.w(), c.h());
        c.clear();
        self.hits.clear();
        crate::desktop::rect(c, 0.0, 0.0, w, h, 0.0, Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.42));
        // layout of the column (width 376)
        let cw = 376.0;
        let title = Style::ui(15.0, 600.0);
        let body = Style::ui(12.5, 450.0);
        let small = Style::ui(10.5, 450.0);
        let msg_lines = crate::gfx::wrap(t, &self.req.message, &body, cw, 6);
        let info_lines = if self.info.is_empty() { vec![] } else { crate::gfx::wrap(t, &self.info, &Style::ui(12.0, 450.0), cw, 3) };
        let (lt, lb, ls, li) = (crate::desktop::line_h(t, &title), crate::desktop::line_h(t, &body), crate::desktop::line_h(t, &small), crate::desktop::line_h(t, &Style::ui(12.0, 450.0)));
        let mut col_h = 56.0 + 10.0 + lt + 10.0 + lb * msg_lines.len() as f32 + 10.0 + ls + 10.0 + 4.0 + 10.0 + lb + 10.0 + 38.0;
        if !info_lines.is_empty() {
            col_h += 10.0 + li * info_lines.len() as f32;
        }
        col_h += 10.0 + 4.0 + 10.0 + 34.0;
        let (cardw, cardh) = (440.0, col_h + 56.0);
        let (x0, y0) = (((w - cardw) / 2.0).round() + self.shake_x(), ((h - cardh) / 2.0).round());
        crate::glass::Glass { radius: 26.0, tint: 0.3 + 0.82 * 0.2, shine: 1.15, shadow_opacity: 0.2, rims: false, ..Default::default() }.paint(c, x0, y0, cardw, cardh);
        let cx = x0 + cardw / 2.0;
        let lx = x0 + 32.0;
        let mut y = y0 + 28.0;
        crate::desktop::circle(c, cx, y + 28.0, 28.0, FILL2);
        c.icon(t, cx, y + 28.0, "lock", 26.0, T1);
        y += 56.0 + 10.0;
        let center = |c: &mut Canvas, t: &mut Text, y: f32, s: &str, st: &Style, col: Rgba| {
            let sw = t.width(s, st).min(cw);
            crate::desktop::text(c, t, (cx - sw / 2.0).round(), y, s, st, col, false, Some(cw));
        };
        center(c, t, y, "Wymagane uprawnienia administratora", &title, T1);
        y += lt + 10.0;
        for l in &msg_lines {
            center(c, t, y, l, &body, T2);
            y += lb;
        }
        y += 10.0;
        // the action id, elided in the middle
        let mut a = self.req.action.clone();
        if t.width(&a, &small) > cw {
            let ch: Vec<char> = a.chars().collect();
            let mut k = ch.len() / 2;
            while k > 4 {
                let s: String = ch[..k / 2].iter().collect::<String>() + "…" + &ch[ch.len() - k / 2..].iter().collect::<String>();
                if t.width(&s, &small) <= cw {
                    a = s;
                    break;
                }
                k -= 2;
            }
        }
        center(c, t, y, &a, &small, T4);
        y += ls + 10.0 + 4.0 + 10.0;
        center(c, t, y, &format!("Użytkownik: {}", self.user), &body, T2);
        y += lb + 10.0;
        // the field
        crate::desktop::rect(c, lx, y, cw, 38.0, 19.0, FILL);
        crate::desktop::border(c, lx, y, cw, 38.0, 19.0, 1.0, ACCENT);
        let fs = Style::ui(14.0, 450.0);
        let fh = crate::desktop::line_h(t, &fs);
        if self.pw.is_empty() {
            let p = if self.prompt.is_empty() { "Hasło".to_string() } else { self.prompt.trim_end().trim_end_matches(':').to_string() };
            let ps = Style::ui(13.5, 450.0);
            let ph = crate::desktop::line_h(t, &ps);
            crate::desktop::text(c, t, lx + 16.0, y + (38.0 - ph) / 2.0, &p, &ps, T4, false, Some(cw - 32.0));
        }
        let shown: String = if self.echo { self.pw.clone() } else { "•".repeat(self.pw.chars().count()) };
        let tw = if shown.is_empty() { 0.0 } else { crate::desktop::text(c, t, lx + 16.0, y + (38.0 - fh) / 2.0, &shown, &fs, T1, false, Some(cw - 32.0)) };
        if !self.busy && (self.blink.elapsed().as_millis() % 1000) < 500 {
            crate::desktop::rect(c, lx + 16.0 + tw.min(cw - 32.0), y + 10.0, 1.0, 18.0, 0.0, T1);
        }
        y += 38.0;
        if !info_lines.is_empty() {
            y += 10.0;
            for l in &info_lines {
                center(c, t, y, l, &Style::ui(12.0, 450.0), if self.info_err { DANGER } else { T3 });
                y += li;
            }
        }
        y += 10.0 + 4.0 + 10.0;
        // buttons
        let bs = Style::ui(13.5, 550.0);
        let bh = crate::desktop::line_h(t, &bs);
        let (b1, b2) = (cx - 5.0 - 150.0, cx + 5.0);
        crate::desktop::rect(c, b1, y, 150.0, 34.0, 17.0, if self.hover == Some(0) { FILL2 } else { FILL });
        let w1 = t.width("Anuluj", &bs);
        crate::desktop::text(c, t, b1 + (150.0 - w1) / 2.0, y + (34.0 - bh) / 2.0, "Anuluj", &bs, T1, false, None);
        let strong = Rgba::hex("#409CFF");
        crate::desktop::rect(c, b2, y, 150.0, 34.0, 17.0, if self.busy { FILL2 } else if self.hover == Some(1) { ACCENT } else { strong });
        let ok = if self.busy { "Sprawdzanie…" } else { "Uwierzytelnij" };
        let os = Style::ui(13.5, 600.0);
        let w2 = t.width(ok, &os);
        crate::desktop::text(c, t, b2 + (150.0 - w2) / 2.0, y + (34.0 - bh) / 2.0, ok, &os, if self.busy { T2 } else { Rgba::hex("#0e1420") }, false, None);
        self.hits.push((b1, y, 150.0, 34.0, 0));
        self.hits.push((b2, y, 150.0, 34.0, 1));
        crate::glass::Glass { radius: 26.0, rim_only: true, shine: 1.15, ..Default::default() }.paint(c, x0, y0, cardw, cardh);
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<u8> {
        self.hits.iter().find(|h| x >= h.0 && x < h.0 + h.2 && y >= h.1 && y < h.1 + h.3).map(|h| h.4)
    }
}
