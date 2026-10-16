// Mist Files painting (FilesWindow.qml geometry): glass sidebar, tab strip, navigation row with breadcrumbs
// and search, command bar, details / icons panel, preview pane, status bar, menus, sheets, tooltips.
// Immediate mode: every paint rebuilds the window's hit list (rect, Hit, tooltip) that input reads.
use super::{fs, Act, Files, Focus, Hit, SortKey, Win, VIEW_ROW};
use crate::gfx::{stroke_rrect, Canvas, Rgba};
use crate::glass::{self, Glass};
use crate::text::{Style, Text};
use std::cell::RefCell;
use std::collections::HashMap;

pub const SIDE_X: f32 = 8.0;
pub const SIDE_W: f32 = 236.0;
pub const HEAD_H: f32 = 34.0;
pub const ROW_H: f32 = 32.0;
pub const CELL_W: f32 = 118.0;
pub const CELL_H: f32 = 124.0;

const FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);
const FG2: Rgba = Rgba(222.0 / 255.0, 230.0 / 255.0, 242.0 / 255.0, 0.86);
const MUTED: Rgba = Rgba(206.0 / 255.0, 216.0 / 255.0, 232.0 / 255.0, 0.74);
const FAINT: Rgba = Rgba(196.0 / 255.0, 208.0 / 255.0, 226.0 / 255.0, 0.6);
const ACC: Rgba = Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 1.0);
const BAD: Rgba = Rgba(238.0 / 255.0, 149.0 / 255.0, 163.0 / 255.0, 1.0);
const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.1);
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const DARK: Rgba = Rgba(11.0 / 255.0, 17.0 / 255.0, 24.0 / 255.0, 1.0);

// tabler glyphs used here (icons.rs has the shell's set)
pub fn glyph(name: &str) -> Option<char> {
    let c = match name {
        "back" => 0xea19,
        "fwd" => 0xea1f,
        "up" => 0xea25,
        "refresh" => 0xeb13,
        "search" => 0xeb1c,
        "cut" => 0xeb1b,
        "copy" => 0xea7a,
        "paste" => 0xea6f,
        "rename" => 0xee6d,
        "trash" => 0xeb41,
        "share" => 0xeb21,
        "plus" => 0xeb0b,
        "folderPlus" => 0xeaab,
        "filePlus" => 0xeaa0,
        "sort" => 0xeb5a,
        "asc" => 0xeb26,
        "desc" => 0xeb27,
        "list" => 0xef40,
        "grid" => 0xedba,
        "dots" => 0xea95,
        "home" => 0xeac1,
        "desktop" => 0xea89,
        "download" => 0xea96,
        "photo" => 0xeb0a,
        "music" => 0xeafc,
        "movie" => 0xeafa,
        "doc" => 0xeaa2,
        "file" => 0xeaa4,
        "drive" => 0xeb1f,
        "chevR" => 0xea61,
        "chevD" => 0xea5f,
        "chevU" => 0xea62,
        "x" => 0xeb55,
        "sidebar" => 0xeada,
        "eye" => 0xea9a,
        "terminal" => 0xebef,
        "open" => 0xea99,
        "info" => 0xeac5,
        "clock" => 0xea70,
        "folder" => 0xeaad,
        "check" => 0xea5e,
        "laptop" => 0xeb64,
        "eyeOff" => 0xecf0,
        "undo" => 0xeb77,
        "link" => 0xeade,
        "preview" => 0xf007,
        "minus" => 0xeaf2,
        "app" => 0xefe6,
        _ => return None,
    };
    char::from_u32(c)
}
fn gl(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, name: &str, size: f32, col: Rgba) {
    let Some(ch) = glyph(name) else { return };
    let st = Style::icon(size);
    let s = ch.to_string();
    let w = t.width(&s, &st);
    c.text(t, cx - w / 2.0, cy, &s, &st, col);
}

thread_local! {
    // where the focused text field's text starts (window x), per window
    static FIELDS: RefCell<HashMap<u32, f32>> = RefCell::new(HashMap::new());
    // the list's scratch canvas (rows clip to the panel)
    static LIST: RefCell<Option<Canvas>> = const { RefCell::new(None) };
}
pub fn field_x(w: &Win) -> f32 {
    FIELDS.with(|f| f.borrow().get(&w.id).copied().unwrap_or(0.0))
}

pub fn main_x(sidebar: bool) -> f32 {
    if sidebar { SIDE_X + SIDE_W + 6.0 } else { 8.0 }
}
fn bottom_h(w: &Win) -> f32 {
    if w.pick.is_some() { 56.0 } else { 32.0 }
}
pub fn preview_w(w: &Win, sidebar: bool) -> f32 {
    let mw = w.w - main_x(sidebar) - 8.0;
    (mw * 0.3).min(320.0).round()
}
// the file panel: x, y, w, h
pub fn panel_rect(w: &Win, sidebar: bool, preview: bool) -> (f32, f32, f32, f32) {
    let mx = main_x(sidebar);
    let mut pw = w.w - mx - 8.0;
    if preview {
        pw -= preview_w(w, sidebar) + 8.0;
    }
    let y = 8.0 + 128.0;
    (mx, y, pw.max(100.0), (w.h - y - 8.0 - bottom_h(w)).max(60.0))
}

struct P<'a> {
    c: &'a mut Canvas,
    hits: Vec<(f32, f32, f32, f32, Hit, &'static str)>,
    hover: Option<Hit>,
}
impl P<'_> {
    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, k: Hit, tip: &'static str) -> bool {
        let on = self.hover.as_ref() == Some(&k);
        self.hits.push((x, y, w, h, k, tip));
        on
    }
}

impl Files {
    pub fn paint_win(&mut self, id: u32, c: &mut Canvas) {
        let Some(i) = self.wins.iter().position(|w| w.id == id) else { return };
        let mut w = self.wins.remove(i);
        w.w = c.w();
        w.h = c.h();
        let hover = w.hover.clone();
        let mut p = P { c, hits: Vec::new(), hover };
        self.paint_all(&mut w, &mut p);
        w.hits = p.hits;
        self.wins.insert(i, w);
    }

