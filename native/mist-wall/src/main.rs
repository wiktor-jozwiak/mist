// mist-wall: the animated wallpaper (was LiveWallpaper.qml + LiveWall.qml) on every monitor, drawn on the GPU.
// It is the only Mist process with a GL context (~160 MB on NVIDIA), so it runs only while a live scene is
// chosen in ~/.config/mist/wallpaper.json ({"live": id, "tempo", "music"}); with a photo it exits and
// mist-ui draws the photo on the CPU. Scenes: shaders/live|fx|toon.frag (GLSL 150 taken from the .qsb that
// Qt used), presets from components/LivePresets.js, the vignette (shaders/vignette.frag) on the main monitor.
// Pacing like before: `fps` (30) ticks, 5 behind windows (only their blur shows it), paused under a
// fullscreen window or while the wallpaper gallery covers the monitor; the music level lifts it gently.
#[path = "../../common/astro.rs"]
mod astro;
#[path = "../../common/presets.rs"]
mod presets;

use glow::HasContext;
use serde_json::Value;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use wayland_client::{globals::registry_queue_init, protocol::{wl_output, wl_surface}, Connection, Proxy, QueueHandle};

type Egl = khronos_egl::Instance<khronos_egl::Static>;

const PRIMARY: &str = "DP-3";
// the main monitor's scene is visible: only then is the music measured
static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

struct Surf {
    output: wl_output::WlOutput,
    name: String,
    layer: LayerSurface,
    w: i32,
    h: i32,
    egl_win: Option<wayland_egl::WlEglSurface>,
    egl_surf: Option<khronos_egl::Surface>,
    last: Instant,
}

struct Prog {
    prog: glow::Program,
    loc: HashMap<String, glow::UniformLocation>,
}

