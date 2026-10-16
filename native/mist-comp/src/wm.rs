//! Window management: workspaces (per output, Hyprland-like ids), bind actions, maximize/fullscreen.
//!
//! Windows of hidden workspaces leave the Space (kept in `stash` with their position), so rendering,
//! hit-testing and frame callbacks never touch them.
use smithay::utils::IsAlive as _;
use std::cell::{Cell, RefCell};

use smithay::{
    desktop::{layer_map_for_output, Window},
    output::Output,
    reexports::{wayland_protocols::xdg::shell::server::xdg_toplevel, wayland_server::Resource},
    utils::{Logical, Point, Rectangle},
};

use crate::{
    config::{Action, Dir, WsArg},
    MistComp,
};

/// Workspace id of minimized windows (Hyprland's special:minimized).
pub const MINIMIZED: u32 = u32::MAX;
/// Hyprland's special workspace: a scratchpad shown over the current one (SUPER+grave)
pub const SPECIAL: u32 = u32::MAX - 1;

/// Per-window data in `Window::user_data()`.
#[derive(Default)]
pub struct WinData {
    pub ws: Cell<u32>,
    /// Hyprland-style address (hyprcompat), unique per window
    pub addr: Cell<u64>,
    /// geometry before maximize / fullscreen
    pub restore: RefCell<Option<Rectangle<i32, Logical>>>,
    /// server-side title bar
    pub deco: crate::decor::Deco,
    /// Windows 11 snap state
    pub snap: crate::snap::SnapData,
    /// on every workspace (Picture-in-Picture)
    pub pinned: Cell<bool>,
    /// the terminal this window was started from, hidden while it lives (swallow.rs)
    pub swallowed: RefCell<Option<Window>>,
}

/// Which monitor (left to right) a workspace belongs to: three each.
pub fn ws_group(ws: u32) -> usize {
    (ws.max(1) as usize - 1) / 3
}

pub fn data(w: &Window) -> &WinData {
    w.user_data().insert_if_missing(WinData::default);
    w.user_data().get::<WinData>().unwrap()
}

impl MistComp {
    /// The output under the pointer (else the first one).
    pub fn active_output(&self) -> Option<Output> {
        let p = self.seat.get_pointer().unwrap().current_location();
        self.space
            .outputs()
            .find(|o| self.space.output_geometry(o).is_some_and(|g| g.to_f64().contains(p)))
            .or_else(|| self.space.outputs().next())
            .cloned()
    }

    pub fn active_ws(&mut self, output: &Output) -> u32 {
        if let Some(&ws) = self.ws_active.get(&output.name()) {
            return ws;
        }
        // a new output: the first free workspace that lives there
        let ws = (1..).find(|i| !self.ws_active.values().any(|v| v == i) && self.ws_home(*i).is_none_or(|h| h == *output)).unwrap();
        self.ws_active.insert(output.name(), ws);
        ws
    }

    /// Hyprland workspace rules (workspaces.lua): 1-3 live on the leftmost monitor, 4-6 on the next one...
    /// None with a single monitor (any workspace anywhere).
    pub fn ws_home(&self, ws: u32) -> Option<Output> {
        let mut outs: Vec<(Output, Point<i32, Logical>)> = self.space.outputs().filter_map(|o| Some((o.clone(), self.space.output_geometry(o)?.loc))).collect();
        if outs.len() < 2 {
            return None;
        }
        outs.sort_by_key(|(_, l)| (l.x, l.y));
        outs.get(ws_group(ws)).map(|(o, _)| o.clone())
    }

    /// Outputs came or went: each shows a workspace of its own group (the right monitor may have been
    /// mapped first and taken 1).
    pub fn fix_ws(&mut self) {
        let outs: Vec<Output> = self.space.outputs().cloned().collect();
        for o in &outs {
            let cur = self.ws_active.get(&o.name()).copied();
            if cur.is_some_and(|c| self.ws_home(c).is_none_or(|h| h == *o)) {
                continue;
            }
            self.ws_active.remove(&o.name());
        }
        for o in &outs {
            if !self.ws_active.contains_key(&o.name()) {
                self.active_ws(o);
            }
        }
    }

