//! Glass: what is behind a Mist panel (bar, island, dock, popups, spotlight...) blurred, like Hyprland's
//! `layer_rule { blur = true, ignore_alpha = .. }`. The layer's main surface is wrapped: its draw blits the
//! framebuffer under it to half size, runs dual Kawase (4 halvings, then back up, Hyprland's
//! contrast/brightness on the last pass) and paints that where the surface's alpha > ignore_alpha, then the
//! surface itself on top.
//! GPU: the blur runs only when something behind the panel changed (hash of the elements under it); a
//! frame that only damages the panel reuses the cached texture. VRAM: ~1/3 of the panel's area x 4 B per
//! level. CPU: one hash per glass layer per frame.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    hash::{Hash, Hasher},
};

use smithay::{
    backend::renderer::{
        element::{surface::WaylandSurfaceRenderElement, utils::CropRenderElement, Element, Id, Kind, RenderElement, UnderlyingStorage},
        gles::{ffi, GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform, UniformName, UniformType},
        utils::{CommitCounter, DamageSet, OpaqueRegions},
        Frame,
    },
    utils::{Buffer as BufferCoords, Physical, Point, Rectangle, Scale, Transform},
};

/// Layer namespaces with glass and their ignore_alpha (windowrules.lua).
pub fn rule(namespace: &str) -> Option<f32> {
    thread_local!(static OFF: bool = std::env::var("MIST_COMP_BLUR").is_ok_and(|v| v == "0"));
    if OFF.with(|o| *o) {
        return None;
    }
    let ns = namespace.strip_prefix("mist-")?;
    match ns {
        "desktop" | "bar" | "island" | "dock" | "start" | "popup" | "spotlight" | "switcher" | "session" => Some(0.25),
        "snap" | "snap-assist" => Some(0.1),
        "gallery" => Some(0.45),
        _ => None,
    }
}

/// Windows with glass (Hyprland's decoration:blur): apps whose Mist theme is translucent and that draw
/// their own corners. Blurred under every pixel with alpha > 0.02 (Firefox leaves a trace of alpha in its
/// transparent corners) inside the window geometry (not under the
/// client-side shadow around it).
pub fn window_rule(app_id: &str) -> bool {
    thread_local!(static OFF: bool = std::env::var("MIST_COMP_BLUR").is_ok_and(|v| v == "0"));
    !OFF.with(|o| *o) && matches!(app_id, "zen" | "zen-browser" | "zen-beta" | "mist-files")
}

const PASSES: usize = 4;
const OFFSET: f32 = 2.0;
const CONTRAST: f32 = 0.8916;
const BRIGHTNESS: f32 = 0.8172;

const MASK: &str = r#"
#version 100
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision mediump float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform sampler2D blurred;
uniform vec2 origin;
uniform vec2 bsize;
uniform float ignore;
uniform float soft;
void main() {
#if defined(NO_ALPHA)
    float a = 1.0;
#else
    float a = texture2D(tex, v_coords).a;
#endif
    if (a <= ignore)
        discard;
    // soft > 0: the glass fades in over that much of the surface's alpha, so antialiased edges (a window's
    // rounded corners) stay smooth instead of getting a hard blurred fringe
    float cov = soft > 0.0 ? clamp((a - ignore) / soft, 0.0, 1.0) : 1.0;
    vec2 uv = (gl_FragCoord.xy - origin) / bsize;
    gl_FragColor = vec4(texture2D(blurred, uv).rgb, 1.0) * alpha * cov;
}
"#;

