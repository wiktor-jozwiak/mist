// Mist phone calls over Bluetooth: the computer is the phone's hands-free kit (HFP, HF role of PipeWire's own
// bluez5 backend). WirePlumber runs PipeWire's telephony service (org.pipewire.Telephony, session bus): the
// phone is an "audio gateway" there and every call an object with Answer / Hangup. One JSON object per line:
//   {"t": "tel", "ags": [{"path", "addr"}], "calls": [{"path", "state", "number", "name", "since"}]}
//        state = incoming | waiting | dialing | alerting | active | held | disconnected; since = start (s, epoch)
//   {"t": "audio", "on": bool, "pc": bool, "mute": bool, "err": str}
//        on = the call audio goes through this computer now; pc = it should (user choice, default yes)
// Commands on stdin, one JSON object per line: {"c": name, "a": argument}
//   answer [path] · hangup [path] · hangupall · dial number · tones "123#" · mute · pc (toggle PC / phone)
// Call audio: while a call is up, a private PipeWire client (a generated config, `pipewire -c`) loads WebRTC
// echo cancellation + two loopbacks: phone voice -> default speakers, default microphone -> phone. Handing the
// call back to the phone = the Bluetooth card leaves its "audio-gateway" profile (the phone drops the SCO link
// and goes back to the earpiece); taking it again = the profile back + Transport.Activate.
// While a call rings (and no call audio runs) the computer rings too. No polling: D-Bus signals of the
// service, the objects are read again after them. Children die with this process (PR_SET_PDEATHSIG).
use crate::common::*;
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};
use std::os::unix::process::CommandExt;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const SVC: &str = "org.pipewire.Telephony";
const ROOT: &str = "/org/pipewire/Telephony";
const AG: &str = "org.pipewire.Telephony.AudioGateway1";
const CALL: &str = "org.pipewire.Telephony.Call1";
const TRANSPORT: &str = "org.pipewire.Telephony.AudioGatewayTransport1";
const RING: &str = "/usr/share/sounds/freedesktop/stereo/phone-incoming-call.oga";

