//! Windows come back after a compositor crash. While it runs, mist-comp keeps ~/.cache/mist/session.json up
//! to date (each window: app id, its process's command line, workspace, place, maximized), written at most
//! every 2 s after a change. start-mist restarts a crashed mist-comp with MIST_COMP_RESTORE=1: it starts each
//! command once and puts the first window of each app back where it was.
use crate::state::MistComp;
use serde_json::{json, Value};
use smithay::desktop::Window;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::utils::{Logical, Point, Rectangle};
use std::time::Duration;

pub struct Slot {
    pub app: String,
    pub ws: u32,
    pub rect: Rectangle<i32, Logical>,
    pub max: bool,
}

/// MIST_COMP_SESSION_FILE: another file (tests); a nested compositor writes only that one.
fn path() -> String {
    std::env::var("MIST_COMP_SESSION_FILE").unwrap_or_else(|_| format!("{}/.cache/mist/session.json", std::env::var("HOME").unwrap_or_default()))
}

/// A process's command line, each argument single-quoted for sh.
fn cmdline(pid: i32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args: Vec<String> = raw.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| format!("'{}'", String::from_utf8_lossy(a).replace('\'', "'\\''"))).collect();
    (!args.is_empty()).then(|| args.join(" "))
}

impl MistComp {
    /// Something about the windows changed: the file follows within 2 s.
    pub fn session_mark(&mut self) {
        // the real session only (nested / headless test compositors leave the file alone unless given their own)
        let own = std::env::var_os("MIST_COMP_SESSION_FILE").is_some();
        if self.session_timer || (self.udev.is_none() && !own) || std::env::var_os("MIST_COMP_NO_SESSION").is_some() {
            return;
        }
        self.session_timer = true;
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(2)), |_, _, st: &mut MistComp| {
            st.session_timer = false;
            st.session_write();
            TimeoutAction::Drop
        });
    }

    fn session_write(&self) {
        let mut out = Vec::new();
        let all = self.space.elements().map(|w| (w, self.space.element_geometry(w))).chain(self.stash.iter().map(|(w, at)| (w, Some(Rectangle::new(*at, w.geometry().size)))));
        for (w, g) in all {
            let d = crate::wm::data(w);
            let (Some(g), app) = (g, crate::hyprcompat::app_id(w)) else { continue };
            if d.addr.get() == 0 || app.is_empty() || self.parent_of(w).is_some() || d.ws.get() == crate::wm::MINIMIZED {
                continue;
            }
            let Some(cmd) = self.window_pid(w).and_then(cmdline) else { continue };
            let r = d.restore.borrow().unwrap_or(g);
            out.push(json!({ "app": app, "cmd": cmd, "ws": d.ws.get(), "at": [r.loc.x, r.loc.y], "size": [r.size.w, r.size.h], "max": d.restore.borrow().is_some() }));
        }
        let tmp = format!("{}.tmp", path());
        if std::fs::write(&tmp, Value::Array(out).to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, path());
        }
    }

    /// After a crash (MIST_COMP_RESTORE=1): start what was open, keep where each app's window was.
    pub fn session_restore(&mut self) {
        if std::env::var("MIST_COMP_RESTORE").as_deref() != Ok("1") {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path()) else { return };
        let _ = std::fs::rename(path(), format!("{}.restored", path()));
        let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text) else { return };
        let mut started = std::collections::HashSet::new();
        for v in list {
            let n = |k: &str, i: usize| v[k][i].as_i64().unwrap_or(0) as i32;
            let (Some(app), Some(cmd)) = (v["app"].as_str(), v["cmd"].as_str()) else { continue };
            self.restore.push(Slot { app: app.to_lowercase(), ws: v["ws"].as_u64().unwrap_or(1) as u32, rect: Rectangle::new((n("at", 0), n("at", 1)).into(), (n("size", 0), n("size", 1)).into()), max: v["max"].as_bool().unwrap_or(false) });
            if started.insert(cmd.to_string()) {
                tracing::info!("session restore: {app}: {cmd}");
                crate::wm::spawn(cmd);
            }
        }
        // what never came back is forgotten after a minute
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(60)), |_, _, st: &mut MistComp| {
            st.restore.clear();
            TimeoutAction::Drop
        });
    }

    /// The saved size for a new window of this app (its initial configure).
    pub fn restore_size(&self, w: &Window) -> Option<(i32, i32)> {
        let app = crate::hyprcompat::app_id(w).to_lowercase();
        self.restore.iter().find(|s| s.app == app && !s.max).map(|s| (s.rect.size.w, s.rect.size.h))
    }

    /// The saved place of this app's window (once), if it is on a monitor that exists.
    pub fn restore_take(&mut self, w: &Window) -> Option<Slot> {
        let app = crate::hyprcompat::app_id(w).to_lowercase();
        let i = self.restore.iter().position(|s| s.app == app)?;
        let s = self.restore.remove(i);
        let on = self.space.outputs().any(|o| self.space.output_geometry(o).is_some_and(|g| g.contains(s.rect.loc + Point::from((s.rect.size.w / 2, s.rect.size.h / 2)))));
        on.then_some(s)
    }
}
