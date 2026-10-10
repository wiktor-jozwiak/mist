// Small one-shot tools: clipboard history (store / list) and the WebP -> PNG mirror of notification images.
use crate::common::*;
use serde_json::{Value as J, json};
use std::io::Read;

fn clip_dir() -> String {
    let c = std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", home()));
    format!("{}/mist/clip", c)
}

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)[..12].to_string()
}

fn image_ext(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if b.starts_with(b"\xff\xd8\xff") {
        Some("jpeg")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else if b.len() > 12 && b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

/// Mist clipboard history, run by `wl-paste --watch` (Clipboard.qml): the new selection arrives on stdin and is
/// kept as ~/.cache/mist/clip/<ms>-<hash>.<txt|png|…>, deduplicated (a repeat moves to the top), newest 80.
/// Password managers mark their copies with x-kde-passwordManagerHint: those are never stored.
pub fn clipstore() {
    let d = clip_dir();
    let _ = std::fs::create_dir_all(&d);
    let mut data = vec![];
    let _ = std::io::stdin().take(64 << 20).read_to_end(&mut data);
    let (_, types) = run(&["wl-paste", "--list-types"], 3.0);
    if types.contains("x-kde-passwordManagerHint") || data.is_empty() {
        return;
    }
    let ext = match image_ext(&data) {
        Some(e) => e,
        // text: no NUL byte (what `grep -qI` calls binary) and something besides whitespace
        None if !data.contains(&0) && data.iter().any(|c| !c.is_ascii_whitespace()) => "txt",
        None => return,
    };
    let h = fnv(&data);
    let suffix = format!("-{}.", h);
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = vec![];
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with('.') {
                continue;
            }
            if n.contains(&suffix) {
                let _ = std::fs::remove_file(e.path());
                continue;
            }
            files.push((e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()));
        }
    }
    let ms = (now() * 1000.0) as u64;
    let tmp = format!("{}/.in.{}", d, std::process::id());
    if std::fs::write(&tmp, &data).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, format!("{}/{}-{}.{}", d, ms, h, ext));
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in files.iter().skip(79) {
        let _ = std::fs::remove_file(p);
    }
}

/// The history for Clipboard.qml: [{f, kind: "text"|"image", text, t}], newest first.
pub fn cliplist(dir: &str) {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect())
        .unwrap_or_default();
    names.sort_by(|a, b| b.cmp(a));
    let mut items: Vec<J> = vec![];
    for f in names {
        let p = format!("{}/{}", dir, f);
        let t: i64 = f.split('-').next().and_then(|x| x.parse().ok()).unwrap_or(0);
        if f.ends_with(".txt") {
            let mut b = vec![];
            if let Ok(fh) = std::fs::File::open(&p) {
                let _ = fh.take(16000).read_to_end(&mut b);
            }
            let text: String = String::from_utf8_lossy(&b).chars().take(4000).collect();
            items.push(json!({"f": p, "kind": "text", "text": text, "t": t}));
        } else {
            items.push(json!({"f": p, "kind": "image", "text": "", "t": t}));
        }
    }
    println!("{}", J::Array(items));
}

/// Noctalia stores notification images as WebP; Mist's Qt has no WebP plugin: mirror them as PNG
/// into ~/.cache/mist/notif/ (only missing ones).
pub fn notifimg() {
    let src = format!("{}/.local/state/noctalia/notification_history_assets", home());
    let dst = format!("{}/.cache/mist/notif", home());
    let _ = std::fs::create_dir_all(&dst);
    let Ok(rd) = std::fs::read_dir(&src) else { return };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = n.strip_suffix(".webp") else { continue };
        let out = format!("{}/{}.png", dst, stem);
        if std::path::Path::new(&out).exists() {
            continue;
        }
        if let Ok(img) = image::open(e.path()) {
            let tmp = format!("{}.tmp.png", out);
            if img.save_with_format(&tmp, image::ImageFormat::Png).is_ok() {
                let _ = std::fs::rename(&tmp, &out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_hash() {
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(image_ext(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(image_ext(b"plain text"), None);
        assert_eq!(fnv(b"abc").len(), 12);
        assert_eq!(fnv(b"abc"), fnv(b"abc"));
        assert_ne!(fnv(b"abc"), fnv(b"abd"));
    }
}
