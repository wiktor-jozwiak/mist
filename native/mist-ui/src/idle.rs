// Idle (was Idle.qml): lock the session after `lock` s without input, turn the monitors off after `off` s
// (back on with the first input). Apps that inhibit idle (video players, browsers playing a video, games)
// hold both off: ext-idle-notify's get_idle_notification respects inhibitors. Times in
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
}

fn cfg() -> (u32, u32) {
    let v: serde_json::Value = std::fs::read_to_string(format!("{}/.config/mist/idle.json", std::env::var("HOME").unwrap_or_default())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    (v["lock"].as_u64().unwrap_or(600) as u32, v["off"].as_u64().unwrap_or(900) as u32)
}

pub fn dpms(on: bool) {
    let _ = std::process::Command::new("hyprctl").args(["eval", &format!("hl.dispatch(hl.dsp.dpms({{ action = \"{}\" }}))", if on { "on" } else { "off" })]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map(|mut c| std::thread::spawn(move || c.wait()));
}

impl Idle {
    pub fn bind(&mut self, globals: &wayland_client::globals::GlobalList, qh: &QueueHandle<App>) {
        self.notifier = globals.bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(qh, 1..=1, ()).ok();
    }
    // (re)arm both notifications for the seat (idle.json changed or the seat appeared)
    pub fn arm(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<App>) {
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
