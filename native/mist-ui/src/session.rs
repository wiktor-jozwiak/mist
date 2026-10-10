// Session menu (was Session.qml; Super+Alt+C, the Control centre power button): dimmed screen, a glass
// card with the user and five round actions. Restart / shut down / log out ask like macOS ("…automatically
// in 60 s"). Keys 1–5, ←→ + Enter, Esc.
use crate::anim::{Anim, Ease};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::time::Instant;

struct Action {
    id: &'static str,
    icon: &'static str,
    t: &'static str,
    q: &'static str,
    n: &'static str,
}
const ACTIONS: [Action; 5] = [
    Action { id: "lock", icon: "lock", t: "Zablokuj", q: "", n: "" },
    Action { id: "logout", icon: "logout", t: "Wyloguj", q: "Czy na pewno chcesz zamknąć wszystkie aplikacje i się wylogować?", n: "wylogujesz się" },
    Action { id: "suspend", icon: "zzz", t: "Uśpij", q: "", n: "" },
    Action { id: "reboot", icon: "reload", t: "Uruchom ponownie", q: "Czy na pewno chcesz teraz ponownie uruchomić komputer?", n: "komputer uruchomi się ponownie" },
    Action { id: "poweroff", icon: "power", t: "Wyłącz", q: "Czy na pewno chcesz teraz wyłączyć komputer?", n: "komputer wyłączy się" },
];

fn cmd(id: &str) -> Vec<String> {
    let bin = format!("{}/.local/bin/mist-ui-bin", std::env::var("HOME").unwrap_or_default());
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match id {
        "lock" => vec![bin, "lock".into()],
        "logout" => s(&["loginctl", "terminate-session", &std::env::var("XDG_SESSION_ID").unwrap_or("self".into())]),
        "suspend" => vec!["sh".into(), "-c".into(), "(\"$0\" lock &); sleep 0.6; systemctl suspend".into(), bin],
        "reboot" => s(&["systemctl", "reboot"]),
        _ => s(&["systemctl", "poweroff"]),
    }
}

pub struct Session {
    sel: usize,
    ask: Option<usize>,
    asked_at: Instant,
    name: String,
    h: Anim,
    hover: Option<usize>,
    hits: Vec<((f32, f32, f32, f32), u8)>, // 0..4 actions, 10 cancel, 11 ok, 20 card
    ptr: (f32, f32),
    pressed_outside: bool,
}

impl Session {
    pub fn new() -> Session {
        Session { sel: 4, ask: None, asked_at: Instant::now(), name: full_name(), h: Anim::new(292.0), hover: None, hits: Vec::new(), ptr: (-1.0, -1.0), pressed_outside: false }
    }
    fn left(&self) -> i64 {
        60 - self.asked_at.elapsed().as_secs() as i64
    }
    fn pick(&mut self, i: usize) -> Act {
        let a = &ACTIONS[i];
        if !a.q.is_empty() && self.ask != Some(i) {
            self.ask = Some(i);
            self.asked_at = Instant::now();
            self.h.set(250.0, 160.0, Ease::OutCubic);
            return Act::Redraw;
        }
        Act::Run(cmd(a.id))
    }
    fn unask(&mut self) {
        self.ask = None;
        self.h.set(292.0, 160.0, Ease::OutCubic);
    }
    fn hit(&self, x: f32, y: f32) -> Option<u8> {
        self.hits.iter().rev().find(|h| inside((x, y), h.0)).map(|h| h.1)
    }
}

