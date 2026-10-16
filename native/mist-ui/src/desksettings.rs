// "Dostosuj pulpit": Mist's settings in one sheet. Tab Ogólne: the UI language, the weather place (searched with
// `mistd geo`, saved to ~/.config/mist/location.json), the wallpaper gallery; it opens by itself on the first
// start (settings.json "welcomed"). Then every desktop widget setting (right click on the desktop's cards or the
// clock, or `msg mist toggleOn desk <screen>`). Tab Układ: how many cards, which widget in which place, the
// clock / date / sun path, the cards' glass; one tab per widget with what it shows (the same options as its
// adjust button). The look of the cards' backs (dark sheet, chips); a label column, options on the right;
// the body scrolls under the fixed header + tabs. Every click is saved to ~/.config/mist/widgets.json and
// applied at once (Act::DeskCfg), the desktop stays visible.
use crate::anim::{Anim, Ease};
use crate::desktop::{border, chip, chip_w, groups, line_h, rect, st, text, width, Cfg, FAINT, FG, FG2, HAIR, MUTED, WHITE};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const TABS: [(&str, &str); 5] = [("general", "Ogólne"), ("desk", "Układ"), ("garmin", "Garmin"), ("weather", "Pogoda"), ("running", "Bieganie")];
const WIDGETS: [(&str, &str); 3] = [("garmin", "Garmin"), ("weather", "Pogoda"), ("running", "Bieganie")];
const W: f32 = 620.0;
const M: f32 = 40.0; // the layer's room for the sheet's shadow
const TOP: f32 = 108.0; // header + tabs (fixed); the body scrolls under them
const LX: f32 = 24.0; // label column
const CX: f32 = 196.0; // options column
const R: f32 = 24.0;