const VERT: &str = r#"
#version 100
attribute vec2 pos;
varying vec2 uv;
void main() {
    uv = pos;
    gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const DOWN: &str = r#"
#version 100
precision mediump float;
uniform sampler2D t;
uniform vec2 hp;
varying vec2 uv;
void main() {
    vec4 s = texture2D(t, uv) * 4.0;
    s += texture2D(t, uv - hp);
    s += texture2D(t, uv + hp);
    s += texture2D(t, uv + vec2(hp.x, -hp.y));
    s += texture2D(t, uv - vec2(hp.x, -hp.y));
    gl_FragColor = s / 8.0;
}
"#;

const UP: &str = r#"
#version 100
precision mediump float;
uniform sampler2D t;
uniform vec2 hp;
uniform float last;
uniform float contrast;
uniform float brightness;
varying vec2 uv;
void main() {
    vec4 s = texture2D(t, uv + vec2(-hp.x * 2.0, 0.0));
    s += texture2D(t, uv + vec2(-hp.x, hp.y)) * 2.0;
    s += texture2D(t, uv + vec2(0.0, hp.y * 2.0));
    s += texture2D(t, uv + vec2(hp.x, hp.y)) * 2.0;
    s += texture2D(t, uv + vec2(hp.x * 2.0, 0.0));
    s += texture2D(t, uv + vec2(hp.x, -hp.y)) * 2.0;
    s += texture2D(t, uv + vec2(0.0, -hp.y * 2.0));
    s += texture2D(t, uv + vec2(-hp.x, -hp.y)) * 2.0;
    vec3 c = s.rgb / 12.0;
    if (last > 0.5)
        c = ((c - 0.5) * contrast + 0.5) * brightness;
    gl_FragColor = vec4(c, 1.0);
}
"#;

struct Passes {
    down: u32,
    up: u32,
    down_t: i32,
    down_hp: i32,
    up_t: i32,
    up_hp: i32,
    up_last: i32,
    up_contrast: i32,
    up_brightness: i32,
}

/// One glass layer's levels: level 0 = half the region, each next one half again. Level 0 holds the result.
struct Cache {
    region: (i32, i32),
    sizes: Vec<(i32, i32)>,
    tex: Vec<u32>,
    fbo: Vec<u32>,
    behind: Option<u64>,
    /// last drawn: a surface gone (closed panel, hidden workspace) frees its textures a second later
    seen: std::time::Instant,
    /// IPC `blurs`: requests and the ones that ran
    calls: u64,
    runs: u64,
}

/// A glass element's own commit counter: bumped when the blur behind or the client's surface changed. The
/// last few bumps remember the client's commit at the time and the blur version then, so the damage since
/// one of them is the client's own damage (a page scrolling) instead of the whole window.
#[derive(Clone, Default)]
struct Track {
    behind: u64,
    own: CommitCounter,
    out: CommitCounter,
    /// (our counter, the client's commit then, the blur version then)
    hist: Vec<(CommitCounter, CommitCounter, u64)>,
    blur: u64,
    /// the scene build it was last in (closed panels' entries go)
    frame: u64,
}

thread_local! {
    static MASK_PROGRAM: RefCell<Option<GlesTexProgram>> = const { RefCell::new(None) };
    static PASSES_PROGRAM: RefCell<Option<Passes>> = const { RefCell::new(None) };
    static CACHE: RefCell<HashMap<(String, Id), Cache>> = RefCell::new(HashMap::new());
    /// per glass surface: its commit counter and what it stands for
    static STATE: RefCell<HashMap<(String, Id), Track>> = RefCell::new(HashMap::new());
    static FRAME: Cell<u64> = const { Cell::new(0) };
    /// per blur (window or panel): the layout behind it (hash), a version bumped whenever pixels behind it
    /// change, the commit of each element behind it last seen, and the scene build it was last used in
    static BEHIND: RefCell<HashMap<(String, Id), (u64, u64, HashMap<Id, CommitCounter>, u64)>> = RefCell::new(HashMap::new());
}

fn mask_program(r: &mut GlesRenderer) -> Option<GlesTexProgram> {
    MASK_PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            let u = [
                UniformName::new("blurred", UniformType::_1i),
                UniformName::new("origin", UniformType::_2f),
                UniformName::new("bsize", UniformType::_2f),
                UniformName::new("ignore", UniformType::_1f),
                UniformName::new("soft", UniformType::_1f),
            ];
            match r.compile_custom_texture_shader(MASK, &u) {
                Ok(prog) => *p = Some(prog),
                Err(e) => tracing::warn!("glass mask shader: {e:?}"),
            }
        }
        p.clone()
    })
}

