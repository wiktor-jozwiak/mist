// Clipboard history (was Clipboard.qml; Super+V), styled like Spotlight: search capsule, the list on the
// left, a preview on the right. History: `wl-paste --watch mistd clipstore` (started by mist-ui, watch())
// -> ~/.cache/mist/clip (text + images, newest 80). Enter copies the item back (paste with Ctrl+V),
// Delete removes it, Esc closes.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const INSET: Rgba = Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.26);
const SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);

fn mistd() -> String {
    format!("{}/.config/quickshell/mist/scripts/bin/mistd", std::env::var("HOME").unwrap_or_default())
}
fn dir() -> String {
    format!("{}/.cache/mist/clip", std::env::var("HOME").unwrap_or_default())
}

// the history watcher: one wl-paste for the whole session (restarted by the caller when it exits)
pub fn watch() -> Option<std::process::Child> {
    std::process::Command::new("setpriv").args(["--pdeathsig", "TERM", "wl-paste", "--watch", &mistd(), "clipstore"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()
}

pub struct Clipboard {
    items: Vec<Value>,
    field: Field,
    sel: usize,
    top: usize, // first visible row
    ptr: (f32, f32),
    rows: Vec<((f32, f32, f32, f32), usize)>,
    btns: Vec<((f32, f32, f32, f32), u8)>, // 0 delete, 1 copy, 9 card
    pressed_outside: bool,
    last_click: (usize, std::time::Instant),
}

impl Clipboard {
    pub fn new() -> Clipboard {
        let items = std::process::Command::new(mistd()).args(["cliplist", &dir()]).output().ok().and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok()).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        Clipboard { items, field: Field::default(), sel: 0, top: 0, ptr: (-1.0, -1.0), rows: Vec::new(), btns: Vec::new(), pressed_outside: false, last_click: (usize::MAX, std::time::Instant::now()) }
    }
    fn shown(&self) -> Vec<&Value> {
        let q = self.field.text.trim().to_lowercase();
        self.items.iter().filter(|i| q.is_empty() || (i["kind"] == "text" && i["text"].as_str().unwrap_or("").to_lowercase().contains(&q)) || (i["kind"] == "image" && "obraz image".contains(&q))).collect()
    }
    fn pick(&self, i: usize) -> Act {
        let Some(it) = self.shown().get(i).cloned().cloned() else { return Act::None };
        let f = it["f"].as_str().unwrap_or("").to_string();
        let ty = if it["kind"] == "image" { format!("image/{}", f.rsplit('.').next().unwrap_or("png")) } else { "text/plain;charset=utf-8".into() };
        Act::Run(vec!["sh".into(), "-c".into(), "wl-copy --type \"$2\" < \"$1\"".into(), "sh".into(), f, ty])
    }
    fn remove(&mut self, i: usize) -> Act {
        let Some(f) = self.shown().get(i).map(|it| it["f"].as_str().unwrap_or("").to_string()) else { return Act::None };
        self.items.retain(|x| x["f"].as_str() != Some(&f));
        self.sel = self.sel.min(self.shown().len().saturating_sub(1));
        Act::Exec(vec!["rm".into(), "-f".into(), f])
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
        self.btns.iter().any(|b| b.1 < 2 && inside(self.ptr, b.0))
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
        let shown: Vec<Value> = self.shown().into_iter().cloned().collect();
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
            let img = it["kind"] == "image";
            let text = it["text"].as_str().unwrap_or("");
            crate::desktop::rect(c, lx + 8.0, ry + 8.0, 28.0, 28.0, 8.0, if img { Rgba(187.0 / 255.0, 169.0 / 255.0, 238.0 / 255.0, 0.18) } else { FILL2 });
            let icon = if img { "photo" } else if text.trim_start().starts_with("http://") || text.trim_start().starts_with("https://") { "link" } else { "fileText" };
            c.icon(t, lx + 22.0, ry + 22.0, icon, 15.0, if img { Rgba::hex("#BF5AF2") } else { T2 });
            let label = if img { "Obraz".to_string() } else { text.split_whitespace().collect::<Vec<_>>().join(" ") };
            let ls = Style::ui(13.5, 480.0);
            let lhh = crate::desktop::line_h(t, &ls);
            crate::desktop::text(c, t, lx + 46.0, ry + (44.0 - lhh) / 2.0, &label, &ls, T1, false, Some(lw - 56.0));
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
            if cur["kind"] == "image" {
                if let Some(pm) = cx.imgs.get(cur["f"].as_str().unwrap_or(""), 900, cx.icons) {
                    crate::img::draw(c, &pm, px + 10.0, py + 10.0, pw - 20.0, ph - 54.0 - 20.0, 0.0, false, 1.0);
                }
            } else {
                let ps = Style::ui(14.0, 450.0);
                let plh = crate::desktop::line_h(t, &ps);
                let max_lines = ((ph - 54.0 - 28.0) / plh).floor() as usize;
                let mut lines = Vec::new();
                for para in cur["text"].as_str().unwrap_or("").split('\n') {
                    // WrapAnywhere
                    let mut line = String::new();
                    for ch in para.chars() {
                        line.push(ch);
                        if t.width(&line, &ps) > pw - 28.0 {
                            line.pop();
                            lines.push(std::mem::take(&mut line));
                            line.push(ch);
                        }
                    }
                    lines.push(line);
                    if lines.len() > max_lines {
                        break;
                    }
                }
                for (i, l) in lines.iter().take(max_lines).enumerate() {
                    crate::desktop::text(c, t, px + 14.0, py + 14.0 + i as f32 * plh, l, &ps, T1, false, None);
                }
            }
            let by = py + ph - 34.0;
            let info = if cur["kind"] == "image" { "Obraz".to_string() } else { format!("{} znaków", cur["text"].as_str().unwrap_or("").chars().count()) };
            let l1 = lh(t, 12.5, 550.0);
            let l2 = lh(t, 11.5, 450.0);
            let iy = by + (34.0 - l1 - l2) / 2.0;
            crate::desktop::text(c, t, px, iy, &info, &Style::ui(12.5, 550.0), T2, false, None);
            crate::desktop::text(c, t, px, iy + l1, &format!("Skopiowano {}", ago(cur["t"].as_i64().unwrap_or(0))), &Style::ui(11.5, 450.0), T3, false, None);
            let bx = px + pw - 236.0;
            let dh = inside(self.ptr, (bx, by, 100.0, 34.0));
            pill(c, t, bx, by, 100.0, "Usuń  ⌦", if dh { Rgba(238.0 / 255.0, 154.0 / 255.0, 170.0 / 255.0, 0.24) } else { FILL }, T2, 550.0);
            let kh = inside(self.ptr, (bx + 108.0, by, 120.0, 34.0));
            pill(c, t, bx + 108.0, by, 120.0, "Kopiuj  ↵", if kh { ACCENT } else { Rgba::hex("#409CFF") }, Rgba::hex("#0e1420"), 600.0);
            self.btns.push(((bx, by, 100.0, 34.0), 0));
            self.btns.push(((bx + 108.0, by, 120.0, 34.0), 1));
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
                    // double click copies
                    let dbl = self.last_click.0 == r && self.last_click.1.elapsed().as_millis() < 400;
                    self.last_click = (r, std::time::Instant::now());
                    self.sel = r;
                    return if dbl { self.pick(r) } else { Act::Redraw };
                }
                match btn {
                    Some(0) => self.remove(self.sel),
                    Some(1) => self.pick(self.sel),
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
            Keysym::Return | Keysym::KP_Enter => self.pick(self.sel),
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
