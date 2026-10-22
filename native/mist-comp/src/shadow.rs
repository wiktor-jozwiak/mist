//! Drop shadow around windows whose shape mist-comp knows (Mist bar, glass, rounded). One soft Gaussian
//! shadow (erf of the rounded-box distance: no hard band) plus a 1 px dark hairline that marks the edge on
//! dark wallpapers; the focused window gets the full shadow, the others a lighter one. A GLSL rounded-box
//! shadow; one element per window kept in its WinData, so it damages the screen only when the window moves,
//! resizes, fades or changes focus, and never paints under the window itself (its glass is translucent).
//! GPU: the ring around the window, only where damaged. CPU: none per frame.
use std::{cell::RefCell, collections::HashMap};

use smithay::{
    backend::renderer::{
        element::Kind,
        gles::{element::PixelShaderElement, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType},
    },
    utils::{Logical, Rectangle},
};

/// How far the shadow reaches past the window (OFFSET down + 3 sigma).
const PAD: i32 = 56;
/// The window's corner radius (Zen's rounded window, Mist Files).
const RADIUS: f32 = crate::rounded::RADIUS as f32;
/// Shadow strength of windows without focus, relative to the focused one.
pub const INACTIVE: f32 = 0.55;

// ambient shadow: 12 px down, sigma 14, 0.42; hairline: 1 px ring just outside the edge, 0.45
const SRC: &str = r#"
precision mediump float;
uniform float alpha;
uniform vec2 size;
varying vec2 v_coords;
uniform vec4 win;
uniform float radius;
float rbox(vec2 p, vec2 c, vec2 h, float r) {
    vec2 q = abs(p - c) - h + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
float erf(float x) {
    float x2 = x * x;
    float t = sqrt(1.0 - exp(-x2 * (1.2732 + 0.147 * x2) / (1.0 + 0.147 * x2)));
    return x < 0.0 ? -t : t;
}
void main() {
    vec2 p = v_coords * size;
    vec2 h = win.zw * 0.5;
    vec2 c = win.xy + h;
    float d = rbox(p, c, h, radius);
    if (d < 0.0)
        discard;
    float amb = 0.42 * 0.5 * (1.0 - erf(rbox(p, c + vec2(0.0, 12.0), h - 4.0, radius) / (14.0 * 1.4142)));
    float hair = 0.45 * (1.0 - smoothstep(0.0, 1.0, d));
    float a = 1.0 - (1.0 - amb) * (1.0 - hair);
    gl_FragColor = vec4(0.0, 0.0, 0.0, a * alpha);
}
"#;

thread_local! {
    static PROGRAM: RefCell<Option<GlesPixelProgram>> = const { RefCell::new(None) };
}

fn program(r: &mut GlesRenderer) -> Option<GlesPixelProgram> {
    PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            let u = [UniformName::new("win", UniformType::_4f), UniformName::new("radius", UniformType::_1f)];
            match r.compile_custom_pixel_shader(SRC, &u) {
                Ok(prog) => *p = Some(prog),
                Err(e) => tracing::warn!("window shadow shader: {e:?}"),
            }
        }
        p.clone()
    })
}

/// The shadow for a window whose geometry is `win` (relative to the output), drawn at `alpha`. `slots` keep
/// the element per output between frames: same id, a new commit only when the window moved, resized or faded.
pub fn element(r: &mut GlesRenderer, slots: &RefCell<HashMap<String, PixelShaderElement>>, output: &str, win: Rectangle<i32, Logical>, alpha: f32) -> Option<PixelShaderElement> {
    let prog = program(r)?;
    let area = Rectangle::new((win.loc.x - PAD, win.loc.y - PAD).into(), (win.size.w + 2 * PAD, win.size.h + 2 * PAD).into());
    let uniforms = || vec![Uniform::new("win", [PAD as f32, PAD as f32, win.size.w as f32, win.size.h as f32]), Uniform::new("radius", RADIUS)];
    let mut slots = slots.borrow_mut();
    match slots.get_mut(output) {
        // a fade (open / close / workspace) changes alpha: a new element, as alpha is fixed at creation
        Some(e) if (smithay::backend::renderer::element::Element::alpha(e) - alpha).abs() < 0.001 => {
            if smithay::backend::renderer::element::Element::geometry(e, 1.0.into()).size != area.size.to_physical(1) {
                e.update_uniforms(uniforms());
            }
            e.resize(area, None);
        }
        _ => {
            slots.insert(output.to_string(), PixelShaderElement::new(prog, area, None, alpha, uniforms(), Kind::Unspecified));
        }
    }
    slots.get(output).cloned()
}
