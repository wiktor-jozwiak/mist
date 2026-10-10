// Shared plumbing: JSON lines on stdout, children that die with us, external commands with a timeout,
// D-Bus values as JSON (the shape `busctl --json=short` used to give the old Python scripts).
use serde_json::{Map, Value as J, json};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zbus::zvariant::Value;

static OUT: Mutex<()> = Mutex::new(());

/// One JSON object per line, flushed (stdout is a pipe to Quickshell).
pub fn out(v: J) {
    let _g = OUT.lock().unwrap();
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{}", v);
    let _ = o.flush();
}

/// A raw line (notify's TSV).
pub fn out_line(s: &str) {
    let _g = OUT.lock().unwrap();
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{}", s);
    let _ = o.flush();
}

/// This process gets SIGTERM when Quickshell (the parent) goes away.
pub fn pdeath() {
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) };
}

pub fn ignore_sigpipe_default() {
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
}

/// A command whose process dies with us (PR_SET_PDEATHSIG in the child).
pub fn cmd(argv: &[&str]) -> Command {
    let mut c = Command::new(argv[0]);
    c.args(&argv[1..]);
    unsafe {
        c.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    c
}

/// A long-running child with its stdout piped.
pub fn spawn(argv: &[&str]) -> Option<Child> {
    cmd(argv).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()
}

/// Run to completion: (exit code, stdout). -1 on spawn failure or timeout (the child is killed).
pub fn run_bytes(argv: &[&str], timeout: f64, input: Option<&[u8]>) -> (i32, Vec<u8>) {
    let mut c = cmd(argv);
    c.stdout(Stdio::piped()).stderr(Stdio::null());
    c.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let Ok(mut ch) = c.spawn() else { return (-1, vec![]) };
    if let (Some(data), Some(mut si)) = (input, ch.stdin.take()) {
        let d = data.to_vec();
        std::thread::spawn(move || {
            let _ = si.write_all(&d);
        });
    }
    let mut so = ch.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let end = Instant::now() + Duration::from_secs_f64(timeout);
    loop {
        match ch.try_wait() {
            Ok(Some(st)) => return (st.code().unwrap_or(-1), reader.join().unwrap_or_default()),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = ch.kill();
                let _ = ch.wait();
                return (-1, reader.join().unwrap_or_default());
            }
        }
    }
}

pub fn run(argv: &[&str], timeout: f64) -> (i32, String) {
    let (rc, b) = run_bytes(argv, timeout, None);
    (rc, String::from_utf8_lossy(&b).into_owned())
}

pub fn which(name: &str) -> Option<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').map(|d| format!("{}/{}", d, name)).find(|p| {
        std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
    })
}

pub fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
}

pub fn runtime_dir() -> String {
    std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into())
}

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Whitespace collapsed, at most n characters.
pub fn clean(s: &str, n: usize) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(n).collect()
}

pub fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// A D-Bus value as plain JSON (variants unwrapped, structs as arrays, dicts as objects).
pub fn val(v: &Value) -> J {
    match v {
        Value::U8(x) => json!(x),
        Value::Bool(x) => json!(x),
        Value::I16(x) => json!(x),
        Value::U16(x) => json!(x),
        Value::I32(x) => json!(x),
        Value::U32(x) => json!(x),
        Value::I64(x) => json!(x),
        Value::U64(x) => json!(x),
        Value::F64(x) => json!(x),
        Value::Str(s) => json!(s.as_str()),
        Value::Signature(s) => json!(s.to_string()),
        Value::ObjectPath(p) => json!(p.as_str()),
        Value::Value(b) => val(b),
        Value::Array(a) => J::Array(a.inner().iter().map(val).collect()),
        Value::Dict(d) => {
            let mut m = Map::new();
            for (k, v) in d.iter() {
                let key = match val(k) {
                    J::String(s) => s,
                    o => o.to_string(),
                };
                m.insert(key, val(v));
            }
            J::Object(m)
        }
        Value::Structure(s) => J::Array(s.fields().iter().map(val).collect()),
        #[allow(unreachable_patterns)]
        _ => J::Null,
    }
}

/// The arguments of a message body as a JSON array.
pub fn body_args(m: &zbus::Message) -> Vec<J> {
    let b = m.body();
    if b.signature().to_string().is_empty() {
        return vec![];
    }
    match b.deserialize::<zbus::zvariant::Structure>() {
        Ok(s) => s.fields().iter().map(val).collect(),
        Err(_) => vec![],
    }
}

pub fn js<'a>(v: &'a J, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

/// A thread reading stdin lines into a channel; None on EOF. A line longer than 1 MiB is dropped (its bytes are
/// skipped up to the next newline), broken UTF-8 is replaced: no input can grow memory or end the reader early.
pub fn stdin_lines<T: Send + 'static>(tx: std::sync::mpsc::Sender<T>, wrap: fn(Option<String>) -> T) {
    const MAX: usize = 1 << 20;
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut line: Vec<u8> = Vec::new();
        let mut skipping = false;
        loop {
            let buf = match std::io::BufRead::fill_buf(&mut stdin) {
                Ok(b) if !b.is_empty() => b,
                Ok(_) => {
                    let _ = tx.send(wrap(None));
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = tx.send(wrap(None));
                    return;
                }
            };
            let (chunk, nl) = match buf.iter().position(|&c| c == b'\n') {
                Some(i) => (&buf[..i], Some(i + 1)),
                None => (buf, None),
            };
            if !skipping {
                if line.len() + chunk.len() > MAX {
                    skipping = true;
                    line.clear();
                } else {
                    line.extend_from_slice(chunk);
                }
            }
            let used = nl.unwrap_or(buf.len());
            std::io::BufRead::consume(&mut stdin, used);
            if nl.is_some() {
                if !skipping && tx.send(wrap(Some(String::from_utf8_lossy(&line).into_owned()))).is_err() {
                    return;
                }
                line.clear();
                skipping = false;
            }
        }
    });
}