    pub fn focused_window(&self) -> Option<Window> {
        let kb = self.seat.get_keyboard().unwrap();
        let f = kb.current_focus()?;
        self.space.elements().find(|w| crate::focus::of_window(w).as_ref() == Some(&f)).cloned()
    }

    /// The output holding most of the window (from geometry: valid right after map_element, unlike
    /// Space::outputs_for_element which waits for refresh()).
    pub fn output_of(&self, w: &Window) -> Option<Output> {
        let g = self.space.element_geometry(w)?;
        self.space
            .outputs()
            .filter_map(|o| {
                let og = self.space.output_geometry(o)?;
                let i = og.intersection(g)?;
                Some((i.size.w * i.size.h, o))
            })
            .max_by_key(|(a, _)| *a)
            .map(|(_, o)| o.clone())
    }

    fn resolve(&mut self, output: &Output, arg: WsArg) -> u32 {
        let group = |st: &mut Self| {
            let cur = st.active_ws(output);
            match st.ws_home(cur) {
                Some(_) => ws_group(cur) as u32 * 3,
                None => 0,
            }
        };
        match arg {
            WsArg::Id(n) => n,
            WsArg::Special => SPECIAL,
            WsArg::Local(n) => group(self) + n,
            WsArg::Empty => {
                // the first one of this monitor without windows (the current one if it is empty)
                let base = group(self);
                let used = |st: &Self, i: u32| st.space.elements().chain(st.stash.iter().map(|(w, _)| w)).any(|w| data(w).ws.get() == i && !data(w).pinned.get());
                let grouped = self.ws_home(base + 1).is_some();
                (base + 1..).take(if grouped { 3 } else { 99 }).find(|i| !used(self, *i)).unwrap_or(base + 1)
            }
            WsArg::Rel(d) => {
                let cur = self.active_ws(output) as i32;
                // with workspace groups: stays within this monitor's three
                match self.ws_home(cur as u32) {
                    Some(_) => {
                        let first = ws_group(cur as u32) as i32 * 3 + 1;
                        (cur + d).clamp(first, first + 2) as u32
                    }
                    None => (cur + d).max(1) as u32,
                }
            }
        }
    }

    /// Show workspace `ws` on `output` (hiding the one it had).
    pub fn switch_ws(&mut self, output: &Output, ws: u32) {
        // a workspace of the other monitor: shown there, the pointer goes along (Hyprland warps too)
        if let Some(home) = self.ws_home(ws).filter(|h| h != output) {
            if let Some(g) = self.space.output_geometry(&home) {
                let c = g.loc.to_f64() + g.size.to_f64().downscale(2.0).to_point();
                let t = self.start_time.elapsed().as_millis() as u32;
                self.pointer_to(c, t);
            }
            return self.switch_ws(&home, ws);
        }
        let cur = self.active_ws(output);
        if cur == ws {
            return;
        }
        // shown on another monitor: just move the pointer focus there (Hyprland does the same)
        if let Some((name, _)) = self.ws_active.iter().find(|(n, v)| **v == ws && **n != output.name()) {
            let name = name.clone();
            let o = self.space.outputs().find(|o| o.name() == name).cloned();
            if let Some(o) = o {
                self.focus_top_on(&o);
            }
            return;
        }
        let og = self.space.output_geometry(output).unwrap_or_default();
        // hide the current one
        let leaving: Vec<Window> = self
            .space
            .elements()
            .filter(|w| data(w).ws.get() == cur && !data(w).pinned.get() && self.output_of(w).as_ref() == Some(output))
            .cloned()
            .collect();
        let mut slide = Vec::new();
        for w in leaving {
            let at = self.space.element_location(&w).unwrap_or_default();
            slide.push((w.clone(), at));
            self.space.unmap_elem(&w);
            self.stash.push((w, at - og.loc));
        }
        self.anim.ws_switch(output.name(), slide, if ws > cur { 1 } else { -1 });
        // show the target (positions are relative to the output they were hidden on)
        let (show, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.stash).into_iter().partition(|(w, _)| data(w).ws.get() == ws);
        self.stash = keep;
        for (w, loc) in show {
            self.space.map_element(w, og.loc + loc, false);
        }
        for w in self.space.elements() {
            if data(w).pinned.get() && data(w).ws.get() == cur {
                data(w).ws.set(ws);
            }
        }
        self.ws_active.insert(output.name(), ws);
        self.focus_top_on(output);
        tracing::info!("workspace {ws} on {}", output.name());
        self.hypr_event("workspace", &ws.to_string());
        self.hypr_event("workspacev2", &format!("{ws},{ws}"));
        self.ws_changed = true;
        self.dirty = true;
    }

