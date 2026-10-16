// Spotlight (was Spotlight.qml; Super+Space / S / R): a glass search capsule in the upper third, results
// drop out of it. Apps (ranked by ~/.config/mist/usage.json, shared with Start), calculator, commands,
// emoji, web. Emoji mode (Super+.: popup "emoji", or a ":" prefix) -> an emoji grid, Enter copies.
// Keys: ↑↓ (←→ in the emoji grid), Enter, Esc.
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::text::Style;
use crate::theme::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const SOFT: Rgba = Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18);

#[derive(Clone)]
enum Kind {
    Calc(String),
    App(usize),
    Cmd(Vec<String>),
    Emoji(String),
    Web(String),
}
#[derive(Clone)]
struct Res {
    kind: Kind,
    title: String,
    sub: String,
    icon: String,
}

pub fn norm(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'ą' => 'a', 'ć' => 'c', 'ę' => 'e', 'ł' => 'l', 'ń' => 'n', 'ó' => 'o', 'ś' => 's', 'ź' | 'ż' => 'z',
        'á' | 'à' | 'â' | 'ä' => 'a', 'é' | 'è' | 'ê' | 'ë' => 'e', 'í' | 'ì' | 'î' | 'ï' => 'i', 'ú' | 'ù' | 'û' | 'ü' => 'u', 'ö' | 'ô' => 'o',
        c => c,
    }).collect()
}

// the calculator: + - * / % ^ and parentheses, "," as the decimal point
pub fn calc(src: &str) -> Option<String> {
    let s = src.trim();
    if s.is_empty() || !s.chars().all(|c| "0123456789+-*/().,%^ ".contains(c)) || !s.chars().any(|c| c.is_ascii_digit()) || !s.chars().any(|c| "+-*/^%".contains(c)) {
        return None;
    }
    let toks: Vec<char> = s.replace(',', ".").chars().filter(|c| !c.is_whitespace()).collect();
    struct P { t: Vec<char>, i: usize }
    impl P {
        fn peek(&self) -> Option<char> { self.t.get(self.i).copied() }
        fn expr(&mut self) -> Option<f64> {
            let mut v = self.term()?;
            while let Some(c) = self.peek() {
                if c == '+' { self.i += 1; v += self.term()?; } else if c == '-' { self.i += 1; v -= self.term()?; } else { break }
            }
            Some(v)
        }
        fn term(&mut self) -> Option<f64> {
            let mut v = self.pow()?;
            while let Some(c) = self.peek() {
                match c { '*' => { self.i += 1; v *= self.pow()? } '/' => { self.i += 1; v /= self.pow()? } '%' => { self.i += 1; v %= self.pow()? } _ => break }
            }
            Some(v)
        }
        fn pow(&mut self) -> Option<f64> {
            let b = self.unary()?;
            if self.peek() == Some('^') { self.i += 1; let e = self.pow()?; return Some(b.powf(e)); }
            Some(b)
        }
        fn unary(&mut self) -> Option<f64> {
            match self.peek() {
                Some('-') => { self.i += 1; Some(-self.unary()?) }
                Some('+') => { self.i += 1; self.unary() }
                Some('(') => { self.i += 1; let v = self.expr()?; if self.peek() == Some(')') { self.i += 1; } Some(v) }
                _ => {
                    let st = self.i;
                    while self.peek().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false) { self.i += 1; }
                    self.t[st..self.i].iter().collect::<String>().parse().ok()
                }
            }
        }
    }
    let mut p = P { t: toks, i: 0 };
    let v = p.expr()?;
    if p.i != p.t.len() || !v.is_finite() {
        return None;
    }
    let r = (v * 1e8).round() / 1e8;
    Some(if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }.replace('.', ","))
}

