#![allow(irrefutable_let_patterns)]

mod prof;
mod cover;
mod session;
mod watchdog;
mod handlers;

mod grabs;
mod input;
mod ipc;
mod hyprcompat;
mod state;
mod config;
mod udev;
mod wm;
mod lock;
mod headless;
mod winit;
mod cursor;
mod screencopy;
mod hyprproto;
mod wlr;
mod outman;
mod migrate;
mod decor;
mod scene;
mod rounded;
mod blur;
mod shadow;
mod anim;
mod overview;
mod bgeffect;
mod extws;
mod rules;
mod swallow;
mod snap;
mod focus;
mod xwayland;
#[path = "../../mist-ui/src/text.rs"]
#[allow(dead_code)]
mod text;
// text.rs translates what it draws (the greeter, titles stay as they are)
#[path = "../../common/tokens.rs"]
mod tokens;
#[path = "../../common/paths.rs"]
#[allow(dead_code)]
mod paths;
#[path = "../../mist-ui/src/i18n.rs"]
#[allow(dead_code)]
mod i18n;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::Display,
};
pub use state::MistComp;

/// The event loop's data is the compositor state itself (smithay helpers like the idle notifier need
/// `LoopHandle<MistComp>`).
pub type CalloopData = MistComp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // colours only on a terminal (start-mist writes ~/.cache/mist/comp.log)
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stderr());
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_ansi(ansi).with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().with_ansi(ansi).init();
    }

    // Mist's own binaries (mist-ui-bin in the key bindings) live in ~/.local/bin, which a session started by
    // greetd / a display manager may not have in PATH
    if let Ok(home) = std::env::var("HOME") {
        let bin = format!("{home}/.local/bin");
        let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
        if !path.split(':').any(|p| p == bin) {
            std::env::set_var("PATH", format!("{bin}:{path}"));
        }
    }

    // `hyprctl` for scripts on a session without Hyprland (start-mist links it): `hyprctl [-j] cmd args…`
    let argv: Vec<String> = std::env::args().collect();
    let as_hyprctl = argv.first().is_some_and(|a| a.rsplit('/').next() == Some("hyprctl"));
    if as_hyprctl || argv.get(1).map(String::as_str) == Some("hyprctl") {
        let rest = if as_hyprctl { &argv[1..] } else { &argv[2..] };
        std::process::exit(hyprcompat::cli(rest));
    }
    if argv.get(1).map(String::as_str) == Some("migrate") {
        std::process::exit(migrate::run(&argv[2..]));
    }
    if crate::config::safe_mode() {
        // read by blur.rs / anim.rs on first use
        std::env::set_var("MIST_COMP_BLUR", "0");
        std::env::set_var("MIST_COMP_ANIM", "0");
        tracing::warn!("safe mode: no blur, animations, VRR, explicit sync or dmabuf screen casting");
    }
    if std::env::args().any(|a| a == "--probe") {
        return crate::udev::probe();
    }
    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;

    let display: Display<MistComp> = Display::new()?;
    let mut data = MistComp::new(&mut event_loop, display);

    // nested inside another compositor unless started from a TTY (or forced with --tty)
    let nested = (std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some())
        && !std::env::args().any(|a| a == "--tty");
    let headless = std::env::args().any(|a| a == "--headless");
    let backend = if headless {
        crate::headless::init(&mut event_loop, &mut data)?;
        std::env::set_var("WAYLAND_DISPLAY", &data.socket_name);
        None
    } else if nested {
        Some(crate::winit::init_winit(&mut event_loop, &mut data)?)
    } else {
        crate::udev::init(&mut event_loop, &mut data)?;
        std::env::set_var("WAYLAND_DISPLAY", &data.socket_name);
        None
    };
    data.dirty = true;
    let xw = std::env::var("MIST_COMP_XWAYLAND").ok();
    if xw.as_deref() == Some("1") || (!nested && !headless && xw.as_deref() != Some("0")) {
        let h = event_loop.handle();
        crate::xwayland::start(&mut data, &h);
    }
    let sock = data.socket_name.to_string_lossy().into_owned();
    ipc::serve(&mut event_loop, &sock);
    watchdog::start(&mut data);
    data.session_restore();
    hyprcompat::serve(&mut event_loop, &sock);

    // `-c CMD`: run one client (tests); otherwise the config's `exec` lines (only in a real session, or with
    // MIST_COMP_AUTOSTART=1 nested, so tests never start a second shell)
    let args: Vec<String> = std::env::args().collect();
    // `--session CMD`: run it and quit with it (the greeter: greetd starts the user session once it ends)
    if let Some(cmd) = args.iter().position(|a| a == "--session").and_then(|i| args.get(i + 1)) {
        let signal = data.loop_signal.clone();
        match std::process::Command::new("sh").arg("-c").arg(cmd).stdin(std::process::Stdio::null()).spawn() {
            Ok(mut c) => {
                std::thread::spawn(move || {
                    let _ = c.wait();
                    signal.stop();
                    signal.wakeup();
                });
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else if let Some(i) = args.iter().position(|a| a == "-c" || a == "--command") {
        if let Some(cmd) = args.get(i + 1) {
            crate::wm::spawn(cmd);
        }
    } else if (!nested && !headless) || std::env::var_os("MIST_COMP_AUTOSTART").is_some() {
        for cmd in data.config.exec.clone() {
            crate::wm::spawn(&cmd);
        }
    }

    // no fixed tick: the loop sleeps until a client or input event; a frame is requested only when dirty
    tracing::trace!("run");
    event_loop.run(None, &mut data, move |data| {
        let turn = std::time::Instant::now();
        data.wakeups += 1;
        data.prune_inhibitors();
        let full = std::mem::take(&mut data.dirty);
        let part = std::mem::take(&mut data.dirty_outputs);
        if full || !part.is_empty() {
            data.sync_x11();
            data.update_bars();
            match &backend {
                Some(b) => {
                    data.redraw_pending = true;
                    b.borrow().window().request_redraw()
                }
                None if data.headless.is_some() => crate::headless::render(data, if full { None } else { Some(&part) }),
                None => {
                    if full {
                        crate::udev::render_all(data);
                    } else {
                        crate::udev::render_outputs(data, &part);
                    }
                    // wl_surface.enter / leave for the outputs a window covers (clients pick their scale,
                    // refresh rate and menu placement from it), dead windows and popups dropped
                    data.space.refresh();
                    data.popups.cleanup();
                }
            }
            // one more frame while something moves (paced by vblank / the frame tick)
            if data.anim.active() {
                data.dirty = true;
                // headless has no vblank to wake the loop: this arms its frame tick
                if backend.is_none() && data.headless.is_some() {
                    crate::headless::render(data, None);
                }
            }
        }
        // no refresh on its way (nothing changed on screen, headless, nested): FIFO waiters go now
        let frame_on_its_way = data.udev.as_ref().is_some_and(|u| u.any_pending()) || data.redraw_pending || crate::headless::frame_pending(data);
        if !data.dirty && !frame_on_its_way {
            data.fifo_signal();
        }
        crate::screencopy::process(data);
        crate::hyprproto::process(data);
        // window bookkeeping (what covers what, workspace and toplevel lists, monitor scales): at once after
        // a change of the layout, at most 10 times a second while clients only draw (a 165 Hz browser woke
        // it at every commit)
        let now = std::time::Instant::now();
        if !part.is_empty() {
            data.sync_due = true;
        }
        let ripe = data.sync_at.is_none_or(|t| now.duration_since(t) >= std::time::Duration::from_millis(100));
        if full || (data.sync_due && ripe) {
            let t = std::time::Instant::now();
            data.sync_at = Some(now);
            data.sync_due = false;
            data.cover_sync();
            data.extws_sync();
            data.ftm_sync();
            data.update_scales();
            crate::prof::add(5, t.elapsed());
        } else if data.sync_due && !data.sync_timer {
            data.sync_timer = true;
            let _ = data.loop_handle.insert_source(smithay::reexports::calloop::timer::Timer::from_duration(std::time::Duration::from_millis(100)), |_, _, st: &mut MistComp| {
                st.sync_timer = false;
                smithay::reexports::calloop::timer::TimeoutAction::Drop
            });
        }
        let _ = data.display_handle.flush_clients();
        crate::prof::add(4, turn.elapsed());
        tracing::trace!("loop iteration");
    })?;

    if data.restart {
        std::process::exit(75);
    }
    Ok(())
}
