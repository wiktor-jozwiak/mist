// Mist phone link: KDE Connect (kdeconnectd stays the backend) seen through D-Bus, plus ADB when `adb` is
// installed and the phone is authorised (USB, or Wi-Fi after "adb tcpip"). One JSON object per line:
//   {"t": "state", "ok": bool, "devs": [...], "main": {...} | null}      only when something changed
//   {"t": "sms", "threads": [...]}                                        latest message of every thread
//   {"t": "conv", "thread": id, "msgs": [...]}                            one conversation (on request)
//   {"t": "contacts", "list": [{"name", "tels"}]}
//   {"t": "adb", "ok": bool, "serial", "wifi", "model", "android", ...}   ADB details (on request)
//   {"t": "ev", "k": "conn|disc|pairreq|paired|unpaired|plug|full|photo|shot|call|done|err", ...}
//   (call: "what" = ringing | talking | missedCall from KDE Connect; ringing | dialing | talking | held | idle
//    with "src": "adb" from the phone's own Telecom log)
// Commands on stdin, one JSON object per line: {"c": name, "a": argument, "id": device (optional)}.
// No polling: the daemon's D-Bus signals (battery, network, notifications, pairing, SMS, reachability) make
// the state be read again (debounced); ADB is asked only while the phone panel is open, plus one
// `adb logcat` stream for the calls. Slow work (ADB, SMS, contacts) runs on threads.
use crate::common::*;
use serde_json::{Map, Value as J, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::BufRead;
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedValue, StructureBuilder, Value};

const BUS: &str = "org.kde.kdeconnect";
const ROOT: &str = "/modules/kdeconnect";
const DEV: &str = "org.kde.kdeconnect.device";
const CONV: &str = "org.kde.kdeconnect.device.conversations";
const PKG: &str = "org.kde.kdeconnect_tp";
// plugin paths whose signals change the state (mprisremote fires on every track position: ignored)
const WATCHED: [&str; 6] = ["", "/battery", "/connectivity_report", "/notifications", "/sftp", "/clipboard"];
const SMS_SIGNALS: [&str; 4] = ["conversationCreated", "conversationUpdated", "conversationRemoved", "conversationLoaded"];

fn conn_with(timeout: u64) -> zbus::blocking::Connection {
    loop {
        match zbus::blocking::connection::Builder::session().and_then(|b| b.method_timeout(Duration::from_secs(timeout)).build()) {
            Ok(c) => return c,
            Err(_) => std::thread::sleep(Duration::from_secs(5)),
        }
    }
}

static CONN: LazyLock<zbus::blocking::Connection> = LazyLock::new(|| conn_with(5)); // reads (busctl --timeout=4)
static FIRE: LazyLock<zbus::blocking::Connection> = LazyLock::new(|| conn_with(60)); // ring, share, browse... may take long

#[derive(Default)]
struct St {
    last: Option<J>,
    last_json: String,
    online_id: String,
    contacts: Vec<J>,
    by_number: HashMap<String, String>,
    threads_json: String,
    conv_cache: HashMap<i64, BTreeMap<i64, J>>, // thread -> {uid: msg}
    conv_want: HashSet<i64>,                     // threads the UI is looking at
    note_cache: HashMap<String, J>,              // public id -> notification (dropped on notificationUpdated)
    adb_serial: String,
    adb_last_try: Option<Instant>,
    dialed: (String, f64),
}

static ST: LazyLock<Mutex<St>> = LazyLock::new(|| Mutex::new(St::default()));

fn st() -> std::sync::MutexGuard<'static, St> {
    ST.lock().unwrap_or_else(|e| e.into_inner())
}

fn bg(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f);
}

fn dev_path(did: &str) -> String {
    format!("{}/devices/{}", ROOT, did)
}

// ------------------------------------------------------------ D-Bus
fn call<B>(path: &str, iface: &str, method: &str, body: &B) -> Option<Vec<J>>
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType,
{
    CONN.call_method(Some(BUS), path, Some(iface), method, body).ok().map(|m| body_args(&m))
}

fn call0(path: &str, iface: &str, method: &str) -> Option<Vec<J>> {
    call(path, iface, method, &())
}

/// Fire-and-forget D-Bus call (ring, share, browse... may take seconds).
fn fire<B>(path: String, iface: &'static str, method: &'static str, body: B)
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
{
    bg(move || {
        let _ = FIRE.call_method(Some(BUS), path.as_str(), Some(iface), method, &body);
    });
}

fn props(path: &str, iface: &str) -> Option<Map<String, J>> {
    let m = CONN.call_method(Some(BUS), path, Some("org.freedesktop.DBus.Properties"), "GetAll", &(iface,)).ok()?;
    let h: HashMap<String, OwnedValue> = m.body().deserialize().ok()?;
    Some(h.iter().map(|(k, v)| (k.clone(), val(v))).collect())
}

fn pb(m: &Map<String, J>, k: &str) -> bool {
    m.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn ps<'a>(m: &'a Map<String, J>, k: &str) -> &'a str {
    m.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

fn pi(m: &Map<String, J>, k: &str) -> i64 {
    m.get(k).and_then(|v| v.as_i64()).unwrap_or(0)
}

// ------------------------------------------------------------ contacts (vCards of the contacts plugin)
fn norm_tel(t: &str) -> String {
    let d: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() >= 9 { d[d.len() - 9..].to_string() } else { d }
}

fn qp_decode(v: &str, charset: &str) -> String {
    let b = v.as_bytes();
    let mut o = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(x) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                o.push(x);
                i += 3;
                continue;
            }
        }
        o.push(b[i]);
        i += 1;
    }
    let cs = charset.to_lowercase();
    if cs.contains("8859-1") || cs == "latin1" {
        o.iter().map(|&c| c as char).collect()
    } else {
        String::from_utf8_lossy(&o).into_owned()
    }
}

fn load_contacts(did: &str) -> bool {
    let dir = format!("{}/.local/share/kpeoplevcard/kdeconnect-{}", home(), did);
    let mut lst: Vec<J> = vec![];
    let mut idx = HashMap::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if !e.file_name().to_string_lossy().ends_with(".vcf") {
            continue;
        }
        let Ok(raw) = std::fs::read(e.path()) else { continue };
        // folded lines; quoted-printable soft breaks ("=" at the end of a line)
        let txt = String::from_utf8_lossy(&raw).replace("\r\n ", "").replace("\n ", "").replace("=\r\n", "").replace("=\n", "");
        for card in txt.split("BEGIN:VCARD").skip(1) {
            let (mut name, mut alt, mut tels) = (String::new(), String::new(), Vec::<String>::new());
            for l in card.lines() {
                let (k, v) = l.split_once(':').unwrap_or((l, ""));
                let params = k.to_uppercase();
                let key = k.split(';').next().unwrap_or("").to_uppercase();
                let mut v = v.to_string();
                if params.contains("QUOTED-PRINTABLE") && (key == "FN" || key == "N") {
                    let cs = params.split(';').find_map(|p| p.strip_prefix("CHARSET=")).unwrap_or("UTF-8");
                    v = qp_decode(&v, cs);
                }
                match key.as_str() {
                    "FN" => name = v.trim().to_string(),
                    "N" => {
                        // family;given;middle;prefix;suffix
                        let mut n: Vec<String> = v.split(';').map(|x| x.trim().to_string()).collect();
                        n.resize(5.max(n.len()), String::new());
                        alt = [&n[3], &n[1], &n[2], &n[0], &n[4]].iter().filter(|x| !x.is_empty()).map(|x| x.as_str()).collect::<Vec<_>>().join(" ");
                    }
                    "TEL" if !v.trim().is_empty() => tels.push(v.trim().to_string()),
                    _ => {}
                }
            }
            if name.is_empty() {
                name = alt;
            }
            if !name.is_empty() && !tels.is_empty() {
                for t in &tels {
                    idx.insert(norm_tel(t), name.clone());
                }
                lst.push(json!({"name": name, "tels": tels}));
            }
        }
    }
    lst.sort_by_key(|c| js(c, "name").to_lowercase());
    let mut s = st();
    if lst != s.contacts {
        s.contacts = lst.clone();
        s.by_number = idx;
        drop(s);
        out(json!({"t": "contacts", "list": lst}));
        return true;
    }
    false
}

