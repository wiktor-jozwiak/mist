//! Windows come back after a compositor crash. While it runs, mist-comp keeps ~/.cache/mist/session.json up
//! to date (each window: app id, its process's command line, workspace, place, maximized), written at most
//! every 2 s after a change. start-mist restarts a crashed mist-comp with MIST_COMP_RESTORE=1: it starts each
//! command once and puts the first window of each app back where it was. A command waits until the processes
//! of the old session running it are gone (a browser dumping core after the crash still holds its profile lock:
//! started next to it, Zen quit at once) and is skipped when a new one started meanwhile.
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
    quote_args(&std::fs::read(format!("/proc/{pid}/cmdline")).ok()?)
}

/// NUL-separated argv as one sh command line.
fn quote_args(raw: &[u8]) -> Option<String> {
    let mut args: Vec<String> = raw.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    // Chromium apps (Spotify, Electron) rewrite argv into one space-joined string: split it back unless it is a
    // real path with spaces
    if args.len() == 1 && args[0].contains(' ') && !std::path::Path::new(&args[0]).exists() {
        args = args[0].split_whitespace().map(String::from).collect();
    }
    let args: Vec<String> = args.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect();
    (!args.is_empty()).then(|| args.join(" "))
}

/// The program of a saved command line (its first sh-quoted argument).
fn argv0(cmd: &str) -> String {
    cmd.strip_prefix('\'').and_then(|c| c.split('\'').next()).unwrap_or_else(|| cmd.split_whitespace().next().unwrap_or("")).to_string()
}

/// Processes running this program (by executable, or argv[0] for scripts and Chromium's one-string argv).
fn pids_of(exe: &str) -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    let me = std::process::id() as i32;
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|&p| p != me)
        .filter(|p| {
            let link = std::fs::read_link(format!("/proc/{p}/exe")).ok();
            link.is_some_and(|l| l.to_string_lossy().trim_end_matches(" (deleted)") == exe)
                || std::fs::read(format!("/proc/{p}/cmdline")).ok().is_some_and(|c| c.split(|b| *b == 0 || *b == b' ').next() == Some(exe.as_bytes()))
        })
        .collect()
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

    pub fn session_write(&self) {
        let out = self.windows_json();
        let tmp = format!("{}.tmp", path());
        if std::fs::write(&tmp, Value::Array(out).to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, path());
        }
    }

    /// Each window as session.json keeps it (also a saved layout, layouts.rs).
    pub fn windows_json(&self) -> Vec<Value> {
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
        out
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
        let mut pending: Vec<(String, String, String, Vec<i32>)> = Vec::new();
        for v in list {
            let n = |k: &str, i: usize| v[k][i].as_i64().unwrap_or(0) as i32;
            let (Some(app), Some(cmd)) = (v["app"].as_str(), v["cmd"].as_str()) else { continue };
            self.restore.push(Slot { app: app.to_lowercase(), ws: v["ws"].as_u64().unwrap_or(1) as u32, rect: Rectangle::new((n("at", 0), n("at", 1)).into(), (n("size", 0), n("size", 1)).into()), max: v["max"].as_bool().unwrap_or(false) });
            if started.insert(cmd.to_string()) {
                let exe = argv0(cmd);
                let old = pids_of(&exe);
                pending.push((app.to_string(), cmd.to_string(), exe, old));
            }
        }
        // spawn each command once its old processes are gone (checked every 250 ms, at most 10 s)
        let mut tries = 0;
        let _ = self.loop_handle.insert_source(Timer::immediate(), move |_, _, _: &mut MistComp| {
            tries += 1;
            pending.retain(|(app, cmd, exe, old)| {
                let alive = old.iter().any(|p| std::path::Path::new(&format!("/proc/{p}")).exists());
                if alive && tries < 40 {
                    return true;
                }
                if pids_of(exe).iter().any(|p| !old.contains(p)) {
                    tracing::info!("session restore: {app}: already started again");
                } else {
                    tracing::info!("session restore: {app}: {cmd}{}", if alive { " (old process still there)" } else { "" });
                    crate::wm::spawn(cmd);
                }
                false
            });
            if pending.is_empty() { TimeoutAction::Drop } else { TimeoutAction::ToDuration(Duration::from_millis(250)) }
        });
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

#[cfg(test)]
mod tests {
    #[test]
    fn chromium_argv_split_back() {
        let raw = b"/opt/spotify/spotify --ozone-platform=wayland --uri=%u\0";
        assert_eq!(super::quote_args(raw).unwrap(), "'/opt/spotify/spotify' '--ozone-platform=wayland' '--uri=%u'");
        assert_eq!(super::quote_args(b"/usr/bin/kitty\0--class\0it's\0").unwrap(), "'/usr/bin/kitty' '--class' 'it'\\''s'");
        assert_eq!(super::argv0("'/opt/zen-browser-bin/zen-bin' '--no-remote'"), "/opt/zen-browser-bin/zen-bin");
    }
}
