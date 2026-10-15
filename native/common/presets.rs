// Live wallpaper presets, read from components/LivePresets.js at run time (the JS object literals of `list`
// turned into JSON), so presets added there need no Rust change. day() / tempo() are ported.
#![allow(dead_code)]
use serde_json::Value;

pub fn path() -> String {
    format!("{}/.config/quickshell/mist/components/LivePresets.js", std::env::var("HOME").unwrap_or_default())
}

// JS literal -> JSON: quote bare keys, drop comments and trailing commas
fn js_to_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() { out.push(b[i + 1]); i += 2; continue; }
            if c == '"' { in_str = false; }
            i += 1;
            continue;
        }
        if c == '"' { in_str = true; out.push(c); i += 1; continue; }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' { i += 1; }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') { i += 1; }
            let word: String = b[st..i].iter().collect();
            let mut j = i;
            while j < b.len() && b[j] == ' ' { j += 1; }
            if j < b.len() && b[j] == ':' { out.push('"'); out.push_str(&word); out.push('"'); } else { out.push_str(&word); }
            continue;
        }
        out.push(c);
        i += 1;
    }
    // trailing commas
    let re = out.replace(",\n]", "\n]").replace(", ]", " ]").replace(",]", "]");
    re
}

pub fn load() -> Vec<Value> {
    let Ok(src) = std::fs::read_to_string(path()) else { return Vec::new() };
    let Some(start) = src.find("var list = [") else { return Vec::new() };
    let rest = &src[start + "var list = ".len()..];
    // the array ends at the first "\n]" at column 0
    let Some(end) = rest.find("\n]") else { return Vec::new() };
    let arr = &rest[..end + 2];
    serde_json::from_str::<Vec<Value>>(&js_to_json(arr)).unwrap_or_default()
}

pub fn get<'a>(list: &'a [Value], id: &str) -> Option<&'a Value> {
    list.iter().find(|p| p["id"].as_str() == Some(id) && p.get("mode").is_some())
}

// the wallpaper gallery: photos in ~/Obrazy/Tapety (2 levels), sorted
pub fn photo_dir() -> String {
    format!("{}/Obrazy/Tapety", std::env::var("HOME").unwrap_or_default())
}
pub fn photos() -> Vec<String> {
    fn walk(d: &std::path::Path, depth: u32, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth > 0 {
                    walk(&p, depth - 1, out);
                }
            } else if p.extension().and_then(|x| x.to_str()).map(|x| matches!(x.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")).unwrap_or(false) {
                out.push(p.to_string_lossy().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(std::path::Path::new(&photo_dir()), 1, &mut out);
    out.sort();
    out
}
// the gallery's still thumbnail of "live:<id>" / "photo:<path>" (rendered by mist-wall)
pub const THUMB_W: i32 = 220;
pub const THUMB_H: i32 = 124;
pub fn thumb_dir() -> String {
    format!("{}/.cache/mist/thumbs", std::env::var("HOME").unwrap_or_default())
}
pub fn thumb_path(key: &str) -> String {
    let k: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("{}/{k}.png", thumb_dir())
}

pub fn tempo(id: &str) -> f32 {
    match id { "calm" => 0.55, "fast" => 2.0, _ => 1.0 }
}

pub fn hex(c: &str) -> [f32; 3] {
    let v = u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0);
    [((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0]
}
fn mix(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}
fn round_css(v: [f32; 3]) -> [f32; 3] {
    // css() rounds to 8 bit
    [(v[0].clamp(0.0, 1.0) * 255.0).round() / 255.0, (v[1].clamp(0.0, 1.0) * 255.0).round() / 255.0, (v[2].clamp(0.0, 1.0) * 255.0).round() / 255.0]
}

// colours + stars for the sun at `alt`, dulled by clouds (0..1) and wetness
pub fn day(alt: f64, cloud: f32, wet: bool) -> ([[f32; 3]; 4], f32) {
    let keys: [(f64, [&str; 4], f32); 5] = [
        (-18.0, ["#03060e", "#0c1736", "#1f3170", "#3a3384"], 0.9),
        (-7.0, ["#060a1c", "#1a2656", "#45428a", "#b06a86"], 0.35),
        (1.0, ["#0e0812", "#2c1c48", "#b0564a", "#f0a060"], 0.0),
        (12.0, ["#071624", "#164a7a", "#2f86b0", "#8ad6d0"], 0.0),
        (35.0, ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], 0.0),
    ];
    let mut i = 0;
    while i < keys.len() - 2 && alt > keys[i + 1].0 { i += 1; }
    let (a, b) = (keys[i], keys[i + 1]);
    let mut k = (((alt - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)) as f32;
    k = k * k * (3.0 - 2.0 * k);
    let dull = (0.55 * cloud + if wet { 0.3 } else { 0.0 }).min(1.0);
    let mut c = [[0f32; 3]; 4];
    for j in 0..4 {
        let v = mix(hex(a.1[j]), hex(b.1[j]), k);
        let g = 0.3 * v[0] + 0.55 * v[1] + 0.15 * v[2];
        let v = mix(v, [g * 0.95, g, g * 1.08], dull * 0.45);
        c[j] = round_css([v[0] * (1.0 - 0.1 * dull), v[1] * (1.0 - 0.1 * dull), v[2] * (1.0 - 0.1 * dull)]);
    }
    (c, (a.2 + (b.2 - a.2) * k) * (1.0 - cloud))
}
