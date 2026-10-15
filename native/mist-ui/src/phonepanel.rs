// Phone panel (was PhonePanel.qml, popup "phone"): everything about the phone in one sheet.
//   left   the phone (battery, network, ADB details), pairing, quick actions, send a link / text, media on the phone
//   right  tabs: notifications (reply, actions, dismiss), SMS threads + one conversation / a new message, contacts
// The state is phone.rs's (mistd phone + mistd calls); commands go straight to mistd over its stdin.
// The lists are virtualised: only the rows inside the page are drawn, into one page-sized layer.
use crate::anim::{Anim, Ease};
use crate::desktop::{border, circle, line_h, rect, text};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::phone::{digits9, Phone};
use crate::text::{Style, Text};
use crate::theme::*;
use serde_json::{json, Value};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::time::Instant;

const W: f32 = 1020.0; // the QML window 1040 x 760, the sheet 10 px inside
const H: f32 = 740.0;
const INSET: Rgba = Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.26);
const RIM: Rgba = Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16);
const ACC_SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);
const ACC_STRONG: Rgba = Rgba::rgb8(0x40, 0x9C, 0xFF, 1.0);
const M_ACC: Rgba = Rgba::rgb8(0x8a, 0xd6, 0xd0, 1.0);
const INK: Rgba = Rgba::rgb8(0x0e, 0x14, 0x20, 1.0);
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const PAGE_W: f32 = W - 441.0 - 52.0;
const LEFT_W: f32 = 372.0;
const DAYS: [&str; 7] = ["nd", "pon", "wt", "śr", "czw", "pt", "sob"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum F {
    Push,
    Reply,
    To,
    Msg,
    Search,
}
#[derive(Clone, PartialEq, Debug)]
enum Hit {
    Sheet,
    Accept,
    Reject,
    Pair,
    Act(u8),
    Field(F),
    Send(F),
    MPrev,
    MPlay,
    MNext,
    Tab(&'static str),
    ClearAll,
    NoteX(String),
    NoteReply(String),
    NoteAct(String, String),
    NewSms,
    Thread(usize),
    Back,
    CallThread,
    ToHint(String),
    PSms(String),
    PCall(String, String),
}

pub struct PhonePanel {
    ph: Phone,
    main: Value,
    ok: bool,
    threads: Vec<Value>,
    convs: Value,
    contacts: Vec<Value>,
    adb: Value,
    hfp: bool,
    tab: &'static str,
    thread: Option<(Value, String, Vec<String>)>, // thread id, name, addresses
    composing: bool,
    push: Field,
    reply: Field,
    to: Field,
    msg: Field,
    search: Field,
    focus: Option<F>,
    replying: Option<String>,
    said: String,
    said_at: Option<Instant>,
    toast: Anim,
    scroll: f32,
    content_h: f32,
    conv_n: usize,
    hits: Vec<((f32, f32, f32, f32), Hit)>,
    ptr: (f32, f32),
    pressed_outside: bool,
    adb_at: Option<Instant>,
    page: Canvas,
    origin: (f32, f32),
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}
fn hue(s: &str) -> Rgba {
    // Qt.hsla(h, 0.45, 0.72)
    let mut h: u32 = 0;
    for c in s.chars() {
        h = (h * 31 + c as u32) % 360;
    }
    let (h, sat, l) = (h as f32 / 360.0, 0.45f32, 0.72f32);
    let q = if l < 0.5 { l * (1.0 + sat) } else { l + sat - l * sat };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = (t + 1.0) % 1.0;
        if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t } else if t < 0.5 { q } else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 } else { p }
    };
    Rgba(f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0), 1.0)
}
fn initials(n: &str) -> String {
    if n.starts_with(|c: char| c == '+' || c.is_ascii_digit()) {
        return "#".into();
    }
    let clean: String = n.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
    let p: Vec<&str> = clean.split_whitespace().collect();
    let a = p.first().and_then(|w| w.chars().next()).unwrap_or('?');
    let b = p.get(1).and_then(|w| w.chars().next());
    format!("{a}{}", b.map(String::from).unwrap_or_default()).to_uppercase()
}
fn when(ms: i64) -> String {
    let (t, n) = (ms / 1000, crate::data::now());
    let (lt, ln) = (crate::data::local(t), crate::data::local(n));
    let day = |x: &libc::tm| (x.tm_year, x.tm_yday);
    if day(&lt) == day(&ln) {
        return crate::data::hm(t);
    }
    if day(&lt) == day(&crate::data::local(n - 86400)) {
        return "wczoraj".into();
    }
    if n - t < 6 * 86400 {
        return DAYS[lt.tm_wday as usize].into();
    }
    format!("{}.{:02}{}", lt.tm_mday, lt.tm_mon + 1, if lt.tm_year != ln.tm_year { format!(".{}", lt.tm_year + 1900) } else { String::new() })
}
fn gb(b: f64) -> String {
    let v = b / 1e9;
    (if b < 1e10 { format!("{v:.1}") } else { format!("{v:.0}") }).replace('.', ",")
}
fn dur(s: i64) -> String {
    let (d, h) = (s / 86400, s % 86400 / 3600);
    if d > 0 { format!("{d} d {h} h") } else { format!("{h} h {} min", s % 3600 / 60) }
}
fn st(size: f32, w: f32) -> Style {
    Style::ui(size, w)
}
// a line box of `size` centred on cy
fn text_cy(c: &mut Canvas, t: &mut Text, x: f32, cy: f32, s: &str, sty: &Style, col: Rgba, max: Option<f32>) -> f32 {
    let h = line_h(t, sty);
    text(c, t, x, cy - h / 2.0, s, sty, col, false, max)
}

