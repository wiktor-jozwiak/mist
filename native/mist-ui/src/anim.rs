// Tweened values for mist-ui (QML Behaviors: NumberAnimation with a duration and an easing curve).
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
pub enum Ease {
    Linear,
    OutCubic,
}

#[derive(Clone, Copy)]
pub struct Anim {
    from: f32,
    to: f32,
    start: Instant,
    dur: f32, // ms
    ease: Ease,
}

impl Anim {
    pub fn new(v: f32) -> Anim {
        Anim { from: v, to: v, start: Instant::now(), dur: 0.0, ease: Ease::Linear }
    }
    pub fn value(&self) -> f32 {
        if self.dur <= 0.0 {
            return self.to;
        }
        let t = (self.start.elapsed().as_secs_f32() * 1000.0 / self.dur).min(1.0);
        let k = match self.ease {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
        };
        self.from + (self.to - self.from) * k
    }
    pub fn running(&self) -> bool {
        self.dur > 0.0 && self.start.elapsed().as_secs_f32() * 1000.0 < self.dur
    }
    pub fn target(&self) -> f32 {
        self.to
    }
    // go to `to` from wherever the value is now (a Behavior)
    pub fn set(&mut self, to: f32, dur: f32, ease: Ease) {
        if (to - self.to).abs() < 1e-6 {
            return;
        }
        self.from = self.value();
        self.to = to;
        self.start = Instant::now();
        self.dur = dur;
        self.ease = ease;
    }
}
