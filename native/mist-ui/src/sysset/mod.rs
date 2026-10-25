// Mist Settings (`mist-ui-bin settings [page]`, Super+O): the system settings in one window, laid out like
// Windows 11 Settings (sidebar of categories, a page of cards per category, sub pages with a breadcrumb,
// search over every row) in Mist's look (glass sidebar, coloured category tiles, dark translucent window).
// An appwin app like Mist Files: CPU drawn into wl_shm, no GPU context, single instance (a second launch
// hands its page to the running one). Nothing is polled while idle: data is read when a page opens or the
// window gets focus, after each change, and every 2 s only while a Bluetooth scan runs.
pub(crate) mod backend;
mod pages;
mod paint;

use crate::appwin::{self, Cmd, Cx, Ev, Logic, WinId, LEFT};
use crate::edit::{EditResult, LineEdit};
use backend::{Mon, Req, Res, Wifi};
use pages::{Act, Page};
use serde_json::{json, Value};
use smithay_client_toolkit::seat::keyboard::Keysym;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use std::any::Any;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

pub const W0: u32 = 1120;
pub const H0: u32 = 760;

// the text fields
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fid {
    Search,
    WifiPwd,
    Tz,
    Place,
    Keys,
}

// the sliders
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sl {
    Sink,
    Source,
    App(usize),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    A(Act),
    Slider(Sl, f32, f32),
    Field(Fid),
    Light(u8),
    Drag,
    Content,
    // the content's scroll bar: its track (y, height) and the thumb's height
    Scroll(f32, f32, f32),
}

#[derive(Default)]
pub struct Data {
    pub snd: Option<crate::control::Snd>,
    pub bt: Option<crate::control::Bt>,
    pub net: Option<crate::control::Net>,
    pub wifi: Vec<Wifi>,
    pub wifi_on: bool,
    pub wifi_err: String,
    pub info: Vec<(String, String)>,
    pub updates: Option<Result<Vec<String>, String>>,
    pub checking: bool,
    pub last_upgrade: String,
    pub mime: Vec<(String, Vec<String>)>,
    pub tz: String,
    pub ntp: bool,
    pub tzs: Vec<String>,
    pub profile: String,
    pub profiles: Vec<String>,
    pub mons: Vec<Mon>,
    pub drives: Vec<crate::files::fs::Drive>,
    // login, full name, groups
    pub user: (String, String, Vec<String>),
    pub face: String,
    pub night: bool,
    pub dnd: bool,
    pub notif_count: usize,
    pub idle: (u32, u32),
    pub wall: Value,
    pub contrast: bool,
    pub place: String,
    // `mistd geo` answers for the place field, and whether one is on its way
    pub found: Vec<Value>,
    pub geo_busy: bool,
}

pub struct Settings {
    pub win: Option<WinId>,
    pub w: f32,
    pub h: f32,
    pub focused: bool,
    pub page: Page,
    back: Vec<Page>,
    pub scroll: f32,
    pub max_scroll: f32,
    pub hits: Vec<(f32, f32, f32, f32, Hit)>,
    pub hover: Option<Hit>,
    pub mouse: (f32, f32),
    drag: Option<(Sl, f32, f32)>,
    // the scroll bar's thumb held: where in it (px from its top), the track (y, height), the thumb's height
    pub sb_drag: Option<(f32, f32, f32, f32)>,
    pub field: Option<Fid>,
    pub search: LineEdit,
    pub wifi_pwd: LineEdit,
    pub tz_q: LineEdit,
    pub keys_q: LineEdit,
    pub place_q: LineEdit,
    // when the typed place is searched (a short pause after the last key)
    geo_at: Option<Instant>,
    // widgets.json (the desktop and its widgets)
    pub cfg: crate::desktop::Cfg,
    pub wifi_ask: Option<String>,
    pub text: crate::text::Text,
    pub icons: crate::apps::Icons,
    pub imgs: crate::img::Images,
    pub thumbs: std::collections::HashMap<String, Option<std::rc::Rc<tiny_skia::Pixmap>>>,
    pub presets: Vec<Value>,
    pub apps: Vec<(String, String, String, String)>,
    apps_all: Option<crate::apps::Apps>,
    req: Option<Sender<Req>>,
    pub d: Data,
    pub toast: Option<(String, Instant)>,
    want: Option<Page>,
    scan_at: Option<Instant>,
    pending: Option<u64>,
}

impl Settings {
    pub fn new(page: Page) -> Settings {
        let mut s = Settings {
            win: None,
            w: W0 as f32,
            h: H0 as f32,
            focused: true,
            page,
            back: Vec::new(),
            scroll: 0.0,
            max_scroll: 0.0,
            hits: Vec::new(),
            hover: None,
            mouse: (-1.0, -1.0),
            drag: None,
            sb_drag: None,
            field: None,
            search: LineEdit::default(),
            wifi_pwd: LineEdit::default(),
            tz_q: LineEdit::default(),
            keys_q: LineEdit::default(),
            place_q: LineEdit::default(),
            geo_at: None,
            cfg: crate::desktop::Cfg::load(),
            wifi_ask: None,
            text: crate::text::Text::new(),
            icons: crate::apps::Icons::new(),
            imgs: crate::img::Images::new(),
            thumbs: Default::default(),
            presets: crate::presets::load(),
            apps: Vec::new(),
            apps_all: None,
            req: None,
            d: Data::default(),
            toast: None,
            want: None,
            scan_at: None,
            pending: None,
        };
        s.read_local();
        s
    }