    /// The special workspace on / off over `output` (its windows keep their place relative to the output).
    pub fn toggle_special(&mut self, output: &Output) {
        let og = self.space.output_geometry(output).unwrap_or_default();
        if let Some(name) = self.special_on.take() {
            let from = self.space.outputs().find(|o| o.name() == name).and_then(|o| self.space.output_geometry(o)).unwrap_or(og);
            let shown: Vec<Window> = self.space.elements().filter(|w| data(w).ws.get() == SPECIAL).cloned().collect();
            for w in shown {
                let loc = self.space.element_location(&w).unwrap_or_default() - from.loc;
                self.animate_slide_down(&w);
                self.space.unmap_elem(&w);
                self.stash.push((w, loc));
            }
            self.hypr_event("activespecial", &format!(",{name}"));
            self.hypr_event("activespecialv2", &format!(",,{name}"));
            let o = self.space.outputs().find(|o| o.name() == name).cloned();
            if let Some(o) = o {
                self.focus_top_on(&o);
            }
        } else {
            let (show, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.stash).into_iter().partition(|(w, _)| data(w).ws.get() == SPECIAL);
            self.stash = keep;
            let mut top = None;
            for (w, loc) in show {
                // specialWorkspaceIn: slide in from the top
                self.anim.moved(data(&w).addr.get(), (0, -og.size.h).into());
                self.space.map_element(w.clone(), og.loc + loc, true);
                top = Some(w);
            }
            self.special_on = Some(output.name());
            self.hypr_event("activespecial", &format!("special:special,{}", output.name()));
            self.hypr_event("activespecialv2", &format!("-98,special:special,{}", output.name()));
            self.focus_window(top.as_ref());
        }
        self.ws_changed = true;
        self.dirty = true;
    }

    /// Hide a window (dock click / yellow button / xdg set_minimized).
    pub fn minimize(&mut self, w: &Window) {
        let Some(o) = self.output_of(w) else { return };
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let loc = self.space.element_location(w).unwrap_or_default() - og.loc;
        self.animate_out(w, true);
        data(w).ws.set(MINIMIZED);
        self.space.unmap_elem(w);
        self.stash.push((w.clone(), loc));
        self.hypr_event("movewindowv2", &format!("{},-99,special:minimized", crate::hyprcompat::addr(w).trim_start_matches("0x")));
        self.focus_top_on(&o);
    }

    /// Bring a window back and focus it: from another workspace (switches to it) or minimized (onto `ws`,
    /// else the active one).
    pub fn restore(&mut self, w: &Window, ws: Option<u32>) {
        let cur = data(w).ws.get();
        let output = self.active_output();
        if cur == MINIMIZED {
            let Some(o) = output else { return };
            let target = ws.unwrap_or_else(|| self.active_ws(&o));
            if let Some(i) = self.stash.iter().position(|(x, _)| x == w) {
                let (w2, loc) = self.stash.remove(i);
                data(&w2).ws.set(target);
                if self.active_ws(&o) == target {
                    let og = self.space.output_geometry(&o).unwrap_or_default();
                    self.space.map_element(w2, og.loc + loc, true);
                } else {
                    self.stash.push((w2, loc));
                    self.switch_ws(&o, target);
                }
            }
        } else if self.stash.iter().any(|(x, _)| x == w) {
            // on a hidden workspace: show it where it was last shown
            if let Some(o) = output {
                self.switch_ws(&o, cur);
            }
        }
        self.focus_window(Some(w));
    }

