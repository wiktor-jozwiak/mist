//! Which surface of a window or layer takes the pointer: the topmost one of its tree that has the point
//! in its input region. smithay 0.7's under_from_surface_tree walks the tree top-down but goes on after a
//! hit and keeps the last one, so a subsurface placed below its parent (Mist Player's video plane under
//! its controls) took the pointer from the surface drawn over it.

use std::cell::RefCell;

use smithay::{
    backend::renderer::utils::RendererSurfaceStateUserData,
    desktop::{LayerSurface, Window, WindowSurfaceType},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
    wayland::seat::WaylandFocus,
    wayland::compositor::{with_surface_tree_downward, SurfaceAttributes, SurfaceData, TraversalAction},
};

fn offset(states: &SurfaceData) -> Option<Point<i32, Logical>> {
    states.data_map.get::<RendererSurfaceStateUserData>()?.lock().unwrap().view().map(|v| v.offset)
}

fn takes(states: &SurfaceData, point: Point<f64, Logical>) -> bool {
    let Some(size) = states.data_map.get::<RendererSurfaceStateUserData>().and_then(|d| d.lock().unwrap().view()).map(|v| v.dst) else {
        return false;
    };
    if !Rectangle::from_size(size).to_f64().contains(point) {
        return false;
    }
    let mut attrs = states.cached_state.get::<SurfaceAttributes>();
    attrs.current().input_region.as_ref().is_none_or(|r| r.contains(point.to_i32_round()))
}

/// The topmost surface of `surface`'s tree (subsurfaces in their stacking order) under `point`.
pub fn under_tree(surface: &WlSurface, point: Point<f64, Logical>, location: Point<i32, Logical>) -> Option<(WlSurface, Point<i32, Logical>)> {
    let found = RefCell::new(None);
    with_surface_tree_downward(
        surface,
        location,
        |_, states, loc: &Point<i32, Logical>| match offset(states) {
            Some(o) if found.borrow().is_none() => TraversalAction::DoChildren(*loc + o),
            _ => TraversalAction::SkipChildren,
        },
        |s, states, loc: &Point<i32, Logical>| {
            if found.borrow().is_some() {
                return;
            }
            let Some(o) = offset(states) else { return };
            let loc = *loc + o;
            if takes(states, point - loc.to_f64()) {
                *found.borrow_mut() = Some((s.clone(), loc));
            }
        },
        |_, _, _| found.borrow().is_none(),
    );
    found.into_inner()
}

/// Window::surface_under(ALL) with the toplevel's tree walked by [`under_tree`] (popups first, as smithay).
pub fn window_under(w: &Window, point: Point<f64, Logical>) -> Option<(WlSurface, Point<i32, Logical>)> {
    w.surface_under(point, WindowSurfaceType::POPUP | WindowSurfaceType::SUBSURFACE)
        .or_else(|| w.wl_surface().and_then(|s| under_tree(&s, point, (0, 0).into())))
}

/// LayerSurface::surface_under(ALL), the same way.
pub fn layer_under(l: &LayerSurface, point: Point<f64, Logical>) -> Option<(WlSurface, Point<i32, Logical>)> {
    l.surface_under(point, WindowSurfaceType::POPUP | WindowSurfaceType::SUBSURFACE)
        .or_else(|| under_tree(l.wl_surface(), point, (0, 0).into()))
}
