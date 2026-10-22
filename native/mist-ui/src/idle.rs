// Idle (was Idle.qml): lock the session after `lock` s without input, turn the monitors off after `off` s
// (back on with the first input). Apps that inhibit idle hold both off, by either route:
// - Wayland idle-inhibit (Firefox, mpv, the Mist player): ext-idle-notify's get_idle_notification respects it;
// - D-Bus org.freedesktop.ScreenSaver / PowerManagement.Inhibit (Chromium, VLC, Spotify, Flatpak apps), which
//   hypridle answered and Mist serves here (`screensaver`); an app's cookies go when it leaves the bus;
// - and a fullscreen window while a player plays (a video in a browser that asks neither).
// While held, idle events are ignored; when the hold ends the timers start over. Times in
// ~/.config/mist/idle.json {"lock": 600, "off": 900}; 0 = never. Plus the locker itself: `mist-ui-bin lock`
// is spawned (its own process) for Super+L, idle, logind (loginctl lock-session, going to sleep).
use crate::app::App;
use wayland_client::{protocol::wl_seat, Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1};

#[derive(Default)]
pub struct Idle {
    notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    lock_n: Option<ext_idle_notification_v1::ExtIdleNotificationV1>,
    off_n: Option<ext_idle_notification_v1::ExtIdleNotificationV1>,
    cfg: (u32, u32),
    child: Option<std::process::Child>,
    seat: Option<wl_seat::WlSeat>,
    /// D-Bus inhibitors alive (ScreenSaver / PowerManagement cookies)
    pub dbus: u32,
    held: bool,
}

