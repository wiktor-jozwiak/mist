// mist-wall: everything Mist draws on the GPU, in one process (the only one with a GL context: ~160 MB on
// NVIDIA). Started (and restarted) by mist-ui, one instance (flock).
//  - the wallpaper of every monitor (was LiveWallpaper.qml + LiveWall.qml): ~/.config/mist/wallpaper.json
//    {"live": id | "", "image", "tempo", "music"}. A live scene: shaders/live|fx|toon.frag (GLSL 150 taken
//    from the .qsb Qt used), presets from components/LivePresets.js; 30 fps, 5 behind windows (only their
//    blur shows it), paused under a fullscreen window or while the wallpaper gallery covers the monitor; the
//    music level lifts it gently. A photo: one texture per monitor size, cross-faded 600 ms, drawn only when
//    it changes. The desktop vignette (shaders/vignette.frag) over both on the main monitor.
//  - the weather sky of the desktop weather card (was components/Sky.qml): shaders/clouds.frag at half
//    resolution into a texture (15 fps), shaders/sky.frag over it (30 fps, 5 behind windows), in a Background
//    surface at the card's place ($XDG_RUNTIME_DIR/mist/sky.json, written by mist-ui, which draws the card's
//    text above it on the Bottom layer).
#[path = "../../common/astro.rs"]
mod astro;
#[path = "../../common/presets.rs"]
mod presets;
#[path = "../../common/sky.rs"]
mod sky;

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use wayland_client::{globals::registry_queue_init, protocol::{wl_output, wl_surface}, Connection, Proxy, QueueHandle};

type Egl = khronos_egl::Instance<khronos_egl::Static>;

const PRIMARY: &str = "DP-3";
const FADE_MS: f32 = 600.0;
// the main monitor's scene is visible: only then is the music measured
static SHOWN: AtomicBool = AtomicBool::new(true);

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Wall,
    Sky,
    Preview,
}

struct Surf {
    kind: Kind,
    output: wl_output::WlOutput,
    name: String,
    layer: LayerSurface,
    w: i32,
    h: i32,
    egl_win: Option<wayland_egl::WlEglSurface>,
    egl_surf: Option<khronos_egl::Surface>,
    last: Instant,
    dirty: bool,
}

struct Prog {
    prog: glow::Program,
    loc: HashMap<String, glow::UniformLocation>,
}

// one monitor's windows (Cover.qml)
#[derive(Clone, Default)]
struct Mon {
    name: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    reserved: [f64; 4],
    rects: Vec<(f64, f64, f64, f64)>,
    fs: bool,
}

// a value that glides to a new target (QML Behavior: NumberAnimation, linear or InOutSine)
#[derive(Clone, Copy)]
struct Smooth {
    from: f32,
    to: f32,
    start: Instant,
    dur: f32,
    sine: bool,
}
impl Smooth {
    fn new(v: f32, dur_ms: f32, sine: bool) -> Smooth {
        Smooth { from: v, to: v, start: Instant::now(), dur: dur_ms / 1000.0, sine }
    }
    fn get(&self) -> f32 {
        let t = (self.start.elapsed().as_secs_f32() / self.dur).min(1.0);
        let e = if self.sine { -((std::f32::consts::PI * t).cos() - 1.0) / 2.0 } else { t };
        self.from + (self.to - self.from) * e
    }
    fn set(&mut self, v: f32) {
        if (v - self.to).abs() > 1e-6 {
            self.from = self.get();
            self.to = v;
            self.start = Instant::now();
        }
    }
}

struct Photo {
    path: String,
    tex: HashMap<(i32, i32), glow::Texture>,
    old: HashMap<(i32, i32), glow::Texture>,
    fade: Option<Instant>,
    asked: Vec<(String, i32, i32)>,
}

// the wallpaper gallery's full-screen preview (mist-ui writes $XDG_RUNTIME_DIR/mist/preview.json
// {"screen", "key": "live:<id>" | "photo:<path>"}): a Top-layer surface under mist-ui's gallery pane; the scene
// renders into fbo[cur], cross-faded 420 ms from the frozen fbo[1 - cur]
struct Prev {
    screen: String,
    key: String,
    shown: String,
    fbo: [Option<(glow::Framebuffer, glow::Texture)>; 2],
    size: (i32, i32),
    cur: usize,
    fade: Option<Instant>,
    photo: Option<(String, glow::Texture)>,
    asked: String,
    t: f64,
}
const PREV_FADE_MS: f32 = 420.0;

struct SkyState {
    rect: Option<(i32, i32, i32, i32)>, // on the primary monitor
    model: sky::Model,
    model_at: i64,
    off_x: f64,
    evo: f64,
    ct: f64,
    c_off: f64,
    c_evo: f64,
    tick: u32,
    last: Instant,
    last_bolt: [f64; 4],
    // smoothed inputs: cover, rain, snow, fog, haze, slant, frost, mist, heat, ground snow
    sm: [Smooth; 10],
    fbo: Option<(glow::Framebuffer, glow::Texture, i32, i32)>,
    hidden: bool,
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
    // wallpaper
    presets: Vec<Value>,
    live: String,
    image: String,
    tempo: String,
    weather: Value,
    t: f64,
    t_last: Instant,
    energy: f32,
    energy_to: f32,
    gallery: (bool, String), // the wallpaper gallery is open on that monitor
    prev: Prev,
    prev_tx: std::sync::mpsc::Sender<(String, i32, i32, Vec<u8>)>,
    prev_rx: std::sync::mpsc::Receiver<(String, i32, i32, Vec<u8>)>,
    hidden: HashMap<String, bool>,
    fs: HashMap<String, bool>,
    mons: Vec<Mon>,
    photo: Photo,
    photo_tx: std::sync::mpsc::Sender<(String, i32, i32, Vec<u8>)>,
    photo_rx: std::sync::mpsc::Receiver<(String, i32, i32, Vec<u8>)>,
    sky: SkyState,
    target_fb: Option<glow::Framebuffer>, // None = the window; an FBO for --png
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
fn run_dir() -> String {
    format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()))
}
fn read_json(p: &str) -> Value {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}
fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

// GLSL 150 of a Qt .qsb (cached by mtime in $XDG_RUNTIME_DIR/mist/shaders)
fn shader_src(name: &str) -> Option<String> {
    if name == "photo" {
        return Some(PHOTO.to_string());
    }
    if name == "scrim" {
        return Some(SCRIM.to_string());
    }
    let qsb = format!("{}/.config/quickshell/mist/shaders/{name}.frag.qsb", home());
    let m = std::fs::metadata(&qsb).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let dir = format!("{}/shaders", run_dir());
    let _ = std::fs::create_dir_all(&dir);
    let out = format!("{dir}/{name}.{m}.frag");
    if !std::path::Path::new(&out).exists() {
        std::process::Command::new("/usr/lib/qt6/bin/qsb").args(["-x", "glsl,150", "-o", &out, &qsb]).status().ok()?;
    }
    std::fs::read_to_string(out).ok()
}

