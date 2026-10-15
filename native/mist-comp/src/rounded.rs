//! Rounded bottom corners for windows with a Mist title bar (the bar rounds the top ones): the window's main
//! surface is drawn with a texture shader that fades the two corners out. The corners leave the opaque region
//! (what is behind gets drawn there) and the surface never goes straight to a scanout plane uncornered.
//! GPU: a few ALU ops per pixel of the window's own surface, nothing per frame when nothing is damaged.
use std::cell::RefCell;

use smithay::{
    backend::renderer::{
        element::{surface::WaylandSurfaceRenderElement, Element, Id, Kind, RenderElement, UnderlyingStorage},
        gles::{GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform, UniformName, UniformType},
        utils::{CommitCounter, DamageSet, OpaqueRegions},
    },
    utils::{Buffer as BufferCoords, Physical, Point, Rectangle, Scale, Transform},
};

pub const RADIUS: f64 = 10.0;

const SHADER: &str = r#"
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
uniform vec2 size;
uniform float radius;
void main() {
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    vec2 p = v_coords * size;
    // bottom corners only: distance past the corner circle, 1 px antialiased edge
    vec2 c = vec2(clamp(p.x, radius, size.x - radius), size.y - radius);
    if (p.y > size.y - radius) {
        float d = length(p - c) - radius;
        color *= clamp(0.5 - d, 0.0, 1.0);
    }
    color *= alpha;
#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif
    gl_FragColor = color;
}
"#;

thread_local! {
    static PROGRAM: RefCell<Option<GlesTexProgram>> = const { RefCell::new(None) };
}

fn program(r: &mut GlesRenderer) -> Option<GlesTexProgram> {
    PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            match r.compile_custom_texture_shader(SHADER, &[UniformName::new("size", UniformType::_2f), UniformName::new("radius", UniformType::_1f)]) {
                Ok(prog) => *p = Some(prog),
                Err(e) => tracing::warn!("rounded corners shader: {e:?}"),
            }
        }
        p.clone()
    })
}

pub struct Rounded {
    inner: WaylandSurfaceRenderElement<GlesRenderer>,
    program: GlesTexProgram,
    radius: f64,
}

impl Rounded {
    /// Wrap a window's main surface element (None: no shader, draw it as is).
    pub fn new(r: &mut GlesRenderer, inner: WaylandSurfaceRenderElement<GlesRenderer>, scale: f64) -> Result<Self, WaylandSurfaceRenderElement<GlesRenderer>> {
        match program(r) {
            Some(program) => Ok(Rounded { inner, program, radius: RADIUS * scale }),
            None => Err(inner),
        }
    }
}

impl Element for Rounded {
    fn id(&self) -> &Id {
        self.inner.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
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
        self.inner.damage_since(scale, commit)
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        let size = self.geometry(scale).size;
        let r = self.radius.ceil() as i32;
        let corners = [Rectangle::new((0, size.h - r).into(), (r, r).into()), Rectangle::new((size.w - r, size.h - r).into(), (r, r).into())];
        Rectangle::subtract_rects_many(self.inner.opaque_regions(scale), corners).into_iter().collect()
    }
    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }
    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for Rounded {
    fn draw(&self, frame: &mut GlesFrame<'_, '_>, src: Rectangle<f64, BufferCoords>, dst: Rectangle<i32, Physical>, damage: &[Rectangle<i32, Physical>], opaque_regions: &[Rectangle<i32, Physical>]) -> Result<(), GlesError> {
        frame.override_default_tex_program(
            self.program.clone(),
            vec![Uniform::new("size", (dst.size.w as f32, dst.size.h as f32)), Uniform::new("radius", self.radius as f32)],
        );
        let r = RenderElement::<GlesRenderer>::draw(&self.inner, frame, src, dst, damage, opaque_regions);
        frame.clear_tex_program_override();
        r
    }

    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
