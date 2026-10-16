// One-line text field for the apps (address bar, search, rename, file name): caret, selection, word jumps,
// clipboard through wl-copy / wl-paste, horizontal scroll that keeps the caret in view.
use crate::gfx::{Canvas, Rgba};
use crate::text::{Style, Text};
use smithay_client_toolkit::seat::keyboard::Keysym;

#[derive(Default, Clone)]
pub struct LineEdit {
    pub text: String,
    pub caret: usize,  // byte index
    pub anchor: usize, // selection start (== caret: none)
    scroll: f32,
}

pub enum EditResult {
    // the key was used (text or caret changed)
    Used,
    // Return / Escape / something the field does not handle
    Accept,
    Cancel,
    Ignored,
}

fn prev_char(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map(|(k, _)| k).unwrap_or(0)
}
fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(s.len())
}
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl LineEdit {
    pub fn new(s: &str) -> LineEdit {
        LineEdit { text: s.to_string(), caret: s.len(), anchor: s.len(), scroll: 0.0 }
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }
    // select [a, b) bytes (rename: the name without the extension)
    pub fn select(&mut self, a: usize, b: usize) {
        self.anchor = a.min(self.text.len());
        self.caret = b.min(self.text.len());
    }
    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.caret = s.len();
        self.anchor = s.len();
    }
    fn sel(&self) -> (usize, usize) {
        (self.anchor.min(self.caret), self.anchor.max(self.caret))
    }
    pub fn has_sel(&self) -> bool {
        self.anchor != self.caret
    }
    fn del_sel(&mut self) -> bool {
        let (a, b) = self.sel();
        if a == b {
            return false;
        }
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
        true
    }
    pub fn insert(&mut self, s: &str) {
        self.del_sel();
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(self.caret, &s);
        self.caret += s.len();
        self.anchor = self.caret;
    }
    fn word_left(&self, mut i: usize) -> usize {
        while i > 0 && !self.text[..i].chars().next_back().map(is_word).unwrap_or(false) {
            i = prev_char(&self.text, i);
        }
        while i > 0 && self.text[..i].chars().next_back().map(is_word).unwrap_or(false) {
            i = prev_char(&self.text, i);
        }
        i
    }
    fn word_right(&self, mut i: usize) -> usize {
        let n = self.text.len();
        while i < n && !self.text[i..].chars().next().map(is_word).unwrap_or(false) {
            i = next_char(&self.text, i);
        }
        while i < n && self.text[i..].chars().next().map(is_word).unwrap_or(false) {
            i = next_char(&self.text, i);
        }
        i
    }

    pub fn key(&mut self, k: Keysym, utf8: Option<&str>, ctrl: bool, shift: bool) -> EditResult {
        let mv = |s: &mut LineEdit, to: usize| {
            s.caret = to;
            if !shift {
                s.anchor = to;
            }
        };
        match k {
            Keysym::Return | Keysym::KP_Enter => return EditResult::Accept,
            Keysym::Escape => return EditResult::Cancel,
            Keysym::Left => {
                if !shift && self.has_sel() {
                    let a = self.sel().0;
                    mv(self, a);
                } else {
                    let t = if ctrl { self.word_left(self.caret) } else { prev_char(&self.text, self.caret) };
                    mv(self, t);
                }
            }
            Keysym::Right => {
                if !shift && self.has_sel() {
                    let b = self.sel().1;
                    mv(self, b);
                } else {
                    let t = if ctrl { self.word_right(self.caret) } else { next_char(&self.text, self.caret) };
                    mv(self, t);
                }
            }
            Keysym::Home | Keysym::KP_Home => mv(self, 0),
            Keysym::End | Keysym::KP_End => {
                let n = self.text.len();
                mv(self, n)
            }
            Keysym::BackSpace => {
                if !self.del_sel() && self.caret > 0 {
                    let a = if ctrl { self.word_left(self.caret) } else { prev_char(&self.text, self.caret) };
                    self.text.replace_range(a..self.caret, "");
                    self.caret = a;
                    self.anchor = a;
                }
            }
            Keysym::Delete | Keysym::KP_Delete => {
                if !self.del_sel() && self.caret < self.text.len() {
                    let b = if ctrl { self.word_right(self.caret) } else { next_char(&self.text, self.caret) };
                    self.text.replace_range(self.caret..b, "");
                }
            }
            _ if ctrl => {
                let c = utf8.and_then(|s| s.chars().next()).map(|c| c.to_ascii_lowercase());
                // Ctrl+letter arrives as a control character: use the keysym
                let name = match k {
                    Keysym::a | Keysym::A => 'a',
                    Keysym::c | Keysym::C => 'c',
                    Keysym::x | Keysym::X => 'x',
                    Keysym::v | Keysym::V => 'v',
                    _ => c.unwrap_or('\0'),
                };
                match name {
                    'a' => self.select_all(),
                    'c' | 'x' => {
                        let (a, b) = self.sel();
                        if a < b {
                            copy_text(&self.text[a..b]);
                            if name == 'x' {
                                self.del_sel();
                            }
                        }
                    }
                    'v' => {
                        let s = paste_text();
                        let s = s.lines().next().unwrap_or("").to_string();
                        self.insert(&s);
                    }
                    _ => return EditResult::Ignored,
                }
            }
            _ => match utf8 {
                Some(s) if !s.is_empty() && !s.chars().any(|c| c.is_control()) => self.insert(s),
                _ => return EditResult::Ignored,
            },
        }
        EditResult::Used
    }

    // caret position from a click at x (field coordinates, after the padding)
    pub fn click(&mut self, t: &mut Text, st: &Style, x: f32, shift: bool) {
        let x = x + self.scroll;
        let mut best = (0usize, f32::MAX);
        let mut i = 0;
        loop {
            let w = t.width(&self.text[..i], st);
            let d = (w - x).abs();
            if d < best.1 {
                best = (i, d);
            }
            if i >= self.text.len() {
                break;
            }
            i = next_char(&self.text, i);
        }
        self.caret = best.0;
        if !shift {
            self.anchor = best.0;
        }
    }
    pub fn select_word_at(&mut self, t: &mut Text, st: &Style, x: f32) {
        self.click(t, st, x, false);
        let a = self.word_left(next_char(&self.text, self.caret).min(self.text.len()));
        let b = self.word_right(a);
        self.anchor = a;
        self.caret = b;
    }

    // draw the text in (x, y, w, h); caret and selection when focused; centred text when `center`
    #[allow(clippy::too_many_arguments)]
    pub fn paint(&mut self, c: &mut Canvas, t: &mut Text, x: f32, y: f32, w: f32, h: f32, st: &Style, fg: Rgba, placeholder: &str, focused: bool, center: bool) {
        let cy = y + h / 2.0;
        if self.text.is_empty() && !placeholder.is_empty() {
            let pw = t.width(placeholder, st);
            let px = if center { x + (w - pw) / 2.0 } else { x };
            clip_text(c, t, px, cy, x, w, placeholder, st, fg.alpha(0.55));
        }
        let tw = t.width(&self.text, st);
        let cw = t.width(&self.text[..self.caret.min(self.text.len())], st);
        // keep the caret visible
        if tw <= w {
            self.scroll = 0.0;
        } else {
            if cw - self.scroll > w - 2.0 {
                self.scroll = cw - w + 2.0;
            }
            if cw - self.scroll < 0.0 {
                self.scroll = cw;
            }
            self.scroll = self.scroll.clamp(0.0, (tw - w + 2.0).max(0.0));
        }
        let ox = if center && tw <= w { x + (w - tw) / 2.0 } else { x - self.scroll };
        if focused && self.has_sel() {
            let (a, b) = self.sel();
            let (sa, sb) = (t.width(&self.text[..a], st), t.width(&self.text[..b], st));
            let (l, r) = ((ox + sa).max(x), (ox + sb).min(x + w));
            if r > l {
                c.rrect(l, cy - st.size * 0.72, r - l, st.size * 1.44, 3.0, Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 0.40));
            }
        }
        clip_text(c, t, ox, cy, x, w, &self.text.clone(), st, fg);
        if focused {
            let cx = (ox + cw).round().clamp(x, x + w);
            c.rrect(cx, cy - st.size * 0.62, 1.5, st.size * 1.24, 0.5, Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 1.0));
        }
    }
}

// text clipped to [cx0, cx0 + cw] (drawn on a scratch canvas when it would overflow)
#[allow(clippy::too_many_arguments)]
pub fn clip_text(c: &mut Canvas, t: &mut Text, x: f32, cy: f32, cx0: f32, cw: f32, s: &str, st: &Style, col: Rgba) {
    let w = t.width(s, st);
    if x >= cx0 - 0.5 && x + w <= cx0 + cw + 0.5 {
        c.text(t, x, cy, s, st, col);
        return;
    }
    let h = (st.size * 2.0).ceil() as u32 + 4;
    let mut tmp = Canvas::new(cw.max(1.0).ceil() as u32, h);
    tmp.text(t, x - cx0, h as f32 / 2.0, s, st, col);
    c.pm.draw_pixmap(cx0.round() as i32, (cy - h as f32 / 2.0).round() as i32, tmp.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
}

pub fn copy_text(s: &str) {
    use std::io::Write;
    if let Ok(mut ch) = std::process::Command::new("wl-copy").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        if let Some(mut i) = ch.stdin.take() {
            let _ = i.write_all(s.as_bytes());
        }
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}
pub fn paste_text() -> String {
    std::process::Command::new("wl-paste").args(["-n", "-t", "text/plain"]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}