unsafe fn shader(gl: &ffi::Gles2, kind: u32, src: &str) -> u32 {
    let s = gl.CreateShader(kind);
    let ptr = src.as_ptr() as *const ffi::types::GLchar;
    let len = src.len() as i32;
    gl.ShaderSource(s, 1, &ptr, &len);
    gl.CompileShader(s);
    let mut ok = 0;
    gl.GetShaderiv(s, ffi::COMPILE_STATUS, &mut ok);
    if ok == 0 {
        tracing::warn!("glass shader compile failed");
    }
    s
}

unsafe fn link(gl: &ffi::Gles2, frag: &str) -> u32 {
    let p = gl.CreateProgram();
    let v = shader(gl, ffi::VERTEX_SHADER, VERT);
    let f = shader(gl, ffi::FRAGMENT_SHADER, frag);
    gl.AttachShader(p, v);
    gl.AttachShader(p, f);
    gl.BindAttribLocation(p, 0, c"pos".as_ptr());
    gl.LinkProgram(p);
    gl.DeleteShader(v);
    gl.DeleteShader(f);
    p
}

fn passes(gl: &ffi::Gles2) -> Option<()> {
    PASSES_PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            unsafe {
                let down = link(gl, DOWN);
                let up = link(gl, UP);
                let mut ok = 0;
                gl.GetProgramiv(down, ffi::LINK_STATUS, &mut ok);
                let mut ok2 = 0;
                gl.GetProgramiv(up, ffi::LINK_STATUS, &mut ok2);
                if ok == 0 || ok2 == 0 {
                    tracing::warn!("glass blur programs failed to link");
                    return None;
                }
                let u = |p: u32, n: &std::ffi::CStr| gl.GetUniformLocation(p, n.as_ptr());
                *p = Some(Passes {
                    down,
                    up,
                    down_t: u(down, c"t"),
                    down_hp: u(down, c"hp"),
                    up_t: u(up, c"t"),
                    up_hp: u(up, c"hp"),
                    up_last: u(up, c"last"),
                    up_contrast: u(up, c"contrast"),
                    up_brightness: u(up, c"brightness"),
                });
            }
        }
        Some(())
    })
}

impl Cache {
    unsafe fn new(gl: &ffi::Gles2, region: (i32, i32)) -> Cache {
        let mut sizes = Vec::new();
        let (mut w, mut h) = region;
        for _ in 0..PASSES {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            sizes.push((w, h));
            if w <= 8 || h <= 8 {
                break;
            }
        }
        let n = sizes.len();
        let mut tex = vec![0u32; n];
        let mut fbo = vec![0u32; n];
        gl.GenTextures(n as i32, tex.as_mut_ptr());
        gl.GenFramebuffers(n as i32, fbo.as_mut_ptr());
        for i in 0..n {
            gl.BindTexture(ffi::TEXTURE_2D, tex[i]);
            gl.TexImage2D(ffi::TEXTURE_2D, 0, ffi::RGBA as i32, sizes[i].0, sizes[i].1, 0, ffi::RGBA, ffi::UNSIGNED_BYTE, std::ptr::null());
            for (k, v) in [(ffi::TEXTURE_MIN_FILTER, ffi::LINEAR), (ffi::TEXTURE_MAG_FILTER, ffi::LINEAR), (ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE), (ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE)] {
                gl.TexParameteri(ffi::TEXTURE_2D, k, v as i32);
            }
            gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo[i]);
            gl.FramebufferTexture2D(ffi::FRAMEBUFFER, ffi::COLOR_ATTACHMENT0, ffi::TEXTURE_2D, tex[i], 0);
        }
        gl.BindTexture(ffi::TEXTURE_2D, 0);
        Cache { region, sizes, tex, fbo, behind: None, seen: std::time::Instant::now(), calls: 0, runs: 0 }
    }

    unsafe fn free(&self, gl: &ffi::Gles2) {
        gl.DeleteTextures(self.tex.len() as i32, self.tex.as_ptr());
        gl.DeleteFramebuffers(self.fbo.len() as i32, self.fbo.as_ptr());
    }
}

