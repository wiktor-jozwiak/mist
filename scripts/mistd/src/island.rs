// Mist Dynamic Island: system events Quickshell has no service for, one JSON object per line:
//   {"t": "clip", "kind": "text|link|color|files|image|secret", "text": ..., "n": ..., "path": ...}
//   {"t": "usb", "on": bool, "name": ..., "kind": "keyboard|mouse|storage|audio|video|phone|pad|printer|net|device"}
//   {"t": "mount", "on": bool, "name": ..., "path": ..., "free": bytes}
//   {"t": "cam", "on": bool, "apps": [...]}
//   {"t": "lock", "caps": bool, "num": bool, "init": bool}
// Sources: wl-paste --watch (clipboard), udevadm monitor (USB), poll(POLLPRI) on /proc/self/mounts,
// inotify on /dev/video* (camera; holders are looked up in /proc only after an open / close),
// /sys/class/leds (Caps / Num Lock: not readable from evdev without the input group, polled 4x/s,
// a read of a 2-byte sysfs file). Children die with this process (PR_SET_PDEATHSIG).
use crate::common::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::process::Child;
use std::time::{Duration, Instant};

// ------------------------------------------------------------ clipboard
struct Clip {
    slot: usize,
    last: (u64, Instant),
    skip: bool,
    tmp: String,
    slots: [String; 2],
}

fn drop_file(p: &str) {
    let _ = std::fs::remove_file(p);
}

fn is_image(raw: &[u8]) -> bool {
    raw.starts_with(b"\x89PNG\r\n\x1a\n") || raw.starts_with(b"\xff\xd8\xff") || raw.starts_with(b"RIFF") || raw.starts_with(b"GIF8")
}

fn is_color(s: &str) -> bool {
    let h = s.strip_prefix('#').unwrap_or(s);
    (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_link(s: &str) -> bool {
    ["http://", "https://", "ftp://"].iter().any(|p| s.starts_with(p) && s.len() > p.len()) && !s.chars().any(char::is_whitespace)
}

impl Clip {
    fn proc_(&mut self) -> Option<Child> {
        self.skip = true; // --watch reports the current offer at start: not a copy
        spawn(&["wl-paste", "--watch", "sh", "-c", "head -c 16777216 > \"$1\"; echo \"${CLIPBOARD_STATE:-data}\"", "_", &self.tmp])
    }

    fn event(&mut self, state: &str) {
        if self.skip {
            self.skip = false;
            drop_file(&self.tmp);
            return;
        }
        if state != "data" {
            drop_file(&self.tmp); // never keep a password (or anything else) on disk
            if state == "sensitive" {
                out(json!({"t": "clip", "kind": "secret"}));
            }
            return;
        }
        let Ok(raw) = std::fs::read(&self.tmp) else { return };
        if !is_image(&raw) {
            drop_file(&self.tmp);
        }
        if raw.is_empty() {
            return;
        }
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        raw.hash(&mut hs);
        let h = hs.finish();
        // clipboard managers re-offer the same data (selection from history, owner closed): once per 8 s
        if h == self.last.0 && self.last.1.elapsed() < Duration::from_secs(8) {
            return;
        }
        self.last = (h, Instant::now());
        if is_image(&raw) {
            self.slot ^= 1;
            let base = &self.slots[self.slot];
            let dst = format!("{}{}", base, if raw.starts_with(b"\x89PNG") { ".png" } else { ".jpg" });
            for ext in [".png", ".jpg"] {
                drop_file(&format!("{}{}", base, ext));
            }
            let _ = std::fs::rename(&self.tmp, &dst);
            out(json!({"t": "clip", "kind": "image", "path": dst, "size": raw.len()}));
            return;
        }
        let Ok(text) = String::from_utf8(raw) else { return };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if !lines.is_empty() && lines.iter().all(|l| l.starts_with("file://")) {
            let names: Vec<&str> = lines.iter().map(|l| l.trim_end_matches('/').rsplit('/').next().unwrap_or("")).collect();
            out(json!({"t": "clip", "kind": "files", "n": names.len(), "text": trunc(&names.join(", "), 160)}));
            return;
        }
        let s = text.trim();
        if is_color(s) {
            let c = if s.starts_with('#') { s.to_string() } else { format!("#{}", s) };
            out(json!({"t": "clip", "kind": "color", "text": c}));
        } else if is_link(s) {
            out(json!({"t": "clip", "kind": "link", "text": trunc(s, 200)}));
        } else {
            out(json!({"t": "clip", "kind": "text", "text": clean(s, 200), "n": s.chars().count()}));
        }
    }
}

// ------------------------------------------------------------ USB devices
// ID_USB_INTERFACES = :CCSSPP: per interface; HID boot protocol 01 = keyboard, 02 = mouse
const USB_KIND: [(&str, &str); 10] = [
    (":030101", "keyboard"), (":030102", "mouse"), (":08", "storage"), (":0e", "video"), (":01", "audio"),
    (":06", "phone"), (":07", "printer"), (":e0", "net"), (":02", "net"), (":03", "device"),
];

fn g<'a>(p: &'a HashMap<String, String>, k: &str) -> &'a str {
    p.get(k).map(|s| s.as_str()).unwrap_or("")
}