fn bin() -> String {
    crate::paths::self_bin()
}
fn commands() -> Vec<(&'static str, &'static str, &'static str, Vec<String>)> {
    let m = |a: &[&str]| -> Vec<String> { let mut v = vec![bin(), "msg".into()]; v.extend(a.iter().map(|s| s.to_string())); v };
    vec![
        ("zablokuj lock ekran", "Zablokuj ekran", "lock", vec![bin(), "lock".into()]),
        ("wylacz uruchom ponownie restart reboot shutdown uspij sleep wyloguj logout sesja", "Wyłącz / uruchom ponownie…", "power", m(&["mist", "toggle", "session"])),
        ("tapeta wallpaper tlo", "Zmień tapetę", "photo", m(&["mist", "toggle", "wallpaper"])),
        ("schowek clipboard historia", "Historia schowka", "clipboard", m(&["mist", "toggle", "clipboard"])),
        ("zrzut ekranu screenshot", "Zrzut ekranu", "screenshot", m(&["shot", "toolbar"])),
        ("powiadomienia notifications", "Centrum powiadomień", "bell", m(&["mist", "toggle", "notifications"])),
        ("ustawienia settings centrum sterowania control", "Centrum sterowania", "settings", m(&["mist", "toggle", "control"])),
        ("ustawienia settings wyglad qt motyw theme", "Wygląd aplikacji Qt", "settings", vec!["qt6ct".into()]),
        ("ustawienia settings wyglad gtk motyw theme", "Wygląd aplikacji GTK", "settings", vec!["nwg-look".into()]),
    ]
}

const CATEGORIES: &[(&str, &str, &str)] = &[
    ("Wszystkie", "", ""),
    ("😀", "people", "Ludzie i emocje"),
    ("🐱", "animals", "Zwierzęta i natura"),
    ("🍔", "food", "Jedzenie i napoje"),
    ("⚽", "activity", "Aktywności"),
    ("✈️", "travel", "Podróże i miejsca"),
    ("💡", "objects", "Przedmioty"),
    ("🔣", "symbols", "Symbole"),
    ("🚩", "flags", "Flagi"),
];

pub struct Spotlight {
    field: Field,
    sel: usize,
    cat_sel: usize,
    cat_rows: Vec<((f32, f32, f32, f32), usize)>,
    usage: Value,
    emoji: Vec<Value>,
    h: Anim,
    ptr: (f32, f32),
    rows: Vec<((f32, f32, f32, f32), usize)>,
    card: (f32, f32, f32, f32),
    pressed_outside: bool,
    etop: usize,
    // opened as "emoji" (Super+.): the same name toggles it closed
    as_emoji: bool,
}

fn usage_path() -> String {
    format!("{}/.config/mist/usage.json", std::env::var("HOME").unwrap_or_default())
}

