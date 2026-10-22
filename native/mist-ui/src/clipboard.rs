// Clipboard history (was Clipboard.qml; Super+V), styled like Spotlight: search capsule, the list on the
// left, a preview on the right. History: `wl-paste --watch mistd clipstore` (started by mist-ui, watch())
// -> ~/.cache/mist/clip (text + images, newest 80 + pinned ones, their hashes in .pinned). Enter pastes the
// item into the window below (copy + MistPaste: mist-comp types Ctrl+V), Ctrl+Enter / Ctrl+C only copies,
// Ctrl+P pins, Delete removes, Esc closes. The list is read in-process and kept while the folder is unchanged.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const INSET: Rgba = Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.26);
const SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);

fn mistd() -> String {
    crate::paths::mistd()
}
fn dir() -> String {
    format!("{}/.cache/mist/clip", std::env::var("HOME").unwrap_or_default())
}

// the history watcher: one wl-paste for the whole session (restarted by the caller when it exits)
pub fn watch() -> Option<std::process::Child> {
    std::process::Command::new("setpriv").args(["--pdeathsig", "TERM", "wl-paste", "--watch", &mistd(), "clipstore"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()
}

#[derive(Clone)]
struct Item {
    f: String,
    img: bool,
    text: String,
    t: i64,
    hash: String,
    pinned: bool,
}

// the history as last read and the folder's mtime then (opening again re-reads only after a change)
static CACHE: std::sync::Mutex<Option<(std::time::SystemTime, Vec<Item>)>> = std::sync::Mutex::new(None);

fn pins() -> Vec<String> {
    std::fs::read_to_string(format!("{}/.pinned", dir())).unwrap_or_default().lines().map(String::from).collect()
}

fn load() -> Vec<Item> {
    use std::io::Read;
    let d = dir();
    let mtime = std::fs::metadata(&d).and_then(|m| m.modified()).ok();
    if let (Some(m), Ok(c)) = (mtime, CACHE.lock()) {
        if let Some((_, items)) = c.as_ref().filter(|(at, _)| *at == m) {
            return items.clone();
        }
    }
    let pins = pins();
    let mut names: Vec<String> = std::fs::read_dir(&d).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect()).unwrap_or_default();
    names.sort_by(|a, b| b.cmp(a));
    let mut items: Vec<Item> = names
        .into_iter()
        .map(|n| {
            let f = format!("{d}/{n}");
            let t = n.split('-').next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let hash = n.split_once('-').map(|(_, r)| r.split('.').next().unwrap_or(r).to_string()).unwrap_or_default();
            let img = !n.ends_with(".txt");
            let mut text = String::new();
            if !img {
                let mut b = Vec::new();
                if let Ok(fh) = std::fs::File::open(&f) {
                    let _ = fh.take(16000).read_to_end(&mut b);
                }
                text = String::from_utf8_lossy(&b).chars().take(4000).collect();
            }
            let pinned = pins.contains(&hash);
            Item { f, img, text, t, hash, pinned }
        })
        .collect();
    // pinned first, each group newest first (stable sort keeps the name order)
    items.sort_by_key(|i| !i.pinned);
    if let (Some(m), Ok(mut c)) = (mtime, CACHE.lock()) {
        *c = Some((m, items.clone()));
    }
    items
}

fn forget_cache() {
    if let Ok(mut c) = CACHE.lock() {
        *c = None;
    }
}

pub struct Clipboard {
    items: Vec<Item>,
    field: Field,
    sel: usize,
    top: usize, // first visible row
    ptr: (f32, f32),
    rows: Vec<((f32, f32, f32, f32), usize)>,
    btns: Vec<((f32, f32, f32, f32), u8)>, // 0 delete, 1 copy, 2 paste, 3 pin, 9 card
    pressed_outside: bool,
    last_click: (usize, std::time::Instant),
    wrapped: Option<(String, f32, Vec<String>)>, // the preview's lines (file, width): wrapping measures every char
}

impl Clipboard {
    pub fn new() -> Clipboard {
        Clipboard { items: load(), field: Field::default(), sel: 0, top: 0, ptr: (-1.0, -1.0), rows: Vec::new(), btns: Vec::new(), pressed_outside: false, last_click: (usize::MAX, std::time::Instant::now()), wrapped: None }
    }
    fn shown(&self) -> Vec<&Item> {
        let q = self.field.text.trim().to_lowercase();
        self.items.iter().filter(|i| q.is_empty() || (!i.img && i.text.to_lowercase().contains(&q)) || (i.img && "obraz image".contains(&q))).collect()
    }
    fn mime(it: &Item) -> String {
        if it.img { format!("image/{}", it.f.rsplit('.').next().unwrap_or("png")) } else { "text/plain;charset=utf-8".into() }
    }
    // only onto the clipboard (paste yourself)
    fn copy(&self, i: usize) -> Act {
        let Some(it) = self.shown().get(i).cloned().cloned() else { return Act::None };
        Act::Run(vec!["sh".into(), "-c".into(), "wl-copy --type \"$2\" < \"$1\"".into(), "sh".into(), it.f.clone(), Self::mime(&it)])
    }
    // onto the clipboard and into the window below
    fn paste(&self, i: usize) -> Act {
        let Some(it) = self.shown().get(i).cloned().cloned() else { return Act::None };
        Act::Paste(it.f.clone(), Self::mime(&it))
    }
    fn remove(&mut self, i: usize) -> Act {
        let Some(f) = self.shown().get(i).map(|it| it.f.clone()) else { return Act::None };
        self.items.retain(|x| x.f != f);
        self.sel = self.sel.min(self.shown().len().saturating_sub(1));
        forget_cache();
        Act::Exec(vec!["rm".into(), "-f".into(), f])
    }
    fn toggle_pin(&mut self, i: usize) -> Act {
        let Some(h) = self.shown().get(i).map(|it| it.hash.clone()) else { return Act::None };
        let mut p = pins();
        let on = !p.contains(&h);
        if on { p.push(h.clone()) } else { p.retain(|x| *x != h) }
        let _ = std::fs::write(format!("{}/.pinned", dir()), p.join("\n"));
        for it in self.items.iter_mut().filter(|it| it.hash == h) {
            it.pinned = on;
        }
        self.items.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.t.cmp(&a.t)));
        // the selection follows the item
        self.sel = self.shown().iter().position(|it| it.hash == h).unwrap_or(0);
        forget_cache();
        Act::Redraw
    }
}

