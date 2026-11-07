// Mist Player's controls, drawn on the CPU (tiny-skia) into the window's wl_shm surface above the video
// plane. Painted only when their state changes: the clock and the seek bar move once a second while visible,
// nothing at all while hidden (the window keeps its last, transparent buffer).
use super::*;
use crate::gfx::{stroke_rrect, Canvas, Rgba};
use crate::text::Style;

const FG: Rgba = Rgba(244.0 / 255.0, 247.0 / 255.0, 251.0 / 255.0, 0.97);
const FG2: Rgba = Rgba(222.0 / 255.0, 230.0 / 255.0, 242.0 / 255.0, 0.86);
const MUTED: Rgba = Rgba(206.0 / 255.0, 216.0 / 255.0, 232.0 / 255.0, 0.74);
const FAINT: Rgba = Rgba(196.0 / 255.0, 208.0 / 255.0, 226.0 / 255.0, 0.6);
const ACC: Rgba = Rgba(138.0 / 255.0, 214.0 / 255.0, 208.0 / 255.0, 1.0);
const ACC2: Rgba = Rgba(168.0 / 255.0, 228.0 / 255.0, 223.0 / 255.0, 1.0);
const RIM: Rgba = Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16);
const HAIR: Rgba = Rgba(200.0 / 255.0, 214.0 / 255.0, 236.0 / 255.0, 0.1);
const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);
const INK: Rgba = Rgba(11.0 / 255.0, 17.0 / 255.0, 24.0 / 255.0, 1.0);
const SMOKE: Rgba = Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.78);