fn who(addr: &str) -> String {
    if addr.is_empty() {
        return String::new();
    }
    st().by_number.get(&norm_tel(addr)).cloned().unwrap_or_default()
}

// ------------------------------------------------------------ SMS
/// (isa(s)xiixixa(xsss)) -> {body, addr, date ms, out (type 2), read, thread, uid, att}
fn msg(d: &J) -> Option<J> {
    let d = d.as_array()?;
    if d.len() < 9 {
        return None;
    }
    let addrs: Vec<J> = d[2].as_array().into_iter().flatten().map(|a| a.as_array().and_then(|x| x.first()).cloned().unwrap_or(a.clone())).collect();
    let n = |v: &J| v.as_i64().unwrap_or(0);
    Some(json!({"body": d[1].as_str().unwrap_or(""), "addr": addrs, "date": n(&d[3]), "out": n(&d[4]) == 2,
        "read": n(&d[5]) != 0 || d[5].as_bool() == Some(true), "thread": n(&d[6]), "uid": n(&d[7]),
        "att": d.get(9).and_then(|x| x.as_array()).map(|x| x.len()).unwrap_or(0)}))
}

fn load_threads(did: &str) {
    let Some(r) = call0(&dev_path(did), CONV, "activeConversations") else { return };
    let mut ms: Vec<J> = r.first().and_then(|x| x.as_array()).into_iter().flatten().filter_map(msg).collect();
    ms.sort_by_key(|m| std::cmp::Reverse(m["date"].as_i64().unwrap_or(0)));
    let th: Vec<J> = ms
        .into_iter()
        .take(40)
        .map(|mut m| {
            let names: Vec<String> = m["addr"].as_array().into_iter().flatten().map(|a| a.as_str().unwrap_or("")).map(|a| {
                let w = who(a);
                if w.is_empty() { a.to_string() } else { w }
            }).collect();
            m["name"] = json!(names.join(", "));
            m["body"] = json!(clean(js(&m, "body"), 200));
            m
        })
        .collect();
    let j = J::Array(th.clone()).to_string();
    let mut s = st();
    if j != s.threads_json {
        s.threads_json = j;
        drop(s);
        out(json!({"t": "sms", "threads": th}));
    }
}

fn conv_out(thread: i64) {
    let mut ms: Vec<J> = st().conv_cache.get(&thread).map(|c| c.values().cloned().collect()).unwrap_or_default();
    ms.sort_by_key(|m| m["date"].as_i64().unwrap_or(0));
    let skip = ms.len().saturating_sub(80);
    out(json!({"t": "conv", "thread": thread, "msgs": ms[skip..]}));
}

// ------------------------------------------------------------ ADB (optional)
fn adb_bin() -> Option<String> {
    which("adb")
}

fn adb_devices() -> Vec<String> {
    let Some(b) = adb_bin() else { return vec![] };
    let (rc, o) = run(&[&b, "devices"], 5.0);
    if rc != 0 {
        return vec![];
    }
    o.lines().skip(1).filter(|l| l.split_whitespace().last() == Some("device")).filter_map(|l| l.split_whitespace().next()).map(String::from).collect()
}

/// The phone's ADB serial: USB first, else the Wi-Fi one (ip:5555), connecting to it when needed.
fn adb_pick(ip: &str) -> String {
    let Some(b) = adb_bin() else {
        st().adb_serial.clear();
        return String::new();
    };
    let devs = adb_devices();
    let pick = devs.iter().find(|d| !d.contains(':')).or_else(|| {
        devs.iter().find(|d| !ip.is_empty() && d.starts_with(&format!("{}:", ip))).or_else(|| devs.iter().find(|d| d.contains(':')))
    });
    if let Some(p) = pick {
        st().adb_serial = p.clone();
        return p.clone();
    }
    let due = st().adb_last_try.is_none_or(|t| t.elapsed() > Duration::from_secs(60));
    if !ip.is_empty() && due {
        // a closed port fails at once; at most once a minute
        st().adb_last_try = Some(Instant::now());
        let target = format!("{}:5555", ip);
        run(&[&b, "connect", &target], 4.0);
        if adb_devices().contains(&target) {
            st().adb_serial = target.clone();
            return target;
        }
    }
    st().adb_serial.clear();
    String::new()
}

fn serial() -> String {
    st().adb_serial.clone()
}

fn adb_bytes(args: &[&str], timeout: f64, input: Option<&[u8]>) -> (i32, Vec<u8>) {
    let s = serial();
    let Some(b) = adb_bin().filter(|_| !s.is_empty()) else { return (-1, vec![]) };
    let mut argv = vec![b.as_str(), "-s", s.as_str()];
    argv.extend_from_slice(args);
    run_bytes(&argv, timeout, input)
}

fn adb(args: &[&str], timeout: f64) -> (i32, String) {
    let (rc, b) = adb_bytes(args, timeout, None);
    (rc, String::from_utf8_lossy(&b).into_owned())
}

const INFO: &str = r#"
echo "model=$(getprop ro.product.marketname)|$(getprop ro.product.model)"
echo "android=$(getprop ro.build.version.release)"
echo "ui=$(getprop ro.mi.os.version.name)$(getprop ro.miui.ui.version.name)"
echo "operator=$(getprop gsm.operator.alpha)"
echo "uptime=$(cut -d' ' -f1 /proc/uptime)"
df /data | tail -1 | awk '{print "storage="$2" "$3}'
awk '/MemTotal|MemAvailable/{printf "%s=%s\n",$1,$2}' /proc/meminfo
dumpsys battery | grep -E '^ +(temperature|health|voltage|level|AC powered|USB powered|Wireless powered):' | sed 's/^ *//;s/: /=/'
dumpsys power | grep -m1 -E 'mWakefulness=' | sed 's/^ *//'
echo "wifiinfo=$(dumpsys wifi | grep -m1 'mWifiInfo SSID')"
echo "dnd=$(settings get global zen_mode)"
echo "bright=$(settings get system screen_brightness)"
echo "ring=$(cmd audio get-ringer-mode 2>/dev/null || dumpsys audio | grep -m1 -o 'mRingerMode=[A-Z]*')"
"#;

fn digits(s: &str) -> i64 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn between<'a>(s: &'a str, pre: &str, stop: &[char]) -> Option<&'a str> {
    let i = s.find(pre)? + pre.len();
    let rest = &s[i..];
    Some(&rest[..rest.find(|c| stop.contains(&c)).unwrap_or(rest.len())])
}