/// Blur the framebuffer region (GL window coords) into the cache (unless what is behind is unchanged).
/// Leaves the frame's GL state as Smithay set it. Returns the result texture.
unsafe fn blur(gl: &ffi::Gles2, key: &(String, Id), region: Rectangle<i32, Physical>, behind: u64) -> Option<u32> {
    passes(gl)?;
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        // surfaces gone for a second: free their textures (every panel opening is a new surface, so a frame
        // count let dozens of full-screen blur chains pile up at a low frame rate)
        let now = std::time::Instant::now();
        c.retain(|_, e| {
            let keep = now.duration_since(e.seen) < std::time::Duration::from_secs(1);
            if !keep {
                e.free(gl);
            }
            keep
        });
        let size = (region.size.w, region.size.h);
        if c.get(key).is_some_and(|e| e.region != size) {
            c.remove(key).unwrap().free(gl);
        }
        let mut prev_fbo = 0;
        gl.GetIntegerv(ffi::FRAMEBUFFER_BINDING, &mut prev_fbo);
        let mut vp = [0i32; 4];
        gl.GetIntegerv(ffi::VIEWPORT, vp.as_mut_ptr());
        if !c.contains_key(key) {
            let e = Cache::new(gl, size);
            c.insert(key.clone(), e);
            gl.BindFramebuffer(ffi::FRAMEBUFFER, prev_fbo as u32);
        }
        let e = c.get_mut(key).unwrap();
        e.seen = now;
        let hit = e.behind == Some(behind);
        COUNTS.with(|n| n.set((n.get().0 + 1, n.get().1 + !hit as u64)));
        e.calls += 1;
        e.runs += !hit as u64;
        if hit {
            return Some(e.tex[0]);
        }
        e.behind = Some(behind);
        let p = PASSES_PROGRAM.with(|p| p.borrow().as_ref().map(|p| (p.down, p.up, p.down_t, p.down_hp, p.up_t, p.up_hp, p.up_last, p.up_contrast, p.up_brightness)))?;
        let (down, up, down_t, down_hp, up_t, up_hp, up_last, up_contrast, up_brightness) = p;

        gl.Disable(ffi::SCISSOR_TEST);
        gl.Disable(ffi::BLEND);
        // what is behind, straight to half size
        gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, prev_fbo as u32);
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, e.fbo[0]);
        let r = region;
        gl.BlitFramebuffer(r.loc.x, r.loc.y, r.loc.x + r.size.w, r.loc.y + r.size.h, 0, 0, e.sizes[0].0, e.sizes[0].1, ffi::COLOR_BUFFER_BIT, ffi::LINEAR);

        static QUAD: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
        gl.EnableVertexAttribArray(0);
        if gl.VertexAttribDivisor.is_loaded() {
            gl.VertexAttribDivisor(0, 0);
        }
        gl.VertexAttribPointer(0, 2, ffi::FLOAT, ffi::FALSE, 0, QUAD.as_ptr() as *const _);
        gl.ActiveTexture(ffi::TEXTURE0);
        let n = e.sizes.len();
        let pass = |from: usize, to: usize, prog: u32, t: i32, hp: i32| {
            gl.BindFramebuffer(ffi::FRAMEBUFFER, e.fbo[to]);
            gl.Viewport(0, 0, e.sizes[to].0, e.sizes[to].1);
            gl.UseProgram(prog);
            gl.BindTexture(ffi::TEXTURE_2D, e.tex[from]);
            gl.Uniform1i(t, 0);
            gl.Uniform2f(hp, OFFSET * 0.5 / e.sizes[from].0 as f32, OFFSET * 0.5 / e.sizes[from].1 as f32);
        };
        for i in 1..n {
            pass(i - 1, i, down, down_t, down_hp);
            gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
        }
        for i in (0..n.saturating_sub(1)).rev() {
            pass(i + 1, i, up, up_t, up_hp);
            gl.Uniform1f(up_last, if i == 0 { 1.0 } else { 0.0 });
            gl.Uniform1f(up_contrast, CONTRAST);
            gl.Uniform1f(up_brightness, BRIGHTNESS);
            gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
        }
        gl.DisableVertexAttribArray(0);
        gl.BindTexture(ffi::TEXTURE_2D, 0);
        gl.BindFramebuffer(ffi::FRAMEBUFFER, prev_fbo as u32);
        gl.Viewport(vp[0], vp[1], vp[2], vp[3]);
        gl.Enable(ffi::SCISSOR_TEST);
        gl.Enable(ffi::BLEND);
        gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
        Some(e.tex[0])
    })
}

