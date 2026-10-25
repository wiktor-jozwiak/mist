// Shortcut cheat sheet (Super+/, `msg mist toggle keys`): every bind of comp.conf (or the defaults) in words,
// grouped like Mist Settings > Shortcuts, in columns on a dimmed screen. Numbered binds fold into one row
// (Super+1…6: Pulpit 1–6). Read once when it opens; any key or click closes it.
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use crate::theme::*;
use smithay_client_toolkit::seat::keyboard::KeyEvent;

const GROUPS: [&str; 7] = ["Mist", "Okna", "Pulpity", "Aplikacje", "Multimedia", "Dostępność", "System"];

pub struct Keys {
    // group, description, key caps
    rows: Vec<(&'static str, String, Vec<String>)>,
}

// (combo, action) as comp.conf lists them -> rows: one per description, numbered ones folded
pub fn rows(binds: &[(String, String)]) -> Vec<(&'static str, String, Vec<String>)> {
    let mut out: Vec<(&'static str, String, Vec<String>)> = Vec::new();
    // folded rows: (group, description prefix, combo prefix) -> (first, last) digit and the row index
    let mut nums: Vec<((&'static str, String, String), (u32, u32), usize)> = Vec::new();
    // descriptions already shown (Super+Ctrl+1 is Pulpit 1 again: the first combination wins)
    let mut seen = std::collections::HashSet::new();
    for (combo, act) in binds {
        if combo.contains("mouse_") {
            continue;
        }
        let (g, d) = crate::sysset::pages::describe(act);
        if !seen.insert(d.clone()) {
            continue;
        }
        let g = GROUPS.iter().copied().find(|x| *x == g).unwrap_or("System");
        let digit = combo.rsplit('+').next().and_then(|k| k.parse::<u32>().ok()).filter(|n| (1..=9).contains(n));
        let dnum = d.rsplit(' ').next().and_then(|k| k.parse::<u32>().ok());
        if let (Some(n), Some(m)) = (digit, dnum) {
            if n == m {
                let key = (g, d[..d.len() - m.to_string().len()].trim_end().to_string(), combo[..combo.len() - 1].to_string());
                if let Some(e) = nums.iter_mut().find(|e| e.0 == key) {
                    e.1 = (e.1 .0.min(n), e.1 .1.max(n));
                    let (a, b) = e.1;
                    out[e.2].1 = format!("{} {a}–{b}", key.1);
                    let mut caps = caps(&key.2);
                    caps.push(format!("{a}…{b}"));
                    out[e.2].2 = caps;
                    continue;
                }
                nums.push((key, (n, n), out.len()));
                out.push((g, d, caps(combo)));
                continue;
            }
        }
        out.push((g, d, caps(combo)));
    }
    out
}

fn caps(combo: &str) -> Vec<String> {
    combo.split('+').filter(|k| !k.is_empty()).map(crate::sysset::pages::key_name).collect()
}

impl Keys {
    pub fn new() -> Keys {
        Keys { rows: rows(&crate::sysset::backend::binds()) }
    }
}

impl Pane for Keys {
    fn name(&self) -> &'static str {
        "keys"
    }
    fn namespace(&self) -> &'static str {
        "mist-keys"
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        dim(c);
        let t = &mut *cx.t;
        // the list as headers and rows, cut into columns of about equal height (a group may go on in the
        // next column); on a short screen everything shrinks down to 0.7
        let mut items: Vec<(Option<&'static str>, usize)> = Vec::new();
        for g in GROUPS {
            let idx: Vec<usize> = (0..self.rows.len()).filter(|&i| self.rows[i].0 == g).collect();
            if !idx.is_empty() {
                items.push((Some(g), 0));
                items.extend(idx.into_iter().map(|i| (None, i)));
            }
        }
        let (col_w, gap) = (360.0, 28.0);
        let ncol = (((c.w() - 72.0 + gap) / (col_w + gap)).floor() as usize).clamp(1, 4);
        let unit = |it: &(Option<&str>, usize)| if it.0.is_some() { 40.0 } else { 30.0 };
        let total: f32 = items.iter().map(unit).sum();
        let target = total / ncol as f32;
        let mut cols: Vec<Vec<(Option<&'static str>, usize)>> = vec![Vec::new()];
        let mut used = 0.0;
        for (k, it) in items.iter().enumerate() {
            // a header never ends a column; a column is full past the target
            let header_last = it.0.is_some() && items.get(k + 1).is_some_and(|n| n.0.is_none());
            if used >= target - 1.0 && cols.len() < ncol && !cols.last().unwrap().is_empty() {
                cols.push(Vec::new());
                used = 0.0;
            }
            if header_last && used + 70.0 > target + 30.0 && cols.len() < ncol && !cols.last().unwrap().is_empty() {
                cols.push(Vec::new());
                used = 0.0;
            }
            cols.last_mut().unwrap().push(*it);
            used += unit(it);
        }
        let tallest = cols.iter().map(|c| c.iter().map(unit).sum::<f32>()).fold(0.0, f32::max);
        let k = ((c.h() - 32.0 - 92.0) / tallest).clamp(0.7, 1.0);
        let (row_h, head_h) = (30.0 * k, 40.0 * k);
        let (w, h) = (cols.len() as f32 * (col_w + gap) - gap + 56.0, tallest * k + 92.0 * k.max(0.85));
        let (x0, y0) = (((c.w() - w) / 2.0).round(), ((c.h() - h) / 2.0).max(16.0).round());
        glass(c, x0, y0, w, h, 26.0, 0.82);
        let title = crate::i18n::tr("Skróty klawiszowe");
        let lt = lh(t, 20.0 * k, 650.0);
        txt_c(c, t, x0 + w / 2.0, y0 + 20.0 * k, &title, 20.0 * k, 650.0, T1, w - 40.0);
        let (fs, hf, kf) = (13.0 * k.max(0.85), 12.0 * k.max(0.85), 11.5 * k.max(0.85));
        let (st, hs, ks) = (crate::text::Style::ui(fs, 450.0), crate::text::Style::ui(hf, 650.0), crate::text::Style::ui(kf, 550.0));
        let (l13, l12, l11) = (lh(t, fs, 450.0), lh(t, hf, 650.0), lh(t, kf, 550.0));
        for (ci, col) in cols.iter().enumerate() {
            let cx0 = x0 + 28.0 + ci as f32 * (col_w + gap);
            let mut y = y0 + 20.0 * k + lt + 18.0 * k;
            for it in col {
                if let Some(g) = it.0 {
                    let head = crate::i18n::tr(g).to_uppercase();
                    crate::desktop::text(c, t, cx0, y + head_h - 12.0 * k - l12, &head, &hs, T3, false, None);
                    y += head_h;
                    continue;
                }
                let (_, d, keys) = &self.rows[it.1];
                // caps from the right
                let mut kx = cx0 + col_w;
                for key in keys.iter().rev() {
                    let tw = t.width(key, &ks);
                    let kw = (tw + 14.0).max(24.0 * k);
                    kx -= kw;
                    crate::desktop::rect(c, kx, y + 4.0 * k, kw, row_h - 8.0 * k, 6.0, Rgba(1.0, 1.0, 1.0, 0.12));
                    crate::desktop::border(c, kx, y + 4.0 * k, kw, row_h - 8.0 * k, 6.0, 1.0, Rgba(1.0, 1.0, 1.0, 0.10));
                    crate::desktop::text(c, t, kx + (kw - tw) / 2.0, y + (row_h - l11) / 2.0, key, &ks, T1, false, None);
                    kx -= 4.0;
                }
                let dt = crate::i18n::tr(d);
                crate::desktop::text(c, t, cx0, y + (row_h - l13) / 2.0, &dt, &st, T1, false, Some((kx - cx0 - 10.0).max(40.0)));
                y += row_h;
            }
        }
        glass_rim(c, x0, y0, w, h, 26.0);
    }
    fn pointer(&mut self, ev: Ptr, _x: f32, _y: f32, _cx: &mut Ctx) -> Act {
        match ev {
            Ptr::Release(_) => Act::Close,
            _ => Act::None,
        }
    }
    fn key(&mut self, _ev: &KeyEvent, _cx: &mut Ctx) -> Act {
        Act::Close
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbered_binds_fold() {
        let b = |k: &str, a: &str| (k.to_string(), a.to_string());
        let r = super::rows(&[b("SUPER+1", "workspace 1"), b("SUPER+2", "workspace 2"), b("SUPER+3", "workspace 3"), b("SUPER+CTRL+1", "workspace m~1"), b("SUPER+CTRL+2", "workspace m~2"), b("SUPER+Q", "close"), b("ALT+F4", "close"), b("SUPER+mouse_up", "workspace m+1")]);
        let names: Vec<&str> = r.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(names, ["Pulpit 1–3", "Zamknij okno"], "{r:?}");
        assert_eq!(r[0].2, ["Super", "1…3"]);
        assert_eq!(r[1].2, ["Super", "Q"]);
    }
}