    /// A new window (xdg or X11): on the active output's shown workspace, Hyprland address, mapped, focused.
    pub fn adopt_window(&mut self, window: Window, loc: Point<i32, Logical>, focus: bool) {
        let mut ws = 1;
        let at = self.space.outputs().find(|o| self.space.output_geometry(o).is_some_and(|g| g.contains(loc))).cloned();
        if let Some(o) = at.or_else(|| self.active_output()) {
            ws = self.active_ws(&o);
            data(&window).ws.set(ws);
        }
        self.next_addr += 0x10;
        data(&window).addr.set(self.next_addr);
        let class = crate::hyprcompat::app_id(&window);
        self.hypr_event("openwindow", &format!("{:x},{ws},{class},", self.next_addr));
        self.space.map_element(window.clone(), loc, true);
        self.anim.opened(self.next_addr);
        if focus {
            self.focus_window(Some(&window));
        }
    }

    /// A window is gone (destroyed / X11 unmapped): forget it, focus goes to the window below.
    pub fn drop_window(&mut self, w: &Window) {
        let a = data(w).addr.get();
        // persistent_size: remember a normal window's size (not maximized / snapped / PiP / menus)
        let d = data(w);
        let size = w.geometry().size;
        if a != 0 && d.restore.borrow().is_none() && !d.pinned.get() && !d.snap.is_snapped() && size.w > 50 && size.h > 50 {
            let app = crate::hyprcompat::app_id(w).to_lowercase();
            if !app.is_empty() {
                self.sizes.insert(app, size);
            }
        }
        if a != 0 {
            self.animate_out(w, false);
        }
        self.space.unmap_elem(w);
        self.stash.retain(|(x, _)| x != w);
        self.focus_history.retain(|x| *x != a);
        self.hypr_event("closewindow", &format!("{a:x}"));
        let next = self.space.elements().filter(|w| w.alive()).next_back().cloned();
        self.focus_window(next.as_ref());
        self.unswallow(w);
    }

    /// Place a window on a fraction of the usable area of its output (snap zones).
    pub fn snap_to(&mut self, w: &Window, f: [f64; 4]) {
        let Some(o) = self.output_of(w).or_else(|| self.active_output()) else { return };
        let og = self.space.output_geometry(&o).unwrap_or_default();
        let z = layer_map_for_output(&o).non_exclusive_zone();
        let (x, y) = (og.loc.x + z.loc.x + (z.size.w as f64 * f[0]) as i32, og.loc.y + z.loc.y + (z.size.h as f64 * f[1]) as i32);
        let (cw, ch) = ((z.size.w as f64 * f[2]) as i32, (z.size.h as f64 * f[3]) as i32);
        // the server-side bar takes the top of the slot
        let bar = self.bar_h(w);
        if let Some(t) = w.toplevel() {
            t.with_pending_state(|s| s.size = Some((cw, ch - bar).into()));
            t.send_pending_configure();
        }
        self.space.map_element(w.clone(), Point::from((x, y + bar)), true);
        self.focus_window(Some(w));
    }

    /// Screens off / on (idle). Nested: nothing to switch.
    pub fn set_dpms(&mut self, on: bool) {
        tracing::info!("dpms {}", if on { "on" } else { "off" });
        if let Some(u) = self.udev.as_mut() {
            u.set_active(on);
        }
        self.force_full = true;
        self.dirty = true;
    }

    pub fn set_night(&mut self, on: bool) {
        tracing::info!("night light {}", if on { "on" } else { "off" });
        self.night = on;
        if let Some(u) = self.udev.as_mut() {
            u.set_night(on);
        }
    }

    fn focus_top_on(&mut self, output: &Output) {
        let top = self.space.elements().rev().find(|w| self.output_of(w).as_ref() == Some(output)).cloned();
        self.focus_window(top.as_ref());
    }