fn ago(t: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let s = (now - t) / 1000;
    if s < 60 { "przed chwilą".into() } else if s < 3600 { format!("{} min temu", s / 60) } else if s < 86400 { format!("{} godz. temu", s / 3600) } else { format!("{} dni temu", s / 86400) }
}

impl Pane for Clipboard {
    fn name(&self) -> &'static str {
        "clipboard"
    }
    fn namespace(&self) -> &'static str {
        "mist-spotlight"
    }
    fn hand(&self) -> bool {
        self.btns.iter().any(|b| b.1 < 9 && inside(self.ptr, b.0))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        c.clear();
        self.rows.clear();
        self.btns.clear();
        let (w, h) = (820.0, 500.0);
        let (x0, y0) = (((c.w() - w) / 2.0).round(), (c.h() * 0.24).round());
        glass(c, x0, y0, w, h, 30.0, 0.76);
        self.btns.push(((x0, y0, w, h), 9));
        let shown: Vec<Item> = self.shown().into_iter().cloned().collect();
        // search bar
        c.icon(t, x0 + 22.0 + 11.0, y0 + 30.0, "clipboard", 22.0, T2);
        let fs = Style::ui(22.0, 400.0);
        let fh = crate::desktop::line_h(t, &fs);
        let fx = x0 + 22.0 + 22.0 + 14.0;
        let count = if self.items.is_empty() { String::new() } else { format!("{} / {}", shown.len(), self.items.len()) };
        let cs = Style::ui(12.0, 450.0);
        let cw = t.width(&count, &cs);
        let fw = w - (fx - x0) - 22.0 - cw - 12.0;
        if self.field.text.is_empty() {
            crate::desktop::text(c, t, fx, y0 + (60.0 - fh) / 2.0, "Szukaj w historii schowka", &fs, T3, false, Some(fw));
            crate::desktop::rect(c, fx, y0 + 18.0, 1.5, 24.0, 0.0, T1);
        } else {
            let tw = crate::desktop::text(c, t, fx, y0 + (60.0 - fh) / 2.0, &self.field.text, &fs, T1, false, Some(fw));
            crate::desktop::rect(c, fx + tw + 1.0, y0 + 18.0, 1.5, 24.0, 0.0, T1);
        }
        let ch = crate::desktop::line_h(t, &cs);
        crate::desktop::text(c, t, x0 + w - 22.0 - cw, y0 + (60.0 - ch) / 2.0, &count, &cs, T3, false, None);
        crate::desktop::rect(c, x0 + 16.0, y0 + 60.0, w - 32.0, 1.0, 0.0, HAIR);
        // list (44 px rows, scrolled to keep the selection visible)
        let (lx, ly, lw, lhgt) = (x0 + 8.0, y0 + 68.0, 330.0, h - 76.0);
        let vis = (lhgt / 44.0).floor() as usize;
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
        for (k, it) in shown.iter().enumerate().skip(self.top).take(vis) {
            let ry = ly + (k - self.top) as f32 * 44.0;
            let hov = inside(self.ptr, (lx, ry, lw, 44.0));
            if k == self.sel {
                crate::desktop::rect(c, lx, ry, lw, 44.0, 11.0, SOFT);
            } else if hov {
                crate::desktop::rect(c, lx, ry, lw, 44.0, 11.0, FILL);
            }
            let img = it.img;
            let text = it.text.as_str();
            crate::desktop::rect(c, lx + 8.0, ry + 8.0, 28.0, 28.0, 8.0, if img { Rgba(187.0 / 255.0, 169.0 / 255.0, 238.0 / 255.0, 0.18) } else { FILL2 });
            let icon = if img { "photo" } else if text.trim_start().starts_with("http://") || text.trim_start().starts_with("https://") { "link" } else { "fileText" };
            c.icon(t, lx + 22.0, ry + 22.0, icon, 15.0, if img { Rgba::hex("#BF5AF2") } else { T2 });
            let label = if img { "Obraz".to_string() } else { text.split_whitespace().collect::<Vec<_>>().join(" ") };
            let ls = Style::ui(13.5, 480.0);
            let lhh = crate::desktop::line_h(t, &ls);
            let pin_w = if it.pinned { 22.0 } else { 0.0 };
            crate::desktop::text(c, t, lx + 46.0, ry + (44.0 - lhh) / 2.0, &label, &ls, T1, false, Some(lw - 56.0 - pin_w));
            if it.pinned {
                c.icon(t, lx + lw - 18.0, ry + 22.0, "pinned", 13.0, T3);
            }
            self.rows.push(((lx, ry, lw, 44.0), k));
        }
        if shown.is_empty() {
            let s = if self.items.is_empty() { "Historia schowka jest pusta" } else { "Brak wyników" };
            let s13 = lh(t, 13.0, 450.0);
            txt_c(c, t, lx + lw / 2.0, ly + lhgt / 2.0 - s13 / 2.0, s, 13.0, 450.0, T3, lw);
        }
        crate::desktop::rect(c, x0 + 346.0, y0 + 72.0, 1.0, h - 88.0, 0.0, HAIR);
        // preview
        if let Some(cur) = shown.get(self.sel) {
            let (px, py, pw, ph) = (x0 + 362.0, y0 + 72.0, w - 362.0 - 18.0, h - 72.0 - 18.0);
            crate::desktop::rect(c, px, py, pw, ph - 54.0, 14.0, INSET);
            if cur.img {
                if let Some(pm) = cx.imgs.get(&cur.f, 900, cx.icons) {
                    crate::img::draw(c, &pm, px + 10.0, py + 10.0, pw - 20.0, ph - 54.0 - 20.0, 0.0, false, 1.0);
                }
            } else {
                let ps = Style::ui(14.0, 450.0);
                let plh = crate::desktop::line_h(t, &ps);
                let max_lines = ((ph - 54.0 - 28.0) / plh).floor() as usize;
                if self.wrapped.as_ref().is_none_or(|(f, w, _)| *f != cur.f || *w != pw) {
                    // WrapAnywhere, only as far as the preview shows
                    let mut lines = Vec::new();
                    'paras: for para in cur.text.split('\n') {
                        let mut line = String::new();
                        for ch in para.chars() {
                            line.push(ch);
                            if t.width(&line, &ps) > pw - 28.0 {
                                line.pop();
                                lines.push(std::mem::take(&mut line));
                                line.push(ch);
                                if lines.len() >= max_lines {
                                    break 'paras;
                                }
                            }
                        }
                        lines.push(line);
                        if lines.len() >= max_lines {
                            break;
                        }
                    }
                    self.wrapped = Some((cur.f.clone(), pw, lines));
                }
                let lines = &self.wrapped.as_ref().unwrap().2;
                for (i, l) in lines.iter().take(max_lines).enumerate() {
                    crate::desktop::text(c, t, px + 14.0, py + 14.0 + i as f32 * plh, l, &ps, T1, false, None);
                }
            }
            let by = py + ph - 34.0;
            let info = if cur.img { "Obraz".to_string() } else { format!("{} znaków", cur.text.chars().count()) };
            let l1 = lh(t, 12.5, 550.0);
            let l2 = lh(t, 11.5, 450.0);
            let iy = by + (34.0 - l1 - l2) / 2.0;
            crate::desktop::text(c, t, px, iy, &info, &Style::ui(12.5, 550.0), T2, false, None);
            crate::desktop::text(c, t, px, iy + l1, &format!("Skopiowano {}", ago(cur.t)), &Style::ui(11.5, 450.0), T3, false, None);
            // pin, delete, copy (Ctrl+Enter), paste (Enter: the main one)
            let bx = px + pw - 290.0;
            let hov = |b: (f32, f32, f32, f32)| inside(self.ptr, b);
            let b_pin = (bx, by, 34.0, 34.0);
            crate::desktop::rect(c, bx, by, 34.0, 34.0, 17.0, if hov(b_pin) || cur.pinned { FILL2 } else { FILL });
            c.icon(t, bx + 17.0, by + 17.0, "pinned", 15.0, if cur.pinned { accent() } else { T2 });
            let b_del = (bx + 40.0, by, 34.0, 34.0);
            crate::desktop::rect(c, b_del.0, by, 34.0, 34.0, 17.0, if hov(b_del) { Rgba(238.0 / 255.0, 154.0 / 255.0, 170.0 / 255.0, 0.24) } else { FILL });
            c.icon(t, b_del.0 + 17.0, by + 17.0, "trash", 15.0, T2);
            let b_copy = (bx + 80.0, by, 92.0, 34.0);
            pill(c, t, b_copy.0, by, 92.0, "Kopiuj", if hov(b_copy) { FILL2 } else { FILL }, T1, 550.0);
            let b_paste = (bx + 178.0, by, 112.0, 34.0);
            pill(c, t, b_paste.0, by, 112.0, "Wklej  ↵", if hov(b_paste) { accent() } else { Rgba::hex("#409CFF") }, Rgba::hex("#0e1420"), 600.0);
            self.btns.push((b_pin, 3));
            self.btns.push((b_del, 0));
            self.btns.push((b_copy, 1));
            self.btns.push((b_paste, 2));
        }
        glass_rim(c, x0, y0, w, h, 30.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let row = self.rows.iter().find(|r| inside((x, y), r.0)).map(|r| r.1);
        let btn = self.btns.iter().rev().find(|b| inside((x, y), b.0)).map(|b| b.1);
        match ev {
            Ptr::Motion | Ptr::Enter | Ptr::Leave => Act::Redraw,
            Ptr::Press(_) => {
                self.pressed_outside = btn.is_none();
                Act::None
            }
            Ptr::Release(_) => {
                if let Some(r) = row {
                    // double click pastes
                    let dbl = self.last_click.0 == r && self.last_click.1.elapsed().as_millis() < 400;
                    self.last_click = (r, std::time::Instant::now());
                    self.sel = r;
                    return if dbl { self.paste(r) } else { Act::Redraw };
                }
                match btn {
                    Some(0) => self.remove(self.sel),
                    Some(1) => self.copy(self.sel),
                    Some(2) => self.paste(self.sel),
                    Some(3) => self.toggle_pin(self.sel),
                    None if self.pressed_outside => Act::Close,
                    _ => Act::None,
                }
            }
            Ptr::Wheel(dy) => {
                let n = self.shown().len();
                self.sel = ((self.sel as i32 + dy.signum() as i32).max(0) as usize).min(n.saturating_sub(1));
                Act::Redraw
            }
        }
    }
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        let n = self.shown().len();
        match ev.keysym {
            Keysym::Escape => Act::Close,
            Keysym::Down => {
                self.sel = (self.sel + 1).min(n.saturating_sub(1));
                Act::Redraw
            }
            Keysym::Up => {
                self.sel = self.sel.saturating_sub(1);
                Act::Redraw
            }
            Keysym::Return | Keysym::KP_Enter if cx.ctrl => self.copy(self.sel),
            Keysym::Return | Keysym::KP_Enter => self.paste(self.sel),
            Keysym::c | Keysym::C if cx.ctrl => self.copy(self.sel),
            Keysym::p | Keysym::P if cx.ctrl => self.toggle_pin(self.sel),
            Keysym::Delete => self.remove(self.sel),
            _ => {
                if self.field.key(ev, cx.ctrl) {
                    self.sel = 0;
                    self.top = 0;
                    Act::Redraw
                } else {
                    Act::None
                }
            }
        }
    }
}