pub struct Glass {
    /// cropped to the window's geometry for a window's own surfaces (see `new`), else the whole surface
    inner: CropRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    mask: GlesTexProgram,
    key: (String, Id),
    /// the blur cache's key: the window's main surface for every surface of a window (one blur per window,
    /// not one per surface of it: Firefox has 2-3 over the same place), else `key`
    group: Option<(String, Id)>,
    ignore: f32,
    behind: u64,
    commit: CommitCounter,
    /// output-physical rect the glass stays inside (a window's geometry), None: the whole surface
    clip: Option<Rectangle<i32, Physical>>,
    track: Track,
    /// the alpha range the glass fades in over (0: on or off at `ignore`)
    soft: f32,
}

impl Glass {
    #[allow(clippy::result_large_err)] // the surface element comes back unchanged when it cannot be wrapped
    /// `crop`: draw only this part of the surface (a window's geometry: GTK clients such as Firefox draw their
    /// own frame and shadow in a margin around it, which mist-comp's rounded shadow replaces). Ok(None): the
    /// surface lies wholly outside it.
    pub fn new(r: &mut GlesRenderer, inner: WaylandSurfaceRenderElement<GlesRenderer>, output: &str, ignore: f32, crop: Option<Rectangle<i32, Physical>>, scale: Scale<f64>) -> Result<Option<Self>, WaylandSurfaceRenderElement<GlesRenderer>> {
        let Some(mask) = mask_program(r) else { return Err(inner) };
        let key = (output.to_string(), inner.id().clone());
        let rect = crop.unwrap_or_else(|| inner.geometry(scale));
        let Some(inner) = CropRenderElement::from_element(inner, scale, rect) else { return Ok(None) };
        Ok(Some(Glass { inner, mask, key, group: None, ignore, behind: 0, commit: CommitCounter::default(), clip: None, track: Track::default(), soft: 0.0 }))
    }

    pub fn soft(mut self, soft: f32) -> Self {
        self.soft = soft;
        self
    }

    pub fn clipped(mut self, clip: Rectangle<i32, Physical>) -> Self {
        self.clip = Some(clip);
        self
    }

    /// One of a window's surfaces (`main`: the window's main surface): they share one blur.
    pub fn grouped(mut self, main: Id) -> Self {
        self.group = Some((self.key.0.clone(), main));
        self
    }

    pub fn group(&self) -> Option<&Id> {
        self.group.as_ref().map(|g| &g.1)
    }