impl PhonePanel {
    pub fn new(ph: Phone) -> PhonePanel {
        // `--pane-png` runs without mistd: the running shell's state file
        {
            let mut s = ph.st.lock().unwrap();
            if s.raw.is_empty() {
                let p = format!("{}/mist/phone-state.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
                if let Some(o) = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()) {
                    s.raw = o;
                }
            }
        }
        ph.send("threads", None);
        ph.send("contacts", None);
        let mut p = PhonePanel {
            ph,
            main: Value::Null,
            ok: false,
            threads: Vec::new(),
            convs: json!({}),
            contacts: Vec::new(),
            adb: json!({}),
            hfp: false,
            tab: "sms",
            thread: None,
            composing: false,
            push: Field::default(),
            reply: Field::default(),
            to: Field::default(),
            msg: Field::default(),
            search: Field::default(),
            focus: None,
            replying: None,
            said: String::new(),
            said_at: None,
            toast: Anim::new(0.0),
            scroll: 0.0,
            content_h: 0.0,
            conv_n: 0,
            hits: Vec::new(),
            ptr: (-1.0, -1.0),
            pressed_outside: false,
            adb_at: None,
            page: Canvas::new(PAGE_W as u32, 640),
            origin: (0.0, 0.0),
        };
        p.sync();
        if p.note_count() > 0 {
            p.tab = "notes";
        }
        p
    }
    fn sync(&mut self) {
        let s = self.ph.st.lock().unwrap();
        self.ok = s.get("ok").as_bool().unwrap_or(false);
        self.main = s.main();
        self.threads = s.get("threads").as_array().cloned().unwrap_or_default();
        self.convs = s.get("convs");
        self.contacts = s.get("contacts").as_array().cloned().unwrap_or_default();
        self.adb = s.get("adb");
        self.hfp = s.get("hfp").as_bool().unwrap_or(false);
    }
    fn present(&self) -> bool {
        self.main.is_object()
    }
    fn paired(&self) -> bool {
        self.main["pair"].as_i64() == Some(3)
    }
    fn online(&self) -> bool {
        self.paired() && self.main["reach"].as_bool().unwrap_or(false)
    }
    fn battery(&self) -> i64 {
        if self.online() { self.main["bat"].as_i64().unwrap_or(-1) } else { -1 }
    }
    fn charging(&self) -> bool {
        self.online() && self.main["chg"].as_bool().unwrap_or(false)
    }
    fn adb_ok(&self) -> bool {
        self.online() && self.adb["ok"].as_bool().unwrap_or(false)
    }
    fn has(&self, p: &str) -> bool {
        self.online() && self.main["plugins"].as_array().map(|a| a.iter().any(|x| x.as_str() == Some(p))).unwrap_or(false)
    }
    fn notes(&self) -> Vec<Value> {
        if self.online() { self.main["notes"].as_array().cloned().unwrap_or_default() } else { Vec::new() }
    }
    fn note_count(&self) -> i64 {
        if self.online() { self.main["nNotes"].as_i64().unwrap_or(0) } else { 0 }
    }
    fn contact_name(&self, tel: &str) -> String {
        let d = digits9(tel);
        if d.len() < 9 {
            return String::new();
        }
        self.contacts.iter().find(|c| c["tels"].as_array().map(|t| t.iter().any(|x| digits9(x.as_str().unwrap_or("")) == d)).unwrap_or(false)).map(|c| s(c, "name")).unwrap_or_default()
    }
    fn say(&mut self, t: &str) {
        self.said = t.to_string();
        self.said_at = Some(Instant::now());
        self.toast.set(1.0, 260.0, Ease::OutCubic);
    }
    fn send(&self, c: &str, a: Option<Value>) {
        self.ph.send(c, a);
    }
    fn close_thread(&mut self) {
        if let Some((th, _, _)) = self.thread.take() {
            self.send("unconv", Some(th));
        }
        self.composing = false;
        self.focus = None;
        self.scroll = 0.0;
    }
    fn open_thread(&mut self, th: Value, name: String, addr: Vec<String>) {
        self.close_thread();
        self.send("conv", Some(th.clone()));
        self.thread = Some((th, name, addr));
        self.tab = "sms";
        self.conv_n = 0;
        self.focus = Some(F::Msg);
    }
    fn compose(&mut self, number: &str) {
        self.close_thread();
        self.composing = true;
        self.tab = "sms";
        self.to.text = number.trim().to_string();
        self.focus = Some(if self.to.text.is_empty() { F::To } else { F::Msg });
    }
    fn field(&mut self, f: F) -> &mut Field {
        match f {
            F::Push => &mut self.push,
            F::Reply => &mut self.reply,
            F::To => &mut self.to,
            F::Msg => &mut self.msg,
            F::Search => &mut self.search,
        }
    }
    fn submit(&mut self, f: F) -> Act {
        let t = self.field(f).text.trim().to_string();
        if t.is_empty() {
            return Act::None;
        }
        match f {
            F::Push => {
                let low = t.to_lowercase();
                if !t.contains(char::is_whitespace) && (low.starts_with("http://") || low.starts_with("https://") || low.starts_with("www.")) {
                    let u = if t.contains("://") { t.clone() } else { format!("https://{t}") };
                    self.send("share", Some(json!(u)));
                    self.say("Link otwarty na telefonie");
                } else {
                    self.send("text", Some(json!(t)));
                    self.say("Tekst wysłany na telefon");
                }
                self.push.text.clear();
            }
            F::Reply => {
                if let Some(id) = self.replying.take() {
                    self.send("reply", Some(json!({ "id": id, "text": t })));
                    self.say("Odpowiedź wysłana");
                }
                self.reply.text.clear();
                self.focus = None;
            }
            F::To => self.focus = Some(F::Msg),
            F::Msg => {
                if let Some((th, _, _)) = &self.thread {
                    self.send("sendsms", Some(json!({ "thread": th, "text": t })));
                } else if !self.to.text.trim().is_empty() {
                    let to = self.to.text.trim().to_string();
                    self.send("sendsms", Some(json!({ "to": to, "text": t })));
                    self.composing = false;
                    self.focus = None;
                }
                self.msg.text.clear();
                self.say("SMS wysłany");
            }
            F::Search => {}
        }
        Act::Redraw
    }
    fn hov(&self, h: &Hit) -> bool {
        self.hit().as_ref() == Some(h)
    }
    fn hit(&self) -> Option<Hit> {
        self.hits.iter().rev().find(|h| inside(self.ptr, h.0)).map(|h| h.1.clone())
    }
    fn push_hit(&mut self, r: (f32, f32, f32, f32), h: Hit) {
        self.hits.push(((r.0 + self.origin.0, r.1 + self.origin.1, r.2, r.3), h));
    }

    // ---------------------------------------------------------------- parts
    fn input(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32, f: F, hint: &str, send_icon: bool) {
        let focused = self.focus == Some(f);
        rect(c, x, y, w, 42.0, 14.0, FILL);
        border(c, x, y, w, 42.0, 14.0, 1.0, if focused { Rgba(169.0 / 255.0, 194.0 / 255.0, 238.0 / 255.0, 0.45) } else { RIM });
        let tw = w - 28.0 - if send_icon { 34.0 } else { 0.0 };
        let sty = st(14.0, 400.0);
        let val = self.field(f).text.clone();
        if val.is_empty() {
            text_cy(c, t, x + 14.0, y + 21.0, hint, &st(13.5, 400.0), T4, Some(tw));
            if focused {
                rect(c, x + 14.0, y + 12.0, 1.5, 18.0, 0.0, ACCENT);
            }
        } else {
            // the end of a long text stays visible
            let mut shown: &str = &val;
            while t.width(shown, &sty) > tw - 4.0 && !shown.is_empty() {
                let mut it = shown.char_indices();
                it.next();
                shown = it.next().map(|(i, _)| &shown[i..]).unwrap_or("");
            }
            let ww = text_cy(c, t, x + 14.0, y + 21.0, shown, &sty, T1, None);
            if focused {
                rect(c, x + 14.0 + ww + 1.0, y + 12.0, 1.5, 18.0, 0.0, ACCENT);
            }
        }
        self.push_hit((x, y, w, 42.0), Hit::Field(f));
        if send_icon {
            let on = !val.trim().is_empty();
            let (bx, by) = (x + w - 5.0 - 32.0, y + 5.0);
            rect(c, bx, by, 32.0, 32.0, 16.0, if on { ACCENT } else { FILL });
            c.icon(t, bx + 16.0, by + 16.0, "send", 15.0, if on { WHITE } else { T3 });
            if on {
                self.push_hit((bx, by, 32.0, 32.0), Hit::Send(f));
            }
        }
    }
    fn text_btn(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, label: &str, primary: bool, small: bool, h: Hit) -> f32 {
        let sty = st(if small { 11.5 } else { 12.5 }, 600.0);
        let w = t.width(label, &sty) + if small { 20.0 } else { 26.0 };
        let (bh, r) = if small { (28.0, 9.0) } else { (32.0, 11.0) };
        let hov = self.hov(&h);
        rect(c, x, y, w, bh, r, if primary { if hov { ACC_STRONG } else { ACCENT } } else if hov { FILL2 } else { FILL });
        if !primary {
            border(c, x, y, w, bh, r, 1.0, HAIR);
        }
        let lw = t.width(label, &sty);
        text_cy(c, t, x + (w - lw) / 2.0, y + bh / 2.0, label, &sty, if primary { INK } else { T1 }, None);
        self.push_hit((x, y, w, bh), h);
        w
    }
    fn round_btn(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, size: f32, icon: &str, active: bool, clear: bool, h: Hit) {
        let hov = self.hov(&h);
        let fill = if active { if hov { ACC_STRONG } else { ACCENT } } else if hov { FILL2 } else if clear { Rgba(0.0, 0.0, 0.0, 0.0) } else { FILL };
        rect(c, x, y, size, size, size / 2.0, fill);
        c.icon(t, x + size / 2.0, y + size / 2.0, icon, size * 0.47, if active { WHITE } else { T1 });
        self.push_hit((x, y, size, size), h);
    }
    fn pill(c: &mut Canvas, t: &mut Text, x: f32, y: f32, icon: &str, label: &str, col: Rgba) -> f32 {
        let sty = st(11.5, 600.0);
        let w = 13.0 + 5.0 + t.width(label, &sty) + 16.0;
        rect(c, x, y, w, 24.0, 12.0, FILL);
        border(c, x, y, w, 24.0, 12.0, 1.0, HAIR);
        c.icon(t, x + 8.0 + 6.5, y + 12.0, icon, 13.0, col);
        text_cy(c, t, x + 8.0 + 18.0, y + 12.0, label, &sty, T2, None);
        w
    }
    fn empty(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, w: f32, icon: &str, title: &str, sub: &str) {
        let subs = if sub.is_empty() { Vec::new() } else { crate::gfx::wrap(t, sub, &st(12.0, 400.0), w * 0.8, 4) };
        let h = 30.0 + 8.0 + 18.0 + if subs.is_empty() { 0.0 } else { 8.0 + subs.len() as f32 * 16.0 };
        let mut y = cy - h / 2.0;
        c.icon(t, cx, y + 15.0, icon, 30.0, T4);
        y += 38.0;
        let ts = st(14.0, 600.0);
        let tw = t.width(title, &ts);
        text(c, t, cx - tw / 2.0, y, title, &ts, T3, false, None);
        y += 26.0;
        for l in subs {
            let ss = st(12.0, 400.0);
            let lw = t.width(&l, &ss);
            text(c, t, cx - lw / 2.0, y, &l, &ss, T4, false, None);
            y += 16.0;
        }
    }

