// mist-ui: the Mist shell surfaces drawn on the CPU (no GPU context, so no NVIDIA driver in this process).
// Replaces the Quickshell (QML) surfaces one by one; see native/README.md.
//   mist-ui            run (bars on every monitor)
//   mist-ui --test     one bar on HDMI-A-1 in the Overlay layer, no exclusive zone (side by side check)
//   mist-ui --png <out.png> '<state json>'   render a bar into a PNG
mod anim;
mod app;
mod apps;
mod bar;
mod data;
mod dock;
mod glass;
mod img;
mod ipc;
mod island;
mod media;
mod notif;
mod phone;
mod popup;
mod gfx;
mod icons;
mod sys;
mod text;
mod theme;
mod tray;

use calloop::{channel, timer::{TimeoutAction, Timer}, EventLoop};
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("msg") {
        // client of the running mist-ui: mist-ui-bin msg <target> <function> [args...]
        std::process::exit(ipc::call(&args[2..]));
    }
    if args.get(1).map(|s| s.as_str()) == Some("--png") {
        return png(&args);
    }
    if args.get(1).map(|s| s.as_str()) == Some("--dock-png") {
        // the dock as it is now (hovering slot N when given) into a PNG
        let h = sys::hypr_now();
        let ap = apps::Apps::load();
        let mut ic = apps::Icons::new();
        let mut d = dock::Dock::new();
        d.rebuild(&h, &ap);
        d.bare = true;
        d.hover_slot = args.get(3).and_then(|s| s.parse().ok());
        d.update_lift();
        d.update_y();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut t = text::Text::new();
        let mut c = gfx::Canvas::new(d.win_w(), dock::WIN_H);
        d.draw(&mut c, &mut t, &mut ic);
        c.pm.save_png(&args[2]).unwrap();
        return;
    }
    if args.get(1).map(|s| s.as_str()) == Some("--tray-dump") {
        // list the tray items and their menus (checks the SNI host + dbusmenu parsing without a click)
        let (tx, rx) = channel::channel::<sys::Msg>();
        let (mtx, mrx) = channel::channel::<(String, Vec<tray::MenuEntry>)>();
        let (req_tx, req_rx) = std::sync::mpsc::channel::<tray::Req>();
        tray::spawn(tx, req_rx, mtx);
        let mut ev: EventLoop<()> = EventLoop::try_new().unwrap();
        let rt = req_tx.clone();
        ev.handle().insert_source(rx, move |e, _, _| {
            if let channel::Event::Msg(sys::Msg::Tray(items)) = e {
                for i in &items {
                    println!("item {} id={} icon={} pixmap={:?} menu={:?}", i.key, i.id, i.icon_name, i.pixmap.as_ref().map(|p| (p.0, p.1)), i.menu);
                    let _ = rt.send(tray::Req::Menu(i.key.clone()));
                }
            }
        }).unwrap();
        ev.handle().insert_source(mrx, |e, _, _| {
            if let channel::Event::Msg((k, entries)) = e {
                println!("menu of {k}:");
                fn pr(v: &[tray::MenuEntry], d: usize) { for e in v { println!("{}{} {:?}{}{}", "  ".repeat(d + 1), e.id, e.label, if e.separator { " ---" } else { "" }, if e.enabled { "" } else { " (off)" }); pr(&e.submenu, d + 1); } }
                pr(&entries, 0);

            }
        }).unwrap();
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < 3 { ev.dispatch(Some(std::time::Duration::from_millis(100)), &mut ()).unwrap(); }
        return;
    }
    let test = args.iter().any(|a| a == "--test");
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, queue) = registry_queue_init::<app::App>(&conn).expect("registry");
    let qh = queue.handle();
    let mut ev: EventLoop<app::App> = EventLoop::try_new().expect("event loop");
    let lh = ev.handle();
    WaylandSource::new(conn.clone(), queue).insert(lh.clone()).expect("wayland source");

    let (tx, rx) = channel::channel::<sys::Msg>();
    let (mtx, mrx) = channel::channel::<(String, Vec<tray::MenuEntry>)>();
    let (req_tx, req_rx) = std::sync::mpsc::channel::<tray::Req>();
    let (ntx, nrx) = std::sync::mpsc::channel::<notif::Cmd>();
    let (mdtx, mdrx) = std::sync::mpsc::channel::<media::Cmd>();
    let native_services = std::env::var("MIST_UI_NO_SERVICES").is_err() && !test;
    let phone = if native_services { phone::Phone::start(tx.clone()) } else { phone::Phone::start_idle() };
    if std::env::var("MIST_UI_DEBUG").is_ok() { eprintln!("dbg: before App::new"); }
    let mut app = app::App::new(&globals, &qh, req_tx, test, phone, ntx, mdtx);
    lh.insert_source(rx, |e, _, app: &mut app::App| {
        if let channel::Event::Msg(m) = e {
            app.on_msg(m);
        }
    })
    .unwrap();
    lh.insert_source(mrx, |e, _, app: &mut app::App| {
        if let channel::Event::Msg((k, entries)) = e {
            app.open_tray_menu(k, entries);
        }
    })
    .unwrap();
    sys::hyprland(tx.clone());
    sys::network(tx.clone());
    sys::bluetooth(tx.clone());
    sys::audio(tx.clone());
    sys::stats(tx.clone());
    if std::env::var("MIST_UI_DEBUG").is_ok() { eprintln!("dbg: before state_files"); }
    sys::state_files(tx.clone());
    if native_services {
        // the services Quickshell ran: notifications, players, data, phone, the island's event sources
        notif::spawn(tx.clone(), nrx);
    }
    if native_services || std::env::var("MIST_UI_SOCK").is_ok() {
        if std::env::var("MIST_UI_DEBUG").is_ok() { eprintln!("dbg: ipc serve {}", ipc::path()); }
        ipc::serve(tx.clone());
    }
    media::spawn(tx.clone(), mdrx);
    data::spawn(tx.clone());
    sys::power_profile(tx.clone());
    sys::island_events(tx.clone());
    sys::nm_monitor(tx.clone());
    tray::spawn(tx, req_rx, mtx);

    // animation tick, only while something fades
    lh.insert_source(Timer::from_duration(app::tick_interval()), |_, _, app: &mut app::App| {
        if app.animating() {
            app.tick();
        }
        TimeoutAction::ToDuration(app::tick_interval())
    })
    .unwrap();
    // seconds: island timers, the clock's minute, call timeouts
    lh.insert_source(Timer::from_duration(std::time::Duration::from_millis(250)), |_, _, app: &mut app::App| {
        app.slow_tick();
        TimeoutAction::ToDuration(std::time::Duration::from_millis(250))
    })
    .unwrap();
    loop {
        // wait for events; draw what changed after each batch
        ev.dispatch(None, &mut app).expect("dispatch");
        app.draw_all();
        let _ = conn.flush();
    }
}

fn png(args: &[String]) {
    let st: serde_json::Value = serde_json::from_str(args.get(3).map(|s| s.as_str()).unwrap_or("{}")).unwrap();
    let g = |k: &str| st.get(k).cloned().unwrap_or(serde_json::Value::Null);
    let s = bar::BarState {
        primary: true,
        app: g("app").as_str().unwrap_or("").into(),
        title: g("title").as_str().unwrap_or("").into(),
        show_win: g("show").as_bool().unwrap_or(true),
        cpu: g("cpu").as_i64().unwrap_or(5) as i32,
        mem: g("mem").as_f64().unwrap_or(10.0) as f32,
        mem_total: 31.3,
        rx: g("rx").as_f64().unwrap_or(0.0),
        bt_on: g("bt").as_bool().unwrap_or(false),
        net: "ethernet".into(),
        vol: g("vol").as_f64().unwrap_or(0.5) as f32,
        ..Default::default()
    };
    let mut t = text::Text::new();
    let mut c = gfx::Canvas::new(g("w").as_u64().unwrap_or(1920) as u32, bar::HEIGHT);
    bar::draw(&mut c, &mut t, &s, None, None, false, &|_, _, _, _, _| {});
    c.pm.save_png(&args[2]).unwrap();
}