    fn paint_all(&mut self, w: &mut Win, p: &mut P) {
        let (ww, wh) = (w.w, w.h);
        p.c.clear();
        // window: dark translucent gradient (Hyprland blurs behind)
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, ww, wh) {
            let path = tiny_skia::PathBuilder::from_rect(r);
            crate::desktop::fill_lin(p.c, &path, (0.0, 0.0), (0.0, wh), &[(0.0, Rgba::rgb8(20, 26, 38, 0.80)), (1.0, Rgba::rgb8(9, 12, 19, 0.84))]);
        }
        let mx = main_x(self.sidebar);
        // empty chrome drags the window
        p.hit(0.0, 0.0, ww, wh, Hit::Drag, "");
        if self.sidebar {
            self.paint_sidebar(w, p);
        }
        let lx = if self.sidebar { SIDE_X + 10.0 } else { mx + 2.0 };
        self.paint_lights(w, p, lx, 17.0);
        self.paint_tabs(w, p, if self.sidebar { mx } else { mx + 76.0 });
        self.paint_nav(w, p, mx);
        self.paint_cmd(w, p, mx);
        self.paint_panel(w, p);
        if self.preview {
            self.paint_preview(w, p);
        }
        if w.pick.is_some() {
            self.paint_pickbar(w, p, mx);
        } else {
            self.paint_status(w, p, mx);
        }
        if w.menu.is_some() {
            self.paint_menu(w, p);
        }
        if w.ask.is_some() {
            self.paint_ask(w, p);
        }
        if let Some((s, x, y)) = w.tip.clone() {
            self.paint_tip(p, &s, x, y, ww, wh);
        }
    }

    fn paint_lights(&mut self, w: &Win, p: &mut P, x: f32, y: f32) {
        let cols = [Rgba::hex("#ff5f57"), Rgba::hex("#febc2e"), Rgba::hex("#28c840")];
        let names = ["x", "minus", "plus"];
        let any = matches!(p.hover, Some(Hit::Light(_)));
        for k in 0..3 {
            let lx = x + k as f32 * 21.0;
            p.hit(lx - 3.0, y - 3.0, 19.0, 19.0, Hit::Light(k as u8), "");
            let col = if w.focused || any { cols[k] } else { Rgba::rgb8(120, 128, 140, 0.55) };
            p.c.rrect(lx, y, 13.0, 13.0, 6.5, col);
            if any {
                gl(p.c, &mut self.text, lx + 6.5, y + 6.5, names[k], 10.0, Rgba::rgb8(60, 20, 10, 0.75));
            }
        }
    }

    fn paint_sidebar(&mut self, w: &mut Win, p: &mut P) {
        let (sx, sy, sw, sh) = (SIDE_X, 8.0, SIDE_W, w.h - 16.0);
        Glass { radius: 20.0, tint: 0.30, shadow: false, corners: true, ..Default::default() }.paint(p.c, sx, sy, sw, sh);
        p.hit(sx, sy, sw, sh, Hit::Side, "");
        let t = &mut self.text;
        let cur = w.cur().path.clone();
        let head = Style::ui(11.0, 650.0);
        let lab = Style::ui(13.0, 450.0);
        let labc = Style::ui(13.0, 560.0);
        // content height, then scroll limits
        let content = 26.0 + 30.0 * self.places.len() as f32 + 8.0 + 26.0 + 48.0 * self.drives.len() as f32 + 8.0 + 30.0 + 10.0;
        let avail = sh - 44.0;
        w.side_scroll = w.side_scroll.clamp(0.0, (content - avail).max(0.0));
        let top = sy + 40.0;
        let bot = sy + sh - 6.0;
        let mut y = top - w.side_scroll;
        let vis = |y: f32, h: f32| y >= top - 1.0 && y + h <= bot + 1.0;
        let row_bg = |p: &mut P, y: f32, h: f32, on: bool, hov: bool, drop: bool| {
            let col = if drop { ACC.alpha(0.28) } else if on { WHITE.alpha(0.10) } else if hov { WHITE.alpha(0.05) } else { return };
            p.c.rrect(sx + 8.0, y, sw - 16.0, h, 9.0, col);
        };
        if vis(y, 26.0) {
            p.c.text(t, sx + 18.0, y + 13.0, "Ulubione", &head, FAINT);
        }
        y += 26.0;
        for (name, path, g) in self.places.clone() {
            if vis(y, 30.0) {
                let k = Hit::Place(path.clone());
                let hov = p.hit(sx + 8.0, y, sw - 16.0, 30.0, k, "");
                let on = cur == path;
                row_bg(p, y, 30.0, on, hov, w.drop.as_deref() == Some(path.as_str()));
                gl(p.c, t, sx + 8.0 + 10.0 + 8.5, y + 15.0, g, 17.0, ACC);
                let st = if on { &labc } else { &lab };
                let s = crate::bar::elide(t, &name, st, sw - 38.0 - 14.0);
                p.c.text(t, sx + 38.0, y + 15.0, &s, st, FG);
            }
            y += 30.0;
        }
        y += 8.0;
        if vis(y, 26.0) {
            p.c.text(t, sx + 18.0, y + 13.0, "Ten komputer", &head, FAINT);
        }
        y += 26.0;
        let small = Style::ui(10.5, 450.0);
        for d in self.drives.clone() {
            if vis(y, 46.0) {
                let k = Hit::Place(d.path.clone());
                let hov = p.hit(sx + 8.0, y, sw - 16.0, 46.0, k, "");
                let on = cur == d.path;
                row_bg(p, y, 46.0, on, hov, w.drop.as_deref() == Some(d.path.as_str()));
                gl(p.c, t, sx + 8.0 + 10.0 + 8.5, y + 16.0, if d.removable { "drive" } else { "laptop" }, 17.0, ACC);
                let info = format!("{} wolne z {}", fs::size(d.free), fs::size(d.size));
                let iw = t.width(&info, &small);
                let st = if on { &labc } else { &lab };
                let s = crate::bar::elide(t, &d.name, st, sw - 38.0 - 18.0);
                p.c.text(t, sx + 38.0, y + 12.0, &s, st, FG);
                let _ = iw;
                p.c.text(t, sx + 38.0, y + 35.0, &info, &small, FAINT);
                let bw = sw - 38.0 - 18.0;
                let used = if d.size > 0 { 1.0 - d.free as f32 / d.size as f32 } else { 0.0 };
                p.c.rrect(sx + 38.0, y + 22.0, bw, 4.0, 2.0, WHITE.alpha(0.10));
                p.c.rrect(sx + 38.0, y + 22.0, (bw * used).max(4.0), 4.0, 2.0, if used > 0.9 { BAD } else { ACC.alpha(0.85) });
            }
            y += 48.0;
        }
        y += 8.0;
        if vis(y, 30.0) {
            let hov = p.hit(sx + 8.0, y, sw - 16.0, 30.0, Hit::Trash, "");
            let on = cur.starts_with("trash:");
            row_bg(p, y, 30.0, on, hov, w.drop.as_deref() == Some("trash:///"));
            gl(p.c, t, sx + 8.0 + 10.0 + 8.5, y + 15.0, "trash", 17.0, ACC);
            p.c.text(t, sx + 38.0, y + 15.0, "Kosz", if on { &labc } else { &lab }, FG);
        }
    }

    fn paint_tabs(&mut self, w: &Win, p: &mut P, x0: f32) {
        let x1 = w.w - 8.0;
        let y = 8.0;
        p.hit(x0, y, x1 - x0, 36.0, Hit::TabStrip, "");
        let n = w.tabs.len();
        let tw = ((x1 - x0 - 50.0) / n as f32 - 4.0).clamp(120.0, 220.0).min((x1 - x0 - 50.0) / n as f32 - 4.0);
        let st = Style::ui(12.5, 450.0);
        let mut x = x0;
        for (k, tab) in w.tabs.iter().enumerate() {
            let on = k == w.active;
            let hov = p.hit(x, y + 2.0, tw, 32.0, Hit::Tab(k), "");
            let close_hov = p.hover == Some(Hit::TabClose(k));
            if on {
                p.c.rrect(x, y + 2.0, tw, 32.0, 11.0, WHITE.alpha(0.10));
                stroke_rrect(p.c, x + 0.5, y + 2.5, tw - 1.0, 31.0, 10.5, WHITE.alpha(0.07));
            } else if hov || close_hov {
                p.c.rrect(x, y + 2.0, tw, 32.0, 11.0, WHITE.alpha(0.05));
            }
            let names = self.folder_icon_names(&tab.path);
            if let Some(pm) = self.icon(&names, 16) {
                crate::img::draw(p.c, &pm, x + 11.0, y + 10.0, 16.0, 16.0, 0.0, false, 1.0);
            }
            let show_close = on || hov || close_hov;
            let title = tab.title();
            let s = crate::bar::elide(&mut self.text, &title, &st, tw - 34.0 - if show_close { 30.0 } else { 10.0 });
            p.c.text(&mut self.text, x + 34.0, y + 18.0, &s, &st, if on { FG } else { FG2 });
            if show_close {
                let (cx, cy) = (x + tw - 18.0, y + 18.0);
                if p.hit(cx - 10.0, cy - 10.0, 20.0, 20.0, Hit::TabClose(k), "Zamknij kartę") {
                    p.c.rrect(cx - 10.0, cy - 10.0, 20.0, 20.0, 10.0, WHITE.alpha(0.10));
                }
                gl(p.c, &mut self.text, cx, cy, "x", 13.0, FG2);
            }
            x += tw + 4.0;
        }
        if p.hit(x, y + 4.0, 28.0, 28.0, Hit::NewTab, "Nowa karta (Ctrl+T)") {
            p.c.rrect(x, y + 4.0, 28.0, 28.0, 9.0, WHITE.alpha(0.07));
        }
        gl(p.c, &mut self.text, x + 14.0, y + 18.0, "plus", 16.0, FG2);
    }

    fn tool_btn(&mut self, p: &mut P, x: f32, y: f32, g: &str, k: Hit, tip: &'static str, enabled: bool) {
        let on = p.hit(x, y, 32.0, 32.0, if enabled { k } else { Hit::Drag }, tip);
        if on && enabled {
            p.c.rrect(x, y, 32.0, 32.0, 10.0, WHITE.alpha(0.07));
        }
        gl(p.c, &mut self.text, x + 16.0, y + 16.0, g, 17.0, if enabled { FG2 } else { FG2.alpha(0.35) });
    }

    fn paint_nav(&mut self, w: &mut Win, p: &mut P, mx: f32) {
        let y = 8.0 + 42.0;
        let t = w.cur();
        let (can_back, can_fwd, can_up) = (!t.back.is_empty(), !t.fwd.is_empty(), !fs::parent(&t.path).is_empty());
        self.tool_btn(p, mx, y + 3.0, "back", Hit::Back, "Wstecz (Alt+←)", can_back);
        self.tool_btn(p, mx + 34.0, y + 3.0, "fwd", Hit::Fwd, "Dalej (Alt+→)", can_fwd);
        self.tool_btn(p, mx + 68.0, y + 3.0, "up", Hit::Up, "W górę (Alt+↑)", can_up);
        self.tool_btn(p, mx + 102.0, y + 3.0, "refresh", Hit::Reload, "Odśwież (F5)", true);
        let x1 = w.w - 8.0;
        let search_focus = w.focus == Focus::Search;
        let sw = if search_focus || !w.search.text.is_empty() { 300.0 } else { 240.0 };
        let ax = mx + 134.0 + 8.0;
        let aw = x1 - ax - sw - 8.0;
        let st = Style::ui(13.0, 450.0);
        // address bar
        let editing = w.addr.is_some();
        p.c.rrect(ax, y + 1.0, aw, 36.0, 12.0, WHITE.alpha(if editing { 0.085 } else { 0.055 }));
        if editing {
            stroke_rrect(p.c, ax + 0.5, y + 1.5, aw - 1.0, 35.0, 11.5, ACC.alpha(0.45));
        }
        p.hit(ax, y + 1.0, aw, 36.0, Hit::Addr, "");
        if let Some(le) = w.addr.as_mut() {
            FIELDS.with(|f| f.borrow_mut().insert(w.id, ax + 12.0));
            le.paint(p.c, &mut self.text, ax + 12.0, y + 1.0, aw - 24.0, 36.0, &st, FG, "", true, false);
        } else {
            let names = self.folder_icon_names(&w.cur().path);
            if let Some(pm) = self.icon(&names, 18) {
                crate::img::draw(p.c, &pm, ax + 12.0, y + 10.0, 18.0, 18.0, 0.0, false, 1.0);
            }
            self.paint_crumbs(w, p, ax + 38.0, y + 1.0, aw - 38.0 - 8.0);
        }
        // search
        let sx = x1 - sw;
        p.c.rrect(sx, y + 1.0, sw, 36.0, 12.0, WHITE.alpha(if search_focus { 0.085 } else { 0.055 }));
        if search_focus {
            stroke_rrect(p.c, sx + 0.5, y + 1.5, sw - 1.0, 35.0, 11.5, ACC.alpha(0.45));
        }
        p.hit(sx, y + 1.0, sw, 36.0, Hit::Search, "");
        gl(p.c, &mut self.text, sx + 19.0, y + 19.0, "search", 16.0, FAINT);
        let ph = format!("Szukaj w: {}", w.cur().title());
        let has = !w.search.text.is_empty();
        if search_focus {
            FIELDS.with(|f| f.borrow_mut().insert(w.id, sx + 34.0));
        }
        w.search.paint(p.c, &mut self.text, sx + 34.0, y + 1.0, sw - 34.0 - if has { 34.0 } else { 12.0 }, 36.0, &st, FG, &ph, search_focus, false);
        if has {
            let (cx, cy) = (sx + sw - 19.0, y + 19.0);
            if p.hit(cx - 11.0, cy - 11.0, 22.0, 22.0, Hit::SearchClear, "Wyczyść") {
                p.c.rrect(cx - 11.0, cy - 11.0, 22.0, 22.0, 11.0, WHITE.alpha(0.10));
            }
            gl(p.c, &mut self.text, cx, cy, "x", 14.0, FG2);
        }
    }

    fn paint_crumbs(&mut self, w: &Win, p: &mut P, x: f32, y: f32, maxw: f32) {
        let path = w.cur().path.clone();
        let home = fs::home();
        // (label, path)
        let mut crumbs: Vec<(String, String)> = vec![];
        if path.starts_with("trash:") {
            crumbs.push(("Kosz".into(), "trash:///".into()));
        } else if path == home || path.starts_with(&format!("{home}/")) {
            crumbs.push(("Katalog domowy".into(), home.clone()));
            let mut acc = home.clone();
            for part in path[home.len()..].split('/').filter(|s| !s.is_empty()) {
                acc = format!("{acc}/{part}");
                crumbs.push((part.into(), acc.clone()));
            }
        } else {
            crumbs.push(("Dysk systemowy".into(), "/".into()));
            let mut acc = String::new();
            for part in path.split('/').filter(|s| !s.is_empty()) {
                acc = format!("{acc}/{part}");
                crumbs.push((part.into(), acc.clone()));
            }
        }
        let st = Style::ui(13.0, 450.0);
        let stl = Style::ui(13.0, 600.0);
        let n = crumbs.len();
        let widths: Vec<f32> = crumbs.iter().enumerate().map(|(i, (l, _))| self.text.width(l, if i == n - 1 { &stl } else { &st }).min(220.0) + 16.0).collect();
        // drop leading crumbs until it fits (an ellipsis stands for them)
        let mut first = 0;
        let total = |a: usize| widths[a..].iter().sum::<f32>() + 16.0 * (n - a - 1) as f32 + if a > 0 { 36.0 } else { 0.0 };
        while first < n - 1 && total(first) > maxw {
            first += 1;
        }
        let mut cx = x;
        let cy = y + 18.0;
        if first > 0 {
            p.c.text(&mut self.text, cx + 4.0, cy, "…", &st, FAINT);
            cx += 20.0;
            gl(p.c, &mut self.text, cx + 8.0, cy, "chevR", 13.0, FAINT);
            cx += 16.0;
        }
        for i in first..n {
            let (l, cp) = &crumbs[i];
            let last = i == n - 1;
            let bw = widths[i];
            if p.hit(cx, y + 5.0, bw, 26.0, Hit::Crumb(cp.clone()), "") {
                p.c.rrect(cx, y + 5.0, bw, 26.0, 8.0, WHITE.alpha(0.07));
            }
            let s = crate::bar::elide(&mut self.text, l, if last { &stl } else { &st }, bw - 16.0);
            p.c.text(&mut self.text, cx + 8.0, cy, &s, if last { &stl } else { &st }, if last { FG } else { FG2 });
            cx += bw;
            if !last {
                gl(p.c, &mut self.text, cx + 8.0, cy, "chevR", 13.0, FAINT);
                cx += 16.0;
            }
        }
    }

    // a command bar button with a label (and a dropdown chevron)
    fn label_btn(&mut self, p: &mut P, x: f32, y: f32, g: &str, label: &str, act: Act, chev: bool, enabled: bool, col: Rgba) -> f32 {
        let st = Style::ui(13.0, 500.0);
        let tw = self.text.width(label, &st);
        let w = 10.0 + 18.0 + 6.0 + tw + if chev { 20.0 } else { 10.0 };
        let on = p.hit(x, y, w, 32.0, if enabled { Hit::Btn(act) } else { Hit::Drag }, "");
        if on && enabled {
            p.c.rrect(x, y, w, 32.0, 10.0, WHITE.alpha(0.07));
        }
        let a = if enabled { 1.0 } else { 0.35 };
        gl(p.c, &mut self.text, x + 19.0, y + 16.0, g, 17.0, col.alpha(a));
        p.c.text(&mut self.text, x + 34.0, y + 16.0, label, &st, FG2.alpha(a));
        if chev {
            gl(p.c, &mut self.text, x + 34.0 + tw + 9.0, y + 17.0, "chevD", 12.0, FAINT);
        }
        w
    }
    fn sep(p: &mut P, x: f32, y: f32) -> f32 {
        p.c.rrect(x + 5.0, y + 7.0, 1.0, 18.0, 0.0, HAIR);
        11.0
    }

    fn paint_cmd(&mut self, w: &Win, p: &mut P, mx: f32) {
        let y = 8.0 + 86.0 + 2.0;
        let t = w.cur();
        let nsel = t.sel.len();
        let one = nsel == 1;
        let mut x = mx;
        if w.pick.is_some() {
            if !t.is_trash() {
                x += self.label_btn(p, x, y, "folderPlus", "Nowy folder", Act::NewFolder, false, true, ACC) + 2.0;
                x += Self::sep(p, x, y);
            }
        } else if t.is_trash() {
            x += self.label_btn(p, x, y, "undo", "Przywróć", Act::Restore, false, nsel > 0, ACC) + 2.0;
            x += self.label_btn(p, x, y, "trash", "Usuń trwale", Act::Delete, false, nsel > 0, BAD) + 2.0;
            x += self.label_btn(p, x, y, "trash", "Opróżnij kosz", Act::EmptyTrash, false, !t.all.is_empty(), BAD) + 2.0;
            x += Self::sep(p, x, y);
        } else {
            x += self.label_btn(p, x, y, "plus", "Nowy", Act::MenuNew, true, true, ACC) + 2.0;
            x += Self::sep(p, x, y);
            let paste = !self.clip.is_empty();
            for (g, a, tip, en) in [
                ("cut", Act::Cut, "Wytnij (Ctrl+X)", nsel > 0),
                ("copy", Act::Copy, "Kopiuj (Ctrl+C)", nsel > 0),
                ("paste", Act::Paste, "Wklej (Ctrl+V)", paste || true),
                ("rename", Act::Rename, "Zmień nazwę (F2)", one),
                ("link", Act::CopyPath, "Kopiuj ścieżkę (Ctrl+Shift+C)", true),
                ("trash", Act::Trash, "Przenieś do kosza (Del)", nsel > 0),
            ] {
                self.tool_btn(p, x, y, g, Hit::Btn(a), tip, en);
                x += 34.0;
            }
            x += Self::sep(p, x, y);
        }
        x += self.label_btn(p, x, y, "sort", "Sortuj", Act::MenuSort, true, true, FG2) + 2.0;
        x += self.label_btn(p, x, y, if self.icons_view { "grid" } else { "list" }, "Widok", Act::MenuView, true, true, FG2) + 2.0;
        if w.pick.is_none() {
            self.tool_btn(p, x, y, "dots", Hit::Btn(Act::MenuMore), "Więcej", true);
            // right: terminal
            let st = Style::ui(13.0, 500.0);
            let bw = 10.0 + 18.0 + 6.0 + self.text.width("Terminal", &st) + 10.0;
            self.label_btn(p, w.w - 8.0 - bw, y, "terminal", "Terminal", Act::Terminal(None), false, !t.is_trash(), FG2);
        }
    }

    fn paint_panel(&mut self, w: &mut Win, p: &mut P) {
        let (px, py, pw, ph) = panel_rect(w, self.sidebar, self.preview);
        p.c.rrect(px, py, pw, ph, 16.0, WHITE.alpha(0.028));
        stroke_rrect(p.c, px + 0.5, py + 0.5, pw - 1.0, ph - 1.0, 15.5, WHITE.alpha(0.055));
        p.hit(px, py, pw, ph, Hit::View, "");
        let drop_here = w.drop.as_deref() == Some(w.cur().path.as_str());
        if drop_here {
            stroke_rrect(p.c, px + 1.0, py + 1.0, pw - 2.0, ph - 2.0, 15.0, ACC.alpha(0.6));
        }
        let ch = self.content_h(w);
        // the list draws into a scratch canvas the panel's size (rows clip at the panel edges)
        let top = if self.icons_view { 0.0 } else { HEAD_H };
        let (lw, lh) = (pw.max(1.0) as u32, (ph - top - 2.0).max(1.0) as u32);
        let mut lc = LIST.with(|l| l.borrow_mut().take()).filter(|c| c.pm.width() == lw && c.pm.height() == lh).unwrap_or_else(|| Canvas::new(lw, lh));
        lc.clear();
        {
            let t = w.cur();
            let scroll = t.scroll;
            let n = t.items.len();
            if n == 0 {
                let (g, msg) = if t.loading {
                    ("", String::new())
                } else if !t.error.is_empty() {
                    ("info", t.error.clone())
                } else if !t.filter.is_empty() {
                    ("search", format!("Brak wyników dla „{}”", t.filter))
                } else if t.is_trash() {
                    ("trash", "Kosz jest pusty".into())
                } else {
                    ("folder", "Ten folder jest pusty".into())
                };
                if !msg.is_empty() {
                    let cy = lh as f32 / 2.0 - 10.0;
                    gl(&mut lc, &mut self.text, lw as f32 / 2.0, cy - 16.0, g, 40.0, FAINT);
                    let st = Style::ui(14.0, 450.0);
                    let tw = self.text.width(&msg, &st);
                    lc.text(&mut self.text, (lw as f32 - tw) / 2.0, cy + 26.0, &msg, &st, FAINT);
                }
            } else if self.icons_view {
                self.paint_icons(w, &mut lc, pw, scroll);
            } else {
                self.paint_rows(w, &mut lc, pw, scroll);
            }
        }
        p.c.pm.draw_pixmap(px as i32, (py + top + 1.0) as i32, lc.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        LIST.with(|l| *l.borrow_mut() = Some(lc));
        if !self.icons_view {
            self.paint_header(w, p, px, py, pw);
        }
        // rename field over the item
        if let Some((rp, _)) = w.rename.as_ref() {
            let rp = rp.clone();
            if let Some(i) = w.cur().items.iter().position(|&k| w.cur().all[k].path == rp) {
                let scroll = w.cur().scroll;
                let st = Style::ui(13.0, 450.0);
                let (fx, fy, fw, fh, center) = if self.icons_view {
                    let cols = ((pw - 20.0) / CELL_W).floor().max(1.0) as usize;
                    let cx = px + 10.0 + (i % cols) as f32 * CELL_W;
                    let cy = py + 10.0 + (i / cols) as f32 * CELL_H - scroll;
                    (cx + 2.0, cy + 78.0, CELL_W - 4.0, 24.0, true)
                } else {
                    let (cname, _, _, _) = cols(pw);
                    (px + 10.0 + 32.0, py + HEAD_H + 1.0 + i as f32 * ROW_H - scroll + 3.0, cname - 36.0, ROW_H - 6.0, false)
                };
                if fy >= py + top - 2.0 && fy + fh <= py + ph + 2.0 {
                    p.c.rrect(fx, fy, fw, fh, 7.0, Rgba::rgb8(16, 22, 32, 0.98));
                    stroke_rrect(p.c, fx + 0.5, fy + 0.5, fw - 1.0, fh - 1.0, 6.5, ACC.alpha(0.6));
                    FIELDS.with(|f| f.borrow_mut().insert(w.id, fx + 6.0));
                    if let Some((_, le)) = w.rename.as_mut() {
                        le.paint(p.c, &mut self.text, fx + 6.0, fy, fw - 12.0, fh, &st, FG, "", true, center);
                    }
                    p.hit(fx, fy, fw, fh, Hit::Search, "");
                    // the field is not the search box: a click in it keeps the rename going
                    p.hits.pop();
                    p.hit(fx, fy, fw, fh, Hit::PickName, "");
                    p.hits.pop();
                }
            }
        }
        // scroll bar
        let vh = ph - top;
        if ch > ph + 1.0 {
            let k = vh / (ch - top);
            let th = (vh * k).max(28.0);
            let ty = py + top + (w.cur().scroll / (ch - ph).max(1.0)) * (vh - th);
            let on = p.hit(px + pw - 12.0, py + top, 12.0, vh, Hit::ScrollBar, "");
            p.c.rrect(px + pw - if on { 9.0 } else { 7.0 }, ty + 3.0, if on { 6.0 } else { 4.0 }, th - 6.0, 3.0, WHITE.alpha(if on { 0.30 } else { 0.18 }));
        }
        if let Some((bx, by, bw, bh)) = w.band {
            let (x0, y0) = (bx.max(px), by.max(py + top));
            let (x1, y1) = ((bx + bw).min(px + pw), (by + bh).min(py + ph));
            if x1 > x0 && y1 > y0 {
                p.c.rrect(x0, y0, x1 - x0, y1 - y0, 3.0, ACC.alpha(0.12));
                stroke_rrect(p.c, x0 + 0.5, y0 + 0.5, x1 - x0 - 1.0, y1 - y0 - 1.0, 3.0, ACC.alpha(0.55));
            }
        }
    }

    fn paint_header(&mut self, w: &Win, p: &mut P, px: f32, py: f32, pw: f32) {
        p.c.rrect(px + 1.0, py + 1.0, pw - 2.0, HEAD_H, 15.0, Rgba::rgb8(14, 19, 28, 0.92));
        p.c.rrect(px + 1.0, py + 17.0, pw - 2.0, HEAD_H - 16.0, 0.0, Rgba::rgb8(14, 19, 28, 0.92));
        p.c.rrect(px + 1.0, py + HEAD_H, pw - 2.0, 1.0, 0.0, HAIR);
        let (cname, cdate, ctype, csize) = cols(pw);
        let t = w.cur();
        let st = Style::ui(12.0, 600.0);
        let x0 = px + 10.0;
        let heads = [(SortKey::Name, "Nazwa", x0 + 36.0, x0, cname), (SortKey::Mtime, "Data modyfikacji", x0 + cname, x0 + cname - 6.0, cdate), (SortKey::Type, "Typ", x0 + cname + cdate, x0 + cname + cdate - 6.0, ctype), (SortKey::Size, "Rozmiar", 0.0, x0 + cname + cdate + ctype - 6.0, csize)];
        for (k, label, tx, hx, hw) in heads {
            let on = p.hit(hx, py + 2.0, hw, HEAD_H - 4.0, Hit::Header(k), "");
            if on {
                p.c.rrect(hx, py + 4.0, hw, HEAD_H - 8.0, 7.0, WHITE.alpha(0.05));
            }
            let lw = self.text.width(label, &st);
            let tx = if k == SortKey::Size { hx + hw - 8.0 - lw } else { tx };
            p.c.text(&mut self.text, tx, py + HEAD_H / 2.0 + 1.0, label, &st, MUTED);
            if t.sort_key == k {
                let gx = if k == SortKey::Size { tx - 10.0 } else { tx + lw + 10.0 };
                gl(p.c, &mut self.text, gx, py + HEAD_H / 2.0 + 1.0, if t.sort_desc { "chevD" } else { "chevU" }, 12.0, ACC);
            }
        }
    }

    fn row_alpha(&self, e: &fs::Entry) -> f32 {
        if (self.clip_cut && self.clip.contains(&e.path)) || e.hidden { 0.5 } else { 1.0 }
    }

    fn paint_rows(&mut self, w: &Win, c: &mut Canvas, pw: f32, scroll: f32) {
        let t = w.cur();
        let (cname, cdate, ctype, csize) = cols(pw);
        let lh = c.h();
        let i0 = (scroll / ROW_H).floor().max(0.0) as usize;
        let i1 = (((scroll + lh) / ROW_H).ceil() as usize).min(t.items.len());
        let hov = if w.hover == Some(Hit::View) && w.menu.is_none() { VIEW_ROW.with(|r| r.get()) } else { None };
        let st = Style::ui(13.0, 450.0);
        let sm = Style::ui(12.5, 450.0);
        let x0 = 10.0;
        for i in i0..i1 {
            let e = t.all[t.items[i]].clone();
            let y = i as f32 * ROW_H - scroll;
            let sel = t.sel.contains(&e.path);
            let drop = e.dir && w.drop.as_deref() == Some(e.path.as_str());
            if drop {
                c.rrect(x0 - 4.0, y + 1.0, pw - 20.0 + 8.0, ROW_H - 2.0, 9.0, ACC.alpha(0.30));
            } else if sel {
                c.rrect(x0 - 4.0, y + 1.0, pw - 20.0 + 8.0, ROW_H - 2.0, 9.0, ACC.alpha(0.20));
                if t.cur == i as i32 {
                    stroke_rrect(c, x0 - 3.5, y + 1.5, pw - 20.0 + 7.0, ROW_H - 3.0, 8.5, ACC.alpha(0.35));
                }
            } else if hov == Some(i) {
                c.rrect(x0 - 4.0, y + 1.0, pw - 20.0 + 8.0, ROW_H - 2.0, 9.0, WHITE.alpha(0.045));
            }
            let a = self.row_alpha(&e);
            if let Some(pm) = self.file_icon(&e, 20) {
                crate::img::draw(c, &pm, x0 + 6.0, y + 6.0, 20.0, 20.0, 0.0, false, a);
            }
            let cy = y + ROW_H / 2.0;
            let renaming = w.rename.as_ref().map(|r| r.0 == e.path).unwrap_or(false);
            if !renaming {
                let s = crate::bar::elide(&mut self.text, &e.name, &st, cname - 36.0 - 10.0);
                c.text(&mut self.text, x0 + 36.0, cy, &s, &st, FG.alpha(a));
            }
            let d = fs::date(e.mtime);
            let s = crate::bar::elide(&mut self.text, &d, &sm, cdate - 12.0);
            c.text(&mut self.text, x0 + cname, cy, &s, &sm, MUTED.alpha(a));
            let ty = self.mime.type_name(&e);
            let s = crate::bar::elide(&mut self.text, &ty, &sm, ctype - 12.0);
            c.text(&mut self.text, x0 + cname + cdate, cy, &s, &sm, MUTED.alpha(a));
            if !e.dir {
                let s = fs::size(e.size);
                let sw = self.text.width(&s, &sm);
                c.text(&mut self.text, x0 + cname + cdate + ctype + csize - 14.0 - sw, cy, &s, &sm, MUTED.alpha(a));
            }
        }
    }

    fn paint_icons(&mut self, w: &Win, c: &mut Canvas, pw: f32, scroll: f32) {
        let t = w.cur();
        let cols = ((pw - 20.0) / CELL_W).floor().max(1.0) as usize;
        let lh = c.h();
        let r0 = ((scroll - 10.0) / CELL_H).floor().max(0.0) as usize;
        let r1 = ((scroll + lh) / CELL_H).ceil() as usize + 1;
        let hov = if w.hover == Some(Hit::View) && w.menu.is_none() { VIEW_ROW.with(|r| r.get()) } else { None };
        let st = Style::ui(12.0, 450.0);
        for i in (r0 * cols)..((r1 * cols).min(t.items.len())) {
            let e = t.all[t.items[i]].clone();
            let x = 10.0 + (i % cols) as f32 * CELL_W;
            let y = 10.0 + (i / cols) as f32 * CELL_H - scroll;
            let sel = t.sel.contains(&e.path);
            let drop = e.dir && w.drop.as_deref() == Some(e.path.as_str());
            let (bx, by, bw, bh) = (x + 6.0, y + 4.0, CELL_W - 12.0, CELL_H - 8.0);
            if drop {
                c.rrect(bx, by, bw, bh, 14.0, ACC.alpha(0.30));
            } else if sel {
                c.rrect(bx, by, bw, bh, 14.0, ACC.alpha(0.20));
                if t.cur == i as i32 {
                    stroke_rrect(c, bx + 0.5, by + 0.5, bw - 1.0, bh - 1.0, 13.5, ACC.alpha(0.35));
                }
            } else if hov == Some(i) {
                c.rrect(bx, by, bw, bh, 14.0, WHITE.alpha(0.045));
            }
            let a = self.row_alpha(&e);
            let (ix, iy, iw, ih) = (x + (CELL_W - 72.0) / 2.0, y + 12.0, 72.0, 64.0);
            let th = if self.has_thumb(&e) { self.thumb(&e, 128) } else { None };
            if let Some(pm) = th {
                // a thin frame around pictures
                let (tw, tht) = (pm.width() as f32, pm.height() as f32);
                let k = (iw / tw).min(ih / tht);
                let (dw, dh) = (tw * k, tht * k);
                let (dx, dy) = (ix + (iw - dw) / 2.0, iy + (ih - dh) / 2.0);
                p_shadow(c, dx, dy, dw, dh);
                crate::img::draw(c, &pm, dx, dy, dw, dh, 5.0, true, a);
            } else if let Some(pm) = self.file_icon(&e, 64) {
                crate::img::draw(c, &pm, x + (CELL_W - 60.0) / 2.0, iy + 2.0, 60.0, 60.0, 0.0, false, a);
            }
            let renaming = w.rename.as_ref().map(|r| r.0 == e.path).unwrap_or(false);
            if !renaming {
                let lines = crate::gfx::wrap(&mut self.text, &e.name, &st, CELL_W - 16.0, 2);
                for (k, l) in lines.iter().enumerate() {
                    let lw = self.text.width(l, &st);
                    c.text(&mut self.text, x + (CELL_W - lw) / 2.0, y + 90.0 + k as f32 * 15.0, l, &st, FG.alpha(a));
                }
            }
        }
    }

    fn paint_preview(&mut self, w: &Win, p: &mut P) {
        let (px, py, pw, ph) = panel_rect(w, self.sidebar, self.preview);
        let (x, y, vw, vh) = (px + pw + 8.0, py, preview_w(w, self.sidebar), ph);
        p.c.rrect(x, y, vw, vh, 16.0, WHITE.alpha(0.028));
        stroke_rrect(p.c, x + 0.5, y + 0.5, vw - 1.0, vh - 1.0, 15.5, WHITE.alpha(0.055));
        p.hit(x, y, vw, vh, Hit::Preview, "");
        let t = w.cur();
        let sel: Vec<fs::Entry> = t.selected().into_iter().cloned().collect();
        let big = Style::ui(15.0, 600.0);
        let lab = Style::ui(12.0, 450.0);
        let val = Style::ui(12.5, 500.0);
        let pic_h = (vh * 0.42).min(220.0);
        let mut rows: Vec<(String, String)> = vec![];
        let title;
        if sel.len() > 1 {
            title = format!("Zaznaczono {} elem.", sel.len());
            gl(p.c, &mut self.text, x + vw / 2.0, y + 20.0 + pic_h / 2.0, "copy", 64.0, FAINT);
            let files: u64 = sel.iter().filter(|e| !e.dir).map(|e| e.size).sum();
            rows.push(("Rozmiar plików".into(), fs::size(files)));
            rows.push(("Foldery".into(), sel.iter().filter(|e| e.dir).count().to_string()));
        } else {
            let e = match sel.first() {
                Some(e) => e.clone(),
                None => fs::Entry { name: t.title(), path: t.path.clone(), dir: true, ..Default::default() },
            };
            title = e.name.clone();
            let (ix, iy, iw, ih) = (x + 20.0, y + 20.0, vw - 40.0, pic_h);
            let th = if self.has_thumb(&e) { self.thumb(&e, 256) } else { None };
            if let Some(pm) = th {
                let (tw, tht) = (pm.width() as f32, pm.height() as f32);
                let k = (iw / tw).min(ih / tht);
                let (dw, dh) = (tw * k, tht * k);
                let (dx, dy) = (ix + (iw - dw) / 2.0, iy + (ih - dh) / 2.0);
                p_shadow(p.c, dx, dy, dw, dh);
                crate::img::draw(p.c, &pm, dx, dy, dw, dh, 8.0, true, 1.0);
            } else if let Some(pm) = self.file_icon(&e, 128) {
                let s = ih.min(128.0);
                crate::img::draw(p.c, &pm, x + (vw - s) / 2.0, iy + (ih - s) / 2.0, s, s, 0.0, false, 1.0);
            }
            rows.push(("Typ".into(), if e.path.starts_with("trash:") && sel.is_empty() { "Kosz".into() } else { self.mime.type_name(&e) }));
            if e.dir {
                if !e.path.starts_with("trash:") {
                    match self.folder_info(&e.path) {
                        Some((n, s)) if n != usize::MAX => {
                            rows.push(("Zawiera".into(), fs::count_pl(n)));
                            rows.push(("Rozmiar".into(), fs::size(s)));
                        }
                        _ => rows.push(("Zawiera".into(), "…".into())),
                    }
                }
            } else {
                rows.push(("Rozmiar".into(), fs::size(e.size)));
            }
            if e.mtime > 0 {
                rows.push(("Zmodyfikowano".into(), fs::date(e.mtime)));
            }
            if !e.orig.is_empty() {
                rows.push(("Pierwotnie".into(), e.orig.clone()));
            }
            if !e.path.starts_with("trash:") {
                rows.push(("Położenie".into(), fs::parent(&e.path)));
            }
        }
        let mut cy = y + 20.0 + pic_h + 26.0;
        for l in crate::gfx::wrap(&mut self.text, &title, &big, vw - 32.0, 2) {
            let lw = self.text.width(&l, &big);
            p.c.text(&mut self.text, x + (vw - lw) / 2.0, cy, &l, &big, FG);
            cy += 20.0;
        }
        cy += 10.0;
        p.c.rrect(x + 16.0, cy, vw - 32.0, 1.0, 0.0, HAIR);
        cy += 18.0;
        for (k, v) in rows {
            if cy > y + vh - 20.0 {
                break;
            }
            p.c.text(&mut self.text, x + 16.0, cy, &k, &lab, FAINT);
            cy += 17.0;
            let s = crate::bar::elide(&mut self.text, &v, &val, vw - 32.0);
            p.c.text(&mut self.text, x + 16.0, cy, &s, &val, FG2);
            cy += 24.0;
        }
    }

    fn paint_status(&mut self, w: &Win, p: &mut P, mx: f32) {
        let y = w.h - 8.0 - 32.0;
        let st = Style::ui(12.0, 450.0);
        let t = w.cur();
        let mut x = mx + 8.0;
        let msg = if !w.toast.is_empty() {
            w.toast.clone()
        } else if t.loading && t.items.is_empty() {
            "Wczytywanie…".into()
        } else {
            let mut s = fs::count_pl(t.items.len());
            let n = t.sel.len();
            if n > 0 {
                s += &format!(" · zaznaczono {n}");
                let sz = t.sel_size();
                if sz > 0 {
                    s += &format!(", {}", fs::size(sz));
                }
            }
            s
        };
        x += p.c.text(&mut self.text, x, y + 16.0, &msg, &st, if w.toast.is_empty() { FAINT } else { FG2 }) + 18.0;
        if let Some(j) = &self.job {
            let done = j.pr.done.load(std::sync::atomic::Ordering::Relaxed);
            let frac = if j.total > 0 { (done as f32 / j.total as f32).min(1.0) } else { 0.0 };
            let s = crate::bar::elide(&mut self.text, &j.title, &st, 260.0);
            x += p.c.text(&mut self.text, x, y + 16.0, &s, &st, FG2) + 10.0;
            p.c.rrect(x, y + 13.0, 160.0, 6.0, 3.0, WHITE.alpha(0.10));
            p.c.rrect(x, y + 13.0, (160.0 * frac).max(6.0), 6.0, 3.0, ACC);
            x += 168.0;
            if p.hit(x, y + 5.0, 22.0, 22.0, Hit::OpsCancel, "Anuluj") {
                p.c.rrect(x, y + 5.0, 22.0, 22.0, 11.0, WHITE.alpha(0.10));
            }
            gl(p.c, &mut self.text, x + 11.0, y + 16.0, "x", 13.0, FG2);
        }
        // right: preview, details, icons
        let mut rx = w.w - 8.0 - 28.0;
        for (g, a, on, tip) in [("grid", Act::View(true), self.icons_view, "Ikony (Ctrl+Shift+2)"), ("list", Act::View(false), !self.icons_view, "Szczegóły (Ctrl+Shift+6)"), ("preview", Act::TogglePreview, self.preview, "Okienko podglądu (Alt+P)")] {
            let hov = p.hit(rx, y + 2.0, 28.0, 28.0, Hit::Btn(a), tip);
            if on {
                p.c.rrect(rx, y + 2.0, 28.0, 28.0, 8.0, ACC.alpha(0.18));
            } else if hov {
                p.c.rrect(rx, y + 2.0, 28.0, 28.0, 8.0, WHITE.alpha(0.07));
            }
            gl(p.c, &mut self.text, rx + 14.0, y + 16.0, g, 15.0, if on { ACC } else { FG2 });
            rx -= 30.0;
        }
    }

    fn paint_pickbar(&mut self, w: &mut Win, p: &mut P, mx: f32) {
        let Some(pk) = w.pick.as_ref() else { return };
        let (save, accept, filt) = (pk.save, pk.accept.clone(), pk.filters.get(pk.cur_filter).map(|f| f.0.clone()));
        let y = w.h - 8.0 - 56.0 + 10.0;
        let st = Style::ui(13.0, 450.0);
        let stb = Style::ui(13.0, 600.0);
        let mut x = mx + 4.0;
        if save {
            x += p.c.text(&mut self.text, x, y + 18.0, "Nazwa:", &st, MUTED) + 10.0;
            let fw = 340.0f32.min(w.w - x - 420.0).max(160.0);
            let focus = w.focus == Focus::Name;
            p.c.rrect(x, y, fw, 36.0, 11.0, WHITE.alpha(if focus { 0.085 } else { 0.055 }));
            if focus {
                stroke_rrect(p.c, x + 0.5, y + 0.5, fw - 1.0, 35.0, 10.5, ACC.alpha(0.45));
                FIELDS.with(|f| f.borrow_mut().insert(w.id, x + 12.0));
            }
            p.hit(x, y, fw, 36.0, Hit::PickName, "");
            w.name.paint(p.c, &mut self.text, x + 12.0, y, fw - 24.0, 36.0, &st, FG, "Nazwa pliku", focus, false);
            x += fw + 10.0;
        }
        if let Some(f) = filt {
            let s = crate::bar::elide(&mut self.text, &f, &st, 220.0);
            let bw = self.text.width(&s, &st) + 44.0;
            if p.hit(x, y + 2.0, bw, 32.0, Hit::Btn(Act::MenuFilter), "Typ plików") {
                p.c.rrect(x, y + 2.0, bw, 32.0, 10.0, WHITE.alpha(0.07));
            }
            stroke_rrect(p.c, x + 0.5, y + 2.5, bw - 1.0, 31.0, 9.5, HAIR);
            p.c.text(&mut self.text, x + 12.0, y + 18.0, &s, &st, FG2);
            gl(p.c, &mut self.text, x + bw - 16.0, y + 19.0, "chevD", 12.0, FAINT);
        }
        // right: cancel, accept
        let aw = self.text.width(&accept, &stb) + 36.0;
        let cw = self.text.width("Anuluj", &st) + 36.0;
        let ax = w.w - 8.0 - 4.0 - aw;
        let cx = ax - 10.0 - cw;
        let hov = p.hit(cx, y + 1.0, cw, 34.0, Hit::PickCancel, "");
        p.c.rrect(cx, y + 1.0, cw, 34.0, 11.0, WHITE.alpha(if hov { 0.12 } else { 0.08 }));
        p.c.text(&mut self.text, cx + 18.0, y + 18.0, "Anuluj", &st, FG);
        let hov = p.hit(ax, y + 1.0, aw, 34.0, Hit::PickOk, "");
        p.c.rrect(ax, y + 1.0, aw, 34.0, 11.0, ACC.alpha(if hov { 1.0 } else { 0.9 }));
        p.c.text(&mut self.text, ax + 18.0, y + 18.0, &accept, &stb, DARK);
    }

    fn paint_menu(&mut self, w: &Win, p: &mut P) {
        let Some(m) = w.menu.as_ref() else { return };
        p.hit(0.0, 0.0, w.w, w.h, Hit::MenuOutside, "");
        let h: f32 = m.entries.iter().map(|e| if e.sep { 9.0 } else if e.head { 24.0 } else { 30.0 }).sum::<f32>() + 12.0;
        glass::shadow(p.c, m.x, m.y + 6.0, m.w, h, 14.0, 24.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.35));
        p.c.rrect(m.x, m.y, m.w, h, 14.0, Rgba::rgb8(22, 28, 40, 0.97));
        stroke_rrect(p.c, m.x + 0.5, m.y + 0.5, m.w - 1.0, h - 1.0, 13.5, Rgba::rgb8(214, 224, 242, 0.12));
        let st = Style::ui(13.0, 450.0);
        let ks = Style::ui(12.0, 450.0);
        let hs = Style::ui(11.0, 650.0);
        let mut y = m.y + 6.0;
        for (i, e) in m.entries.iter().enumerate() {
            if e.sep {
                p.c.rrect(m.x + 12.0, y + 4.0, m.w - 24.0, 1.0, 0.0, HAIR);
                y += 9.0;
                continue;
            }
            if e.head {
                p.c.text(&mut self.text, m.x + 14.0, y + 13.0, &e.label, &hs, FAINT);
                y += 24.0;
                continue;
            }
            p.hit(m.x + 6.0, y, m.w - 12.0, 30.0, Hit::MenuItem(i), "");
            let hi = m.hi == i as i32 && e.enabled;
            let a = if e.enabled { 1.0 } else { 0.4 };
            if hi {
                p.c.rrect(m.x + 6.0, y, m.w - 12.0, 30.0, 9.0, if e.danger { BAD.alpha(0.85) } else { ACC.alpha(0.9) });
            }
            let fg = if hi { DARK } else if e.danger { BAD } else { FG };
            let g = if !e.glyph.is_empty() { e.glyph } else if e.checked { "check" } else { "" };
            if !g.is_empty() {
                gl(p.c, &mut self.text, m.x + 6.0 + 18.0, y + 15.0, g, 16.0, (if hi { DARK } else if e.danger { BAD } else { FG2 }).alpha(a));
            }
            p.c.text(&mut self.text, m.x + 6.0 + 38.0, y + 15.0, &e.label, &st, fg.alpha(a));
            let right = if e.checked && !e.glyph.is_empty() { "✓" } else { e.keys };
            if !right.is_empty() {
                let rs = if right == "✓" { Style::ui(13.0, 600.0) } else { Style::ui(12.0, 450.0) };
                let kw = self.text.width(right, &rs);
                let col = if hi { DARK.alpha(0.7) } else if right == "✓" { ACC } else { FAINT };
                p.c.text(&mut self.text, m.x + m.w - 6.0 - 12.0 - kw, y + 15.0, right, &rs, col.alpha(a));
            }
            let _ = &ks;
            y += 30.0;
        }
    }

    fn paint_ask(&mut self, w: &Win, p: &mut P) {
        let Some(a) = w.ask.as_ref() else { return };
        p.c.rrect(0.0, 0.0, w.w, w.h, 0.0, Rgba(0.0, 0.0, 0.0, 0.35));
        p.hit(0.0, 0.0, w.w, w.h, Hit::MenuOutside, "");
        let st = Style::ui(13.0, 450.0);
        let lines = crate::gfx::wrap(&mut self.text, &a.text, &st, 380.0 - 48.0, 5);
        let (cw, ch) = (380.0, 150.0 + lines.len() as f32 * 18.0 + 40.0);
        let (x, y) = (((w.w - cw) / 2.0).round(), ((w.h - ch) / 2.0 - 20.0).round());
        glass::shadow(p.c, x, y + 10.0, cw, ch, 20.0, 40.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.45));
        p.c.rrect(x, y, cw, ch, 20.0, Rgba::rgb8(24, 30, 43, 0.98));
        stroke_rrect(p.c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 19.5, Rgba::rgb8(214, 224, 242, 0.12));
        p.hit(x, y, cw, ch, Hit::Sheet, "");
        let danger = matches!(a.act, Act::DeleteNow | Act::EmptyTrashNow);
        gl(p.c, &mut self.text, x + cw / 2.0, y + 40.0, if danger { "trash" } else { "info" }, 30.0, if danger { BAD } else { ACC });
        let ts = Style::ui(16.0, 650.0);
        let tw = self.text.width(&a.title, &ts);
        p.c.text(&mut self.text, x + (cw - tw) / 2.0, y + 80.0, &a.title, &ts, FG);
        let mut ly = y + 106.0;
        for l in &lines {
            let lw = self.text.width(l, &st);
            p.c.text(&mut self.text, x + (cw - lw) / 2.0, ly, l, &st, MUTED);
            ly += 18.0;
        }
        let by = y + ch - 24.0 - 34.0;
        let bw = (cw - 24.0 * 2.0 - 10.0) / 2.0;
        let hov = p.hit(x + 24.0, by, bw, 34.0, Hit::AskCancel, "");
        p.c.rrect(x + 24.0, by, bw, 34.0, 11.0, WHITE.alpha(if hov { 0.12 } else { 0.08 }));
        let aw = self.text.width("Anuluj", &st);
        p.c.text(&mut self.text, x + 24.0 + (bw - aw) / 2.0, by + 17.0, "Anuluj", &st, FG);
        let ox = x + 24.0 + bw + 10.0;
        let hov = p.hit(ox, by, bw, 34.0, Hit::AskOk, "");
        let col = if danger { BAD } else { ACC };
        p.c.rrect(ox, by, bw, 34.0, 11.0, col.alpha(if hov { 1.0 } else { 0.88 }));
        let sb = Style::ui(13.0, 600.0);
        let ow = self.text.width(&a.ok, &sb);
        p.c.text(&mut self.text, ox + (bw - ow) / 2.0, by + 17.0, &a.ok, &sb, DARK);
    }

    fn paint_tip(&mut self, p: &mut P, s: &str, cx: f32, y: f32, ww: f32, wh: f32) {
        let st = Style::ui(12.0, 450.0);
        let tw = self.text.width(s, &st) + 18.0;
        let th = 26.0;
        let x = (cx - tw / 2.0).clamp(4.0, ww - tw - 4.0);
        let y = if y + th > wh - 4.0 { y - th - 50.0 } else { y };
        p.c.rrect(x, y, tw, th, 8.0, Rgba::rgb8(24, 30, 42, 0.97));
        stroke_rrect(p.c, x + 0.5, y + 0.5, tw - 1.0, th - 1.0, 7.5, HAIR);
        p.c.text(&mut self.text, x + 9.0, y + th / 2.0, s, &st, FG2);
    }
}

// details columns: name, date, type, size
pub fn cols(pw: f32) -> (f32, f32, f32, f32) {
    let (cdate, ctype, csize) = (160.0, (pw * 0.16).min(170.0), 92.0);
    ((pw - 20.0 - cdate - ctype - csize).max(120.0), cdate, ctype, csize)
}

fn p_shadow(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    glass::shadow(c, x, y + 2.0, w, h, 5.0, 6.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.35));
}