static TERM: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    TERM.store(true, Ordering::SeqCst);
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn sval(p: &HashMap<String, OwnedValue>, k: &str) -> String {
    match p.get(k).map(|v| &**v) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

struct Tel {
    conn: zbus::blocking::Connection,
    since: HashMap<String, i64>,
    last: Option<J>,
}

#[derive(Clone)]
struct CallObj {
    path: String,
    state: String,
}

impl Tel {
    fn bus_call(&self, path: &str, iface: &str, method: &str, arg: Option<&str>) {
        let r = match arg {
            Some(a) => self.conn.call_method(Some(SVC), path, Some(iface), method, &(a,)),
            None => self.conn.call_method(Some(SVC), path, Some(iface), method, &()),
        };
        let _ = r;
    }

    /// (ags [(path, addr)], calls, transports)
    fn read(&mut self) -> (Vec<(String, String)>, Vec<CallObj>, Vec<String>) {
        let objs: Objects = self
            .conn
            .call_method(Some(SVC), ROOT, Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
            .ok()
            .and_then(|m| m.body().deserialize::<Objects>().ok())
            .unwrap_or_default();
        let sorted: BTreeMap<String, _> = objs.into_iter().map(|(k, v)| (k.as_str().to_string(), v)).collect();
        let (mut ags, mut calls, mut transports, mut jcalls) = (vec![], vec![], vec![], vec![]);
        for (path, ifaces) in &sorted {
            if let Some(p) = ifaces.get(AG) {
                ags.push((path.clone(), sval(p, "Address")));
            }
            if let Some(p) = ifaces.get(CALL) {
                let st = sval(p, "State");
                if st == "active" && !self.since.contains_key(path) {
                    self.since.insert(path.clone(), now() as i64);
                }
                jcalls.push(json!({"path": path, "state": st, "number": sval(p, "LineIdentification"), "name": sval(p, "Name"),
                    "since": self.since.get(path).copied().unwrap_or(0)}));
                calls.push(CallObj { path: path.clone(), state: st });
            }
            if ifaces.contains_key(TRANSPORT) {
                transports.push(path.clone());
            }
        }
        self.since.retain(|k, _| calls.iter().any(|c| &c.path == k));
        let st = json!({"ags": ags.iter().map(|(p, a)| json!({"path": p, "addr": a})).collect::<Vec<_>>(), "calls": jcalls});
        if self.last.as_ref() != Some(&st) {
            let mut o = st.clone();
            o["t"] = json!("tel");
            out(o);
            self.last = Some(st);
        }
        (ags, calls, transports)
    }
}

// ------------------------------------------------------------ PipeWire: the phone's Bluetooth card and nodes
fn pw() -> Vec<J> {
    let (_, o) = run(&["pw-dump"], 8.0);
    serde_json::from_str::<J>(&o).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

fn norm(a: &str) -> String {
    a.to_uppercase().replace('_', ":")
}

fn props(o: &J) -> &J {
    &o["info"]["props"]
}

fn card<'a>(dump: &'a [J], addr: &str) -> Option<&'a J> {
    dump.iter().find(|o| o["type"] == "PipeWire:Interface:Device" && norm(js(props(o), "api.bluez5.address")) == norm(addr))
}

fn nodes(dump: &[J], addr: &str) -> (String, String) {
    let (mut src, mut snk) = (String::new(), String::new());
    for o in dump {
        if o["type"] != "PipeWire:Interface:Node" {
            continue;
        }
        let p = props(o);
        if norm(js(p, "api.bluez5.address")) != norm(addr) {
            continue;
        }
        let mc = js(p, "media.class");
        if mc.contains("Source") && src.is_empty() {
            src = js(p, "node.name").to_string();
        } else if mc.contains("Sink") && snk.is_empty() {
            snk = js(p, "node.name").to_string();
        }
    }
    (src, snk)
}

fn node_id(dump: &[J], name: &str) -> Option<i64> {
    dump.iter().find(|o| o["type"] == "PipeWire:Interface:Node" && js(props(o), "node.name") == name).and_then(|o| o["id"].as_i64())
}

const TEMPLATE: &str = r#"# generated by Mist mistd calls: call audio of the phone (Bluetooth hands-free)
context.properties = { core.daemon = false  core.name = mist-call  log.level = 1 }
context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}
context.modules = [
    { name = libpipewire-module-rt  args = { }  flags = [ ifexists nofail ] }
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-echo-cancel
      args = {
        library.name   = aec/libspa-aec-webrtc
        audio.channels = 1
        audio.position = [ MONO ]
        capture.props  = { node.name = "mist-call.capture"  node.description = "Rozmowa: mikrofon (wejście)" }
        source.props   = { node.name = "mist-call.mic"  node.description = "Rozmowa telefoniczna: mikrofon"  priority.session = 0  priority.driver = 0 }
        sink.props     = { node.name = "mist-call.voice"  node.description = "Rozmowa telefoniczna: głos"  priority.session = 0  priority.driver = 0 }
        playback.props = { node.name = "mist-call.playback"  node.description = "Rozmowa: głośniki"  media.role = "Communication" }
      } }
    { name = libpipewire-module-loopback
      args = {
        node.description = "Rozmowa: telefon -> głośniki"
        audio.position = [ MONO ]
        capture.props  = { node.name = "mist-call.from-phone"  target.object = "@SRC@"  node.dont-reconnect = true  stream.dont-remix = true }
        playback.props = { node.name = "mist-call.from-phone.out"  target.object = "mist-call.voice"  node.dont-reconnect = true }
      } }
    { name = libpipewire-module-loopback
      args = {
        node.description = "Rozmowa: mikrofon -> telefon"
        audio.position = [ MONO ]
        capture.props  = { node.name = "mist-call.to-phone"  target.object = "mist-call.mic"  node.dont-reconnect = true }
        playback.props = { node.name = "mist-call.to-phone.out"  target.object = "@SNK@"  node.dont-reconnect = true }
      } }
]
"#;

struct Audio {
    proc_: Option<Child>, // the private PipeWire client while the call audio runs here
    pc: bool,             // route calls through the computer (toggle: "pc")
    mute: bool,
    err: String,
    prev: Option<(i64, Option<i64>)>, // (device id, profile index) before the call took the card
    due: Option<Instant>,             // retry while the phone's audio nodes are not there yet
    tries: u32,
    sent: Option<J>,
    conf: String,
}

impl Audio {
    fn running(&mut self) -> bool {
        self.proc_.as_mut().is_some_and(|p| matches!(p.try_wait(), Ok(None)))
    }

    fn report(&mut self) {
        let s = json!({"on": self.running(), "pc": self.pc, "mute": self.mute, "err": self.err});
        if self.sent.as_ref() != Some(&s) {
            let mut o = s.clone();
            o["t"] = json!("audio");
            out(o);
            self.sent = Some(s);
        }
    }