impl Pane for Session {
    fn name(&self) -> &'static str {
        "session"
    }
    fn namespace(&self) -> &'static str {
        "mist-session"
    }
    fn animating(&self) -> bool {
        self.h.running()
    }
    fn slow(&mut self) -> bool {
        self.ask.is_some()
    }
    fn timer(&mut self) -> Act {
        match self.ask {
            Some(i) if self.left() <= 0 => Act::Run(cmd(ACTIONS[i].id)),
            _ => Act::None,
        }
    }
    fn hand(&self) -> bool {
        matches!(self.hit(self.ptr.0, self.ptr.1), Some(0..=4))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        let t = &mut *cx.t;
        c.clear();
        self.hits.clear();
        dim(c);
        let (w, h) = (c.w(), c.h());
        let ch = self.h.value();
        let (x0, y0) = (((w - 600.0) / 2.0).round(), ((h - ch) / 2.0).round());
        glass(c, x0, y0, 600.0, ch, 26.0, 0.8);
        self.hits.push(((x0, y0, 600.0, ch), 20));
        let cxm = x0 + 300.0;
        match self.ask {
            None => {
                let mut y = y0 + 26.0;
                // avatar with initials
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_circle(cxm, y + 32.0, 32.0);
                if let Some(p) = pb.finish() {
                    crate::desktop::fill_lin(c, &p, (0.0, y), (0.0, y + 64.0), &[(0.0, Rgba::hex("#a9b1be")), (1.0, Rgba::hex("#6c7482"))]);
                }
                let ini: String = self.name.split(' ').filter(|s| !s.is_empty()).take(2).filter_map(|s| s.chars().next()).collect();
                let ih = lh(t, 24.0, 600.0);
                txt_c(c, t, cxm, y + 32.0 - ih / 2.0, &ini, 24.0, 600.0, Rgba(1.0, 1.0, 1.0, 1.0), 64.0);
                y += 64.0 + 10.0;
                txt_c(c, t, cxm, y, &self.name, 17.0, 600.0, T1, 560.0);
                y += lh(t, 17.0, 600.0) + 26.0;
                let rw = 5.0 * 100.0 + 4.0 * 14.0;
                for (i, a) in ACTIONS.iter().enumerate() {
                    let ax = cxm - rw / 2.0 + i as f32 * 114.0;
                    let on = i == self.sel;
                    let (bx, by) = (ax + 22.0, y);
                    let fill = if on { if a.id == "poweroff" { Rgba(238.0 / 255.0, 154.0 / 255.0, 170.0 / 255.0, 0.28) } else { Rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 0.18) } } else if self.hover == Some(i) { FILL2 } else { FILL };
                    crate::desktop::rect(c, bx, by, 56.0, 56.0, 28.0, fill);
                    if on {
                        crate::desktop::border(c, bx, by, 56.0, 56.0, 28.0, 1.0, Rgba(214.0 / 255.0, 224.0 / 255.0, 242.0 / 255.0, 0.16));
                    }
                    c.icon(t, bx + 28.0, by + 28.0, a.icon, 24.0, if a.id == "poweroff" && on { DANGER } else { T1 });
                    self.hits.push(((bx, by, 56.0, 56.0), i as u8));
                    let ly = by + 56.0 + 8.0;
                    txt_c(c, t, ax + 50.0, ly, a.t, 12.5, 500.0, if on { T1 } else { T2 }, 160.0);
                    let ly2 = ly + lh(t, 12.5, 500.0) + 8.0;
                    txt_c(c, t, ax + 50.0, ly2, &(i + 1).to_string(), 10.5, 450.0, T4, 100.0);
                }
            }
            Some(i) => {
                let a = &ACTIONS[i];
                let cw = 520.0;
                let mut y = y0 + 30.0;
                crate::desktop::circle(c, cxm, y + 26.0, 26.0, FILL2);
                c.icon(t, cxm, y + 26.0, a.icon, 24.0, T1);
                y += 52.0 + 10.0;
                let st = crate::text::Style::ui(15.0, 600.0);
                for l in crate::gfx::wrap(t, a.q, &st, cw, 3) {
                    txt_c(c, t, cxm, y, &l, 15.0, 600.0, T1, cw);
                    y += lh(t, 15.0, 600.0);
                }
                y += 10.0;
                let s2 = crate::text::Style::ui(12.5, 450.0);
                for l in crate::gfx::wrap(t, &format!("Jeśli nic nie zrobisz, {} automatycznie za {} s.", a.n, self.left().max(0)), &s2, cw, 3) {
                    txt_c(c, t, cxm, y, &l, 12.5, 450.0, T2, cw);
                    y += lh(t, 12.5, 450.0);
                }
                y += 10.0 + 6.0 + 10.0;
                let (b1, b2) = (cxm - 155.0, cxm + 5.0);
                pill(c, t, b1, y, 150.0, "Anuluj", if self.hover == Some(10) { FILL2 } else { FILL }, T1, 550.0);
                pill(c, t, b2, y, 150.0, a.t, if self.hover == Some(11) { ACCENT } else { Rgba::hex("#409CFF") }, Rgba::hex("#0e1420"), 600.0);
                self.hits.push(((b1, y, 150.0, 34.0), 10));
                self.hits.push(((b2, y, 150.0, 34.0), 11));
            }
        }
        glass_rim(c, x0, y0, 600.0, ch, 26.0);
    }
    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, _cx: &mut Ctx) -> Act {
        self.ptr = (x, y);
        let h = self.hit(x, y);
        match ev {
            Ptr::Motion | Ptr::Enter => {
                let hv = match h { Some(n) if n < 5 || n == 10 || n == 11 => Some(n as usize), _ => None };
                let mut ch = hv != self.hover;
                self.hover = hv;
                // hovering an action selects it (onPositionChanged)
                if let Some(n) = hv {
                    if n < 5 && self.sel != n {
                        self.sel = n;
                        ch = true;
                    }
                }
                if ch { Act::Redraw } else { Act::None }
            }
            Ptr::Press(_) => {
                self.pressed_outside = h.is_none();
                Act::None
            }
            Ptr::Release(_) => match h {
                None if self.pressed_outside => Act::Close,
                Some(n) if n < 5 => self.pick(n as usize),
                Some(10) => {
                    self.unask();
                    Act::Redraw
                }
                Some(11) => self.ask.map(|i| Act::Run(cmd(ACTIONS[i].id))).unwrap_or(Act::None),
                _ => Act::None,
            },
            _ => Act::None,
        }
    }
    fn key(&mut self, ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        match ev.keysym {
            Keysym::Escape => {
                if self.ask.is_some() {
                    self.unask();
                    Act::Redraw
                } else {
                    Act::Close
                }
            }
            Keysym::_1 | Keysym::_2 | Keysym::_3 | Keysym::_4 | Keysym::_5 => {
                let i = (ev.keysym.raw() - Keysym::_1.raw()) as usize;
                self.pick(i)
            }
            Keysym::Return | Keysym::KP_Enter if self.ask.is_some() => Act::Run(cmd(ACTIONS[self.ask.unwrap()].id)),
            Keysym::Right | Keysym::Tab => {
                self.sel = (self.sel + 1) % 5;
                Act::Redraw
            }
            Keysym::Left | Keysym::ISO_Left_Tab => {
                self.sel = (self.sel + 4) % 5;
                Act::Redraw
            }
            Keysym::Return | Keysym::KP_Enter | Keysym::space => self.pick(self.sel),
            _ => Act::None,
        }
    }
}


