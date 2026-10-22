//! A client drawing nobody sees: commits per client counted all the time (a map add per commit), once a
//! minute the ones over 20 commits/s whose windows are all hidden (another workspace, minimized) go to the
//! log, and over 300/s any client (a busy loop). The first sign of a wasted CPU/GPU, with no measuring by hand.
use crate::state::MistComp;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::{backend::ClientId, Resource};
use smithay::wayland::seat::WaylandFocus;
use std::time::Duration;

fn period() -> u64 {
    std::env::var("MIST_COMP_WATCHDOG_SECS").ok().and_then(|v| v.parse().ok()).filter(|v| *v > 0).unwrap_or(60)
}

pub fn start(state: &mut MistComp) {
    let _ = state.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(period())), |_, _, st: &mut MistComp| {
        st.watchdog_check();
        TimeoutAction::ToDuration(Duration::from_secs(period()))
    });
}

impl MistComp {
    pub fn count_commit(&mut self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) {
        if let Some(c) = surface.client() {
            *self.commit_rate.entry(c.id()).or_default() += 1;
        }
    }

    fn watchdog_check(&mut self) {
        let counts = std::mem::take(&mut self.commit_rate);
        let client_of = |w: &smithay::desktop::Window| w.wl_surface().and_then(|s| s.client()).map(|c| c.id());
        for (id, n) in counts {
            let rate = n as f64 / period() as f64;
            if rate < 20.0 {
                continue;
            }
            let shown: Vec<_> = self.space.elements().filter(|w| client_of(w).as_ref() == Some(&id)).cloned().collect();
            let hidden: Vec<_> = self.stash.iter().map(|(w, _)| w).filter(|w| client_of(w).as_ref() == Some(&id)).cloned().collect();
            let name = |w: &smithay::desktop::Window| crate::hyprcompat::app_id(w);
            if shown.is_empty() && !hidden.is_empty() {
                tracing::warn!("watchdog: {} draws {rate:.0} frames/s while hidden", name(&hidden[0]));
            } else if rate > 300.0 {
                let who = shown.first().map(name).unwrap_or_else(|| self.client_name(&id));
                tracing::warn!("watchdog: {who} commits {rate:.0}/s");
            }
        }
    }

    /// A client without windows (a shell surface): the layer namespace of one of its surfaces.
    fn client_name(&self, id: &ClientId) -> String {
        for o in self.space.outputs() {
            for l in smithay::desktop::layer_map_for_output(o).layers() {
                if l.wl_surface().client().is_some_and(|c| &c.id() == id) {
                    return l.namespace().to_string();
                }
            }
        }
        format!("{id:?}")
    }
}
