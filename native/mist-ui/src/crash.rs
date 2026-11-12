// A panic leaves a trace the user can find and the shell can show. Every Mist binary is built with
// panic = "abort": the process dies, the guard (~/.local/bin/mist-ui) or the session restarts it, and until
// now the only sign was a line somewhere in a log. The hook appends to ~/.cache/mist/crash.log (time, binary,
// version, thread, place, message) and leaves the last one in $XDG_RUNTIME_DIR/mist/crash, which the island
// shows once after the restart. Shared by path with mist-wall, mist-comp and mistd; std only.
use std::io::Write;

fn stamp() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // civil date from days since 1970 (Howard Hinnant's algorithm), UTC
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn last_path() -> String {
    format!("{}/mist/crash", std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()))
}

pub fn log_path() -> String {
    format!("{}/.cache/mist/crash.log", std::env::var("HOME").unwrap_or_default())
}

// one line: `thread 'stats' at mist-ui/src/sys.rs:560:13: index out of bounds`
fn describe(thread: &str, place: &str, msg: &str) -> String {
    let msg: String = msg.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("thread '{thread}' at {place}: {}", msg.chars().take(300).collect::<String>())
}

pub fn install(binary: &'static str) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let line = describe(std::thread::current().name().unwrap_or("?"), &place, &msg);
        let log = log_path();
        if let Some(dir) = std::path::Path::new(&log).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = writeln!(f, "{} {binary} {} {line}", stamp(), env!("CARGO_PKG_VERSION"));
        }
        let last = last_path();
        if let Some(dir) = std::path::Path::new(&last).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&last, format!("{binary}: {line}"));
        prev(info);
    }));
    // tests: `MIST_PANIC_TEST=1 mistd` must leave both files and die
    if std::env::var_os("MIST_PANIC_TEST").is_some() {
        panic!("MIST_PANIC_TEST");
    }
}

// the crash left by the previous run, once
#[allow(dead_code)]
pub fn take() -> Option<String> {
    let p = last_path();
    let s = std::fs::read_to_string(&p).ok()?;
    let _ = std::fs::remove_file(&p);
    Some(s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_and_stamp() {
        assert_eq!(describe("stats", "a.rs:1:2", "index out\n  of bounds"), "thread 'stats' at a.rs:1:2: index out of bounds");
        let s = stamp();
        assert!(s.len() == 23 && s.ends_with(" UTC") && s.starts_with("20"), "{s}");
    }
}
