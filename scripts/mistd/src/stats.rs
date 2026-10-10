// mistd stats: live CPU %, RAM and network speed for the bar, sampled every 2 s from /proc. One line
// "cpu% memUsedGiB memTotalGiB rxBytes/s txBytes/s" only when a value as the bar shows it changed, so
// QML wakes up and re-renders only for real changes (was: bash + JS parsing all of /proc/meminfo every 2 s).
use std::io::Write;
use std::time::{Duration, Instant};

fn cpu() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let v: Vec<u64> = s.lines().next()?.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return None;
    }
    Some((v.iter().sum(), v[3] + v[4]))
}

fn mem() -> (f64, f64) {
    let s = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let (mut tot, mut avail) = (0.0, 0.0);
    for l in s.lines() {
        let mut it = l.split_whitespace();
        match (it.next(), it.next().and_then(|v| v.parse::<f64>().ok())) {
            (Some("MemTotal:"), Some(v)) => tot = v,
            (Some("MemAvailable:"), Some(v)) => avail = v,
            _ => {}
        }
    }
    (tot - avail, tot)
}

fn net() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
    let (mut rx, mut tx) = (0, 0);
    for l in s.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else { continue };
        let n = name.trim();
        if n == "lo" || n.starts_with("docker") || n.starts_with("veth") || n.starts_with("br-") {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if f.len() > 8 {
            rx += f[0];
            tx += f[8];
        }
    }
    (rx, tx)
}

// the precision the bar prints (Data.bytes: B, 0.1 KB, 0.1 MB)
fn round_rate(b: f64) -> u64 {
    if b < 1024.0 {
        b.round() as u64
    } else if b < 1048576.0 {
        ((b / 102.4).round() * 102.4) as u64
    } else {
        ((b / 104857.6).round() * 104857.6) as u64
    }
}

pub fn main() {
    let mut pc = cpu();
    let mut pn = (net(), Instant::now());
    let mut last = String::new();
    let out = std::io::stdout();
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let c = cpu();
        let pct = match (pc, c) {
            (Some((t0, i0)), Some((t1, i1))) if t1 > t0 => (100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64)).round() as i64,
            _ => 0,
        };
        pc = c;
        let (used, tot) = mem();
        let (n, t) = (net(), Instant::now());
        let dt = t.duration_since(pn.1).as_secs_f64().max(0.1);
        let rx = round_rate(n.0.saturating_sub(pn.0 .0) as f64 / dt);
        let tx = round_rate(n.1.saturating_sub(pn.0 .1) as f64 / dt);
        pn = (n, t);
        let line = format!("{} {:.1} {:.1} {} {}", pct, used / 1048576.0, tot / 1048576.0, rx, tx);
        if line != last {
            let mut o = out.lock();
            if writeln!(o, "{}", line).and_then(|_| o.flush()).is_err() {
                return;
            }
            last = line;
        }
    }
}