fn adb_info(ip: &str) {
    if adb_pick(ip).is_empty() {
        out(json!({"t": "adb", "ok": false, "have": adb_bin().is_some()}));
        return;
    }
    let (rc, o) = adb(&["shell", INFO], 10.0);
    if rc != 0 {
        out(json!({"t": "adb", "ok": false, "have": true}));
        return;
    }
    let mut kv: HashMap<String, String> = HashMap::new();
    for l in o.lines() {
        let (k, v) = l.split_once('=').unwrap_or((l, ""));
        kv.insert(k.trim().to_string(), v.trim().to_string());
    }
    let g = |k: &str| kv.get(k).map(|s| s.as_str()).unwrap_or("");
    let st_: Vec<&str> = (if g("storage").is_empty() { "0 0" } else { g("storage") }).split_whitespace().collect();
    let num = |s: Option<&&str>| s.filter(|x| x.bytes().all(|c| c.is_ascii_digit()) && !x.is_empty()).map(|x| x.parse::<i64>().unwrap_or(0) * 1024).unwrap_or(0);
    let model = g("model");
    let (mk, md) = model.split_once('|').unwrap_or((model, ""));
    let temp = g("temperature");
    let wi = g("wifiinfo");
    let ssid = wi.find("mWifiInfo SSID: ").map(|i| {
        let r = wi[i + 16..].trim_start_matches('"');
        r[..r.find(['"', ',']).unwrap_or(r.len())].to_string()
    });
    let has_ssid = ssid.is_some();
    let ssid = ssid.filter(|s| s != "<unknown ssid>").unwrap_or_default();
    let rssi = between(wi, "RSSI: ", &[',', ' ']).and_then(|x| x.parse::<i64>().ok());
    let link = between(wi, "Link speed: ", &['M', ',', ' ']).and_then(|x| x.parse::<i64>().ok());
    let health = match g("health") { "2" => "dobra", "3" => "przegrzana", "4" => "zużyta", "5" => "przepięcie", "7" => "zimna", _ => "" };
    let s = serial();
    out(json!({"t": "adb", "ok": true, "have": true, "serial": s, "wifi": s.contains(':'),
        "model": if mk.is_empty() { md } else { mk }, "android": g("android"), "ui": g("ui"), "operator": g("operator").trim_matches(','),
        "uptime": g("uptime").parse::<f64>().unwrap_or(0.0) as i64,
        "storage": [num(st_.first()), num(st_.get(1))],
        "ram": [digits(g("MemTotal:")) * 1024, digits(g("MemAvailable:")) * 1024],
        "temp": if !temp.is_empty() && temp.bytes().all(|c| c.is_ascii_digit()) { json!(temp.parse::<f64>().unwrap_or(0.0) / 10.0) } else { J::Null },
        "health": health, "volt": g("voltage").parse::<f64>().unwrap_or(0.0) / 1000.0, "screen": g("mWakefulness") == "Awake",
        "ssid": ssid, "rssi": if has_ssid { rssi.unwrap_or(0) } else { 0 }, "link": if has_ssid { link.unwrap_or(0) } else { 0 },
        "dnd": !matches!(g("dnd"), "0" | "null" | ""),
        "bright": if !g("bright").is_empty() && g("bright").bytes().all(|c| c.is_ascii_digit()) { g("bright").parse::<i64>().unwrap_or(-1) } else { -1 },
        "ringer": g("ring").replace("mRingerMode=", "").to_lowercase()}));
}

// what KDE Connect needs on Android, granted at once (each one fails silently when not declared)
const PERMS: [&str; 17] = [
    "READ_CONTACTS", "READ_SMS", "SEND_SMS", "RECEIVE_SMS", "RECEIVE_MMS", "READ_PHONE_STATE", "READ_CALL_LOG", "READ_PHONE_NUMBERS",
    "POST_NOTIFICATIONS", "READ_MEDIA_IMAGES", "READ_MEDIA_VIDEO", "READ_MEDIA_AUDIO", "READ_EXTERNAL_STORAGE", "ACCESS_FINE_LOCATION",
    "ACCESS_COARSE_LOCATION", "BLUETOOTH_CONNECT", "READ_LOGS",
];

fn ev(k: &str, text: &str) {
    out(json!({"t": "ev", "k": k, "text": text}));
}

fn adb_fix(ip: &str) {
    if adb_pick(ip).is_empty() {
        return ev("err", "ADB niedostępne");
    }
    let mut parts: Vec<String> = PERMS.iter().map(|p| format!("pm grant {} android.permission.{} 2>/dev/null", PKG, p)).collect();
    parts.extend([
        format!("cmd notification allow_listener {}/org.kde.kdeconnect.Plugins.NotificationsPlugin.NotificationReceiver", PKG),
        format!("appops set {} MANAGE_EXTERNAL_STORAGE allow", PKG), // SFTP: whole storage
        format!("appops set {} SYSTEM_ALERT_WINDOW allow", PKG),     // with READ_LOGS: clipboard from the phone in background
        format!("appops set {} RUN_ANY_IN_BACKGROUND allow", PKG),
        format!("dumpsys deviceidle whitelist +{}", PKG), // no battery optimisation: stays connected
        "echo ok".into(),
    ]);
    let (rc, o) = adb(&["shell", &parts.join(";")], 20.0);
    if rc != 0 || !o.contains("ok") {
        return ev("err", "nie udało się nadać uprawnień");
    }
    out(json!({"t": "ev", "k": "done", "icon": "shield", "text": "Uprawnienia KDE Connect nadane"}));
    // the app picks the grants up after a restart of its service
    let sh = format!("am force-stop {0}; monkey -p {0} -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; input keyevent KEYCODE_HOME", PKG);
    adb(&["shell", &sh], 10.0);
}

/// USB -> `adb tcpip 5555` -> connect over the LAN: the cable can go.
fn adb_wifi(ip: &str) {
    let devs = adb_devices();
    let usb: Vec<&String> = devs.iter().filter(|d| !d.contains(':')).collect();
    let Some(b) = adb_bin() else { return ev("err", "podłącz telefon kablem (debugowanie USB)") };
    if usb.is_empty() || ip.is_empty() {
        return ev("err", "podłącz telefon kablem (debugowanie USB)");
    }
    run(&[&b, "-s", usb[0], "tcpip", "5555"], 8.0);
    std::thread::sleep(Duration::from_millis(2500));
    let target = format!("{}:5555", ip);
    run(&[&b, "connect", &target], 6.0);
    let ok = adb_devices().contains(&target);
    out(json!({"t": "ev", "k": if ok { "done" } else { "err" }, "icon": "wifi",
        "text": if ok { "ADB przez Wi-Fi włączone" } else { "nie udało się połączyć przez Wi-Fi" }}));
}

fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

fn photos_dir() -> String {
    format!("{}/Obrazy/Telefon", home())
}

fn adb_shot(ip: &str) {
    if adb_pick(ip).is_empty() {
        return;
    }
    let (rc, png) = adb_bytes(&["exec-out", "screencap", "-p"], 15.0, None);
    if rc != 0 || !png.starts_with(b"\x89PNG") {
        return ev("err", "zrzut ekranu telefonu nie powiódł się");
    }
    let _ = std::fs::create_dir_all(photos_dir());
    let dst = format!("{}/zrzut_{}.png", photos_dir(), stamp());
    let _ = std::fs::write(&dst, &png);
    run_bytes(&["wl-copy", "--type", "image/png"], 5.0, Some(&png));
    out(json!({"t": "ev", "k": "shot", "path": dst}));
}

fn adb_simple(ip: &str, args: &[&str], ok_text: &str) {
    if adb_pick(ip).is_empty() {
        return ev("err", "ADB niedostępne");
    }
    let (rc, _) = adb(args, 60.0);
    if rc != 0 {
        ev("err", "polecenie ADB nie powiodło się");
    } else if !ok_text.is_empty() {
        ev("done", ok_text);
    }
}