    fn take_card(&mut self, tel: &Tel, addr: &str, transports: &[String]) -> Vec<J> {
        let dump = pw();
        let Some(dev) = card(&dump, addr) else { return dump };
        let par = &dev["info"]["params"];
        let profs = par["EnumProfile"].as_array().cloned().unwrap_or_default();
        let cur = &par["Profile"][0];
        if !js(cur, "name").contains("gateway") {
            if let Some(gw) = profs.iter().find(|p| js(p, "name").contains("gateway") && js(p, "available") != "no") {
                let id = dev["id"].as_i64().unwrap_or(0);
                if self.prev.is_none() {
                    self.prev = Some((id, cur["index"].as_i64()));
                }
                run(&["wpctl", "set-profile", &id.to_string(), &gw["index"].as_i64().unwrap_or(0).to_string()], 6.0);
                for t in transports {
                    tel.bus_call(t, TRANSPORT, "Activate", None);
                }
                std::thread::sleep(Duration::from_millis(600));
                return pw();
            }
        }
        dump
    }

    fn start(&mut self, tel: &Tel, addr: &str, transports: &[String]) {
        if self.running() {
            return;
        }
        let dump = self.take_card(tel, addr, transports);
        let (src, snk) = nodes(&dump, addr);
        if src.is_empty() || snk.is_empty() {
            self.tries += 1;
            self.due = Some(Instant::now() + Duration::from_millis(700));
            if self.tries > 12 {
                self.err = "brak dźwięku z telefonu przez Bluetooth".into();
                self.due = None;
            }
            self.report();
            return;
        }
        let _ = std::fs::write(&self.conf, TEMPLATE.replace("@SRC@", &src).replace("@SNK@", &snk));
        self.proc_ = cmd(&["pipewire", "-c", &self.conf]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok();
        self.err.clear();
        self.tries = 0;
        self.due = None;
        if self.mute {
            std::thread::sleep(Duration::from_millis(400));
            self.set_mute(true);
        }
        self.report();
    }

    fn stop(&mut self) {
        if let Some(mut p) = self.proc_.take() {
            unsafe { libc::kill(p.id() as i32, libc::SIGTERM) };
            let end = Instant::now() + Duration::from_secs(2);
            while matches!(p.try_wait(), Ok(None)) && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = p.kill();
            let _ = p.wait();
        }
        if let Some((dev, idx)) = self.prev.take() {
            if let Some(i) = idx {
                run(&["wpctl", "set-profile", &dev.to_string(), &i.to_string()], 6.0);
            }
        }
        self.due = None;
        self.tries = 0;
        self.report();
    }

    fn set_mute(&mut self, on: bool) {
        self.mute = on;
        if let Some(id) = node_id(&pw(), "mist-call.mic") {
            run(&["wpctl", "set-mute", &id.to_string(), if on { "1" } else { "0" }], 6.0);
        }
        self.report();
    }
}

struct Ringer(Option<Child>);

impl Ringer {
    fn set(&mut self, on: bool) {
        let alive = self.0.as_mut().is_some_and(|p| matches!(p.try_wait(), Ok(None)));
        if on && !alive && std::path::Path::new(RING).exists() {
            // a short shell loop: the tone, a pause, again (its own process group, killed when answered)
            let script = format!("while :; do pw-play --media-role=Notification \"{}\"; sleep 1; done", RING);
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &script]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            unsafe {
                c.pre_exec(|| {
                    libc::setsid();
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            self.0 = c.spawn().ok();
        } else if !on {
            if let Some(mut p) = self.0.take() {
                unsafe { libc::kill(-(p.id() as i32), libc::SIGTERM) };
                let _ = p.wait();
            }
        }
    }
}

const UP: [&str; 4] = ["active", "dialing", "alerting", "held"];

fn sync(tel: &mut Tel, audio: &mut Audio, ringer: &mut Ringer) {
    let (ags, calls, transports) = tel.read();
    let up = calls.iter().any(|c| UP.contains(&c.state.as_str()));
    let ringing = calls.iter().any(|c| c.state == "incoming" || c.state == "waiting");
    if up && audio.pc && !ags.is_empty() {
        let addr = ags[0].1.clone();
        audio.start(tel, &addr, &transports);
    } else if !up && (audio.proc_.is_some() || audio.prev.is_some()) {
        audio.stop();
        audio.mute = false;
        audio.err.clear();
        audio.report();
    }
    ringer.set(ringing && !up);
}

fn pick(calls: &[CallObj], path: &str, states: &[&str]) -> String {
    if !path.is_empty() {
        return path.to_string();
    }
    calls.iter().find(|c| states.contains(&c.state.as_str())).map(|c| c.path.clone()).unwrap_or_default()
}

fn command(line: &str, tel: &mut Tel, audio: &mut Audio, ringer: &mut Ringer) {
    let Ok(o) = serde_json::from_str::<J>(line) else { return };
    let (c, a) = (js(&o, "c"), js(&o, "a"));
    let (ags, calls, _) = tel.read();
    match c {
        "answer" => {
            let p = pick(&calls, a, &["incoming", "waiting"]);
            if !p.is_empty() {
                ringer.set(false);
                tel.bus_call(&p, CALL, "Answer", None);
            }
        }
        "hangup" => {
            let p = pick(&calls, a, &["incoming", "waiting", "active", "dialing", "alerting", "held"]);
            if !p.is_empty() {
                tel.bus_call(&p, CALL, "Hangup", None);
            }
        }
        "hangupall" if !ags.is_empty() => tel.bus_call(&ags[0].0, AG, "HangupAll", None),
        "dial" if !a.is_empty() && !ags.is_empty() => tel.bus_call(&ags[0].0, AG, "Dial", Some(a)),
        "tones" if !a.is_empty() && !ags.is_empty() => tel.bus_call(&ags[0].0, AG, "SendTones", Some(a)),
        "mute" => {
            let m = !audio.mute;
            audio.set_mute(m);
        }
        "pc" => {
            audio.pc = !audio.pc;
            if audio.pc {
                audio.err.clear();
            } else {
                audio.stop(); // the card leaves the gateway profile: the phone takes the audio
            }
            audio.report();
        }
        _ => {}
    }
    sync(tel, audio, ringer);
}

enum Ev {
    Bus,
    Cmd(Option<String>),
}

pub fn main() {
    unsafe {
        libc::signal(libc::SIGTERM, on_term as *const () as libc::sighandler_t);
    }
    let conn = loop {
        match zbus::blocking::connection::Builder::session().and_then(|b| b.method_timeout(Duration::from_secs(25)).build()) {
            Ok(c) => break c,
            Err(_) => std::thread::sleep(Duration::from_secs(5)),
        }
    };
    let (tx, rx) = mpsc::channel::<Ev>();
    stdin_lines(tx.clone(), Ev::Cmd);
    {
        // signals of the service (and its restarts): one read after each burst
        let conn = conn.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            for rule in [
                format!("type='signal',sender='{}'", SVC),
                format!("type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{}'", SVC),
            ] {
                let _ = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(rule,));
            }
            for m in zbus::blocking::MessageIterator::from(&conn) {
                let Ok(m) = m else { continue };
                if m.message_type() != zbus::message::Type::Signal {
                    continue;
                }
                let h = m.header();
                let from_svc = h.sender().map(|s| s.as_str()).is_some_and(|s| s != "org.freedesktop.DBus");
                let noc = h.member().map(|x| x.as_str()) == Some("NameOwnerChanged");
                if (from_svc && h.path().is_some_and(|p| p.as_str().starts_with(ROOT))) || noc {
                    if tx.send(Ev::Bus).is_err() {
                        return;
                    }
                }
            }
        });
    }
    let mut tel = Tel { conn, since: HashMap::new(), last: None };
    let mut audio = Audio {
        proc_: None, pc: true, mute: false, err: String::new(), prev: None, due: None, tries: 0, sent: None,
        conf: format!("{}/mist-call.conf", runtime_dir()),
    };
    let mut ringer = Ringer(None);
    let mut due = Some(Instant::now());
    audio.report();
    loop {
        if TERM.load(Ordering::SeqCst) {
            audio.stop();
            ringer.set(false);
            return;
        }
        let mut wait = Duration::from_millis(500);
        for d in [due, audio.due].into_iter().flatten() {
            wait = wait.min(d.saturating_duration_since(Instant::now()));
        }
        match rx.recv_timeout(wait) {
            Ok(Ev::Cmd(None)) => {
                audio.stop();
                ringer.set(false);
                return;
            }
            Ok(Ev::Cmd(Some(l))) => command(&l, &mut tel, &mut audio, &mut ringer),
            Ok(Ev::Bus) => {
                if due.is_none() {
                    due = Some(Instant::now() + Duration::from_millis(100)); // a burst of signals: one read
                }
            }
            Err(_) => {}
        }
        let now = Instant::now();
        if due.is_some_and(|d| now >= d) {
            due = None;
            sync(&mut tel, &mut audio, &mut ringer);
        }
        if audio.due.is_some_and(|d| now >= d) {
            audio.due = None;
            sync(&mut tel, &mut audio, &mut ringer);
        }
    }
}
