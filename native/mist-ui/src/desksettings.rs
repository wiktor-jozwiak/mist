// "Dostosuj pulpit": every desktop widget setting in one sheet (right click on the desktop's cards, or
// `msg ui toggle desk`). Tab Układ: how many cards, which widget in which place, the clock / date / sun path,
// the cards' glass; one tab per widget with what it shows (the same options as its adjust button). Every click
// is saved to ~/.config/mist/widgets.json and applied at once (Act::DeskCfg), the desktop stays visible.
use crate::anim::{Anim, Ease};
use crate::desktop::{caps, chip, chip_w, groups, line_h, st, text, Cfg, FG2, MUTED};
use crate::gfx::Canvas;
use crate::pane::*;
use serde_json::Value;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

const TABS: [(&str, &str); 4] = [("desk", "Układ"), ("garmin", "Garmin"), ("weather", "Pogoda"), ("running", "Bieganie")];
const WIDGETS: [(&str, &str); 3] = [("garmin", "Garmin"), ("weather", "Pogoda"), ("running", "Bieganie")];
const W: f32 = 600.0;
const M: f32 = 40.0; // the layer's room for the sheet's shadow

#[derive(Clone, PartialEq, Debug)]
enum H {
    Tab(usize),
    Opt(&'static str, &'static str, Value),
    Slot(usize, &'static str),
    Show(&'static str),
    Done,
    Sheet,
}

pub struct DeskSettings {
    cfg: Cfg,
    tab: usize,
    ptr: (f32, f32),
    hits: Vec<(f32, f32, f32, f32, H)>,
    hot: Option<H>, // under the pointer at the start of the paint (the hits are rebuilt while painting)
    o: Anim,
}

impl DeskSettings {
    // opened on a card: its tab first
    pub fn new(card: &str) -> DeskSettings {
        let mut o = Anim::new(0.0);
        o.set(1.0, 140.0, Ease::OutCubic);
        let tab = TABS.iter().position(|t| t.0 == card).unwrap_or(0);
        DeskSettings { cfg: Cfg::load(), tab, ptr: (-1.0, -1.0), hits: Vec::new(), hot: None, o }
    }
    fn hit(&self) -> Option<H> {
        let p = self.ptr;
        self.hits.iter().rev().find(|r| p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3).map(|r| r.4.clone())
    }
    // a row of chips, wrapped at the sheet's width; returns the y under it
    fn chips(&mut self, l: &mut Canvas, cx: &mut Ctx, ox: f32, oy: f32, y: f32, label: &str, items: Vec<(String, bool, H)>) -> f32 {
        let ls = st(11.0, 600.0);
        text(l, cx.t, 24.0, y, label, &ls, MUTED, true, None);
        let mut cy = y + line_h(cx.t, &ls) + 6.0;
        let mut x = 24.0;
        let hot = self.hot.clone();
        for (lab, on, h) in items {
            let cw = chip_w(cx.t, &lab, on);
            if x > 24.0 && x + cw > W - 24.0 {
                x = 24.0;
                cy += 32.0;
            }
            chip(l, cx.t, x, cy, &lab, on, hot.as_ref() == Some(&h), 1.0);
            self.hits.push((ox + x, oy + cy, cw, 26.0, h));
            x += cw + 6.0;
        }
        cy + 26.0 + 14.0
    }
}

impl Pane for DeskSettings {
    fn name(&self) -> &'static str {
        "desk"
    }
    fn animating(&self) -> bool {
        self.o.running()
    }
    fn hand(&self) -> bool {
        !matches!(self.hit(), None | Some(H::Sheet))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hot = self.hit();
        self.hits.clear();
        // the content first (its height decides the sheet's), then glass under it
        let mut body = Canvas::new(W as u32, 1000);
        let (ox, oy) = ((c.w() - W - 40.0).round(), 64.0);
        // header: title · Gotowe
        caps(&mut body, cx.t, 24.0, 22.0, "Dostosuj pulpit", FG2, None);
        let dw = chip_w(cx.t, "Gotowe", true);
        let hot = self.hot.clone();
        chip(&mut body, cx.t, W - 20.0 - dw, 16.0, "Gotowe", true, hot == Some(H::Done), 1.0);
        self.hits.push((ox + W - 20.0 - dw, oy + 16.0, dw, 26.0, H::Done));
        // tabs
        let mut x = 24.0;
        for (i, (_, lab)) in TABS.iter().enumerate() {
            let cw = chip_w(cx.t, lab, i == self.tab);
            chip(&mut body, cx.t, x, 56.0, lab, i == self.tab, hot == Some(H::Tab(i)), 1.0);
            self.hits.push((ox + x, oy + 56.0, cw, 26.0, H::Tab(i)));
            x += cw + 6.0;
        }
        crate::desktop::rect(&mut body, 24.0, 96.0, W - 48.0, 1.0, 0.0, crate::desktop::HAIR);
        let mut y = 110.0;
        let card = TABS[self.tab].0;
        if card == "desk" {
            let order = self.cfg.order();
            for gr in groups("desk") {
                let cur = self.cfg.opt("desk", gr.key);
                let items = gr.opts.iter().map(|(v, lab)| (lab.to_string(), &cur == v, H::Opt("desk", gr.key, v.clone()))).collect();
                y = self.chips(&mut body, cx, ox, oy, y, gr.label, items);
                // after the count: which widget goes where
                if gr.key == "cards" {
                    for i in 0..self.cfg.cards() {
                        let items = WIDGETS.iter().map(|(k, lab)| (lab.to_string(), order.get(i).map(|o| o == k).unwrap_or(false), H::Slot(i, k))).collect();
                        y = self.chips(&mut body, cx, ox, oy, y, &format!("Miejsce {}", i + 1), items);
                    }
                }
            }
        } else {
            if !self.cfg.shown().iter().any(|k| k == card) {
                let ns = st(13.0, 550.0);
                text(&mut body, cx.t, 24.0, y, "Ta karta jest teraz ukryta.", &ns, FG2, true, None);
                let lw = crate::desktop::width(cx.t, "Ta karta jest teraz ukryta.", &ns);
                let pw = chip_w(cx.t, "Pokaż na pulpicie", true);
                let by = y + (line_h(cx.t, &ns) - 26.0) / 2.0;
                chip(&mut body, cx.t, 24.0 + lw + 12.0, by, "Pokaż na pulpicie", true, hot == Some(H::Show(card)), 1.0);
                self.hits.push((ox + 24.0 + lw + 12.0, oy + by, pw, 26.0, H::Show(card)));
                y += 26.0 + 18.0;
            }
            for gr in groups(card) {
                let cur = self.cfg.opt(card, gr.key);
                let label = if gr.multi && gr.max > 0 { format!("{}  ·  maks. {}", gr.label, gr.max) } else { gr.label.to_string() };
                let items = gr.opts.iter().map(|(v, lab)| {
                    let on = if gr.multi { cur.as_array().map(|a| a.contains(v)).unwrap_or(false) } else { &cur == v };
                    (lab.to_string(), on, H::Opt(card, gr.key, v.clone()))
                }).collect();
                y = self.chips(&mut body, cx, ox, oy, y, &label, items);
            }
        }
        let h = (y + 6.0).min(c.h() - oy - 24.0);
        self.hits.insert(0, (ox, oy, W, h, H::Sheet));
        // the sheet: glass + its content, faded in as one layer (sheet-sized, not the monitor)
        let mut l = Canvas::new((W + 2.0 * M) as u32, (h + 2.0 * M) as u32);
        glass(&mut l, M, M, W, h, 24.0, 0.85);
        let clip = tiny_skia::Rect::from_xywh(M, M, W, h).and_then(|r| {
            let mut m = tiny_skia::Mask::new(l.pm.width(), l.pm.height())?;
            m.fill_path(&tiny_skia::PathBuilder::from_rect(r), tiny_skia::FillRule::Winding, false, tiny_skia::Transform::identity());
            Some(m)
        });
        l.pm.draw_pixmap(M as i32, M as i32, body.pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), clip.as_ref());
        glass_rim(&mut l, M, M, W, h, 24.0);
        let paint = tiny_skia::PixmapPaint { opacity: self.o.value(), ..Default::default() };
        c.pm.draw_pixmap((ox - M) as i32, (oy - M) as i32, l.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        let before = self.hit();
        self.ptr = (x, y);
        match ev {
            Ptr::Release(0x110) => match self.hit() {
                None => Act::Close,
                Some(H::Done) => Act::Close,
                Some(H::Sheet) => Act::None,
                Some(H::Tab(i)) => {
                    self.tab = i;
                    Act::Redraw
                }
                Some(H::Opt(card, key, v)) => {
                    if let Some(gr) = groups(card).into_iter().find(|g| g.key == key) {
                        self.cfg.click(card, &gr, &v);
                    }
                    Act::DeskCfg
                }
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
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape | Keysym::Return | Keysym::KP_Enter => Act::Close,
            Keysym::Tab | Keysym::Right => {
                self.tab = (self.tab + 1) % TABS.len();
                Act::Redraw
            }
            Keysym::ISO_Left_Tab | Keysym::Left => {
                self.tab = (self.tab + TABS.len() - 1) % TABS.len();
                Act::Redraw
            }
            _ => Act::None,
        }
    }
}
