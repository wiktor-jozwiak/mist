//! IPC `prof`: where the compositor's time goes on a live session, per stage of a frame and per loop turn
//! (count, average and total since the last `prof`). A clock read per stage; nothing else.
use std::{cell::RefCell, time::Duration};

pub const STAGES: [&str; 7] = ["scene", "render", "queue", "frames", "turn", "sync", "clients"];

thread_local!(static T: RefCell<[(u64, Duration); 7]> = const { RefCell::new([(0, Duration::ZERO); 7]) });

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
