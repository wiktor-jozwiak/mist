// Mist Settings painting. Immediate mode like Mist Files: every paint rebuilds the hit list that input reads.
// Left: the glass sidebar (traffic lights, the account, the search field, the categories as coloured tiles).
// Right: the page header (title or breadcrumb), then the page's blocks scrolled under it: cards of rows with
// one control each, the wallpaper grid, accent swatches, quick tiles. Only what is on screen is drawn.
use super::pages::{Act, Block, Btn, Ctl, Page, Row, Thumb, Tile, ALL, SIDEBAR};
use super::{Fid, Hit, Settings, Sl};
use crate::gfx::{stroke_rrect, Canvas, Rgba};
use crate::glass::{self, Glass};
use crate::text::{Style, Text};

pub const SIDE_X: f32 = 8.0;
pub const SIDE_W: f32 = 272.0;
const HEAD: f32 = 92.0;
const PAD: f32 = 34.0;
const MAX_W: f32 = 940.0;

const FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);
const FG2: Rgba = Rgba(222.0 / 255.0, 230.0 / 255.0, 242.0 / 255.0, 0.86);
const MUTED: Rgba = Rgba(206.0 / 255.0, 216.0 / 255.0, 232.0 / 255.0, 0.70);
const FAINT: Rgba = Rgba(196.0 / 255.0, 208.0 / 255.0, 226.0 / 255.0, 0.55);
const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.09);
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);