fn cfg() -> (u32, u32) {
    let v: serde_json::Value = std::fs::read_to_string(format!("{}/.config/mist/idle.json", std::env::var("HOME").unwrap_or_default())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    (v["lock"].as_u64().unwrap_or(600) as u32, v["off"].as_u64().unwrap_or(900) as u32)
}

pub fn dpms(on: bool) {
    crate::paths::screen_note(Some(!on), None);
    crate::sys::hypr_eval(&format!("hl.dispatch(hl.dsp.dpms({{ action = \"{}\" }}))", if on { "on" } else { "off" }));
}

impl Idle {
    pub fn bind(&mut self, globals: &wayland_client::globals::GlobalList, qh: &QueueHandle<App>) {
        self.notifier = globals.bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(qh, 1..=1, ()).ok();
    }
    // (re)arm both notifications for the seat (idle.json changed or the seat appeared)
    pub fn arm(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<App>) {
        self.seat = Some(seat.clone());
        let Some(n) = &self.notifier else { return };
        self.cfg = cfg();
        for x in [self.lock_n.take(), self.off_n.take()].into_iter().flatten() {
            x.destroy();
        }
        if self.cfg.0 > 0 {
            self.lock_n = Some(n.get_idle_notification(self.cfg.0.max(30) * 1000, seat, qh, 0u8));
        }
        if self.cfg.1 > 0 {
            self.off_n = Some(n.get_idle_notification(self.cfg.1.max(30) * 1000, seat, qh, 1u8));
        }
    }
    /// Something wants the screen on (see the top); released: lock and screen-off count from now.
    pub fn set_held(&mut self, held: bool, qh: &QueueHandle<App>) {
        if held == self.held {
            return;
        }
        self.held = held;
        if !held {
            if let Some(s) = self.seat.clone() {
                self.arm(&s, qh);
            }
        }
    }
    pub fn held(&self) -> bool {
        self.held
    }
    pub fn changed(&self) -> bool {
        cfg() != self.cfg
    }
    // start the locker (it refuses to run twice); `preview`: unlocks itself after N s
    pub fn lock(&mut self, preview: Option<u32>) {
        self.reap();
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or("mist-ui-bin".into());
        let mut c = std::process::Command::new(exe);
        c.arg("lock");
        if let Some(n) = preview {
            c.args(["preview", &n.to_string()]);
        }
        if let Ok(ch) = c.stdin(std::process::Stdio::null()).spawn() {
            self.child = Some(ch);
        }
    }
    pub fn reap(&mut self) {
        if let Some(c) = &mut self.child {
            if let Ok(Some(_)) = c.try_wait() {
                self.child = None;
            }
        }
    }
    // the locker holds $XDG_RUNTIME_DIR/mist/lock.lock while the session is locked
    pub fn locked() -> bool {
        let p = format!("{}/mist/lock.lock", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
        let Ok(f) = std::fs::OpenOptions::new().read(true).open(p) else { return false };
        let fd = std::os::fd::AsRawFd::as_raw_fd(&f);
        if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            unsafe { libc::flock(fd, libc::LOCK_UN) };
            return false;
        }
        true
    }
}

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for App {
    fn event(_: &mut App, _: &ext_idle_notifier_v1::ExtIdleNotifierV1, _: ext_idle_notifier_v1::Event, _: &(), _: &Connection, _: &QueueHandle<App>) {}
}
impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, u8> for App {
    fn event(app: &mut App, _: &ext_idle_notification_v1::ExtIdleNotificationV1, ev: ext_idle_notification_v1::Event, kind: &u8, _: &Connection, _: &QueueHandle<App>) {
        match (ev, *kind) {
            (ext_idle_notification_v1::Event::Idled, _) if app.idle.held() => {}
            (ext_idle_notification_v1::Event::Idled, 0) => app.idle.lock(None),
            (ext_idle_notification_v1::Event::Idled, 1) => dpms(false),
            (ext_idle_notification_v1::Event::Resumed, 1) => dpms(true),
            _ => {}
        }
    }
}

// logind: `loginctl lock-session` (Session.Lock on our session) and going to sleep (PrepareForSleep true)
pub fn logind(tx: calloop::channel::Sender<crate::sys::Msg>) {
    let _ = std::thread::Builder::new().name("logind".into()).stack_size(256 * 1024).spawn(move || loop {
        let run = || -> Option<()> {
            let c = zbus::blocking::Connection::system().ok()?;
            let sid = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "auto".into());
            // the object path escapes every non-letter as _xx
            let path = format!("/org/freedesktop/login1/session/{}", sid.chars().map(|ch| if ch.is_ascii_alphabetic() { ch.to_string() } else { format!("_{:x}", ch as u32) }).collect::<String>());
            let rules = [
                "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'".to_string(),
                format!("type='signal',interface='org.freedesktop.login1.Session',member='Lock',path='{path}'"),
            ];
            let dbus = zbus::blocking::fdo::DBusProxy::new(&c).ok()?;
            for r in &rules {
                dbus.add_match_rule(zbus::MatchRule::try_from(r.as_str()).ok()?).ok()?;
            }
            for m in zbus::blocking::MessageIterator::from(&c) {
                let Ok(m) = m else { continue };
                let h = m.header();
                let member = h.member().map(|x| x.to_string()).unwrap_or_default();
                let lock = match member.as_str() {
                    "PrepareForSleep" => m.body().deserialize::<bool>().unwrap_or(false),
                    "Lock" => h.path().map(|p| p.as_str() == path).unwrap_or(false),
                    _ => false,
                };
                if lock && tx.send(crate::sys::Msg::Lock).is_err() {
                    return Some(());
                }
            }
            Some(())
        };
        run();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
}

// ---------------------------------------------------------------- D-Bus inhibitors (what hypridle answered)
type Cookies = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u32, (String, String)>>>;

struct Inhibit {
    cookies: Cookies,
    next: std::sync::Arc<std::sync::atomic::AtomicU32>,
    tx: calloop::channel::Sender<crate::sys::Msg>,
}

impl Inhibit {
    fn add(&self, hdr: &zbus::message::Header<'_>, app: String, reason: &str) -> u32 {
        let sender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        let cookie = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        eprintln!("mist-ui: idle inhibited by {app} ({reason})");
        let n = {
            let mut c = self.cookies.lock().unwrap();
            c.insert(cookie, (sender, app));
            c.len()
        };
        let _ = self.tx.send(crate::sys::Msg::IdleInhibit(n as u32));
        cookie
    }
    fn remove(&self, cookie: u32) {
        let n = {
            let mut c = self.cookies.lock().unwrap();
            c.remove(&cookie);
            c.len()
        };
        let _ = self.tx.send(crate::sys::Msg::IdleInhibit(n as u32));
    }
    fn clone_shared(&self) -> Inhibit {
        Inhibit { cookies: self.cookies.clone(), next: self.next.clone(), tx: self.tx.clone() }
    }
}

struct ScreenSaver(Inhibit);
#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn inhibit(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, application_name: String, reason_for_inhibit: String) -> u32 {
        self.0.add(&hdr, application_name, &reason_for_inhibit)
    }
    fn un_inhibit(&self, cookie: u32) {
        self.0.remove(cookie)
    }
    fn get_active(&self) -> bool {
        Idle::locked()
    }
    fn get_active_time(&self) -> u32 {
        0
    }
    fn get_session_idle_time(&self) -> u32 {
        0
    }
    fn set_active(&self, _active: bool) -> bool {
        false
    }
    // apps "poke" the screensaver to keep it away (old X11 style): treated as nothing to do, the
    // compositor already counts real input
    fn simulate_user_activity(&self) {}
}

