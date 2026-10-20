//! The protocols Smithay implements and a daily desktop needs beyond the basics: input methods (fcitx5),
//! virtual keyboards (KDE Connect remote input, wtype), tablets, the window list for docks / bars / portals,
//! ext-data-control (clipboard managers), xdg-foreign (portal dialogs over their app), toplevel icons and tags,
//! content type, alpha modifier, the system bell, XWayland keyboard grabs, and security-context for sandboxes.
//!
//! Sandboxed clients (flatpak, through security-context) do not see the privileged globals: virtual keyboard,
//! input method, data control, the window list.
use std::sync::Arc;

use smithay::wayland::seat::WaylandFocus as _;

use smithay::{
    delegate_alpha_modifier, delegate_content_type, delegate_ext_data_control, delegate_foreign_toplevel_list,
    delegate_input_method_manager, delegate_security_context, delegate_tablet_manager, delegate_text_input_manager,
    delegate_virtual_keyboard_manager, delegate_xdg_foreign, delegate_xdg_system_bell, delegate_xdg_toplevel_icon,
    delegate_xdg_toplevel_tag, delegate_xwayland_keyboard_grab,
    desktop::{PopupKind, Window},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client, DisplayHandle},
    utils::{Logical, Rectangle},
    wayland::{
        alpha_modifier::AlphaModifierState,
        content_type::ContentTypeState,
        foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelListHandler, ForeignToplevelListState},
        input_method::{InputMethodHandler, InputMethodManagerState, PopupSurface as ImPopupSurface},
        security_context::{SecurityContext, SecurityContextHandler, SecurityContextListenerSource, SecurityContextState},
        selection::{
            ext_data_control::{DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState},
            primary_selection::PrimarySelectionState,
        },
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_foreign::{XdgForeignHandler, XdgForeignState},
        xdg_system_bell::{XdgSystemBellHandler, XdgSystemBellState},
        xdg_toplevel_icon::{XdgToplevelIconHandler, XdgToplevelIconManager},
        xdg_toplevel_tag::{XdgToplevelTagHandler, XdgToplevelTagManager},
        xwayland_keyboard_grab::{XWaylandKeyboardGrabHandler, XWaylandKeyboardGrabState},
    },
};

use crate::{state::ClientState, MistComp};

/// Not inside a sandbox (no security context): may use the privileged globals.
fn trusted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_none_or(|c| c.security_context.is_none())
}

pub struct Extra {
    pub foreign_toplevels: ForeignToplevelListState,
    pub ext_data_control: ExtDataControlState,
    pub xdg_foreign: XdgForeignState,
    _virtual_keyboard: VirtualKeyboardManagerState,
    _input_method: InputMethodManagerState,
    _text_input: TextInputManagerState,
    _tablet: TabletManagerState,
    _icon: XdgToplevelIconManager,
    _tag: XdgToplevelTagManager,
    _content_type: ContentTypeState,
    _alpha: AlphaModifierState,
    _bell: XdgSystemBellState,
    _xwayland_grab: XWaylandKeyboardGrabState,
    _security: SecurityContextState,
}

impl Extra {
    pub fn new(dh: &DisplayHandle, primary: Option<&PrimarySelectionState>) -> Self {
        Self {
            foreign_toplevels: ForeignToplevelListState::new_with_filter::<MistComp>(dh, trusted),
            ext_data_control: ExtDataControlState::new::<MistComp, _>(dh, primary, trusted),
            xdg_foreign: XdgForeignState::new::<MistComp>(dh),
            _virtual_keyboard: VirtualKeyboardManagerState::new::<MistComp, _>(dh, trusted),
            _input_method: InputMethodManagerState::new::<MistComp, _>(dh, trusted),
            _text_input: TextInputManagerState::new::<MistComp>(dh),
            _tablet: TabletManagerState::new::<MistComp>(dh),
            _icon: XdgToplevelIconManager::new::<MistComp>(dh),
            _tag: XdgToplevelTagManager::new::<MistComp>(dh),
            _content_type: ContentTypeState::new::<MistComp>(dh),
            _alpha: AlphaModifierState::new::<MistComp>(dh),
            _bell: XdgSystemBellState::new::<MistComp>(dh),
            _xwayland_grab: XWaylandKeyboardGrabState::new::<MistComp>(dh),
            // a sandbox inside a sandbox may not create more contexts
            _security: SecurityContextState::new::<MistComp, _>(dh, trusted),
        }
    }
}

// ------------------------------------------------------------------ window list (ext-foreign-toplevel-list)
impl ForeignToplevelListHandler for MistComp {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.extra.foreign_toplevels
    }
}
delegate_foreign_toplevel_list!(MistComp);