// qt_TexCoord0 has its origin top-left; `mist_flip` renders into a texture the way Qt's layers do
const VERT: &str = "#version 150\nin vec2 pos;\nout vec2 qt_TexCoord0;\nuniform float mist_flip;\nvoid main() { qt_TexCoord0 = vec2(pos.x * 0.5 + 0.5, mix(0.5 - pos.y * 0.5, 0.5 + pos.y * 0.5, mist_flip)); gl_Position = vec4(pos, 0.0, 1.0); }\n";
// the photo (already cropped to the monitor), cross-fading from the previous one
const PHOTO: &str = "#version 150\nin vec2 qt_TexCoord0;\nout vec4 fragColor;\nuniform sampler2D a;\nuniform sampler2D b;\nuniform float mixk;\nvoid main() { fragColor = vec4(mix(texture(a, qt_TexCoord0).rgb, texture(b, qt_TexCoord0).rgb, mixk), 1.0); }\n";

// the gallery's scrim under the shelf and the title (Wallpapers.qml): bottom 55 %, black 0 -> .35 at 55 % -> .62,
// premultiplied (drawn here, not in mist-ui's pane, so Hyprland's blur behind the pane skips it)
const SCRIM: &str = "#version 150\nin vec2 qt_TexCoord0;\nout vec4 fragColor;\nvoid main() { float s = clamp((qt_TexCoord0.y - 0.45) / 0.55, 0.0, 1.0); float a = s < 0.55 ? 0.35 * s / 0.55 : 0.35 + 0.27 * (s - 0.55) / 0.45; fragColor = vec4(0.0, 0.0, 0.0, a); }\n";

