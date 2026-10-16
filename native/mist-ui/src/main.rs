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
mod i18n;
mod theme;
mod tray;
mod desktop;
mod lock;
mod appwin;
mod edit;
mod files;
mod player;
mod idle;
mod polkit;
mod pane;
mod session;
mod calendar;
mod notifcenter;
mod switcher;
mod clipboard;
mod control;
mod phonepanel;
mod shot;
mod spotlight;
mod start;
mod snap;
mod wallpapers;
mod wxcard;
mod wincap;
mod winpick;
mod desksettings;
#[path = "../../common/astro.rs"]
mod astro;
#[path = "../../common/paths.rs"]
mod paths;
#[path = "../../common/presets.rs"]
mod presets;
#[path = "../../common/sky.rs"]
mod sky;
mod wall;

use calloop::{channel, EventLoop};
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

fn main() {
    // ~35 threads, mostly asleep: glibc would give each one that allocates its own malloc arena (memory freed in
    // one is not reused by the others); two arenas are plenty for a shell
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("msg") {
        // client of the running mist-ui: mist-ui-bin msg <target> <function> [args...]
        std::process::exit(ipc::call(&args[2..]));
    }
    if args.get(1).map(|s| s.as_str()) == Some("player") {
        std::process::exit(player::run(&args[2..]));
    }
    if args.get(1).map(|s| s.as_str()) == Some("files") {
        std::process::exit(files::run(&args[2..]));
    }
    // `lock [preview N]`: the session lock, its own process while locked
    if args.get(1).map(|s| s.as_str()) == Some("lock") {
        std::process::exit(lock::run(&args[2..]));
    }
    // `greeter [greeter.json]`: the same screen as the login under greetd
    if args.get(1).map(|s| s.as_str()) == Some("greeter") {
        std::process::exit(lock::run(&args[1..]));
    }
    // `--desk-png dir`: the desktop stage and the weather card (cached data), for checks
    if args.get(1).map(|s| s.as_str()) == Some("--desk-png") {
        let mut t = text::Text::new();
        let mut d = desktop::Desk::new();
        let rd = |n: &str| -> serde_json::Value { std::fs::read_to_string(format!("{}/.cache/mist/{n}.json", std::env::var("HOME").unwrap_or_default())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() };
        d.weather = rd("weather");
        d.garmin = rd("garmin");
        d.running = rd("running");
        let t0 = std::time::Instant::now();
        d.paint_stage(&mut t);
        eprintln!("stage {:?}", t0.elapsed());
        let t0 = std::time::Instant::now();
        d.dirty = true;
        d.paint_stage(&mut t);
        eprintln!("stage again {:?}", t0.elapsed());
        let t0 = std::time::Instant::now();
        for _ in 0..100 {
            wxcard::Wx::n_insights(&d.weather, &d.garmin, &d.cfg);
        }
        eprintln!("insights x100 {:?}", t0.elapsed());
        d.canvas.pm.save_png(format!("{}/stage.png", args[2])).unwrap();
        let p = desktop::CARD_PAD;
        let mut c = gfx::Canvas::new((desktop::WX_W + 2.0 * p) as u32, (desktop::BENTO_H + 2.0 * p) as u32);
        let (wx, g) = (d.weather.clone(), d.garmin.clone());
        let t0 = std::time::Instant::now();
        d.wx.paint(&mut c, &mut t, &wx, &g, &d.cfg);
        eprintln!("card {:?}", t0.elapsed());
        c.pm.save_png(format!("{}/card.png", args[2])).unwrap();
        return;
    }
    // `--wall-ppm out.ppm`: the main monitor's photo wallpaper (with the vignette), for checks
    if args.get(1).map(|s| s.as_str()) == Some("--wall-ppm") {
        let (_, image) = wall::Wall::read_cfg();
        let px = wall::render(&image, 1920, 1080, true);
        let mut out = b"P6\n1920 1080\n255\n".to_vec();
        for p in px.chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        std::fs::write(&args[2], out).unwrap();
        return;
    }
    if args.get(1).map(|s| s.as_str()) == Some("--png") {
        return png(&args);
    }
    // `--island-bench <demo> [out.png]`: an island demo drawn every 16 ms for 1.5 s (frame cost, to_bgra cost),
    // the last frame into a PNG
    if args.get(1).map(|s| s.as_str()) == Some("--island-bench") {
        let (mut t, mut ic, mut im, ph) = (text::Text::new(), apps::Icons::new(), img::Images::new(), phone::State::default());
        let mut isl = island::Island::new();
        let mut c = gfx::Canvas::new(island::WIN_W, island::WIN_H);
        let now = data::now();
        let mut frame = |isl: &mut island::Island, c: &mut gfx::Canvas| {
            let mut cx = island::Ctx { t: &mut t, icons: &mut ic, imgs: &mut im, phone: &ph, now };
            let t0 = std::time::Instant::now();
            isl.draw(c, &mut cx, false, false);
            t0.elapsed().as_micros() as u64
        };
        frame(&mut isl, &mut c);
        let kind = args.get(2).map(|s| s.as_str()).unwrap_or("volume");
        // `eq`: a playing track at rest, frames of the equalizer alone against full frames drawn right after
        if kind == "eq" {
            isl.set_players(vec![media::Player { identity: "Bench".into(), title: "Track".into(), artist: "Artist".into(), playing: true, ..Default::default() }]);
            let start = std::time::Instant::now();
            while !isl.eq_only() && start.elapsed().as_secs() < 5 {
                frame(&mut isl, &mut c);
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            let (mut n, mut fast_us, mut full_us, mut worst, mut off) = (0u64, 0u64, 0u64, 0u8, 0u64);
            let start = std::time::Instant::now();
            while start.elapsed().as_millis() < 1500 {
                std::thread::sleep(std::time::Duration::from_millis(16));
                let mut a = gfx::Canvas::new(island::WIN_W, island::WIN_H);
                a.pm.data_mut().copy_from_slice(c.pm.data());
                let t0 = std::time::Instant::now();
                if isl.eq_frame(&mut a).is_none() {
                    eprintln!("no eq spot");
                    break;
                }
                fast_us += t0.elapsed().as_micros() as u64;
                full_us += frame(&mut isl, &mut c);
                for (i, (p, q)) in a.pm.data().iter().zip(c.pm.data()).enumerate() {
                    let d = p.abs_diff(*q);
                    if d > 4 && std::env::var("MIST_DIFF").is_ok() {
                        eprintln!("px {} {} ch {} fast {p} full {q}", (i / 4) as u32 % island::WIN_W, (i / 4) as u32 / island::WIN_W, i % 4);
                    }
                    worst = worst.max(d);
                    off += (d > 0) as u64;
                }
                n += 1;
            }
            println!("eq frames {n}: fast {} us full {} us; vs full: {off} channel values differ, worst {worst}", fast_us / n.max(1), full_us / n.max(1));
            if let Some(p) = args.get(3) {
                c.pm.save_png(p).unwrap();
            }
            return;
        }
        isl.demo(kind);
        let (start, mut n, mut us) = (std::time::Instant::now(), 0u64, 0u64);
        while start.elapsed().as_millis() < 1500 {
            us += frame(&mut isl, &mut c);
            n += 1;
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let mut buf = vec![0u8; (island::WIN_W * island::WIN_H * 4) as usize];
        let t0 = std::time::Instant::now();
        for _ in 0..200 {
            c.to_bgra(&mut buf);
        }
        let bgra = t0.elapsed().as_micros() / 200;
        let mut sc = gfx::Canvas::new(island::WIN_W, island::WIN_H);
        let t0 = std::time::Instant::now();
        for _ in 0..200 {
            glass::shadow(&mut sc, 80.0, 13.0, 400.0, 110.0, 30.0, 24.0, -4.0, gfx::Rgba(0.0, 0.0, 0.0, 0.45));
        }
        println!("frames {n} draw {} us/frame to_bgra {bgra} us shadow(400x110) {} us", us / n.max(1), t0.elapsed().as_micros() / 200);
        if let Some(p) = args.get(3) {
            c.pm.save_png(p).unwrap();
        }
        return;
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
        // MIST_BENCH=n: the average frame
        if let Some(n) = std::env::var("MIST_BENCH").ok().and_then(|v| v.parse::<u32>().ok()) {
            let t0 = std::time::Instant::now();
            for _ in 0..n {
                d.draw(&mut c, &mut t, &mut ic);
            }
            eprintln!("dock frame {:.0} us", t0.elapsed().as_secs_f64() * 1e6 / n.max(1) as f64);
        }
        c.pm.save_png(&args[2]).unwrap();
        return;
    }
    if args.get(1).map(|s| s.as_str()) == Some("--pane-png") {
        // a popup on a transparent w x h canvas, settled: --pane-png <name> <w> <h> <out.png> [query]
        let (pn, pg) = args[2].split_once(':').unwrap_or((&args[2], ""));
        let Some(mut p) = pane::make(pn, pg) else { return eprintln!("no native pane {}", args[2]) };
        if let Some(q) = args.get(6) {
            p.seed(q);
        }
        let (w, h): (u32, u32) = (args[3].parse().unwrap(), args[4].parse().unwrap());
        let (ap, cal) = (apps::Apps::load(), serde_json::Value::Null);
        let (mut t, mut ic, mut im, mut hid) = (text::Text::new(), apps::Icons::new(), img::Images::new(), std::collections::HashMap::new());
        let mut c = gfx::Canvas::new(w, h);
        let bst = bar::BarState { net: "ethernet".into(), ..Default::default() };
        loop {
            let mut cx = pane::Ctx { t: &mut t, cal: &cal, icons: &mut ic, imgs: &mut im, apps: &ap, notif_hidden: &mut hid, ctrl: false, st: &bst, players: &[], phone_name: "", wx: &serde_json::Value::Null };
            p.paint(&mut c, &mut cx);
            if !p.animating() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        // MIST_BENCH=n: the settled paint's average time
        if let Some(n) = std::env::var("MIST_BENCH").ok().and_then(|v| v.parse::<u32>().ok()) {
            let t0 = std::time::Instant::now();
            for _ in 0..n {
                let mut cx = pane::Ctx { t: &mut t, cal: &cal, icons: &mut ic, imgs: &mut im, apps: &ap, notif_hidden: &mut hid, ctrl: false, st: &bst, players: &[], phone_name: "", wx: &serde_json::Value::Null };
                p.paint(&mut c, &mut cx);
            }
            eprintln!("paint {:.2} ms", t0.elapsed().as_secs_f64() * 1000.0 / n.max(1) as f64);
        }
        c.pm.save_png(&args[5]).unwrap();
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
    // night light remembered across restarts (Control centre toggles it)
    if native_services && control::night_on() {
        let c = control::night_cmd(true);
        let _ = std::process::Command::new(&c[0]).args(&c[1..]).spawn();
    }
    if std::env::var("MIST_UI_DEBUG").is_ok() { eprintln!("dbg: before App::new"); }
    let mut app = app::App::new(&globals, &qh, req_tx, test, phone, ntx, mdtx);
    app.tx = Some(tx.clone());
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
        polkit::spawn(tx.clone());
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
    idle::logind(tx.clone());
    tray::spawn(tx, req_rx, mtx);

    // wait for events; draw what changed after each batch. The animation tick (16 ms) runs only while something
    // animates. The slow tick (island timers, the clock's minute, call timeouts) runs every 250 ms while
    // something it times is pending, else every 1 s; an event that makes something pending pulls it in
    let mut last_tick = std::time::Instant::now();
    let mut last_slow = std::time::Instant::now();
    loop {
        let slow_left = app.slow_period().saturating_sub(last_slow.elapsed());
        let timeout = if app.animating() { slow_left.min(app::tick_interval().saturating_sub(last_tick.elapsed())) } else { slow_left };
        let timeout = app.intent_due().map(|d| timeout.min(d.saturating_duration_since(std::time::Instant::now()))).unwrap_or(timeout);
        ev.dispatch(Some(timeout), &mut app).expect("dispatch");
        app.intent_tick();
        if last_slow.elapsed() >= app.slow_period() {
            last_slow = std::time::Instant::now();
            app.slow_tick();
        }
        let t0 = std::time::Instant::now();
        if app.animating() && last_tick.elapsed() >= app::tick_interval() {
            last_tick = std::time::Instant::now();
            app.tick();
        }
        let t1 = std::time::Instant::now();
        app.draw_all();
        let _ = conn.flush();
        app::count("tick_us", (t1 - t0).as_micros() as u64);
        app::count("draw_us", t1.elapsed().as_micros() as u64);
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
