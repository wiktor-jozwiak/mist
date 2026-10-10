// The phone (KDE Connect + Bluetooth hands-free), was Phone.qml: runs `mistd phone` and `mistd calls`
// (JSON lines both ways), keeps their state, follows calls (KDE Connect / ADB / Bluetooth, the same rules as
// before) and writes the whole state to $XDG_RUNTIME_DIR/mist/phone-state.json for the QML panels that are not
// native yet (Phone.qml is a bridge now: it reads that file and sends commands over `mist-ui-bin msg phone ...`).
// Events (connected, pairing, photo, call...) go to the island.
use crate::sys::Msg;
use calloop::channel::Sender;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    pub raw: serde_json::Map<String, Value>, // ok, devs, main, threads, convs, contacts, adb, hfp, btCalls, callAudio
    pub kc_call: Option<Value>,
    pub kc_at: Option<Instant>,
}

pub struct Phone {
    pub st: Arc<Mutex<State>>,
    phone_in: Arc<Mutex<Option<ChildStdin>>>,
    calls_in: Arc<Mutex<Option<ChildStdin>>>,
}

fn mistd() -> String {
    format!("{}/.config/quickshell/mist/scripts/bin/mistd", std::env::var("HOME").unwrap_or_default())
}
fn spawn_mode(mode: &str) -> Option<Child> {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(mistd());
    c.arg(mode).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    c.spawn().ok()
}

pub fn digits9(t: &str) -> String {
    let d: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    d[d.len().saturating_sub(9)..].to_string()
}

impl State {
    pub fn get(&self, k: &str) -> Value {
        self.raw.get(k).cloned().unwrap_or(Value::Null)
    }
    pub fn main(&self) -> Value {
        self.get("main")
    }
    pub fn online(&self) -> bool {
        let m = self.main();
        m["pair"].as_i64() == Some(3) && m["reach"].as_bool().unwrap_or(false)
    }
    pub fn name(&self) -> String {
        self.main()["name"].as_str().unwrap_or("").to_string()
    }
    pub fn battery(&self) -> i64 {
        if self.online() { self.main()["bat"].as_i64().unwrap_or(-1) } else { -1 }
    }
    pub fn notes(&self) -> Vec<Value> {
        if self.online() { self.main()["notes"].as_array().cloned().unwrap_or_default() } else { Vec::new() }
    }
    pub fn contact_name(&self, tel: &str) -> String {
        let d = digits9(tel);
        if d.len() < 9 {
            return String::new();
        }
        for c in self.get("contacts").as_array().cloned().unwrap_or_default() {
            if c["tels"].as_array().map(|t| t.iter().any(|x| digits9(x.as_str().unwrap_or("")) == d)).unwrap_or(false) {
                return c["name"].as_str().unwrap_or("").to_string();
            }
        }
        String::new()
    }
    // Phone.call: a Bluetooth call (by state order), else the one KDE Connect / ADB reports
    pub fn call(&self) -> Option<Value> {
        let order = ["incoming", "waiting", "active", "dialing", "alerting", "held"];
        let mut bt: Vec<Value> = self.get("btCalls").as_array().cloned().unwrap_or_default().into_iter().filter(|c| order.contains(&c["state"].as_str().unwrap_or(""))).collect();
        bt.sort_by_key(|c| order.iter().position(|o| *o == c["state"].as_str().unwrap_or("")).unwrap_or(9));
        if let Some(mut c) = bt.into_iter().next() {
            let num = c["number"].as_str().unwrap_or("").to_string();
            let who = [c["name"].as_str().unwrap_or("").to_string(), self.contact_name(&num), num.clone()].into_iter().find(|s| !s.is_empty()).unwrap_or("Nieznany numer".into());
            c["bt"] = json!(true);
            c["who"] = json!(who);
            return Some(c);
        }
        self.kc_call.clone()
    }
    pub fn ringing(&self) -> bool {
        self.call().map(|c| matches!(c["state"].as_str(), Some("incoming") | Some("waiting"))).unwrap_or(false)
    }
    pub fn adb_ok(&self) -> bool {
        self.online() && self.get("adb")["ok"].as_bool().unwrap_or(false)
    }