    // the small local state: config files, the user, the disks (all quick reads)
    fn read_local(&mut self) {
        self.cfg = crate::desktop::Cfg::load();
        let d = &mut self.d;
        let st = backend::read_json("settings.json");
        d.contrast = st["contrast"] == "high";
        let idle = backend::read_json("idle.json");
        d.idle = (idle["lock"].as_u64().unwrap_or(600) as u32, idle["off"].as_u64().unwrap_or(900) as u32);
        d.wall = backend::read_json("wallpaper.json");
        d.night = crate::control::night_on();
        let n: Value = std::fs::read_to_string(format!("{}/.local/state/mist/notifications.json", backend::home())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        d.dnd = n["dnd"].as_bool().unwrap_or(false);
        d.notif_count = n["entries"].as_array().map(|a| a.len()).unwrap_or(0);
        d.place = backend::read_json("location.json")["city"].as_str().unwrap_or("").to_string();
        let login = std::env::var("USER").unwrap_or_default();
        let pw = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
        let full = pw.lines().find(|l| l.starts_with(&format!("{login}:"))).and_then(|l| l.split(':').nth(4)).map(|g| g.split(',').next().unwrap_or("").to_string()).filter(|g| !g.is_empty()).unwrap_or_else(|| login.clone());
        let groups = std::fs::read_to_string("/etc/group").unwrap_or_default().lines().filter(|l| l.rsplit(':').next().map(|m| m.split(',').any(|u| u == login)).unwrap_or(false)).filter_map(|l| l.split(':').next().map(String::from)).collect();
        d.user = (login, full, groups);
        d.face = [format!("{}/.face", backend::home()), format!("/var/lib/AccountsService/icons/{}", d.user.0)].into_iter().find(|p| std::path::Path::new(p).is_file()).unwrap_or_default();
        d.drives = crate::files::fs::drives();
    }

    pub fn accent_now(&self) -> u32 {
        let c = crate::theme::accent();
        ((c.0 * 255.0).round() as u32) << 16 | ((c.1 * 255.0).round() as u32) << 8 | (c.2 * 255.0).round() as u32
    }
    // the shown wallpaper: its thumbnail and name
    pub fn wall_now(&self) -> (String, String) {
        let live = self.d.wall["live"].as_str().unwrap_or("");
        if !live.is_empty() {
            let name = self.presets.iter().find(|p| p["id"] == live && p.get("mode").is_some()).and_then(|p| p["name"].as_str()).unwrap_or(live).to_string();
            return (crate::presets::thumb_path(&format!("live:{live}")), name);
        }
        let img = self.d.wall["image"].as_str().unwrap_or("").to_string();
        let name = img.rsplit('/').next().unwrap_or("").rsplit_once('.').map(|x| x.0.replace(['-', '_'], " ")).unwrap_or_default();
        (img, name)
    }
    pub fn app_list(&self) -> &Vec<(String, String, String, String)> {
        &self.apps
    }
    pub fn app_name(&self, id: &str) -> String {
        self.apps.iter().find(|a| a.0 == id).map(|a| a.1.clone()).unwrap_or_else(|| id.rsplit('.').next().unwrap_or(id).to_string())
    }
    fn load_apps(&mut self) {
        let a = crate::apps::Apps::load();
        let mut v: Vec<(String, String, String, String)> = a.entries.iter().filter(|e| !e.no_display && !e.exec.is_empty()).map(|e| (e.id.clone(), e.name.clone(), e.icon.clone(), if e.comment.is_empty() { e.generic.clone() } else { e.comment.clone() })).collect();
        v.sort_by_key(|x| x.1.to_lowercase());
        // a name shown by the mime chips also for hidden entries
        self.apps_all = Some(a);
        self.apps = v;
    }

    fn send(&self, r: Req) {
        if let Some(t) = &self.req {
            let _ = t.send(r);
        }
    }
    // what a page needs, asked when it opens
    fn enter(&mut self, p: Page) {
        use Page::*;
        let mut reqs: Vec<Req> = Vec::new();
        match p {
            Home => {
                reqs.extend([Req::Net, Req::Bt, Req::Snd, Req::Info]);
                if self.d.updates.is_none() && !self.d.checking {
                    self.d.checking = true;
                    reqs.push(Req::Updates);
                }
            }
            System | Display => reqs.push(Req::Mons),
            Sound => reqs.push(Req::Snd),
            Devices | Bluetooth => reqs.push(Req::Bt),
            Network => reqs.push(Req::Net),
            About => reqs.push(Req::Info),
            Defaults => reqs.push(Req::Mime),
            Time => reqs.extend([Req::Time, Req::Tzs]),
            Power => reqs.push(Req::Profile),
            Storage => self.d.drives = crate::files::fs::drives(),
            Updates if self.d.updates.is_none() && !self.d.checking => {
                self.d.checking = true;
                reqs.push(Req::Updates)
            }
            _ => {}
        }
        if !matches!(p, Wallpaper | Desktop) && !self.thumbs.is_empty() {
            // ~110 kB per decoded thumbnail: only while the page shows them
            self.thumbs.clear();
            unsafe { libc::malloc_trim(0) };
        }
        for r in reqs {
            self.send(r);
        }
    }
    pub fn go(&mut self, p: Page, cx: &mut Cx) {
        if p != self.page {
            self.back.push(self.page);
            if self.back.len() > 32 {
                self.back.remove(0);
            }
        }
        self.page = p;
        self.scroll = 0.0;
        self.wifi_ask = None;
        if self.field != Some(Fid::Search) {
            self.field = None;
        }
        self.search.set("");
        self.field = None;
        self.enter(p);
        self.redraw(cx);
    }
    fn redraw(&self, cx: &mut Cx) {
        if let Some(id) = self.win {
            cx.redraw(id);
        }
    }
    fn say(&mut self, s: &str, cx: &mut Cx) {
        self.toast = Some((s.to_string(), Instant::now() + Duration::from_millis(2600)));
        cx.wake_at(Instant::now() + Duration::from_millis(2650));
    }
    // settings.json changed: the shell re-reads language, accent, contrast
    fn tell_shell(&self) {
        backend::shell_msg(&["ui", "settings"]);
    }

    fn set_slider(&mut self, s: Sl, v: f32) {
        let v = v.clamp(0.0, 1.0);
        let pct = format!("{}%", (v * 100.0).round() as i32);
        let Some(snd) = &mut self.d.snd else { return };
        match s {
            Sl::Sink => {
                if let Some(d) = snd.sinks.iter_mut().find(|d| d.name == snd.sink) {
                    d.vol = v;
                }
                self.send(Req::Run(vec!["pactl".into(), "set-sink-volume".into(), "@DEFAULT_SINK@".into(), pct], "snd"));
            }
            Sl::Source => {
                if let Some(d) = snd.sources.iter_mut().find(|d| d.name == snd.source) {
                    d.vol = v;
                }
                self.send(Req::Run(vec!["pactl".into(), "set-source-volume".into(), "@DEFAULT_SOURCE@".into(), pct], "snd"));
            }
            Sl::App(i) => {
                let Some(a) = snd.apps.get_mut(i) else { return };
                a.vol = v;
                let ins = a.inputs.clone();
                for ix in ins {
                    self.send(Req::Run(vec!["pactl".into(), "set-sink-input-volume".into(), ix.to_string(), pct.clone()], "snd"));
                }
            }
        }
    }

    pub fn act(&mut self, a: Act, cx: &mut Cx) {
        let run = |v: &[&str], then: &'static str| Req::Run(v.iter().map(|s| s.to_string()).collect(), then);
        match a {
            Act::Nav(p) => return self.go(p, cx),
            Act::Back => {
                if let Some(p) = self.back.pop() {
                    self.page = p;
                    self.scroll = 0.0;
                    self.enter(p);
                } else if let Some(p) = self.page.parent() {
                    self.page = p;
                    self.scroll = 0.0;
                    self.enter(p);
                }
            }
            Act::Lang(code) => {
                crate::i18n::set(code);
                self.load_apps();
                self.tell_shell();
            }
            Act::Accent(c) => {
                backend::patch_json("settings.json", json!({ "accent": format!("#{c:06x}") }));
                crate::theme::reload_accent();
                self.tell_shell();
            }
            Act::Contrast(on) => {
                backend::patch_json("settings.json", json!({ "contrast": if on { json!("high") } else { Value::Null } }));
                crate::theme::reload_contrast();
                self.d.contrast = on;
                self.tell_shell();
            }
            Act::Night(on) => {
                let _ = std::fs::create_dir_all(format!("{}/.local/state/mist", backend::home()));
                let _ = std::fs::write(crate::control::night_path(), if on { "1" } else { "0" });
                let c = crate::control::night_cmd(on);
                appwin::spawn(&c.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                self.d.night = on;
            }
            Act::Dnd(on) => {
                backend::shell_msg(&["notif", "dnd", if on { "true" } else { "false" }]);
                self.d.dnd = on;
            }
            Act::NotifClear => {
                backend::shell_msg(&["notif", "clear"]);
                self.d.notif_count = 0;
                self.say("Wyczyszczono powiadomienia", cx);
            }
            Act::WallLive(id) => self.d.wall = backend::patch_json("wallpaper.json", json!({ "live": id })),
            Act::WallPhoto(p) => self.d.wall = backend::patch_json("wallpaper.json", json!({ "live": "", "image": p })),
            Act::WallMusic(on) => self.d.wall = backend::patch_json("wallpaper.json", json!({ "music": on })),
            Act::WallTempo(t) => self.d.wall = backend::patch_json("wallpaper.json", json!({ "tempo": t })),
            Act::Gallery => backend::shell_msg(&["mist", "toggle", "wallpaper"]),
            Act::Desk(card, key, v) => {
                self.cfg.set(card, key, v);
                self.tell_shell();
            }
            Act::DeskChip(card, key, v) => {
                if let Some(gr) = crate::desktop::groups(card).into_iter().find(|g| g.key == key) {
                    self.cfg.click(card, &gr, &v);
                }
                self.tell_shell();
            }
            Act::DeskSlot(i, k) => {
                self.cfg.put_slot(i, k);
                self.tell_shell();
            }
            Act::WidgetShow(k, on) => {
                let n = self.cfg.cards();
                if on {
                    // one more place, the widget into the last one
                    if n < 3 {
                        self.cfg.set("desk", "cards", json!(n + 1));
                    }
                    let n = self.cfg.cards();
                    if n > 0 {
                        self.cfg.put_slot(n - 1, k);
                    }
                } else {
                    // out of the shown places, one place less
                    let mut o = self.cfg.order();
                    if let Some(p) = o.iter().position(|x| x == k).filter(|p| *p < n) {
                        let x = o.remove(p);
                        o.push(x);
                        self.cfg.set("order", "", json!(o));
                        self.cfg.set("desk", "cards", json!(n.saturating_sub(1)));
                    }
                }
                self.tell_shell();
            }
            Act::Place(i) => {
                if let Some(v) = self.d.found.get(i).cloned() {
                    backend::write_atomic(&backend::cfg("location.json"), &json!({"city": v["city"], "lat": v["lat"], "lon": v["lon"]}).to_string());
                    self.d.place = v["city"].as_str().unwrap_or("").to_string();
                    self.d.found.clear();
                    self.place_q.set("");
                    self.field = None;
                    // the shell's weather thread asks for the new place now
                    self.tell_shell();
                    self.say(&format!("Pogoda dla: {}", self.d.place), cx);
                }
            }
            Act::Primary(name) => {
                backend::patch_json("settings.json", json!({ "primary": name }));
            }
            Act::Restart => backend::restart_shell(),
            Act::Sink(n) => self.send(run(&["pactl", "set-default-sink", &n], "snd")),
            Act::Source(n) => self.send(run(&["pactl", "set-default-source", &n], "snd")),
            Act::SinkMute => self.send(run(&["pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle"], "snd")),
            Act::SourceMute => self.send(run(&["pactl", "set-source-mute", "@DEFAULT_SOURCE@", "toggle"], "snd")),
            Act::AppMute(i) => {
                for ix in self.d.snd.as_ref().and_then(|s| s.apps.get(i)).map(|a| a.inputs.clone()).unwrap_or_default() {
                    self.send(run(&["pactl", "set-sink-input-mute", &ix.to_string(), "toggle"], "snd"));
                }
            }
            Act::BtPower(on) => self.send(Req::BtPower(on)),
            Act::BtScan(on) => {
                self.send(Req::BtScan(on));
                self.scan_at = on.then(|| Instant::now() + Duration::from_secs(2));
                if on {
                    cx.wake_at(Instant::now() + Duration::from_secs(2));
                }
            }
            Act::BtConn(p, on) => {
                self.send(Req::BtConnect(p, on));
                self.say(if on { "Łączę…" } else { "Rozłączam…" }, cx);
            }
            Act::BtPair(p) => {
                self.send(Req::BtPair(p));
                self.say("Paruję…", cx);
            }
            Act::Wifi(ssid) => {
                let w = self.d.wifi.iter().find(|w| w.ssid == ssid).cloned().unwrap_or_default();
                self.d.wifi_err.clear();
                if w.known || !w.secure {
                    self.send(Req::Wifi(ssid, if w.known { None } else { Some(String::new()) }));
                    self.say("Łączę…", cx);
                } else {
                    self.wifi_ask = Some(ssid);
                    self.wifi_pwd.set("");
                    self.field = Some(Fid::WifiPwd);
                }
            }
            Act::WifiOn(on) => {
                self.d.wifi_on = on;
                self.send(run(&["nmcli", "radio", "wifi", if on { "on" } else { "off" }], "net"));
            }
            Act::NetEditor => appwin::spawn(&["nm-connection-editor"]),
            Act::Idle(k, s) => {
                backend::patch_json("idle.json", json!({ k: s }));
                if k == "lock" {
                    self.d.idle.0 = s;
                } else {
                    self.d.idle.1 = s;
                }
            }
            Act::Profile(p) => {
                self.d.profile = p.clone();
                self.send(run(&["powerprofilesctl", "set", &p], "profile"));
            }
            Act::SessionMenu => backend::shell_msg(&["mist", "toggle", "session"]),
            Act::Lock => appwin::spawn(&[&backend::bin(), "lock"]),
            Act::LockPreview => appwin::spawn(&[&backend::bin(), "lock", "preview", "6"]),
            Act::Scale(n, s) => {
                backend::set_scale(&self.d.mons, &n, s);
                if let Some(m) = self.d.mons.iter_mut().find(|m| m.name == n) {
                    m.scale = s;
                }
                self.pending = Some(1);
                cx.wake_at(Instant::now() + Duration::from_millis(400));
            }
            Act::Comp("__paccache", _) => appwin::spawn(&["kitty", "-e", "sh", "-c", "sudo paccache -r; echo; read -p 'Enter: zamknij' x"]),
            Act::Comp("__overview", _) => {
                backend::comp_ipc("action overview");
            }
            Act::Comp(k, v) => backend::comp_set(k, &v),
            Act::DefApp(i, id) => {
                let mut v: Vec<String> = vec!["xdg-mime".into(), "default".into(), format!("{id}.desktop")];
                v.extend(backend::MIME_CATS[i].2.iter().map(|m| m.to_string()));
                if let Some(m) = self.d.mime.get_mut(i) {
                    m.0 = id;
                }
                self.send(Req::Run(v, "mime"));
            }
            Act::Launch(id) => {
                if let Some(e) = self.apps_all.as_ref().and_then(|a| a.entries.iter().find(|e| e.id == id)) {
                    let exec: Vec<&str> = e.exec.split_whitespace().filter(|a| !(a.len() == 2 && a.starts_with('%'))).collect();
                    let cmd = exec.join(" ");
                    crate::app::sh(&if e.terminal { format!("kitty -e {cmd}") } else { cmd });
                }
            }
            Act::Ntp(on) => {
                self.d.ntp = on;
                self.send(run(&["timedatectl", "set-ntp", if on { "true" } else { "false" }], "time"));
            }
            Act::Tz(z) => {
                self.d.tz = z.clone();
                self.tz_q.set("");
                self.field = None;
                self.send(run(&["timedatectl", "set-timezone", &z], "time"));
            }
            Act::Passwd => appwin::spawn(&["kitty", "-e", "passwd"]),
            Act::Update => {
                let up = if std::path::Path::new("/usr/bin/yay").exists() { "yay -Syu" } else { "sudo pacman -Syu" };
                appwin::spawn(&["kitty", "-e", "sh", "-c", &format!("{up}; echo; read -p 'Enter: zamknij' x")]);
                self.d.updates = None;
            }
            Act::CheckUpdates => {
                self.d.checking = true;
                self.send(Req::Updates);
            }
            Act::Open(p) => appwin::spawn(&[&backend::bin(), "files", &p]),
            Act::EditComp => {
                let p = backend::cfg("comp.conf");
                if !std::path::Path::new(&p).exists() {
                    // the flags only: binds and autostart stay the defaults until the file has its own
                    backend::comp_set("numlock", &{
                        let v = backend::comp_get("numlock");
                        if v.is_empty() { "off".to_string() } else { v }
                    });
                }
                appwin::spawn(&["gnome-text-editor", &p]);
            }
            Act::CopyInfo => {
                let s: Vec<String> = self.d.info.iter().map(|(k, v)| format!("{}: {v}", crate::i18n::tr(k))).collect();
                crate::edit::copy_text(&s.join("\n"));
                self.say("Skopiowano", cx);
            }
            Act::Zoom(z) => {
                backend::comp_ipc(&format!("action zoom {z}"));
            }
            Act::Url(u) => {
                appwin::spawn(&["xdg-open", &u]);
            }
        }
        self.redraw(cx);
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|h| x >= h.0 && y >= h.1 && x < h.0 + h.2 && y < h.1 + h.3).map(|h| h.4.clone())
    }
    fn edit_of(&mut self, f: Fid) -> &mut LineEdit {
        match f {
            Fid::Search => &mut self.search,
            Fid::WifiPwd => &mut self.wifi_pwd,
            Fid::Tz => &mut self.tz_q,
            Fid::Keys => &mut self.keys_q,
            Fid::Place => &mut self.place_q,
        }
    }

    fn key(&mut self, k: Keysym, utf8: Option<String>, cx: &mut Cx) {
        let m = cx.mods;
        if m.ctrl && (k == Keysym::w || k == Keysym::q) {
            if let Some(id) = self.win {
                cx.cmds.push(Cmd::Close(id));
            }
            self.win = None;
            cx.cmds.push(Cmd::Exit);
            return;
        }
        if m.ctrl && k == Keysym::f {
            self.field = Some(Fid::Search);
            self.search.select_all();
            return self.redraw(cx);
        }
        if m.alt && k == Keysym::Left {
            return self.act(Act::Back, cx);
        }
        if let Some(f) = self.field {
            let r = self.edit_of(f).key(k, utf8.as_deref(), m.ctrl, m.shift);
            match r {
                EditResult::Accept => match f {
                    Fid::WifiPwd => {
                        let pwd = self.wifi_pwd.text.clone();
                        if let Some(ssid) = self.wifi_ask.take() {
                            self.send(Req::Wifi(ssid, Some(pwd)));
                            self.say("Łączę…", cx);
                        }
                        self.wifi_pwd.set("");
                        self.field = None;
                    }
                    Fid::Tz => {
                        let q = self.tz_q.text.to_lowercase();
                        if let Some(z) = self.d.tzs.iter().find(|z| z.to_lowercase().contains(&q)).cloned() {
                            return self.act(Act::Tz(z), cx);
                        }
                    }
                    Fid::Search => {
                        // Enter opens the first result
                        if let Some(p) = paint::search_hits(self).first().map(|r| r.1) {
                            return self.go(p, cx);
                        }
                    }
                    Fid::Keys => self.field = None,
                    Fid::Place => {
                        if self.d.found.is_empty() {
                            self.geo_at = Some(Instant::now());
                            self.tick(cx);
                        } else {
                            return self.act(Act::Place(0), cx);
                        }
                    }
                },
                EditResult::Cancel => {
                    if f == Fid::WifiPwd {
                        self.wifi_ask = None;
                    }
                    self.edit_of(f).set("");
                    self.field = None;
                }
                EditResult::Used => {
                    if f == Fid::Search {
                        self.scroll = 0.0;
                    }
                    if f == Fid::Place {
                        // searched once typing pauses, not on every key
                        let at = Instant::now() + Duration::from_millis(350);
                        self.geo_at = Some(at);
                        cx.wake_at(at);
                    }
                }
                EditResult::Ignored => {}
            }
            return self.redraw(cx);
        }
        match k {
            Keysym::Escape | Keysym::BackSpace => self.act(Act::Back, cx),
            Keysym::Up => self.scroll_by(-60.0, cx),
            Keysym::Down => self.scroll_by(60.0, cx),
            Keysym::Page_Up => self.scroll_by(-(self.h - 140.0), cx),
            Keysym::Page_Down => self.scroll_by(self.h - 140.0, cx),
            _ => {
                // typing anywhere searches (like the Windows Settings box)
                if let Some(t) = utf8.filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()) && !m.ctrl && !m.alt) {
                    self.field = Some(Fid::Search);
                    self.search.set("");
                    self.search.insert(&t);
                    self.scroll = 0.0;
                    self.redraw(cx);
                }
            }
        }
    }
    // the held scroll bar thumb follows the pointer
    fn sb_to(&mut self, y: f32, cx: &mut Cx) {
        let Some((grab, ty, th, tbh)) = self.sb_drag else { return };
        let k = ((y - grab - ty) / (th - tbh).max(1.0)).clamp(0.0, 1.0);
        let s = (k * self.max_scroll).round();
        if s != self.scroll {
            self.scroll = s;
        }
        self.redraw(cx);
    }
    fn scroll_by(&mut self, dy: f32, cx: &mut Cx) {
        let s = (self.scroll + dy).clamp(0.0, self.max_scroll.max(0.0));
        if s != self.scroll {
            self.scroll = s;
            self.redraw(cx);
        }
    }

    fn press(&mut self, x: f32, y: f32, b: u32, cx: &mut Cx) {
        let Some(id) = self.win else { return };
        if b != LEFT {
            return;
        }
        let h = self.hit_at(x, y);
        // a click elsewhere leaves the field (the Wi‑Fi password stays until Escape / Enter)
        if !matches!(h, Some(Hit::Field(_))) && self.field.is_some() && self.field != Some(Fid::WifiPwd) {
            self.field = None;
            self.redraw(cx);
        }
        match h {
            Some(Hit::Light(0)) => {
                cx.cmds.push(Cmd::Close(id));
                self.win = None;
                cx.cmds.push(Cmd::Exit);
            }
            Some(Hit::Light(1)) => cx.cmds.push(Cmd::Minimize(id)),
            Some(Hit::Light(_)) => cx.cmds.push(Cmd::Maximize(id)),
            Some(Hit::Drag) => cx.cmds.push(Cmd::Move(id)),
            Some(Hit::A(a)) => self.act(a, cx),
            Some(Hit::Slider(s, x0, w)) => {
                self.drag = Some((s, x0, w));
                self.set_slider(s, (x - x0) / w);
                self.redraw(cx);
            }
            Some(Hit::Scroll(ty, th, tbh)) => {
                // on the thumb: hold it where it was taken; on the track: the thumb's middle jumps there
                let at = ty + self.scroll / self.max_scroll.max(1.0) * (th - tbh);
                let grab = if y >= at && y <= at + tbh { y - at } else { tbh / 2.0 };
                self.sb_drag = Some((grab, ty, th, tbh));
                self.sb_to(y, cx);
            }
            Some(Hit::Field(f)) => {
                self.field = Some(f);
                if f == Fid::Tz && self.d.tzs.is_empty() {
                    self.send(Req::Tzs);
                }
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn pointer(&mut self, ev: Ev, cx: &mut Cx) {
        match ev {
            Ev::Motion(x, y) => {
                self.mouse = (x, y);
                if let Some((s, x0, w)) = self.drag {
                    self.set_slider(s, (x - x0) / w);
                    return self.redraw(cx);
                }
                if self.sb_drag.is_some() {
                    return self.sb_to(y, cx);
                }
                let h = self.hit_at(x, y);
                if h != self.hover {
                    let c = match &h {
                        Some(Hit::A(_)) | Some(Hit::Slider(..)) | Some(Hit::Light(_)) | Some(Hit::Scroll(..)) => CursorIcon::Pointer,
                        Some(Hit::Field(_)) => CursorIcon::Text,
                        _ => CursorIcon::Default,
                    };
                    cx.cursor(c);
                    self.hover = h;
                    self.redraw(cx);
                }
            }
            Ev::Leave => {
                self.mouse = (-1.0, -1.0);
                if self.hover.take().is_some() {
                    self.redraw(cx);
                }
            }
            Ev::Press(x, y, b, _) => self.press(x, y, b, cx),
            Ev::Release(..) => {
                if self.sb_drag.take().is_some() {
                    self.redraw(cx);
                }
                if self.drag.take().is_some() {
                    self.send(Req::Snd);
                }
            }
            Ev::Scroll(x, _, dy) => {
                // a wheel notch comes as 15 px: 60 px per notch (like the arrow keys), Mist Files takes 3x
                if x > paint::SIDE_X + paint::SIDE_W {
                    self.scroll_by(dy * 4.0, cx);
                }
            }
            _ => {}
        }
    }

    fn apply_res(&mut self, r: Res, cx: &mut Cx) {
        let d = &mut self.d;
        match r {
            Res::Snd(s) => {
                // a slider being dragged keeps its own value
                if self.drag.is_none() {
                    d.snd = Some(s);
                }
            }
            Res::Bt(b) => {
                if !b.discovering {
                    self.scan_at = None;
                }
                d.bt = Some(b);
            }
            Res::Net(n, w) => {
                d.wifi_on = !w.is_empty() || std::process::Command::new("nmcli").args(["radio", "wifi"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled").unwrap_or(false);
                d.net = Some(n);
                d.wifi = w;
            }
            Res::Info(i) => d.info = i,
            Res::Updates(u, last) => {
                d.updates = Some(u);
                d.last_upgrade = last;
                d.checking = false;
            }
            Res::Mime(m) => d.mime = m,
            Res::Time(tz, ntp) => {
                d.tz = tz;
                d.ntp = ntp;
            }
            Res::Tzs(t) => d.tzs = t,
            Res::Geo(q, v) => {
                // only the answer for what the field holds now
                if q == self.place_q.text.trim() {
                    d.found = v;
                    d.geo_busy = false;
                }
            }
            Res::Profile(p, all) => {
                d.profile = p;
                d.profiles = all;
            }
            Res::Mons(m) => d.mons = m,
            Res::BtDone => {}
            Res::WifiErr(e) => {
                d.wifi_err = e;
            }
        }
        self.redraw(cx);
    }
}

impl Logic for Settings {
    fn app_id(&self) -> &str {
        "mist-settings"
    }
    fn opened(&mut self, id: WinId, _token: u64, cx: &mut Cx) {
        self.win = Some(id);
        if self.req.is_none() {
            let tx = std::sync::Mutex::new(cx.tx.clone());
            self.req = Some(backend::start(std::sync::Arc::new(move |m| tx.lock().map(|t| t.send(m).is_ok()).unwrap_or(false))));
        }
        self.load_apps();
        let p = self.want.take().unwrap_or(self.page);
        self.page = p;
        // the home page's tiles, the about data for the search and the hero
        self.send(Req::Info);
        self.enter(p);
        if p != Page::Home {
            self.send(Req::Net);
            self.send(Req::Bt);
        }
    }
    fn paint(&mut self, _id: WinId, c: &mut crate::gfx::Canvas, _cx: &mut Cx) {
        self.paint_win(c);
    }
    fn event(&mut self, id: WinId, ev: Ev, cx: &mut Cx) {
        match ev {
            Ev::Key(k, u) => self.key(k, u, cx),
            Ev::Focus(f) => {
                self.focused = f;
                if f {
                    // things change outside (another app, the control centre): read the page again
                    self.read_local();
                    self.enter(self.page);
                }
                cx.redraw(id);
            }
            Ev::Resized => {
                self.scroll = self.scroll.min(self.max_scroll.max(0.0));
            }
            e => self.pointer(e, cx),
        }
    }
    fn close_requested(&mut self, id: WinId, cx: &mut Cx) {
        cx.cmds.push(Cmd::Close(id));
        self.win = None;
        cx.cmds.push(Cmd::Exit);
    }
    fn tick(&mut self, cx: &mut Cx) {
        let now = Instant::now();
        if self.toast.as_ref().is_some_and(|t| t.1 <= now) {
            self.toast = None;
            self.redraw(cx);
        }
        if self.pending.take().is_some() {
            self.send(Req::Mons);
        }
        if let Some(t) = self.geo_at {
            if t <= now {
                self.geo_at = None;
                let q = self.place_q.text.trim().to_string();
                if q.chars().count() >= 2 {
                    self.d.geo_busy = true;
                    self.send(Req::Geo(q));
                } else {
                    self.d.found.clear();
                }
                self.redraw(cx);
            } else {
                cx.wake_at(t);
            }
        }
        // a Bluetooth scan: new devices every 2 s while the page shows them
        if let Some(t) = self.scan_at {
            if t <= now {
                if matches!(self.page, Page::Bluetooth | Page::Devices) {
                    self.send(Req::Bt);
                }
                self.scan_at = Some(now + Duration::from_secs(2));
                cx.wake_at(now + Duration::from_secs(2));
            } else {
                cx.wake_at(t);
            }
        }
    }
    fn message(&mut self, args: Vec<String>, cx: &mut Cx) {
        // another launch: its page (or the current one); a closed window opens again
        let p = args.first().and_then(|a| Page::from_id(a));
        if self.win.is_none() {
            self.want = p;
            cx.new_window("Ustawienia", W0, H0, (720, 520));
            return;
        }
        if let Some(p) = p {
            self.go(p, cx);
        }
    }
    fn async_msg(&mut self, m: Box<dyn Any + Send>, cx: &mut Cx) {
        if let Ok(r) = m.downcast::<Res>() {
            self.apply_res(*r, cx);
        }
    }
}

// offscreen check: `settings --png out.png [page] [WxH]` (data read synchronously, no window)
fn render_png(args: &[String]) -> i32 {
    let out = &args[1];
    let page = args.get(2).and_then(|s| Page::from_id(s)).unwrap_or(Page::Home);
    let (w, h) = args.get(3).and_then(|s| s.split_once('x')).map(|(a, b)| (a.parse().unwrap_or(W0), b.parse().unwrap_or(H0))).unwrap_or((W0, H0));
    let mut s = Settings::new(page);
    s.load_apps();
    // the worker's answers, read here in line
    let (tx, rx) = std::sync::mpsc::channel::<Box<dyn Any + Send>>();
    let tx = std::sync::Mutex::new(tx);
    let req = backend::start(std::sync::Arc::new(move |m| tx.lock().map(|t| t.send(m).is_ok()).unwrap_or(false)));
    for r in [Req::Snd, Req::Bt, Req::Net, Req::Info, Req::Mime, Req::Time, Req::Profile, Req::Mons] {
        let _ = req.send(r);
    }
    let t0 = Instant::now();
    let mut got = 0;
    while got < 8 && t0.elapsed() < Duration::from_secs(8) {
        if let Ok(m) = rx.recv_timeout(Duration::from_millis(200)) {
            if let Ok(r) = m.downcast::<Res>() {
                apply_plain(&mut s, *r);
                got += 1;
            }
        }
    }
    if let Some(q) = args.iter().find_map(|a| a.strip_prefix("q=")) {
        s.search.set(q);
    }
    if let Some(y) = args.iter().find_map(|a| a.strip_prefix("scroll=")).and_then(|v| v.parse().ok()) {
        s.scroll = y;
    }
    let mut c = crate::gfx::Canvas::new(w, h);
    let n: u32 = std::env::var("MIST_BENCH").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    s.paint_win(&mut c);
    if n > 0 {
        let t = Instant::now();
        for _ in 0..n {
            s.paint_win(&mut c);
        }
        println!("paint {:.2} ms", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    }
    c.pm.save_png(out).map(|_| 0).unwrap_or(1)
}
// Res without a window (render_png)
fn apply_plain(s: &mut Settings, r: Res) {
    let d = &mut s.d;
    match r {
        Res::Snd(x) => d.snd = Some(x),
        Res::Bt(x) => d.bt = Some(x),
        Res::Net(n, w) => {
            d.net = Some(n);
            d.wifi = w;
        }
        Res::Info(i) => d.info = i,
        Res::Mime(m) => d.mime = m,
        Res::Time(tz, ntp) => {
            d.tz = tz;
            d.ntp = ntp;
        }
        Res::Profile(p, all) => {
            d.profile = p;
            d.profiles = all;
        }
        Res::Mons(m) => d.mons = m,
        _ => {}
    }
}

// a page of Mist Settings from the shell: the old Ustawienia panel's names map to their pages here
pub fn open(page: &str) {
    let id = match page {
        "" | "desk" => "desktop",
        "general" => "home",
        "lock" => "lockscreen",
        "notif" => "notifications",
        p => p,
    };
    let id = if Page::from_id(id).is_some() { id } else { "desktop" };
    appwin::spawn(&[&backend::bin(), "settings", id]);
}

pub fn run(args: &[String]) -> i32 {
    if args.first().map(|s| s.as_str()) == Some("--png") {
        return render_png(args);
    }
    if appwin::forward("settings", args) {
        return 0;
    }
    // the launch's arguments come to message(), which opens the window
    appwin::run("settings", Box::new(Settings::new(Page::Home)))
}
