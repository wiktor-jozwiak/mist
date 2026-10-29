//! IPC `prof`: where the compositor's time goes on a live session, per stage of a frame and per loop turn
//! (count, average and total since the last `prof`). A clock read per stage; nothing else.
use std::{cell::RefCell, time::Duration};

pub const STAGES: [&str; 8] = ["scene", "render", "queue", "frames", "turn", "sync", "clients", "blur"];

thread_local!(static T: RefCell<[(u64, Duration); 8]> = const { RefCell::new([(0, Duration::ZERO); 8]) });

pub fn add(stage: usize, d: Duration) {
    T.with(|t| {
        let mut t = t.borrow_mut();
        t[stage].0 += 1;
        t[stage].1 += d;
    });
}

/// The report, and start over.
pub fn take() -> String {
    T.with(|t| {
        let t = std::mem::take(&mut *t.borrow_mut());
        t.iter()
            .zip(STAGES)
            .map(|((n, d), name)| format!("{name:8} {n:8} x {:7.1} us = {:8.1} ms\n", if *n > 0 { d.as_secs_f64() * 1e6 / *n as f64 } else { 0.0 }, d.as_secs_f64() * 1e3))
            .collect()
    })
}

/// IPC `frametime`: per output, the cost of each frame that reached the screen (scene + render + queue)
/// as p50 / p99 / max over the last 1024, and the late ones: a vblank more than 1.5 refresh periods after
/// the one before while a frame waited for it (continuous drawing that missed a refresh). Since the last call.
#[derive(Default)]
struct Out {
    cost_us: std::collections::VecDeque<u32>,
    frames: u64,
    late: u64,
    last: Option<std::time::Instant>,
}

thread_local!(static F: RefCell<std::collections::BTreeMap<String, Out>> = const { RefCell::new(std::collections::BTreeMap::new()) });

pub fn frame(output: &str, cost: Duration) {
    F.with(|f| {
        let mut f = f.borrow_mut();
        let o = f.entry(output.to_string()).or_default();
        if o.cost_us.len() == 1024 {
            o.cost_us.pop_front();
        }
        o.cost_us.push_back(cost.as_micros().min(u32::MAX as u128) as u32);
        o.frames += 1;
    });
}

/// A frame of `output` is on screen; `waited`: another frame was asked for while it scanned out.
pub fn vblank(output: &str, period: Option<Duration>, waited: bool) {
    let now = std::time::Instant::now();
    F.with(|f| {
        let mut f = f.borrow_mut();
        let o = f.entry(output.to_string()).or_default();
        if let (Some(last), Some(p), true) = (o.last, period, waited) {
            if now - last > p * 3 / 2 {
                o.late += 1;
            }
        }
        o.last = Some(now);
    });
}

pub fn frametime() -> String {
    F.with(|f| {
        let mut f = f.borrow_mut();
        let s = f
            .iter()
            .map(|(name, o)| {
                let mut v: Vec<u32> = o.cost_us.iter().copied().collect();
                v.sort_unstable();
                let q = |p: usize| v.get((v.len() * p / 100).min(v.len().saturating_sub(1))).copied().unwrap_or(0);
                format!("{name} frames {} p50 {} us p99 {} us max {} us late {}\n", o.frames, q(50), q(99), v.last().copied().unwrap_or(0), o.late)
            })
            .collect();
        for o in f.values_mut() {
            *o = Out { last: o.last, ..Default::default() };
        }
        s
    })
}