#[derive(Clone, PartialEq, Debug)]
enum H {
    Tab(usize),
    Opt(&'static str, &'static str, Value),
    Slot(usize, &'static str),
    Show(&'static str),
    Lang(&'static str),
    Field,
    Place(usize),
    Gallery,
    Done,
    Sheet,
}

pub struct DeskSettings {
    cfg: Cfg,
    tab: usize,
    ptr: (f32, f32),
    hits: Vec<(f32, f32, f32, f32, H)>,
    hot: Option<H>, // under the pointer at the start of the paint (the hits are rebuilt while painting)
    scroll: f32,
    max_scroll: f32,
    o: Anim,
    // the weather place search: the field, its answers, the search running in a thread
    place: crate::edit::LineEdit,
    editing: bool,
    found: Vec<Value>,
    asked: String,
    geo_rx: Option<std::sync::mpsc::Receiver<Vec<Value>>>,
    welcome: bool,
}

// one row: a label on the left, its chips wrapped in the options column (under a text field when `field`)
struct Row {
    label: String,
    items: Vec<(String, bool, H)>,
    field: bool,
}

fn settings_path() -> String {
    format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default())
}
// first start: nothing saved yet about the welcome
pub fn first_run() -> bool {
    let v: Value = std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    !v["welcomed"].as_bool().unwrap_or(false)
}
fn mark_welcomed() {
    let p = settings_path();
    let mut v: Value = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::json!({}));
    v["welcomed"] = serde_json::json!(true);
    let _ = std::fs::create_dir_all(std::path::Path::new(&p).parent().unwrap());
    let tmp = format!("{p}.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}
fn location_path() -> String {
    format!("{}/.config/mist/location.json", std::env::var("HOME").unwrap_or_default())
}
fn current_place() -> String {
    std::fs::read_to_string(location_path()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["city"].as_str().map(String::from)).unwrap_or_default()
}
fn place_label(v: &Value) -> String {
    let mut s = v["city"].as_str().unwrap_or("").to_string();
    for k in ["region", "country"] {
        if let Some(x) = v[k].as_str().filter(|x| !x.is_empty() && *x != s) {
            s.push_str(", ");
            s.push_str(x);
        }
    }
    s
}

impl DeskSettings {
    // opened on a card: its tab first
    pub fn new(card: &str) -> DeskSettings {
        let mut o = Anim::new(0.0);
        o.set(1.0, 140.0, Ease::OutCubic);
        let welcome = first_run();
        let tab = if welcome { 0 } else { TABS.iter().position(|t| t.0 == card).unwrap_or(1) };
        if welcome {
            mark_welcomed();
        }
        let editing = welcome && current_place().is_empty();
        DeskSettings { cfg: Cfg::load(), tab, ptr: (-1.0, -1.0), hits: Vec::new(), hot: None, scroll: 0.0, max_scroll: 0.0, o, place: Default::default(), editing, found: Vec::new(), asked: String::new(), geo_rx: None, welcome }
    }
    fn hit(&self) -> Option<H> {
        let p = self.ptr;
        self.hits.iter().rev().find(|r| p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3).map(|r| r.4.clone())
    }
    fn row(label: &str, card: &'static str, key: &'static str, cfg: &Cfg) -> Row {
        let gr = groups(card).into_iter().find(|g| g.key == key);
        let cur = cfg.opt(card, key);
        let items = gr.as_ref().map(|gr| gr.opts.iter().map(|(v, lab)| {
            let on = if gr.multi { cur.as_array().map(|a| a.contains(v)).unwrap_or(false) } else { &cur == v };
            (lab.to_string(), on, H::Opt(card, gr.key, v.clone()))
        }).collect()).unwrap_or_default();
        Row { label: label.to_string(), items, field: false }
    }
    // start a place search in a thread (the answer arrives in slow())
    fn search(&mut self) {
        let q = self.place.text.trim().to_string();
        if q.chars().count() < 2 || q == self.asked {
            return;
        }
        self.asked = q.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.geo_rx = Some(rx);
        let lang = if crate::i18n::setting() == "pl" || std::env::var("LANG").unwrap_or_default().starts_with("pl") { "pl" } else { "en" };
        let _ = std::thread::Builder::new().name("geo".into()).spawn(move || {
            let out = std::process::Command::new(crate::paths::mistd()).args(["geo", &q, lang]).stderr(std::process::Stdio::null()).output();
            let v: Vec<Value> = out.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or_default();
            let _ = tx.send(v);
        });
    }
    fn choose(&mut self, i: usize) {
        let Some(v) = self.found.get(i) else { return };
        let loc = serde_json::json!({"city": v["city"], "lat": v["lat"], "lon": v["lon"]});
        let p = location_path();
        let tmp = format!("{p}.tmp");
        if std::fs::write(&tmp, loc.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
        self.found.clear();
        self.editing = false;
        self.place.set("");
        self.asked.clear();
        crate::data::refresh("weather");
    }
    // the UI language (i18n.rs); language names stay in their own language
    fn lang_row() -> Row {
        let cur = crate::i18n::setting();
        let mut items = vec![("Automatycznie".to_string(), cur == "auto", H::Lang("auto"))];
        items.extend(crate::i18n::LANGS.iter().map(|(code, name)| (name.to_string(), cur == *code, H::Lang(code))));
        Row { label: "Język".into(), items, field: false }
    }
    // the tab's content: sections (title, rows)
    fn sections(&self) -> Vec<(&'static str, Vec<Row>)> {
        let cfg = &self.cfg;
        let card = TABS[self.tab].0;
        if card == "general" {
            let cur = current_place();
            let mut place = Row { label: "Miejsce pogody".into(), items: Vec::new(), field: true };
            place.items = self.found.iter().enumerate().map(|(i, v)| (place_label(v), false, H::Place(i))).collect();
            if !self.editing && !cur.is_empty() {
                place.field = false;
                place.items = vec![(cur, true, H::Field)];
            }
            return vec![
                ("Język i miejsce", vec![Self::lang_row(), place]),
                ("Tapeta", vec![Row { label: "Tło pulpitu".into(), items: vec![("Wybierz tapetę…".into(), false, H::Gallery)], field: false }]),
            ];
        }
        if card == "desk" {
            let order = cfg.order();
            let mut cards = vec![Self::row("Liczba kart", "desk", "cards", cfg)];
            for i in 0..cfg.cards() {
                let items = WIDGETS.iter().filter(|(k, _)| crate::data::available(k)).map(|(k, lab)| (lab.to_string(), order.get(i).map(|o| o == k).unwrap_or(false), H::Slot(i, k))).collect();
                cards.push(Row { label: format!("Miejsce {}", i + 1), items, field: false });
            }
            return vec![
                ("Karty", cards),
                ("Zegar", vec![Self::row("Rozmiar", "desk", "clock", cfg), Self::row("Data", "desk", "date", cfg), Self::row("Wschód i zachód", "desk", "sun", cfg)]),
                ("Wygląd", vec![Self::row("Tło kart", "desk", "glass", cfg), Self::lang_row()]),
            ];
        }
        let rows = groups(card).into_iter().map(|gr| {
            let label = if gr.multi && gr.max > 0 { format!("{} (maks. {})", gr.label, gr.max) } else { gr.label.to_string() };
            let mut r = Self::row(&label, card, gr.key, cfg);
            r.label = label;
            r
        }).collect();
        vec![("Dane na karcie", rows)]
    }
}

impl Pane for DeskSettings {
    fn name(&self) -> &'static str {
        "desk"
    }
    fn animating(&self) -> bool {
        self.o.running()
    }
    // the place search answered
    fn slow(&mut self) -> bool {
        let Some(rx) = &self.geo_rx else { return false };
        match rx.try_recv() {
            Ok(v) => {
                self.found = v;
                self.geo_rx = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.geo_rx = None;
                true
            }
            Err(_) => false,
        }
    }
    // `--pane-png desk:general … <query>`: the field typed and searched (blocking, for the check)
    fn seed(&mut self, q: &str) {
        self.tab = 0;
        self.editing = true;
        self.place.set(q);
        self.search();
        if let Some(rx) = self.geo_rx.take() {
            self.found = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_default();
        }
    }
    fn hand(&self) -> bool {
        !matches!(self.hit(), None | Some(H::Sheet))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hot = self.hit();
        self.hits.clear();
        let hot = self.hot.clone();
        let (ox, oy) = ((c.w() - W - 40.0).round(), 64.0);
        let card = TABS[self.tab].0;

        // ---- the body (scrolls): sections of label | chips rows
        let mut body = Canvas::new(W as u32, 1600);
        let mut bh: Vec<(f32, f32, f32, f32, H)> = Vec::new();
        let mut y = 4.0;
        if self.welcome && card == "general" {
            let ws = st(13.0, 500.0);
            text(&mut body, cx.t, LX, y, "Witaj w Mist. Wybierz język i miejsce pogody.", &ws, FG, true, Some(W - 2.0 * LX));
            y += line_h(cx.t, &ws) + 4.0;
            text(&mut body, cx.t, LX, y, "Ten arkusz otworzysz później prawym klikiem na zegarze pulpitu.", &st(12.0, 500.0), MUTED, true, Some(W - 2.0 * LX));
            y += line_h(cx.t, &st(12.0, 500.0)) + 18.0;
        }
        if !matches!(card, "desk" | "general") && !self.cfg.shown().iter().any(|k| k == card) {
            // a hidden card: say so, offer to show it
            rect(&mut body, LX, y, W - 2.0 * LX, 48.0, 14.0, WHITE.alpha(0.05));
            let ns = st(13.0, 550.0);
            let nh = line_h(cx.t, &ns);
            text(&mut body, cx.t, LX + 16.0, y + (48.0 - nh) / 2.0, "Ta karta jest ukryta", &ns, FG2, true, None);
            let pw = chip_w(cx.t, "Pokaż na pulpicie", true);
            let px = W - LX - 11.0 - pw;
            chip(&mut body, cx.t, px, y + 11.0, "Pokaż na pulpicie", true, hot == Some(H::Show(card)), 1.0);
            bh.push((px, y + 11.0, pw, 26.0, H::Show(card)));
            y += 48.0 + 18.0;
        }
        let ls = st(12.5, 550.0);
        let lsh = line_h(cx.t, &ls);
        let caps_s = st(11.0, 650.0).spaced(1.2);
        for (si, (title, rows)) in self.sections().into_iter().enumerate() {
            if si > 0 {
                rect(&mut body, LX, y, W - 2.0 * LX, 1.0, 0.0, HAIR);
                y += 16.0;
            }
            text(&mut body, cx.t, LX, y, &title.to_uppercase(), &caps_s, FAINT, true, None);
            y += line_h(cx.t, &caps_s) + 12.0;
            for r in rows {
                text(&mut body, cx.t, LX, y + (26.0 - lsh) / 2.0, &r.label, &ls, MUTED, true, Some(CX - LX - 12.0));
                let mut x = CX;
                let mut cy = y;
                if r.field {
                    // the search field: Enter or a pause searches, the answers come as chips under it
                    let fw = W - LX - CX;
                    rect(&mut body, CX, cy, fw, 28.0, 14.0, WHITE.alpha(if self.editing { 0.10 } else { 0.06 }));
                    if self.editing {
                        border(&mut body, CX, cy, fw, 28.0, 14.0, 1.0, crate::theme::ACCENT.alpha(0.6));
                    }
                    let fs = st(13.0, 500.0);
                    let hint = if self.geo_rx.is_some() { "Szukam…" } else { "Wpisz miasto i naciśnij Enter" };
                    self.place.paint(&mut body, cx.t, CX + 12.0, cy, fw - 24.0, 28.0, &fs, FG, hint, self.editing, false);
                    bh.push((CX, cy, fw, 28.0, H::Field));
                    cy += 28.0 + 8.0;
                    if r.items.is_empty() && !self.asked.is_empty() && self.geo_rx.is_none() {
                        text(&mut body, cx.t, CX + 4.0, cy + 2.0, "Nic nie znaleziono", &st(12.0, 500.0), MUTED, true, None);
                        cy += 22.0;
                    }
                }
                let ends_at_field = r.field && r.items.is_empty();
                for (lab, on, h) in r.items {
                    let cw = chip_w(cx.t, &lab, on);
                    if x > CX && x + cw > W - LX {
                        x = CX;
                        cy += 32.0;
                    }
                    chip(&mut body, cx.t, x, cy, &lab, on, hot == Some(h.clone()), 1.0);
                    bh.push((x, cy, cw, 26.0, h));
                    x += cw + 6.0;
                }
                y = if ends_at_field { cy + 4.0 } else { cy + 26.0 + 12.0 };
            }
            y += 6.0;
        }
        let content_h = y + 8.0;

        // ---- the sheet: fixed header + tabs, the body clipped under them
        let max_h = (c.h() - oy - 24.0).max(TOP + 80.0);
        let h = (TOP + content_h).min(max_h);
        let view = h - TOP;
        self.max_scroll = (content_h - view).max(0.0);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll);
        let mut l = Canvas::new((W + 2.0 * M) as u32, (h + 2.0 * M) as u32);
        // the cards' back: dark, nearly opaque, hairline rim, soft shadow
        {
            let mut s = Canvas::new((W + 2.0 * M) as u32, (h + 2.0 * M) as u32);
            rect(&mut s, M, M, W, h, R, Rgba(0.0, 0.0, 0.0, 1.0));
            let sh = crate::desktop::drop_shadow(&s.pm, 14.0, Rgba(0.0, 0.0, 0.0, 1.0), 0.45, 0, 8);
            l.pm.draw_pixmap(0, 0, sh.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        }
        rect(&mut l, M, M, W, h, R, Rgba(10.0 / 255.0, 14.0 / 255.0, 24.0 / 255.0, 0.96));
        border(&mut l, M, M, W, h, R, 1.0, WHITE.alpha(0.1));
        // header: title · Gotowe
        let ts = st(17.0, 650.0);
        let tsh = line_h(cx.t, &ts);
        text(&mut l, cx.t, M + LX, M + 18.0 + (30.0 - tsh) / 2.0, "Dostosuj pulpit", &ts, FG, true, None);
        let dw = chip_w(cx.t, "Gotowe", true);
        let (dx, dy) = (M + W - LX - dw, M + 20.0);
        chip(&mut l, cx.t, dx, dy, "Gotowe", true, hot == Some(H::Done), 1.0);
        self.hits.push((ox + W - LX - dw, oy + 20.0, dw, 26.0, H::Done));
        // tabs: a segmented control
        let (tx, ty, tw, th) = (M + LX, M + 60.0, W - 2.0 * LX, 32.0);
        rect(&mut l, tx, ty, tw, th, 16.0, WHITE.alpha(0.06));
        let segw = tw / TABS.len() as f32;
        let tabs_s = st(13.0, 600.0);
        for (i, (_, lab)) in TABS.iter().enumerate() {
            let sx = tx + i as f32 * segw;
            if i == self.tab {
                rect(&mut l, sx + 3.0, ty + 3.0, segw - 6.0, th - 6.0, 13.0, WHITE.alpha(0.16));
            } else if hot == Some(H::Tab(i)) {
                rect(&mut l, sx + 3.0, ty + 3.0, segw - 6.0, th - 6.0, 13.0, WHITE.alpha(0.07));
            }
            let lw = width(cx.t, lab, &tabs_s);
            let tbh = line_h(cx.t, &tabs_s);
            text(&mut l, cx.t, (sx + (segw - lw) / 2.0).round(), ty + (th - tbh) / 2.0, lab, &tabs_s, if i == self.tab { FG } else { FG2 }, true, None);
            self.hits.push((ox + sx - M, oy + ty - M, segw, th, H::Tab(i)));
        }
        // the body, clipped to the view under the tabs (rounded bottom corners)
        if let Some(p) = crate::gfx::rrect_path(M, M, W, h, R) {
            let mut mask = tiny_skia::Mask::new(l.pm.width(), l.pm.height()).unwrap();
            mask.fill_path(&p, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
            // the rows above the view (header + tabs) masked out
            let cut = ((M + TOP) as usize * l.pm.width() as usize).min(mask.data().len());
            mask.data_mut()[..cut].fill(0);
            l.pm.draw_pixmap(M as i32, (M + TOP - self.scroll) as i32, body.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), Some(&mask));
        }
        // the header band's hairline, a scroll bar when it scrolls
        rect(&mut l, M + LX, M + TOP - 1.0, W - 2.0 * LX, 1.0, 0.0, if self.scroll > 0.0 { WHITE.alpha(0.12) } else { HAIR });
        if self.max_scroll > 0.0 {
            let track = view - 16.0;
            let bar = (track * view / content_h).max(28.0);
            let by = M + TOP + 8.0 + (track - bar) * (self.scroll / self.max_scroll);
            rect(&mut l, M + W - 8.0, by, 3.0, bar, 1.5, WHITE.alpha(0.25));
        }
        for (x, y, w, hh, hit) in bh {
            let ry = oy + TOP + y - self.scroll;
            if ry + hh > oy + TOP && ry < oy + h {
                self.hits.push((ox + x, ry, w, hh, hit));
            }
        }
        self.hits.insert(0, (ox, oy, W, h, H::Sheet));
        let paint = tiny_skia::PixmapPaint { opacity: self.o.value(), ..Default::default() };
        c.pm.draw_pixmap((ox - M) as i32, (oy - M) as i32, l.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let before = self.hit();
        self.ptr = (x, y);
        match ev {
            Ptr::Wheel(d) => {
                let s = (self.scroll + d * 48.0).clamp(0.0, self.max_scroll);
                if s != self.scroll {
                    self.scroll = s;
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Release(0x110) => match self.hit() {
                None => Act::Close,
                Some(H::Done) => Act::Close,
                Some(H::Sheet) => Act::None,
                Some(H::Tab(i)) => {
                    self.tab = i;
                    self.scroll = 0.0;
                    Act::Redraw
                }
                Some(H::Opt(card, key, v)) => {
                    if let Some(gr) = groups(card).into_iter().find(|g| g.key == key) {
                        self.cfg.click(card, &gr, &v);
                    }
                    Act::DeskCfg
                }
                Some(H::Lang(code)) => {
                    crate::i18n::set(code);
                    Act::DeskCfg
                }
                Some(H::Field) => {
                    self.editing = true;
                    Act::Redraw
                }
                Some(H::Place(i)) => {
                    self.choose(i);
                    Act::DeskCfg
                }
                Some(H::Gallery) => Act::Toggle("wallpaper"),
                Some(H::Slot(i, k)) => {
                    self.cfg.put_slot(i, k);
                    Act::DeskCfg
                }
                // a hidden card: into the last place shown (one more place when there is room)
                Some(H::Show(k)) => {
                    let n = self.cfg.cards();
                    if n < 3 {
                        self.cfg.set("desk", "cards", serde_json::json!(n + 1));
                    }
                    let n = self.cfg.cards();
                    self.cfg.put_slot(n - 1, k);
                    Act::DeskCfg
                }
            },
            Ptr::Motion | Ptr::Enter | Ptr::Leave => {
                if self.hit() != before { Act::Redraw } else { Act::None }
            }
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        // typing into the place field
        if self.editing && TABS[self.tab].0 == "general" {
            let shift = false;
            return match self.place.key(ev.keysym, ev.utf8.as_deref(), cx.ctrl, shift) {
                crate::edit::EditResult::Accept => {
                    // Enter: the only answer is taken, else search
                    if self.found.len() == 1 && self.asked == self.place.text.trim() {
                        self.choose(0);
                        Act::DeskCfg
                    } else {
                        self.search();
                        Act::Redraw
                    }
                }
                crate::edit::EditResult::Cancel => {
                    self.editing = false;
                    self.found.clear();
                    Act::Redraw
                }
                crate::edit::EditResult::Used => Act::Redraw,
                crate::edit::EditResult::Ignored => Act::None,
            };
        }
        match ev.keysym {
            Keysym::Escape | Keysym::Return | Keysym::KP_Enter => Act::Close,
            Keysym::Tab | Keysym::Right => {
                self.tab = (self.tab + 1) % TABS.len();
                self.scroll = 0.0;
                Act::Redraw
            }
            Keysym::ISO_Left_Tab | Keysym::Left => {
                self.tab = (self.tab + TABS.len() - 1) % TABS.len();
                self.scroll = 0.0;
                Act::Redraw
            }
            Keysym::Down | Keysym::Page_Down => {
                self.scroll = (self.scroll + if ev.keysym == Keysym::Down { 48.0 } else { 240.0 }).min(self.max_scroll);
                Act::Redraw
            }
            Keysym::Up | Keysym::Page_Up => {
                self.scroll = (self.scroll - if ev.keysym == Keysym::Up { 48.0 } else { 240.0 }).max(0.0);
                Act::Redraw
            }
            _ => Act::None,
        }
    }
}