// the photo cropped to w x h (PreserveAspectCrop) as RGBA, rows top first
fn decode_photo(path: &str, w: i32, h: i32) -> Vec<u8> {
    let im = image::ImageReader::open(path).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.decode().ok());
    match im {
        Some(im) => im.resize_to_fill(w as u32, h as u32, image::imageops::FilterType::Triangle).to_rgba8().into_raw(),
        None => {
            let mut v = vec![0u8; (w * h * 4) as usize];
            v.chunks_exact_mut(4).for_each(|p| p[3] = 255);
            v
        }
    }
}

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
        // the uniform block instance name SPIRV-Cross picked ("uniform buf _339;") and its fields
        let inst = src.lines().find_map(|l| l.trim().strip_prefix("uniform buf ").map(|r| r.trim_end_matches(';').trim().to_string())).unwrap_or_default();
        let mut names: Vec<(String, String)> = Vec::new();
        if let Some(s) = src.find("struct buf") {
            let e = src[s..].find("};").map(|e| e + s).unwrap_or(s);
            for l in src[s..e].lines() {
                if let Some(f) = l.trim().strip_suffix(';').and_then(|x| x.split_whitespace().nth(1)) {
                    names.push((f.to_string(), format!("{inst}.{f}")));
                }
            }
        }
        // plain uniforms (samplers, the photo's) and the vertex shader's flip
        for l in src.lines() {
            let l = l.trim();
            if l.starts_with("uniform ") && !l.starts_with("uniform buf ") {
                if let Some(n) = l.trim_end_matches(';').split_whitespace().last() {
                    names.push((n.to_string(), n.to_string()));
                }
            }
        }
        names.push(("mist_flip".into(), "mist_flip".into()));
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
            for (k, n) in names {
                if let Some(l) = gl.get_uniform_location(p, &n) {
                    loc.insert(k, l);
                }
            }
            Some(Prog { prog: p, loc })
        }
    }
    // use a program and set its uniforms (1, 2 or 4 floats; 16 = a matrix)
    fn run(&mut self, name: &str, u: &[(&str, Vec<f32>)], samplers: &[(&str, glow::Texture)], flip: bool) {
        let Some(p) = self.prog(name) else { return };
        let (prog, loc) = (p.prog, p.loc.clone());
        let gl = self.gl();
        unsafe {
            gl.use_program(Some(prog));
            for (k, v) in u {
                if let Some(l) = loc.get(*k) {
                    match v.len() {
                        1 => gl.uniform_1_f32(Some(l), v[0]),
                        2 => gl.uniform_2_f32(Some(l), v[0], v[1]),
                        4 => gl.uniform_4_f32(Some(l), v[0], v[1], v[2], v[3]),
                        16 => gl.uniform_matrix_4_f32_slice(Some(l), false, v),
                        _ => {}
                    }
                }
            }
            if let Some(l) = loc.get("qt_Matrix") {
                gl.uniform_matrix_4_f32_slice(Some(l), false, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
            }
            if let Some(l) = loc.get("qt_Opacity") {
                gl.uniform_1_f32(Some(l), 1.0);
            }
            if let Some(l) = loc.get("mist_flip") {
                gl.uniform_1_f32(Some(l), if flip { 1.0 } else { 0.0 });
            }
            for (i, (k, t)) in samplers.iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*t));
                if let Some(l) = loc.get(*k) {
                    gl.uniform_1_i32(Some(l), i as i32);
                }
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    fn add_output(&mut self, o: wl_output::WlOutput) {
        let Some(info) = self.outputs.info(&o) else { return };
        let name = info.name.clone().unwrap_or_default();
        if self.surfs.iter().any(|s| s.name == name && s.kind == Kind::Wall) {
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
        self.surfs.push(Surf { kind: Kind::Wall, output: o, name: name.clone(), layer, w: 0, h: 0, egl_win: None, egl_surf: None, last: Instant::now(), dirty: true });
        // the sky must stay above this monitor's wallpaper: surfaces stack in creation order
        if name == PRIMARY {
            self.place_sky(true);
        }
    }

    // the weather card's sky surface at sky.rect (re-created when `force` or the rect moved)
    fn place_sky(&mut self, force: bool) {
        let cur = self.surfs.iter().position(|s| s.kind == Kind::Sky);
        let Some((x, y, w, h)) = self.sky.rect else {
            if let Some(i) = cur {
                self.drop_surf(i);
            }
            return;
        };
        if let Some(i) = cur {
            if !force {
                let s = &self.surfs[i];
                s.layer.set_margin(y, 0, 0, x);
                s.layer.set_size(w as u32, h as u32);
                s.layer.commit();
                return;
            }
            self.drop_surf(i);
        }
        let Some(o) = self.outputs.outputs().find(|o| self.outputs.info(o).and_then(|i| i.name.clone()).as_deref() == Some(PRIMARY)) else { return };
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Background, Some("mist-sky"), Some(&o));
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(y, 0, 0, x);
        layer.set_size(w as u32, h as u32);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        if let Ok(r) = Region::new(&self.compositor) {
            layer.wl_surface().set_input_region(Some(r.wl_region()));
        }
        layer.commit();
        self.surfs.push(Surf { kind: Kind::Sky, output: o, name: PRIMARY.into(), layer, w: 0, h: 0, egl_win: None, egl_surf: None, last: Instant::now(), dirty: true });
    }

    fn drop_surf(&mut self, i: usize) {
        let s = self.surfs.remove(i);
        if let Some(es) = s.egl_surf {
            let _ = self.egl.make_current(self.display, None, None, Some(self.ctx));
            let _ = self.egl.destroy_surface(self.display, es);
        }
    }

    fn preset(&self) -> Value {
        presets::get(&self.presets, &self.live).cloned().or_else(|| self.presets.first().cloned()).unwrap_or(Value::Null)
    }

    // uniforms of LiveWall.qml for the current preset, time, weather and sun
    fn uniforms(&self, w: f32, h: f32) -> (String, Vec<(&'static str, Vec<f32>)>, f32) {
        self.uniforms_of(&self.preset(), w, h, self.t)
    }
    fn uniforms_of(&self, p: &Value, w: f32, h: f32, t: f64) -> (String, Vec<(&'static str, Vec<f32>)>, f32) {
        let p = p.clone();
        let wx = &self.weather;
        let lat = wx["lat"].as_f64().unwrap_or(52.2297);
        let lon = wx["lon"].as_f64().unwrap_or(21.0122);
        let auto = p["auto"].as_bool().unwrap_or(false);
        let now_min = (now_ms() / 60000.0).floor() * 60000.0; // Data.now ticks per minute
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
            ("time", vec![t as f32]),
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

    fn make_current(&self, i: usize) -> bool {
        match self.surfs[i].egl_surf {
            Some(es) => self.egl.make_current(self.display, Some(es), Some(es), Some(self.ctx)).is_ok(),
            None => false,
        }
    }

    fn draw(&mut self, i: usize) {
        if !self.make_current(i) {
            return;
        }
        let (w, h, kind) = (self.surfs[i].w, self.surfs[i].h, self.surfs[i].kind);
        let vao = self.vao;
        unsafe {
            self.gl().bind_framebuffer(glow::FRAMEBUFFER, self.target_fb);
            self.gl().viewport(0, 0, w, h);
            self.gl().bind_vertex_array(vao);
            self.gl().disable(glow::BLEND);
        }
        match kind {
            Kind::Wall => {
                let primary = self.surfs[i].name == PRIMARY;
                self.wall_scene(w, h, primary);
            }
            Kind::Sky => self.sky_scene(w, h),
            Kind::Preview => self.preview_scene(w, h),
        }
        let es = self.surfs[i].egl_surf.unwrap();
        let _ = self.egl.swap_buffers(self.display, es);
        self.surfs[i].last = Instant::now();
        self.surfs[i].dirty = false;
    }

    fn wall_scene(&mut self, w: i32, h: i32, primary: bool) {
        if self.live.is_empty() {
            let key = (w, h);
            match self.photo.tex.get(&key).copied() {
                Some(b) => {
                    let a = self.photo.old.get(&key).copied().unwrap_or(b);
                    let t = self.photo.fade.map(|f| (f.elapsed().as_secs_f32() * 1000.0 / FADE_MS).min(1.0)).unwrap_or(1.0);
                    let e = if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }; // InOutQuad
                    self.run("photo", &[("mixk", vec![e])], &[("a", a), ("b", b)], false);
                }
                None => unsafe {
                    self.gl().clear_color(0.0, 0.0, 0.0, 1.0);
                    self.gl().clear(glow::COLOR_BUFFER_BIT);
                },
            }
        } else {
            let (shader, u, _) = self.uniforms(w as f32, h as f32);
            self.run(&shader, &u, &[], false);
        }
        // the vignette under the desktop cards (main monitor), premultiplied over the scene
        if primary {
            unsafe {
                self.gl().enable(glow::BLEND);
                self.gl().blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            }
            self.run("vignette", &[("size", vec![w as f32, h as f32])], &[], false);
            unsafe { self.gl().disable(glow::BLEND) };
        }
    }

    // Sky.qml: pass A (clouds, half resolution, into a texture) when due, pass B (details, corners) on top
    fn sky_scene(&mut self, w: i32, h: i32) {
        let m = self.sky.model.clone();
        let t = sky::sky_time();
        let b = sky::bolt(t, m.storm_k);
        let bolt = vec![b[0] as f32, b[1] as f32, b[2] as f32, b[3] as f32];
        let sm: Vec<f32> = self.sky.sm.iter().map(|s| s.get()).collect();
        let (cov, rain, snow, fog, haze, slant, frost, mist, heat, gsnow) = (sm[0], sm[1], sm[2], sm[3], sm[4], sm[5], sm[6], sm[7], sm[8], sm[9]);
        let wx = vec![cov, rain, snow, fog];
        let (tw, th) = ((w + 1) / 2, (h + 1) / 2);
        // clouds every 2nd tick (15 fps), every tick behind windows (5 fps) and while lightning changes
        self.sky.tick = self.sky.tick.wrapping_add(1);
        let need = self.sky.fbo.map(|f| f.2 != tw || f.3 != th).unwrap_or(true);
        let bolt_changed = b != self.sky.last_bolt;
        self.sky.last_bolt = b;
        if need {
            unsafe {
                let gl = self.gl.as_ref().unwrap();
                if let Some((fb, tex, _, _)) = self.sky.fbo.take() {
                    gl.delete_framebuffer(fb);
                    gl.delete_texture(tex);
                }
                let tex = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, tw, th, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                let fb = gl.create_framebuffer().unwrap();
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                self.sky.fbo = Some((fb, tex, tw, th));
            }
        }
        if need || self.sky.hidden || self.sky.tick % 2 == 0 || bolt_changed {
            self.sky.ct = t;
            self.sky.c_off = self.sky.off_x;
            self.sky.c_evo = self.sky.evo;
            let d = m.dir(m.sun.az, m.sun.alt);
            let fb = self.sky.fbo.unwrap().0;
            unsafe {
                self.gl().bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                self.gl().viewport(0, 0, tw, th);
            }
            let u = vec![
                ("time", vec![self.sky.ct as f32]),
                ("fov", vec![sky::FOV as f32]),
                ("pitch", vec![m.pitch as f32]),
                ("size", vec![w as f32, h as f32]),
                ("sunDir", vec![d.x as f32, d.y as f32, d.z as f32, m.sun.alt as f32]),
                ("scene", vec![0.0, 0.0, self.sky.c_off as f32, self.sky.c_evo as f32]),
                ("wx", wx.clone()),
                ("wx2", vec![m.storm_k as f32, m.moon_light as f32, m.aurora_k as f32, haze]),
                ("bolt", bolt.clone()),
                ("fx", vec![m.bow_k as f32, sky::HOR as f32, gsnow, 0.0]),
            ];
            self.run("clouds", &u, &[], true);
            unsafe {
                self.gl().bind_framebuffer(glow::FRAMEBUFFER, self.target_fb);
                self.gl().viewport(0, 0, w, h);
            }
        }
        let (sx, sy) = m.sun_s(w as f64, h as f64);
        let (mx, my) = m.moon_s(w as f64, h as f64);
        let (i, z) = (m.moon.inc, m.moon.limb_zenith);
        let u = vec![
            ("time", vec![t as f32]),
            ("radius", vec![25.0]),
            ("size", vec![w as f32, h as f32]),
            ("sun", vec![sx as f32, sy as f32, m.sun.alt as f32, 0.0]),
            ("moon", vec![mx as f32, my as f32, m.moon.alt as f32, m.moon.fraction as f32]),
            ("moonLit", vec![(-z.sin() * i.sin()) as f32, (-z.cos() * i.sin()) as f32, i.cos() as f32, m.moon.pa as f32]),
            ("wx", wx),
            ("wx2", vec![m.storm_k as f32, m.hail_k as f32, if m.rate < 0.5 { 1.0 } else { 0.0 }, m.wind_k as f32]),
            ("fx", vec![slant, 0.0, m.meteor_k as f32, haze]),
            ("bolt", bolt),
            ("life", vec![1.0, m.leaf_k as f32, m.bird_k as f32, m.ray_k as f32]),
            ("env", vec![m.temp as f32, (m.hum / 100.0) as f32, frost, heat]),
            ("env2", vec![gsnow, m.firefly_k as f32, m.mote_k as f32, sky::HOR as f32]),
            ("env3", vec![m.halo_k as f32, mist, m.curtain_k as f32, m.glass_snow_k as f32]),
        ];
        let tex = self.sky.fbo.unwrap().1;
        unsafe {
            self.gl().clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl().clear(glow::COLOR_BUFFER_BIT);
        }
        self.run("sky", &u, &[("src", tex)], false);
    }

    // the weather reading / minute changed: new sky model, smoothed inputs glide to it
    fn sky_model(&mut self) {
        let now_min = (now_ms() / 60000.0).floor() * 60000.0;
        self.sky.model = sky::Model::new(&self.weather, now_min);
        self.sky.model_at = (now_min / 60000.0) as i64;
        let m = &self.sky.model;
        let v = [m.cover_pct / 100.0, m.rain_k, m.snow_k, m.fog_k, m.haze_k, m.slant, m.frost_k, m.mist_k, m.heat_k, m.gsnow_k];
        for (s, x) in self.sky.sm.iter_mut().zip(v) {
            s.set(x as f32);
        }
    }

    fn cur_fps(&self, name: &str) -> f32 {
        if *self.hidden.get(name).unwrap_or(&false) { 5.0 } else { 30.0 }
    }
    fn animate(&self, name: &str) -> bool {
        !*self.fs.get(name).unwrap_or(&false) && !(self.gallery.0 && self.gallery.1 == name)
    }

    // windows over a rect (global coords): 85 % covered = hidden (Cover.qml)
    fn covered(&self, mon: &str, x: f64, y: f64, w: f64, h: f64) -> bool {
        let Some(m) = self.mons.iter().find(|m| m.name == mon) else { return false };
        let mut n = 0;
        for i in 0..32 {
            let px = x + (i as f64 + 0.5) * w / 32.0;
            for j in 0..18 {
                let py = y + (j as f64 + 0.5) * h / 18.0;
                if m.rects.iter().any(|b| px >= b.0 && px < b.0 + b.2 && py >= b.1 && py < b.1 + b.3) {
                    n += 1;
                }
            }
        }
        n as f64 / 576.0 >= 0.85
    }
    fn on_cover(&mut self, mons: Vec<Mon>) {
        self.mons = mons;
        self.hidden.clear();
        self.fs.clear();
        for m in self.mons.clone() {
            let r = m.reserved;
            self.hidden.insert(m.name.clone(), self.covered(&m.name, m.x + r[0], m.y + r[1], m.w - r[0] - r[2], m.h - r[1] - r[3]));
            self.fs.insert(m.name.clone(), m.fs);
        }
        self.sky.hidden = match (self.sky.rect, self.mons.iter().find(|m| m.name == PRIMARY)) {
            (Some((x, y, w, h)), Some(m)) => self.covered(PRIMARY, m.x + x as f64, m.y + y as f64, w as f64, h as f64),
            _ => false,
        };
        SHOWN.store(self.animate(PRIMARY) && !*self.hidden.get(PRIMARY).unwrap_or(&false), Ordering::Relaxed);
    }

    // one tick: advance the clocks, draw what is due; returns how long to sleep
    fn tick(&mut self) -> Duration {
        let dt = self.t_last.elapsed().as_secs_f64().min(0.25);
        self.t_last = Instant::now();
        let mut next = 1.0f32;
        // photos decoded meanwhile
        while let Ok((path, w, h, px)) = self.photo_rx.try_recv() {
            self.photo.asked.retain(|a| !(a.0 == path && a.1 == w && a.2 == h));
            if path != self.photo.path || self.gl.is_none() {
                continue;
            }
            unsafe {
                let gl = self.gl.as_ref().unwrap();
                let tex = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&px)));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                if let Some(prev) = self.photo.tex.insert((w, h), tex) {
                    if let Some(o2) = self.photo.old.insert((w, h), prev) {
                        gl.delete_texture(o2);
                    }
                }
                // fade from the photo shown before (none at start)
                if self.photo.old.contains_key(&(w, h)) {
                    self.photo.fade = Some(Instant::now());
                }
            }
            for s in &mut self.surfs {
                if s.kind == Kind::Wall && (s.w, s.h) == (w, h) {
                    s.dirty = true;
                }
            }
        }
        if self.live.is_empty() {
            // the photo: draw on change and while fading, then free the previous textures
            let fading = self.photo.fade.map(|f| f.elapsed().as_secs_f32() * 1000.0 < FADE_MS + 40.0).unwrap_or(false);
            if self.photo.fade.is_some() && !fading {
                self.photo.fade = None;
                let old: Vec<glow::Texture> = self.photo.old.drain().map(|(_, t)| t).collect();
                for s in &mut self.surfs {
                    if s.kind == Kind::Wall {
                        s.dirty = true;
                    }
                }
                if self.gl.is_some() {
                    for t in old {
                        unsafe { self.gl().delete_texture(t) };
                    }
                }
            }
            for i in 0..self.surfs.len() {
                if self.surfs[i].kind == Kind::Wall && (self.surfs[i].dirty || fading) && self.surfs[i].egl_surf.is_some() {
                    self.draw(i);
                }
            }
            if fading {
                next = next.min(1.0 / 60.0);
            }
        } else {
            self.energy += (self.energy_to - self.energy) * (dt as f32 * 4.0).min(1.0);
            let p = self.preset();
            let (_, _, wind_k) = self.uniforms(1.0, 1.0);
            let speed = p["speed"].as_f64().unwrap_or(1.0) as f32 * presets::tempo(&self.tempo) * wind_k * (1.0 + 0.4 * self.energy);
            if self.animate(PRIMARY) {
                self.t += speed as f64 * dt;
            }
            for i in 0..self.surfs.len() {
                if self.surfs[i].kind != Kind::Wall || self.surfs[i].egl_surf.is_none() {
                    continue;
                }
                let name = self.surfs[i].name.clone();
                if !self.animate(&name) && !self.surfs[i].dirty {
                    continue;
                }
                let due = 1.0 / self.cur_fps(&name);
                let left = due - self.surfs[i].last.elapsed().as_secs_f32();
                if left <= 0.002 || self.surfs[i].dirty {
                    self.draw(i);
                    next = next.min(due);
                } else {
                    next = next.min(left);
                }
            }
        }
        // the gallery's preview: a live scene at 30 fps, a photo when it arrives, both while fading
        while let Ok((path, w, h, px)) = self.prev_rx.try_recv() {
            if self.gl.is_none() || self.prev.asked != path {
                continue;
            }
            unsafe {
                let gl = self.gl.as_ref().unwrap();
                let tex = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&px)));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                if let Some((_, old)) = self.prev.photo.replace((path, tex)) {
                    gl.delete_texture(old);
                }
            }
            // fade in from what was shown (black meanwhile would flash)
            self.prev.fade = Some(Instant::now());
            for s in &mut self.surfs {
                if s.kind == Kind::Preview {
                    s.dirty = true;
                }
            }
        }
        if let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Preview && s.egl_surf.is_some()) {
            let live = self.prev.key.strip_prefix("live:").and_then(|id| presets::get(&self.presets, id)).cloned();
            if let Some(p) = &live {
                let (_, _, wind_k) = self.uniforms_of(p, 1.0, 1.0, 0.0);
                self.prev.t += (p["speed"].as_f64().unwrap_or(1.0) as f32 * presets::tempo(&self.tempo) * wind_k) as f64 * dt;
            }
            let fading = self.prev_fading();
            if !fading && self.prev.fade.is_some() {
                self.prev.fade = None;
            }
            // the gallery is open: see the next pick within a frame (preview.json comes between ticks)
            next = next.min(1.0 / 30.0);
            if live.is_some() || fading || self.surfs[i].dirty || self.prev.key != self.prev.shown {
                let due = if fading { 1.0 / 60.0 } else { 1.0 / 30.0 };
                let left = due - self.surfs[i].last.elapsed().as_secs_f32();
                if left <= 0.002 || self.surfs[i].dirty {
                    self.draw(i);
                    next = next.min(due);
                } else {
                    next = next.min(left);
                }
            }
        }
        // the weather sky: 30 fps, 5 behind windows, stopped under a fullscreen window
        if let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Sky && s.egl_surf.is_some()) {
            if (now_ms() / 60000.0) as i64 != self.sky.model_at {
                self.sky_model();
            }
            let fs = *self.fs.get(PRIMARY).unwrap_or(&false);
            let sdt = self.sky.last.elapsed().as_secs_f64().min(0.25);
            if !fs || self.surfs[i].dirty {
                let due = if self.sky.hidden { 0.2 } else { 1.0 / 30.0 };
                let left = due - self.surfs[i].last.elapsed().as_secs_f32();
                if left <= 0.002 || self.surfs[i].dirty {
                    self.sky.last = Instant::now();
                    if !fs {
                        self.sky.off_x = (self.sky.off_x + self.sky.model.cloud_speed * sdt) % 1e6;
                        self.sky.evo += (0.03 + self.sky.model.wind * 0.001) * sdt;
                    }
                    self.draw(i);
                    next = next.min(due);
                } else {
                    next = next.min(left);
                }
            } else {
                self.sky.last = Instant::now();
            }
        }
        Duration::from_secs_f32(next.max(0.004))
    }

    // wallpaper.json changed
    fn load_cfg(&mut self) {
        let c = read_json(&format!("{}/.config/mist/wallpaper.json", home()));
        let live = c["live"].as_str().unwrap_or("").to_string();
        let def = format!("{}/Obrazy/Tapety/glow-ember.png", home());
        self.image = c["image"].as_str().filter(|s| !s.is_empty()).unwrap_or(&def).to_string();
        self.tempo = c["tempo"].as_str().unwrap_or("normal").to_string();
        if live != self.live {
            for s in &mut self.surfs {
                s.dirty = true;
            }
        }
        self.live = live;
        self.presets = presets::load();
        self.want_photo();
    }

    // preview.json changed: the gallery opened / closed / shows another wallpaper
    fn load_preview(&mut self) {
        let v = read_json(&format!("{}/preview.json", run_dir()));
        let (screen, key) = (v["screen"].as_str().unwrap_or("").to_string(), v["key"].as_str().unwrap_or("").to_string());
        let opened = !screen.is_empty() && self.prev.screen.is_empty();
        if screen != self.prev.screen {
            self.drop_preview();
            self.prev.screen = screen.clone();
        }
        self.gallery = (!screen.is_empty(), screen.clone());
        self.prev.key = key;
        if screen.is_empty() {
            return;
        }
        if opened {
            self.presets = presets::load();
            self.thumbs();
        }
        if !self.surfs.iter().any(|s| s.kind == Kind::Preview) {
            let Some(o) = self.outputs.outputs().find(|o| self.outputs.info(o).and_then(|i| i.name.clone()).as_deref() == Some(screen.as_str())) else { return };
            let surface = self.compositor.create_surface(&self.qh);
            let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Top, Some("mist-wallpreview"), Some(&o));
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            if let Ok(r) = Region::new(&self.compositor) {
                layer.wl_surface().set_input_region(Some(r.wl_region()));
            }
            layer.commit();
            self.surfs.push(Surf { kind: Kind::Preview, output: o, name: screen, layer, w: 0, h: 0, egl_win: None, egl_surf: None, last: Instant::now(), dirty: true });
        }
        for s in &mut self.surfs {
            if s.kind == Kind::Preview {
                s.dirty = true;
            }
        }
    }
    fn drop_preview(&mut self) {
        if let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Preview) {
            self.drop_surf(i);
        }
        if self.gl.is_some() {
            let _ = self.egl.make_current(self.display, None, None, Some(self.ctx));
            let fbs: Vec<(glow::Framebuffer, glow::Texture)> = self.prev.fbo.iter_mut().filter_map(|f| f.take()).collect();
            unsafe {
                for (fb, tex) in fbs {
                    self.gl().delete_framebuffer(fb);
                    self.gl().delete_texture(tex);
                }
                if let Some((_, t)) = self.prev.photo.take() {
                    self.gl().delete_texture(t);
                }
            }
        }
        self.prev.shown.clear();
        self.prev.asked.clear();
        self.prev.fade = None;
        self.prev.size = (0, 0);
        // the decoded 4K photos (33 MB each, in the decode threads' arenas) back to the OS
        unsafe { libc::malloc_trim(0) };
    }
    fn prev_fading(&self) -> bool {
        self.prev.fade.map(|f| f.elapsed().as_secs_f32() * 1000.0 < PREV_FADE_MS + 40.0).unwrap_or(false)
    }
    fn preview_scene(&mut self, w: i32, h: i32) {
        let gl = self.gl.as_ref().unwrap();
        unsafe {
            if self.prev.size != (w, h) {
                for f in self.prev.fbo.iter_mut() {
                    if let Some((fb, tex)) = f.take() {
                        gl.delete_framebuffer(fb);
                        gl.delete_texture(tex);
                    }
                    let tex = gl.create_texture().unwrap();
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                    let fb = gl.create_framebuffer().unwrap();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    *f = Some((fb, tex));
                }
                self.prev.size = (w, h);
                self.prev.shown.clear();
            }
        }
        // another wallpaper: freeze the one shown, fade to the new one
        if self.prev.key != self.prev.shown {
            if !self.prev.shown.is_empty() {
                self.prev.cur = 1 - self.prev.cur;
                self.prev.fade = Some(Instant::now());
            }
            self.prev.shown = self.prev.key.clone();
        }
        let key = self.prev.shown.clone();
        let (cur_fb, cur_tex) = self.prev.fbo[self.prev.cur].unwrap();
        let old_tex = self.prev.fbo[1 - self.prev.cur].unwrap().1;
        unsafe {
            self.gl().bind_framebuffer(glow::FRAMEBUFFER, Some(cur_fb));
            self.gl().viewport(0, 0, w, h);
        }
        if let Some(id) = key.strip_prefix("live:") {
            let p = presets::get(&self.presets, id).cloned().unwrap_or(Value::Null);
            let (shader, u, _) = self.uniforms_of(&p, w as f32, h as f32, self.prev.t);
            self.run(&shader, &u, &[], true);
        } else if let Some(path) = key.strip_prefix("photo:") {
            match self.prev.photo.as_ref().filter(|p| p.0 == path).map(|p| p.1) {
                Some(t) => self.run("photo", &[("mixk", vec![1.0])], &[("a", t), ("b", t)], true),
                None => {
                    if self.prev.asked != path {
                        self.prev.asked = path.to_string();
                        let (tx, path) = (self.prev_tx.clone(), path.to_string());
                        let _ = std::thread::Builder::new().name("prevphoto".into()).spawn(move || {
                            let px = decode_photo(&path, w, h);
                            let _ = tx.send((path, w, h, px));
                        });
                    }
                    unsafe {
                        self.gl().clear_color(0.0, 0.0, 0.0, 1.0);
                        self.gl().clear(glow::COLOR_BUFFER_BIT);
                    }
                }
            }
        }
        let k = self.prev.fade.map(|f| (f.elapsed().as_secs_f32() * 1000.0 / PREV_FADE_MS).min(1.0)).unwrap_or(1.0);
        let e = if k < 0.5 { 2.0 * k * k } else { 1.0 - (-2.0 * k + 2.0).powi(2) / 2.0 }; // InOutQuad
        unsafe {
            self.gl().bind_framebuffer(glow::FRAMEBUFFER, self.target_fb);
            self.gl().viewport(0, 0, w, h);
        }
        self.run("photo", &[("mixk", vec![e])], &[("a", old_tex), ("b", cur_tex)], false);
        unsafe {
            self.gl().enable(glow::BLEND);
            self.gl().blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        }
        self.run("scrim", &[], &[], false);
        unsafe { self.gl().disable(glow::BLEND) };
    }

    // the gallery's still thumbnails (presets::thumb_path): live scenes rendered here, photos decoded in a
    // thread; only the missing / outdated ones (scenes that follow the sun: older than 10 min)
    fn thumbs(&mut self) {
        let dir = presets::thumb_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mtime = |p: &str| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let src_m = mtime(&presets::path());
        let (tw, th) = (presets::THUMB_W, presets::THUMB_H);
        let fresh = |thumb: &str, src: Option<std::time::SystemTime>, max_age: Option<u64>| -> bool {
            let Some(t) = mtime(thumb) else { return false };
            if src.map(|s| s > t).unwrap_or(false) {
                return false;
            }
            max_age.map(|a| t.elapsed().map(|e| e.as_secs() < a).unwrap_or(false)).unwrap_or(true)
        };
        // one worker, one job at a time (a thread per thumbnail left ~130 MB in glibc arenas): rendered
        // scenes to encode, photos to decode (4K RGBA = 33 MB each) + encode
        enum Job {
            Save(String, Vec<u8>),
            Photo(String, String),
        }
        let mut jobs: Vec<Job> = Vec::new();
        let mut save = |path: String, px: Vec<u8>| jobs.push(Job::Save(path, px));
        let live: Vec<Value> = self.presets.iter().filter(|p| p.get("mode").is_some()).filter(|p| {
            let path = presets::thumb_path(&format!("live:{}", p["id"].as_str().unwrap_or("")));
            !fresh(&path, src_m, if p["auto"].as_bool().unwrap_or(false) || p["weather"].as_bool().unwrap_or(false) { Some(600) } else { None })
        }).cloned().collect();
        if !live.is_empty() && self.gl.is_some() && self.egl.make_current(self.display, None, None, Some(self.ctx)).is_ok() {
            unsafe {
                let gl = self.gl.as_ref().unwrap();
                let fb = gl.create_framebuffer().unwrap();
                let rb = gl.create_renderbuffer().unwrap();
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
                gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, tw, th);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rb));
                let vao = self.vao;
                for p in &live {
                    let gl = self.gl.as_ref().unwrap();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                    gl.viewport(0, 0, tw, th);
                    gl.bind_vertex_array(vao);
                    gl.disable(glow::BLEND);
                    // a moment into the scene (t = 0 is often still empty)
                    let (shader, u, _) = self.uniforms_of(p, tw as f32, th as f32, 40.0);
                    self.run(&shader, &u, &[], false);
                    let mut px = vec![0u8; (tw * th * 4) as usize];
                    self.gl().read_pixels(0, 0, tw, th, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
                    // GL rows go bottom-up; opaque
                    let mut out = Vec::with_capacity(px.len());
                    for y in (0..th).rev() {
                        for q in px[(y * tw * 4) as usize..((y + 1) * tw * 4) as usize].chunks_exact(4) {
                            out.extend_from_slice(&[q[0], q[1], q[2], 255]);
                        }
                    }
                    save(presets::thumb_path(&format!("live:{}", p["id"].as_str().unwrap_or(""))), out);
                }
                let gl = self.gl.as_ref().unwrap();
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fb);
                gl.delete_renderbuffer(rb);
            }
        }
        for f in presets::photos() {
            let path = presets::thumb_path(&format!("photo:{f}"));
            if fresh(&path, mtime(&f), None) {
                continue;
            }
            jobs.push(Job::Photo(f, path));
        }
        if jobs.is_empty() {
            return;
        }
        let _ = std::thread::Builder::new().name("thumbs".into()).spawn(move || {
            for j in jobs {
                let (path, px) = match j {
                    Job::Save(path, px) => (path, px),
                    Job::Photo(src, path) => (path, decode_photo(&src, tw, th)),
                };
                let tmp = format!("{path}.tmp");
                if image::save_buffer_with_format(&tmp, &px, tw as u32, th as u32, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
            unsafe { libc::malloc_trim(0) };
        });
    }

    // decode the photo for every monitor size that lacks it (threads)
    fn want_photo(&mut self) {
        if !self.live.is_empty() {
            return; // decoded when needed
        }
        if self.photo.path != self.image {
            self.photo.path = self.image.clone();
            // keep the shown textures as the fade's start
            let cur: Vec<((i32, i32), glow::Texture)> = self.photo.tex.drain().collect();
            for (k, t) in cur {
                if let Some(o) = self.photo.old.insert(k, t) {
                    if self.gl.is_some() {
                        unsafe { self.gl().delete_texture(o) };
                    }
                }
            }
        }
        let sizes: Vec<(i32, i32)> = self.surfs.iter().filter(|s| s.kind == Kind::Wall && s.w > 1).map(|s| (s.w, s.h)).collect();
        for (w, h) in sizes {
            if self.photo.tex.contains_key(&(w, h)) || self.photo.asked.iter().any(|a| a.0 == self.photo.path && a.1 == w && a.2 == h) {
                continue;
            }
            self.photo.asked.push((self.photo.path.clone(), w, h));
            let (tx, path) = (self.photo_tx.clone(), self.photo.path.clone());
            let _ = std::thread::Builder::new().name("photo".into()).spawn(move || {
                let px = decode_photo(&path, w, h);
                let _ = tx.send((path, w, h, px));
            });
        }
    }
}