    /// What is behind it (front-to-back, everything after it in the scene) decides when to re-blur.
    pub fn set_behind<'a, E: Element + 'a>(&mut self, behind: impl Iterator<Item = &'a E>, scale: Scale<f64>) {
        // grouped surfaces of one window: the same place for all (the window), so the same hash and one blur
        let geo = match (&self.group, self.clip) {
            (Some(_), Some(c)) => c,
            _ => self.geometry(scale),
        };
        // the layout behind (which elements, where, how opaque) in a hash; their contents by damage: a client
        // that redraws only away from the glass (a chat streaming text below the dock) does not re-blur it
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut now: HashMap<Id, CommitCounter> = HashMap::new();
        let mut changed = false;
        let bkey = self.group.clone().unwrap_or_else(|| self.key.clone());
        let frame = FRAME.with(|f| f.get());
        BEHIND.with(|b| {
            let mut b = b.borrow_mut();
            let prev = b.get(&bkey);
            for e in behind {
                let g = e.geometry(scale);
                if !g.overlaps(geo) {
                    continue;
                }
                e.id().hash(&mut h);
                (g.loc.x, g.loc.y, g.size.w, g.size.h).hash(&mut h);
                e.alpha().to_bits().hash(&mut h);
                let c = e.current_commit();
                if let Some(last) = prev.and_then(|p| p.2.get(e.id())).filter(|l| **l != c) {
                    changed |= e.damage_since(scale, Some(*last)).iter().any(|d| Rectangle::new(d.loc + g.loc, d.size).overlaps(geo));
                }
                now.insert(e.id().clone(), c);
            }
            (geo.loc.x, geo.loc.y, geo.size.w, geo.size.h).hash(&mut h);
            let layout = h.finish();
            let version = match prev {
                Some(p) if p.0 == layout && p.3 == frame => p.1,
                Some(p) if p.0 == layout => p.1 + changed as u64,
                Some(p) => p.1 + 1,
                None => 0,
            };
            // the group's other surfaces in this scene build saw the same: keep their bookkeeping
            if prev.is_none_or(|p| p.3 != frame) {
                b.insert(bkey.clone(), (layout, version, now, frame));
            }
            if b.len() > 64 {
                b.retain(|_, v| frame.saturating_sub(v.3) < 600);
            }
            let mut v = std::collections::hash_map::DefaultHasher::new();
            (layout, version).hash(&mut v);
            self.behind = v.finish();
        });
        let own = self.inner.current_commit();
        self.track = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let e = s.entry(self.key.clone()).or_insert_with(|| Track { behind: self.behind, own, ..Default::default() });
            if e.behind != self.behind || e.own != own {
                if e.behind != self.behind {
                    e.blur += 1;
                }
                e.behind = self.behind;
                e.own = own;
                e.out.increment();
                let t = (e.out, own, e.blur);
                e.hist.push(t);
                if e.hist.len() > 8 {
                    e.hist.remove(0);
                }
            }
            e.frame = FRAME.with(|f| f.get());
            let t = e.clone();
            if s.len() > 64 {
                s.retain(|_, v| t.frame.saturating_sub(v.frame) < 600);
            }
            t
        });
        self.commit = self.track.out;
    }
}

/// One scene build: ages the blur caches (textures of layers not drawn for ~10 s get freed).
/// IPC `stats`: blur requests and the ones that ran (the rest came from the cache).
pub fn counts() -> (u64, u64) {
    COUNTS.with(|c| c.get())
}
thread_local!(static COUNTS: Cell<(u64, u64)> = const { Cell::new((0, 0)) });
/// IPC `blurs`: requests and runs per live cache (output, surface).
pub fn per_cache() -> String {
    CACHE.with(|c| c.borrow().iter().map(|(k, e)| format!("{}/{} {} {:?}\n", e.runs, e.calls, k.0, k.1)).collect())
}

/// IPC `mem`: entries of the blur bookkeeping (blur chains with their GL textures, glass surfaces, blurs'
/// views of what is behind them)
pub fn sizes() -> (usize, usize, usize) {
    (CACHE.with(|c| c.borrow().len()), STATE.with(|s| s.borrow().len()), BEHIND.with(|b| b.borrow().len()))
}

pub fn tick() {
    FRAME.with(|f| f.set(f.get() + 1));
}