    pub fn move_to_ws(&mut self, w: &Window, ws: u32) {
        let Some(output) = self.output_of(w) else { return };
        if data(w).ws.get() == ws {
            return;
        }
        data(w).ws.set(ws);
        let visible = if ws == SPECIAL { self.special_on.clone() } else { self.ws_active.iter().find(|(_, v)| **v == ws).map(|(n, _)| n.clone()) };
        match visible.and_then(|n| self.space.outputs().find(|o| o.name() == n).cloned()) {
            // shown elsewhere: carry it over, same offset
            Some(o) => {
                let from = self.space.output_geometry(&output).unwrap_or_default().loc;
                let to = self.space.output_geometry(&o).unwrap_or_default().loc;
                let loc = self.space.element_location(w).unwrap_or_default() - from + to;
                self.space.map_element(w.clone(), loc, false);
            }
            None => {
                let og = self.space.output_geometry(&output).unwrap_or_default();
                let loc = self.space.element_location(w).unwrap_or_default() - og.loc;
                self.space.unmap_elem(w);
                self.stash.push((w.clone(), loc));
            }
        }
        self.focus_top_on(&output);
        self.ws_changed = true;
        self.hypr_event("movewindowv2", &format!("{},{ws},{ws}", crate::hyprcompat::addr(w).trim_start_matches("0x")));
    }

    pub fn do_action(&mut self, action: Action) {
        let output = self.active_output();
        let focused = self.focused_window();
        match action {
            Action::Exec(cmd) => spawn(&cmd),
            Action::Close => {
                if let Some(t) = focused.as_ref().and_then(|w| w.toplevel()) {
                    t.send_close();
                } else if let Some(x) = focused.as_ref().and_then(|w| w.x11_surface()) {
                    let _ = x.close();
                }
            }
            Action::Kill => {
                if let Some(t) = focused.as_ref().and_then(|w| w.toplevel()) {
                    if let Ok(c) = self.display_handle.get_client(t.wl_surface().id()) {
                        if let Ok(cred) = c.get_credentials(&self.display_handle) {
                            spawn(&format!("kill -9 {}", cred.pid));
                        }
                    }
                }
            }
            Action::Quit => self.loop_signal.stop(),
            Action::Reload => {
                self.config = crate::config::load();
                tracing::info!("config reloaded: {} binds", self.config.binds.len());
            }
            Action::Workspace(arg) => {
                if let Some(o) = output {
                    let ws = self.resolve(&o, arg);
                    self.switch_ws(&o, ws);
                }
            }
            Action::MoveToWorkspace(arg) => {
                if let (Some(o), Some(w)) = (output, focused) {
                    let ws = self.resolve(&o, arg);
                    self.move_to_ws(&w, ws);
                    // the view follows (special: it stays a scratchpad)
                    if ws != SPECIAL {
                        self.switch_ws(&o, ws);
                        self.focus_window(Some(&w));
                    }
                }
            }
            Action::MoveToWorkspaceSilent(arg) => {
                if let (Some(o), Some(w)) = (output, focused) {
                    let ws = self.resolve(&o, arg);
                    self.move_to_ws(&w, ws);
                }
            }
            Action::ToggleSpecial => {
                if let Some(o) = output {
                    self.toggle_special(&o);
                }
            }
            Action::Maximize => {
                if let Some(w) = focused {
                    self.toggle_max(&w, false);
                }
            }
            Action::Fullscreen => {
                if let Some(w) = focused {
                    self.toggle_max(&w, true);
                }
            }
            // everything floats in Mist
            Action::Float => {}
            Action::Snap(k) => self.snap_key(k),
            Action::MoveToMonitor(d) => {
                if let Some(w) = focused {
                    self.move_to_monitor(&w, d);
                }
            }
            Action::Focus(dir) => {
                if let Some(w) = focused.and_then(|f| self.neighbour(&f, dir)) {
                    self.focus_window(Some(&w));
                }
            }
            Action::Cycle => {
                // bottom-most window on this output comes up
                let o = output;
                let w = self.space.elements().find(|w| self.output_of(w) == o).cloned();
                self.focus_window(w.as_ref());
            }
            Action::Vt(n) => self.change_vt(n),
            Action::Nothing => return,
        }
        self.dirty = true;
    }

