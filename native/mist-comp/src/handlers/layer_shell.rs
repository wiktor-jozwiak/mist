use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface, WindowSurfaceType},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{
            KeyboardInteractivity, Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
            WlrLayerShellState,
        },
    },
};

use smithay::utils::SERIAL_COUNTER;

use crate::MistComp;

impl WlrLayerShellHandler for MistComp {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(&mut self, surface: WlrLayerSurface, wl_output: Option<WlOutput>, _layer: Layer, namespace: String) {
        let Some(output) = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned())
        else {
            surface.send_close();
            return;
        };
        tracing::info!("layer {namespace} on {} ({_layer:?})", output.name());
        let mut map = layer_map_for_output(&output);
        map.map_layer(&LayerSurface::new(surface, namespace)).unwrap();
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        if let Some((mut map, layer)) = self.space.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer = map.layers().find(|l| l.layer_surface() == &surface).cloned();
            layer.map(|l| (map, l))
        }) {
            map.unmap_layer(&layer);
        }
        // a popup that had the keys gives them back to the top window
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.current_focus().is_some_and(|f| f.is(surface.wl_surface())) {
            let top = self.space.elements().last().and_then(crate::focus::of_window);
            keyboard.set_focus(self, top, SERIAL_COUNTER.next_serial());
        }
    }
}

delegate_layer_shell!(MistComp);

/// Arrange the layer map and send the first configure. True when the surface was a layer.
pub fn handle_commit(state: &mut MistComp, surface: &WlSurface) -> bool {
    let Some(output) = state.space.outputs().find(|o| {
        layer_map_for_output(o).layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some()
    }) else {
        return false;
    };
    let sent = with_states(surface, |s| {
        s.data_map.get::<LayerSurfaceData>().unwrap().lock().unwrap().initial_configure_sent
    });
    let mut map = layer_map_for_output(output);
    map.arrange();
    let layer = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).unwrap().clone();
    drop(map);
    if !sent {
        layer.layer_surface().send_configure();
    }
    // mapped (first buffer, or a buffer again after a null one): fade in like Hyprland's layersIn
    let has = smithay::backend::renderer::utils::with_renderer_surface_state(surface, |s| s.buffer().is_some()).unwrap_or(false);
    let had = with_states(surface, |s| s.data_map.get_or_insert(|| std::cell::Cell::new(false)).replace(has));
    if has && !had && layer.layer() != Layer::Background && crate::anim::layer_fades(layer.namespace()) {
        state.anim.layer_opened(smithay::reexports::wayland_server::Resource::id(surface));
        state.dirty = true;
    }
    // exclusive keyboard (Spotlight, lock-like popups): focus on map, give it back when it goes
    let exclusive = layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive;
    let keyboard = state.seat.get_keyboard().unwrap();
    let focused = keyboard.current_focus().is_some_and(|f| f.is(surface));
    if exclusive && !focused && sent {
        keyboard.set_focus(state, Some(surface.clone().into()), SERIAL_COUNTER.next_serial());
    }
    true
}
