//! Saved window layouts (`layout save NAME`, `layout load NAME`; Spotlight "układ …"): the windows as
//! session.json keeps them (app, command, workspace, place, maximized) in ~/.config/mist/layouts/NAME.json.
//! Loading moves the windows already open to their saved place and workspace, and starts the missing apps
//! once; those take their place when their window comes (the session restore path, session.rs).
use crate::session::Slot;
use crate::state::MistComp;
use serde_json::Value;
use smithay::desktop::Window;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::utils::{Point, Rectangle};
use std::time::Duration;

/// MIST_COMP_LAYOUT_DIR: another folder (tests).
fn dir() -> String {
    std::env::var("MIST_COMP_LAYOUT_DIR").unwrap_or_else(|_| format!("{}/.config/mist/layouts", std::env::var("HOME").unwrap_or_default()))
}

/// A name that stays one file inside the folder.
fn file(name: &str) -> Option<String> {
    let n = name.trim();
    (!n.is_empty() && !n.starts_with('.') && !n.contains('/') && n.len() <= 64).then(|| format!("{}/{n}.json", dir()))
}

impl MistComp {
    pub fn layout_save(&self, name: &str) {
        let Some(f) = file(name) else { return };
        let _ = std::fs::create_dir_all(dir());
        let n = self.windows_json().len();
        if std::fs::write(&f, Value::Array(self.windows_json()).to_string()).is_ok() {
            tracing::info!("layout saved: {name} ({n} windows)");
        }
    }

    pub fn layout_load(&mut self, name: &str) {
        let Some(list) = file(name).and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str::<Value>(&s).ok()) else { return };
        let Value::Array(list) = list else { return };
        // open windows by app id, each used once
        let mut open: Vec<Window> = self.space.elements().cloned().chain(self.stash.iter().map(|(w, _)| w.clone())).filter(|w| self.parent_of(w).is_none()).collect();
        for v in list {
            let n = |k: &str, i: usize| v[k][i].as_i64().unwrap_or(0) as i32;
            let (Some(app), Some(cmd)) = (v["app"].as_str(), v["cmd"].as_str()) else { continue };
            let slot = Slot { app: app.to_lowercase(), ws: v["ws"].as_u64().unwrap_or(1) as u32, rect: Rectangle::new((n("at", 0), n("at", 1)).into(), (n("size", 0), n("size", 1)).into()), max: v["max"].as_bool().unwrap_or(false) };
            if let Some(i) = open.iter().position(|w| crate::hyprcompat::app_id(w).to_lowercase() == slot.app) {
                let w = open.remove(i);
                self.layout_apply(&w, &slot);
            } else {
                // one start per missing window (kitty, browsers: each start is a new window)
                tracing::info!("layout {name}: starting {app}");
                crate::wm::spawn(cmd);
                self.restore.push(slot);
            }
        }
        // what never came is forgotten after a minute
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(60)), |_, _, st: &mut MistComp| {
            st.restore.clear();
            TimeoutAction::Drop
        });
    }

    /// An open window to its saved workspace, place and size.
    fn layout_apply(&mut self, w: &Window, s: &Slot) {
        let d = crate::wm::data(w);
        if d.ws.get() == crate::wm::MINIMIZED {
            self.restore(w, Some(s.ws));
        }
        if d.restore.borrow().is_some() {
            self.toggle_max(w, false);
        }
        let mapped = self.space.elements().any(|x| x == w);
        if mapped && d.ws.get() != s.ws {
            self.move_to_ws(w, s.ws);
        }
        self.resize_to(w, s.rect.size.w, s.rect.size.h);
        let shown = self.ws_active.values().any(|v| *v == s.ws);
        if let Some(i) = self.stash.iter().position(|(x, _)| x == w) {
            // from a hidden workspace: mapped when its workspace is shown, else the stash keeps the place
            // relative to the monitor
            let moved = d.ws.get() != s.ws;
            d.ws.set(s.ws);
            if shown {
                self.stash.remove(i);
                self.space.map_element(w.clone(), s.rect.loc, false);
            } else {
                let og = self.space.outputs().filter_map(|o| self.space.output_geometry(o)).find(|g| g.contains(s.rect.loc + Point::from((s.rect.size.w / 2, s.rect.size.h / 2)))).unwrap_or_default();
                self.stash[i].1 = s.rect.loc - og.loc;
            }
            if moved {
                self.ws_changed = true;
                self.hypr_event("movewindowv2", &format!("{},{},{}", crate::hyprcompat::addr(w).trim_start_matches("0x"), s.ws, s.ws));
            }
        } else if self.space.elements().any(|x| x == w) {
            self.space.map_element(w.clone(), s.rect.loc, false);
        }
        if s.max {
            self.mist_maximize(w);
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_stay_in_the_folder() {
        assert!(super::file("Praca").is_some_and(|f| f.ends_with("/layouts/Praca.json")));
        assert!(super::file("../x").is_none() && super::file(".hidden").is_none() && super::file(" ").is_none());
    }
}