    // ---------------------------------------------------------------- left column (origin = column top-left)
    fn paint_left(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        let (online, bat, chg) = (self.online(), self.battery(), self.charging());
        let bc = if chg { GREEN } else if (0..=15).contains(&bat) { DANGER } else if (0..=30).contains(&bat) { YELLOW } else { M_ACC };
        // the phone, filled up to its charge
        rect(c, 0.0, 0.0, 70.0, 124.0, 17.0, INSET);
        if bat >= 0 {
            let fh = (116.0 * bat as f32 / 100.0).max(16.0);
            let pulse = if chg { 0.775 + 0.225 * (cx_ms() as f32 / 2200.0 * std::f32::consts::TAU).cos() } else { 1.0 };
            let path = crate::gfx::rrect_path(4.0, 120.0 - fh, 62.0, fh, 13.0);
            if let Some(p) = path {
                crate::desktop::fill_lin(c, &p, (0.0, 120.0 - fh), (0.0, 120.0), &[(0.0, bc.alpha(0.34 * pulse)), (1.0, bc.alpha(0.12 * pulse))]);
            }
        }
        border(c, 0.0, 0.0, 70.0, 124.0, 17.0, 2.0, if online { RIM } else { HAIR });
        rect(c, 25.0, 8.0, 20.0, 4.0, 2.0, RIM);
        let icon = if online { if chg { "bolt" } else { "mobile" } } else { "mobileOff" };
        if bat >= 0 {
            c.icon(t, 35.0, 52.0, icon, 18.0, bc);
            let ps = st(17.0, 680.0);
            let s = format!("{bat}%");
            let w = t.width(&s, &ps);
            text_cy(c, t, 35.0 - w / 2.0, 74.0, &s, &ps, T1, None);
        } else {
            c.icon(t, 35.0, 62.0, icon, 18.0, if online { bc } else { T3 });
        }
        // name, model, pills
        let tx = 92.0;
        let tw = LEFT_W - tx;
        let name = if self.present() { s(&self.main, "name") } else { "Telefon".into() };
        let adb = self.adb.clone();
        let sub = if self.adb_ok() {
            [s(&adb, "model"), format!("Android {}", s(&adb, "android")), s(&adb, "ui")].into_iter().filter(|x| !x.is_empty() && x != "Android ").collect::<Vec<_>>().join(" · ")
        } else if online {
            format!("KDE Connect · {}", s(&self.main, "ip"))
        } else if self.present() {
            if self.paired() { "poza zasięgiem".into() } else { "niesparowany".into() }
        } else {
            "brak urządzenia".into()
        };
        let mut pills: Vec<(&str, String, Rgba)> = Vec::new();
        if chg {
            pills.push(("charging", "ładuje".into(), GREEN));
        }
        if online && !chg && (0..=15).contains(&bat) {
            pills.push(("battLow", "słaba bateria".into(), DANGER));
        }
        let sig = if online { self.main["sig"].as_i64().unwrap_or(-1) } else { -1 };
        if sig >= 0 {
            let icons = ["barsOff", "bars1", "bars2", "bars3", "bars4", "bars5"];
            let op = if self.adb_ok() && !s(&adb, "operator").is_empty() { format!("{} · ", s(&adb, "operator")) } else { String::new() };
            pills.push((icons[(sig + 1).clamp(0, 5) as usize], format!("{op}{}", s(&self.main, "net")), T1));
        }
        if self.adb_ok() {
            if !s(&adb, "ssid").is_empty() {
                let link = adb["link"].as_i64().map(|l| format!(" · {l} Mb/s")).unwrap_or_default();
                let weak = adb["rssi"].as_i64().map(|r| r < -75).unwrap_or(false);
                pills.push(("wifi", format!("{}{link}", s(&adb, "ssid")), if weak { ORANGE } else { T1 }));
            }
            if let Some(tp) = adb["temp"].as_f64() {
                pills.push(("temp", format!("{}°C", tp.to_string().replace('.', ",")), if tp > 40.0 { DANGER } else { T1 }));
            }
            if adb["dnd"].as_bool().unwrap_or(false) {
                pills.push(("bellZ", "nie przeszkadzać".into(), T1));
            }
            let wifi = adb["wifi"].as_bool().unwrap_or(false);
            pills.push((if wifi { "link" } else { "usb" }, if wifi { "ADB".into() } else { "ADB · USB".into() }, GREEN));
        }
        if self.hfp {
            pills.push(("btLink", "rozmowy na PC".into(), GREEN));
        }
        // measure the pills' rows to centre the column on the glyph
        let mut rows = 1;
        let mut px = 0.0;
        for (_, l, _) in &pills {
            let w = 34.0 + t.width(l, &st(11.5, 600.0));
            if px > 0.0 && px + w > tw {
                rows += 1;
                px = 0.0;
            }
            px += w + 6.0;
        }
        let ch = line_h(t, &st(22.0, 650.0)) + 4.0 + line_h(t, &st(12.0, 400.0)) + if pills.is_empty() { 0.0 } else { 6.0 + rows as f32 * 30.0 - 6.0 };
        let mut y = (62.0 - ch / 2.0).max(0.0);
        y += text(c, t, tx, y, &name, &st(22.0, 650.0), T1, false, Some(tw)).min(0.0) + line_h(t, &st(22.0, 650.0)) + 4.0;
        text(c, t, tx, y, &sub, &st(12.0, 400.0), T3, false, Some(tw));
        y += line_h(t, &st(12.0, 400.0)) + 6.0;
        let mut px = tx;
        for (icon, l, col) in &pills {
            let w = 34.0 + t.width(l, &st(11.5, 600.0));
            if px > tx && px + w > tx + tw {
                px = tx;
                y += 30.0;
            }
            px += Self::pill(c, t, px, y, icon, l, *col) + 6.0;
        }
        let mut y = 124.0 + 22.0;

        // pairing / status
        if !online {
            let key = s(&self.main, "key");
            let pair = self.main["pair"].as_i64().unwrap_or(0);
            let msg = if !self.ok {
                "KDE Connect nie działa (kdeconnectd).".to_string()
            } else if !self.present() {
                "Otwórz KDE Connect na telefonie w tej samej sieci.".into()
            } else if pair == 2 {
                format!("Telefon chce się sparować. Sprawdź, czy klucz {key} jest taki sam na telefonie.")
            } else if pair == 1 {
                format!("Zaakceptuj parowanie na telefonie (klucz {key}).")
            } else if !self.paired() {
                "Telefon w sieci, ale niesparowany.".into()
            } else {
                "Telefon poza zasięgiem. Wróci sam, gdy będzie w tej samej sieci.".into()
            };
            let lines = crate::gfx::wrap(t, &msg, &st(13.0, 400.0), LEFT_W - 32.0, 6);
            let btns = pair == 2 || pair == 1 || (self.present() && !self.paired());
            let bh = lines.len() as f32 * 17.0 + if btns { 10.0 + 32.0 } else { 0.0 } + 28.0;
            rect(c, 0.0, y, LEFT_W, bh, 18.0, FILL);
            border(c, 0.0, y, LEFT_W, bh, 18.0, 1.0, HAIR);
            let mut ly = y + 14.0;
            for l in &lines {
                text(c, t, 16.0, ly, l, &st(13.0, 400.0), T2, false, None);
                ly += 17.0;
            }
            if btns {
                let mut bx = 16.0;
                ly += 10.0;
                if pair == 2 {
                    bx += self.text_btn(c, t, bx, ly, "Akceptuj", true, false, Hit::Accept) + 8.0;
                }
                if pair == 2 || pair == 1 {
                    self.text_btn(c, t, bx, ly, if pair == 1 { "Anuluj" } else { "Odrzuć" }, false, false, Hit::Reject);
                } else if self.present() && !self.paired() {
                    self.text_btn(c, t, bx, ly, "Sparuj", true, false, Hit::Pair);
                }
            }
            y += bh + 22.0;
        } else if self.adb_ok() {
            // ADB details
            let bh = 28.0 + 50.0 + 12.0 + 1.0 + 12.0 + 16.0;
            rect(c, 0.0, y, LEFT_W, bh, 18.0, FILL);
            border(c, 0.0, y, LEFT_W, bh, 18.0, 1.0, HAIR);
            let sw = (LEFT_W - 32.0 - 28.0) / 3.0;
            let num = |v: &Value, i: usize| v.as_array().and_then(|a| a.get(i)).and_then(|x| x.as_f64()).unwrap_or(0.0);
            let (s0, s1) = (num(&adb["storage"], 0), num(&adb["storage"], 1));
            let (r0, r1) = (num(&adb["ram"], 0), num(&adb["ram"], 1));
            let health = if s(&adb, "health").is_empty() { "—".into() } else { s(&adb, "health") };
            let volt = adb["volt"].as_f64().map(|v| v.to_string().replace('.', ",")).unwrap_or_default();
            let stats = [
                ("drive", "Pamięć", format!("{} / {} GB", gb(s1), gb(s0)), (s1 / s0.max(1.0)) as f32, YELLOW),
                ("ram", "RAM", format!("{} / {} GB", gb(r0 - r1), gb(r0)), (1.0 - r1 / r0.max(1.0)) as f32, ACCENT),
                ("heart", "Bateria", format!("{health} · {volt} V"), bat as f32 / 100.0, bc),
            ];
            for (i, (icon, label, value, frac, col)) in stats.into_iter().enumerate() {
                let sx = 16.0 + i as f32 * (sw + 14.0);
                let sy = y + 14.0;
                c.icon(t, sx + 6.0, sy + 7.0, icon, 12.0, col);
                text_cy(c, t, sx + 17.0, sy + 7.0, label, &st(11.0, 550.0), T3, None);
                text_cy(c, t, sx, sy + 26.0, &value, &st(13.0, 640.0), T1, Some(sw));
                rect(c, sx, sy + 40.0, sw, 3.0, 1.5, FILL2);
                rect(c, sx, sy + 40.0, (sw * frac.clamp(0.0, 1.0)).max(3.0), 3.0, 1.5, col);
            }
            let ly = y + 14.0 + 50.0 + 12.0;
            rect(c, 16.0, ly, LEFT_W - 32.0, 1.0, 0.0, HAIR);
            let screen = adb["screen"].as_bool().unwrap_or(false);
            let ip = { let i = s(&self.main, "ip"); if i.is_empty() { s(&adb, "serial").split(':').next().unwrap_or("").to_string() } else { i } };
            let bits = [("clock", dur(adb["uptime"].as_i64().unwrap_or(0)), false), (if screen { "screenOn" } else { "screenOff" }, if screen { "ekran wł.".to_string() } else { "ekran wył.".into() }, screen), ("network", ip, false)];
            let mut bx = 16.0;
            for (icon, l, lit) in bits {
                if l.is_empty() {
                    continue;
                }
                c.icon(t, bx + 6.0, ly + 21.0, icon, 12.0, if lit { GREEN } else { T3 });
                bx += 17.0 + text_cy(c, t, bx + 17.0, ly + 21.0, &l, &st(11.0, 400.0), T2, None) + 14.0;
            }
            y += bh + 22.0;
        } else {
            let msg = if !self.adb["have"].as_bool().unwrap_or(false) {
                "ADB daje więcej (ekran telefonu na PC, zrzuty, dzwonienie, pamięć, temperatura): sudo pacman -S android-tools scrcpy"
            } else {
                "ADB: podłącz telefon kablem z włączonym debugowaniem USB i zezwól na komputer. Potem „ADB przez Wi-Fi” i kabel można odłączyć."
            };
            let lines = crate::gfx::wrap(t, msg, &st(11.5, 400.0), LEFT_W - 24.0 - 26.0, 5);
            let bh = lines.len() as f32 * 15.0 + 22.0;
            rect(c, 0.0, y, LEFT_W, bh, 18.0, FILL);
            border(c, 0.0, y, LEFT_W, bh, 18.0, 1.0, HAIR);
            c.icon(t, 20.0, y + 19.0, "tool", 16.0, T3);
            let mut ly = y + 11.0;
            for l in &lines {
                text(c, t, 38.0, ly, l, &st(11.5, 400.0), T3, false, None);
                ly += 15.0;
            }
            y += bh + 22.0;
        }

        // quick actions
        if online {
            text(c, t, 4.0, y, "SZYBKIE AKCJE", &st(11.0, 600.0).spaced(1.2), T3, false, None);
            y += 16.0 + 10.0;
            let (aok, scr, dnd, have, wifi) = (self.adb_ok(), self.adb["screen"].as_bool().unwrap_or(false), self.adb["dnd"].as_bool().unwrap_or(false), self.adb["have"].as_bool().unwrap_or(false), self.adb["wifi"].as_bool().unwrap_or(false));
            let tiles: [(&str, &str, bool, bool); 12] = [
                ("bellRing", "Znajdź", self.has("findmyphone"), false),
                ("clipboard", "Schowek", self.has("clipboard"), false),
                ("fileUp", "Wyślij pliki", self.has("share"), false),
                ("folderOpen", "Pliki", self.has("sftp"), false),
                ("screenOn", "Ekran na PC", aok, false),
                ("screenshot", "Zrzut", aok, false),
                ("camera", "Zdjęcie", self.has("photo"), false),
                (if aok && scr { "lock" } else { "bulb" }, if aok && scr { "Zablokuj" } else { "Wybudź" }, aok, false),
                ("bellZ", "Nie przeszk.", aok, aok && dnd),
                ("wifi", "ADB Wi-Fi", have, aok && wifi),
                ("shield", "Uprawnienia", aok, false),
                ("settings", "KDE Connect", true, false),
            ];
            let tw = (LEFT_W - 24.0) / 4.0;
            for (i, (icon, label, on, lit)) in tiles.into_iter().enumerate() {
                let (x, ty) = ((i % 4) as f32 * (tw + 8.0), y + (i / 4) as f32 * 70.0);
                let h = Hit::Act(i as u8);
                let hov = on && self.hov(&h);
                let a = if on { 1.0 } else { 0.35 };
                rect(c, x, ty, tw, 62.0, 14.0, (if lit { ACC_SOFT } else if hov { FILL2 } else { FILL }).alpha_mul(a));
                border(c, x, ty, tw, 62.0, 14.0, 1.0, (if lit { Rgba(169.0 / 255.0, 194.0 / 255.0, 238.0 / 255.0, 0.4) } else { HAIR }).alpha_mul(a));
                c.icon(t, x + tw / 2.0, ty + 22.0, icon, 19.0, (if lit { ACCENT } else { T1 }).alpha_mul(a));
                let ls = st(11.0, 550.0);
                let lw = t.width(label, &ls);
                text_cy(c, t, x + (tw - lw) / 2.0, ty + 44.0, label, &ls, T2.alpha_mul(a), None);
                if on {
                    self.push_hit((x, ty, tw, 62.0), h);
                }
            }
            y += 3.0 * 70.0 - 8.0 + 14.0;
        }

        // media on the phone (bottom)
        let player = cx.players.iter().find(|p| p.bus.contains("kdeconnect") && !p.title.is_empty()).cloned();
        let media_y = 676.0 - 88.0;
        if online && self.has("share") && (player.is_none() || media_y - y > 56.0) {
            self.input(c, t, 0.0, y, LEFT_W, F::Push, "Wyślij link lub tekst na telefon", true);
        }
        if let Some(p) = player {
            let my = media_y;
            rect(c, 0.0, my, LEFT_W, 88.0, 18.0, FILL);
            border(c, 0.0, my, LEFT_W, 88.0, 18.0, 1.0, HAIR);
            rect(c, 14.0, my + 14.0, 60.0, 60.0, 12.0, FILL2);
            match (!p.art.is_empty()).then(|| cx.imgs.get(&p.art, 112, cx.icons)).flatten() {
                Some(pm) => crate::img::draw(c, &pm, 14.0, my + 14.0, 60.0, 60.0, 12.0, true, 1.0),
                None => c.icon(t, 44.0, my + 44.0, "music", 20.0, T3),
            }
            let (bx, tw) = (86.0, LEFT_W - 86.0 - 12.0 - 106.0);
            let id = p.identity.split(" - ").next().unwrap_or("").to_string();
            text(c, t, bx, my + 16.0, &format!("NA TELEFONIE · {}", id.to_uppercase()), &st(10.0, 600.0).spaced(1.2), T3, false, Some(tw));
            text(c, t, bx, my + 31.0, &p.title, &st(13.5, 620.0), T1, false, Some(tw));
            text(c, t, bx, my + 49.0, &p.artist, &st(11.5, 400.0), T2, false, Some(tw));
            if p.length > 0.0 {
                let pos = p.position + if p.playing { p.pos_at.map(|a| a.elapsed().as_secs_f64()).unwrap_or(0.0) } else { 0.0 };
                rect(c, bx, my + 68.0, tw, 3.0, 1.5, FILL2);
                rect(c, bx, my + 68.0, (tw * (pos / p.length).clamp(0.0, 1.0) as f32).max(3.0), 3.0, 1.5, T1);
            }
            let rx = LEFT_W - 12.0 - 102.0;
            self.round_btn(c, t, rx, my + 29.0, 30.0, "prev", false, true, Hit::MPrev);
            self.round_btn(c, t, rx + 34.0, my + 27.0, 34.0, if p.playing { "pause" } else { "play" }, true, false, Hit::MPlay);
            self.round_btn(c, t, rx + 72.0, my + 29.0, 30.0, "next", false, true, Hit::MNext);
        }
    }