/// The window list entry of a window, kept in its user data.
#[derive(Default)]
pub struct ListEntry(pub std::cell::RefCell<Option<ForeignToplevelHandle>>);

impl MistComp {
    /// A window was mapped: announce it.
    pub fn list_window(&mut self, w: &Window) {
        let (title, app) = (crate::hyprcompat::title(w), crate::hyprcompat::app_id(w));
        let h = self.extra.foreign_toplevels.new_toplevel::<MistComp>(&title, &app);
        w.user_data().insert_if_missing(ListEntry::default);
        *w.user_data().get::<ListEntry>().unwrap().0.borrow_mut() = Some(h);
    }

    /// Its title or app id changed.
    pub fn list_update(&mut self, w: &Window) {
        let Some(e) = w.user_data().get::<ListEntry>() else { return };
        if let Some(h) = e.0.borrow().as_ref() {
            h.send_title(&crate::hyprcompat::title(w));
            h.send_app_id(&crate::hyprcompat::app_id(w));
            h.send_done();
        }
    }

    /// It is gone.
    pub fn unlist_window(&mut self, w: &Window) {
        let Some(e) = w.user_data().get::<ListEntry>() else { return };
        if let Some(h) = e.0.borrow_mut().take() {
            h.send_closed();
            self.extra.foreign_toplevels.remove_toplevel(&h);
        }
    }
}

// ------------------------------------------------------------------ input
delegate_virtual_keyboard_manager!(MistComp);
delegate_text_input_manager!(MistComp);
delegate_tablet_manager!(MistComp);

impl InputMethodHandler for MistComp {
    fn new_popup(&mut self, surface: ImPopupSurface) {
        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("input method popup: {e}");
        }
    }
    fn popup_repositioned(&mut self, _surface: ImPopupSurface) {}
    fn dismiss_popup(&mut self, surface: ImPopupSurface) {
        if let Some(parent) = surface.get_parent().map(|p| p.surface.clone()) {
            let _ = smithay::desktop::PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }
    // where the text field's window is (the candidate popup is placed next to the cursor rectangle in it)
    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.space
            .elements()
            .find_map(|w| (w.wl_surface().as_deref() == Some(parent)).then(|| w.geometry()))
            .unwrap_or_default()
    }
}
delegate_input_method_manager!(MistComp);

impl XWaylandKeyboardGrabHandler for MistComp {
    fn keyboard_focus_for_xsurface(&self, surface: &WlSurface) -> Option<crate::focus::KbFocus> {
        self.space
            .elements()
            .find_map(|w| w.x11_surface().filter(|x| x.wl_surface().as_ref() == Some(surface)).cloned())
            .map(crate::focus::KbFocus::X11)
    }
}
delegate_xwayland_keyboard_grab!(MistComp);

// ------------------------------------------------------------------ clipboard managers (ext-data-control)
impl ExtDataControlHandler for MistComp {
    fn data_control_state(&self) -> &ExtDataControlState {
        &self.extra.ext_data_control
    }
}
delegate_ext_data_control!(MistComp);

// ------------------------------------------------------------------ small ones
impl XdgForeignHandler for MistComp {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.extra.xdg_foreign
    }
}
delegate_xdg_foreign!(MistComp);

impl XdgToplevelIconHandler for MistComp {}
delegate_xdg_toplevel_icon!(MistComp);

impl XdgToplevelTagHandler for MistComp {}
delegate_xdg_toplevel_tag!(MistComp);

delegate_content_type!(MistComp);
delegate_alpha_modifier!(MistComp);

impl XdgSystemBellHandler for MistComp {
    // mist-ui shows it (island): a terminal's bell, a dialog's beep
    fn ring(&mut self, surface: Option<WlSurface>) {
        let addr = surface
            .and_then(|s| self.space.elements().find(|w| w.wl_surface().as_deref() == Some(&s)).map(|w| crate::wm::data(w).addr.get()))
            .unwrap_or(0);
        self.hypr_event("bell", &format!("{addr:x}"));
    }
}
delegate_xdg_system_bell!(MistComp);

// ------------------------------------------------------------------ sandboxes (security-context)
impl SecurityContextHandler for MistComp {
    fn context_created(&mut self, source: SecurityContextListenerSource, context: SecurityContext) {
        let dh = self.display_handle.clone();
        let r = self.loop_handle.insert_source(source, move |stream, _, _| {
            let state = ClientState { security_context: Some(context.clone()), ..Default::default() };
            if let Err(e) = dh.clone().insert_client(stream, Arc::new(state)) {
                tracing::warn!("sandboxed client: {e}");
            }
        });
        if let Err(e) = r {
            tracing::warn!("security context listener: {e}");
        }
    }
}
delegate_security_context!(MistComp);