struct App {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    qh: QueueHandle<App>,
    surfs: Vec<Surf>,
    egl: Egl,
    display: khronos_egl::Display,
    config: khronos_egl::Config,
    ctx: khronos_egl::Context,
    gl: Option<glow::Context>,
    vao: Option<glow::VertexArray>,
    progs: HashMap<String, Option<Prog>>,
    // state
    presets: Vec<Value>,
    live: String,
    tempo: String,
    music: bool,
    weather: Value,
    t: f64,
    energy: f32,
    energy_to: f32,
    gallery: (bool, String), // the wallpaper gallery is open on that monitor
    hidden: HashMap<String, bool>,
    fs: HashMap<String, bool>,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
fn read_json(p: &str) -> Value {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

// GLSL 150 of a Qt .qsb (cached by mtime in $XDG_RUNTIME_DIR/mist/shaders)
fn shader_src(name: &str) -> Option<String> {
    let qsb = format!("{}/.config/quickshell/mist/shaders/{name}.frag.qsb", home());
    let m = std::fs::metadata(&qsb).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let dir = format!("{}/mist/shaders", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let _ = std::fs::create_dir_all(&dir);
    let out = format!("{dir}/{name}.{m}.frag");
    if !std::path::Path::new(&out).exists() {
        std::process::Command::new("/usr/lib/qt6/bin/qsb").args(["-x", "glsl,150", "-o", &out, &qsb]).status().ok()?;
    }
    std::fs::read_to_string(out).ok()
}

const VERT: &str = "#version 150\nin vec2 pos;\nout vec2 qt_TexCoord0;\nvoid main() { qt_TexCoord0 = vec2(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5); gl_Position = vec4(pos, 0.0, 1.0); }\n";

impl App {
    fn gl(&self) -> &glow::Context {
        self.gl.as_ref().unwrap()
    }
    fn prog(&mut self, name: &str) -> Option<&Prog> {
        if !self.progs.contains_key(name) {
            let p = self.compile(name);
            if p.is_none() {
                eprintln!("mist-wall: shader {name} failed");
            }
            self.progs.insert(name.to_string(), p);
        }
        self.progs.get(name).and_then(|p| p.as_ref())
    }
    fn compile(&self, name: &str) -> Option<Prog> {
        let src = shader_src(name)?;
        // the uniform block instance name SPIRV-Cross picked ("uniform buf _339;")
        let inst = src.lines().find_map(|l| l.trim().strip_prefix("uniform buf ").map(|r| r.trim_end_matches(';').trim().to_string())).unwrap_or_default();
        let fields: Vec<String> = {
            let s = src.find("struct buf")?;
            let body = &src[s..src[s..].find("};")? + s];
            body.lines().filter_map(|l| l.trim().strip_suffix(';').and_then(|x| x.split_whitespace().nth(1)).map(String::from)).collect()
        };
        unsafe {
            let gl = self.gl();
            let vs = gl.create_shader(glow::VERTEX_SHADER).ok()?;
            gl.shader_source(vs, VERT);
            gl.compile_shader(vs);
            let fs = gl.create_shader(glow::FRAGMENT_SHADER).ok()?;
            gl.shader_source(fs, &src);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                eprintln!("{}", gl.get_shader_info_log(fs));
                return None;
            }
            let p = gl.create_program().ok()?;
            gl.attach_shader(p, vs);
            gl.attach_shader(p, fs);
            gl.bind_attrib_location(p, 0, "pos");
            gl.link_program(p);
            if !gl.get_program_link_status(p) {
                eprintln!("{}", gl.get_program_info_log(p));
                return None;
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            let mut loc = HashMap::new();
            for f in fields {
                if let Some(l) = gl.get_uniform_location(p, &format!("{inst}.{f}")) {
                    loc.insert(f, l);
                }
            }
            Some(Prog { prog: p, loc })
        }
    }

    fn add_output(&mut self, o: wl_output::WlOutput) {
        let Some(info) = self.outputs.info(&o) else { return };
        let name = info.name.clone().unwrap_or_default();
        if self.surfs.iter().any(|s| s.name == name) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Background, Some("mist-live"), Some(&o));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        if let Ok(r) = Region::new(&self.compositor) {
            layer.wl_surface().set_input_region(Some(r.wl_region()));
        }
        layer.commit();
        self.surfs.push(Surf { output: o, name, layer, w: 0, h: 0, egl_win: None, egl_surf: None, last: Instant::now() });
    }

    fn preset(&self) -> Value {
        presets::get(&self.presets, &self.live).cloned().or_else(|| self.presets.first().cloned()).unwrap_or(Value::Null)
    }

    // uniforms of LiveWall.qml for the current preset, time, weather and sun
    fn uniforms(&self, w: f32, h: f32) -> (String, Vec<(&'static str, Vec<f32>)>, f32) {
        let p = self.preset();
        let wx = &self.weather;
        let lat = wx["lat"].as_f64().unwrap_or(52.2297);
        let lon = wx["lon"].as_f64().unwrap_or(21.0122);
        let auto = p["auto"].as_bool().unwrap_or(false);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        let now_min = (now / 60000.0).floor() * 60000.0; // Data.now ticks per minute
        let code = wx["code"].as_i64().unwrap_or(3);
        let cloud = ((wx["cloud"].as_f64().unwrap_or(if code >= 3 { 90.0 } else { 20.0 })) / 100.0).clamp(0.0, 1.0) as f32;
        let wet = wx["precip"].as_f64().unwrap_or(0.0) > 0.0 || code >= 51;
        let sun_p = if auto { Some(astro::sun(now_min, lat, lon)) } else { None };
        let mut mode = p["mode"].as_f64().unwrap_or(0.0) as i64;
        if p["weather"].as_bool().unwrap_or(false) {
            mode = if (71..=77).contains(&code) || code == 85 || code == 86 { 16 } else if wet { 8 } else if code == 45 || code == 48 { 20 } else if sun_p.map(|s| s.alt < -6.0).unwrap_or(false) && cloud < 0.5 { 11 } else { 14 };
        }
        let moon_p = if auto && (mode == 5 || mode == 14) { Some(astro::moon(now_min, lat, lon)) } else { None };
        let (cols, stars) = match (auto, sun_p) {
            (true, Some(s)) => {
                let (c, st) = presets::day(s.alt, cloud, wet);
                (c, st * if mode == 5 || mode == 14 { 0.8 } else { 0.5 })
            }
            _ => {
                let c = p["c"].as_array().cloned().unwrap_or_default();
                let mut out = [[0f32; 3]; 4];
                for (i, v) in c.iter().take(4).enumerate() {
                    out[i] = presets::hex(v.as_str().unwrap_or("#000000"));
                }
                (out, p["stars"].as_f64().unwrap_or(0.0) as f32)
            }
        };
        let place = |az: f64, alt: f64, strength: f32| -> Vec<f32> { vec![(0.5 + (az - 180.0) / 140.0).clamp(0.08, 0.92) as f32, (0.64 - alt.max(-5.0) / 60.0 * 0.6) as f32, 0.026, strength] };
        let none = vec![0.5, 2.0, 0.02, 0.0];
        let sun_v = if mode != 5 && mode != 14 {
            vec![0.0; 4]
        } else if !auto {
            p["sun"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or(none.clone())
        } else {
            match (sun_p, moon_p) {
                (Some(s), _) if s.alt > -3.0 => place(s.az, s.alt, 1.0 - 0.75 * cloud),
                (_, Some(m)) if m.alt > 0.0 => place(m.az, m.alt, -(0.25 + 0.6 * m.fraction as f32) * (1.0 - 0.8 * cloud)),
                _ => none,
            }
        };
        let shader = match p["shader"].as_str() { Some("toon") => "toon", Some("fx") => "fx", _ => "live" }.to_string();
        let wind_k = if auto { (0.8 + wx["wind"].as_f64().unwrap_or(0.0) / 25.0).min(2.2) as f32 } else { 1.0 };
        let c = |i: usize| vec![cols[i][0], cols[i][1], cols[i][2], 1.0];
        let u = vec![
            ("qt_Opacity", vec![1.0]),
            ("time", vec![self.t as f32]),
            ("mode", vec![mode as f32]),
            ("size", vec![w, h]),
            ("c0", c(0)),
            ("c1", c(1)),
            ("c2", c(2)),
            ("c3", c(3)),
            ("opt", vec![stars, p["vig"].as_f64().unwrap_or(0.5) as f32, p["bright"].as_f64().unwrap_or(1.0) as f32, self.energy]),
            ("sun", sun_v),
        ];
        (shader, u, wind_k)
    }

    fn draw(&mut self, i: usize) {
        let (w, h) = (self.surfs[i].w, self.surfs[i].h);
        let Some(es) = self.surfs[i].egl_surf else { return };
        if self.egl.make_current(self.display, Some(es), Some(es), Some(self.ctx)).is_err() {
            return;
        }
        let primary = self.surfs[i].name == PRIMARY;
        self.scene(w, h, primary);
        let _ = self.egl.swap_buffers(self.display, es);
        self.surfs[i].last = Instant::now();
    }

    fn scene(&mut self, w: i32, h: i32, primary: bool) {
        let (shader, u, _) = self.uniforms(w as f32, h as f32);
        let vao = self.vao;
        unsafe {
            self.gl().viewport(0, 0, w, h);
            self.gl().bind_vertex_array(vao);
        }
        if let Some(p) = self.prog(&shader) {
            let (prog, loc) = (p.prog, p.loc.clone());
            let gl = self.gl();
            unsafe {
                gl.disable(glow::BLEND);
                gl.use_program(Some(prog));
                for (k, v) in &u {
                    if let Some(l) = loc.get(*k) {
                        match v.len() {
                            1 => gl.uniform_1_f32(Some(l), v[0]),
                            2 => gl.uniform_2_f32(Some(l), v[0], v[1]),
                            4 => gl.uniform_4_f32(Some(l), v[0], v[1], v[2], v[3]),
                            _ => {}
                        }
                    }
                }
                if let Some(l) = loc.get("qt_Matrix") {
                    gl.uniform_matrix_4_f32_slice(Some(l), false, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                }
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
        }
        // the vignette under the desktop cards (main monitor), premultiplied over the scene
        if primary {
            if let Some(p) = self.prog("vignette") {
                let (prog, loc) = (p.prog, p.loc.clone());
                let gl = self.gl();
                unsafe {
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                    gl.use_program(Some(prog));
                    if let Some(l) = loc.get("qt_Opacity") { gl.uniform_1_f32(Some(l), 1.0) }
                    if let Some(l) = loc.get("size") { gl.uniform_2_f32(Some(l), w as f32, h as f32) }
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
            }
        }
    }

    fn cur_fps(&self, name: &str) -> f32 {
        if *self.hidden.get(name).unwrap_or(&false) { 5.0 } else { 30.0 }
    }
    fn animate(&self, name: &str) -> bool {
        !*self.fs.get(name).unwrap_or(&false) && !(self.gallery.0 && self.gallery.1 == name)
    }

    // one tick: advance time by the primary's pace, draw the monitors that are due
    fn tick(&mut self) {
        self.energy += (self.energy_to - self.energy) * 0.15;
        let p = self.preset();
        let (_, _, wind_k) = self.uniforms(1.0, 1.0);
        let speed = p["speed"].as_f64().unwrap_or(1.0) as f32 * presets::tempo(&self.tempo) * wind_k * (1.0 + 0.4 * self.energy);
        let fps = self.surfs.iter().find(|s| s.name == PRIMARY).map(|s| self.cur_fps(&s.name)).unwrap_or(30.0);
        self.t += (speed / fps) as f64;
        for i in 0..self.surfs.len() {
            let name = self.surfs[i].name.clone();
            if !self.animate(&name) || self.surfs[i].egl_surf.is_none() {
                continue;
            }
            let due = 1.0 / self.cur_fps(&name);
            if self.surfs[i].last.elapsed().as_secs_f32() + 0.002 >= due {
                self.draw(i);
            }
        }
    }
}

// windows covering a monitor (Cover.qml): 85 % of the area outside the bars, sampled on a 32 x 18 grid
fn coverage() -> (HashMap<String, bool>, HashMap<String, bool>) {
    let req = |cmd: &str| -> Value {
        use std::io::{Read, Write};
        let p = format!("{}/hypr/{}/.socket.sock", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(), std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default());
        let Ok(mut s) = std::os::unix::net::UnixStream::connect(p) else { return Value::Null };
        let _ = s.write_all(cmd.as_bytes());
        let mut o = String::new();
        let _ = s.read_to_string(&mut o);
        serde_json::from_str(&o).unwrap_or(Value::Null)
    };
    let (mons, clients) = (req("j/monitors"), req("j/clients"));
    let (mut hid, mut fs) = (HashMap::new(), HashMap::new());
    for m in mons.as_array().cloned().unwrap_or_default() {
        let name = m["name"].as_str().unwrap_or("").to_string();
        let ws = m["activeWorkspace"]["name"].as_str().unwrap_or("").to_string();
        let sp = m["specialWorkspace"]["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("-").to_string();
        let r = m["reserved"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or(vec![0.0; 4]);
        let (x, y, w, h) = (m["x"].as_f64().unwrap_or(0.0) + r[0], m["y"].as_f64().unwrap_or(0.0) + r[1], m["width"].as_f64().unwrap_or(1.0) - r[0] - r[2], m["height"].as_f64().unwrap_or(1.0) - r[1] - r[3]);
        let mut rects = Vec::new();
        let mut full = false;
        for c in clients.as_array().cloned().unwrap_or_default() {
            let wn = c["workspace"]["name"].as_str().unwrap_or("");
            if c["hidden"].as_bool().unwrap_or(false) || c["mapped"].as_bool() == Some(false) || (wn != ws && wn != sp) {
                continue;
            }
            if wn == ws && c["fullscreen"].as_i64().unwrap_or(0) > 0 {
                full = true;
            }
            let at = c["at"].as_array().map(|a| (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            let sz = c["size"].as_array().map(|a| (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            rects.push((at.0, at.1, sz.0, sz.1));
        }
        let mut n = 0;
        for i in 0..32 {
            let px = x + (i as f64 + 0.5) * w / 32.0;
            for j in 0..18 {
                let py = y + (j as f64 + 0.5) * h / 18.0;
                if rects.iter().any(|b| px >= b.0 && px < b.0 + b.2 && py >= b.1 && py < b.1 + b.3) {
                    n += 1;
                }
            }
        }
        hid.insert(name.clone(), n as f64 / 576.0 >= 0.85);
        fs.insert(name, full);
    }
    (hid, fs)
}

enum Ev {
    Config,
    Weather,
    Ui,
    Cover(HashMap<String, bool>, HashMap<String, bool>),
    Level(f32),
}

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, queue) = registry_queue_init::<App>(&conn).expect("registry");
    let qh = queue.handle();
    // EGL + desktop GL 3.3 core (the GLSL 150 shaders of the .qsb files)
    let egl = Egl::new(khronos_egl::Static);
    let display = unsafe { egl.get_display(conn.backend().display_ptr() as *mut std::ffi::c_void) }.expect("egl display");
    egl.initialize(display).expect("egl init");
    egl.bind_api(khronos_egl::OPENGL_API).expect("gl api");
    let attrs = [khronos_egl::RED_SIZE, 8, khronos_egl::GREEN_SIZE, 8, khronos_egl::BLUE_SIZE, 8, khronos_egl::ALPHA_SIZE, 8, khronos_egl::SURFACE_TYPE, khronos_egl::WINDOW_BIT, khronos_egl::RENDERABLE_TYPE, khronos_egl::OPENGL_BIT, khronos_egl::NONE];
    let config = egl.choose_first_config(display, &attrs).ok().flatten().expect("egl config");
    let cattrs = [khronos_egl::CONTEXT_MAJOR_VERSION, 3, khronos_egl::CONTEXT_MINOR_VERSION, 3, khronos_egl::CONTEXT_OPENGL_PROFILE_MASK, khronos_egl::CONTEXT_OPENGL_CORE_PROFILE_BIT, khronos_egl::NONE];
    let ctx = egl.create_context(display, config, None, &cattrs).expect("egl context");
    let mut app = App {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).expect("compositor"),
        layer_shell: LayerShell::bind(&globals, &qh).expect("layer shell"),
        qh: qh.clone(),
        surfs: Vec::new(),
        egl,
        display,
        config,
        ctx,
        gl: None,
        vao: None,
        progs: HashMap::new(),
        presets: presets::load(),
        live: String::new(),
        tempo: "normal".into(),
        music: false,
        weather: read_json(&format!("{}/.cache/mist/weather.json", home())),
        // a wall-clock start keeps all monitors in step
        t: (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)) % 10000.0,
        energy: 0.0,
        energy_to: 0.0,
        gallery: (false, String::new()),
        hidden: HashMap::new(),
        fs: HashMap::new(),
    };
    let load_cfg = |app: &mut App| -> bool {
        let c = read_json(&format!("{}/.config/mist/wallpaper.json", home()));
        app.live = c["live"].as_str().unwrap_or("").to_string();
        app.tempo = c["tempo"].as_str().unwrap_or("normal").to_string();
        app.music = c["music"].as_bool().unwrap_or(false);
        !app.live.is_empty()
    };
    // `mist-wall --png out.ppm [preset] [w h]`: one frame offscreen (surfaceless context + FBO), for checks
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--png").unwrap_or(false) {
        load_cfg(&mut app);
        if let Some(id) = args.get(3) {
            app.live = id.clone();
        }
        let w: i32 = args.get(4).and_then(|x| x.parse().ok()).unwrap_or(1920);
        let h: i32 = args.get(5).and_then(|x| x.parse().ok()).unwrap_or(1080);
        app.egl.make_current(app.display, None, None, Some(app.ctx)).expect("surfaceless");
        let gl = unsafe { glow::Context::from_loader_function(|n| app.egl.get_proc_address(n).map(|p| p as *const _).unwrap_or(std::ptr::null())) };
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            let fb = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            let rb = gl.create_renderbuffer().unwrap();
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rb));
            let vao = gl.create_vertex_array().ok();
            gl.bind_vertex_array(vao);
            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let v: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, std::slice::from_raw_parts(v.as_ptr() as *const u8, 24), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            app.vao = vao;
            app.gl = Some(gl);
            app.scene(w, h, true);
            app.gl().read_pixels(0, 0, w, h, glow::RGB, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px[..(w * h * 3) as usize])));
        }
        // GL rows go bottom-up
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in (0..h).rev() {
            out.extend_from_slice(&px[(y * w * 3) as usize..((y + 1) * w * 3) as usize]);
        }
        std::fs::write(&args[2], out).unwrap();
        return;
    }
    if !load_cfg(&mut app) {
        return; // a photo: mist-ui draws it
    }
    // one instance (mist-ui restarts spawn it again): exit 3 = already running
    let lock = format!("{}/mist/wall.lock", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let _ = std::fs::create_dir_all(std::path::Path::new(&lock).parent().unwrap());
    let lf = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&lock).expect("lock file");
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lf), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        std::process::exit(3);
    }
    let mut ev: calloop::EventLoop<App> = calloop::EventLoop::try_new().unwrap();
    let lh = ev.handle();
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue).insert(lh.clone()).unwrap();
    let (tx, rx) = calloop::channel::channel::<Ev>();
    // config / weather / popup files (inotify)
    {
        let tx = tx.clone();
        std::thread::spawn(move || unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC);
            let dirs = [format!("{}/.config/mist", home()), format!("{}/.cache/mist", home()), format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default())];
            for d in &dirs {
                let c = std::ffi::CString::new(d.as_str()).unwrap();
                libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO);
            }
            let mut buf = [0u8; 4096];
            loop {
                let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                if n <= 0 {
                    return;
                }
                let mut i = 0usize;
                while i < n as usize {
                    let e = &*(buf.as_ptr().add(i) as *const libc::inotify_event);
                    let name = std::ffi::CStr::from_ptr(buf.as_ptr().add(i + std::mem::size_of::<libc::inotify_event>()) as *const libc::c_char).to_string_lossy().to_string();
                    let m = match name.as_str() { "wallpaper.json" => Some(Ev::Config), "weather.json" => Some(Ev::Weather), "ui.json" => Some(Ev::Ui), _ => None };
                    if let Some(m) = m {
                        let _ = tx.send(m);
                    }
                    i += std::mem::size_of::<libc::inotify_event>() + e.len as usize;
                }
            }
        });
    }
    // windows over the monitors: on Hyprland events and every 3 s (moving / resizing sends none)
    {
        let tx1 = tx.clone();
        std::thread::spawn(move || loop {
            let (h, f) = coverage();
            if tx1.send(Ev::Cover(h, f)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_secs(3));
        });
        let tx2 = tx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let p = format!("{}/hypr/{}/.socket2.sock", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(), std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default());
            let Ok(s) = std::os::unix::net::UnixStream::connect(p) else { return };
            for l in std::io::BufReader::new(s).lines().map_while(Result::ok) {
                let e = l.split(">>").next().unwrap_or("");
                if matches!(e, "openwindow" | "closewindow" | "movewindowv2" | "changefloatingmode" | "fullscreen" | "workspacev2" | "focusedmonv2" | "activespecialv2" | "activewindowv2") {
                    std::thread::sleep(Duration::from_millis(150));
                    let (h, f) = coverage();
                    if tx2.send(Ev::Cover(h, f)).is_err() {
                        return;
                    }
                }
            }
        });
    }
    // music: the default output's loudness while a player plays (LiveCfg.qml: parec | RMS, slow envelope)
    {
        let tx = tx.clone();
        std::thread::spawn(move || music_loop(tx));
    }
    lh.insert_source(rx, |e, _, app: &mut App| {
        let calloop::channel::Event::Msg(m) = e else { return };
        match m {
            Ev::Config => {
                if !{
                    let c = read_json(&format!("{}/.config/mist/wallpaper.json", home()));
                    app.live = c["live"].as_str().unwrap_or("").to_string();
                    app.tempo = c["tempo"].as_str().unwrap_or("normal").to_string();
                    app.music = c["music"].as_bool().unwrap_or(false);
                    !app.live.is_empty()
                } {
                    std::process::exit(0); // back to a photo: mist-ui draws it
                }
                app.presets = presets::load();
            }
            Ev::Weather => app.weather = read_json(&format!("{}/.cache/mist/weather.json", home())),
            Ev::Ui => {
                let u = read_json(&format!("{}/mist/ui.json", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default()));
                app.gallery = (u["popup"].as_str() == Some("wallpaper"), u["screen"].as_str().unwrap_or("").to_string());
            }
            Ev::Cover(h, f) => {
                app.hidden = h;
                app.fs = f;
                SHOWN.store(app.animate(PRIMARY) && !*app.hidden.get(PRIMARY).unwrap_or(&false), std::sync::atomic::Ordering::Relaxed);
            }
            Ev::Level(l) => app.energy_to = l,
        }
    })
    .unwrap();
    lh.insert_source(calloop::timer::Timer::from_duration(Duration::from_millis(33)), |_, _, app: &mut App| {
        if app.gl.is_some() {
            app.tick();
        }
        // wake at the primary's pace (30 fps, 5 behind windows)
        let fps = app.surfs.iter().find(|s| s.name == PRIMARY).map(|s| app.cur_fps(&s.name)).unwrap_or(30.0);
        calloop::timer::TimeoutAction::ToDuration(Duration::from_millis((1000.0 / fps.max(5.0)) as u64))
    })
    .unwrap();
    loop {
        ev.dispatch(None, &mut app).unwrap();
        let _ = conn.flush();
    }
}

