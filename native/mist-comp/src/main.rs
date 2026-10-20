#![allow(irrefutable_let_patterns)]

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
mod wlr;
mod outman;
mod migrate;
mod decor;
mod scene;
mod rounded;
mod blur;
mod anim;
mod rules;
mod swallow;
mod snap;
mod focus;
mod xwayland;
#[path = "../../mist-ui/src/text.rs"]
#[allow(dead_code)]
mod text;
// text.rs translates what it draws (the greeter, titles stay as they are)
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
        data.wakeups += 1;
        if std::mem::take(&mut data.dirty) {
            data.sync_x11();
            data.update_bars();
            match &backend {
                Some(b) => {
                    data.redraw_pending = true;
                    b.borrow().window().request_redraw()
                }
                None if data.headless.is_some() => crate::headless::render(data),
                None => crate::udev::render_all(data),
            }
            // one more frame while something moves (paced by vblank / the frame tick)
            if data.anim.active() {
                data.dirty = true;
                // headless has no vblank to wake the loop: this arms its frame tick
                if backend.is_none() && data.headless.is_some() {
                    crate::headless::render(data);
                }
            }
        }
        // no refresh on its way (nothing changed on screen, headless, nested): FIFO waiters go now
        let frame_on_its_way = data.udev.as_ref().is_some_and(|u| u.any_pending()) || data.redraw_pending || crate::headless::frame_pending(data);
        if !data.dirty && !frame_on_its_way {
            data.fifo_signal();
        }
        crate::screencopy::process(data);
        data.ftm_sync();
        data.update_scales();
        let _ = data.display_handle.flush_clients();
        tracing::trace!("loop iteration");
    })?;

    Ok(())
}