fn glyph(name: &str) -> Option<char> {
    let c = match name {
        "play" => 0xf691,
        "pause" => 0xf690,
        "next" => 0xf694,
        "prev" => 0xf693,
        "back10" => 0xfaba,
        "fwd10" => 0xfac2,
        "vol0" => 0xeb50,
        "vol1" => 0xeb4f,
        "vol2" => 0xeb51,
        "full" => 0xeaea,
        "unfull" => 0xeaf1,
        "list" => 0xeec0,
        "cc" => 0xf55a,
        "music" => 0xeafc,
        "open" => 0xfaf7,
        "x" => 0xeb55,
        "repeat" => 0xeb72,
        "repeat1" => 0xeb71,
        "shuffle" => 0xf000,
        "movie" => 0xeafa,
        "clock" => 0xea70,
        "head" => 0xeabd,
        "check" => 0xea5e,
        "plus" => 0xeaa0,
        "kbd" => 0xebd6,
        "crop" => 0xea85,
        "minus" => 0xeaf2,
        "close" => 0xeb55,
        "maxi" => 0xeb0b,
        "pip" => 0xed35,
        "pipoff" => 0xed43,
        "chapters" => 0xef11,
        "resumeq" => 0xeb72,
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
fn sk(c: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.0.clamp(0.0, 1.0), c.1.clamp(0.0, 1.0), c.2.clamp(0.0, 1.0), c.3.clamp(0.0, 1.0)).unwrap()
}
fn elide(t: &mut Text, s: &str, st: &Style, max: f32) -> String {
    crate::bar::elide(t, s, st, max)
}
// hsl (h in degrees) to colour
fn hsl(h: f32, s: f32, l: f32) -> Rgba {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Rgba(r + m, g + m, b + m, 1.0)
}
fn centered(c: &mut Canvas, t: &mut Text, cx: f32, cy: f32, s: &str, st: &Style, col: Rgba) {
    let w = t.width(s, st);
    c.text(t, cx - w / 2.0, cy, s, st, col);
}

impl Player {
    // a track's own hue (from its name): the placeholder art and the backdrop differ per track
    fn track_hue(&self) -> f32 {
        let h = self.cur.bytes().fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193));
        (h % 360) as f32
    }
    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, k: Hit) -> bool {
        let on = self.hover.as_ref() == Some(&k);
        self.hits.push((x, y, w, h, k));
        on
    }
    // round glyph button (RBtn.qml): 38 px, or 52 px white for play
    #[allow(clippy::too_many_arguments)]
    fn rbtn(&mut self, c: &mut Canvas, x: f32, cy: f32, g: &str, k: Hit, on: bool, big: bool) -> f32 {
        let d = if big { 52.0 } else { 38.0 };
        let hov = self.hit(x, cy - d / 2.0, d, d, k);
        if big {
            crate::glass::shadow(c, x, cy - d / 2.0 + 3.0, d, d, d / 2.0, 12.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.28));
            c.rrect(x, cy - d / 2.0, d, d, d / 2.0, WHITE.alpha(if hov { 0.98 } else { 0.92 }));
        } else if hov || on {
            c.rrect(x, cy - d / 2.0, d, d, d / 2.0, WHITE.alpha(if on { 0.12 } else { 0.10 }));
        }
        let col = if big { INK } else if on { ACC2 } else if hov { FG } else { FG2 };
        gl(c, &mut self.text, x + d / 2.0 + if big && g == "play" { 1.5 } else { 0.0 }, cy, g, if big { 24.0 } else { 20.0 }, col);
        d
    }
    // a slider; `cuts` (fractions) split it into segments with a small gap (chapters), the one under the
    // pointer drawn a little taller
    #[allow(clippy::too_many_arguments)]
    fn bar(&mut self, c: &mut Canvas, x: f32, cy: f32, w: f32, v: f64, k: Hit, hover_v: Option<f64>, cuts: &[f32]) {
        let hov = self.hit(x - 6.0, cy - 10.0, w + 12.0, 20.0, k.clone()) || self.drag.as_ref() == Some(&k);
        let h = if hov { 6.0 } else { 4.0 };
        let v = v.clamp(0.0, 1.0) as f32;
        let hv = hover_v.filter(|_| hov).map(|f| f as f32);
        let mut edges = vec![0.0];
        edges.extend(cuts.iter().copied().filter(|f| *f > 0.0 && *f < 1.0 && w * f > 6.0));
        edges.push(1.0);
        let gap = if edges.len() > 2 { 1.5 } else { 0.0 };
        for s in edges.windows(2) {
            let (a, b) = (s[0], s[1]);
            let x0 = x + w * a + if a > 0.0 { gap } else { 0.0 };
            let x1 = x + w * b - if b < 1.0 { gap } else { 0.0 };
            let under = edges.len() > 2 && hv.is_some_and(|f| f >= a && f < b);
            let sh = if under { h + 3.0 } else { h };
            let seg = |c: &mut Canvas, to: f32, col: Rgba| {
                let ww = to.min(x1) - x0;
                if ww > 0.5 {
                    c.rrect(x0, cy - sh / 2.0, ww, sh, (sh / 2.0).min(ww / 2.0), col);
                }
            };
            seg(c, x1, WHITE.alpha(0.16));
            if let Some(f) = hv {
                seg(c, x + w * f, WHITE.alpha(0.22));
            }
            seg(c, (x + w * v).max(x + h), FG);
        }
        if hov {
            c.rrect(x + w * v - 7.0, cy - 7.0, 14.0, 14.0, 7.0, WHITE);
        }
    }

    pub(super) fn paint_all(&mut self, c: &mut Canvas) {
        self.hits.clear();
        let (w, h) = (self.w, self.h);
        c.clear();
        let show_video = self.video && !self.cur.is_empty() && self.pip.is_none();
        if !show_video {
            self.paint_backdrop(c);
        } else {
            self.backdrop = None;
        }
        self.hit(0.0, 0.0, w, h, Hit::Picture);
        let chrome = self.chrome();
        if self.no_libs {
            let st = Style::ui(15.0, 600.0);
            centered(c, &mut self.text, w / 2.0, h / 2.0, &format!("Mist Player: {}", self.err), &st, FG);
            self.paint_lights(c);
            return;
        }
        if self.cur.is_empty() {
            self.paint_empty(c);
        } else if self.pip.is_some() {
            self.paint_pip_away(c);
        } else if !show_video {
            self.paint_artwork(c);
        }
        // big play / pause flash
        if let Some((g, t0)) = self.flash {
            let k = (t0.elapsed().as_secs_f32() / 0.65).min(1.0);
            let a = 1.0 - k * k * k;
            let s = 96.0 * (0.8 + 0.55 * (1.0 - (1.0 - k).powi(3)));
            c.rrect(w / 2.0 - s / 2.0, h / 2.0 - s / 2.0, s, s, s / 2.0, Rgba(0.0, 0.0, 0.0, 0.42 * a));
            gl(c, &mut self.text, w / 2.0 + if g == "play" { 3.0 } else { 0.0 }, h / 2.0, g, 40.0 * s / 96.0, WHITE.alpha(a));
        }
        if chrome {
            self.paint_top(c);
            if !self.cur.is_empty() {
                self.paint_controls(c);
            }
        }
        match self.panel {
            Panel::Speed | Panel::Tracks => self.paint_pop(c),
            Panel::Chapters if !self.chapters.is_empty() => self.paint_pop(c),
            Panel::List if !self.cur.is_empty() => self.paint_queue(c),
            Panel::Help => self.paint_help(c),
            Panel::Info if !self.cur.is_empty() => self.paint_info(c),
            _ => {}
        }
        if let Some((g, s, _)) = self.osd.clone() {
            let st = Style::ui(13.0, 600.0);
            let tw = self.text.width(&s, &st) + 17.0 + 9.0;
            let bw = tw + 30.0;
            let (x, y) = (w / 2.0 - bw / 2.0, 76.0);
            c.rrect(x, y, bw, 40.0, 20.0, SMOKE);
            stroke_rrect(c, x + 0.5, y + 0.5, bw - 1.0, 39.0, 19.5, RIM);
            gl(c, &mut self.text, x + 15.0 + 8.5, y + 20.0, g, 17.0, FG);
            c.text(&mut self.text, x + 15.0 + 26.0, y + 20.0, &s, &st, FG);
        }
        if let Some((p, _)) = self.resume_at {
            // centred just above the controls (or the bottom when they are hidden)
            let st = Style::ui(13.0, 520.0);
            let sb = Style::ui(12.0, 600.0);
            let msg = format!("Wznowiono od {}", time(p));
            let tw = self.text.width(&msg, &st);
            let bw2 = self.text.width("Od początku", &sb) + 18.0;
            let bw = 12.0 + 16.0 + 10.0 + tw + 10.0 + bw2 + 7.0;
            let (_, ctl_y, _, _) = self.controls_rect();
            let (x0, y) = ((w / 2.0 - bw / 2.0).round(), if chrome { ctl_y - 12.0 - 42.0 } else { h - 22.0 - 42.0 });
            crate::glass::shadow(c, x0, y + 4.0, bw, 42.0, 21.0, 16.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.25));
            c.rrect(x0, y, bw, 42.0, 21.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.82));
            stroke_rrect(c, x0 + 0.5, y + 0.5, bw - 1.0, 41.0, 20.5, RIM);
            gl(c, &mut self.text, x0 + 12.0 + 8.0, y + 21.0, "clock", 16.0, ACC2);
            c.text(&mut self.text, x0 + 38.0, y + 21.0, &msg, &st, FG);
            let bx = x0 + 38.0 + tw + 10.0;
            let hov = self.hit(bx, y + 7.0, bw2, 28.0, Hit::FromStart);
            c.rrect(bx, y + 7.0, bw2, 28.0, 14.0, WHITE.alpha(if hov { 0.18 } else { 0.10 }));
            c.text(&mut self.text, bx + 9.0, y + 21.0, "Od początku", &sb, FG);
        }
        if self.drop_hover {
            c.rrect(14.0, 14.0, w - 28.0, h - 28.0, 22.0, Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.12));
            stroke_rrect(c, 15.0, 15.0, w - 30.0, h - 30.0, 21.0, Rgba::hex("#0A84FF"));
            gl(c, &mut self.text, w / 2.0, h / 2.0 - 16.0, "plus", 40.0, Rgba::hex("#409CFF"));
            let st = Style::ui(15.0, 600.0);
            let s = if self.cur.is_empty() { "Upuść, aby odtworzyć" } else { "Upuść, aby odtworzyć  ·  Shift: dodaj do kolejki" };
            centered(c, &mut self.text, w / 2.0, h / 2.0 + 24.0, s, &st, FG);
        }
    }

    fn paint_backdrop(&mut self, c: &mut Canvas) {
        // the gradients are the costly part of a repaint: drawn once per size, then copied
        let key = (c.pm.width(), c.pm.height(), self.cur.is_empty());
        if let Some((w, h, e, pm)) = &self.backdrop
            && (*w, *h, *e) == key
        {
            c.pm.data_mut().copy_from_slice(pm.data());
            return;
        }
        self.paint_backdrop_fresh(c);
        self.backdrop = Some((key.0, key.1, key.2, c.pm.clone()));
    }
    fn paint_backdrop_fresh(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        c.rrect(0.0, 0.0, w, h, 0.0, Rgba::rgb8(14, 19, 29, 1.0));
        // soft colour blobs (static: no per-frame work); the album art's colours when there is one
        let a = if self.cur.is_empty() { 0.18 } else if self.cover_cols.is_some() { 0.5 } else { 0.4 };
        let m = w.max(h);
        let hue = self.track_hue();
        let cols = match self.cover_cols {
            Some(c) => c,
            None if !self.cur.is_empty() => [hsl(hue, 0.65, 0.45), hsl(hue + 45.0, 0.6, 0.42), hsl(hue - 60.0, 0.55, 0.45)],
            None => [Rgba::hex("#5E5CE6"), Rgba::hex("#BF5AF2"), Rgba::hex("#64D2FF")],
        };
        for (col, fx, fy, fs) in [(cols[0], 0.10, 0.15, 0.55), (cols[1], 0.75, 0.25, 0.45), (cols[2], 0.45, 0.85, 0.4)] {
            let r = m * fs / 2.0;
            let (cx, cy) = (w * fx, h * fy);
            let stops = vec![tiny_skia::GradientStop::new(0.0, sk(col.alpha(a))), tiny_skia::GradientStop::new(1.0, sk(col.alpha(0.0)))];
            if let Some(sh) = tiny_skia::RadialGradient::new(tiny_skia::Point::from_xy(cx, cy), tiny_skia::Point::from_xy(cx, cy), r * 1.4, stops, tiny_skia::SpreadMode::Pad, tiny_skia::Transform::identity()) {
                let p = tiny_skia::Paint { shader: sh, anti_alias: true, ..Default::default() };
                if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
                    c.pm.fill_rect(rect, &p, tiny_skia::Transform::identity(), None);
                }
            }
        }
    }
    fn paint_lights(&mut self, c: &mut Canvas) {
        if self.fs {
            return;
        }
        let cols = [Rgba::hex("#ff5f57"), Rgba::hex("#febc2e"), Rgba::hex("#28c840")];
        let any = matches!(self.hover, Some(Hit::Light(_)));
        for k in 0..3 {
            let x = 18.0 + k as f32 * 21.0;
            self.hit(x - 3.0, 15.0, 19.0, 19.0, Hit::Light(k as u8));
            c.rrect(x, 18.0, 13.0, 13.0, 6.5, cols[k]);
            if any {
                gl(c, &mut self.text, x + 6.5, 24.5, ["close", "minus", "maxi"][k], 10.0, Rgba::rgb8(60, 20, 10, 0.75));
            }
        }
    }
    fn paint_top(&mut self, c: &mut Canvas) {
        let w = self.w;
        if self.video && !self.cur.is_empty() {
            let path = tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w, 104.0).unwrap());
            crate::desktop::fill_lin(c, &path, (0.0, 0.0), (0.0, 104.0), &[(0.0, Rgba(0.0, 0.0, 0.0, 0.55)), (1.0, Rgba(0.0, 0.0, 0.0, 0.0))]);
        }
        self.hit(0.0, 0.0, w, 64.0, Hit::Drag);
        self.paint_lights(c);
        if !self.cur.is_empty() && self.video {
            let st = Style::ui(14.0, 600.0);
            let nt = self.nice_title();
            let t = elide(&mut self.text, &nt, &st, w - 260.0);
            let mut parts = vec![];
            if let Some(i) = self.chapter_now() {
                let n = &self.chapters[i].1;
                parts.push(if n.is_empty() { format!("Rozdział {}", i + 1) } else { n.clone() });
            }
            if self.queue.len() > 1 {
                parts.push(format!("{} z {}", self.idx + 1, self.queue.len()));
            }
            let sub = elide(&mut self.text, &parts.join("  ·  "), &Style::ui(11.0, 450.0), w - 260.0);
            let cy = if sub.is_empty() { 30.0 } else { 24.0 };
            centered(c, &mut self.text, w / 2.0, cy, &t, &st, FG);
            if !sub.is_empty() {
                centered(c, &mut self.text, w / 2.0, 42.0, &sub, &Style::ui(11.0, 450.0), MUTED);
            }
        }
        let help = self.panel == Panel::Help;
        self.rbtn(c, w - 14.0 - 38.0, 32.0, "kbd", Hit::Help, help, false);
        self.rbtn(c, w - 14.0 - 80.0, 32.0, "open", Hit::Open, false, false);
    }

    fn paint_empty(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        let cw = (w - 60.0).min(560.0);
        let rec: Vec<Recent> = self.recents.iter().take(6).cloned().collect();
        let list_h = if rec.is_empty() { 0.0 } else { 8.0 + 26.0 + rec.len() as f32 * 48.0 + 12.0 };
        let resume_h = if self.saved_queue.is_empty() { 0.0 } else { 46.0 };
        let total = 92.0 + 18.0 + 34.0 + 8.0 + 20.0 + 18.0 + 40.0 + resume_h + list_h;
        let mut y = ((h - total) / 2.0).max(60.0);
        let x0 = (w - cw) / 2.0;
        // icon
        let ix = w / 2.0 - 46.0;
        if let Some(p) = crate::gfx::rrect_path(ix, y, 92.0, 92.0, 24.0) {
            crate::desktop::fill_lin(c, &p, (0.0, y), (0.0, y + 92.0), &[(0.0, Rgba::hex("#3a8dff")), (1.0, Rgba::hex("#5E5CE6"))]);
        }
        gl(c, &mut self.text, w / 2.0 + 3.0, y + 46.0, "play", 42.0, WHITE);
        y += 92.0 + 18.0;
        centered(c, &mut self.text, w / 2.0, y + 17.0, "Mist Player", &Style::ui(28.0, 700.0), FG);
        y += 34.0 + 8.0;
        centered(c, &mut self.text, w / 2.0, y + 10.0, "Upuść tu film lub muzykę, albo wybierz plik", &Style::ui(14.0, 450.0), MUTED);
        y += 20.0 + 18.0;
        let st = Style::ui(14.0, 600.0);
        let bw = self.text.width("Otwórz plik…", &st) + 17.0 + 8.0 + 36.0;
        let hov = self.hit(w / 2.0 - bw / 2.0, y, bw, 40.0, Hit::Open);
        c.rrect(w / 2.0 - bw / 2.0, y, bw, 40.0, 20.0, if hov { Rgba::hex("#409CFF") } else { Rgba::hex("#0A84FF") });
        gl(c, &mut self.text, w / 2.0 - bw / 2.0 + 18.0 + 8.5, y + 20.0, "open", 17.0, WHITE);
        c.text(&mut self.text, w / 2.0 - bw / 2.0 + 18.0 + 25.0, y + 20.0, "Otwórz plik…", &st, WHITE);
        y += 40.0 + 8.0;
        if !self.saved_queue.is_empty() {
            // the queue of the last session, where it stopped
            let n = self.saved_queue.len();
            let i = self.saved_idx.min(n - 1);
            let sm = Style::ui(13.0, 550.0);
            let name = elide(&mut self.text, &pretty(&self.saved_queue[i]), &sm, (cw - 150.0).max(120.0));
            let label = format!("Wznów kolejkę · {name} ({} z {n})", i + 1);
            let rw = self.text.width(&label, &sm) + 17.0 + 8.0 + 32.0;
            let hov = self.hit(w / 2.0 - rw / 2.0, y + 4.0, rw, 34.0, Hit::ResumeQueue);
            c.rrect(w / 2.0 - rw / 2.0, y + 4.0, rw, 34.0, 17.0, WHITE.alpha(if hov { 0.16 } else { 0.09 }));
            stroke_rrect(c, w / 2.0 - rw / 2.0 + 0.5, y + 4.5, rw - 1.0, 33.0, 16.5, HAIR);
            gl(c, &mut self.text, w / 2.0 - rw / 2.0 + 16.0 + 8.5, y + 21.0, "resumeq", 16.0, ACC2);
            c.text(&mut self.text, w / 2.0 - rw / 2.0 + 16.0 + 25.0, y + 21.0, &label, &sm, FG);
            y += 46.0;
        }
        if rec.is_empty() {
            return;
        }
        c.text(&mut self.text, x0 + 6.0, y + 13.0, "Ostatnio odtwarzane", &Style::ui(12.0, 600.0), FAINT);
        y += 26.0;
        let lh = rec.len() as f32 * 48.0 + 12.0;
        c.rrect(x0, y, cw, lh, 18.0, WHITE.alpha(0.05));
        stroke_rrect(c, x0 + 0.5, y + 0.5, cw - 1.0, lh - 1.0, 17.5, HAIR);
        let name = Style::ui(13.0, 520.0);
        let tm = Style::ui(12.0, 450.0).tnum();
        for (i, r) in rec.iter().enumerate() {
            let ry = y + 6.0 + i as f32 * 48.0;
            let hov = self.hit(x0 + 6.0, ry, cw - 12.0, 48.0, Hit::Recent(i));
            let xh = self.hover == Some(Hit::RecentForget(i));
            if hov || xh {
                c.rrect(x0 + 6.0, ry, cw - 12.0, 48.0, 12.0, WHITE.alpha(0.07));
            }
            gl(c, &mut self.text, x0 + 6.0 + 24.0, ry + 24.0, if is_audio(&r.path) { "music" } else { "movie" }, 20.0, FG2);
            let tw = cw - 12.0 - 46.0 - 90.0;
            let s = elide(&mut self.text, &pretty(&r.path), &name, tw);
            let has_bar = r.dur > 0.0 && r.pos > 0.0;
            c.text(&mut self.text, x0 + 6.0 + 46.0, ry + if has_bar { 19.0 } else { 24.0 }, &s, &name, FG);
            if has_bar {
                c.rrect(x0 + 52.0, ry + 31.0, tw * 0.6, 3.0, 1.5, WHITE.alpha(0.12));
                c.rrect(x0 + 52.0, ry + 31.0, tw * 0.6 * (r.pos / r.dur).min(1.0) as f32, 3.0, 1.5, ACC);
            }
            let ts = if r.pos > 0.0 { format!("{} / {}", time(r.pos / 1000.0), time(r.dur / 1000.0)) } else { time(r.dur / 1000.0) };
            let tww = self.text.width(&ts, &tm);
            let right = x0 + cw - 6.0 - if hov || xh { 44.0 } else { 14.0 };
            c.text(&mut self.text, right - tww, ry + 24.0, &ts, &tm, FAINT);
            if hov || xh {
                let bx = x0 + cw - 6.0 - 36.0;
                if self.hit(bx, ry + 9.0, 30.0, 30.0, Hit::RecentForget(i)) {
                    c.rrect(bx, ry + 9.0, 30.0, 30.0, 15.0, WHITE.alpha(0.10));
                }
                gl(c, &mut self.text, bx + 15.0, ry + 24.0, "x", 15.0, FG2);
            }
        }
    }

    // the audio view: the album art (or a placeholder), the title, artist and album, what comes next
    fn paint_artwork(&mut self, c: &mut Canvas) {
        let w = self.w;
        let (_, ctl_y, _, _) = self.controls_rect();
        let next = if self.queue.len() > 1 && (self.idx + 1) < self.queue.len() as i32 { Some(pretty(&self.queue[self.idx as usize + 1])) } else { None };
        let (title, artist) = self.track_meta();
        let sub = [artist, self.album.clone()];
        // the "resumed from" pill sits above the controls: the art makes room for it
        let pill = if self.resume_at.is_some() { 54.0 } else { 0.0 };
        let lines = 30.0 + if sub[0].is_empty() { 0.0 } else { 24.0 } + if sub[1].is_empty() { 0.0 } else { 20.0 };
        // the art as big as the room above the controls allows
        let room = ctl_y - 70.0 - 26.0 - lines - 20.0 - pill;
        let s = room.min(w - 80.0).min(340.0).max(96.0);
        let y = 64.0 + ((room - s) / 2.0).max(0.0);
        let x = w / 2.0 - s / 2.0;
        // paused: the art steps back a little
        let k = if self.paused { 0.92 } else { 1.0 };
        let (sx, sy, ss) = (x + s * (1.0 - k) / 2.0, y + s * (1.0 - k) / 2.0, s * k);
        let r = (ss * 0.06).max(10.0);
        let glow = self.cover_cols.map(|c| Rgba(c[2].0 * 0.35, c[2].1 * 0.35, c[2].2 * 0.35, 0.65)).unwrap_or(Rgba(0.0, 0.0, 0.0, 0.42));
        crate::glass::shadow(c, sx, sy + 16.0, ss, ss, r, 46.0, 0.0, glow);
        if let Some(pm) = self.cover.clone() {
            // scaled once per size (bicubic), then copied: a repaint per second costs a blit
            let (px, py, n) = (sx.round(), sy.round(), ss.round().max(1.0) as u32);
            if self.cover_px.as_ref().is_none_or(|(k, _)| *k != n) {
                let mut sc = Canvas::new(n, n);
                crate::img::draw(&mut sc, &pm, 0.0, 0.0, n as f32, n as f32, r, true, 1.0);
                self.cover_px = Some((n, sc.pm));
            }
            if let Some((_, sc)) = &self.cover_px {
                c.pm.draw_pixmap(px as i32, py as i32, sc.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
            }
        } else {
            let hue = self.track_hue();
            if let Some(p) = crate::gfx::rrect_path(sx, sy, ss, ss, r) {
                crate::desktop::fill_lin(c, &p, (sx, sy), (sx + ss, sy + ss), &[(0.0, hsl(hue, 0.55, 0.6)), (1.0, hsl(hue + 40.0, 0.6, 0.42))]);
            }
            gl(c, &mut self.text, w / 2.0, sy + ss / 2.0, "music", ss * 0.36, WHITE.alpha(0.92));
        }
        stroke_rrect(c, sx + 0.5, sy + 0.5, ss - 1.0, ss - 1.0, r - 0.5, RIM);
        let mut ty = y + s + 26.0 + 14.0;
        let st = Style::ui(26.0, 700.0);
        let t = elide(&mut self.text, &title, &st, w - 80.0);
        centered(c, &mut self.text, w / 2.0, ty, &t, &st, FG);
        ty += 16.0;
        if !sub[0].is_empty() {
            ty += 16.0;
            let st = Style::ui(17.0, 550.0);
            let s2 = elide(&mut self.text, &sub[0], &st, w - 80.0);
            // the artist in a light tint of the art (the accent without one)
            let tint = self.cover_cols.map(|c| Rgba(0.55 + 0.45 * c[0].0, 0.55 + 0.45 * c[0].1, 0.55 + 0.45 * c[0].2, 0.95)).unwrap_or_else(|| hsl(self.track_hue(), 0.6, 0.8));
            centered(c, &mut self.text, w / 2.0, ty, &s2, &st, tint);
            ty += 8.0;
        }
        if !sub[1].is_empty() {
            ty += 14.0;
            let st = Style::ui(14.0, 450.0);
            let s2 = elide(&mut self.text, &sub[1], &st, w - 80.0);
            centered(c, &mut self.text, w / 2.0, ty, &s2, &st, MUTED);
        }
        // up next, just above the controls
        if let Some(n) = next
            && pill == 0.0
            && ctl_y - ty > 54.0
        {
            let st = Style::ui(12.0, 500.0);
            let label = format!("Dalej: {}", elide(&mut self.text, &n, &st, (w - 120.0).min(420.0)));
            centered(c, &mut self.text, w / 2.0, ctl_y - 18.0, &label, &st, FAINT);
        }
    }

    pub(super) fn controls_rect(&self) -> (f32, f32, f32, f32) {
        let cw = (self.w - 48.0).min(860.0);
        ((self.w - cw) / 2.0, self.h - 22.0 - 106.0, cw, 106.0)
    }
    fn paint_controls(&mut self, c: &mut Canvas) {
        let (x, y, cw, ch) = self.controls_rect();
        crate::glass::shadow(c, x, y + 8.0, cw, ch, 28.0, 30.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.30));
        c.rrect(x, y, cw, ch, 28.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.72));
        stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 27.5, RIM);
        self.hit(x, y, cw, ch, Hit::Card);
        // seek row
        let tm = Style::ui(12.0, 550.0).tnum();
        let sy = y + 14.0 + 11.0;
        self.shown_sec = self.pos as i64;
        let cur = self.seek_preview.map(|f| f * self.dur).unwrap_or(self.pos);
        c.text(&mut self.text, x + 22.0, sy, &time(cur), &tm, FG2);
        let end = if self.remaining { format!("−{}", time(self.dur - self.pos)) } else { time(self.dur) };
        let ew = self.text.width(&end, &tm);
        c.text(&mut self.text, x + cw - 22.0 - ew, sy, &end, &tm, FG2);
        self.hit(x + cw - 22.0 - 58.0, sy - 11.0, 58.0, 22.0, Hit::Remaining);
        let (bx, bw) = (x + 22.0 + 52.0 + 8.0, cw - 44.0 - 52.0 - 58.0 - 16.0);
        let frac = if self.dur > 0.0 { cur / self.dur } else { 0.0 };
        let hover_v = if self.hover == Some(Hit::Seek) || self.drag == Some(Hit::Seek) { Some(((self.mouse.0 - bx) / bw).clamp(0.0, 1.0) as f64) } else { None };
        // chapter starts: gaps in the bar
        let cuts: Vec<f32> = if self.dur > 0.0 { self.chapters.iter().map(|(t, _)| (t / self.dur) as f32).collect() } else { vec![] };
        self.bar(c, bx, sy, bw, frac, Hit::Seek, hover_v, &cuts);
        if let (Some(hv), true) = (self.seek_preview.or(hover_v), self.dur > 0.0) {
            let st = Style::ui(12.0, 600.0).tnum();
            let at = time(hv * self.dur);
            let s = match self.chapters.iter().rev().find(|(t, _)| *t <= hv * self.dur) {
                Some((_, n)) if !n.is_empty() => format!("{}  ·  {at}", elide(&mut self.text, n, &st, 240.0)),
                _ => at,
            };
            let tw = self.text.width(&s, &st) + 16.0;
            let tx = (bx + bw * hv as f32 - tw / 2.0).clamp(bx - 20.0, bx + bw - tw + 20.0);
            if self.video && self.pip.is_none() {
                let (dur, tx2) = (self.dur, self.tx.clone());
                if let Some(pm) = self.thumbs.get(hv * dur, dur, tx2.as_ref()) {
                    let (tw2, th2) = (thumbs::TW as f32, thumbs::TH as f32);
                    let px = (tx + tw / 2.0 - tw2 / 2.0).clamp(8.0, self.w - tw2 - 8.0).round();
                    let py = (sy - 46.0 - 6.0 - th2).round();
                    crate::glass::shadow(c, px, py + 4.0, tw2, th2, 10.0, 14.0, 0.0, Rgba(0.0, 0.0, 0.0, 0.35));
                    if let Some(path) = crate::gfx::rrect_path(px, py, tw2, th2, 10.0) {
                        let paint = tiny_skia::Paint {
                            shader: tiny_skia::Pattern::new(pm.as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Nearest, 1.0, tiny_skia::Transform::from_translate(px, py)),
                            anti_alias: true,
                            ..Default::default()
                        };
                        c.pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
                    }
                    stroke_rrect(c, px + 0.5, py + 0.5, tw2 - 1.0, th2 - 1.0, 9.5, RIM);
                }
            }
            c.rrect(tx, sy - 34.0 - 12.0, tw, 24.0, 12.0, Rgba(20.0 / 255.0, 26.0 / 255.0, 38.0 / 255.0, 0.95));
            stroke_rrect(c, tx + 0.5, sy - 46.0 + 0.5, tw - 1.0, 23.0, 11.5, RIM);
            c.text(&mut self.text, tx + 8.0, sy - 34.0, &s, &st, FG);
        }
        // buttons row
        let cy = y + 44.0 + 27.0;
        let mut lx = x + 14.0;
        let vg = self.vol_glyph();
        lx += self.rbtn(c, lx, cy, vg, Hit::Mute, false, false) + 6.0;
        let vw = (cw * 0.12).min(110.0);
        let v = if self.muted { 0.0 } else { self.volume };
        self.bar(c, lx, cy, vw, v, Hit::Vol, None, &[]);
        let widths = 38.0 + 6.0 + 38.0 + 6.0 + 52.0 + 6.0 + 38.0 + 6.0 + 38.0;
        let mut mx = x + cw / 2.0 - widths / 2.0;
        mx += self.rbtn(c, mx, cy, "prev", Hit::Prev, false, false) + 6.0;
        mx += self.rbtn(c, mx, cy, "back10", Hit::Back10, false, false) + 6.0;
        let pg = if self.paused { "play" } else { "pause" };
        mx += self.rbtn(c, mx, cy, pg, Hit::Play, false, true) + 6.0;
        mx += self.rbtn(c, mx, cy, "fwd10", Hit::Fwd10, false, false) + 6.0;
        self.rbtn(c, mx, cy, "next", Hit::Next, false, false);
        // right: speed, tracks, queue, fullscreen
        let mut rx = x + cw - 14.0 - 38.0;
        let fg = if self.fs { "unfull" } else { "full" };
        self.rbtn(c, rx, cy, fg, Hit::Full, false, false);
        rx -= 40.0;
        // the extra buttons only where they do not reach the middle group (their keys work anyway)
        let room = cw / 2.0 - widths / 2.0 - 14.0 - 12.0 - 172.0;
        if self.video && room >= 40.0 {
            let on = self.pip.is_some();
            self.rbtn(c, rx, cy, if on { "pipoff" } else { "pip" }, Hit::Pip, on, false);
            rx -= 40.0;
        }
        let on = self.panel == Panel::List;
        self.rbtn(c, rx, cy, "list", Hit::List, on, false);
        rx -= 40.0;
        // audio / subtitles: only when there is something to choose (a plain MP3 has nothing)
        if self.video || !self.of_kind("sub").is_empty() || self.of_kind("audio").len() > 1 {
            let on = self.panel == Panel::Tracks || self.of_kind("sub").iter().any(|t| t.selected);
            self.rbtn(c, rx, cy, "cc", Hit::Tracks, on, false);
        } else {
            rx += 40.0;
        }
        if !self.chapters.is_empty() && room >= if self.video { 80.0 } else { 40.0 } {
            rx -= 40.0;
            let on = self.panel == Panel::Chapters;
            self.rbtn(c, rx, cy, "chapters", Hit::Chapters, on, false);
        }
        let st = Style::ui(14.0, 600.0).tnum();
        let label = rate(self.speed);
        let lw = self.text.width(&label, &st) + 22.0;
        rx -= lw + 2.0;
        let on = self.panel == Panel::Speed || (self.speed - 1.0).abs() > 0.01;
        let hov = self.hit(rx, cy - 19.0, lw, 38.0, Hit::Speed);
        if hov || on {
            c.rrect(rx, cy - 19.0, lw, 38.0, 19.0, WHITE.alpha(if on { 0.12 } else { 0.10 }));
        }
        c.text(&mut self.text, rx + 11.0, cy, &label, &st, if on { ACC2 } else if hov { FG } else { MUTED });
    }

    fn paint_pop(&mut self, c: &mut Canvas) {
        let (x, y, cw, _) = self.controls_rect();
        let mut rows: Vec<Row> = vec![];
        if self.panel == Panel::Chapters {
            // as many as fit above the controls, the current one in view
            let fit = (((y - 10.0 - 70.0 - 16.0 - 28.0) / 34.0).floor().max(3.0)) as usize;
            let now = self.chapter_now().unwrap_or(0);
            let n = self.chapters.len();
            let first = now.saturating_sub(fit / 2).min(n.saturating_sub(fit));
            rows.push(Row::Head(format!("ROZDZIAŁY · {n}")));
            for (i, (t, name)) in self.chapters.iter().enumerate().skip(first).take(fit) {
                let label = if name.is_empty() { format!("Rozdział {}", i + 1) } else { name.clone() };
                rows.push(Row::Pick(format!("{}\t{label}", time(*t)), i == now, RowAct::Chapter(*t)));
            }
        } else if self.panel == Panel::Speed {
            for r in SPEEDS {
                rows.push(Row::Pick(if r == 1.0 { "Normalna".into() } else { rate(r) }, (self.speed - r).abs() < 0.01, RowAct::Speed(r)));
            }
        } else {
            rows.push(Row::Head("AUDIO".into()));
            let a = self.of_kind("audio");
            if a.is_empty() {
                rows.push(Row::Pick("Brak".into(), false, RowAct::None));
            }
            for (i, t) in a.iter().enumerate() {
                rows.push(Row::Pick(track_name(t, i, "Ścieżka"), t.selected || a.len() == 1, RowAct::Aid(t.id)));
            }
            rows.push(Row::Head("NAPISY".into()));
            let s = self.of_kind("sub");
            rows.push(Row::Pick("Wyłączone".into(), !s.iter().any(|t| t.selected), RowAct::Sid(-1)));
            for (i, t) in s.iter().enumerate() {
                rows.push(Row::Pick(track_name(t, i, "Napisy"), t.selected, RowAct::Sid(t.id)));
            }
        }
        let ph: f32 = rows.iter().map(|r| if matches!(r, Row::Head(_)) { 28.0 } else { 34.0 }).sum::<f32>() + 16.0;
        let pw = if self.panel == Panel::Chapters { 330.0 } else { 270.0 };
        let (px, py) = (x + cw - pw, (y - 10.0 - ph).max(70.0));
        c.rrect(px, py, pw, ph, 20.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.92));
        stroke_rrect(c, px + 0.5, py + 0.5, pw - 1.0, ph - 1.0, 19.5, RIM);
        self.hit(px, py, pw, ph, Hit::Card);
        let mut ry = py + 8.0;
        let st = Style::ui(13.0, 500.0);
        for (i, r) in rows.iter().enumerate() {
            match r {
                Row::Head(t) => {
                    c.text(&mut self.text, px + 18.0, ry + 15.0, t, &Style::ui(11.0, 650.0), FAINT);
                    ry += 28.0;
                }
                Row::Pick(t, on, _) => {
                    let hov = self.hit(px + 8.0, ry, pw - 16.0, 34.0, Hit::Item(i));
                    if hov {
                        c.rrect(px + 8.0, ry, pw - 16.0, 34.0, 10.0, WHITE.alpha(0.08));
                    }
                    if *on {
                        gl(c, &mut self.text, px + 8.0 + 18.0, ry + 17.0, "check", 15.0, ACC2);
                    }
                    // "time\tname": the time right-aligned, tabular
                    let (t, at) = t.split_once('\t').map(|(a, b)| (b, Some(a))).unwrap_or((t.as_str(), None));
                    let aw = at.map(|a| self.text.width(a, &Style::ui(12.0, 500.0).tnum()) + 12.0).unwrap_or(0.0);
                    let s = elide(&mut self.text, t, &st, pw - 16.0 - 44.0 - aw);
                    c.text(&mut self.text, px + 8.0 + 36.0, ry + 17.0, &s, &st, if *on { FG } else { FG2 });
                    if let Some(a) = at {
                        let tm = Style::ui(12.0, 500.0).tnum();
                        let w2 = self.text.width(a, &tm);
                        c.text(&mut self.text, px + pw - 8.0 - 12.0 - w2, ry + 17.0, a, &tm, FAINT);
                    }
                    ry += 34.0;
                }
            }
        }
        self.rows = rows;
    }

    fn paint_queue(&mut self, c: &mut Canvas) {
        let (_, cy, _, _) = self.controls_rect();
        let sw = (self.w * 0.42).min(360.0);
        let (x, y) = (self.w - sw - 22.0, 64.0);
        let sh = (cy - 12.0 - y).max(120.0);
        c.rrect(x, y, sw, sh, 24.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.90));
        stroke_rrect(c, x + 0.5, y + 0.5, sw - 1.0, sh - 1.0, 23.5, RIM);
        self.hit(x, y, sw, sh, Hit::Card);
        c.text(&mut self.text, x + 18.0, y + 20.0, "Kolejka", &Style::ui(16.0, 700.0), FG);
        let n = self.queue.len();
        let cnt = format!("{n} {}", if n == 1 { "plik" } else if (2..5).contains(&(n % 10)) && !(12..15).contains(&(n % 100)) { "pliki" } else { "plików" });
        c.text(&mut self.text, x + 18.0, y + 38.0, &cnt, &Style::ui(11.0, 450.0), MUTED);
        let mut bx = x + sw - 10.0 - 38.0;
        self.rbtn(c, bx, y + 27.0, "plus", Hit::Add, false, false);
        bx -= 40.0;
        let rg = if self.repeat == 2 { "repeat1" } else { "repeat" };
        let ron = self.repeat != 0;
        self.rbtn(c, bx, y + 27.0, rg, Hit::Repeat, ron, false);
        bx -= 40.0;
        let son = self.shuffle;
        self.rbtn(c, bx, y + 27.0, "shuffle", Hit::Shuffle, son, false);
        c.rrect(x + 14.0, y + 54.0, sw - 28.0, 1.0, 0.0, HAIR);
        // rows (scrolled so the current one is visible)
        let top = y + 60.0;
        let avail = sh - 68.0;
        let vis = (avail / 46.0).floor().max(1.0) as usize;
        let max_first = n.saturating_sub(vis);
        self.queue_scroll = self.queue_scroll.min(max_first);
        let first = self.queue_scroll;
        let st = Style::ui(13.0, 450.0);
        let stn = Style::ui(13.0, 600.0);
        let small = Style::ui(10.0, 450.0);
        for (k, i) in (first..n.min(first + vis)).enumerate() {
            let ry = top + k as f32 * 46.0;
            let now = i as i32 == self.idx;
            let hov = self.hit(x + 8.0, ry, sw - 16.0, 46.0, Hit::Queue(i));
            let xh = self.hover == Some(Hit::QueueRemove(i));
            if now || hov || xh {
                c.rrect(x + 8.0, ry, sw - 16.0, 46.0, 12.0, WHITE.alpha(if now { 0.11 } else { 0.06 }));
            }
            // the current one: how far it got, a hairline along the row's bottom
            if now && self.dur > 0.0 {
                let f = (self.pos / self.dur).clamp(0.0, 1.0) as f32;
                let (lx, lw) = (x + 8.0 + 40.0, sw - 16.0 - 40.0 - 14.0);
                c.rrect(lx, ry + 41.0, lw, 2.0, 1.0, WHITE.alpha(0.10));
                c.rrect(lx, ry + 41.0, (lw * f).max(2.0), 2.0, 1.0, ACC);
            }
            if now {
                gl(c, &mut self.text, x + 8.0 + 21.0, ry + 23.0, if self.paused { "pause" } else { "vol2" }, 16.0, ACC2);
            } else {
                centered(c, &mut self.text, x + 8.0 + 21.0, ry + 23.0, &(i + 1).to_string(), &Style::ui(12.0, 450.0).tnum(), FAINT);
            }
            let p = self.queue[i].clone();
            let s = elide(&mut self.text, &pretty(&p), if now { &stn } else { &st }, sw - 16.0 - 40.0 - 40.0);
            c.text(&mut self.text, x + 8.0 + 40.0, ry + 17.0, &s, if now { &stn } else { &st }, if now { FG } else { FG2 });
            let res = self.resume_of(&p);
            let meta = format!("{}{}", ext(&p).to_uppercase(), if res > 0.0 && !now { format!(" · wznów od {}", time(res)) } else { String::new() });
            c.text(&mut self.text, x + 8.0 + 40.0, ry + 33.0, &meta, &small, FAINT);
            if hov || xh {
                let bx = x + sw - 8.0 - 4.0 - 30.0;
                if self.hit(bx, ry + 8.0, 30.0, 30.0, Hit::QueueRemove(i)) {
                    c.rrect(bx, ry + 8.0, 30.0, 30.0, 15.0, WHITE.alpha(0.10));
                }
                gl(c, &mut self.text, bx + 15.0, ry + 23.0, "x", 14.0, FG2);
            }
        }
    }

    // the main window while the picture is in the PiP window
    fn paint_pip_away(&mut self, c: &mut Canvas) {
        let (w, h) = (self.w, self.h);
        let cy = h / 2.0 - 60.0;
        c.rrect(w / 2.0 - 44.0, cy - 44.0, 88.0, 88.0, 26.0, WHITE.alpha(0.08));
        stroke_rrect(c, w / 2.0 - 43.5, cy - 43.5, 87.0, 87.0, 25.5, RIM);
        gl(c, &mut self.text, w / 2.0, cy, "pip", 40.0, ACC2);
        let st = Style::ui(20.0, 650.0);
        let nt = self.nice_title();
        let t = elide(&mut self.text, &nt, &st, w - 80.0);
        centered(c, &mut self.text, w / 2.0, cy + 44.0 + 30.0, &t, &st, FG);
        centered(c, &mut self.text, w / 2.0, cy + 44.0 + 56.0, "Odtwarzanie w oknie obraz w obrazie", &Style::ui(14.0, 450.0), MUTED);
        let sb = Style::ui(13.0, 600.0);
        let bw = self.text.width("Przywróć tutaj", &sb) + 17.0 + 8.0 + 32.0;
        let by = cy + 44.0 + 76.0;
        let hov = self.hit(w / 2.0 - bw / 2.0, by, bw, 36.0, Hit::Pip);
        c.rrect(w / 2.0 - bw / 2.0, by, bw, 36.0, 18.0, if hov { Rgba::hex("#409CFF") } else { Rgba::hex("#0A84FF") });
        gl(c, &mut self.text, w / 2.0 - bw / 2.0 + 16.0 + 8.5, by + 18.0, "pipoff", 16.0, WHITE);
        c.text(&mut self.text, w / 2.0 - bw / 2.0 + 16.0 + 25.0, by + 18.0, "Przywróć tutaj", &sb, WHITE);
    }
    // the PiP window's canvas over its video plane: clear, buttons while the pointer is in (or paused)
    pub(super) fn paint_pip(&mut self, c: &mut Canvas) {
        self.pip_hits.clear();
        let (w, h) = (c.w(), c.h());
        c.clear();
        if !self.pip_hover && !self.paused {
            return;
        }
        let path = tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w, h).unwrap());
        crate::desktop::fill_lin(c, &path, (0.0, 0.0), (0.0, h), &[(0.0, Rgba(0.0, 0.0, 0.0, 0.5)), (0.4, Rgba(0.0, 0.0, 0.0, 0.18)), (1.0, Rgba(0.0, 0.0, 0.0, 0.45))]);
        let (mx, my) = self.pip_mouse;
        let inside = |x: f32, y: f32, d: f32| mx >= x && mx < x + d && my >= y && my < y + d;
        // play / pause in the middle
        let d = (h * 0.24).clamp(36.0, 56.0);
        let (px, py) = (w / 2.0 - d / 2.0, h / 2.0 - d / 2.0);
        self.pip_hits.push((px, py, d, d, Hit::PipPlay));
        c.rrect(px, py, d, d, d / 2.0, WHITE.alpha(if inside(px, py, d) { 0.96 } else { 0.86 }));
        let g = if self.paused { "play" } else { "pause" };
        gl(c, &mut self.text, w / 2.0 + if self.paused { 1.5 } else { 0.0 }, h / 2.0, g, d * 0.45, INK);
        // back to the main window (top left), close (top right)
        for (x, g, k) in [(10.0, "pipoff", Hit::PipBack), (w - 10.0 - 30.0, "x", Hit::PipClose)] {
            self.pip_hits.push((x, 10.0, 30.0, 30.0, k));
            c.rrect(x, 10.0, 30.0, 30.0, 15.0, Rgba(0.0, 0.0, 0.0, if inside(x, 10.0, 30.0) { 0.55 } else { 0.35 }));
            gl(c, &mut self.text, x + 15.0, 25.0, g, 16.0, FG);
        }
        // where it is (drawn at hover / pause only: no repaint per second)
        if self.dur > 0.0 {
            let f = (self.pos / self.dur).clamp(0.0, 1.0) as f32;
            c.rrect(12.0, h - 10.0, w - 24.0, 3.0, 1.5, WHITE.alpha(0.25));
            c.rrect(12.0, h - 10.0, ((w - 24.0) * f).max(3.0), 3.0, 1.5, FG);
        }
    }

    fn paint_help(&mut self, c: &mut Canvas) {
        let rows = [
            ("Spacja / K", "Odtwarzaj / pauza"),
            ("← / →", "−5 s / +5 s  (Shift: 30 s)"),
            ("J / L", "−10 s / +10 s"),
            ("↑ / ↓ / kółko", "Głośność"),
            ("M", "Wycisz"),
            ("F / dwuklik", "Pełny ekran"),
            ("N / P  ·  > / <", "Następny / poprzedni"),
            ("PgUp / PgDn  ·  C", "Rozdział: poprzedni / następny / lista"),
            ("Shift+P", "Obraz w obrazie"),
            ("[ / ]  ·  { / }", "Wolniej / szybciej  (×½ / ×2)"),
            ("Backspace", "Normalna prędkość"),
            ("S / V", "Napisy: następne / ukryj"),
            ("Z / X", "Napisy −0,1 s / +0,1 s"),
            ("A / #", "Ścieżka audio"),
            ("0 – 9", "Skok do 0–90%"),
            (", / .", "Klatka wstecz / dalej"),
            ("W", "Wypełnij okno / cały obraz"),
            ("Shift+S", "Zrzut klatki"),
            ("Q / I", "Kolejka / informacje"),
            ("O", "Otwórz plik (też napisy)"),
            ("Ctrl+W", "Zamknij"),
        ];
        let (cw, ch) = (520.0, 20.0 + 34.0 + rows.len() as f32 * 28.0 + 20.0);
        let (x, y) = ((self.w - cw) / 2.0, ((self.h - ch) / 2.0).max(10.0));
        c.rrect(x, y, cw, ch, 26.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.92));
        stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 25.5, RIM);
        self.hit(x, y, cw, ch, Hit::Card);
        c.text(&mut self.text, x + 24.0, y + 32.0, "Skróty klawiszowe", &Style::ui(17.0, 700.0), FG);
        let st = Style::ui(13.0, 450.0);
        let ks = Style::ui(12.0, 600.0);
        for (i, (k, d)) in rows.iter().enumerate() {
            let ry = y + 20.0 + 34.0 + i as f32 * 28.0 + 14.0;
            c.text(&mut self.text, x + 24.0, ry, d, &st, FG2);
            let kw = self.text.width(k, &ks) + 16.0;
            let kx = x + cw - 24.0 - kw;
            c.rrect(kx, ry - 11.0, kw, 22.0, 7.0, WHITE.alpha(0.08));
            stroke_rrect(c, kx + 0.5, ry - 10.5, kw - 1.0, 21.0, 6.5, HAIR);
            c.text(&mut self.text, kx + 8.0, ry, k, &ks, FG);
        }
    }

    // I: what plays and how (codec, size, decoder, dropped frames); refreshed with the clock (once a second)
    fn paint_info(&mut self, c: &mut Canvas) {
        let Some(e) = self.eng.as_ref() else { return };
        let g = |k: &str| e.get_str(k).unwrap_or_default();
        let num = |k: &str| e.get_f64(k).unwrap_or(0.0);
        let vt = self.tracks.iter().find(|t| t.kind == "video" && t.selected).map(|t| t.codec.clone()).unwrap_or_default();
        let at = self.tracks.iter().find(|t| t.kind == "audio" && t.selected).map(|t| t.codec.clone()).unwrap_or_default();
        let (vw, vh) = (e.get_i64("video-params/w").unwrap_or(0), e.get_i64("video-params/h").unwrap_or(0));
        let hw = g("hwdec-current");
        let mut rows: Vec<(&str, String)> = vec![("Plik", pretty(&self.cur))];
        if self.video {
            rows.push(("Obraz", format!("{} · {vw}×{vh} · {} kl/s", vt.to_uppercase(), format!("{:.3}", num("container-fps")).trim_end_matches('0').trim_end_matches('.'))));
            rows.push(("Dekoder", if hw.is_empty() || hw == "no" { "programowy (CPU)".into() } else { format!("sprzętowy ({hw})") }));
            rows.push(("Pominięte klatki", format!("{} · dekoder {}", e.get_i64("frame-drop-count").unwrap_or(0), e.get_i64("decoder-frame-drop-count").unwrap_or(0))));
        }
        rows.push(("Dźwięk", format!("{} · {} Hz · {} kan.", at.to_uppercase(), e.get_i64("audio-params/samplerate").unwrap_or(0), e.get_i64("audio-params/channel-count").unwrap_or(0))));
        let br = num("video-bitrate") + num("audio-bitrate");
        if br > 0.0 {
            rows.push(("Przepływność", format!("{:.1} Mb/s", br / 1e6).replace('.', ",")));
        }
        let (cw, ch) = (480.0, 20.0 + 34.0 + rows.len() as f32 * 28.0 + 16.0);
        let (x, y) = (24.0, 76.0);
        c.rrect(x, y, cw, ch, 22.0, Rgba(14.0 / 255.0, 19.0 / 255.0, 29.0 / 255.0, 0.90));
        stroke_rrect(c, x + 0.5, y + 0.5, cw - 1.0, ch - 1.0, 21.5, RIM);
        self.hit(x, y, cw, ch, Hit::Card);
        c.text(&mut self.text, x + 20.0, y + 30.0, "Informacje", &Style::ui(16.0, 700.0), FG);
        let (ks, vs) = (Style::ui(12.0, 600.0), Style::ui(13.0, 450.0).tnum());
        for (i, (k, v)) in rows.iter().enumerate() {
            let ry = y + 20.0 + 34.0 + i as f32 * 28.0 + 12.0;
            c.text(&mut self.text, x + 20.0, ry, k, &ks, FAINT);
            let s = elide(&mut self.text, v, &vs, cw - 160.0);
            c.text(&mut self.text, x + 150.0, ry, &s, &vs, FG2);
        }
    }
}
