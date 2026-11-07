// Seek-bar thumbnails: one frame at the hovered time, decoded by a short `ffmpeg` (CPU, keyframes only, no GL)
// on a worker thread. One job at a time; the spot the pointer moved to while it ran goes next. Small RGBA
// pixmaps kept per bucket of the file (≤ 64 × 160×90 ≈ 3.7 MB), dropped with the file. No ffmpeg: no thumbnails.
use super::engine::Tx;
use super::Wake;
use std::collections::HashMap;
use std::process::{Command, Stdio};

pub const TW: u32 = 160;
pub const TH: u32 = 90;
const KEEP: usize = 64;

pub struct Thumbs {
    path: String,
    step: f64,
    cache: HashMap<u32, tiny_skia::Pixmap>,
    order: Vec<u32>,
    busy: Option<u32>,
    want: Option<u32>,
    pub off: bool,
    pub made: u64,
}

// the bucket of time `t` in a file of `dur` seconds: ~100 per file, at least 2 s apart
pub fn bucket(t: f64, dur: f64) -> (u32, f64) {
    let step = (dur / 100.0).max(2.0);
    ((t.max(0.0) / step) as u32, step)
}

impl Thumbs {
    pub fn new() -> Thumbs {
        Thumbs { path: String::new(), step: 2.0, cache: HashMap::new(), order: vec![], busy: None, want: None, off: false, made: 0 }
    }
    // a new file (or none): everything goes
    pub fn reset(&mut self, path: &str) {
        if self.path != path {
            self.path = path.to_string();
            self.cache.clear();
            self.order.clear();
            self.want = None;
        }
    }
    pub fn bytes(&self) -> usize {
        self.cache.len() * (TW * TH * 4) as usize
    }
    // the thumbnail for time `t`, or None (asked for then: it comes back as Wake::Thumb)
    pub fn get(&mut self, t: f64, dur: f64, tx: Option<&Tx>) -> Option<&tiny_skia::Pixmap> {
        if self.off || self.path.is_empty() || dur <= 0.0 {
            return None;
        }
        let (b, step) = bucket(t, dur);
        self.step = step;
        if self.cache.contains_key(&b) {
            return self.cache.get(&b);
        }
        if self.busy.is_none() {
            if let Some(tx) = tx {
                self.start(b, tx);
            }
        } else if self.busy != Some(b) {
            self.want = Some(b);
        }
        // the nearest one already made stands in meanwhile
        let near = self.cache.keys().min_by_key(|k| k.abs_diff(b)).copied().filter(|k| k.abs_diff(b) <= 3)?;
        self.cache.get(&near)
    }
    fn start(&mut self, b: u32, tx: &Tx) {
        self.busy = Some(b);
        let (path, at, tx) = (self.path.clone(), (b as f64 + 0.5) * self.step, tx.clone());
        std::thread::spawn(move || {
            let px = grab(&path, at);
            let _ = tx.send(Box::new(Wake::Thumb(path, b, px)));
        });
    }
    // a job came back: keep it, start the next wanted spot
    pub fn done(&mut self, path: &str, b: u32, px: Option<Vec<u8>>, tx: Option<&Tx>) -> bool {
        self.busy = None;
        if path != self.path {
            return false;
        }
        let ok = match px {
            Some(px) => {
                if let Some(pm) = to_pixmap(&px) {
                    self.cache.insert(b, pm);
                    self.order.push(b);
                    if self.order.len() > KEEP {
                        let old = self.order.remove(0);
                        self.cache.remove(&old);
                    }
                    self.made += 1;
                }
                true
            }
            None => {
                // no ffmpeg (or it cannot read the file): stop asking
                self.off = true;
                false
            }
        };
        if let (Some(w), Some(tx)) = (self.want.take(), tx)
            && !self.cache.contains_key(&w)
            && !self.off
        {
            self.start(w, tx);
        }
        ok
    }
    // a ready thumbnail (the offline --png demo)
    pub fn insert(&mut self, b: u32, pm: tiny_skia::Pixmap) {
        self.cache.insert(b, pm);
        self.order.push(b);
    }
}

// straight RGBA from ffmpeg -> premultiplied pixmap
fn to_pixmap(px: &[u8]) -> Option<tiny_skia::Pixmap> {
    if px.len() != (TW * TH * 4) as usize {
        return None;
    }
    let mut pm = tiny_skia::Pixmap::new(TW, TH)?;
    for (d, s) in pm.pixels_mut().iter_mut().zip(px.chunks_exact(4)) {
        *d = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], 255).premultiply();
    }
    Some(pm)
}

// one frame near `at` s, letterboxed into TW×TH (the keyframe at or after the spot: fast, no full decode)
fn grab(path: &str, at: f64) -> Option<Vec<u8>> {
    let vf = format!("scale={TW}:{TH}:force_original_aspect_ratio=decrease,pad={TW}:{TH}:(ow-iw)/2:(oh-ih)/2");
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-threads", "2", "-skip_frame", "nokey", "-ss", &format!("{at:.2}"), "-i", path, "-frames:v", "1", "-an", "-sn", "-dn", "-vf", &vf, "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    // past the last keyframe: the frame before the end
    if out.stdout.len() == (TW * TH * 4) as usize {
        Some(out.stdout)
    } else if out.status.success() {
        Some(vec![])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buckets() {
        assert_eq!(bucket(0.0, 7200.0), (0, 72.0));
        assert_eq!(bucket(150.0, 7200.0), (2, 72.0));
        // short files: 2 s apart
        assert_eq!(bucket(5.0, 60.0), (2, 2.0));
        assert_eq!(bucket(-1.0, 60.0).0, 0);
    }
    #[test]
    fn rgba_to_pixmap() {
        assert!(to_pixmap(&[0; 16]).is_none());
        let px = vec![200u8; (TW * TH * 4) as usize];
        let pm = to_pixmap(&px).unwrap();
        assert_eq!(pm.pixel(5, 5).unwrap().red(), 200);
        assert_eq!(pm.pixel(5, 5).unwrap().alpha(), 255);
    }
}