    // KDE Connect: ringing / talking / missedCall; ADB: every state change (Phone.kdeCall)
    fn kde_call(&mut self, o: &Value) {
        let s = |k: &str| o[k].as_str().unwrap_or("").to_string();
        if s("src") == "adb" {
            let st = match s("what").as_str() { "ringing" => "incoming", "talking" => "active", "dialing" => "alerting", "held" => "held", _ => "" };
            if st.is_empty() {
                self.kc_call = None;
                self.kc_at = None;
                return;
            }
            let keep = self.kc_call.clone().filter(|k| k["adb"].as_bool().unwrap_or(false) && (k["path"].as_str() == o["id"].as_str() || k["pending"].as_bool().unwrap_or(false)));
            let who = if !s("who").is_empty() && s("who") != "Nieznany numer" { s("who") } else if let Some(k) = &keep { k["who"].as_str().unwrap_or("").to_string() } else if s("who").is_empty() { "Nieznany numer".into() } else { s("who") };
            let number = if !s("number").is_empty() { s("number") } else { keep.as_ref().map(|k| k["number"].as_str().unwrap_or("").to_string()).unwrap_or_default() };
            self.kc_call = Some(json!({ "state": st, "bt": false, "adb": true, "path": s("id"), "since": o["since"].as_i64().unwrap_or(0), "who": who, "number": number }));
            self.kc_at = None;
            return;
        }
        if let Some(k) = &self.kc_call {
            if k["adb"].as_bool().unwrap_or(false) && !k["pending"].as_bool().unwrap_or(false) {
                if s("what") == "ringing" && k["who"].as_str() == Some("Nieznany numer") && !s("who").is_empty() {
                    let mut k = k.clone();
                    k["who"] = json!(s("who"));
                    if !s("number").is_empty() {
                        k["number"] = json!(s("number"));
                    }
                    self.kc_call = Some(k);
                }
                return;
            }
        }
        if s("what") == "ringing" {
            let who = [s("who"), s("number")].into_iter().find(|x| !x.is_empty()).unwrap_or("Nieznany numer".into());
            self.kc_call = Some(json!({ "state": "incoming", "bt": false, "who": who, "number": s("number"), "since": 0 }));
            self.kc_at = Some(Instant::now());
        } else {
            self.kc_call = None;
        }
    }
}

impl Phone {
    // test mode: no mistd processes (the running shell owns them)
    pub fn start_idle() -> Phone {
        Phone { st: Arc::new(Mutex::new(State::default())), phone_in: Arc::new(Mutex::new(None)), calls_in: Arc::new(Mutex::new(None)) }
    }
    pub fn start(tx: Sender<Msg>) -> Phone {
        let p = Phone { st: Arc::new(Mutex::new(State::default())), phone_in: Arc::new(Mutex::new(None)), calls_in: Arc::new(Mutex::new(None)) };
        for mode in ["phone", "calls"] {
            let st = p.st.clone();
            let input = if mode == "phone" { p.phone_in.clone() } else { p.calls_in.clone() };
            let tx = tx.clone();
            let _ = std::thread::Builder::new().name(format!("mistd-{mode}")).stack_size(256 * 1024).spawn(move || loop {
                let Some(mut ch) = spawn_mode(mode) else {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                };
                *input.lock().unwrap() = ch.stdin.take();
                for l in BufReader::new(ch.stdout.take().unwrap()).lines() {
                    let Ok(l) = l else { break };
                    let Ok(o) = serde_json::from_str::<Value>(&l) else { continue };
                    let mut ev = None;
                    {
                        let mut s = st.lock().unwrap();
                        match o["t"].as_str().unwrap_or("") {
                            "state" => {
                                s.raw.insert("ok".into(), o["ok"].clone());
                                s.raw.insert("devs".into(), o.get("devs").cloned().unwrap_or(json!([])));
                                s.raw.insert("main".into(), o.get("main").cloned().unwrap_or(Value::Null));
                            }
                            "sms" => { s.raw.insert("threads".into(), o.get("threads").cloned().unwrap_or(json!([]))); }
                            "conv" => {
                                let th = o["thread"].to_string();
                                let mut c = s.raw.get("convs").cloned().unwrap_or(json!({}));
                                c[th.trim_matches('"')] = o.get("msgs").cloned().unwrap_or(json!([]));
                                s.raw.insert("convs".into(), c);
                            }
                            "contacts" => { s.raw.insert("contacts".into(), o.get("list").cloned().unwrap_or(json!([]))); }
                            "adb" => { s.raw.insert("adb".into(), o.clone()); }
                            "ev" => {
                                if o["k"].as_str() == Some("call") {
                                    s.kde_call(&o);
                                }
                                ev = Some(o.clone());
                            }
                            "tel" => {
                                s.raw.insert("hfp".into(), json!(o["ags"].as_array().map(|a| !a.is_empty()).unwrap_or(false)));
                                s.raw.insert("btCalls".into(), o.get("calls").cloned().unwrap_or(json!([])));
                            }
                            "audio" => { s.raw.insert("callAudio".into(), o.clone()); }
                            _ => {}
                        }
                        write_state(&s);
                    }
                    let _ = tx.send(Msg::Phone2(ev));
                }
                let _ = ch.wait();
                *input.lock().unwrap() = None;
                std::thread::sleep(Duration::from_secs(5));
            });
        }
        p
    }