impl Element for Glass {
    fn id(&self) -> &Id {
        self.inner.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.commit
    }
    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }
    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.inner.src()
    }
    fn transform(&self) -> Transform {
        self.inner.transform()
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }
    fn damage_since(&self, scale: Scale<f64>, commit: Option<CommitCounter>) -> DamageSet<i32, Physical> {
        if commit == Some(self.commit) {
            return DamageSet::default();
        }
        // only the client drew since then: its own damage; the blur behind changed (or too long ago): all of
        // it, the blur spreads over the whole surface
        match commit.and_then(|c| self.track.hist.iter().find(|h| h.0 == c)) {
            Some(&(_, own, blur)) if blur == self.track.blur => self.inner.damage_since(scale, Some(own)),
            _ => DamageSet::from_slice(&[Rectangle::from_size(self.geometry(scale).size)]),
        }
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.inner.opaque_regions(scale)
    }
    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }
    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for Glass {
    fn draw(&self, frame: &mut GlesFrame<'_, '_>, src: Rectangle<f64, BufferCoords>, dst: Rectangle<i32, Physical>, damage: &[Rectangle<i32, Physical>], opaque_regions: &[Rectangle<i32, Physical>]) -> Result<(), GlesError> {
        let flipped = matches!(frame.transformation(), Transform::Flipped180);
        let (key, behind) = (self.group.as_ref().unwrap_or(&self.key), self.behind);
        // glass only inside the clip: its part of the surface and of the damage (damage is relative to dst)
        let gdst = match self.clip {
            Some(c) => match dst.intersection(c) {
                Some(g) => g,
                None => return RenderElement::<GlesRenderer>::draw(&self.inner, frame, src, dst, damage, opaque_regions),
            },
            None => dst,
        };
        let gdamage: Vec<Rectangle<i32, Physical>> = damage
            .iter()
            .filter_map(|d| Rectangle::new(d.loc + dst.loc, d.size).intersection(gdst).map(|i| Rectangle::new(i.loc - dst.loc, i.size)))
            .collect();
        let blurred = frame.with_context(|gl| unsafe {
            let mut vp = [0i32; 4];
            gl.GetIntegerv(ffi::VIEWPORT, vp.as_mut_ptr());
            // the region in GL window coords (Smithay's projection: physical y = GL y, winit's flip inverts)
            let y = if flipped { vp[3] - gdst.loc.y - gdst.size.h } else { gdst.loc.y };
            let region = Rectangle::<i32, Physical>::new((gdst.loc.x, y).into(), gdst.size).intersection(Rectangle::new((0, 0).into(), (vp[2], vp[3]).into()))?;
            if region.size.w < 2 || region.size.h < 2 {
                return None;
            }
            let t = blur(gl, key, region, behind)?;
            gl.ActiveTexture(ffi::TEXTURE1);
            gl.BindTexture(ffi::TEXTURE_2D, t);
            gl.ActiveTexture(ffi::TEXTURE0);
            Some(region)
        })?;
        if let Some(region) = blurred.filter(|_| !gdamage.is_empty()) {
            frame.override_default_tex_program(
                self.mask.clone(),
                vec![
                    Uniform::new("blurred", 1i32),
                    Uniform::new("origin", (region.loc.x as f32, region.loc.y as f32)),
                    Uniform::new("bsize", (region.size.w as f32, region.size.h as f32)),
                    Uniform::new("ignore", self.ignore),
                    Uniform::new("soft", self.soft),
                ],
            );
            let r = RenderElement::<GlesRenderer>::draw(&self.inner, frame, src, dst, &gdamage, opaque_regions);
            frame.clear_tex_program_override();
            frame.with_context(|gl| unsafe {
                gl.ActiveTexture(ffi::TEXTURE1);
                gl.BindTexture(ffi::TEXTURE_2D, 0);
                gl.ActiveTexture(ffi::TEXTURE0);
            })?;
            r?;
        }
        RenderElement::<GlesRenderer>::draw(&self.inner, frame, src, dst, damage, opaque_regions)
    }

    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
