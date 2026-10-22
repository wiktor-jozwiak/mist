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
    Shadow = smithay::backend::renderer::gles::element::PixelShaderElement,
    // the overview's windows, scaled into their grid places, over the dimmed desktop
    ScaledSurface = Scaled<WaylandSurfaceRenderElement<GlesRenderer>>,
    ScaledMem = Scaled<MemoryRenderBufferRenderElement<GlesRenderer>>,
    ScaledRounded = Scaled<crate::rounded::Rounded>,
    Solid = smithay::backend::renderer::element::solid::SolidColorRenderElement,
}

smithay::backend::renderer::element::render_elements! {
    /// What an output shows: the scene, magnified around the pointer when the zoom is on
    pub ViewElem<=GlesRenderer>;
    Plain = SceneElem,
    Zoom = smithay::backend::renderer::element::utils::RescaleRenderElement<SceneElem>,
}

/// The magnifier (`zoom in|out|off`): everything scaled around the pointer, so what is under it stays under it
/// and input needs no mapping. Off (1.0) or the pointer on another output: the scene as it is.
pub fn view(elems: Vec<SceneElem>, zoom: f64, pointer: Option<Point<i32, smithay::utils::Physical>>) -> Vec<ViewElem> {
    match pointer.filter(|_| zoom > 1.0) {
        Some(at) => elems.into_iter().map(|e| ViewElem::Zoom(smithay::backend::renderer::element::utils::RescaleRenderElement::from_element(e, at, zoom))).collect(),
        None => elems.into_iter().map(ViewElem::Plain).collect(),
    }
}

/// Where the pointer is on this output (physical, output-relative), if it is on it.
pub fn pointer_on(space: &Space<Window>, output: &Output, ptr: Point<f64, Logical>) -> Option<Point<i32, smithay::utils::Physical>> {
    let og = space.output_geometry(output)?;
    og.to_f64().contains(ptr).then(|| (ptr - og.loc.to_f64()).to_physical(output.current_scale().fractional_scale()).to_i32_round())
}

type Scaled<E> = smithay::backend::renderer::element::utils::RelocateRenderElement<smithay::backend::renderer::element::utils::RescaleRenderElement<E>>;

/// The overview's view of one window: its surfaces and bar scaled from `from` (where it is) to `to`.
fn overview_elements(r: &mut GlesRenderer, w: &Window, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>, scale: Scale<f64>) -> Vec<SceneElem> {
    use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement, RescaleRenderElement};
    let k = to.size.w as f64 / from.size.w.max(1) as f64;
    let origin = from.loc.to_physical_precise_round(scale);
    let offset = (to.loc - from.loc).to_physical_precise_round(scale);
    let bar = if crate::wm::data(w).deco.bar.borrow().is_some() { BAR } else { 0 };
    let loc = from.loc + Point::from((0, bar));
    window_elements(r, w, loc, scale, 1.0, None)
        .into_iter()
        .filter_map(|e| {
            Some(match e {
                SceneElem::Surface(e) => SceneElem::ScaledSurface(RelocateRenderElement::from_element(RescaleRenderElement::from_element(e, origin, k), offset, Relocate::Relative)),
                SceneElem::Mem(e) => SceneElem::ScaledMem(RelocateRenderElement::from_element(RescaleRenderElement::from_element(e, origin, k), offset, Relocate::Relative)),
                SceneElem::Rounded(e) => SceneElem::ScaledRounded(RelocateRenderElement::from_element(RescaleRenderElement::from_element(e, origin, k), offset, Relocate::Relative)),
                _ => return None,
            })
        })
        .collect()
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

