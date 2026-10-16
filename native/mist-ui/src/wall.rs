// The wallpaper (was LiveWallpaper.qml), ~/.config/mist/wallpaper.json. mist-wall (the only Mist process
// with a GL context) draws it, the animated scene or the photo (a texture: no shm copy here), and the
// desktop weather sky; this starts it and restarts it. Only when it keeps crashing does mist-ui draw the
// photo itself, on the CPU once per monitor (cropped to the monitor, the desktop vignette baked in on the
// main one, cross-faded 600 ms when it changes; only the shm buffer stays).
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm},
};
use std::process::Child;
use std::sync::mpsc;
use std::time::Instant;
use wayland_client::{protocol::{wl_output, wl_shm}, QueueHandle};

const FADE_MS: f32 = 600.0;

pub struct Surf {
    pub output: wl_output::WlOutput,
    pub name: String,
    pub layer: LayerSurface,
    w: u32,
    h: u32,
    configured: bool,
    // BGRA pixels while a photo is being shown / faded (dropped after)
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
    fade: Option<Instant>,
    drawn: String,
    buf: Option<smithay_client_toolkit::shm::slot::Buffer>,
}

pub struct Wall {
    pub surfs: Vec<Surf>,
    pool: Option<SlotPool>,
    pub live: String,
    pub image: String,
    child: Option<Child>,
    started: Option<Instant>,
    crashes: Vec<Instant>,
    given_up: bool,
    tx: mpsc::Sender<(String, String, Vec<u8>)>,
    rx: mpsc::Receiver<(String, String, Vec<u8>)>,
    pending: usize,
    pub off: bool,
    checked: Instant,
}

fn bin() -> String {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("mist-wall-bin"))).filter(|p| p.exists()).map(|p| p.to_string_lossy().to_string()).unwrap_or("mist-wall".into())
}

// the photo cropped to w x h (PreserveAspectCrop), vignette on the main monitor, as BGRA
pub fn render(path: &str, w: u32, h: u32, vignette: bool) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    for px in out.chunks_exact_mut(4) {
        px[3] = 255; // black under a missing photo (the window colour)
    }
    if let Some(im) = image::ImageReader::open(path).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.decode().ok()) {
        let im = im.resize_to_fill(w, h, image::imageops::FilterType::Triangle).to_rgb8();
        for (o, i) in out.chunks_exact_mut(4).zip(im.pixels()) {
            o[0] = i[2];
            o[1] = i[1];
            o[2] = i[0];
        }
    }
    if vignette {
        // shaders/vignette.frag: radial + top 90 px / bottom 140 px bands of rgb(4, 7, 16), source-over
        let (wf, hf) = (w as f32, h as f32);
        let (r0, r1) = (hf * 0.35, (wf * wf + hf * hf).sqrt() * 0.58);
        let col = [16.0f32, 7.0, 4.0]; // BGR
        for y in 0..h {
            let py = y as f32 + 0.5;
            let at = if py < 90.0 { 0.22 * (1.0 - py / 90.0) } else { 0.0 };
            let ab = if py > hf - 140.0 { 0.22 * (py - (hf - 140.0)) / 140.0 } else { 0.0 };
            let dy = py - hf * 0.46;
            for x in 0..w {
                let px = x as f32 + 0.5;
                let dx = px - wf * 0.5;
                let d = (dx * dx + dy * dy).sqrt();
                let mut a = 0.38 * ((d - r0) / (r1 - r0)).clamp(0.0, 1.0);
                a += at * (1.0 - a);
                a += ab * (1.0 - a);
                if a <= 0.0 {
                    continue;
                }
                // the shader's sub-LSB dither
                let hsh = {
                    let (mut q0, mut q1, mut q2) = ((px * 0.1031).fract(), (py * 0.1031).fract(), (px * 0.1031).fract());
                    let dd = q0 * (q1 + 33.33) + q1 * (q2 + 33.33) + q2 * (q0 + 33.33);
                    q0 += dd;
                    q1 += dd;
                    q2 += dd;
                    ((q0 + q1) * q2).fract()
                };
                let a = (a + (hsh - 0.5) / 255.0).clamp(0.0, 1.0);
                let i = ((y * w + x) * 4) as usize;
                for c in 0..3 {
                    out[i + c] = (out[i + c] as f32 * (1.0 - a) + col[c] * a + 0.5) as u8;
                }
            }
        }
    }
    out
}

