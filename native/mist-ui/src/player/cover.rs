// Album art for the audio view: cover.jpg / folder.jpg & co. next to the file, else the picture embedded in
// it (one short `ffmpeg`, CPU, no GL). Found on a worker thread once per track; three colours taken from it
// tint the backdrop. ≤ 600 px on the long side (≤ 1.4 MB), dropped with the track.
use crate::gfx::Rgba;
use std::process::{Command, Stdio};
use tiny_skia::Pixmap;

const MAX: u32 = 600;
const NAMES: [&str; 6] = ["cover", "folder", "front", "album", "albumart", "albumartsmall"];
const EXTS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

// the picture for `path` (an audio file), or None
pub fn find(path: &str) -> Option<Pixmap> {
    if let Some(f) = beside(path)
        && let Some(pm) = crate::img::decode_file(&f, MAX)
    {
        return Some(pm);
    }
    embedded(path)
}

// a cover file in the track's folder (any case)
fn beside(path: &str) -> Option<String> {
    let dir = std::path::Path::new(path).parent()?;
    let mut best: Option<(usize, String)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten().take(400) {
        let n = e.file_name().to_string_lossy().to_lowercase();
        let Some((stem, ext)) = n.rsplit_once('.') else { continue };
        if !EXTS.contains(&ext) {
            continue;
        }
        if let Some(rank) = NAMES.iter().position(|k| *k == stem)
            && best.as_ref().is_none_or(|b| rank < b.0)
        {
            best = Some((rank, e.path().to_string_lossy().into_owned()));
        }
    }
    best.map(|b| b.1)
}

fn embedded(path: &str) -> Option<Pixmap> {
    let vf = format!("scale='min(iw,{MAX})':'min(ih,{MAX})':force_original_aspect_ratio=decrease");
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i", path, "-map", "0:v:0", "-frames:v", "1", "-an", "-sn", "-vf", &vf, "-pix_fmt", "rgba", "-f", "image2pipe", "-c:v", "png", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.stdout.is_empty() {
        return None;
    }
    let im = image::load_from_memory_with_format(&out.stdout, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = im.dimensions();
    let mut data = im.into_raw();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (px[0] as u32 * a / 255) as u8;
        px[1] = (px[1] as u32 * a / 255) as u8;
        px[2] = (px[2] as u32 * a / 255) as u8;
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

// three backdrop colours: the average of the top left, top right and bottom of the picture, made vivid
// enough to read as colour and dark enough for white text on top
pub fn palette(pm: &Pixmap) -> [Rgba; 3] {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let avg = |x0: usize, y0: usize, x1: usize, y1: usize| {
        let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
        for y in (y0..y1).step_by(4) {
            for x in (x0..x1).step_by(4) {
                let p = pm.pixels()[y * w + x].demultiply();
                r += p.red() as u64;
                g += p.green() as u64;
                b += p.blue() as u64;
                n += 1;
            }
        }
        let n = n.max(1) as f32 * 255.0;
        vivid(r as f32 / n, g as f32 / n, b as f32 / n)
    };
    [avg(0, 0, w / 2, h / 2), avg(w / 2, 0, w, h / 2), avg(0, h / 2, w, h)]
}

fn vivid(r: f32, g: f32, b: f32) -> Rgba {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) / 2.0;
    // grey pictures stay grey-blue; colour gets pushed out from the grey, the light kept at mid
    let grey = (r + g + b) / 3.0;
    let push = if mx - mn < 0.06 { 1.0 } else { 1.6 };
    let k = 0.55 / l.max(0.2);
    let f = |c: f32| (((c - grey) * push + grey) * k).clamp(0.0, 1.0);
    if mx - mn < 0.06 { Rgba(f(r) * 0.8, f(g) * 0.85, f(b), 1.0) } else { Rgba(f(r), f(g), f(b), 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cover_beside() {
        let d = std::env::temp_dir().join(format!("mist-cover-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let t = d.join("01.flac");
        std::fs::write(&t, b"").unwrap();
        assert!(beside(&t.to_string_lossy()).is_none());
        std::fs::write(d.join("Folder.JPG"), b"").unwrap();
        std::fs::write(d.join("back.jpg"), b"").unwrap();
        assert!(beside(&t.to_string_lossy()).unwrap().ends_with("Folder.JPG"));
        std::fs::write(d.join("cover.png"), b"").unwrap();
        assert!(beside(&t.to_string_lossy()).unwrap().ends_with("cover.png"));
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn colours() {
        let mut pm = Pixmap::new(8, 8).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(200, 40, 40, 255));
        let p = palette(&pm);
        assert!(p[0].0 > p[0].1 && p[0].0 > p[0].2, "red stays red");
        assert!(p[2].0 <= 1.0 && (p[2].0 + p[2].1 + p[2].2) / 3.0 < 0.6, "dark enough for white text");
    }
}