/// One window front to back: its surfaces (main one with rounded corners when barred or square), its shadow,
/// then its bar. `loc` = window geometry origin relative to the target's origin. `output`: the output's name
/// when drawn on it (not into a snapshot): a translucent app's surfaces then blur what is behind them, and
/// windows without a Mist bar get the window shadow.
pub fn window_elements(r: &mut GlesRenderer, w: &Window, loc: Point<i32, Logical>, scale: Scale<f64>, alpha: f32, output: Option<&str>) -> Vec<SceneElem> {
    let mut out = Vec::new();
    if alpha <= 0.0 {
        return out;
    }
    let bar = crate::wm::data(w).deco.bar.borrow();
    let at = (loc - w.geometry().loc).to_physical_precise_round(scale);
    let v: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = w.render_elements(r, at, scale, alpha);
    let app = crate::hyprcompat::app_id(w).to_lowercase();
    // a barred window: its main surface gets the rounded bottom corners; an app with its own bar on a square
    // window (Claude, Spotify) all four
    let round = bar.is_none() && crate::decor::rounds(&app);
    let main = (bar.is_some() || round).then(|| smithay::wayland::seat::WaylandFocus::wl_surface(w)).flatten().map(|s| Id::from_wayland_resource(&*s));
    // a translucent app drawing its own corners (Zen): every surface over blurred glass (Firefox draws the
    // page into a subsurface, the main surface only holds the window shape; popups get it too)
    let clip = Rectangle::new(loc.to_physical_precise_round(scale), w.geometry().size.to_physical_precise_round(scale));
    let glass = output.filter(|_| bar.is_none() && crate::blur::window_rule(&app));
    let gid = glass.and_then(|_| smithay::wayland::seat::WaylandFocus::wl_surface(w)).map(|s| Id::from_wayland_resource(&*s));
    // the window's own surfaces (not its popups) end at its geometry: Firefox's GTK frame and square shadow
    // in the margin around it are cut, mist-comp draws the rounded shadow
    let mut tree = Vec::new();
    if let Some(s) = glass.and_then(|_| smithay::wayland::seat::WaylandFocus::wl_surface(w)) {
        smithay::wayland::compositor::with_surface_tree_downward(
            &s,
            (),
            |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
            |s, _, _| tree.push(Id::from_wayland_resource(s)),
            |_, _, _| true,
        );
    }
    for e in v {
        if let Some(o) = glass {
            let crop = tree.contains(e.id()).then_some(clip);
            match crate::blur::Glass::new(r, e, o, 0.02, crop, scale) {
                Ok(Some(g)) => out.push(SceneElem::Glass(match &gid {
                    Some(id) => g.clipped(clip).grouped(id.clone()).soft(0.5),
                    None => g.clipped(clip).soft(0.5),
                })),
                Ok(None) => {}
                Err(e) => out.push(SceneElem::Surface(e)),
            }
        } else if main.as_ref() == Some(e.id()) {
            match crate::rounded::Rounded::new(r, e, scale.x, round) {
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
    // every window whose shape mist-comp knows (Mist bar, glass, rounded) gets the window shadow, behind it
    // and its bar; windows drawing their own frame (CSD) bring their own shadow
    if let Some(o) = output.filter(|_| bar.is_some() || glass.is_some() || round) {
        let mut shape = Rectangle::new(loc, w.geometry().size);
        if bar.is_some() {
            shape.loc.y -= BAR;
            shape.size.h += BAR;
        }
        if let Some(e) = crate::shadow::element(r, &crate::wm::data(w).shadow, o, shape, if crate::wm::data(w).active.get() { alpha } else { alpha * crate::shadow::INACTIVE }) {
            out.push(SceneElem::Shadow(e));
        }
    }
    out
}

/// The parts of a window its client marks opaque (wl_surface.set_opaque_region of the main surface), in
/// global coordinates; none for translucent apps (glass rule), X11 windows or a region with holes.
fn opaque_rects(w: &Window, loc: Point<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    let app = crate::hyprcompat::app_id(w).to_lowercase();
    if crate::blur::window_rule(&app) {
        return Vec::new();
    }
    let Some(s) = smithay::wayland::seat::WaylandFocus::wl_surface(w).filter(|_| w.toplevel().is_some()) else { return Vec::new() };
    let origin = loc - w.geometry().loc;
    smithay::wayland::compositor::with_states(&s, |st| {
        let mut attrs = st.cached_state.get::<smithay::wayland::compositor::SurfaceAttributes>();
        let Some(r) = attrs.current().opaque_region.clone() else { return Vec::new() };
        if r.rects.iter().any(|(k, _)| matches!(k, smithay::wayland::compositor::RectangleKind::Subtract)) {
            return Vec::new();
        }
        r.rects.iter().map(|(_, rect)| Rectangle::new(rect.loc + origin, rect.size)).collect()
    })
}

/// A drag-and-drop icon: its surface, and where it is on the desktop (the pointer + the icon's offset).
pub type Dnd = (smithay::reexports::wayland_server::protocol::wl_surface::WlSurface, Point<i32, Logical>);

pub fn elements(r: &mut GlesRenderer, space: &Space<Window>, lock: Option<&LockSurface>, locked: bool, output: &Output, anim: &crate::anim::Anims, dnd: Option<&Dnd>) -> Vec<SceneElem> {
    let mut out = Vec::new();
    let scale = Scale::from(output.current_scale().fractional_scale());
    if locked {
        if let Some(l) = lock {
            out.extend(render_elements_from_surface_tree(r, l.wl_surface(), (0, 0), scale, 1.0, Kind::Unspecified).into_iter().map(SceneElem::Surface));
        }
        return out;
    }
    let Some(og) = space.output_geometry(output) else { return out };
    // the dragged thing under the pointer, above everything
    if let Some((icon, at)) = dnd {
        let loc = (*at - og.loc).to_physical_precise_round(scale);
        out.extend(render_elements_from_surface_tree(r, icon, loc, scale, 1.0, Kind::Unspecified).into_iter().map(SceneElem::Surface));
    }
    let now = std::time::Instant::now();
    let map = layer_map_for_output(output);
    let layers = |out: &mut Vec<SceneElem>, r: &mut GlesRenderer, lvl: Layer| {
        for l in map.layers_on(lvl).rev() {
            let Some(g) = map.layer_geometry(l) else { continue };
            let alpha = anim.layer_alpha(&smithay::reexports::wayland_server::Resource::id(l.wl_surface()), now);
            let v: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = l.render_elements(r, g.loc.to_physical_precise_round(scale), scale, alpha);
            // Mist glass panels: the main surface blurs what is behind it; a blur region (ext-background-effect)
            // keeps it to that box (Mist's panels: their cards), or asks for it on any other layer
            let region = crate::bgeffect::region(l.wl_surface());
            let ignore = match region {
                Some(None) => None,
                Some(Some(_)) => Some(crate::blur::rule(l.namespace()).unwrap_or(0.0)),
                None => crate::blur::rule(l.namespace()),
            };
            let clip = region.flatten().map(|b| Rectangle::new(b.loc + g.loc, b.size).to_physical_precise_round(scale));
            let glass = ignore.map(|ignore| (ignore, Id::from_wayland_resource(l.wl_surface())));
            for e in v {
                match glass.as_ref().filter(|(_, id)| e.id() == id) {
                    Some((ignore, _)) => match crate::blur::Glass::new(r, e, &output.name(), *ignore, None, scale) {
                        Ok(Some(gl)) => out.push(SceneElem::Glass(match clip {
                            Some(c) => gl.clipped(c),
                            None => gl,
                        })),
                        Ok(None) => {}
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
    if let Some(ov) = anim.overview.as_ref().filter(|o| o.output == name) {
        let p = ov.progress(now);
        let lerp = |a: i32, b: i32| a + ((b - a) as f64 * p).round() as i32;
        for w in space.elements().rev() {
            let (Some(to), Some(loc)) = (ov.slot(w), space.element_location(w)) else { continue };
            let from = crate::overview::visual(w, loc, og.loc);
            let now_at = Rectangle::new((lerp(from.loc.x, to.loc.x), lerp(from.loc.y, to.loc.y)).into(), (lerp(from.size.w, to.size.w), lerp(from.size.h, to.size.h)).into());
            out.extend(overview_elements(r, w, from, now_at, scale));
        }
        let dim = (0.55 * p) as f32;
        let full = Rectangle::new((0, 0).into(), og.size).to_physical_precise_round(scale);
        out.push(SceneElem::Solid(smithay::backend::renderer::element::solid::SolidColorRenderElement::new(
            ov.dim.clone(),
            full,
            (dim * 1000.0) as usize,
            [0.0, 0.0, 0.0, dim],
            Kind::Unspecified,
        )));
        layers(&mut out, r, Layer::Bottom);
        layers(&mut out, r, Layer::Background);
        crate::blur::tick();
        set_glass_behind(&mut out, scale);
        return out;
    }
    for w in space.elements().rev() {
        let Some(loc) = space.element_location(w) else { continue };
        let on_output = space.element_bbox(w).is_some_and(|b| b.overlaps(og));
        let (off, alpha) = anim.window(crate::wm::data(w).addr.get(), if on_output { &name } else { "" }, og.size.w, now);
        if reaches(w, loc + off, og) {
            out.extend(window_elements(r, w, loc + off - og.loc, scale, alpha, Some(&name)));
        }
    }
    for (w, loc) in anim.leaving(&name, og.size.w, now) {
        if reaches(&w, loc, og) {
            out.extend(window_elements(r, &w, loc - og.loc, scale, 1.0, Some(&name)));
        }
    }
    out.extend(anim.closing(og, scale, now).into_iter().map(SceneElem::Tex));
    layers(&mut out, r, Layer::Bottom);
    layers(&mut out, r, Layer::Background);
    crate::blur::tick();
    set_glass_behind(&mut out, scale);
    out
}

/// Every glass element learns what is behind it (the elements after it).
fn set_glass_behind(out: &mut [SceneElem], scale: Scale<f64>) {
    for i in 0..out.len() {
        let SceneElem::Glass(g) = &out[i] else { continue };
        // a window's surfaces (next to each other) share one blur: what is behind the whole window
        let mut j = i + 1;
        if let Some(id) = g.group().cloned() {
            while matches!(out.get(j), Some(SceneElem::Glass(h)) if h.group() == Some(&id)) {
                j += 1;
            }
        }
        let (front, behind) = out.split_at_mut(j);
        if let SceneElem::Glass(g) = &mut front[i] {
            g.set_behind(behind.iter(), scale);
        }
    }
}

/// wp_presentation: the feedback requests of everything the output shows this frame (windows, layers, the
/// lock surface). The backend marks them presented at the frame's vblank, or discarded when nothing changed.
pub fn take_feedback(space: &Space<Window>, lock: Option<&LockSurface>, output: &Output) -> smithay::desktop::utils::OutputPresentationFeedback {
    use smithay::{reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind as Fk, wayland::compositor::SurfaceData};
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    let mut fb = smithay::desktop::utils::OutputPresentationFeedback::new(output);
    let on = |_: &WlSurface, _: &SurfaceData| Some(output.clone());
    let flags = |_: &WlSurface, _: &SurfaceData| Fk::empty();
    for w in space.elements_for_output(output) {
        w.take_presentation_feedback(&mut fb, on, flags);
    }
    for l in layer_map_for_output(output).layers() {
        l.take_presentation_feedback(&mut fb, on, flags);
    }
    if let Some(l) = lock {
        smithay::desktop::utils::take_presentation_feedback_surface_tree(l.wl_surface(), &mut fb, on, flags);
    }
    fb
}

/// Frame callbacks for everything the output shows (windows, layers, the lock surface).
pub fn send_frames(space: &Space<Window>, lock: Option<&LockSurface>, output: &Output, now: std::time::Duration) {
    let zero = Some(std::time::Duration::ZERO);
    // top to bottom: what the windows above cover with opaque pixels. A window completely under them gets a
    // frame callback once a second instead of every refresh: a browser or chat app behind a maximized window
    // stops drawing (its CPU and GPU, and our frames), and still answers within a second. Uncovering it
    // changes the layout, which redraws everything and sends it a frame at once.
    let mut opaque: Vec<Rectangle<i32, Logical>> = Vec::new();
    let t = std::time::Instant::now();
    for w in space.elements().rev() {
        let Some(loc) = space.element_location(w) else { continue };
        let shape = Rectangle::new(loc, w.geometry().size);
        if !space.outputs_for_element(w).contains(output) {
            opaque.extend(opaque_rects(w, loc));
            continue;
        }
        let covered = !opaque.is_empty() && shape.subtract_rects(opaque.iter().copied()).is_empty();
        let d = crate::wm::data(w);
        if covered {
            if d.covered_frame.get().is_some_and(|at| t.duration_since(at) < std::time::Duration::from_secs(1)) {
                continue;
            }
            d.covered_frame.set(Some(t));
        } else {
            d.covered_frame.set(None);
        }
        w.send_frame(output, now, zero, |_, _| Some(output.clone()));
        opaque.extend(opaque_rects(w, loc));
    }
    for l in layer_map_for_output(output).layers() {
        l.send_frame(output, now, zero, |_, _| Some(output.clone()));
    }
    if let Some(l) = lock {
        smithay::desktop::utils::send_frames_surface_tree(l.wl_surface(), output, now, zero, |_, _| Some(output.clone()));
    }
}