fn mirror(ip: &str) {
    let Some(scrcpy) = which("scrcpy") else { return ev("err", "zainstaluj scrcpy") };
    if adb_pick(ip).is_empty() {
        return ev("err", "ADB niedostępne");
    }
    // survives this process (own session); the phone's screen stays off while mirrored, sound comes to the PC
    let mut c = std::process::Command::new(scrcpy);
    c.args(["-s", &serial(), "--window-title", "Telefon", "--turn-screen-off", "--stay-awake", "--max-size", "1600",
        "--video-bit-rate", "12M", "--max-fps", "60", "--shortcut-mod", "lsuper"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Ok(mut ch) = c.spawn() {
        bg(move || {
            let _ = ch.wait();
        });
    }
}

// ------------------------------------------------------------ calls over ADB
// The phone's KDE Connect does not always send its telephony packet (HyperOS: nothing at all), so while ADB
// is up the phone's own Telecom log is followed too: CallsManager logs every state change of every call
// ("setCallState DIALING -> ACTIVE, call: [Call id=TC@14, ..."). One `adb logcat` stream, no polling. The
// caller is the title of the dialer's call notification (the log masks numbers), or the number dialed from here.
const DIALERS: [&str; 5] = ["com.google.android.dialer", "com.android.incallui", "com.android.dialer", "com.android.phone", "com.samsung.android.incallui"];

fn call_state(s: &str) -> Option<&'static str> {
    Some(match s {
        "RINGING" => "incoming",
        "CONNECTING" | "SELECT_PHONE_ACCOUNT" | "DIALING" | "PULLING" => "dialing",
        "ACTIVE" => "active",
        "ON_HOLD" => "held",
        _ => return None,
    })
}

fn caller() -> String {
    let (rc, o) = adb(&["shell", "dumpsys notification --noredact"], 6.0);
    if rc != 0 {
        return String::new();
    }
    for blk in o.split("NotificationRecord(").skip(1) {
        let head: String = blk.chars().take(300).collect();
        if !DIALERS.iter().any(|d| head.contains(&format!("pkg={} ", d))) || !blk.contains("category=call") {
            continue;
        }
        if let Some(i) = blk.find("android.title=") {
            let r = &blk[i + 14..];
            let line = r.lines().next().unwrap_or("");
            // "String (Kasia)"
            if let (Some(a), Some(b)) = (line.find(" ("), line.rfind(')')) {
                let t = line[a + 2..b].trim();
                if !t.is_empty() && line[..a].chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return t.to_string();
                }
            }
        }
    }
    String::new()
}

