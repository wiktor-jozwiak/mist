//! Window swallowing (Hyprland misc.enable_swallow + swallow_regex): a window started from a terminal hides
//! that terminal (like minimize, no animation); when the window closes the terminal comes back where it was.
//! Only an ancestor process of the new window counts (/proc ppid chain), only terminals swallow.
use smithay::{desktop::Window, utils::IsAlive};

use crate::{wm::data, MistComp};

const TERMINALS: [&str; 6] = ["kitty", "ghostty", "konsole", "alacritty", "gnome-terminal", "xfce4-terminal"];

pub fn is_terminal(app: &str) -> bool {
    let a = app.to_lowercase();
    TERMINALS.contains(&a.as_str()) || a.starts_with("xfce") && a.ends_with("-terminal") || a == "org.gnome.terminal"
}

fn ppid(pid: i32) -> Option<i32> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // "pid (comm) state ppid ...": comm may hold spaces / parens
    s[s.rfind(')')? + 1..].split_whitespace().nth(1)?.parse().ok()
}

impl MistComp {
    pub fn window_pid(&self, w: &Window) -> Option<i32> {
        w.toplevel()
            .and_then(|t| self.display_handle.get_client(smithay::reexports::wayland_server::Resource::id(t.wl_surface())).ok())
            .and_then(|c| c.get_credentials(&self.display_handle).ok())
            .map(|c| c.pid)
            .or_else(|| w.x11_surface().and_then(|x| x.pid()).map(|p| p as i32))
    }

    /// A new window: hide the terminal it was started from.
    pub fn swallow(&mut self, child: &Window) {
        if !self.config.swallow || is_terminal(&crate::hyprcompat::app_id(child)) {
            return;
        }
        let Some(pid) = self.window_pid(child) else { return };
        let mut ancestors = Vec::new();
        let mut p = pid;
        for _ in 0..16 {
            match ppid(p) {
                Some(pp) if pp > 1 => {
                    ancestors.push(pp);
                    p = pp;
                }
                _ => break,
            }
        }
        let term = self
            .space
            .elements()
            .find(|w| *w != child && is_terminal(&crate::hyprcompat::app_id(w)) && self.window_pid(w).is_some_and(|tp| ancestors.contains(&tp)))
            .cloned();
        let Some(term) = term else { return };
        let Some(o) = self.output_of(&term) else { return };
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let loc = self.space.element_location(&term).unwrap_or_default() - og.loc;
        data(&term).ws.set(crate::wm::MINIMIZED);
        self.space.unmap_elem(&term);
        self.stash.push((term.clone(), loc));
        self.hypr_event("movewindowv2", &format!("{},-99,special:minimized", crate::hyprcompat::addr(&term).trim_start_matches("0x")));
        *data(child).swallowed.borrow_mut() = Some(term);
    }

    /// A window closed: bring back the terminal it swallowed (if it is still hidden).
    pub fn unswallow(&mut self, child: &Window) {
        let Some(term) = data(child).swallowed.borrow_mut().take() else { return };
        if term.alive() && data(&term).ws.get() == crate::wm::MINIMIZED {
            let ws = data(child).ws.get();
            self.restore(&term, (ws != crate::wm::MINIMIZED).then_some(ws));
        }
    }
}