    // ---------------------------------------------------------------- right page (into self.page, origin = page top-left)
    fn paint_page(&mut self, cx: &mut Ctx, ph: f32) {
        let mut c = std::mem::replace(&mut self.page, Canvas::new(1, 1));
        if c.pm.height() != ph as u32 {
            c = Canvas::new(PAGE_W as u32, ph as u32);
        }
        c.clear();
        let t = &mut *cx.t;
        let w = PAGE_W;
        let sc = self.scroll;
        let visible = |y: f32, h: f32| y + h > 0.0 && y < ph;
        let total;
        match (self.tab, self.thread.is_some() || self.composing) {
            ("notes", _) => {
                let notes = self.notes();
                let mut y = -sc;
                for n in &notes {
                    let id = s(n, "id");
                    let body = if s(n, "text") == s(n, "title") { String::new() } else { s(n, "text") };
                    let lines = if body.is_empty() { Vec::new() } else { crate::gfx::wrap(t, &body, &st(12.5, 400.0), w - 28.0 - 48.0, 4) };
                    let acts: Vec<String> = n["actions"].as_array().map(|a| a.iter().take(3).filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                    let has_btns = (n["reply"].as_bool().unwrap_or(false) && self.replying.as_deref() != Some(&id)) || !acts.is_empty();
                    let replying = self.replying.as_deref() == Some(&id);
                    let ch = 36.0 + if lines.is_empty() { 0.0 } else { 6.0 + lines.len() as f32 * 16.0 } + if has_btns { 6.0 + 4.0 + 28.0 } else { 0.0 } + if replying { 6.0 + 42.0 } else { 0.0 };
                    let h = ch + 26.0;
                    if visible(y, h) {
                        let rowhov = inside(self.ptr, (self.origin.0, self.origin.1 + y, w, h));
                        rect(&mut c, 0.0, y, w, h, 18.0, if rowhov { FILL2 } else { FILL });
                        border(&mut c, 0.0, y, w, h, 18.0, 1.0, HAIR);
                        let app = s(n, "app");
                        let hc = hue(&app);
                        let (ix, iy) = (14.0, y + 13.0);
                        rect(&mut c, ix, iy, 36.0, 36.0, 10.0, hc.alpha_mul(0.18));
                        let icon = s(n, "icon");
                        match (!icon.is_empty()).then(|| cx.imgs.get(&icon, 72, cx.icons)).flatten() {
                            Some(pm) => crate::img::draw(&mut c, &pm, ix, iy, 36.0, 36.0, 10.0, true, 1.0),
                            None => {
                                let l = app.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or("?".into());
                                let ls = st(14.0, 700.0);
                                let lw = t.width(&l, &ls);
                                text_cy(&mut c, t, ix + 18.0 - lw / 2.0, iy + 18.0, &l, &ls, hc, None);
                            }
                        }
                        let (tx, tw) = (ix + 48.0, w - 28.0 - 48.0 - 30.0);
                        let at = format!("· {}", when(n["at"].as_i64().unwrap_or(0) * 1000));
                        let ats = st(11.0, 400.0);
                        let atw = t.width(&at, &ats);
                        let aw = text(&mut c, t, tx, iy + 1.0, &app, &st(11.0, 600.0), T2, false, Some(tw - atw - 6.0));
                        text(&mut c, t, tx + aw + 6.0, iy + 1.0, &at, &ats, T3, false, None);
                        let title = if s(n, "title").is_empty() { app.clone() } else { s(n, "title") };
                        text(&mut c, t, tx, iy + 16.0, &title, &st(13.5, 640.0), T1, false, Some(tw));
                        if n["dismiss"].as_bool().unwrap_or(false) {
                            let hx = Hit::NoteX(id.clone());
                            let xh = self.hov(&hx);
                            let (bx, by) = (w - 14.0 - 26.0, iy + 5.0);
                            if xh {
                                circle(&mut c, bx + 13.0, by + 13.0, 13.0, FILL2);
                            }
                            c.icon(t, bx + 13.0, by + 13.0, "x", 14.0, (if xh { T1 } else { T2 }).alpha_mul(if rowhov { 1.0 } else { 0.45 }));
                            self.push_hit((bx, by, 26.0, 26.0), hx);
                        }
                        let mut ly = iy + 36.0 + 6.0;
                        for l in &lines {
                            text(&mut c, t, tx, ly, l, &st(12.5, 400.0), T2, false, None);
                            ly += 16.0;
                        }
                        if has_btns {
                            ly += 4.0;
                            let mut bx = tx;
                            if n["reply"].as_bool().unwrap_or(false) && !replying {
                                bx += self.text_btn(&mut c, t, bx, ly, "Odpowiedz", false, true, Hit::NoteReply(id.clone())) + 6.0;
                            }
                            for a in &acts {
                                bx += self.text_btn(&mut c, t, bx, ly, a, false, true, Hit::NoteAct(id.clone(), a.clone())) + 6.0;
                            }
                            ly += 28.0 + 6.0;
                        }
                        if replying {
                            let hint = format!("Odpowiedź do {title}");
                            self.input(&mut c, t, 14.0, ly, w - 28.0, F::Reply, &hint, false);
                        }
                    }
                    y += h + 8.0;
                }
                total = y + sc + 56.0;
                if notes.is_empty() {
                    let sub = if self.online() && self.has("notifications") {
                        format!("Jeśli powinny tu być: daj KDE Connect dostęp do powiadomień na telefonie{}", if self.adb_ok() { " albo kliknij „Uprawnienia”." } else { "." })
                    } else {
                        String::new()
                    };
                    Self::empty(&mut c, t, w / 2.0, ph / 2.0, w, "bell", if self.online() { "Brak powiadomień z telefonu" } else { "Telefon niepołączony" }, &sub);
                }
            }
            ("sms", false) => {
                let mut top = 0.0;
                if self.has("sms") {
                    self.text_btn(&mut c, t, 0.0, 0.0, "Nowa wiadomość", true, true, Hit::NewSms);
                    top = 40.0;
                }
                let threads = self.threads.clone();
                let mut y = top - sc;
                for (i, th) in threads.iter().enumerate() {
                    if visible(y, 58.0) && y + 58.0 > top {
                        let addr0 = th["addr"].as_array().and_then(|a| a.first()).and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let nm = { let n = self.contact_name(&addr0); if n.is_empty() { s(th, "name") } else { n } };
                        let read = th["read"].as_bool().unwrap_or(true);
                        let h = Hit::Thread(i);
                        if self.hov(&h) {
                            rect(&mut c, 0.0, y, w, 58.0, 14.0, FILL2);
                        }
                        let hc = hue(&nm);
                        circle(&mut c, 28.0, y + 29.0, 20.0, hc.alpha_mul(0.2));
                        let ini = initials(&nm);
                        let is = st(13.0, 700.0);
                        let iw = t.width(&ini, &is);
                        text_cy(&mut c, t, 28.0 - iw / 2.0, y + 29.0, &ini, &is, hc, None);
                        let tw = w - 60.0 - 64.0;
                        text(&mut c, t, 60.0, y + 11.0, &nm, &st(13.0, if read { 600.0 } else { 750.0 }), T1, false, Some(tw));
                        let body = format!("{}{}{}", if th["out"].as_bool().unwrap_or(false) { "Ty: " } else { "" }, s(th, "body").replace('\n', " "), if th["att"].as_i64().unwrap_or(0) > 0 { " 📎" } else { "" });
                        text(&mut c, t, 60.0, y + 29.0, &body, &st(12.0, 400.0), if read { T3 } else { T1 }, false, Some(tw));
                        let d = when(th["date"].as_i64().unwrap_or(0));
                        let ds = st(11.0, 400.0);
                        let dw = t.width(&d, &ds);
                        text(&mut c, t, w - 12.0 - dw, y + 13.0, &d, &ds, T3, false, None);
                        if !read {
                            circle(&mut c, w - 16.0, y + 38.0, 4.0, ACCENT);
                        }
                        self.push_hit((0.0, y.max(top), w, 58.0 - (top - y).max(0.0)), h);
                    }
                    y += 60.0;
                }
                total = y + sc;
                if threads.is_empty() {
                    Self::empty(&mut c, t, w / 2.0, ph / 2.0, w, "message", if self.has("sms") { "Wczytuję wiadomości…" } else { "SMS niedostępne" }, if self.has("sms") { "" } else { "Włącz wtyczkę SMS w KDE Connect na telefonie" });
                }
            }
            ("sms", true) => {
                let mut top = 32.0 + 12.0;
                if self.thread.is_none() {
                    top += 42.0;
                    let q = self.to.text.trim().to_lowercase();
                    let is_num = q.chars().filter(|c| c.is_ascii_digit()).count() >= 6 && q.chars().all(|c| c.is_ascii_digit() || " +-".contains(c));
                    if q.chars().count() > 1 && !is_num {
                        top += 6.0 + 36.0 * self.contacts.iter().filter(|c| s(c, "name").to_lowercase().contains(&q)).take(5).count() as f32;
                    }
                }
                top += 12.0;
                let bottom = ph - 42.0 - 12.0;
                // messages, bottom-anchored: scroll = distance from the end
                let msgs: Vec<Value> = match &self.thread { Some((th, _, _)) => self.convs[th.to_string().trim_matches('"')].as_array().cloned().unwrap_or_default(), None => Vec::new() };
                if msgs.len() != self.conv_n {
                    self.conv_n = msgs.len();
                    self.scroll = 0.0;
                }
                let bw_max = w * 0.78;
                let ms = st(13.0, 400.0);
                let mut layout: Vec<(f32, Vec<String>, bool, Option<String>)> = Vec::new();
                let mut hsum = 0.0;
                for (i, m) in msgs.iter().enumerate() {
                    let date = m["date"].as_i64().unwrap_or(0);
                    let stamp = i == 0 || date - msgs[i - 1]["date"].as_i64().unwrap_or(0) > 3_600_000;
                    let body = format!("{}{}", s(m, "body"), if m["att"].as_i64().unwrap_or(0) > 0 { if s(m, "body").is_empty() { "📎 załącznik" } else { "\n📎 załącznik" } } else { "" });
                    let lines = crate::gfx::wrap(t, &body, &ms, bw_max - 26.0, 60);
                    let h = lines.len() as f32 * 17.0 + 16.0 + if stamp { 18.0 } else { 0.0 };
                    let st_txt = stamp.then(|| {
                        let today = crate::data::local(date / 1000).tm_yday == crate::data::local(crate::data::now()).tm_yday;
                        format!("{}{}", when(date), if today { String::new() } else { format!(" {}", crate::data::hm(date / 1000)) })
                    });
                    layout.push((h, lines, m["out"].as_bool().unwrap_or(false), st_txt));
                    hsum += h + 6.0;
                }
                let view = bottom - top;
                let max_sc = (hsum - view).max(0.0);
                self.scroll = self.scroll.clamp(0.0, max_sc);
                total = 0.0;
                let mut y = top + (view - hsum).max(0.0) - (max_sc - self.scroll);
                for (h, lines, out, stamp) in layout {
                    if y + h > top && y < bottom {
                        let mut by = y;
                        if let Some(sx) = &stamp {
                            let ss = st(10.5, 400.0);
                            let sw = t.width(sx, &ss);
                            text(&mut c, t, (w - sw) / 2.0, y, sx, &ss, T4, false, None);
                            by += 18.0;
                        }
                        let tw = lines.iter().map(|l| t.width(l, &ms)).fold(0.0f32, f32::max);
                        let bw = (tw + 26.0).min(bw_max);
                        let bx = if out { w - bw } else { 0.0 };
                        rect(&mut c, bx, by, bw, lines.len() as f32 * 17.0 + 16.0, 16.0, if out { ACCENT } else { FILL2 });
                        let mut ly = by + 8.0;
                        for l in &lines {
                            text(&mut c, t, bx + 13.0, ly, l, &ms, if out { INK } else { T1 }, false, None);
                            ly += 17.0;
                        }
                    }
                    y += h + 6.0;
                }
                if self.thread.is_some() && msgs.is_empty() {
                    Self::empty(&mut c, t, w / 2.0, (top + bottom) / 2.0, w, "message", "Wczytuję rozmowę…", "");
                }
                c.pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w, top - 12.0).unwrap(), &tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() }, tiny_skia::Transform::identity(), None);
                {
                // head
                self.round_btn(&mut c, t, 0.0, 0.0, 32.0, "left", false, false, Hit::Back);
                let (title, sub) = match &self.thread { Some((_, n, a)) => (n.clone(), a.join(", ")), None => ("Nowa wiadomość".into(), String::new()) };
                let can_call = (self.adb_ok() || self.hfp) && self.thread.as_ref().map(|(_, _, a)| a.len() == 1 && a[0].chars().filter(|c| c.is_ascii_digit()).count() >= 6).unwrap_or(false);
                let hw = w - 42.0 - if can_call { 42.0 } else { 0.0 };
                if sub.is_empty() {
                    text_cy(&mut c, t, 42.0, 16.0, &title, &st(15.0, 650.0), T1, Some(hw));
                } else {
                    text_cy(&mut c, t, 42.0, 8.0, &title, &st(15.0, 650.0), T1, Some(hw));
                    text_cy(&mut c, t, 42.0, 26.0, &sub, &st(11.0, 400.0), T3, Some(hw));
                }
                if can_call {
                    self.round_btn(&mut c, t, w - 32.0, 0.0, 32.0, "call", false, false, Hit::CallThread);
                }
                let mut top = 32.0 + 12.0;
                if self.thread.is_none() {
                    self.input(&mut c, t, 0.0, top, w, F::To, "Do: numer lub imię z kontaktów", false);
                    top += 42.0;
                    let q = self.to.text.trim().to_lowercase();
                    let is_num = q.chars().filter(|c| c.is_ascii_digit()).count() >= 6 && q.chars().all(|c| c.is_ascii_digit() || " +-".contains(c));
                    if q.chars().count() > 1 && !is_num {
                        top += 6.0;
                        let hits: Vec<Value> = self.contacts.iter().filter(|c| s(c, "name").to_lowercase().contains(&q)).take(5).cloned().collect();
                        for ct in hits {
                            let tel = ct["tels"][0].as_str().unwrap_or("").to_string();
                            let h = Hit::ToHint(tel.clone());
                            if self.hov(&h) {
                                rect(&mut c, 0.0, top, w, 36.0, 10.0, FILL2);
                            }
                            text_cy(&mut c, t, 12.0, top + 18.0, &format!("{}  ·  {tel}", s(&ct, "name")), &st(12.5, 400.0), T1, Some(w - 24.0));
                            self.push_hit((0.0, top, w, 36.0), h);
                            top += 36.0;
                        }
                    }
                }
                }
                // the list clipped under the head and over the input: repaint those bands
                c.pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, bottom, w, ph - bottom).unwrap(), &tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() }, tiny_skia::Transform::identity(), None);
                self.input(&mut c, t, 0.0, ph - 42.0, w, F::Msg, "Wiadomość SMS", true);
                self.content_h = max_sc;
            }
            _ => {
                // contacts
                self.input(&mut c, t, 0.0, 0.0, w, F::Search, "Szukaj kontaktu", false);
                let q = self.search.text.trim().to_lowercase();
                let list: Vec<Value> = self.contacts.iter().filter(|c| q.is_empty() || s(c, "name").to_lowercase().contains(&q) || c["tels"].as_array().map(|t| t.iter().any(|x| x.as_str().unwrap_or("").replace(' ', "").contains(&q))).unwrap_or(false)).cloned().collect();
                let top = 52.0;
                let mut y = top - sc;
                let (sms, call) = (self.has("sms"), self.adb_ok() || self.hfp);
                for ct in &list {
                    if visible(y, 52.0) && y + 52.0 > top {
                        let nm = s(ct, "name");
                        let tels: Vec<String> = ct["tels"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                        if inside(self.ptr, (self.origin.0, self.origin.1 + y.max(top), w, 52.0)) {
                            rect(&mut c, 0.0, y, w, 52.0, 14.0, FILL2);
                        }
                        let hc = hue(&nm);
                        circle(&mut c, 26.0, y + 26.0, 18.0, hc.alpha_mul(0.2));
                        let ini = initials(&nm);
                        let is = st(12.0, 700.0);
                        let iw = t.width(&ini, &is);
                        text_cy(&mut c, t, 26.0 - iw / 2.0, y + 26.0, &ini, &is, hc, None);
                        let tw = w - 56.0 - 84.0;
                        text(&mut c, t, 56.0, y + 9.0, &nm, &st(13.0, 600.0), T1, false, Some(tw));
                        text(&mut c, t, 56.0, y + 27.0, &tels.join(" · "), &st(11.5, 400.0), T3, false, Some(tw));
                        let tel0 = tels.first().cloned().unwrap_or_default();
                        let mut bx = w - 8.0;
                        if call {
                            bx -= 32.0;
                            self.round_btn(&mut c, t, bx, y + 10.0, 32.0, "call", false, true, Hit::PCall(tel0.clone(), nm.clone()));
                            bx -= 4.0;
                        }
                        if sms {
                            bx -= 32.0;
                            self.round_btn(&mut c, t, bx, y + 10.0, 32.0, "message", false, true, Hit::PSms(tel0.clone()));
                        }
                    }
                    y += 54.0;
                }
                total = y + sc;
                if list.is_empty() {
                    let sub = if self.contacts.is_empty() { format!("KDE Connect na telefonie potrzebuje dostępu do kontaktów{}", if self.adb_ok() { " — kliknij „Uprawnienia”." } else { "." }) } else { String::new() };
                    Self::empty(&mut c, t, w / 2.0, (top + ph) / 2.0, w, "phone", if self.contacts.is_empty() { "Brak kontaktów" } else { "Nic nie znaleziono" }, &sub);
                }
                // rows scrolled under the search field
                c.pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 42.0, w, 10.0).unwrap(), &tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() }, tiny_skia::Transform::identity(), None);
            }
        }
        if !(self.tab == "sms" && (self.thread.is_some() || self.composing)) {
            self.content_h = (total - ph).max(0.0);
            self.scroll = self.scroll.clamp(0.0, self.content_h);
        }
        self.page = c;
    }
}

