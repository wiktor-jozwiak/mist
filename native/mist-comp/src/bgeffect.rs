//! ext-background-effect-v1: a client says where on its surface it wants what is behind blurred. Mist's own
//! panels (mist-ui) mark their glass with it, so a full-monitor surface (Spotlight, the session menu) blurs only
//! under its cards instead of the whole monitor; other clients get glass under their region the same way.
//! The region is double-buffered (applied on wl_surface.commit); the blur keeps to its bounding box, the
//! surface's alpha still decides where the glass shows inside it.
use smithay::{
    reexports::{
        wayland_protocols::ext::background_effect::v1::server::{
            ext_background_effect_manager_v1::{self as mgr, ExtBackgroundEffectManagerV1},
            ext_background_effect_surface_v1::{self as bes, ExtBackgroundEffectSurfaceV1},
        },
        wayland_server::{protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak},
    },
    utils::{Logical, Rectangle},
    wayland::compositor::{get_region_attributes, with_states, Cacheable, RectangleKind},
};

use crate::MistComp;

/// The surface's blur region: None no blur object (or it was destroyed), Some(None) an empty or null region
/// (no blur), Some(Some(r)) the bounding box of the region (surface-local).
#[derive(Default, Clone, Copy)]
pub struct BlurRegion(Option<Option<Rectangle<i32, Logical>>>);

impl Cacheable for BlurRegion {
    fn commit(&mut self, _: &DisplayHandle) -> Self {
        *self
    }
    fn merge_into(self, into: &mut Self, _: &DisplayHandle) {
        *into = self;
    }
}

/// The blur object's surface (gone with the surface).
pub struct Data(Weak<WlSurface>);

/// A surface's blur object, so a second get_background_effect is an error.
#[derive(Default)]
struct Has(std::sync::atomic::AtomicBool);

pub fn init(dh: &DisplayHandle) {
    dh.create_global::<MistComp, ExtBackgroundEffectManagerV1, _>(1, ());
}

/// What the client asked for on this surface (its current state): None: nothing (Mist's own rules decide),
/// Some(None): no blur, Some(Some(r)): blur inside r (surface-local).
pub fn region(s: &WlSurface) -> Option<Option<Rectangle<i32, Logical>>> {
    with_states(s, |st| st.cached_state.get::<BlurRegion>().current().0)
}

fn set(s: &WlSurface, r: Option<Option<Rectangle<i32, Logical>>>) {
    with_states(s, |st| st.cached_state.get::<BlurRegion>().pending().0 = r);
}

impl GlobalDispatch<ExtBackgroundEffectManagerV1, ()> for MistComp {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, res: New<ExtBackgroundEffectManagerV1>, _: &(), di: &mut DataInit<'_, Self>) {
        let m = di.init(res, ());
        m.capabilities(mgr::Capability::Blur);
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for MistComp {
    fn request(_: &mut Self, _: &Client, m: &ExtBackgroundEffectManagerV1, req: mgr::Request, _: &(), _: &DisplayHandle, di: &mut DataInit<'_, Self>) {
        if let mgr::Request::GetBackgroundEffect { id, surface } = req {
            let taken = with_states(&surface, |st| st.data_map.get_or_insert_threadsafe(Has::default).0.swap(true, std::sync::atomic::Ordering::AcqRel));
            if taken {
                m.post_error(mgr::Error::BackgroundEffectExists, "the surface has a background effect object");
                return;
            }
            di.init(id, Data(surface.downgrade()));
        }
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, Data> for MistComp {
    fn request(_: &mut Self, _: &Client, o: &ExtBackgroundEffectSurfaceV1, req: bes::Request, d: &Data, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        let Ok(s) = d.0.upgrade() else {
            if !matches!(req, bes::Request::Destroy) {
                o.post_error(bes::Error::SurfaceDestroyed, "the surface is gone");
            }
            return;
        };
        match req {
            bes::Request::SetBlurRegion { region } => {
                // the bounding box of what the region adds (what it subtracts can only shrink it)
                let bbox = region.map(|r| get_region_attributes(&r)).and_then(|a| {
                    a.rects.iter().filter(|(k, _)| matches!(k, RectangleKind::Add)).map(|(_, r)| *r).filter(|r| !r.is_empty()).reduce(|a, b| a.merge(b))
                });
                set(&s, Some(bbox));
            }
            bes::Request::Destroy => {
                set(&s, None);
                with_states(&s, |st| st.data_map.get::<Has>().map(|h| h.0.store(false, std::sync::atomic::Ordering::Release)));
            }
            _ => {}
        }
    }
}