fn coverage() -> Vec<Mon> {
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
    let mut out = Vec::new();
    for m in mons.as_array().cloned().unwrap_or_default() {
        let ws = m["activeWorkspace"]["name"].as_str().unwrap_or("").to_string();
        let sp = m["specialWorkspace"]["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("-").to_string();
        let r = m["reserved"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or(vec![0.0; 4]);
        let mut mon = Mon {
            name: m["name"].as_str().unwrap_or("").to_string(),
            x: m["x"].as_f64().unwrap_or(0.0),
            y: m["y"].as_f64().unwrap_or(0.0),
            w: m["width"].as_f64().unwrap_or(1.0),
            h: m["height"].as_f64().unwrap_or(1.0),
            reserved: [r.first().copied().unwrap_or(0.0), r.get(1).copied().unwrap_or(0.0), r.get(2).copied().unwrap_or(0.0), r.get(3).copied().unwrap_or(0.0)],
            ..Default::default()
        };
        for c in clients.as_array().cloned().unwrap_or_default() {
            let wn = c["workspace"]["name"].as_str().unwrap_or("");
            if c["hidden"].as_bool().unwrap_or(false) || c["mapped"].as_bool() == Some(false) || (wn != ws && wn != sp) {
                continue;
            }
            // a fullscreen window on the workspace or on a special workspace shown over it
            if c["fullscreen"].as_i64().unwrap_or(0) > 0 {
                mon.fs = true;
            }
            let at = c["at"].as_array().map(|a| (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            let sz = c["size"].as_array().map(|a| (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            mon.rects.push((at.0, at.1, sz.0, sz.1));
        }
        out.push(mon);
    }
    out
}

enum Ev {
    Config,
    Weather,
    Preview,
    Sky,
    Cover(Vec<Mon>),
    Level(f32),
}

fn read_sky() -> Option<(i32, i32, i32, i32)> {
    let v = read_json(&format!("{}/sky.json", run_dir()));
    if !v["show"].as_bool().unwrap_or(false) {
        return None;
    }
    Some((v["x"].as_i64()? as i32, v["y"].as_i64()? as i32, v["w"].as_i64()? as i32, v["h"].as_i64()? as i32))
}

fn main() {
    // decoded photos (4K RGBA = 33 MB) always mmapped and returned on free: glibc's dynamic threshold would
    // rise to them and keep them on the heap (+46 MB per photo previewed in the gallery)
    unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 1 << 20) };
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
    let (photo_tx, photo_rx) = std::sync::mpsc::channel();
    let (prev_tx, prev_rx) = std::sync::mpsc::channel();
    let sm = |d: f32, s: bool| Smooth::new(0.0, d, s);
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
        presets: Vec::new(),
        live: String::new(),
        image: String::new(),
        tempo: "normal".into(),
        weather: read_json(&format!("{}/.cache/mist/weather.json", home())),
        // a wall-clock start keeps all monitors in step
        t: (now_ms() / 1000.0) % 10000.0,
        t_last: Instant::now(),
        energy: 0.0,
        energy_to: 0.0,
        gallery: (false, String::new()),
        prev: Prev { screen: String::new(), key: String::new(), shown: String::new(), fbo: [None, None], size: (0, 0), cur: 0, fade: None, photo: None, asked: String::new(), t: 0.0 },
        prev_tx,
        prev_rx,
        hidden: HashMap::new(),
        fs: HashMap::new(),
        mons: Vec::new(),
        photo: Photo { path: String::new(), tex: HashMap::new(), old: HashMap::new(), fade: None, asked: Vec::new() },
        photo_tx,
        photo_rx,
        sky: SkyState {
            rect: read_sky(),
            model: sky::Model::default(),
            model_at: 0,
            off_x: 0.0,
            evo: 0.0,
            ct: 0.0,
            c_off: 0.0,
            c_evo: 0.0,
            tick: 0,
            last: Instant::now(),
            last_bolt: [0.0; 4],
            sm: [sm(4000.0, true), sm(3000.0, false), sm(3000.0, false), sm(4000.0, false), sm(4000.0, false), sm(3000.0, false), sm(5000.0, true), sm(4000.0, true), sm(3000.0, false), sm(4000.0, false)],
            fbo: None,
            hidden: false,
        },
        target_fb: None,
    };
    app.load_cfg();
    app.sky_model();
    // start without gliding from 0
    for s in app.sky.sm.iter_mut() {
        s.from = s.to;
    }

    // `mist-wall --png out.ppm <preset | photo | sky> [w h]`: one frame offscreen (surfaceless + FBO), for checks
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--png").unwrap_or(false) {
        let what = args.get(3).cloned().unwrap_or_default();
        let w: i32 = args.get(4).and_then(|x| x.parse().ok()).unwrap_or(1920);
        let h: i32 = args.get(5).and_then(|x| x.parse().ok()).unwrap_or(1080);
        app.egl.make_current(app.display, None, None, Some(app.ctx)).expect("surfaceless");
        init_gl(&mut app);
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            let fb = app.gl().create_framebuffer().unwrap();
            let rb = app.gl().create_renderbuffer().unwrap();
            app.gl().bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            app.gl().renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
            app.gl().bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            app.gl().framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rb));
            app.target_fb = Some(fb);
            let bind = |app: &App| {
                app.gl().bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                app.gl().viewport(0, 0, w, h);
            };
            if what == "thumbs" {
                // the gallery's thumbnails into ~/.cache/mist/thumbs (the threads save them)
                app.thumbs();
                std::thread::sleep(Duration::from_millis(1500));
                return;
            }
            if let Some(k) = what.strip_prefix("preview:") {
                // the gallery's preview of a live scene (FBO, cross-fade, scrim)
                app.prev.key = k.to_string();
                bind(&app);
                app.preview_scene(w, h);
            } else if what == "sky" {
                bind(&app);
                app.sky_scene(w, h);
            } else {
                if what == "photo" {
                    app.live.clear();
                    let p = decode_photo(&app.image, w, h);
                    let tex = app.gl().create_texture().unwrap();
                    app.gl().bind_texture(glow::TEXTURE_2D, Some(tex));
                    app.gl().tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&p)));
                    app.gl().tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    app.photo.tex.insert((w, h), tex);
                } else if !what.is_empty() {
                    app.live = what.clone();
                }
                bind(&app);
                app.wall_scene(w, h, true);
            }
            app.gl().read_pixels(0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
        }
        // GL rows go bottom-up; un-premultiply nothing (alpha shown over black)
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in (0..h).rev() {
            for p in px[(y * w * 4) as usize..((y + 1) * w * 4) as usize].chunks_exact(4) {
                out.extend_from_slice(&p[..3]);
            }
        }
        std::fs::write(&args[2], out).unwrap();
        return;
    }

    // one instance (mist-ui restarts spawn it again): exit 3 = already running
    let lock = format!("{}/wall.lock", run_dir());
    let _ = std::fs::create_dir_all(run_dir());
    let lf = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&lock).expect("lock file");
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lf), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        std::process::exit(3);
    }

    let mut ev: calloop::EventLoop<App> = calloop::EventLoop::try_new().unwrap();
    let lh = ev.handle();
    calloop_wayland_source::WaylandSource::new(conn.clone(), queue).insert(lh.clone()).unwrap();
    let (tx, rx) = calloop::channel::channel::<Ev>();
    // config / weather / popup / sky files (inotify)
    {
        let tx = tx.clone();
        std::thread::spawn(move || unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC);
            let dirs = [format!("{}/.config/mist", home()), format!("{}/.cache/mist", home()), run_dir()];
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
                    let m = match name.as_str() { "wallpaper.json" => Some(Ev::Config), "weather.json" => Some(Ev::Weather), "preview.json" => Some(Ev::Preview), "sky.json" => Some(Ev::Sky), _ => None };
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
            if tx1.send(Ev::Cover(coverage())).is_err() {
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
                    if tx2.send(Ev::Cover(coverage())).is_err() {
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
            Ev::Config => app.load_cfg(),
            Ev::Weather => {
                app.weather = read_json(&format!("{}/.cache/mist/weather.json", home()));
                app.sky_model();
            }
            Ev::Preview => app.load_preview(),
            Ev::Sky => {
                let r = read_sky();
                if r != app.sky.rect {
                    app.sky.rect = r;
                    app.place_sky(false);
                }
            }
            Ev::Cover(m) => app.on_cover(m),
            Ev::Level(l) => app.energy_to = l,
        }
    })
    .unwrap();
    lh.insert_source(calloop::timer::Timer::from_duration(Duration::from_millis(33)), |_, _, app: &mut App| {
        let next = if app.gl.is_some() { app.tick() } else { Duration::from_millis(100) };
        calloop::timer::TimeoutAction::ToDuration(next)
    })
    .unwrap();
    loop {
        ev.dispatch(None, &mut app).unwrap();
        let _ = conn.flush();
    }
}