struct PowerInhibit(Inhibit);
#[zbus::interface(name = "org.freedesktop.PowerManagement.Inhibit")]
impl PowerInhibit {
    fn inhibit(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, application: String, reason: String) -> u32 {
        self.0.add(&hdr, application, &reason)
    }
    fn un_inhibit(&self, cookie: u32) {
        self.0.remove(cookie)
    }
    fn has_inhibit(&self) -> bool {
        !self.0.cookies.lock().unwrap().is_empty()
    }
}

/// Serve the inhibit interfaces (unless another program owns the names); drop an app's cookies when it
/// disconnects without calling UnInhibit (crashed, closed).
pub fn screensaver(tx: calloop::channel::Sender<crate::sys::Msg>) {
    let _ = std::thread::Builder::new().name("screensaver".into()).stack_size(256 * 1024).spawn(move || {
        let shared = Inhibit { cookies: Default::default(), next: std::sync::Arc::new(1.into()), tx: tx.clone() };
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.serve_at("/org/freedesktop/ScreenSaver", ScreenSaver(shared.clone_shared())))
            .and_then(|b| b.serve_at("/ScreenSaver", ScreenSaver(shared.clone_shared())))
            .and_then(|b| b.serve_at("/org/freedesktop/PowerManagement/Inhibit", PowerInhibit(shared.clone_shared())))
            .and_then(|b| b.build());
        let Ok(conn) = conn else { return };
        let mut any = false;
        for name in ["org.freedesktop.ScreenSaver", "org.freedesktop.PowerManagement", "org.freedesktop.PowerManagement.Inhibit"] {
            match conn.request_name(name) {
                Ok(_) => any = true,
                Err(e) => eprintln!("mist-ui: {name}: {e}"),
            }
        }
        if !any {
            return;
        }
        let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&conn) else { return };
        let Ok(changes) = dbus.receive_name_owner_changed() else { return };
        for ch in changes {
            let Ok(a) = ch.args() else { continue };
            if a.new_owner().is_some() {
                continue;
            }
            let gone = a.name().to_string();
            let n = {
                let mut c = shared.cookies.lock().unwrap();
                let before = c.len();
                c.retain(|_, (sender, _)| *sender != gone);
                (before != c.len()).then_some(c.len())
            };
            if let Some(n) = n {
                let _ = tx.send(crate::sys::Msg::IdleInhibit(n as u32));
            }
        }
    });
}