// parec on the default sink monitor -> RMS with automatic gain -> slow envelope (0..1), only while a player plays
fn music_loop(tx: calloop::channel::Sender<Ev>) {
    use std::io::Read;
    loop {
        let cfg = read_json(&format!("{}/.config/mist/wallpaper.json", home()));
        let playing = std::process::Command::new("sh").args(["-c", "for p in $(busctl --user list --no-legend | awk '{print $1}' | grep '^org.mpris.MediaPlayer2'); do busctl --user get-property $p /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlaybackStatus; done"]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains("Playing")).unwrap_or(false);
        if !cfg["music"].as_bool().unwrap_or(false) || !playing || !SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tx.send(Ev::Level(0.0));
            std::thread::sleep(Duration::from_secs(3));
            continue;
        }
        let Ok(mut ch) = std::process::Command::new("parec").args(["-d", "@DEFAULT_MONITOR@", "--raw", "--format=s16le", "--rate=8000", "--channels=1", "--latency-msec=40"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else { return };
        let mut out = ch.stdout.take().unwrap();
        let mut buf = [0u8; 532];
        let (mut pk, mut e) = (400.0f32, 0.0f32);
        let start = Instant::now();
        while out.read_exact(&mut buf).is_ok() {
            let n = buf.len() / 2;
            let s: f32 = buf.chunks_exact(2).map(|c| { let v = i16::from_le_bytes([c[0], c[1]]) as f32; v * v }).sum();
            let r = (s / n as f32).sqrt();
            pk = (pk * 0.996).max(r).max(400.0);
            e += (r / pk - e) * 0.1;
            if tx.send(Ev::Level(e.min(1.0))).is_err() {
                let _ = ch.kill();
                return;
            }
            if start.elapsed().as_secs() >= 10 || !SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
                break; // re-check the player / config every 10 s
            }
        }
        let _ = ch.kill();
        let _ = ch.wait();
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.add_output(o);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.surfs.retain(|s| s.output != o);
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface) {
        self.surfs.retain(|s| &s.layer != l);
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        let Some(i) = self.surfs.iter().position(|s| &s.layer == l) else { return };
        let (w, h) = (c.new_size.0.max(1) as i32, c.new_size.1.max(1) as i32);
        let s = &mut self.surfs[i];
        s.w = w;
        s.h = h;
        if let Some(win) = &s.egl_win {
            win.resize(w, h, 0, 0);
        } else {
            let Ok(win) = wayland_egl::WlEglSurface::new(s.layer.wl_surface().id(), w, h) else { return };
            let es = unsafe { self.egl.create_window_surface(self.display, self.config, win.ptr() as khronos_egl::NativeWindowType, None) };
            s.egl_surf = es.ok();
            s.egl_win = Some(win);
        }
        let es = self.surfs[i].egl_surf;
        if let Some(es) = es {
            let _ = self.egl.make_current(self.display, Some(es), Some(es), Some(self.ctx));
            let _ = self.egl.swap_interval(self.display, 0);
            if self.gl.is_none() {
                let gl = unsafe { glow::Context::from_loader_function(|n| self.egl.get_proc_address(n).map(|p| p as *const _).unwrap_or(std::ptr::null())) };
                unsafe {
                    // one fullscreen triangle
                    let vao = gl.create_vertex_array().ok();
                    gl.bind_vertex_array(vao);
                    let vbo = gl.create_buffer().unwrap();
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                    let v: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, std::slice::from_raw_parts(v.as_ptr() as *const u8, 24), glow::STATIC_DRAW);
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
                    self.vao = vao;
                }
                self.gl = Some(gl);
            }
            self.draw(i);
        }
    }
}
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
delegate_compositor!(App);
delegate_output!(App);
delegate_layer!(App);
delegate_registry!(App);