    /// A fullscreen window (xdg Fullscreen state / X11 _NET_WM_STATE_FULLSCREEN) is shown on this output.
    pub fn fullscreen_on(&self, o: &Output) -> bool {
        let Some(og) = self.space.output_geometry(o) else { return false };
        self.space.elements().any(|w| {
            let full = match (w.toplevel(), w.x11_surface()) {
                (Some(t), _) => t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Fullscreen)),
                (_, Some(x)) => x.is_fullscreen(),
                _ => false,
            };
            full && self.space.element_location(w).is_some_and(|l| og.contains(l))
        })
    }

    /// Maximize to the usable area (bar / dock excluded) or fullscreen to the whole output; again restores.
    pub fn toggle_max(&mut self, w: &Window, full: bool) {
        let (t, x) = (w.toplevel().cloned(), w.x11_surface().cloned());
        if t.is_none() && x.is_none() {
            return;
        }
        let Some(output) = self.output_of(w) else { return };
        let d = data(w);
        let og = self.space.output_geometry(&output).unwrap_or_default();
        if let Some(prev) = d.restore.borrow_mut().take() {
            if let Some(t) = &t {
                t.with_pending_state(|s| {
                    s.size = Some(prev.size);
                    s.states.unset(xdg_toplevel::State::Maximized);
                    s.states.unset(xdg_toplevel::State::Fullscreen);
                    s.fullscreen_output = None;
                });
                t.send_pending_configure();
            } else if let Some(x) = &x {
                let _ = x.set_maximized(false);
                let _ = x.set_fullscreen(false);
                let _ = x.configure(prev);
            }
            self.space.map_element(w.clone(), prev.loc, true);
            return;
        }
        let cur = Rectangle::new(self.space.element_location(w).unwrap_or_default(), w.geometry().size);
        *d.restore.borrow_mut() = Some(cur);
        let area = if full {
            og
        } else {
            let z = layer_map_for_output(&output).non_exclusive_zone();
            let bar = self.bar_h(w);
            Rectangle::new(og.loc + z.loc + Point::from((0, bar)), (z.size.w, z.size.h - bar).into())
        };
        if let Some(t) = &t {
            t.with_pending_state(|s| {
                s.size = Some(area.size);
                if full {
                    s.states.set(xdg_toplevel::State::Fullscreen);
                } else {
                    s.states.set(xdg_toplevel::State::Maximized);
                }
            });
            t.send_pending_configure();
        } else if let Some(x) = &x {
            let _ = if full { x.set_fullscreen(true) } else { x.set_maximized(true) };
            let _ = x.configure(area);
        }
        // Space locations are window-geometry origins (CSD shadows are outside them)
        self.space.map_element(w.clone(), area.loc, true);
    }

    /// X11 windows learn where Mist put them (override-redirect menus are placed in root coordinates).
    pub fn sync_x11(&mut self) {
        for w in self.space.elements() {
            let (Some(x), Some(loc)) = (w.x11_surface(), self.space.element_location(w)) else { continue };
            if !x.is_override_redirect() && x.geometry().loc != loc {
                let _ = x.configure(Rectangle::new(loc, x.geometry().size));
            }
        }
    }

    fn neighbour(&self, from: &Window, dir: Dir) -> Option<Window> {
        let c = |w: &Window| {
            let g = self.space.element_geometry(w).unwrap_or_default();
            (g.loc.x + g.size.w / 2, g.loc.y + g.size.h / 2)
        };
        let (fx, fy) = c(from);
        self.space
            .elements()
            .filter(|w| *w != from)
            .filter_map(|w| {
                let (x, y) = c(w);
                let (dx, dy) = (x - fx, y - fy);
                let ok = match dir {
                    Dir::Left => dx < 0,
                    Dir::Right => dx > 0,
                    Dir::Up => dy < 0,
                    Dir::Down => dy > 0,
                };
                ok.then(|| (dx * dx + dy * dy, w.clone()))
            })
            .min_by_key(|(d, _)| *d)
            .map(|(_, w)| w)
    }
}

pub fn spawn(cmd: &str) {
    use std::os::unix::process::CommandExt;
    let r = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .process_group(0)
        .spawn();
    match r {
        // reap in the background so no zombies pile up
        Ok(mut c) => {
            std::thread::spawn(move || c.wait());
        }
        Err(e) => tracing::warn!("exec {cmd:?}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_groups() {
        assert_eq!([1, 2, 3, 4, 6, 7].map(super::ws_group), [0, 0, 0, 1, 1, 2]);
        assert_eq!(super::ws_group(0), 0);
    }
}