impl Spotlight {
    pub fn new(emoji_mode: bool) -> Spotlight {
        let usage = std::fs::read_to_string(usage_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let emoji = std::fs::read_to_string(crate::paths::mist_path("native/assets/emoji.json")).or_else(|_| std::fs::read_to_string("/usr/share/noctalia/assets/emoji.json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        Spotlight { field: Field { text: if emoji_mode { ":".into() } else { String::new() } }, sel: 0, cat_sel: 0, cat_rows: Vec::new(), usage, emoji, h: Anim::new(60.0), ptr: (-1.0, -1.0), rows: Vec::new(), card: (0.0, 0.0, 0.0, 0.0), pressed_outside: false, etop: 0, as_emoji: emoji_mode }
    }
    fn emoji_mode(&self) -> bool {
        self.field.text.starts_with(':')
    }
    fn results(&self, apps: &crate::apps::Apps) -> Vec<Res> {
        let raw = self.field.text.trim();
        if raw.is_empty() || self.emoji_mode() {
            return vec![];
        }
        let q = norm(raw);
        let mut out = Vec::new();
        if let Some(v) = calc(raw) {
            out.push(Res { kind: Kind::Calc(v.clone()), title: v, sub: format!("{raw} ="), icon: "calc".into() });
        }
        let mut scored: Vec<(f64, usize)> = Vec::new();
        for (i, a) in apps.entries.iter().enumerate() {
            if a.no_display || a.exec.is_empty() {
                continue;
            }
            let (n, g, k, id) = (norm(&a.name), norm(&a.generic), norm(&a.keywords), norm(&a.id));
            let s = if n.starts_with(&q) { 100.0 } else if n.split_whitespace().any(|w| w.starts_with(&q)) { 80.0 } else if n.contains(&q) { 60.0 } else if g.contains(&q) || id.contains(&q) { 40.0 } else if k.contains(&q) { 30.0 } else { 0.0 };
            if s > 0.0 {
                let u = self.usage[&a.id]["n"].as_f64().unwrap_or(0.0);
                scored.push((s - n.chars().count() as f64 / 100.0 + (u * 2.5).min(25.0), i));
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, i) in scored.into_iter().take(7) {
            let a = &apps.entries[i];
            let sub = if !a.generic.is_empty() { a.generic.clone() } else if !a.comment.is_empty() { a.comment.clone() } else { "Aplikacja".into() };
            out.push(Res { kind: Kind::App(i), title: a.name.clone(), sub, icon: a.icon.clone() });
        }
        for (k, title, icon, cmd) in commands() {
            if k.split(' ').any(|w| w.starts_with(&q)) {
                out.push(Res { kind: Kind::Cmd(cmd), title: title.into(), sub: "Polecenie".into(), icon: icon.into() });
            }
        }
        if q.chars().count() >= 2 {
            // an exact name first ("fire" -> 🔥, not 🧑‍🚒 firefighter), then a prefix / keyword
            let exact = self.emoji.iter().find(|e| norm(e["name"].as_str().unwrap_or("")) == q);
            if let Some(e) = exact.or_else(|| self.emoji.iter().find(|e| norm(e["name"].as_str().unwrap_or("")).starts_with(&q) || e["keywords"].as_array().map(|a| a.iter().any(|w| norm(w.as_str().unwrap_or("")) == q)).unwrap_or(false))) {
                let g = e["emoji"].as_str().unwrap_or("").to_string();
                out.push(Res { kind: Kind::Emoji(g.clone()), title: format!("{g}  {}", e["name"].as_str().unwrap_or("")), sub: "Emoji · Enter kopiuje".into(), icon: g });
            }
        }
        out.push(Res { kind: Kind::Web(raw.into()), title: format!("Szukaj w sieci: {raw}"), sub: "Google".into(), icon: "world".into() });
        out
    }
    fn emoji_results(&self) -> Vec<&Value> {
        if !self.emoji_mode() {
            return vec![];
        }
        let q = norm(self.field.text[1..].trim());
        let cat_key = CATEGORIES.get(self.cat_sel).map(|c| c.1).unwrap_or("");
        self.emoji
            .iter()
            .filter(|e| {
                if !cat_key.is_empty() {
                    let ec = e["category"].as_str().unwrap_or("");
                    let matches_cat = if cat_key == "animals" {
                        ec == "animals" || ec == "nature"
                    } else {
                        ec == cat_key
                    };
                    if !matches_cat {
                        return false;
                    }
                }
                if q.is_empty() {
                    return true;
                }
                norm(e["name"].as_str().unwrap_or("")).contains(&q)
                    || e["keywords"]
                        .as_array()
                        .map(|a| a.iter().any(|w| norm(w.as_str().unwrap_or("")).starts_with(&q)))
                        .unwrap_or(false)
            })
            .collect()
    }
    fn count(&self, apps: &crate::apps::Apps) -> usize {
        if self.emoji_mode() { self.emoji_results().len() } else { self.results(apps).len() }
    }
    fn activate(&mut self, i: usize, apps: &crate::apps::Apps) -> Act {
        let copy = |t: &str| Act::Run(vec!["wl-copy".into(), "--".into(), t.into()]);
        let type_emoji = |t: &str| {
            let script = format!(
                "wl-copy --type text/plain -- '{t}'; sleep 0.05; cls=$(hyprctl activewindow -j 2>/dev/null | jq -r .class 2>/dev/null); case \"$cls\" in kitty|alacritty|foot|wezterm|konsole|gnome-terminal|xterm) m=\"CTRL SHIFT\" ;; *) m=\"CTRL\" ;; esac; hyprctl dispatch \"hl.dsp.send_shortcut({{ mods = \\\"$m\\\", key = \\\"V\\\", window = \\\"activewindow\\\" }})\""
            );
            Act::Run(vec!["sh".into(), "-c".into(), script])
        };
        if self.emoji_mode() {
            return self.emoji_results().get(i).map(|e| type_emoji(e["emoji"].as_str().unwrap_or(""))).unwrap_or(Act::None);
        }
        let Some(r) = self.results(apps).get(i).cloned() else { return Act::None };
        match r.kind {
            Kind::App(ix) => {
                let a = &apps.entries[ix];
                // usage, shared with Start
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                if !self.usage.is_object() {
                    self.usage = serde_json::json!({});
                }
                let n = self.usage[&a.id]["n"].as_i64().unwrap_or(0) + 1;
                self.usage[&a.id] = serde_json::json!({ "n": n, "t": now });
                let _ = std::fs::write(usage_path(), self.usage.to_string());
                Act::Launch(ix)
            }
            Kind::Calc(v) => copy(&v),
            Kind::Emoji(g) => type_emoji(&g),
            Kind::Cmd(c) => Act::Run(c),
            Kind::Web(q) => {
                let enc: String = q.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
                Act::Run(vec!["xdg-open".into(), format!("https://www.google.com/search?q={enc}")])
            }
        }
    }
}

impl Pane for Spotlight {
    fn name(&self) -> &'static str {
        if self.as_emoji { "emoji" } else { "spotlight" }
    }
    fn namespace(&self) -> &'static str {
        "mist-spotlight"
    }
    fn animating(&self) -> bool {
        self.h.running()
    }
    fn seed(&mut self, q: &str) {
        self.field.text = q.into();
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        c.clear();
        self.rows.clear();
        let emode = self.emoji_mode();
        let res = self.results(cx.apps);
        let ems: Vec<Value> = self.emoji_results().into_iter().cloned().collect();
        let n = if emode { ems.len() } else { res.len() };
        let w = 680.0;
        let x0 = ((c.w() - w) / 2.0).round();
        let y0 = (c.h() * 0.31).round();
        let cell = ((w - 16.0 - 8.0) / 10.0).floor();
        let cat_bar_h = if emode { 38.0 } else { 0.0 };
        let grid_max_h = 340.0;
        let grid_h = if emode {
            if n > 0 {
                (((n + 9) / 10) as f32 * cell).min(grid_max_h).max(cell)
            } else {
                cell
            }
        } else {
            0.0
        };
        let body_h = if emode {
            cat_bar_h + grid_h + 8.0
        } else {
            n as f32 * 48.0 + 4.0
        };
        let want = 60.0 + if n > 0 || emode { body_h + 9.0 } else { 0.0 };
        if (self.h.target() - want).abs() > 0.5 {
            self.h.set(want, 160.0, Ease::OutCubic);
        }
        let h = self.h.value();
        self.card = (x0, y0, w, h);
        glass(c, x0, y0, w, h, 30.0, 0.74);
        // the capsule
        c.icon(t, x0 + 22.0 + 11.0, y0 + 30.0, "search", 22.0, T2);
        let fs = Style::ui(24.0, 400.0);
        let fh = crate::desktop::line_h(t, &fs);
        let fx = x0 + 22.0 + 22.0 + 14.0;
        let fw = w - (fx - x0) - 22.0;
        let tw = if self.field.text.is_empty() {
            crate::desktop::text(c, t, fx, y0 + (60.0 - fh) / 2.0, "Szukaj w Spotlight", &fs, T3, false, Some(fw));
            0.0
        } else {
            crate::desktop::text(c, t, fx, y0 + (60.0 - fh) / 2.0, &self.field.text, &fs, T1, false, Some(fw))
        };
        crate::desktop::rect(c, fx + tw + 1.0, y0 + 17.0, 1.5, 26.0, 0.0, T1);
        if (n > 0 || emode) && h > 62.0 {
            crate::desktop::rect(c, x0 + 16.0, y0 + 60.0, w - 32.0, 1.0, 0.0, HAIR);
        }
        // the body, clipped to the animated sheet
        let mut l = Canvas::new((w - 16.0) as u32, body_h.max(1.0) as u32);
        let (bx, by) = (x0 + 8.0, y0 + 65.0);
        let lp = (self.ptr.0 - bx, self.ptr.1 - by);
        if emode {
            self.cat_rows.clear();
            let n_cats = CATEGORIES.len();
            let pad = 4.0;
            let tab_gap = 4.0;
            let avail_w = w - 16.0 - pad * 2.0;
            let tab_w = ((avail_w - (n_cats as f32 - 1.0) * tab_gap) / n_cats as f32).floor();
            let tab_h = 28.0;
            for (ci, (label, _, _)) in CATEGORIES.iter().enumerate() {
                let tx = pad + ci as f32 * (tab_w + tab_gap);
                let ty = 4.0;
                let is_sel = ci == self.cat_sel;
                let is_hov = inside(lp, (tx, ty, tab_w, tab_h));
                if is_sel {
                    crate::desktop::rect(&mut l, tx, ty, tab_w, tab_h, 8.0, SOFT);
                } else if is_hov {
                    crate::desktop::rect(&mut l, tx, ty, tab_w, tab_h, 8.0, FILL);
                }
                if ci == 0 {
                    let ts = Style::ui(11.0, 500.0);
                    let tw = t.width(label, &ts);
                    let th = crate::desktop::line_h(t, &ts);
                    crate::desktop::text(&mut l, t, tx + (tab_w - tw) / 2.0, ty + (tab_h - th) / 2.0, label, &ts, if is_sel { T1 } else { T2 }, false, None);
                } else {
                    let ts = Style::ui(16.0, 400.0);
                    let tw = t.width(label, &ts);
                    let th = crate::desktop::line_h(t, &ts);
                    crate::desktop::text(&mut l, t, tx + (tab_w - tw) / 2.0, ty + (tab_h - th) / 2.0, label, &ts, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
                }
                self.cat_rows.push(((bx + tx, by + ty, tab_w, tab_h), ci));
            }
            crate::desktop::rect(&mut l, 4.0, cat_bar_h - 2.0, w - 24.0, 1.0, 0.0, HAIR);

            let cols = 10usize;
            let grid_top = cat_bar_h + 4.0;
            let rows_vis = ((grid_h) / cell).floor().max(1.0) as usize;
            let row_sel = self.sel / cols;
            if row_sel < self.etop {
                self.etop = row_sel;
            }
            if row_sel >= self.etop + rows_vis {
                self.etop = row_sel + 1 - rows_vis;
            }
            for (i, e) in ems.iter().enumerate() {
                let (r, col) = (i / cols, i % cols);
                if r < self.etop || r >= self.etop + rows_vis {
                    continue;
                }
                let (ex, ey) = (4.0 + col as f32 * cell, grid_top + (r - self.etop) as f32 * cell);
                let hov = inside(lp, (ex, ey, cell - 4.0, cell - 4.0));
                if i == self.sel {
                    crate::desktop::rect(&mut l, ex, ey, cell - 4.0, cell - 4.0, 12.0, SOFT);
                } else if hov {
                    crate::desktop::rect(&mut l, ex, ey, cell - 4.0, cell - 4.0, 12.0, FILL);
                }
                let g = e["emoji"].as_str().unwrap_or("");
                let es = Style::ui(30.0, 400.0);
                let gw = t.width(g, &es);
                let gh = crate::desktop::line_h(t, &es);
                crate::desktop::text(&mut l, t, ex + (cell - 4.0 - gw) / 2.0, ey + (cell - 4.0 - gh) / 2.0, g, &es, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
                self.rows.push(((bx + ex, by + ey, cell - 4.0, cell - 4.0), i));
            }
        } else {
            for (i, r) in res.iter().enumerate() {
                let ry = i as f32 * 48.0;
                let on = i == self.sel;
                let rw = w - 16.0;
                if on {
                    crate::desktop::rect(&mut l, 0.0, ry, rw, 48.0, 12.0, SOFT);
                } else if inside(lp, (0.0, ry, rw, 48.0)) {
                    crate::desktop::rect(&mut l, 0.0, ry, rw, 48.0, 12.0, FILL);
                }
                let (ix, iy) = (10.0, ry + 8.0);
                match &r.kind {
                    Kind::App(_) => {
                        let pm = cx.imgs.get(&r.icon, 32, cx.icons).or_else(|| cx.imgs.get("application-x-executable", 32, cx.icons));
                        if let Some(pm) = pm {
                            crate::img::draw(&mut l, &pm, ix, iy, 32.0, 32.0, 0.0, false, 1.0);
                        }
                    }
                    Kind::Emoji(g) => {
                        crate::desktop::rect(&mut l, ix, iy, 32.0, 32.0, 9.0, FILL2);
                        let es = Style::ui(20.0, 400.0);
                        let gw = t.width(g, &es);
                        let gh = crate::desktop::line_h(t, &es);
                        crate::desktop::text(&mut l, t, ix + 16.0 - gw / 2.0, iy + 16.0 - gh / 2.0, g, &es, Rgba(1.0, 1.0, 1.0, 1.0), false, None);
                    }
                    Kind::Calc(_) => {
                        crate::desktop::rect(&mut l, ix, iy, 32.0, 32.0, 9.0, Rgba(236.0 / 255.0, 212.0 / 255.0, 142.0 / 255.0, 0.18));
                        l.icon(t, ix + 16.0, iy + 16.0, &r.icon, 18.0, YELLOW);
                    }
                    _ => {
                        crate::desktop::rect(&mut l, ix, iy, 32.0, 32.0, 9.0, FILL2);
                        l.icon(t, ix + 16.0, iy + 16.0, &r.icon, 18.0, T1);
                    }
                }
                let ts = Style::ui(if matches!(r.kind, Kind::Calc(_)) { 18.0 } else { 15.0 }, 560.0);
                let ss = Style::ui(12.0, 450.0);
                let (h1, h2) = (crate::desktop::line_h(t, &ts), crate::desktop::line_h(t, &ss));
                let ty = ry + (48.0 - h1 - h2) / 2.0;
                let tx = ix + 32.0 + 12.0;
                let maxw = rw - tx - 14.0 - 24.0;
                crate::desktop::text(&mut l, t, tx, ty, &r.title, &ts, T1, false, Some(maxw));
                crate::desktop::text(&mut l, t, tx, ty + h1, &r.sub, &ss, T3, false, Some(maxw));
                if on {
                    let hs = Style::ui(15.0, 450.0);
                    let hw = t.width("↵", &hs);
                    let hh = crate::desktop::line_h(t, &hs);
                    crate::desktop::text(&mut l, t, rw - 14.0 - hw, ry + (48.0 - hh) / 2.0, "↵", &hs, T3, false, None);
                }
                self.rows.push(((bx, by + ry, rw, 48.0), i));
            }
        }
        let clip_h = (h - 65.0).max(0.0);
        if clip_h > 0.0 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(bx, by, w - 16.0, clip_h) {
                let mut mask = tiny_skia::Mask::new(c.pm.width(), c.pm.height()).unwrap();
                mask.fill_path(&tiny_skia::PathBuilder::from_rect(rect), tiny_skia::FillRule::Winding, false, tiny_skia::Transform::identity());
                c.pm.draw_pixmap(bx as i32, by as i32, l.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), Some(&mask));
            }
        }
        glass_rim(c, x0, y0, w, h, 30.0);
        // the selected emoji's name under the grid
        if emode {
            if let Some(e) = ems.get(self.sel) {
                let lh13 = lh(t, 13.0, 450.0);
                let _ = lh13;
                let name = e["name"].as_str().unwrap_or("");
                let cat_name = CATEGORIES.get(self.cat_sel).map(|c| c.2).unwrap_or("");
                let label = if cat_name.is_empty() {
                    name.to_string()
                } else {
                    format!("{name}  ·  {cat_name}")
                };
                txt_c(c, t, x0 + w / 2.0, y0 + h + 10.0, &label, 13.0, 450.0, T2, w);
            }
        }
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let cat_hit = self.cat_rows.iter().find(|r| inside((x, y), r.0)).map(|r| r.1);
        let row = self.rows.iter().find(|r| inside((x, y), r.0)).map(|r| r.1);
        match ev {
            Ptr::Motion => {
                if let Some(r) = row {
                    if !self.emoji_mode() && r != self.sel {
                        self.sel = r;
                    }
                }
                Act::Redraw
            }
            Ptr::Press(_) => {
                self.pressed_outside = !inside((x, y), self.card);
                Act::None
            }
            Ptr::Release(_) => {
                if let Some(ci) = cat_hit {
                    if self.cat_sel != ci {
                        self.cat_sel = ci;
                        self.sel = 0;
                        self.etop = 0;
                        return Act::Redraw;
                    }
                }
                match row {
                    Some(r) => self.activate(r, cx.apps),
                    None if self.pressed_outside => Act::Close,
                    _ => Act::None,
                }
            }
            Ptr::Wheel(dy) if self.emoji_mode() => {
                let n = self.count(cx.apps);
                self.sel = ((self.sel as i32 + 10 * dy.signum() as i32).max(0) as usize).min(n.saturating_sub(1));
                Act::Redraw
            }
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        let n = self.count(cx.apps);
        let cols = if self.emoji_mode() { 10 } else { 1 };
        match ev.keysym {
            Keysym::Escape => Act::Close,
            Keysym::Tab if self.emoji_mode() => {
                self.cat_sel = (self.cat_sel + 1) % CATEGORIES.len();
                self.sel = 0;
                self.etop = 0;
                Act::Redraw
            }
            Keysym::ISO_Left_Tab if self.emoji_mode() => {
                self.cat_sel = if self.cat_sel == 0 { CATEGORIES.len() - 1 } else { self.cat_sel - 1 };
                self.sel = 0;
                self.etop = 0;
                Act::Redraw
            }
            Keysym::Down => {
                self.sel = (self.sel + cols).min(n.saturating_sub(1));
                Act::Redraw
            }
            Keysym::Up => {
                self.sel = self.sel.saturating_sub(cols);
                Act::Redraw
            }
            Keysym::Right if self.emoji_mode() => {
                self.sel = (self.sel + 1).min(n.saturating_sub(1));
                Act::Redraw
            }
            Keysym::Left if self.emoji_mode() => {
                self.sel = self.sel.saturating_sub(1);
                Act::Redraw
            }
            Keysym::Return | Keysym::KP_Enter => self.activate(self.sel, cx.apps),
            _ => {
                if self.field.key(ev, cx.ctrl) {
                    self.sel = 0;
                    self.etop = 0;
                    Act::Redraw
                } else {
                    Act::None
                }
            }
        }
    }
}