fn usb_name(p: &HashMap<String, String>) -> String {
    let v = match g(p, "ID_VENDOR_FROM_DATABASE") { "" => g(p, "ID_VENDOR").replace('_', " "), x => x.to_string() };
    let m = match g(p, "ID_MODEL_FROM_DATABASE") { "" => g(p, "ID_MODEL").replace('_', " "), x => x.to_string() };
    if !m.is_empty() && !v.is_empty() {
        // "Logitech, Inc." + "Logitech G502": the model already says who made it
        if let Some(first) = v.split_whitespace().next().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).filter(|w| !w.is_empty()) {
            if m.to_lowercase().starts_with(&first.to_lowercase()) {
                return m;
            }
        }
    }
    let s = format!("{} {}", v, m).trim().to_string();
    if s.is_empty() { "Urządzenie USB".into() } else { s }
}

fn usb_kind(p: &HashMap<String, String>) -> &'static str {
    let ifs = g(p, "ID_USB_INTERFACES");
    let id = format!("{}{}", g(p, "ID_MODEL"), g(p, "ID_VENDOR")).to_lowercase();
    if id.contains("xbox") || (ifs.contains(":0300") && g(p, "ID_MODEL").to_lowercase().contains("gamepad")) {
        return "pad";
    }
    USB_KIND.iter().find(|(k, _)| ifs.contains(k)).map(|(_, v)| *v).unwrap_or("device")
}

fn usb_block(lines: &[String], seen: &mut HashMap<String, (String, &'static str)>) {
    let p: HashMap<String, String> =
        lines.iter().filter_map(|l| l.split_once('=')).map(|(a, b)| (a.to_string(), b.to_string())).collect();
    if g(&p, "SUBSYSTEM") != "usb" || g(&p, "DEVTYPE") != "usb_device" || g(&p, "TYPE").starts_with("9/") {
        return;
    }
    let path = g(&p, "DEVPATH").to_string();
    match g(&p, "ACTION") {
        "add" => {
            let e = (usb_name(&p), usb_kind(&p));
            out(json!({"t": "usb", "on": true, "name": e.0, "kind": e.1}));
            seen.insert(path, e);
        }
        "remove" => {
            let (name, kind) = seen.remove(&path).unwrap_or_else(|| (usb_name(&p), usb_kind(&p)));
            out(json!({"t": "usb", "on": false, "name": name, "kind": kind}));
        }
        _ => {}
    }
}

// ------------------------------------------------------------ removable mounts
fn unescape_octal(s: &str) -> String {
    let b = s.as_bytes();
    let mut o = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() + 0 && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            o.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 4]).unwrap(), 8).unwrap_or(b'?'));
            i += 4;
        } else {
            o.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&o).into_owned()
}

fn mounts() -> BTreeMap<String, String> {
    let mut r = BTreeMap::new();
    if let Ok(t) = std::fs::read_to_string("/proc/self/mounts") {
        for l in t.lines() {
            let mut it = l.split_whitespace();
            let (Some(dev), Some(mp)) = (it.next(), it.next()) else { continue };
            let mp = unescape_octal(mp);
            if mp.starts_with("/run/media/") || mp.starts_with("/media/") || mp.starts_with("/mnt/") {
                r.insert(mp, dev.to_string());
            }
        }
    }
    r
}

pub fn statvfs(path: &str) -> Option<libc::statvfs> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then_some(s)
}