/// A notification title (a name or a number) -> (name, number) through the contacts.
fn name_num(n: &str) -> (String, String) {
    let is_num = n.chars().count() >= 5 && n.chars().all(|c| c.is_ascii_digit() || " +()-".contains(c));
    if is_num {
        let w = who(n);
        return (if w.is_empty() { n.to_string() } else { w }, n.to_string());
    }
    let num = st()
        .contacts
        .iter()
        .find(|c| js(c, "name") == n)
        .and_then(|c| c["tels"].get(0).and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default();
    (n.to_string(), num)
}

fn parse_set_call_state(l: &str) -> Option<(String, String)> {
    let i = l.find("setCallState ")?;
    let r = &l[i + 13..];
    let (_, r) = r.split_once(" -> ")?;
    let (new, r) = r.split_once(", call: [Call id=")?;
    if new.is_empty() || !new.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let id: String = r.chars().take_while(|c| *c != ',' && *c != ']').collect();
    Some((new.to_string(), id))
}

fn call_watch() {
    struct C {
        id: String,
        state: &'static str,
        who: String,
        number: String,
        since: i64,
    }
    let order = ["incoming", "active", "dialing", "held"];
    let report = |calls: &HashMap<String, C>| {
        let name = js(&main_dev(), "name").to_string();
        let live = calls.values().filter(|c| order.contains(&c.state)).min_by_key(|c| order.iter().position(|o| *o == c.state));
        let Some(c) = live else {
            out(json!({"t": "ev", "k": "call", "what": "idle", "src": "adb", "name": name}));
            return;
        };
        let what = match c.state { "incoming" => "ringing", "active" => "talking", s => s };
        out(json!({"t": "ev", "k": "call", "what": what, "src": "adb", "id": c.id,
            "who": if c.who.is_empty() { "Nieznany numer" } else { &c.who }, "number": c.number, "since": c.since, "name": name}));
    };
    let mut calls: HashMap<String, C> = HashMap::new();
    loop {
        let ip = js(&main_dev(), "ip").to_string();
        if ip.is_empty() || adb_pick(&ip).is_empty() {
            std::thread::sleep(Duration::from_secs(20));
            continue;
        }
        let (Some(b), s) = (adb_bin(), serial()) else {
            std::thread::sleep(Duration::from_secs(20));
            continue;
        };
        let Some(mut ch) = spawn(&[&b, "-s", &s, "logcat", "-T", "1", "-v", "brief", "Telecom:I", "*:S"]) else {
            std::thread::sleep(Duration::from_secs(20));
            continue;
        };
        let rd = std::io::BufReader::new(ch.stdout.take().unwrap());
        for l in rd.split(b'\n').map_while(Result::ok) {
            let l = String::from_utf8_lossy(&l);
            let Some((new, cid)) = parse_set_call_state(&l) else { continue };
            let Some(stt) = call_state(&new) else {
                // DISCONNECTED / ABORTED / DISCONNECTING
                if calls.remove(&cid).is_some() {
                    report(&calls);
                }
                continue;
            };
            if !calls.contains_key(&cid) {
                let mut c = C { id: cid.clone(), state: "", who: String::new(), number: String::new(), since: 0 };
                let mut s = st();
                if stt == "dialing" && !s.dialed.0.is_empty() && now() - s.dialed.1 < 30.0 {
                    c.number = std::mem::take(&mut s.dialed.0);
                    drop(s);
                    let w = who(&c.number);
                    c.who = if w.is_empty() { c.number.clone() } else { w };
                }
                calls.insert(cid.clone(), c);
            }
            let c = calls.get_mut(&cid).unwrap();
            if c.state == stt {
                continue;
            }
            c.state = stt;
            if stt == "active" && c.since == 0 {
                c.since = now() as i64;
            }
            if c.who.is_empty() {
                std::thread::sleep(Duration::from_millis(400)); // the dialer posts its notification right after
                let (w, n) = name_num(&caller());
                c.who = w;
                c.number = n;
            }
            report(&calls);
        }
        let _ = ch.kill();
        let _ = ch.wait();
        if !calls.is_empty() {
            calls.clear();
            report(&calls);
        }
        std::thread::sleep(Duration::from_secs(10));
    }
}

// ------------------------------------------------------------ runcommand: the phone controls the PC
// the shell is native (native/mist-ui): its IPC client (lock lock, mist media ...)
fn mist_cmd(a: &[&str]) -> String {
    format!("{}/.local/bin/mist-ui-bin msg {}", home(), a.join(" "))
}

// the island is native (native/mist-ui): its IPC client
fn show(icon: &str, title: &str, sub: &str) -> String {
    format!("{}/.local/bin/mist-ui-bin msg island show {} '{}' '{}'", home(), icon, title, sub)
}

fn pc_commands(did: &str) -> Vec<(&'static str, &'static str, String)> {
    let shot = format!(
        // private runtime dir (0700), not world-readable /tmp. --share only queues the transfer (kdeconnectd
        // reads the file later), so the file is removed after a grace period, or at once on failure.
        "f=\"${{XDG_RUNTIME_DIR:?}}/mist-shot-$(date +%s).png\"; if grim \"$f\" && kdeconnect-cli -d {} --share \"$f\"; then {}; (sleep 120; rm -f \"$f\") >/dev/null 2>&1 & else rm -f \"$f\"; fi",
        did,
        show("screenshot", "Zrzut ekranu", "wysłano na telefon")
    );
    vec![
        ("mist-lock", "Zablokuj komputer", mist_cmd(&["lock", "lock"])),
        ("mist-play", "Odtwarzaj / pauza", mist_cmd(&["mist", "media", "toggle"])),
        ("mist-next", "Następny utwór", mist_cmd(&["mist", "media", "next"])),
        ("mist-volup", "Głośniej", "wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+".into()),
        ("mist-voldn", "Ciszej", "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-".into()),
        ("mist-mute", "Wycisz / włącz dźwięk", "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".into()),
        ("mist-shot", "Zrzut ekranu na telefon", shot),
        ("mist-clip", "Schowek komputera na telefon",
            format!("busctl --user call org.kde.kdeconnect /modules/kdeconnect/devices/{}/clipboard org.kde.kdeconnect.device.clipboard sendClipboard", did)),
        ("mist-suspend", "Uśpij komputer", format!("{}; sleep 1; systemctl suspend", mist_cmd(&["lock", "lock"]))),
        ("mist-poweroff", "Wyłącz komputer", "systemctl poweroff".into()),
    ]
}

/// JSON with every non-ASCII character as \uXXXX (Python's ensure_ascii).
fn ascii_json(v: &J) -> String {
    let mut o = String::new();
    for c in v.to_string().chars() {
        if c.is_ascii() {
            o.push(c);
        } else {
            let mut b = [0u16; 2];
            for u in c.encode_utf16(&mut b) {
                o.push_str(&format!("\\u{:04x}", u));
            }
        }
    }
    o
}

/// Adds the Mist commands to the runcommand plugin (user's own commands are kept), then reloads plugins.
fn setup_runcommand(did: &str) -> bool {
    let f = format!("{}/.config/kdeconnect/{}/kdeconnect_runcommand/config", home(), did);
    let cur: Map<String, J> = std::fs::read_to_string(&f)
        .ok()
        .and_then(|t| {
            let l = t.lines().find(|l| l.starts_with("commands=\"@ByteArray("))?;
            let inner = l.trim_end().strip_prefix("commands=\"@ByteArray(")?.strip_suffix(")\"")?;
            serde_json::from_str::<Map<String, J>>(&inner.replace("\\\"", "\"").replace("\\\\", "\\")).ok()
        })
        .unwrap_or_default();
    let mut merged: Map<String, J> = cur.iter().filter(|(k, _)| !k.starts_with("mist-")).map(|(k, v)| (k.clone(), v.clone())).collect();
    for (k, n, c) in pc_commands(did) {
        merged.insert(k.into(), json!({"name": n, "command": c}));
    }
    if merged == cur {
        return false;
    }
    let raw = ascii_json(&J::Object(merged));
    if let Some(d) = std::path::Path::new(&f).parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // QSettings INI: a quoted @ByteArray, \ and " escaped
    let _ = std::fs::write(&f, format!("[General]\ncommands=\"@ByteArray({})\"\n", raw.replace('\\', "\\\\").replace('"', "\\\"")));
    call0(&dev_path(did), DEV, "reloadPlugins");
    true
}

// ------------------------------------------------------------ state
fn device(did: &str) -> Option<J> {
    let p = props(&dev_path(did), DEV)?;
    let ip = p.get("reachableAddresses").and_then(|a| a.as_array()).and_then(|a| a.first()).and_then(|x| x.as_str()).unwrap_or("");
    Some(json!({"id": did, "name": ps(&p, "name"), "type": ps(&p, "type"), "reach": pb(&p, "isReachable"),
        "pair": pi(&p, "pairState"), "key": ps(&p, "verificationKey"), "ip": ip}))
}

fn details(d: &J) -> J {
    let did = js(d, "id").to_string();
    let path = dev_path(&did);
    let plugins: Vec<String> = call0(&path, DEV, "loadedPlugins")
        .and_then(|r| r.first().and_then(|x| x.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .map(|p| p.as_str().unwrap_or("").replace("kdeconnect_", ""))
        .collect();
    let has = |p: &str| plugins.iter().any(|x| x == p);
    let mut m = d.clone();
    for (k, v) in [("plugins", json!(plugins)), ("bat", json!(-1)), ("chg", json!(false)), ("net", json!("")), ("sig", json!(-1)),
        ("notes", json!([])), ("nNotes", json!(0)), ("mount", json!("")), ("clipAuto", json!(true))] {
        m[k] = v;
    }
    if has("battery") {
        if let Some(b) = props(&format!("{}/battery", path), &format!("{}.battery", DEV)) {
            m["bat"] = json!(pi(&b, "charge"));
            m["chg"] = json!(pb(&b, "isCharging"));
        }
    }
    if has("connectivity_report") {
        if let Some(c) = props(&format!("{}/connectivity_report", path), &format!("{}.connectivity_report", DEV)) {
            m["net"] = json!(ps(&c, "cellularNetworkType"));
            m["sig"] = json!(pi(&c, "cellularNetworkStrength"));
        }
    }
    if has("clipboard") {
        if let Some(c) = props(&format!("{}/clipboard", path), &format!("{}.clipboard", DEV)) {
            m["clipAuto"] = json!(!pb(&c, "isAutoShareDisabled"));
        }
    }
    if has("notifications") {
        let ids: Vec<String> = call0(&format!("{}/notifications", path), &format!("{}.notifications", DEV), "activeNotifications")
            .and_then(|r| r.first().and_then(|x| x.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
        let mut notes = vec![];
        for nid in &ids {
            let cached = st().note_cache.get(nid).cloned();
            let n = match cached {
                Some(n) => n,
                None => {
                    let Some(p) = props(&format!("{}/notifications/{}", path, nid), &format!("{}.notifications.notification", DEV)) else { continue };
                    let n = json!({"id": nid, "app": ps(&p, "appName"), "title": clean(ps(&p, "title"), 120), "text": clean(ps(&p, "text"), 400),
                        "icon": ps(&p, "iconPath"), "dismiss": pb(&p, "dismissable"), "silent": pb(&p, "silent"), "reply": !ps(&p, "replyId").is_empty(),
                        "at": now() as i64, "actions": p.get("actions").cloned().filter(|a| a.is_array()).unwrap_or(json!([]))});
                    st().note_cache.insert(nid.clone(), n.clone());
                    n
                }
            };
            notes.push(n);
        }
        st().note_cache.retain(|k, _| ids.contains(k));
        // ongoing ones (music, navigation) stay on the phone
        let shown: Vec<J> = notes.into_iter().filter(|n| n["dismiss"] == true || n["reply"] == true).collect();
        m["nNotes"] = json!(shown.len());
        m["notes"] = json!(shown.iter().rev().take(30).cloned().collect::<Vec<_>>()); // newest first
    }
    if has("sftp") {
        let sp = format!("{}/sftp", path);
        let si = format!("{}.sftp", DEV);
        if call0(&sp, &si, "isMounted").and_then(|r| r.first().and_then(|x| x.as_bool())) == Some(true) {
            m["mount"] = json!(call0(&sp, &si, "mountPoint").and_then(|r| r.first().and_then(|x| x.as_str().map(String::from))).unwrap_or_default());
        }
    }
    m
}

fn snapshot() -> J {
    let Some(ids) = call(ROOT, "org.kde.kdeconnect.daemon", "devices", &(false, false)) else {
        return json!({"ok": false, "devs": [], "main": null});
    };
    let mut devs: Vec<J> = ids.first().and_then(|x| x.as_array()).into_iter().flatten().filter_map(|i| i.as_str()).filter_map(device).collect();
    // main device: a reachable paired phone > any paired phone > a reachable device > anything
    let rank = |d: &J| {
        let (p, r) = (d["pair"].as_i64().unwrap_or(0), d["reach"] == true);
        (p == 3 && r, p == 3, p == 2, r, js(d, "type") == "phone")
    };
    devs.sort_by(|a, b| rank(b).cmp(&rank(a)));
    let main = match devs.first() {
        Some(d) if d["pair"] == 3 && d["reach"] == true => details(d),
        Some(d) => {
            let mut m = d.clone();
            m["plugins"] = json!([]);
            m
        }
        None => J::Null,
    };
    json!({"ok": true, "devs": devs, "main": main})
}

fn main_dev() -> J {
    st().last.as_ref().map(|l| l["main"].clone()).filter(|m| m.is_object()).unwrap_or(json!({}))
}

fn events(a: &J, b: &J) {
    let old: HashMap<&str, &J> = a["devs"].as_array().into_iter().flatten().map(|d| (js(d, "id"), d)).collect();
    let pair = |d: &J| d["pair"].as_i64().unwrap_or(0);
    for d in b["devs"].as_array().into_iter().flatten() {
        let (id, name) = (js(d, "id"), js(d, "name"));
        let Some(o) = old.get(id) else {
            if pair(d) == 2 {
                out(json!({"t": "ev", "k": "pairreq", "id": id, "name": name, "key": js(d, "key")}));
            }
            continue;
        };
        if pair(d) == 3 && pair(o) == 3 && d["reach"] != o["reach"] {
            out(json!({"t": "ev", "k": if d["reach"] == true { "conn" } else { "disc" }, "id": id, "name": name}));
        }
        if pair(d) != pair(o) {
            match (pair(o), pair(d)) {
                (_, 2) => out(json!({"t": "ev", "k": "pairreq", "id": id, "name": name, "key": js(d, "key")})),
                (_, 3) => out(json!({"t": "ev", "k": "paired", "id": id, "name": name})),
                (3, _) => out(json!({"t": "ev", "k": "unpaired", "id": id, "name": name})),
                (1, 0) => out(json!({"t": "ev", "k": "err", "id": id, "name": name, "text": "parowanie odrzucone"})),
                _ => {}
            }
        }
    }
    let (ma, mb) = (&a["main"], &b["main"]);
    let bat = |m: &J| m["bat"].as_i64().unwrap_or(-1);
    if ma.is_object() && mb.is_object() && ma["id"] == mb["id"] && bat(mb) >= 0 && bat(ma) >= 0 {
        if mb["chg"] == true && ma["chg"] != true {
            out(json!({"t": "ev", "k": "plug", "id": mb["id"], "name": mb["name"], "bat": bat(mb)}));
        } else if mb["chg"] == true && bat(mb) == 100 && bat(ma) < 100 {
            out(json!({"t": "ev", "k": "full", "id": mb["id"], "name": mb["name"], "bat": 100}));
        }
    }
}

fn sorted_json(v: &J) -> String {
    // serde_json maps are ordered by key already: a stable text for comparison
    v.to_string()
}

fn refresh() {
    let s = snapshot();
    let j = sorted_json(&s);
    let prev = st().last.clone();
    if let Some(p) = &prev {
        events(p, &s);
    }
    let changed = {
        let mut g = st();
        g.last = Some(s.clone());
        let c = j != g.last_json;
        if c {
            g.last_json = j;
        }
        c
    };
    if changed {
        let mut o = s.clone();
        o["t"] = json!("state");
        out(o);
    }
    let m = &s["main"];
    let on = if m["pair"] == 3 && m["reach"] == true { js(m, "id").to_string() } else { String::new() };
    let fresh = {
        let mut g = st();
        let f = !on.is_empty() && on != g.online_id;
        g.online_id = on.clone();
        f
    };
    if fresh {
        // just linked: the slow, once-per-link work in the background
        let plugins: Vec<String> = m["plugins"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        let ip = js(m, "ip").to_string();
        bg(move || linked(&on, &plugins, &ip));
    }
}

fn linked(did: &str, plugins: &[String], ip: &str) {
    let has = |p: &str| plugins.iter().any(|x| x == p);
    if has("runcommand") && setup_runcommand(did) {
        return; // plugins reload: the next refresh comes back here
    }
    load_contacts(did);
    let p = dev_path(did);
    if has("contacts") {
        fire(format!("{}/contacts", p), "org.kde.kdeconnect.device.contacts", "synchronizeRemoteWithLocal", ());
    }
    if has("sms") {
        call0(&p, CONV, "requestAllConversationThreads");
        std::thread::sleep(Duration::from_secs(2));
        load_threads(did);
    }
    adb_pick(ip);
}

// ------------------------------------------------------------ commands
fn command(line: &str, photo_wait: &mut Vec<(String, Instant)>) {
    let Ok(o) = serde_json::from_str::<J>(line) else { return };
    let c = js(&o, "c");
    let a = o.get("a").cloned().unwrap_or(J::Null);
    let astr = a.as_str().unwrap_or("").to_string();
    let m = main_dev();
    let did = if js(&o, "id").is_empty() { js(&m, "id").to_string() } else { js(&o, "id").to_string() };
    let ip = js(&m, "ip").to_string();
    if c == "refresh" {
        return refresh();
    }
    if did.is_empty() {
        return;
    }
    let p = dev_path(&did);
    let note = |id: &str| format!("{}/notifications/{}", p, id);
    const NOTE: &str = "org.kde.kdeconnect.device.notifications.notification";
    match c {
        "pair" => fire(p.clone(), DEV, "requestPairing", ()),
        "accept" => fire(p.clone(), DEV, "acceptPairing", ()),
        "reject" => fire(p.clone(), DEV, "cancelPairing", ()),
        "unpair" => fire(p.clone(), DEV, "unpair", ()),
        "ring" => fire(format!("{}/findmyphone", p), "org.kde.kdeconnect.device.findmyphone", "ring", ()),
        "ping" => fire(format!("{}/ping", p), "org.kde.kdeconnect.device.ping", "sendPing", ()),
        "clip" => fire(format!("{}/clipboard", p), "org.kde.kdeconnect.device.clipboard", "sendClipboard", ()),
        "browse" => fire(format!("{}/sftp", p), "org.kde.kdeconnect.device.sftp", "startBrowsing", ()),
        "unmount" => fire(format!("{}/sftp", p), "org.kde.kdeconnect.device.sftp", "unmount", ()),
        "share" if !a.is_null() && a != "" => {
            let list: Vec<String> = match &a {
                J::Array(x) => x.iter().filter_map(|u| u.as_str().map(String::from)).collect(),
                J::String(s) => vec![s.clone()],
                _ => vec![],
            };
            let urls: Vec<String> = list.into_iter().map(|u| if u.contains("://") { u } else { format!("file://{}", u) }).collect();
            fire(format!("{}/share", p), "org.kde.kdeconnect.device.share", "shareUrls", (urls,));
        }
        "text" if !astr.is_empty() => fire(format!("{}/share", p), "org.kde.kdeconnect.device.share", "shareText", (astr,)),
        "dismiss" if !astr.is_empty() => fire(note(&astr), NOTE, "dismiss", ()),
        "reply" if !js(&a, "text").is_empty() => fire(note(js(&a, "id")), NOTE, "sendReply", (js(&a, "text").to_string(),)),
        "action" if a.is_object() => fire(note(js(&a, "id")), NOTE, "sendAction", (js(&a, "action").to_string(),)),
        "photo" => {
            let _ = std::fs::create_dir_all(photos_dir());
            let dst = format!("{}/{}.jpg", photos_dir(), stamp());
            fire(format!("{}/photo", p), "org.kde.kdeconnect.device.photo", "requestPhoto", (format!("file://{}", dst),));
            photo_wait.push((dst, Instant::now() + Duration::from_secs(120)));
        }
        // Play, Pause, PlayPause, Next, Previous, Stop
        "media" if !astr.is_empty() => fire(format!("{}/mprisremote", p), "org.kde.kdeconnect.device.mprisremote", "sendAction", (astr,)),
        "mediavol" if !a.is_null() => {
            let v = a.as_f64().or_else(|| a.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0) as i32;
            let path = format!("{}/mprisremote", p);
            bg(move || {
                let _ = FIRE.call_method(Some(BUS), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Set",
                    &("org.kde.kdeconnect.device.mprisremote", "volume", Value::I32(v)));
            });
        }
        // remote keyboard: a key or a text typed on the phone
        "key" if !astr.is_empty() => fire(format!("{}/remotekeyboard", p), "org.kde.kdeconnect.device.remotekeyboard", "sendKeyPress", (astr,)),
        // ---- SMS
        "threads" => bg(move || {
            call0(&dev_path(&did), CONV, "requestAllConversationThreads");
            std::thread::sleep(Duration::from_millis(1500));
            load_threads(&did);
        }),
        "conv" if !a.is_null() => {
            let t = a.as_i64().or_else(|| a.as_f64().map(|x| x as i64)).or_else(|| a.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
            let cached = {
                let mut g = st();
                g.conv_want.insert(t);
                g.conv_cache.contains_key(&t)
            };
            if cached {
                conv_out(t);
            }
            fire(p.clone(), CONV, "requestConversation", (t, 0i32, 60i32));
        }
        "unconv" if !a.is_null() => {
            let t = a.as_i64().or_else(|| a.as_f64().map(|x| x as i64)).unwrap_or(0);
            st().conv_want.remove(&t);
        }
        "sendsms" if !js(&a, "text").is_empty() => {
            let text = js(&a, "text").to_string();
            let thread = a.get("thread").and_then(|t| t.as_i64().or_else(|| t.as_f64().map(|x| x as i64))).unwrap_or(0);
            if thread != 0 {
                let path = p.clone();
                bg(move || {
                    let _ = FIRE.call_method(Some(BUS), path.as_str(), Some(CONV), "replyToConversation", &(thread, text, Vec::<Value>::new()));
                });
            } else if !a["to"].is_null() {
                let to: Vec<String> = match &a["to"] {
                    J::Array(x) => x.iter().filter_map(|u| u.as_str().map(String::from)).collect(),
                    J::String(s) => vec![s.clone()],
                    _ => vec![],
                };
                let path = p.clone();
                bg(move || {
                    let addrs: Vec<Value> = to.iter().filter_map(|n| StructureBuilder::new().add_field(n.clone()).build().ok()).map(Value::Structure).collect();
                    let _ = FIRE.call_method(Some(BUS), path.as_str(), Some(CONV), "sendWithoutConversation", &(addrs, text, Vec::<Value>::new()));
                });
            }
            bg(move || {
                std::thread::sleep(Duration::from_secs(3));
                load_threads(&did);
            });
        }
        "contacts" => {
            let d2 = did.clone();
            bg(move || {
                call0(&format!("{}/contacts", dev_path(&d2)), "org.kde.kdeconnect.device.contacts", "synchronizeRemoteWithLocal");
                load_contacts(&d2);
            });
            load_contacts(&did);
            out(json!({"t": "contacts", "list": st().contacts.clone()}));
        }
        // ---- ADB
        "adbinfo" => bg(move || adb_info(&ip)),
        "adbfix" => bg(move || adb_fix(&ip)),
        "adbwifi" => bg(move || adb_wifi(&ip)),
        "mirror" => bg(move || mirror(&ip)),
        "shot" => bg(move || adb_shot(&ip)),
        "call" if !astr.is_empty() => {
            st().dialed = (astr.clone(), now());
            let num: String = astr.chars().filter(|c| c.is_ascii_digit() || "+*#".contains(*c)).collect();
            bg(move || adb_simple(&ip, &["shell", &format!("am start -a android.intent.action.CALL -d tel:{}", num)], &format!("dzwonię: {}", astr)));
        }
        "hangup" => bg(move || adb_simple(&ip, &["shell", "input keyevent KEYCODE_ENDCALL"], "")),
        // a ringing call: the headset button answers it
        "answer" => bg(move || adb_simple(&ip, &["shell", "input keyevent KEYCODE_HEADSETHOOK"], "")),
        "wake" => bg(move || adb_simple(&ip, &["shell", "input keyevent KEYCODE_WAKEUP"], "")),
        "sleep" => bg(move || adb_simple(&ip, &["shell", "input keyevent KEYCODE_SLEEP"], "")),
        // 0 off, 1 priority only
        "dnd" => {
            let on = a.as_i64().unwrap_or(0) != 0 || a == true;
            bg(move || adb_simple(&ip, &["shell", &format!("cmd notification set_dnd {}", if on { "priority" } else { "off" })], ""));
        }
        // a link straight in the phone's browser / app
        "open" if !astr.is_empty() => {
            let u = astr.replace('\'', "");
            bg(move || adb_simple(&ip, &["shell", &format!("am start -a android.intent.action.VIEW -d '{}'", u)], ""));
        }
        "apk" if !astr.is_empty() => {
            let f = astr.replace("file://", "");
            let name = f.rsplit('/').next().unwrap_or("").to_string();
            bg(move || adb_simple(&ip, &["install", "-r", &f], &format!("zainstalowano {}", name)));
        }
        _ => {}
    }
}

// ------------------------------------------------------------ signals
enum Ev {
    Sig { member: String, path: String, args: Vec<J> },
    Cmd(Option<String>),
}

fn monitor(tx: mpsc::Sender<Ev>) {
    std::thread::spawn(move || loop {
        let conn = conn_with(5);
        for rule in [
            format!("type='signal',path_namespace='{}'", ROOT),
            format!("type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{}'", BUS),
        ] {
            let _ = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(rule,));
        }
        for m in zbus::blocking::MessageIterator::from(&conn) {
            let Ok(m) = m else { continue };
            if m.message_type() != zbus::message::Type::Signal {
                continue;
            }
            let h = m.header();
            let member = h.member().map(|x| x.to_string()).unwrap_or_default();
            let path = h.path().map(|x| x.to_string()).unwrap_or_default();
            if member != "NameOwnerChanged" && !path.starts_with(ROOT) {
                continue;
            }
            if member == "NameOwnerChanged" && h.sender().map(|s| s.as_str()) != Some("org.freedesktop.DBus") {
                continue;
            }
            if tx.send(Ev::Sig { member, path, args: body_args(&m) }).is_err() {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(5)); // the bus went away: a fresh connection
    });
}

fn relevant(member: &str, path: &str) -> bool {
    if member == "NameOwnerChanged" || path == ROOT {
        return true;
    }
    let Some(rest) = path.strip_prefix(&format!("{}/devices/", ROOT)) else { return false };
    let sub = rest.find('/').map(|i| &rest[i..]).unwrap_or("");
    WATCHED.iter().any(|w| sub == *w || (!w.is_empty() && sub.starts_with(&format!("{}/", w))))
}

pub fn main() {
    let (tx, rx) = mpsc::channel::<Ev>();
    stdin_lines(tx.clone(), Ev::Cmd);
    monitor(tx);
    bg(call_watch);
    let mut due: Option<Instant> = Some(Instant::now()); // first snapshot at once
    let mut sms_due: Option<Instant> = None;
    let mut conv_due: HashMap<i64, Instant> = HashMap::new();
    let mut photo_wait: Vec<(String, Instant)> = vec![];
    loop {
        let now_i = Instant::now();
        let mut wait = Duration::from_secs(3600);
        for d in [due, sms_due].into_iter().flatten().chain(conv_due.values().copied()) {
            wait = wait.min(d.saturating_duration_since(now_i));
        }
        if !photo_wait.is_empty() {
            wait = wait.min(Duration::from_secs(1));
        }
        match rx.recv_timeout(wait) {
            Ok(Ev::Cmd(None)) => return, // Quickshell went away
            Ok(Ev::Cmd(Some(l))) => command(&l, &mut photo_wait),
            Ok(Ev::Sig { member, path, args }) => {
                if member == "callReceived" {
                    // telephony: ringing / talking / missedCall
                    let g = |i: usize| args.get(i).and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let (what, number, name) = (g(0), g(1), g(2));
                    let w = if name.is_empty() { who(&number) } else { name };
                    out(json!({"t": "ev", "k": "call", "what": what, "number": number, "who": w, "name": js(&main_dev(), "name")}));
                    continue;
                }
                if SMS_SIGNALS.contains(&member.as_str()) {
                    // one message per signal (a loaded thread = dozens): cache it, answer once the burst settles
                    if member == "conversationUpdated" || member == "conversationCreated" {
                        if let Some(x) = args.first().and_then(msg) {
                            let t = x["thread"].as_i64().unwrap_or(0);
                            let want = {
                                let mut g = st();
                                g.conv_cache.entry(t).or_default().insert(x["uid"].as_i64().unwrap_or(0), x);
                                g.conv_want.contains(&t)
                            };
                            if want {
                                conv_due.insert(t, Instant::now() + Duration::from_millis(250));
                            }
                        }
                    } else if member == "conversationLoaded" {
                        if let Some(t) = args.first().and_then(|x| x.as_i64()) {
                            conv_due.insert(t, Instant::now() + Duration::from_millis(50));
                        }
                    }
                    if member != "conversationLoaded" && sms_due.is_none() {
                        sms_due = Some(Instant::now() + Duration::from_millis(600));
                    }
                    continue;
                }
                if member == "localCacheSynchronized" {
                    let did = js(&main_dev(), "id").to_string();
                    if !did.is_empty() {
                        bg(move || {
                            load_contacts(&did);
                        });
                    }
                    continue;
                }
                if relevant(&member, &path) {
                    if member == "notificationUpdated" {
                        let mut g = st();
                        for a in &args {
                            if let Some(s) = a.as_str() {
                                g.note_cache.remove(s);
                            }
                        }
                    }
                    // a burst (connect = identity + battery + network + notifications): one read after it settles
                    if due.is_none() {
                        due = Some(Instant::now() + Duration::from_millis(if member == "NameOwnerChanged" { 1000 } else { 150 }));
                    }
                }
            }
            Err(_) => {}
        }
        let now_i = Instant::now();
        if due.is_some_and(|d| now_i >= d) {
            due = None;
            refresh();
        }
        if sms_due.is_some_and(|d| now_i >= d) {
            sms_due = None;
            let did = js(&main_dev(), "id").to_string();
            if !did.is_empty() {
                bg(move || load_threads(&did));
            }
        }
        for (t, at) in conv_due.clone() {
            if now_i >= at {
                conv_due.remove(&t);
                if st().conv_want.contains(&t) {
                    conv_out(t);
                }
            }
        }
        photo_wait.retain(|(path, deadline)| {
            if std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                std::thread::sleep(Duration::from_millis(300)); // the transfer writes the file in one go: let it close
                out(json!({"t": "ev", "k": "photo", "path": path}));
                return false;
            }
            now_i <= *deadline
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_not_in_tmp() {
        let c = pc_commands("dev").into_iter().find(|(k, _, _)| *k == "mist-shot").unwrap().2;
        assert!(!c.contains("/tmp/"), "{c}");
        assert!(c.contains("${XDG_RUNTIME_DIR:?}/mist-shot-"), "{c}");
        assert!(c.contains("rm -f \"$f\""), "{c}");
        // valid sh syntax
        let ok = std::process::Command::new("sh").args(["-n", "-c", &c]).status().unwrap().success();
        assert!(ok, "{c}");
    }

    #[test]
    fn telecom_lines() {
        let l = "10-09 13:17:50.007 I/Telecom ( 1751): CallsManager: setCallState NEW -> RINGING, call: [Call id=TC@13, state=NEW, tpac=ComponentInfo{com.android.phone/x}]";
        assert_eq!(parse_set_call_state(l), Some(("RINGING".into(), "TC@13".into())));
        let l = "I/Telecom ( 1751): CallsManager: setCallState RINGING -> DISCONNECTED, call: [Call id=TC@13, state=RINGING";
        assert_eq!(parse_set_call_state(l), Some(("DISCONNECTED".into(), "TC@13".into())));
        assert_eq!(call_state("DISCONNECTED"), None);
        assert_eq!(call_state("DIALING"), Some("dialing"));
        assert_eq!(parse_set_call_state("I/Telecom: PhoneStateBroadcaster: Broadcasted state change: 1"), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(norm_tel("+48 600-100-200"), "600100200");
        assert_eq!(name_num("+48 600 100 200").1, "+48 600 100 200");
        assert_eq!(name_num("Kasia").0, "Kasia");
        assert_eq!(qp_decode("=C5=81=C3=B3d=C5=BA", "UTF-8"), "Łódź");
    }

    #[test]
    fn sms_struct() {
        let v = json!([1, "hej", [["+48600100200"]], 1700000000000i64, 2, 1, 42, 7, -1, []]);
        let m = msg(&v).unwrap();
        assert_eq!(m["addr"], json!(["+48600100200"]));
        assert_eq!(m["out"], json!(true));
        assert_eq!(m["thread"], json!(42));
    }

    #[test]
    fn runcommand_is_stable() {
        // the config written for each paired device must read back as "nothing to change"
        // (no paired device with Mist's commands: nothing to check)
        let Ok(rd) = std::fs::read_dir(format!("{}/.config/kdeconnect", home())) else { return };
        for e in rd.flatten() {
            let did = e.file_name().to_string_lossy().into_owned();
            let Ok(t) = std::fs::read_to_string(e.path().join("kdeconnect_runcommand/config")) else { continue };
            let Some(l) = t.lines().find(|l| l.starts_with("commands=\"@ByteArray(")) else { continue };
            let inner = l.trim_end().strip_prefix("commands=\"@ByteArray(").unwrap().strip_suffix(")\"").unwrap();
            let cur: Map<String, J> = serde_json::from_str(&inner.replace("\\\"", "\"").replace("\\\\", "\\")).unwrap();
            if !cur.keys().any(|k| k.starts_with("mist-")) {
                continue;
            }
            let mut merged: Map<String, J> = cur.iter().filter(|(k, _)| !k.starts_with("mist-")).map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, n, c) in pc_commands(&did) {
                merged.insert(k.into(), json!({"name": n, "command": c}));
            }
            assert_eq!(merged, cur, "{did}");
        }
    }
}