impl Wall {
    pub fn new() -> Wall {
        let (tx, rx) = mpsc::channel();
        let (live, image) = Wall::read_cfg();
        Wall { surfs: Vec::new(), pool: None, live, image, child: None, started: None, crashes: Vec::new(), given_up: false, tx, rx, pending: 0, off: false, checked: Instant::now() }
    }

    pub fn read_cfg() -> (String, String) {
        let c: serde_json::Value = std::fs::read_to_string(format!("{}/.config/mist/wallpaper.json", std::env::var("HOME").unwrap_or_default())).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let def = format!("{}/glow-ember.png", crate::paths::wallpaper_dir());
        (c["live"].as_str().unwrap_or("").to_string(), c["image"].as_str().filter(|s| !s.is_empty()).unwrap_or(&def).to_string())
    }

    // the CPU photo is needed (mist-wall gave up)
    pub fn photo(&self) -> bool {
        self.given_up
    }
    // nobody holds mist-wall's lock (no instance runs, e.g. ours died while mist-ui restarted)
    fn lock_free() -> bool {
        let p = format!("{}/mist/wall.lock", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
        let Ok(f) = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(p) else { return true };
        let fd = std::os::fd::AsRawFd::as_raw_fd(&f);
        if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            unsafe { libc::flock(fd, libc::LOCK_UN) };
            return true;
        }
        false
    }

    pub fn add_output(&mut self, o: &wl_output::WlOutput, name: &str, comp: &CompositorState, ls: &LayerShell, qh: &QueueHandle<crate::app::App>) {
        if self.off || !self.photo() || self.surfs.iter().any(|s| s.name == name) {
            return;
        }
        let surface = comp.create_surface(qh);
        let layer = ls.create_layer_surface(qh, surface, Layer::Background, Some("mist-live"), Some(o));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        if let Ok(r) = Region::new(comp) {
            layer.wl_surface().set_input_region(Some(r.wl_region()));
        }
        layer.commit();
        self.surfs.push(Surf { output: o.clone(), name: name.to_string(), layer, w: 0, h: 0, configured: false, old: None, new: None, fade: None, drawn: String::new(), buf: None });
    }

    pub fn configure(&mut self, l: &LayerSurface, w: u32, h: u32) -> bool {
        let Some(s) = self.surfs.iter_mut().find(|s| &s.layer == l) else { return false };
        if s.w != w || s.h != h {
            s.drawn.clear();
        }
        s.w = w.max(1);
        s.h = h.max(1);
        s.configured = true;
        self.request();
        true
    }

    // decode the photo for every monitor that does not show it yet (a thread; the result comes in tick)
    fn request(&mut self) {
        for s in &mut self.surfs {
            if !s.configured || s.drawn == self.image {
                continue;
            }
            s.drawn = self.image.clone();
            let (tx, path, name, w, h, vig) = (self.tx.clone(), self.image.clone(), s.name.clone(), s.w, s.h, s.name == crate::paths::primary_monitor());
            self.pending += 1;
            let _ = std::thread::Builder::new().name("wall".into()).spawn(move || {
                let px = render(&path, w, h, vig);
                let _ = tx.send((name, path, px));
            });
        }
    }

    pub fn busy(&self) -> bool {
        self.pending > 0 || self.surfs.iter().any(|s| s.fade.is_some())
    }

    // config changed (wallpaper.json)
    pub fn set(&mut self, live: String, image: String, comp: &CompositorState, ls: &LayerShell, qh: &QueueHandle<crate::app::App>, outs: &[(wl_output::WlOutput, String)]) {
        let was_photo = self.photo();
        if live != self.live && self.given_up {
            // another choice: give mist-wall another chance
            self.given_up = false;
            self.crashes.clear();
        }
        self.live = live;
        self.image = image;
        if self.off {
            return;
        }
        if self.photo() {
            if !was_photo || self.surfs.is_empty() {
                for (o, n) in outs {
                    self.add_output(o, n, comp, ls, qh);
                }
            }
            self.request();
        } else {
            self.spawn();
        }
    }

    fn spawn(&mut self) {
        if self.child.is_some() {
            return;
        }
        match std::process::Command::new(bin()).stdin(std::process::Stdio::null()).spawn() {
            Ok(c) => {
                self.child = Some(c);
                self.started = Some(Instant::now());
            }
            Err(e) => {
                eprintln!("wall: cannot start mist-wall: {e}");
                self.given_up = true;
            }
        }
    }