    // a KDE Connect call that never ended (KDE Connect does not report the end): drop it after 45 s
    pub fn tick(&self) -> bool {
        let mut s = self.st.lock().unwrap();
        if s.kc_at.map(|t| t.elapsed().as_secs() >= 45).unwrap_or(false) {
            let drop = s.kc_call.as_ref().map(|k| !k["adb"].as_bool().unwrap_or(false) || k["pending"].as_bool().unwrap_or(false)).unwrap_or(false);
            s.kc_at = None;
            if drop {
                s.kc_call = None;
                write_state(&s);
                return true;
            }
        }
        false
    }

    pub fn send(&self, c: &str, a: Option<Value>) {
        let line = match a { Some(a) => json!({ "c": c, "a": a }), None => json!({ "c": c }) }.to_string();
        if let Some(i) = self.phone_in.lock().unwrap().as_mut() {
            let _ = writeln!(i, "{line}");
        }
    }
    pub fn tel(&self, c: &str, a: Option<Value>) {
        let line = match a { Some(a) => json!({ "c": c, "a": a }), None => json!({ "c": c }) }.to_string();
        if let Some(i) = self.calls_in.lock().unwrap().as_mut() {
            let _ = writeln!(i, "{line}");
        }
    }
    // the call actions (Phone.answer / hangup / callNumber)
    pub fn answer(&self) {
        let c = self.st.lock().unwrap().call();
        if let Some(c) = c.filter(|c| c["bt"].as_bool().unwrap_or(false)) {
            self.tel("answer", Some(c["path"].clone()));
        } else {
            self.send("answer", None);
            let mut s = self.st.lock().unwrap();
            if !s.kc_call.as_ref().map(|k| k["adb"].as_bool().unwrap_or(false)).unwrap_or(false) {
                s.kc_call = None;
                write_state(&s);
            }
        }
    }
    pub fn hangup(&self) {
        let c = self.st.lock().unwrap().call();
        if let Some(c) = c.filter(|c| c["bt"].as_bool().unwrap_or(false)) {
            self.tel("hangup", Some(c["path"].clone()));
        } else {
            self.send("hangup", None);
            let mut s = self.st.lock().unwrap();
            let keep = s.kc_call.as_ref().map(|k| k["adb"].as_bool().unwrap_or(false) && !k["pending"].as_bool().unwrap_or(false)).unwrap_or(false);
            if !keep {
                s.kc_call = None;
                write_state(&s);
            }
        }
    }
    pub fn call_number(&self, n: &str) {
        let (hfp, adb_ok, has_call, who) = {
            let s = self.st.lock().unwrap();
            (s.get("hfp").as_bool().unwrap_or(false), s.adb_ok(), s.call().is_some(), s.contact_name(n))
        };
        if hfp {
            let d: String = n.chars().filter(|c| c.is_ascii_digit() || "+*#".contains(*c)).collect();
            self.tel("dial", Some(json!(d)));
            return;
        }
        self.send("call", Some(json!(n)));
        if adb_ok && !has_call {
            let mut s = self.st.lock().unwrap();
            s.kc_call = Some(json!({ "state": "alerting", "bt": false, "adb": true, "pending": true, "path": "", "since": 0, "who": if who.is_empty() { n.to_string() } else { who }, "number": n }));
            s.kc_at = Some(Instant::now());
            write_state(&s);
        }
    }
}

pub fn write_state(s: &State) {
    let mut o = Value::Object(s.raw.clone());
    o["kcCall"] = s.kc_call.clone().unwrap_or(Value::Null);
    let p = format!("{}/mist/phone-state.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let tmp = format!("{p}.tmp");
    if std::fs::write(&tmp, o.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
    // the small file the bar reads
    let bar = json!({ "online": s.online(), "pair": s.main()["pair"].as_i64() == Some(2), "battery": s.battery(), "charging": s.online() && s.main()["chg"].as_bool().unwrap_or(false) });
    let pb = format!("{}/mist/phone.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let _ = std::fs::write(format!("{pb}.tmp"), bar.to_string());
    let _ = std::fs::rename(format!("{pb}.tmp"), &pb);
}