fn mount_free(mp: &str) -> i64 {
    statvfs(mp).map(|s| (s.f_bavail as i64) * (s.f_frsize as i64)).unwrap_or(-1)
}

fn base(p: &str) -> String {
    p.rsplit('/').next().unwrap_or("").to_string()
}

// ------------------------------------------------------------ camera
fn cam_users() -> Vec<String> {
    let me = std::process::id().to_string();
    let mut apps = vec![];
    let Ok(rd) = std::fs::read_dir("/proc") else { return apps };
    for e in rd.flatten() {
        let pid = e.file_name().to_string_lossy().into_owned();
        if !pid.bytes().all(|c| c.is_ascii_digit()) || pid == me {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(format!("/proc/{}/fd", pid)) else { continue };
        let hit = fds.flatten().any(|f| std::fs::read_link(f.path()).map(|l| l.to_string_lossy().starts_with("/dev/video")).unwrap_or(false));
        if hit {
            apps.push(std::fs::read_to_string(format!("/proc/{}/comm", pid)).map(|s| s.trim().to_string()).unwrap_or("?".into()));
        }
    }
    apps
}

fn pretty(c: &str) -> String {
    match c {
        "zen-bin" => "Zen Browser", "firefox" => "Firefox", "chromium" => "Chromium", "chrome" => "Chrome", "obs" => "OBS",
        "Discord" | "discord" => "Discord", "vesktop" => "Vesktop", "zoom" => "Zoom", "guvcview" => "Guvcview",
        "cheese" => "Cheese", "snapshot" => "Snapshot", _ => c,
    }
    .to_string()
}

const IGNORE_CAM: [&str; 6] = ["pipewire", "wireplumber", "v4l_id", "udevadm", "systemd-udevd", "mistd"];

// ------------------------------------------------------------ Caps / Num Lock
fn led(kind: &str) -> bool {
    let Ok(rd) = std::fs::read_dir("/sys/class/leds") else { return false };
    rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(&format!("::{}", kind))).any(|e| {
        std::fs::read_to_string(e.path().join("brightness")).map(|s| !matches!(s.trim(), "" | "0")).unwrap_or(false)
    })
}

const IN_MASK: u32 = libc::IN_OPEN | libc::IN_CLOSE_WRITE | libc::IN_CLOSE_NOWRITE;

fn add_watch(ino: i32, p: &str, mask: u32) {
    if let Ok(c) = std::ffi::CString::new(p) {
        unsafe { libc::inotify_add_watch(ino, c.as_ptr(), mask) };
    }
}

struct Watch {
    child: Child,
    buf: Vec<u8>,
}