fn cx_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}
trait AlphaMul {
    fn alpha_mul(self, k: f32) -> Rgba;
}
impl AlphaMul for Rgba {
    fn alpha_mul(self, k: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, self.3 * k)
    }
}

impl Drop for PhonePanel {
    fn drop(&mut self) {
        if let Some((th, _, _)) = self.thread.take() {
            self.ph.send("unconv", Some(th));
        }
    }
}

impl Pane for PhonePanel {
    fn name(&self) -> &'static str {
        "phone"
    }
    fn animating(&self) -> bool {
        self.toast.running()
    }
    fn hand(&self) -> bool {
        matches!(self.hit(), Some(h) if !matches!(h, Hit::Sheet | Hit::Field(_)))
    }
    fn set_page(&mut self, p: &str) -> bool {
        let k = match p { "notes" => "notes", "sms" => "sms", "people" => "people", _ => return false };
        if self.tab == k {
            return false;
        }
        self.close_thread();
        self.tab = k;
        true
    }
    fn data(&mut self, what: &str) -> bool {
        if what == "phone" {
            self.sync();
            return true;
        }
        what == "players"
    }
    fn phone_ev(&mut self, e: &Value) -> bool {
        match e["k"].as_str().unwrap_or("") {
            "done" => self.say(&s(e, "text")),
            "err" => self.say(&format!("⚠ {}", s(e, "text"))),
            "shot" => self.say("Zrzut ekranu telefonu w schowku"),
            _ => return false,
        }
        true
    }
    fn slow(&mut self) -> bool {
        let mut ch = false;
        // ADB details only while open (one `adb shell` per 15 s)
        if self.online() && self.adb_at.map(|a| a.elapsed().as_secs() >= 15).unwrap_or(true) {
            self.adb_at = Some(Instant::now());
            self.send("adbinfo", None);
        }
        if self.said_at.map(|a| a.elapsed().as_millis() >= 2600).unwrap_or(false) {
            self.said_at = None;
            self.toast.set(0.0, 260.0, Ease::OutCubic);
            ch = true;
        }
        // the charging glyph pulses at the 250 ms tick, not per frame
        ch || self.charging()
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hits.clear();
        let (sx, sy) = (((c.w() - W) / 2.0).round(), ((c.h() - H) / 2.0).round());
        glass(c, sx, sy, W, H, 26.0, 0.86);
        self.hits.push(((sx, sy, W, H), Hit::Sheet));
        // left
        self.origin = (sx + 34.0, sy + 32.0);
        let mut left = Canvas::new(LEFT_W as u32 + 2, (H - 64.0) as u32);
        self.paint_left(&mut left, cx);
        c.pm.draw_pixmap(self.origin.0 as i32, self.origin.1 as i32, left.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        drop(left);
        // right
        let (rx, ry, rw, rh) = (sx + 440.0, sy + 1.0, W - 441.0, H - 2.0);
        rect(c, rx, ry, rw, rh, 25.0, INSET);
        rect(c, rx, ry, 1.0, rh, 0.0, HAIR);
        let t = &mut *cx.t;
        self.origin = (0.0, 0.0);
        let mut tx = rx + 26.0;
        let unread = self.threads.iter().filter(|t| !t["read"].as_bool().unwrap_or(true)).count();
        let tabs: [(&'static str, String); 3] = [
            ("notes", format!("Powiadomienia{}", if self.note_count() > 0 { format!(" {}", self.note_count()) } else { String::new() })),
            ("sms", format!("Wiadomości{}", if unread > 0 { format!(" {unread}") } else { String::new() })),
            ("people", format!("Kontakty{}", if self.contacts.is_empty() { String::new() } else { format!(" {}", self.contacts.len()) })),
        ];
        let ty = ry + 28.0;
        for (k, label) in tabs {
            let ts = st(12.5, 600.0);
            let w = t.width(&label, &ts) + 26.0;
            let on = self.tab == k;
            let h = Hit::Tab(k);
            rect(c, tx, ty, w, 32.0, 16.0, if on { FILL2 } else if self.hov(&h) { FILL } else { Rgba(0.0, 0.0, 0.0, 0.0) });
            if on {
                border(c, tx, ty, w, 32.0, 16.0, 1.0, RIM);
            }
            text_cy(c, t, tx + 13.0, ty + 16.0, &label, &ts, if on { T1 } else { T3 }, None);
            self.push_hit((tx, ty, w, 32.0), h);
            tx += w + 6.0;
        }
        let dismissable = self.notes().iter().filter(|n| n["dismiss"].as_bool().unwrap_or(false)).count();
        if self.tab == "notes" && dismissable > 1 {
            let label = "Wyczyść wszystko";
            let bw = t.width(label, &st(11.5, 600.0)) + 20.0;
            self.text_btn(c, t, rx + rw - 26.0 - bw, ty + 2.0, label, false, true, Hit::ClearAll);
        }
        // page
        let (px, py) = (rx + 26.0, ty + 32.0 + 18.0);
        let ph = (ry + rh - 24.0 - py).floor();
        self.origin = (px, py);
        let hits0 = self.hits.len();
        self.paint_page(cx, ph);
        // hits only inside the page
        let mut kept = Vec::new();
        for h in self.hits.drain(hits0..) {
            let (x, y, w, hh) = h.0;
            let (y0, y1) = (y.max(py), (y + hh).min(py + ph));
            if y1 > y0 {
                kept.push(((x, y0, w, y1 - y0), h.1));
            }
        }
        self.hits.extend(kept);
        c.pm.draw_pixmap(px as i32, py as i32, self.page.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        // toast
        let k = self.toast.value();
        if k > 0.001 && !self.said.is_empty() {
            let t = &mut *cx.t;
            let bad = self.said.starts_with('⚠');
            let msg = self.said.trim_start_matches("⚠ ").to_string();
            let ms = st(12.5, 560.0);
            let w = 15.0 + 8.0 + t.width(&msg, &ms) + 28.0;
            let y = ry + rh - 36.0 - 22.0 + (1.0 - k) * 32.0;
            let x = rx + (rw - w) / 2.0;
            rect(c, x, y, w, 36.0, 18.0, Rgba(28.0 / 255.0, 34.0 / 255.0, 46.0 / 255.0, 0.96 * k));
            border(c, x, y, w, 36.0, 18.0, 1.0, RIM.alpha_mul(k));
            c.icon(t, x + 14.0 + 7.5, y + 18.0, if bad { "alert" } else { "check" }, 15.0, (if bad { ORANGE } else { GREEN }).alpha_mul(k));
            text_cy(c, t, x + 14.0 + 23.0, y + 18.0, &msg, &ms, T1.alpha_mul(k), None);
        }
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let hit = self.hit();
        match ev {
            Ptr::Enter | Ptr::Motion | Ptr::Leave => Act::Redraw,
            Ptr::Press(_) => {
                self.pressed_outside = hit.is_none();
                Act::None
            }
            Ptr::Wheel(d) => {
                let conv = self.tab == "sms" && (self.thread.is_some() || self.composing);
                // the conversation scrolls from its end
                self.scroll = (self.scroll + if conv { -d } else { d } * 48.0).clamp(0.0, self.content_h);
                Act::Redraw
            }
            Ptr::Release(_) => {
                let Some(h) = hit else {
                    return if self.pressed_outside { Act::Close } else { Act::None };
                };
                let main_id = s(&self.main, "id");
                match h {
                    Hit::Sheet => {
                        self.focus = None;
                        Act::Redraw
                    }
                    Hit::Accept => { self.send("accept", None); Act::Redraw }
                    Hit::Reject => { self.send("reject", None); Act::Redraw }
                    Hit::Pair => { self.send("pair", None); Act::Redraw }
                    Hit::Act(i) => {
                        let screen = self.adb["screen"].as_bool().unwrap_or(false);
                        match i {
                            0 => { self.send("ring", None); self.say("Telefon dzwoni"); }
                            1 => { self.send("clip", None); self.say("Schowek wysłany na telefon"); }
                            2 => return Act::Run(vec!["kdeconnect-handler".into(), "--device".into(), main_id]),
                            3 => { self.send("browse", None); return Act::Close; }
                            4 => { self.send("mirror", None); return Act::Close; }
                            5 => self.send("shot", None),
                            6 => { self.send("photo", None); self.say("Zrób zdjęcie na telefonie"); }
                            7 => {
                                self.send(if screen { "sleep" } else { "wake" }, None);
                                self.adb_at = Some(Instant::now() - std::time::Duration::from_millis(13_800));
                            }
                            8 => {
                                let on = !self.adb["dnd"].as_bool().unwrap_or(false);
                                self.send("dnd", Some(json!(if on { 1 } else { 0 })));
                                self.adb_at = Some(Instant::now() - std::time::Duration::from_millis(13_800));
                            }
                            9 => {
                                if self.adb_ok() && self.adb["wifi"].as_bool().unwrap_or(false) {
                                    self.say("ADB już działa przez Wi-Fi");
                                } else {
                                    self.send("adbwifi", None);
                                    self.say("Włączam ADB przez Wi-Fi…");
                                }
                            }
                            10 => { self.send("adbfix", None); self.say("Nadaję uprawnienia KDE Connect…"); }
                            _ => return Act::Run(vec!["kdeconnect-app".into()]),
                        }
                        Act::Redraw
                    }
                    Hit::Field(f) => {
                        self.focus = Some(f);
                        Act::Redraw
                    }
                    Hit::Send(f) => self.submit(f),
                    Hit::MPrev => { self.send("media", Some(json!("Previous"))); Act::Redraw }
                    Hit::MPlay => { self.send("media", Some(json!("PlayPause"))); Act::Redraw }
                    Hit::MNext => { self.send("media", Some(json!("Next"))); Act::Redraw }
                    Hit::Tab(k) => {
                        if self.tab != k {
                            self.tab = k;
                            self.scroll = 0.0;
                            self.focus = if k == "people" { Some(F::Search) } else { None };
                        }
                        Act::Redraw
                    }
                    Hit::ClearAll => {
                        for n in self.notes().into_iter().filter(|n| n["dismiss"].as_bool().unwrap_or(false)) {
                            self.send("dismiss", Some(n["id"].clone()));
                        }
                        self.say("Powiadomienia usunięte");
                        Act::Redraw
                    }
                    Hit::NoteX(id) => { self.send("dismiss", Some(json!(id))); Act::Redraw }
                    Hit::NoteReply(id) => {
                        self.replying = Some(id);
                        self.reply.text.clear();
                        self.focus = Some(F::Reply);
                        Act::Redraw
                    }
                    Hit::NoteAct(id, a) => {
                        self.send("action", Some(json!({ "id": id, "action": a })));
                        self.say(&format!("Wysłano: {a}"));
                        Act::Redraw
                    }
                    Hit::NewSms => { self.compose(""); Act::Redraw }
                    Hit::Thread(i) => {
                        if let Some(th) = self.threads.get(i).cloned() {
                            let addr: Vec<String> = th["addr"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                            let nm = { let n = self.contact_name(addr.first().map(|s| s.as_str()).unwrap_or("")); if n.is_empty() { s(&th, "name") } else { n } };
                            self.open_thread(th["thread"].clone(), nm, addr);
                        }
                        Act::Redraw
                    }
                    Hit::Back => { self.close_thread(); Act::Redraw }
                    Hit::CallThread => {
                        if let Some((_, _, a)) = &self.thread {
                            self.ph.call_number(&a[0]);
                        }
                        Act::Redraw
                    }
                    Hit::ToHint(tel) => {
                        self.to.text = tel;
                        self.focus = Some(F::Msg);
                        Act::Redraw
                    }
                    Hit::PSms(tel) => { self.compose(&tel); Act::Redraw }
                    Hit::PCall(tel, nm) => {
                        self.ph.call_number(&tel);
                        self.say(&format!("Dzwonię: {nm}"));
                        Act::Redraw
                    }
                }
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape => {
                if let Some(f) = self.focus {
                    if !self.field(f).text.is_empty() {
                        self.field(f).text.clear();
                        return Act::Redraw;
                    }
                    if f == F::Reply {
                        self.replying = None;
                    }
                    self.focus = None;
                    if !matches!(f, F::Msg | F::To) {
                        return Act::Redraw;
                    }
                }
                if self.thread.is_some() || self.composing {
                    self.close_thread();
                    return Act::Redraw;
                }
                Act::Close
            }
            Keysym::Return | Keysym::KP_Enter => match self.focus {
                Some(f) => self.submit(f),
                None => Act::None,
            },
            _ => {
                // typing on the contacts tab searches
                let f = match self.focus {
                    Some(f) => f,
                    None if self.tab == "people" => F::Search,
                    None if self.tab == "sms" && (self.thread.is_some() || self.composing) => F::Msg,
                    None => return Act::None,
                };
                if self.field(f).key(ev, cx.ctrl) {
                    self.focus = Some(f);
                    if f == F::Search {
                        self.scroll = 0.0;
                    }
                    return Act::Redraw;
                }
                Act::None
            }
        }
    }
}