// glyphs the shell's icons.rs does not have (tabler-icons code points)
fn glyph(name: &str) -> Option<char> {
    if let Some(c) = crate::icons::icon(name) {
        return Some(c);
    }
    let c = match name {
        "accessible" => 0xeba9,
        "app-window" => 0xefe6,
        "arrows-maximize" => 0xea28,
        "bell-off" => 0xece9,
        "brand-github" => 0xec1c,
        "brush" => 0xebb8,
        "circle-check" => 0xea67,
        "chevron-right" => 0xea61,
        "contrast" => 0xec4e,
        "database" => 0xea88,
        "device-desktop" => 0xea89,
        "device-gamepad-2" => 0xf1d2,
        "download" => 0xea96,
        "eye" => 0xea9a,
        "gauge" => 0xeab1,
        "hand-finger" => 0xee94,
        "history" => 0xebea,
        "home" => 0xeac1,
        "info-circle" => 0xeac5,
        "key" => 0xeac7,
        "language" => 0xebbe,
        "layout-dashboard" => 0xf02c,
        "layout-grid" => 0xedba,
        "mail" => 0xeae5,
        "map-pin" => 0xeae8,
        "microphone" => 0xeaf0,
        "numbers" => 0xf015,
        "package" => 0xeaff,
        "pointer" => 0xf265,
        "refresh" => 0xeb13,
        "router" => 0xeb18,
        "server" => 0xeb1f,
        "speakerphone" => 0xed61,
        "star" => 0xeb2e,
        "typography" => 0xebc5,
        "user-circle" => 0xef68,
        "user-shield" => 0xf7d0,
        "volume-off" => 0xf1c3,
        "zoom-in" => 0xeb56,
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
// the window tokens over a blurred backdrop (mist-comp blurs app id mist-settings): translucent enough for the
// wallpaper's colours to come through, opaque with high contrast
fn win_bg() -> [(f32, Rgba); 2] {
    let (t, b) = (crate::tokens::WINDOW_TOP, crate::tokens::WINDOW_BOTTOM);
    let (a0, a1) = if crate::theme::high_contrast() { (0.97, 0.98) } else { (0.78, 0.86) };
    [(0.0, Rgba::rgb8(t.0, t.1, t.2, a0)), (1.0, Rgba::rgb8(b.0, b.1, b.2, a1))]
}
fn rgb(v: u32) -> Rgba {
    Rgba::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0)
}
fn st(size: f32, w: f32) -> Style {
    Style::ui(size, w)
}
fn right_text(c: &mut Canvas, t: &mut Text, xr: f32, cy: f32, s: &str, s_: &Style, col: Rgba) -> f32 {
    let w = t.width(s, s_);
    c.text(t, xr - w, cy, s, s_, col);
    w
}

// search results: (title, page, where) for every page title, caption and row matching the query (in the
// source language and in the shown one)
pub fn search_hits(s: &Settings) -> Vec<(String, Page, String)> {
    let q = s.search.text.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let m = |x: &str| x.to_lowercase().contains(&q) || crate::i18n::tr(x).to_lowercase().contains(&q);
    let path = |p: Page| match p.parent() {
        Some(par) => format!("{} › {}", crate::i18n::tr(par.title()), crate::i18n::tr(p.title())),
        None => crate::i18n::tr(p.title()).to_string(),
    };
    let mut out: Vec<(String, Page, String)> = Vec::new();
    let push = |t: &str, p: Page, out: &mut Vec<(String, Page, String)>| {
        if !out.iter().any(|o| o.0 == t && o.1 == p) {
            out.push((t.to_string(), p, path(p)));
        }
    };
    for p in ALL {
        if m(p.title()) {
            push(p.title(), p, &mut out);
        }
    }
    for p in ALL {
        // the long, data-made lists are not searched (packages, every app, every shortcut)
        if matches!(p, Page::Installed | Page::Updates | Page::Keys) {
            continue;
        }
        for b in s.blocks(p) {
            match b {
                Block::Card(rows) => {
                    for r in rows {
                        if m(&r.title) || (!r.sub.is_empty() && m(&r.sub) && r.sub.len() < 60) {
                            push(&r.title, p, &mut out);
                        }
                    }
                }
                Block::Caption(c) if m(&c) => push(&c, p, &mut out),
                _ => {}
            }
        }
    }
    out.truncate(40);
    out
}

impl Settings {
    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, k: Hit) -> bool {
        let on = self.hover.as_ref() == Some(&k);
        self.hits.push((x, y, w, h, k));
        on
    }

    pub fn paint_win(&mut self, c: &mut Canvas) {
        self.w = c.w();
        self.h = c.h();
        let (ww, wh) = (self.w, self.h);
        self.hits.clear();
        c.clear();
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, ww, wh) {
            let path = tiny_skia::PathBuilder::from_rect(r);
            crate::desktop::fill_lin(c, &path, (0.0, 0.0), (0.0, wh), &win_bg());
        }
        self.hit(0.0, 0.0, ww, wh, Hit::Drag);
        let x0 = SIDE_X + SIDE_W + PAD;
        let cw = (ww - x0 - PAD).min(MAX_W).max(280.0);

        // ---- the page body, scrolled under the header
        let searching = !self.search.text.trim().is_empty();
        let blocks = if searching {
            let hits = search_hits(self);
            if hits.is_empty() {
                vec![Block::Note("Nic nie znaleziono. Spróbuj innego słowa.".into())]
            } else {
                vec![Block::Card(hits.into_iter().map(|(t, p, w)| Row::nav(p.icon(), &t, &w, p)).collect())]
            }
        } else {
            self.blocks(self.page)
        };
        let top = HEAD + 6.0;
        let mut y = top - self.scroll;
        self.hit(x0 - PAD, HEAD, ww - x0 + PAD, wh - HEAD, Hit::Content);
        for b in &blocks {
            y = self.block(c, b, x0, y, cw);
        }
        let content = y + self.scroll - top + 28.0;
        self.max_scroll = (content - (wh - top)).max(0.0);
        if self.scroll > self.max_scroll {
            self.scroll = self.max_scroll;
        }

        // ---- the header over it: the window background again, then the title
        let hx = SIDE_X + SIDE_W + 1.0;
        if let Some(r) = tiny_skia::Rect::from_xywh(hx, 0.0, ww - hx, HEAD) {
            let paint = tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() };
            c.pm.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
            let path = tiny_skia::PathBuilder::from_rect(r);
            crate::desktop::fill_lin(c, &path, (0.0, 0.0), (0.0, wh), &win_bg());
        }
        // a hairline once the rows go under the header
        if self.scroll > 1.0 {
            c.rrect(x0, HEAD - 1.0, cw, 1.0, 0.0, HAIR);
        }
        self.hit(hx, 0.0, ww - hx, HEAD, Hit::Drag);
        self.header(c, x0, cw, searching);

        // ---- the scroll bar: always there while the page is longer than the window, wider under the pointer,
        // the accent while held; the track takes clicks too
        if self.max_scroll > 0.5 {
            let (ty, th) = (HEAD + 6.0, wh - HEAD - 18.0);
            let view = wh - top;
            let tbh = (th * view / (view + self.max_scroll)).clamp(40.0, th);
            let at = ty + self.scroll / self.max_scroll * (th - tbh);
            let held = self.sb_drag.is_some();
            let hov = self.hit(ww - 18.0, ty, 18.0, th, Hit::Scroll(ty, th, tbh)) || held;
            let bw = if hov { 9.0 } else { 6.0 };
            let bx = ww - 6.0 - bw;
            if hov {
                c.rrect(bx, ty, bw, th, bw / 2.0, WHITE.alpha(0.06));
            }
            let col = if held { crate::theme::accent() } else { WHITE.alpha(if hov { 0.48 } else { 0.26 }) };
            c.rrect(bx, at, bw, tbh, bw / 2.0, col);
        }

        // ---- sidebar on top
        self.sidebar(c);

        // ---- toast
        if let Some((s, _)) = self.toast.clone() {
            let ts = st(13.0, 560.0);
            let tw = self.text.width(&s, &ts) + 36.0;
            let (tx, ty) = (x0 + (cw - tw) / 2.0, wh - 64.0);
            glass::shadow(c, tx, ty + 4.0, tw, 38.0, 19.0, 12.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.4));
            c.rrect(tx, ty, tw, 38.0, 19.0, Rgba::rgb8(44, 50, 62, 0.96));
            c.text(&mut self.text, tx + 18.0, ty + 19.0, &s, &ts, FG);
        }
    }

    fn header(&mut self, c: &mut Canvas, x0: f32, cw: f32, searching: bool) {
        let ts = st(27.0, 720.0);
        let cy = 50.0;
        if searching {
            c.text(&mut self.text, x0, cy, "Wyniki wyszukiwania", &ts, FG);
            return;
        }
        let mut x = x0;
        if let Some(par) = self.page.parent() {
            let pt = par.title();
            let w = self.text.width(pt, &ts);
            let hov = self.hit(x - 4.0, cy - 20.0, w + 8.0, 40.0, Hit::A(Act::Nav(par)));
            c.text(&mut self.text, x, cy, pt, &ts, if hov { FG2 } else { MUTED });
            x += w + 10.0;
            gl(c, &mut self.text, x + 10.0, cy + 1.0, "chevron-right", 20.0, FAINT);
            x += 30.0;
        }
        let title = self.page.title();
        let tw = self.text.width(title, &ts);
        let tmax = x0 + cw - x;
        let s = crate::bar::elide(&mut self.text, title, &ts, tmax);
        c.text(&mut self.text, x, cy, &s, &ts, FG);
        let _ = tw;
    }

    fn sidebar(&mut self, c: &mut Canvas) {
        let (sx, sy, sw, sh) = (SIDE_X, 8.0, SIDE_W, self.h - 16.0);
        Glass { radius: 20.0, tint: 0.30, shadow: false, corners: true, ..Default::default() }.paint(c, sx, sy, sw, sh);
        // traffic lights
        let cols = [Rgba::hex("#ff5f57"), Rgba::hex("#febc2e"), Rgba::hex("#28c840")];
        let any = matches!(self.hover, Some(Hit::Light(_)));
        for k in 0..3u8 {
            let lx = sx + 12.0 + k as f32 * 21.0;
            let ly = 17.0;
            self.hit(lx - 3.0, ly - 3.0, 19.0, 19.0, Hit::Light(k));
            let col = if self.focused || any { cols[k as usize] } else { Rgba::rgb8(120, 128, 140, 0.55) };
            c.rrect(lx, ly, 13.0, 13.0, 6.5, col);
            if any {
                c.icon(&mut self.text, lx + 6.5, ly + 6.5, ["x", "minus", "plus"][k as usize], 10.0, Rgba::rgb8(60, 20, 10, 0.75));
            }
        }
        // the account
        let (ax, ay) = (sx + 10.0, 46.0);
        let hov = self.hit(ax, ay, sw - 20.0, 54.0, Hit::A(Act::Nav(Page::Accounts)));
        if hov || self.page == Page::Accounts {
            c.rrect(ax, ay, sw - 20.0, 54.0, 12.0, WHITE.alpha(if self.page == Page::Accounts { 0.09 } else { 0.05 }));
        }
        self.avatar(c, ax + 8.0, ay + 7.0, 40.0);
        let name = self.d.user.1.clone();
        let ns = st(14.0, 650.0);
        let n = crate::bar::elide(&mut self.text, &name, &ns, sw - 90.0);
        c.text(&mut self.text, ax + 58.0, ay + 20.0, &n, &ns, FG);
        c.text(&mut self.text, ax + 58.0, ay + 38.0, "Konto lokalne", &st(11.5, 500.0), MUTED);
        // search
        let (fx, fy, fw, fh) = (sx + 12.0, 112.0, sw - 24.0, 34.0);
        let on = self.field == Some(Fid::Search);
        c.rrect(fx, fy, fw, fh, 10.0, WHITE.alpha(if on { 0.11 } else { 0.07 }));
        if on {
            stroke_rrect(c, fx, fy, fw, fh, 10.0, crate::theme::accent().alpha(0.75));
        }
        gl(c, &mut self.text, fx + 18.0, fy + fh / 2.0, "search", 15.0, MUTED);
        self.hit(fx, fy, fw, fh, Hit::Field(Fid::Search));
        let fs = st(13.0, 500.0);
        self.search.paint(c, &mut self.text, fx + 34.0, fy, fw - 44.0, fh, &fs, FG, "Znajdź ustawienie", on, false);
        // categories
        let mut y = 160.0;
        let cur = self.page.top();
        let searching = !self.search.text.trim().is_empty();
        let ls = st(13.5, 500.0);
        let lsb = st(13.5, 620.0);
        for p in SIDEBAR {
            if y + 38.0 > sy + sh {
                break;
            }
            let sel = p == cur && !searching;
            let hov = self.hit(sx + 8.0, y, sw - 16.0, 38.0, Hit::A(Act::Nav(p)));
            if sel {
                c.rrect(sx + 8.0, y + 1.0, sw - 16.0, 36.0, 9.0, WHITE.alpha(0.10));
                c.rrect(sx + 10.0, y + 11.0, 3.0, 16.0, 1.5, crate::theme::accent());
            } else if hov {
                c.rrect(sx + 8.0, y + 1.0, sw - 16.0, 36.0, 9.0, WHITE.alpha(0.05));
            }
            let tint = p.tint();
            let (tx, ty) = (sx + 22.0, y + 6.0);
            if let Some(path) = crate::gfx::rrect_path(tx, ty, 26.0, 26.0, 7.5) {
                crate::desktop::fill_lin(c, &path, (tx, ty), (tx, ty + 26.0), &[(0.0, tint.mix(WHITE, 0.18)), (1.0, tint.mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.12))]);
            }
            gl(c, &mut self.text, tx + 13.0, ty + 13.0, p.icon(), 15.0, WHITE);
            c.text(&mut self.text, sx + 60.0, y + 19.0, p.title(), if sel { &lsb } else { &ls }, FG);
            y += 38.0;
        }
    }

    fn avatar(&mut self, c: &mut Canvas, x: f32, y: f32, d: f32) {
        let face = self.d.face.clone();
        if !face.is_empty() {
            if let Some(pm) = self.imgs.get(&face, (d * 2.0) as u32, &mut self.icons) {
                crate::img::draw(c, &pm, x, y, d, d, d / 2.0, true, 1.0);
                return;
            }
        }
        let a = crate::theme::accent();
        if let Some(path) = crate::gfx::rrect_path(x, y, d, d, d / 2.0) {
            crate::desktop::fill_lin(c, &path, (x, y), (x + d, y + d), &[(0.0, a.mix(WHITE, 0.25)), (1.0, a.mix(Rgba::hex("#bf5af2"), 0.45))]);
        }
        let ini = super::pages::initials(&self.d.user.1);
        let s = st(d * 0.38, 680.0);
        let w = self.text.width(&ini, &s);
        c.text(&mut self.text, x + (d - w) / 2.0, y + d / 2.0, &ini, &s, WHITE);
    }

    // one block at y, returns the y after it
    fn block(&mut self, c: &mut Canvas, b: &Block, x0: f32, y: f32, cw: f32) -> f32 {
        let vis = |y0: f32, h: f32, wh: f32| y0 + h > HEAD - 4.0 && y0 < wh;
        let wh = self.h;
        match b {
            Block::Caption(s) => {
                let y = y + 14.0;
                if vis(y, 30.0, wh) {
                    c.text(&mut self.text, x0 + 2.0, y + 12.0, s, &st(14.0, 650.0), FG);
                }
                y + 32.0
            }
            Block::Note(s) => {
                let ns = st(12.5, 480.0);
                let lines = crate::gfx::wrap(&mut self.text, &crate::i18n::tr(s), &ns, cw - 8.0, 4);
                let mut yy = y + 4.0;
                for l in &lines {
                    if vis(yy, 18.0, wh) {
                        c.text(&mut self.text, x0 + 2.0, yy + 9.0, l, &ns, MUTED);
                    }
                    yy += 19.0;
                }
                yy + 12.0
            }
            Block::Card(rows) => {
                let hs: Vec<f32> = rows.iter().map(|r| self.row_h(r, cw)).collect();
                let total: f32 = hs.iter().sum();
                if vis(y, total, wh) {
                    c.rrect(x0, y, cw, total, 12.0, WHITE.alpha(0.05));
                    stroke_rrect(c, x0, y, cw, total, 12.0, HAIR);
                    let mut yy = y;
                    for (i, r) in rows.iter().enumerate() {
                        if i > 0 {
                            c.rrect(x0 + 56.0, yy, cw - 56.0, 1.0, 0.0, HAIR);
                        }
                        if vis(yy, hs[i], wh) {
                            self.row(c, r, x0, yy, cw, hs[i]);
                        }
                        yy += hs[i];
                    }
                }
                y + total + 10.0
            }
            Block::Hero { pic, initials, title, lines, act } => {
                let h = 156.0;
                if vis(y, h, wh) {
                    // a soft accent glow under the card
                    let a = crate::theme::accent();
                    if let Some(path) = crate::gfx::rrect_path(x0, y, cw, h, 16.0) {
                        crate::desktop::fill_lin(c, &path, (x0, y), (x0 + cw, y + h), &[(0.0, a.alpha(0.10)), (0.5, WHITE.alpha(0.05)), (1.0, WHITE.alpha(0.04))]);
                    }
                    stroke_rrect(c, x0, y, cw, h, 16.0, HAIR);
                    let (px, py, pw, ph) = (x0 + 16.0, y + 16.0, 220.0, 124.0);
                    let mut tx = x0 + 28.0;
                    if !pic.is_empty() {
                        let pm = self.thumb(pic, 440);
                        match pm {
                            Some(pm) => crate::img::draw(c, &pm, px, py, pw, ph, 10.0, true, 1.0),
                            None => c.rrect(px, py, pw, ph, 10.0, WHITE.alpha(0.06)),
                        }
                        stroke_rrect(c, px, py, pw, ph, 10.0, WHITE.alpha(0.14));
                        tx = px + pw + 24.0;
                    } else if !initials.is_empty() {
                        self.avatar(c, x0 + 28.0, y + 30.0, 96.0);
                        tx = x0 + 148.0;
                    }
                    let btn_w = act.as_ref().map(|(l, _)| self.text.width(l, &st(13.0, 560.0)) + 30.0).unwrap_or(0.0);
                    let tmax = x0 + cw - tx - btn_w - 32.0;
                    let ts = st(22.0, 720.0);
                    let t = crate::bar::elide(&mut self.text, title, &ts, tmax);
                    let lines: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();
                    let block_h = 30.0 + lines.len() as f32 * 21.0;
                    let mut ly = y + (h - block_h) / 2.0 + 14.0;
                    c.text(&mut self.text, tx, ly, &t, &ts, FG);
                    ly += 30.0;
                    for l in lines {
                        let ls = st(13.0, 480.0);
                        let s = crate::bar::elide(&mut self.text, l, &ls, tmax);
                        c.text(&mut self.text, tx, ly, &s, &ls, MUTED);
                        ly += 21.0;
                    }
                    if let Some((l, a)) = act {
                        self.button(c, x0 + cw - 24.0, y + h / 2.0, l, a.clone(), Btn::Plain);
                    }
                }
                y + h + 12.0
            }
            Block::Tiles(tiles) => {
                let n = if cw > 720.0 { 4 } else { 2 };
                let gap = 10.0;
                let tw = (cw - gap * (n as f32 - 1.0)) / n as f32;
                let th = 100.0;
                let mut yy = y;
                for (i, tl) in tiles.iter().enumerate() {
                    let col = i % n;
                    if i > 0 && col == 0 {
                        yy += th + gap;
                    }
                    let tx = x0 + col as f32 * (tw + gap);
                    if vis(yy, th, wh) {
                        self.tile(c, tl, tx, yy, tw, th);
                    }
                }
                yy + th + 12.0
            }
            Block::Swatches(sw) => {
                let h = 74.0;
                if vis(y, h, wh) {
                    c.rrect(x0, y, cw, h, 12.0, WHITE.alpha(0.05));
                    stroke_rrect(c, x0, y, cw, h, 12.0, HAIR);
                    let d = 34.0;
                    let mut x = x0 + 22.0;
                    let mut cur = "";
                    for (col, name, on) in sw {
                        let cy = y + h / 2.0;
                        if x + d > x0 + cw - 150.0 {
                            break;
                        }
                        let hov = self.hit(x - 4.0, cy - d / 2.0 - 4.0, d + 8.0, d + 8.0, Hit::A(Act::Accent(*col)));
                        if *on {
                            cur = name;
                            c.rrect(x - 4.0, cy - d / 2.0 - 4.0, d + 8.0, d + 8.0, d / 2.0 + 4.0, WHITE.alpha(0.9));
                            c.rrect(x - 2.0, cy - d / 2.0 - 2.0, d + 4.0, d + 4.0, d / 2.0 + 2.0, Rgba::rgb8(28, 32, 40, 1.0));
                        } else if hov {
                            c.rrect(x - 4.0, cy - d / 2.0 - 4.0, d + 8.0, d + 8.0, d / 2.0 + 4.0, WHITE.alpha(0.28));
                            c.rrect(x - 2.0, cy - d / 2.0 - 2.0, d + 4.0, d + 4.0, d / 2.0 + 2.0, Rgba::rgb8(28, 32, 40, 1.0));
                        }
                        let k = rgb(*col);
                        if let Some(path) = crate::gfx::rrect_path(x, cy - d / 2.0, d, d, d / 2.0) {
                            crate::desktop::fill_lin(c, &path, (x, cy - d / 2.0), (x, cy + d / 2.0), &[(0.0, k.mix(WHITE, 0.15)), (1.0, k)]);
                        }
                        if *on {
                            c.icon(&mut self.text, x + d / 2.0, cy, "check", 16.0, WHITE);
                        }
                        x += d + 16.0;
                    }
                    if !cur.is_empty() {
                        right_text(c, &mut self.text, x0 + cw - 22.0, y + h / 2.0, cur, &st(13.0, 560.0), FG2);
                    }
                }
                y + h + 10.0
            }
            Block::DeskPreview => {
                // the desktop in small: the wallpaper, the date + clock as set, the cards in their places
                let (pw, ph) = (cw, (cw * 0.46).round().min(340.0));
                if vis(y, ph, wh) {
                    let (pic, _) = self.wall_now();
                    match self.thumb(&pic, 960) {
                        Some(pm) => crate::img::draw(c, &pm, x0, y, pw, ph, 14.0, true, 1.0),
                        None => {
                            if let Some(p) = crate::gfx::rrect_path(x0, y, pw, ph, 14.0) {
                                crate::desktop::fill_lin(c, &p, (x0, y), (x0 + pw, y + ph), &[(0.0, Rgba::hex("#26324a")), (1.0, Rgba::hex("#141a26"))]);
                            }
                        }
                    }
                    if let Some(p) = crate::gfx::rrect_path(x0, y, pw, ph, 14.0) {
                        crate::desktop::fill_lin(c, &p, (x0, y), (x0, y + ph), &[(0.0, Rgba(0.0, 0.0, 0.0, 0.1)), (1.0, Rgba(0.0, 0.0, 0.0, 0.35))]);
                    }
                    stroke_rrect(c, x0, y, pw, ph, 14.0, WHITE.alpha(0.14));
                    // one scale for all: the date, the clock and the cards fit the preview's height
                    let k = (pw / 1920.0 * 1.6).min((ph - 52.0) / 600.0);
                    let mid = x0 + pw / 2.0;
                    let mut ty = y + 26.0;
                    if self.cfg.s("desk", "date") != "hide" {
                        let ds = st((22.0 * k).max(9.0), 600.0);
                        const DAYS: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];
                        let tm = crate::data::local(crate::data::now());
                        let s = format!("{}, {} {}", DAYS[tm.tm_wday as usize], tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]);
                        let w = self.text.width(&s, &ds);
                        c.text(&mut self.text, mid - w / 2.0, ty, &s, &ds, WHITE.alpha(0.8));
                        ty += (22.0 * k).max(9.0) * 1.3;
                    }
                    let csz = match self.cfg.s("desk", "clock").as_str() {
                        "small" => 96.0,
                        "hide" => 0.0,
                        _ => 150.0,
                    } * k;
                    if csz > 0.0 {
                        let wght = match self.cfg.s("desk", "clockw").as_str() {
                            "thin" => 250.0,
                            "bold" => 800.0,
                            _ => 620.0,
                        };
                        let col = match self.cfg.s("desk", "clockc").as_str() {
                            "accent" => crate::theme::accent(),
                            "warm" => Rgba::rgb8(0xf6, 0xd9, 0xb0, 1.0),
                            _ => WHITE.alpha(0.9),
                        };
                        let cs = st(csz, wght).spaced(-4.0 * k).tnum();
                        let s = crate::data::hm(crate::data::now());
                        let w = self.text.width(&s, &cs);
                        c.text(&mut self.text, mid - w / 2.0, ty + csz * 0.38, &s, &cs, col);
                        ty += csz * 0.9;
                    }
                    if self.cfg.s("desk", "sun") != "hide" {
                        c.rrect(mid - 40.0, ty + 4.0, 80.0, 1.0, 0.0, WHITE.alpha(0.3));
                        c.rrect(mid - 12.5, ty + 2.0, 5.0, 5.0, 2.5, WHITE);
                    }
                    // the cards: a click opens that widget's page
                    let shown = self.cfg.shown();
                    let ch = (312.0 * k).round();
                    let unit = 308.0 * k;
                    let gap = 16.0 * k;
                    let wfor = |s: &str| if s == "weather" { 548.0 * k } else { unit };
                    let row_w: f32 = shown.iter().map(|s| wfor(s)).sum::<f32>() + gap * (shown.len() as f32 - 1.0).max(0.0);
                    let mut sx = mid - row_w / 2.0;
                    let sy = (ty + 22.0 * k + 10.0).max(y + ph - ch - 22.0).min(y + ph - ch - 14.0);
                    if shown.is_empty() {
                        let ns = st(12.0, 550.0);
                        let w = self.text.width("Bez kart", &ns);
                        c.text(&mut self.text, mid - w / 2.0, y + ph - 40.0, "Bez kart", &ns, WHITE.alpha(0.6));
                    }
                    for s in &shown {
                        let Some(key) = crate::desktop::WIDGETS.iter().find(|w| w.0 == s).map(|w| w.0) else { continue };
                        let w = wfor(s);
                        let hot = self.hit(sx, sy, w, ch, Hit::A(Act::Nav(Page::Widget(key))));
                        c.rrect(sx, sy, w, ch, 12.0, Rgba(0.04, 0.06, 0.1, if hot { 0.55 } else { 0.42 }));
                        stroke_rrect(c, sx, sy, w, ch, 12.0, if hot { crate::theme::accent() } else { WHITE.alpha(0.22) });
                        let icx = sx + w / 2.0;
                        let icy = sy + ch / 2.0 - 12.0;
                        c.rrect(icx - 18.0, icy - 18.0, 36.0, 36.0, 18.0, crate::desktop::widget_color(key).alpha(0.9));
                        c.icon(&mut self.text, icx, icy, crate::desktop::widget_icon(key), 18.0, WHITE);
                        let ns = st(12.5, 650.0);
                        let name = crate::desktop::widget_name(key);
                        let nw = self.text.width(name, &ns);
                        c.text(&mut self.text, icx - nw / 2.0, sy + ch / 2.0 + 16.0, name, &ns, FG);
                        if hot {
                            let hs = st(10.5, 560.0);
                            let hw = self.text.width("Ustawienia", &hs);
                            c.text(&mut self.text, icx - hw / 2.0, sy + ch / 2.0 + 33.0, "Ustawienia", &hs, crate::theme::accent());
                        }
                        sx += w + gap;
                    }
                }
                y + ph + 14.0
            }
            Block::Thumbs(items) => {
                let gap = 12.0;
                let n = (((cw + gap) / 212.0).floor() as usize).max(2);
                let tw = (cw - gap * (n as f32 - 1.0)) / n as f32;
                let th = (tw * 124.0 / 220.0).round();
                let cell = th + 30.0;
                let mut yy = y;
                for (i, it) in items.iter().enumerate() {
                    let col = i % n;
                    if i > 0 && col == 0 {
                        yy += cell + gap;
                    }
                    let tx = x0 + col as f32 * (tw + gap);
                    if vis(yy, cell, wh) {
                        self.thumb_tile(c, it, tx, yy, tw, th);
                    }
                }
                if items.is_empty() { y } else { yy + cell + 14.0 }
            }
        }
    }

    fn thumb(&mut self, path: &str, max: u32) -> Option<std::rc::Rc<tiny_skia::Pixmap>> {
        if let Some(v) = self.thumbs.get(path) {
            return v.clone();
        }
        let v = crate::img::decode_file(path, max).map(std::rc::Rc::new);
        self.thumbs.insert(path.to_string(), v.clone());
        v
    }

    fn thumb_tile(&mut self, c: &mut Canvas, it: &Thumb, x: f32, y: f32, w: f32, h: f32) {
        let hov = self.hit(x, y, w, h + 28.0, Hit::A(it.act.clone()));
        let a = crate::theme::accent();
        if it.on {
            c.rrect(x - 3.0, y - 3.0, w + 6.0, h + 6.0, 13.0, a);
        }
        match self.thumb(&it.path, 240) {
            Some(pm) => crate::img::draw(c, &pm, x, y, w, h, 10.0, true, 1.0),
            None => {
                if let Some(path) = crate::gfx::rrect_path(x, y, w, h, 10.0) {
                    crate::desktop::fill_lin(c, &path, (x, y), (x + w, y + h), &[(0.0, Rgba::hex("#26324a")), (1.0, Rgba::hex("#141a26"))]);
                }
                gl(c, &mut self.text, x + w / 2.0, y + h / 2.0, "photo", 22.0, FAINT);
            }
        }
        if hov && !it.on {
            stroke_rrect(c, x, y, w, h, 10.0, WHITE.alpha(0.5));
        } else {
            stroke_rrect(c, x, y, w, h, 10.0, WHITE.alpha(0.10));
        }
        if it.on {
            c.rrect(x + w - 30.0, y + 8.0, 22.0, 22.0, 11.0, a);
            c.icon(&mut self.text, x + w - 19.0, y + 19.0, "check", 13.0, WHITE);
        }
        let ls = st(12.5, if it.on { 620.0 } else { 500.0 });
        let s = crate::bar::elide(&mut self.text, &it.name, &ls, w - 4.0);
        let sw = self.text.width(&s, &ls);
        c.text(&mut self.text, x + (w - sw) / 2.0, y + h + 15.0, &s, &ls, if it.on { FG } else { FG2 });
        let _ = it.key.len();
    }

    fn tile(&mut self, c: &mut Canvas, tl: &Tile, x: f32, y: f32, w: f32, h: f32) {
        let hov = self.hit(x, y, w, h, Hit::A(tl.act.clone()));
        c.rrect(x, y, w, h, 14.0, WHITE.alpha(if hov { 0.085 } else { 0.05 }));
        stroke_rrect(c, x, y, w, h, 14.0, HAIR);
        let (ix, iy) = (x + 16.0, y + 16.0);
        if let Some(path) = crate::gfx::rrect_path(ix, iy, 32.0, 32.0, 9.0) {
            crate::desktop::fill_lin(c, &path, (ix, iy), (ix, iy + 32.0), &[(0.0, tl.tint.mix(WHITE, 0.18)), (1.0, tl.tint.mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.12))]);
        }
        gl(c, &mut self.text, ix + 16.0, iy + 16.0, tl.icon, 17.0, WHITE);
        gl(c, &mut self.text, x + w - 20.0, y + 32.0, "chevron-right", 15.0, if hov { FG2 } else { FAINT });
        let ts = st(14.0, 650.0);
        c.text(&mut self.text, x + 16.0, y + 66.0, &tl.title, &ts, FG);
        let ss = st(12.0, 480.0);
        let s = crate::bar::elide(&mut self.text, &tl.sub, &ss, w - 32.0);
        c.text(&mut self.text, x + 16.0, y + 85.0, &s, &ss, MUTED);
    }

    // ---------------------------------------------------------------- rows
    fn chips_w(&mut self, items: &[(String, bool, Act)]) -> Vec<f32> {
        let cs = st(12.5, 560.0);
        items.iter().map(|(l, on, _)| self.text.width(l, &cs) + if *on { 46.0 } else { 28.0 }).collect()
    }
    fn seg_w(&mut self, items: &[(String, bool, Act)]) -> f32 {
        let s = st(12.5, 560.0);
        items.iter().map(|(l, _, _)| self.text.width(l, &s) + 28.0).sum::<f32>() + 6.0
    }
    // chips laid out from x in width w: (rows, positions)
    fn chips_layout(&mut self, items: &[(String, bool, Act)], w: f32) -> (usize, Vec<(f32, usize)>) {
        let ws = self.chips_w(items);
        let mut x = 0.0;
        let mut row = 0usize;
        let mut out = Vec::new();
        for cw in ws {
            if x > 0.0 && x + cw > w {
                x = 0.0;
                row += 1;
            }
            out.push((x, row));
            x += cw + 8.0;
        }
        (if out.is_empty() { 0 } else { row + 1 }, out)
    }
    fn seg_below(&mut self, items: &[(String, bool, Act)], cw: f32) -> bool {
        self.seg_w(items) > cw * 0.52
    }
    fn row_h(&mut self, r: &Row, cw: f32) -> f32 {
        let base = if r.sub.is_empty() { 54.0 } else { 66.0 };
        match &r.ctl {
            Ctl::Chips(items) if !items.is_empty() => {
                let (rows, _) = self.chips_layout(items, cw - 56.0 - 20.0);
                base - 4.0 + rows as f32 * 38.0 + 8.0
            }
            Ctl::Seg(items) if self.seg_below(items, cw) => base + 44.0,
            _ => base,
        }
    }

    fn row(&mut self, c: &mut Canvas, r: &Row, x0: f32, y: f32, cw: f32, h: f32) {
        // a row with a toggle is the toggle; a navigation row is a button
        let row_act = r.act.clone().or_else(|| if let Ctl::Toggle(_, a) = &r.ctl { Some(a.clone()) } else { None });
        let top_h = if r.sub.is_empty() { 54.0 } else { 66.0 };
        if let Some(a) = row_act {
            let hov = self.hit(x0, y, cw, h, Hit::A(a));
            if hov {
                c.rrect(x0 + 4.0, y + 3.0, cw - 8.0, h - 6.0, 9.0, WHITE.alpha(0.04));
            }
        }
        let cy = y + top_h / 2.0;
        // icon
        let mut drew = false;
        if !r.app_icon.is_empty() {
            if let Some(pm) = self.imgs.get(&r.app_icon, 56, &mut self.icons) {
                crate::img::draw(c, &pm, x0 + 16.0, cy - 14.0, 28.0, 28.0, 0.0, false, 1.0);
                drew = true;
            }
        }
        if !drew {
            gl(c, &mut self.text, x0 + 30.0, cy, r.icon, 19.0, FG2);
        }
        // the control's width on the right
        let xr = x0 + cw - 20.0;
        let ctl_w = self.ctl_w(&r.ctl, cw);
        let tmax = (xr - ctl_w - 18.0 - (x0 + 56.0)).max(80.0);
        let ts = st(14.0, 560.0);
        let title = crate::bar::elide(&mut self.text, &r.title, &ts, tmax);
        if r.sub.is_empty() {
            c.text(&mut self.text, x0 + 56.0, cy, &title, &ts, FG);
        } else {
            c.text(&mut self.text, x0 + 56.0, cy - 10.0, &title, &ts, FG);
            let ss = st(12.0, 470.0);
            let sub = crate::bar::elide(&mut self.text, &r.sub, &ss, tmax);
            c.text(&mut self.text, x0 + 56.0, cy + 11.0, &sub, &ss, MUTED);
        }
        match &r.ctl {
            Ctl::None => {}
            Ctl::Text(s) => {
                let vs = st(13.0, 500.0);
                let s = crate::bar::elide(&mut self.text, s, &vs, cw * 0.55);
                right_text(c, &mut self.text, xr, cy, &s, &vs, FG2);
            }
            Ctl::Chev => gl(c, &mut self.text, xr - 6.0, cy, "chevron-right", 16.0, FAINT),
            Ctl::Toggle(on, _) => {
                let (sw, sh) = (42.0, 22.0);
                let sx = xr - sw;
                let a = crate::theme::accent();
                if *on {
                    c.rrect(sx, cy - sh / 2.0, sw, sh, sh / 2.0, a);
                    c.rrect(sx + sw - 19.0, cy - 8.0, 16.0, 16.0, 8.0, WHITE);
                } else {
                    c.rrect(sx, cy - sh / 2.0, sw, sh, sh / 2.0, WHITE.alpha(0.08));
                    stroke_rrect(c, sx, cy - sh / 2.0, sw, sh, sh / 2.0, WHITE.alpha(0.42));
                    c.rrect(sx + 5.0, cy - 6.0, 12.0, 12.0, 6.0, WHITE.alpha(0.82));
                }
                right_text(c, &mut self.text, sx - 12.0, cy, if *on { "Wł." } else { "Wył." }, &st(13.0, 500.0), FG2);
            }
            Ctl::Button(l, a, k) => {
                self.button(c, xr, cy, l, a.clone(), *k);
            }
            Ctl::Buttons(bs) => {
                let mut x = xr;
                for (l, a, k) in bs.iter().rev() {
                    let w = self.button(c, x, cy, l, a.clone(), *k);
                    x -= w + 8.0;
                }
            }
            Ctl::Seg(items) => {
                if self.seg_below(items, cw) {
                    self.seg(c, x0 + 56.0, y + top_h - 4.0 + 20.0, items);
                } else {
                    let w = self.seg_w(items);
                    self.seg(c, xr - w, cy, items);
                }
            }
            Ctl::Chips(items) => {
                let (_, pos) = self.chips_layout(items, cw - 56.0 - 20.0);
                let ws = self.chips_w(items);
                let cs = st(12.5, 560.0);
                let a = crate::theme::accent();
                for (i, (l, on, act)) in items.iter().enumerate() {
                    let (px, row) = pos[i];
                    let (x, yy) = (x0 + 56.0 + px, y + top_h - 4.0 + row as f32 * 38.0);
                    let w = ws[i];
                    let hov = self.hit(x, yy, w, 30.0, Hit::A(act.clone()));
                    if *on {
                        c.rrect(x, yy, w, 30.0, 15.0, a);
                        c.icon(&mut self.text, x + 17.0, yy + 15.0, "check", 13.0, WHITE);
                        c.text(&mut self.text, x + 30.0, yy + 15.0, l, &cs, WHITE);
                    } else {
                        c.rrect(x, yy, w, 30.0, 15.0, WHITE.alpha(if hov { 0.13 } else { 0.075 }));
                        c.text(&mut self.text, x + 14.0, yy + 15.0, l, &cs, FG2);
                    }
                }
            }
            Ctl::Slider(v, sl, muted, mute_act) => {
                let pct = format!("{}%", (v * 100.0).round() as i32);
                let ps = st(12.5, 560.0).tnum();
                right_text(c, &mut self.text, xr, cy, &pct, &ps, FG2);
                let tw = (cw * 0.32).clamp(140.0, 260.0);
                let tx = xr - 58.0 - tw;
                let a = crate::theme::accent();
                let fill = if *muted { WHITE.alpha(0.35) } else { a };
                c.rrect(tx, cy - 2.0, tw, 4.0, 2.0, WHITE.alpha(0.16));
                c.rrect(tx, cy - 2.0, (tw * v.clamp(0.0, 1.0)).max(4.0), 4.0, 2.0, fill);
                let kx = tx + tw * v.clamp(0.0, 1.0);
                glass::shadow(c, kx - 9.0, cy - 8.0, 18.0, 18.0, 9.0, 4.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.35));
                c.rrect(kx - 9.0, cy - 9.0, 18.0, 18.0, 9.0, WHITE);
                self.hit(tx - 9.0, cy - 14.0, tw + 18.0, 28.0, Hit::Slider(*sl, tx, tw));
                let mx = tx - 26.0;
                let hov = self.hit(mx - 15.0, cy - 15.0, 30.0, 30.0, Hit::A(mute_act.clone()));
                if hov {
                    c.rrect(mx - 15.0, cy - 15.0, 30.0, 30.0, 8.0, WHITE.alpha(0.08));
                }
                let g = match sl {
                    Sl::Source => if *muted { "microphone" } else { "microphone" },
                    _ => if *muted { "volume-off" } else { "volume" },
                };
                gl(c, &mut self.text, mx, cy, g, 17.0, if *muted { crate::theme::DANGER } else { FG2 });
            }
            Ctl::Field(f, ph) => {
                let fw = (cw * 0.42).clamp(200.0, 340.0);
                let (fx, fy, fh) = (xr - fw, cy - 16.0, 32.0);
                let on = self.field == Some(*f);
                c.rrect(fx, fy, fw, fh, 8.0, WHITE.alpha(if on { 0.11 } else { 0.07 }));
                if on {
                    stroke_rrect(c, fx, fy, fw, fh, 8.0, crate::theme::accent().alpha(0.75));
                }
                self.hit(fx, fy, fw, fh, Hit::Field(*f));
                let fs = st(13.0, 500.0);
                if *f == Fid::WifiPwd {
                    // the password as dots, the caret where it is
                    let n = self.wifi_pwd.text.chars().count();
                    let k = self.wifi_pwd.text[..self.wifi_pwd.caret].chars().count();
                    let mut m = crate::edit::LineEdit::new(&"•".repeat(n));
                    m.select(k * 3, k * 3);
                    m.paint(c, &mut self.text, fx + 12.0, fy, fw - 24.0, fh, &fs, FG, ph, on, false);
                } else {
                    let mut e = std::mem::take(match f {
                        Fid::Tz => &mut self.tz_q,
                        Fid::Keys => &mut self.keys_q,
                        Fid::BindCmd => &mut self.bind_cmd,
                        Fid::Place => &mut self.place_q,
                        _ => &mut self.search,
                    });
                    e.paint(c, &mut self.text, fx + 12.0, fy, fw - 24.0, fh, &fs, FG, ph, on, false);
                    *match f {
                        Fid::Tz => &mut self.tz_q,
                        Fid::Keys => &mut self.keys_q,
                        Fid::BindCmd => &mut self.bind_cmd,
                        Fid::Place => &mut self.place_q,
                        _ => &mut self.search,
                    } = e;
                }
            }
            Ctl::Bar(frac, label) => {
                let bw = (cw * 0.26).clamp(120.0, 220.0);
                let bx = xr - bw;
                c.rrect(bx, cy - 3.0, bw, 6.0, 3.0, WHITE.alpha(0.12));
                let col = if *frac > 0.9 { crate::theme::DANGER } else { crate::theme::accent() };
                c.rrect(bx, cy - 3.0, (bw * frac.clamp(0.0, 1.0)).max(6.0), 6.0, 3.0, col);
                right_text(c, &mut self.text, bx - 12.0, cy, label, &st(12.5, 560.0).tnum(), FG2);
            }
            Ctl::Keys(caps) => {
                let ks = st(12.0, 620.0);
                let mut x = xr;
                for k in caps.iter().rev() {
                    let w = self.text.width(k, &ks) + 18.0;
                    let w = w.max(28.0);
                    x -= w;
                    c.rrect(x, cy - 13.0, w, 26.0, 6.0, WHITE.alpha(0.09));
                    c.rrect(x, cy + 11.0, w, 2.0, 1.0, Rgba(0.0, 0.0, 0.0, 0.18));
                    stroke_rrect(c, x, cy - 13.0, w, 26.0, 6.0, WHITE.alpha(0.08));
                    let tw = self.text.width(k, &ks);
                    c.text(&mut self.text, x + (w - tw) / 2.0, cy, k, &ks, FG);
                    x -= 6.0;
                }
            }
        }
    }

    fn ctl_w(&mut self, ctl: &Ctl, cw: f32) -> f32 {
        let bs = st(13.0, 560.0);
        match ctl {
            Ctl::None | Ctl::Chips(_) => 0.0,
            Ctl::Text(s) => self.text.width(s, &st(13.0, 500.0)).min(cw * 0.55),
            Ctl::Chev => 20.0,
            Ctl::Toggle(..) => 92.0,
            Ctl::Button(l, ..) => self.text.width(l, &bs) + 30.0,
            Ctl::Buttons(v) => v.iter().map(|(l, ..)| self.text.width(l, &bs) + 38.0).sum(),
            Ctl::Seg(items) => {
                if self.seg_below(items, cw) {
                    0.0
                } else {
                    self.seg_w(items)
                }
            }
            Ctl::Slider(..) => (cw * 0.32).clamp(140.0, 260.0) + 58.0 + 44.0,
            Ctl::Field(..) => (cw * 0.42).clamp(200.0, 340.0),
            Ctl::Bar(..) => (cw * 0.26).clamp(120.0, 220.0) + 56.0,
            Ctl::Keys(caps) => caps.iter().map(|k| (self.text.width(k, &st(12.0, 620.0)) + 18.0).max(28.0) + 6.0).sum(),
        }
    }

    // a button with its right edge at xr, centred on cy; returns its width
    fn button(&mut self, c: &mut Canvas, xr: f32, cy: f32, label: &str, a: Act, k: Btn) -> f32 {
        let bs = st(13.0, 560.0);
        let w = self.text.width(label, &bs) + 30.0;
        let (x, y, h) = (xr - w, cy - 16.0, 32.0);
        let hov = self.hit(x, y, w, h, Hit::A(a));
        match k {
            Btn::Primary => {
                let ac = crate::theme::accent();
                c.rrect(x, y, w, h, 8.0, if hov { ac.mix(WHITE, 0.12) } else { ac });
                c.text(&mut self.text, x + 15.0, cy, label, &bs, WHITE);
            }
            Btn::Plain => {
                c.rrect(x, y, w, h, 8.0, WHITE.alpha(if hov { 0.15 } else { 0.09 }));
                stroke_rrect(c, x, y, w, h, 8.0, WHITE.alpha(0.07));
                c.text(&mut self.text, x + 15.0, cy, label, &bs, FG);
            }
        }
        w
    }

    // a segmented control starting at x, centred on cy
    fn seg(&mut self, c: &mut Canvas, x: f32, cy: f32, items: &[(String, bool, Act)]) {
        let s = st(12.5, 560.0);
        let w = self.seg_w(items);
        c.rrect(x, cy - 16.0, w, 32.0, 9.0, WHITE.alpha(0.06));
        stroke_rrect(c, x, cy - 16.0, w, 32.0, 9.0, WHITE.alpha(0.06));
        let mut sx = x + 3.0;
        for (l, on, a) in items {
            let iw = self.text.width(l, &s) + 28.0;
            let hov = self.hit(sx, cy - 13.0, iw, 26.0, Hit::A(a.clone()));
            if *on {
                glass::shadow(c, sx, cy - 12.0, iw, 24.0, 7.0, 4.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.25));
                c.rrect(sx, cy - 13.0, iw, 26.0, 7.0, WHITE.alpha(0.18));
            } else if hov {
                c.rrect(sx, cy - 13.0, iw, 26.0, 7.0, WHITE.alpha(0.06));
            }
            c.text(&mut self.text, sx + 14.0, cy, l, &s, if *on { FG } else { FG2 });
            sx += iw;
        }
    }
}
