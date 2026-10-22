//! Drop shadow around windows with glass (Zen, Mist Files), the Mist panel shadow a bit stronger (--mist-shadow: 14 px
//! down, blur 30, spread -6, .45 instead of .32, plus 2 px down, blur 6, .18). A GLSL rounded-box shadow; one element per
//! window kept in its WinData, so it damages the screen only when the window moves or resizes, and never
//! paints under the window itself (its glass is translucent). GPU: the ring around the window, only where
//! damaged. CPU: none per frame.
use std::{cell::RefCell, collections::HashMap};

use smithay::{
    backend::renderer::{
        element::Kind,
        gles::{element::PixelShaderElement, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType},
    },
    utils::{Logical, Rectangle},
};

/// How far the shadow reaches past the window (14 down + 30 blur).
const PAD: i32 = 48;
/// The window's corner radius (Zen's rounded window, Mist Files).
const RADIUS: f32 = crate::rounded::RADIUS as f32;

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
void main() {
    vec2 p = v_coords * size;
    vec2 h = win.zw * 0.5;
    vec2 c = win.xy + h;
    if (rbox(p, c, h, radius) < 0.0)
        discard;
    float far = 0.45 * (1.0 - smoothstep(-15.0, 15.0, rbox(p, c + vec2(0.0, 14.0), h - 6.0, radius)));
    float near = 0.18 * (1.0 - smoothstep(-3.0, 3.0, rbox(p, c + vec2(0.0, 2.0), h, radius)));
    float a = 1.0 - (1.0 - far) * (1.0 - near);
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