fn init_gl(app: &mut App) {
    let gl = unsafe { glow::Context::from_loader_function(|n| app.egl.get_proc_address(n).map(|p| p as *const _).unwrap_or(std::ptr::null())) };
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
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        app.vao = vao;
    }
    app.gl = Some(gl);
}

// parec on the default sink monitor -> RMS with automatic gain -> slow envelope (0..1), only while a live
// scene shows on the main monitor, music is on and a player plays
fn music_loop(tx: calloop::channel::Sender<Ev>) {
    use std::io::Read;
    loop {
        let cfg = read_json(&format!("{}/.config/mist/wallpaper.json", home()));
        let on = cfg["music"].as_bool().unwrap_or(false) && !cfg["live"].as_str().unwrap_or("").is_empty() && SHOWN.load(Ordering::Relaxed);
        let playing = on && std::process::Command::new("sh").args(["-c", "for p in $(busctl --user list --no-legend | awk '{print $1}' | grep '^org.mpris.MediaPlayer2'); do busctl --user get-property $p /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlaybackStatus; done"]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains("Playing")).unwrap_or(false);
        if !playing {
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
            if start.elapsed().as_secs() >= 10 || !SHOWN.load(Ordering::Relaxed) {
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
        while let Some(i) = self.surfs.iter().position(|s| s.output == o) {
            self.drop_surf(i);
        }
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface) {
        if let Some(i) = self.surfs.iter().position(|s| &s.layer == l) {
            self.drop_surf(i);
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        let Some(i) = self.surfs.iter().position(|s| &s.layer == l) else { return };
        let (w, h) = (c.new_size.0.max(1) as i32, c.new_size.1.max(1) as i32);
        let s = &mut self.surfs[i];
        s.w = w;
        s.h = h;
        s.dirty = true;
        if let Some(win) = &s.egl_win {
            win.resize(w, h, 0, 0);
        } else {
            let Ok(win) = wayland_egl::WlEglSurface::new(s.layer.wl_surface().id(), w, h) else { return };
            let es = unsafe { self.egl.create_window_surface(self.display, self.config, win.ptr() as khronos_egl::NativeWindowType, None) };
            s.egl_surf = es.ok();
            s.egl_win = Some(win);
        }
        if self.make_current(i) {
            let _ = self.egl.swap_interval(self.display, 0);
            if self.gl.is_none() {
                init_gl(self);
            }
            self.want_photo();
            // a photo not decoded yet: black until it is (no flash of the old one)
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
