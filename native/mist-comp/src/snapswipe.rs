//! A 4-finger touchpad swipe snaps the focused window, like Super+arrows (snap.rs): left / right a half of the
//! screen, up maximizes, down restores (or minimizes). One snap per swipe, on release, along the axis the
//! fingers moved most. Off while 4 fingers switch workspaces (`swipe 4`).

use std::cell::Cell;

pub const FINGERS: u32 = 4;
/// how far (touchpad units, about logical px) the fingers go before the release snaps
const MIN: f64 = 60.0;

thread_local!(static S: Cell<Option<(f64, f64)>> = const { Cell::new(None) });

pub fn wants(fingers: u32, swipe_fingers: u32) -> bool {
    fingers == FINGERS && swipe_fingers != FINGERS
}
pub fn active() -> bool {
    S.get().is_some()
}
pub fn begin() {
    S.set(Some((0.0, 0.0)));
}
pub fn update(dx: f64, dy: f64) {
    if let Some((x, y)) = S.get() {
        S.set(Some((x + dx, y + dy)));
    }
}
/// The key the swipe stands for (None: too short or cancelled).
pub fn end(cancelled: bool) -> Option<crate::snap::Key> {
    let (x, y) = S.take()?;
    if cancelled || x.abs().max(y.abs()) < MIN {
        return None;
    }
    use crate::snap::Key;
    Some(if x.abs() >= y.abs() { if x < 0.0 { Key::Left } else { Key::Right } } else if y < 0.0 { Key::Up } else { Key::Down })
}
