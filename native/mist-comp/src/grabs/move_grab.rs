use crate::MistComp;
use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<MistComp>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
    /// cursor at the last motion (mistdrag>>end reports it)
    pub last: Point<f64, Logical>,
    /// when the last mistdrag>>move went out (one per 8 ms at most)
    pub sent: Option<std::time::Instant>,
}

impl PointerGrab<MistComp> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);
        self.last = event.location;
        // mist-ui's snap overlay follows this instead of polling the cursor
        if self.sent.is_none_or(|t| t.elapsed().as_millis() >= 8) {
            self.sent = Some(std::time::Instant::now());
            data.hypr_event("mistdrag", &format!("move,{},{}", event.location.x as i32, event.location.y as i32));
        }

        let delta = event.location - self.start_data.location;
        let mut new_location = self.initial_window_location.to_f64() + delta;
        // not over the bar / under the island: the title bar stays below the usable area's top
        if let Some(top) = data.usable_top(event.location) {
            new_location.y = new_location.y.max((top + data.bar_h(&self.window)) as f64);
        }
        data.space
            .map_element(self.window.clone(), new_location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        // The button is a button code as defined in the
        // Linux kernel's linux/input-event-codes.h header file, e.g. BTN_LEFT.
        const BTN_LEFT: u32 = 0x110;

        if !handle.current_pressed().contains(&BTN_LEFT) {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(self, data, event.serial, event.time, true);
            data.hypr_event("mistdrag", &format!("end,{},{}", self.last.x as i32, self.last.y as i32));
        }
    }

    fn axis(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut MistComp, handle: &mut PointerInnerHandle<'_, MistComp>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event)
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event)
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event)
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event)
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event)
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event)
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event)
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut MistComp,
        handle: &mut PointerInnerHandle<'_, MistComp>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<MistComp> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut MistComp) {}
}