pub fn main() {
    let run = runtime_dir();
    let mut clip = Clip {
        slot: 0,
        last: (0, Instant::now() - Duration::from_secs(60)),
        skip: true,
        tmp: format!("{}/mist-clip.{}.tmp", run, std::process::id()),
        slots: [format!("{}/mist-clip0", run), format!("{}/mist-clip1", run)],
    };
    // name -> running child; restart deadlines
    let mut procs: HashMap<&'static str, Watch> = HashMap::new();
    let mut restart: HashMap<&'static str, Instant> = HashMap::new();
    let start = |name: &'static str, clip: &mut Clip| -> Option<Watch> {
        let ch = if name == "clip" {
            clip.proc_()
        } else {
            spawn(&["udevadm", "monitor", "--udev", "--property", "--subsystem-match=usb/usb_device"])
        };
        ch.map(|child| Watch { child, buf: vec![] })
    };
    for n in ["clip", "udev"] {
        match start(n, &mut clip) {
            Some(w) => {
                procs.insert(n, w);
            }
            None => {
                restart.insert(n, Instant::now() + Duration::from_secs(5));
            }
        }
    }

    let mfd = std::fs::File::open("/proc/self/mounts").ok();
    let mut known = mounts();
    let ino = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if ino >= 0 {
        add_watch(ino, "/dev", libc::IN_CREATE);
        if let Ok(rd) = std::fs::read_dir("/dev") {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("video") {
                    add_watch(ino, &e.path().to_string_lossy(), IN_MASK);
                }
            }
        }
    }
    let mut cam_on = false;
    let mut cam_due: Option<Instant> = None;
    let (mut caps, mut num) = (led("capslock"), led("numlock"));
    out(json!({"t": "lock", "caps": caps, "num": num, "init": true}));
    let mut udev_buf: Vec<String> = vec![];
    let mut usb_seen = HashMap::new();

    loop {
        let mut timeout = 250i32;
        if let Some(d) = cam_due {
            timeout = timeout.min(d.saturating_duration_since(Instant::now()).as_millis() as i32);
        }
        let mut fds: Vec<libc::pollfd> = vec![];
        let mut names: Vec<&'static str> = vec![];
        if let Some(f) = &mfd {
            fds.push(libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLPRI | libc::POLLERR, revents: 0 });
            names.push("mounts");
        }
        if ino >= 0 {
            fds.push(libc::pollfd { fd: ino, events: libc::POLLIN, revents: 0 });
            names.push("ino");
        }
        for (n, w) in procs.iter() {
            fds.push(libc::pollfd { fd: w.child.stdout.as_ref().unwrap().as_raw_fd(), events: libc::POLLIN, revents: 0 });
            names.push(n);
        }
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout) };
        for (i, pf) in fds.iter().enumerate() {
            if pf.revents == 0 {
                continue;
            }
            match names[i] {
                "mounts" => {
                    let cur = mounts();
                    let (ck, kk): (BTreeSet<_>, BTreeSet<_>) = (cur.keys().collect(), known.keys().collect());
                    for mp in ck.difference(&kk) {
                        out(json!({"t": "mount", "on": true, "name": base(mp), "path": mp, "free": mount_free(mp)}));
                    }
                    for mp in kk.difference(&ck) {
                        out(json!({"t": "mount", "on": false, "name": base(mp), "path": mp}));
                    }
                    known = cur;
                }
                "ino" => {
                    let mut buf = [0u8; 4096];
                    let n = unsafe { libc::read(ino, buf.as_mut_ptr() as *mut _, buf.len()) };
                    if n <= 0 {
                        continue;
                    }
                    let n = n as usize;
                    let mut i = 0;
                    while i + 16 <= n {
                        let mask = u32::from_ne_bytes(buf[i + 4..i + 8].try_into().unwrap());
                        let ln = u32::from_ne_bytes(buf[i + 12..i + 16].try_into().unwrap()) as usize;
                        let name: Vec<u8> = buf[i + 16..(i + 16 + ln).min(n)].iter().copied().take_while(|&c| c != 0).collect();
                        i += 16 + ln;
                        if mask & libc::IN_CREATE != 0 && name.starts_with(b"video") {
                            add_watch(ino, &format!("/dev/{}", String::from_utf8_lossy(&name)), IN_MASK);
                        } else if mask & IN_MASK != 0 {
                            cam_due = Some(Instant::now() + Duration::from_millis(1200)); // probes open / close at once: settle first
                        }
                    }
                }
                name => {
                    let Some(w) = procs.get_mut(name) else { continue };
                    let mut chunk = [0u8; 65536];
                    let n = w.child.stdout.as_mut().unwrap().read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        let mut w = procs.remove(name).unwrap();
                        let _ = w.child.kill();
                        let _ = w.child.wait();
                        restart.insert(name, Instant::now() + Duration::from_secs(5));
                        continue;
                    }
                    w.buf.extend_from_slice(&chunk[..n]);
                    while let Some(p) = w.buf.iter().position(|&c| c == b'\n') {
                        let line: Vec<u8> = w.buf.drain(..=p).collect();
                        let l = String::from_utf8_lossy(&line[..line.len() - 1]).trim().to_string();
                        if name == "clip" {
                            clip.event(&l);
                        } else if !l.is_empty() {
                            udev_buf.push(l);
                        } else {
                            usb_block(&udev_buf, &mut usb_seen);
                            udev_buf.clear();
                        }
                    }
                }
            }
        }
        let now = Instant::now();
        for (name, at) in restart.clone() {
            if now >= at {
                restart.remove(name);
                match start(name, &mut clip) {
                    Some(w) => {
                        procs.insert(name, w);
                    }
                    None => {
                        restart.insert(name, now + Duration::from_secs(5));
                    }
                }
            }
        }
        if cam_due.is_some_and(|d| now >= d) {
            cam_due = None;
            let apps = cam_users();
            let on = apps.iter().any(|a| !matches!(a.as_str(), "v4l_id" | "udevadm" | "systemd-udevd"));
            if on != cam_on {
                cam_on = on;
                let shown: BTreeSet<String> = apps.iter().filter(|a| !IGNORE_CAM.contains(&a.as_str())).map(|a| pretty(a)).collect();
                out(json!({"t": "cam", "on": on, "apps": shown}));
            }
        }
        let (c, n) = (led("capslock"), led("numlock"));
        if (c, n) != (caps, num) {
            (caps, num) = (c, n);
            out(json!({"t": "lock", "caps": caps, "num": num, "init": false}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn clip_kinds() {
        assert!(is_color("ff8800") && is_color("#FF8800cc") && !is_color("#ff880") && !is_color("gg8800"));
        assert!(is_link("https://a.pl/x?y=1") && is_link("ftp://h/f") && !is_link("https://") && !is_link("https://a b") && !is_link("www.a.pl"));
        assert!(is_image(b"\x89PNG\r\n\x1a\nxxxx") && is_image(b"\xff\xd8\xff\xe0") && is_image(b"GIF89a") && !is_image(b"hello"));
    }

    #[test]
    fn usb_names_and_kinds() {
        let mut seen = HashMap::new();
        let add = "ACTION=add\nDEVPATH=/devices/pci/usb1/1-2\nSUBSYSTEM=usb\nDEVTYPE=usb_device\nTYPE=0/0/0\n\
                   ID_VENDOR_FROM_DATABASE=Logitech, Inc.\nID_MODEL_FROM_DATABASE=Logitech G502 Mouse\nID_USB_INTERFACES=:030102:030000:";
        usb_block(&block(add), &mut seen);
        assert_eq!(seen.get("/devices/pci/usb1/1-2"), Some(&("Logitech G502 Mouse".to_string(), "mouse")));
        let mut p = HashMap::new();
        p.insert("ID_VENDOR".to_string(), "SanDisk_Corp.".to_string());
        p.insert("ID_MODEL".to_string(), "Ultra_Fit".to_string());
        p.insert("ID_USB_INTERFACES".to_string(), ":080650:".to_string());
        assert_eq!((usb_name(&p), usb_kind(&p)), ("SanDisk Corp. Ultra Fit".to_string(), "storage"));
        p.insert("ID_MODEL".to_string(), "Xbox_Wireless_Controller".to_string());
        assert_eq!(usb_kind(&p), "pad");
        assert_eq!(usb_name(&HashMap::new()), "Urządzenie USB");
        // hubs and interfaces are ignored, removal reports the name seen at add
        usb_block(&block("ACTION=add\nDEVPATH=/hub\nSUBSYSTEM=usb\nDEVTYPE=usb_device\nTYPE=9/0/3"), &mut seen);
        assert!(!seen.contains_key("/hub"));
        usb_block(&block("ACTION=remove\nDEVPATH=/devices/pci/usb1/1-2\nSUBSYSTEM=usb\nDEVTYPE=usb_device\nTYPE=0/0/0"), &mut seen);
        assert!(seen.is_empty());
        // real devices of this PC (udev database)
        let kind = |ifs: &str| {
            let mut p = HashMap::new();
            p.insert("ID_USB_INTERFACES".to_string(), ifs.to_string());
            usb_kind(&p)
        };
        assert_eq!(kind(":030102:030001:030000:"), "mouse"); // HOLTEK GEM PLUS
        assert_eq!(kind(":030101:030102:"), "keyboard"); // SONiX keyboard with a mouse interface
        assert_eq!(kind(":0e0100:0e0200:010100:010200:"), "video"); // UGREEN camera with a microphone
        assert_eq!(kind(":010100:010200:030000:"), "audio"); // PCM2900C codec with volume keys
        assert_eq!(kind(":030000:"), "device"); // RGB LED controller
        assert_eq!(kind(":e00101:"), "net"); // Bluetooth dongle
    }

    #[test]
    fn mount_paths() {
        assert_eq!(unescape_octal("/run/media/wiktor/My\\040Disk"), "/run/media/wiktor/My Disk");
        assert_eq!(unescape_octal("/mnt/a\\\\b"), "/mnt/a\\\\b");
        assert_eq!(unescape_octal("/x\\04"), "/x\\04");
        assert_eq!(base("/run/media/wiktor/PENDRIVE"), "PENDRIVE");
        assert_eq!(pretty("zen-bin"), "Zen Browser");
        assert_eq!(pretty("kamoso"), "kamoso");
    }
}
