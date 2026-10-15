//! What one output shows, front to back, for every backend (winit, headless, DRM, screencopy):
//! overlay + top layers, windows top to bottom each with its server-side bar, bottom + background layers.
//! Locked: only that output's lock surface (on black), never the desktop.
use smithay::{
    backend::renderer::{
        element::{
            memory::MemoryRenderBufferRenderElement,
            surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
            AsRenderElements, Element, Id, Kind,
        },
        gles::GlesRenderer,
    },
    desktop::{layer_map_for_output, Space, Window},
    output::Output,
    utils::{Logical, Point, Rectangle, Scale},
    wayland::{session_lock::LockSurface, shell::wlr_layer::Layer},
};

use crate::decor::BAR;

smithay::backend::renderer::element::render_elements! {
    pub SceneElem<=GlesRenderer>;
    Surface = WaylandSurfaceRenderElement<GlesRenderer>,
    Mem = MemoryRenderBufferRenderElement<GlesRenderer>,
    Rounded = crate::rounded::Rounded,
    Glass = crate::blur::Glass,
    Tex = smithay::backend::renderer::element::texture::TextureRenderElement<smithay::backend::renderer::gles::GlesTexture>,
}

/// Does the window (at `loc`, with its bar) show on the output rect?
fn reaches(w: &Window, loc: Point<i32, Logical>, og: Rectangle<i32, Logical>) -> bool {
    let mut reach = w.bbox();
    reach.loc += loc - w.geometry().loc;
    if crate::wm::data(w).deco.bar.borrow().is_some() {
        reach = reach.merge(Rectangle::new((loc.x, loc.y - BAR).into(), (w.geometry().size.w, BAR).into()));
    }
    reach.overlaps(og)
}

/// One window front to back: its surfaces (main one with rounded corners when barred), then its bar.
/// `loc` = window geometry origin relative to the target's origin.
pub fn window_elements(r: &mut GlesRenderer, w: &Window, loc: Point<i32, Logical>, scale: Scale<f64>, alpha: f32) -> Vec<SceneElem> {
    let mut out = Vec::new();
    if alpha <= 0.0 {
        return out;
    }
    let bar = crate::wm::data(w).deco.bar.borrow();
    let at = (loc - w.geometry().loc).to_physical_precise_round(scale);
    let v: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = w.render_elements(r, at, scale, alpha);
    // a barred window: its main surface gets the rounded bottom corners
    let main = bar.as_ref().and(smithay::wayland::seat::WaylandFocus::wl_surface(w)).map(|s| Id::from_wayland_resource(&*s));
    for e in v {
        if main.as_ref() == Some(e.id()) {
            match crate::rounded::Rounded::new(r, e, scale.x) {
                Ok(rd) => out.push(SceneElem::Rounded(rd)),
                Err(e) => out.push(SceneElem::Surface(e)),
            }
        } else {
            out.push(SceneElem::Surface(e));
        }
    }
    if let Some(b) = bar.as_ref() {
        let at = (loc - Point::<i32, Logical>::from((0, BAR))).to_f64().to_physical(scale);
        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(r, at, &b.buf, Some(alpha), None, None, Kind::Unspecified) {
            out.push(SceneElem::Mem(e));
        }
    }
    out
}

pub fn elements(r: &mut GlesRenderer, space: &Space<Window>, lock: Option<&LockSurface>, locked: bool, output: &Output, anim: &crate::anim::Anims) -> Vec<SceneElem> {
    let mut out = Vec::new();
    let scale = Scale::from(output.current_scale().fractional_scale());
    if locked {
        if let Some(l) = lock {
            out.extend(render_elements_from_surface_tree(r, l.wl_surface(), (0, 0), scale, 1.0, Kind::Unspecified).into_iter().map(SceneElem::Surface));
        }
        return out;
    }
    let Some(og) = space.output_geometry(output) else { return out };
    let now = std::time::Instant::now();
    let map = layer_map_for_output(output);
    let layers = |out: &mut Vec<SceneElem>, r: &mut GlesRenderer, lvl: Layer| {
        for l in map.layers_on(lvl).rev() {
            let Some(g) = map.layer_geometry(l) else { continue };
            let alpha = anim.layer_alpha(&smithay::reexports::wayland_server::Resource::id(l.wl_surface()), now);
            let v: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = l.render_elements(r, g.loc.to_physical_precise_round(scale), scale, alpha);
            // Mist glass panels: the main surface blurs what is behind it
            let glass = crate::blur::rule(l.namespace()).map(|ignore| (ignore, Id::from_wayland_resource(l.wl_surface())));
            for e in v {
                match glass.as_ref().filter(|(_, id)| e.id() == id) {
                    Some((ignore, _)) => match crate::blur::Glass::new(r, e, &output.name(), *ignore) {
                        Ok(gl) => out.push(SceneElem::Glass(gl)),
                        Err(e) => out.push(SceneElem::Surface(e)),
                    },
                    None => out.push(SceneElem::Surface(e)),
                }
            }
        }
    };
    layers(&mut out, r, Layer::Overlay);
    layers(&mut out, r, Layer::Top);
    let name = output.name();
    for w in space.elements().rev() {
        let Some(loc) = space.element_location(w) else { continue };
        let on_output = space.element_bbox(w).is_some_and(|b| b.overlaps(og));
        let (off, alpha) = anim.window(crate::wm::data(w).addr.get(), if on_output { &name } else { "" }, og.size.w, now);
        if reaches(w, loc + off, og) {
            out.extend(window_elements(r, w, loc + off - og.loc, scale, alpha));
        }
    }
    for (w, loc) in anim.leaving(&name, og.size.w, now) {
        if reaches(&w, loc, og) {
            out.extend(window_elements(r, &w, loc - og.loc, scale, 1.0));
        }
    }
    out.extend(anim.closing(og, scale, now).into_iter().map(SceneElem::Tex));
    layers(&mut out, r, Layer::Bottom);
    layers(&mut out, r, Layer::Background);
    crate::blur::tick();
    for i in 0..out.len() {
        let (front, behind) = out.split_at_mut(i + 1);
        if let SceneElem::Glass(g) = &mut front[i] {
            g.set_behind(behind.iter(), scale);
        }
    }
    out
}