    // every 250 ms: mist-wall alive? photo surfaces still needed?
    pub fn slow_tick(&mut self, comp: &CompositorState, ls: &LayerShell, qh: &QueueHandle<crate::app::App>, outs: &[(wl_output::WlOutput, String)]) {
        if self.off {
            return;
        }
        if let Some(c) = &mut self.child {
            if let Ok(Some(st)) = c.try_wait() {
                self.child = None;
                // 3: another mist-wall already runs (mist-ui restarted): watched through its lock below
                if !self.photo() && st.code() != Some(3) {
                    self.crashes.retain(|t| t.elapsed().as_secs() < 120);
                    self.crashes.push(Instant::now());
                    if self.crashes.len() >= 5 {
                        eprintln!("wall: mist-wall keeps crashing, showing the photo");
                        self.given_up = true;
                        for (o, n) in outs {
                            self.add_output(o, n, comp, ls, qh);
                        }
                    } else {
                        self.spawn();
                    }
                }
            }
        }
        if self.child.is_none() && !self.photo() && self.checked.elapsed().as_secs() >= 5 {
            self.checked = Instant::now();
            if Self::lock_free() {
                self.spawn();
            }
        }
        // the scene is up: the photo surfaces (and their buffers) can go
        if !self.photo() && !self.surfs.is_empty() && self.started.map(|t| t.elapsed().as_millis() > 1500).unwrap_or(true) {
            self.surfs.clear();
            self.pool = None;
        }
    }

    pub fn remove_output(&mut self, o: &wl_output::WlOutput) {
        self.surfs.retain(|s| &s.output != o);
    }
    pub fn closed(&mut self, l: &LayerSurface) -> bool {
        let n = self.surfs.len();
        self.surfs.retain(|s| &s.layer != l);
        n != self.surfs.len()
    }

    // animation frames: decoded photos arrive, fades advance
    pub fn tick(&mut self, shm: &Shm) {
        while let Ok((name, path, px)) = self.rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            if path != self.image {
                continue; // a newer photo was chosen meanwhile
            }
            if let Some(s) = self.surfs.iter_mut().find(|s| s.name == name) {
                if px.len() != (s.w * s.h * 4) as usize {
                    continue;
                }
                // first photo: straight in; later ones: fade from what is shown
                if s.new.is_some() || s.old.is_some() {
                    s.old = s.new.take().or(s.old.take());
                    s.fade = Some(Instant::now());
                }
                s.new = Some(px);
            }
        }
        for s in &mut self.surfs {
            if s.new.is_none() {
                continue;
            }
            let t = s.fade.map(|f| (f.elapsed().as_secs_f32() * 1000.0 / FADE_MS).min(1.0)).unwrap_or(1.0);
            let e = if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }; // InOutQuad
            let (w, h) = (s.w as i32, s.h as i32);
            let pool = self.pool.get_or_insert_with(|| SlotPool::new((w * h * 4) as usize, shm).expect("shm pool"));
            // one buffer per monitor, rewritten in place once the compositor has released it
            if s.buf.as_ref().map(|b| b.height() != h || b.stride() != w * 4).unwrap_or(true) {
                s.buf = pool.create_buffer(w, h, w * 4, wl_shm::Format::Xrgb8888).ok().map(|(b, _)| b);
            }
            let Some(buf) = s.buf.as_ref() else { continue };
            let Some(data) = buf.canvas(pool) else { continue }; // still in use: next frame
            let new = s.new.as_ref().unwrap();
            match (&s.old, t < 1.0) {
                (Some(old), true) => {
                    let k = (e * 256.0) as u32;
                    for ((d, a), b) in data.iter_mut().zip(old.iter()).zip(new.iter()) {
                        *d = ((*a as u32 * (256 - k) + *b as u32 * k) >> 8) as u8;
                    }
                }
                _ => data.copy_from_slice(new),
            }
            let surf = s.layer.wl_surface();
            surf.damage_buffer(0, 0, w, h);
            let _ = buf.attach_to(surf);
            s.layer.commit();
            if t >= 1.0 {
                s.fade = None;
                s.old = None;
                s.new = None; // the buffer keeps the pixels
            }
        }
    }
}
