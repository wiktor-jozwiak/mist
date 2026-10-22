use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent, GestureEndEvent,
        GesturePinchUpdateEvent as GesturePinchUpdateEvent_, GestureSwipeUpdateEvent as GestureSwipeUpdateEvent_, InputBackend,
        InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult},
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
            GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent,
            RelativeMotionEvent,
        },
    },
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::pointer_constraints::{with_pointer_constraint, PointerConstraint},
};

use crate::config::{Action, Mods};
use crate::grabs::{resize_grab::ResizeEdge, ResizeSurfaceGrab};
use smithay::{
    input::pointer::{Focus, GrabStartData as PointerGrabStartData},
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::Rectangle,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

use crate::state::MistComp;

impl MistComp {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.dirty = true;
        self.idle.notify_activity(&self.seat);
        match event {
            InputEvent::Keyboard { event, .. } => {
                // once: keys do reach the compositor (not which ones: comp.log must not hold passwords)
                if !self.keys_seen {
                    self.keys_seen = true;
                    tracing::info!("keyboard input arrives");
                }
                self.handle_key(event.key_code(), event.state(), Event::time_msec(&event));
            }
            InputEvent::PointerMotion { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let mut pos = pointer.current_location();
                let under = self.surface_under(pos);
                let serial = SERIAL_COUNTER.next_serial();

                // a game that locked the pointer only gets relative motion
                let mut locked = false;
                let mut confined = false;
                if let Some((surface, _)) = &under {
                    with_pointer_constraint(surface, &pointer, |c| {
                        if let Some(c) = c.filter(|c| c.is_active()) {
                            match &*c {
                                PointerConstraint::Locked(_) => locked = true,
                                PointerConstraint::Confined(_) => confined = true,
                            }
                        }
                    });
                }
                pointer.relative_motion(
                    self,
                    under.clone(),
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );
                if locked {
                    pointer.frame(self);
                    return;
                }

                pos += event.delta();
                // clamp to the nearest output (not their bounding box: monitors of different sizes or offsets leave
                // corners no output shows, where the cursor would vanish)
                pos = self.clamp_to_outputs(pos);

                let new_under = self.surface_under(pos);
                if confined && new_under.as_ref().map(|u| &u.0) != under.as_ref().map(|u| &u.0) {
                    pointer.frame(self);
                    return;
                }
                self.bar_drag_motion(pos);
                pointer.motion(self, new_under.clone(), &MotionEvent { location: pos, serial, time: event.time_msec() });
                self.activate_constraint(new_under.as_ref(), pos);
                pointer.frame(self);
                self.bar_hover(pos);
                self.edge_hover(pos);
                self.note_pointer_output();
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                // tablets / VM pointers map onto the first monitor (none plugged in: nowhere to go)
                let Some(output_geo) = self.space.outputs().next().and_then(|o| self.space.output_geometry(o)) else { return };
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
                self.pointer_to(pos, event.time_msec());
            }
            InputEvent::PointerButton { event, .. } => {
                self.handle_button(event.button_code(), event.state(), event.time_msec());
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                // wheel binds (SUPER+mouse_down: previous workspace...): one action per notch, the client gets nothing
                if source == AxisSource::Wheel && event.amount_v120(Axis::Vertical).is_some_and(|v| self.wheel_bind(v)) {
                    return;
                }

                let horizontal_amount = event
                    .amount(Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            // touchpad gestures: an N-finger horizontal swipe switches workspaces (`swipe N` in comp.conf,
            // default 3); everything else goes to the client under the pointer (pinch zoom, hold, 2-finger swipes)
            InputEvent::GestureSwipeBegin { event, .. } => {
                let fingers = GestureBeginEvent::fingers(&event);
                if self.config.swipe_fingers != 0 && fingers == self.config.swipe_fingers && !self.locked {
                    self.swipe = Some(crate::state::Swipe::default());
                    return;
                }
                let p = self.seat.get_pointer().unwrap();
                p.gesture_swipe_begin(self, &GestureSwipeBeginEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), fingers });
                p.frame(self);
            }
            InputEvent::GestureSwipeUpdate { event, .. } => {
                if let Some(s) = self.swipe.as_mut() {
                    s.last = GestureSwipeUpdateEvent_::delta_x(&event);
                    s.dx += s.last;
                    s.dy += GestureSwipeUpdateEvent_::delta_y(&event);
                    self.swipe_update();
                    return;
                }
                let p = self.seat.get_pointer().unwrap();
                p.gesture_swipe_update(self, &GestureSwipeUpdateEvent { time: event.time_msec(), delta: GestureSwipeUpdateEvent_::delta(&event) });
                p.frame(self);
            }
            InputEvent::GestureSwipeEnd { event, .. } => {
                if self.swipe.is_some() {
                    self.swipe_end(GestureEndEvent::cancelled(&event));
                    return;
                }
                let p = self.seat.get_pointer().unwrap();
                p.gesture_swipe_end(self, &GestureSwipeEndEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), cancelled: GestureEndEvent::cancelled(&event) });
                p.frame(self);
            }
            InputEvent::GesturePinchBegin { event, .. } => {
                let p = self.seat.get_pointer().unwrap();
                p.gesture_pinch_begin(self, &GesturePinchBeginEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), fingers: GestureBeginEvent::fingers(&event) });
                p.frame(self);
            }
            InputEvent::GesturePinchUpdate { event, .. } => {
                let p = self.seat.get_pointer().unwrap();
                p.gesture_pinch_update(
                    self,
                    &GesturePinchUpdateEvent {
                        time: event.time_msec(),
                        delta: GesturePinchUpdateEvent_::delta(&event),
                        scale: GesturePinchUpdateEvent_::scale(&event),
                        rotation: GesturePinchUpdateEvent_::rotation(&event),
                    },
                );
                p.frame(self);
            }
            InputEvent::GesturePinchEnd { event, .. } => {
                let p = self.seat.get_pointer().unwrap();
                p.gesture_pinch_end(self, &GesturePinchEndEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), cancelled: GestureEndEvent::cancelled(&event) });
                p.frame(self);
            }
            InputEvent::GestureHoldBegin { event, .. } => {
                let p = self.seat.get_pointer().unwrap();
                p.gesture_hold_begin(self, &GestureHoldBeginEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), fingers: GestureBeginEvent::fingers(&event) });
                p.frame(self);
            }
            InputEvent::GestureHoldEnd { event, .. } => {
                let p = self.seat.get_pointer().unwrap();
                p.gesture_hold_end(self, &GestureHoldEndEvent { serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(), cancelled: GestureEndEvent::cancelled(&event) });
                p.frame(self);
            }
            _ => {}
        }
    }

    /// Type a shortcut into the focused client (Hyprland's send_shortcut; Spotlight pastes emoji with it):
    /// mods like "CTRL SHIFT", key a letter or digit. Goes straight to the client, binds are not checked.
    pub fn send_shortcut(&mut self, mods: &str, key: &str) {
        const LETTERS: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
        const CODES: [u32; 26] = [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44];
        let k = key.to_ascii_lowercase();
        let code = match k.as_bytes() {
            [c] if c.is_ascii_lowercase() => CODES[LETTERS.iter().position(|x| x == c).unwrap()],
            [b'0'] => 11,
            [c] if (b'1'..=b'9').contains(c) => (c - b'0') as u32 + 1,
            _ => match k.as_str() {
                "return" | "enter" => 28,
                "tab" => 15,
                "escape" => 1,
                "space" => 57,
                _ => return,
            },
        };
        let mut seq: Vec<u32> = mods
            .split(|c: char| c == ' ' || c == '+' || c == '_')
            .filter_map(|m| match m.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => Some(29),
                "SHIFT" => Some(42),
                "ALT" => Some(56),
                "SUPER" | "META" | "WIN" => Some(125),
                _ => None,
            })
            .collect();
        seq.push(code);
        let Some(kb) = self.seat.get_keyboard() else { return };
        let time = self.start_time.elapsed().as_millis() as u32;
        let press = |st: &mut Self, c: u32, state: KeyState| {
            kb.input::<(), _>(st, smithay::input::keyboard::Keycode::new(c + 8), state, SERIAL_COUNTER.next_serial(), time, |_, _, _| FilterResult::Forward);
        };
        for &c in &seq {
            press(self, c, KeyState::Pressed);
        }
        for &c in seq.iter().rev() {
            press(self, c, KeyState::Released);
        }
    }

    pub fn handle_key(&mut self, code: smithay::input::keyboard::Keycode, key_state: KeyState, time: u32) {
        let serial = SERIAL_COUNTER.next_serial();
        let pressed = key_state == KeyState::Pressed;

        let kc: u32 = code.into();
        let action = self.seat.get_keyboard().unwrap().input::<Action, _>(
            self,
            code,
            key_state,
            serial,
            time,
            |st, mods, handle| {
                {
                    let x = handle.xkb().lock().unwrap();
                    let name = x.layout_name(x.active_layout());
                    if st.ev_layout != name {
                        let first = st.ev_layout.is_empty();
                        st.ev_layout = name.to_string();
                        if !first {
                            st.hypr_event("activelayout", &format!("keyboard,{name}"));
                        }
                    }
                }
                let m = Mods::from(mods);
                let modified = handle.modified_sym().raw();
                let sym = handle.raw_latin_sym_or_raw_current_sym().map(|s| s.raw()).unwrap_or(modified);
                let is_super = sym == keysyms::KEY_Super_L || sym == keysyms::KEY_Super_R;
                if pressed {
                    st.super_alone = is_super;
                    // Ctrl+Alt+Fn produces XF86Switch_VT_n
                    if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&modified) {
                        st.suppressed.insert(kc);
                        return FilterResult::Intercept(Action::Vt((modified - keysyms::KEY_XF86Switch_VT_1 + 1) as i32));
                    }
                    if sym == keysyms::KEY_Escape && st.overview_open() {
                        st.suppressed.insert(kc);
                        return FilterResult::Intercept(Action::Overview);
                    }
                    if let Some(b) = st.config.find(m, sym, false) {
                        if !st.locked || b.locked {
                            st.suppressed.insert(kc);
                            if let Action::Global(n) = &b.action {
                                st.globals_held.insert(kc, n.clone());
                            }
                            return FilterResult::Intercept(b.action.clone());
                        }
                    }
                } else {
                    // release binds (Super alone opens Start): fire only if nothing else was pressed meanwhile
                    let alone = std::mem::take(&mut st.super_alone);
                    if is_super && alone {
                        let m = Mods { logo: true, ..m };
                        if let Some(b) = st.config.find(m, sym, true).filter(|b| !st.locked || b.locked) {
                            let a = b.action.clone();
                            if st.suppressed.remove(&kc) {
                                return FilterResult::Intercept(a);
                            }
                            // the press reached the client: let the release through too
                            st.do_action(a);
                            return FilterResult::Forward;
                        }
                    }
                    if st.suppressed.remove(&kc) {
                        // a held `global` bind: its app hears the release (whatever modifiers are up by now)
                        if let Some(n) = st.globals_held.remove(&kc) {
                            return FilterResult::Intercept(Action::GlobalUp(n));
                        }
                        return FilterResult::Intercept(Action::Nothing);
                    }
                }
                FilterResult::Forward
            },
        );
        if let Some(a) = action {
            self.do_action(a);
        }
    }

    /// The point itself when an output shows it, else the closest point of the closest output.
    pub fn clamp_to_outputs(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        self.space
            .outputs()
            .filter_map(|o| self.space.output_geometry(o))
            .map(|g| {
                let x = pos.x.clamp(g.loc.x as f64, (g.loc.x + g.size.w - 1) as f64);
                let y = pos.y.clamp(g.loc.y as f64, (g.loc.y + g.size.h - 1) as f64);
                Point::from((x, y))
            })
            .min_by(|a, b| {
                let d = |p: &Point<f64, Logical>| (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2);
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(pos)
    }

    pub fn pointer_to(&mut self, pos: Point<f64, Logical>, time: u32) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        let under = self.surface_under(pos);
        // the magnified view follows the pointer
        if self.anim.zoom > 1.0 {
            self.dirty = true;
        }
        self.bar_drag_motion(pos);
        pointer.motion(self, under.clone(), &MotionEvent { location: pos, serial, time });
        self.activate_constraint(under.as_ref(), pos);
        pointer.frame(self);
        self.bar_hover(pos);
        self.edge_hover(pos);
        self.note_pointer_output();
    }

    /// The pointer is over a surface whose lock / confinement waits (a game it came back to after
    /// Alt+Tab): it takes effect where its region allows.
    fn activate_constraint(&mut self, under: Option<&(smithay::reexports::wayland_server::protocol::wl_surface::WlSurface, Point<f64, Logical>)>, pos: Point<f64, Logical>) {
        let Some((surface, origin)) = under else { return };
        let pointer = self.seat.get_pointer().unwrap();
        with_pointer_constraint(surface, &pointer, |c| {
            if let Some(c) = c.filter(|c| !c.is_active()) {
                let at = (pos - *origin).to_i32_round();
                if c.region().is_none_or(|r| r.contains(at)) {
                    c.activate();
                }
            }
        });
    }

    fn bar_drag_motion(&mut self, pos: Point<f64, Logical>) {
        if self.bar_drag.as_ref().is_some_and(|(_, p, _)| (pos.x - p.x).abs() + (pos.y - p.y).abs() > 4.0) {
            let (w, _, button) = self.bar_drag.take().unwrap();
            self.start_move(w, button, SERIAL_COUNTER.next_serial());
        }
    }

    /// A wheel notch (v120, > 0 = down) on a bind: run it, true = the client gets nothing.
    pub fn wheel_bind(&mut self, v120: f64) -> bool {
        if self.locked || v120 == 0.0 {
            return false;
        }
        let m = self.seat.get_keyboard().map(|k| Mods::from(&k.modifier_state())).unwrap_or_default();
        let sym = if v120 > 0.0 { crate::config::MOUSE_DOWN } else { crate::config::MOUSE_UP };
        let Some(a) = self.config.binds.iter().find(|b| b.sym == sym && b.mods == m).map(|b| b.action.clone()) else { return false };
        self.super_alone = false;
        self.do_action(a);
        true
    }

    pub fn handle_button(&mut self, button: u32, button_state: ButtonState, time: u32) {
        // Super+click / Super+drag is not "Super alone"
        self.super_alone = false;
        // the overview's clicks pick a window, no client hears them
        if button_state == ButtonState::Pressed && self.overview_open() {
            let at = self.seat.get_pointer().unwrap().current_location();
            if self.overview_click(at) {
                self.swallow_button = Some(button);
                return;
            }
        }
        if button_state == ButtonState::Released && self.swallow_button == Some(button) {
            self.swallow_button = None;
            return;
        }
        let pointer = self.seat.get_pointer().unwrap();
        let keyboard = self.seat.get_keyboard().unwrap();

        let serial = SERIAL_COUNTER.next_serial();



        // Super + left drag moves, Super + right drag resizes (from the nearest corner)
        // ALT works like Super here (windows-like.lua: ALT + mouse:272 / 273)
        let logo = keyboard.modifier_state().logo || keyboard.modifier_state().alt;
        if logo && button_state == ButtonState::Pressed && !pointer.is_grabbed() && (button == BTN_LEFT || button == BTN_RIGHT) {
            let loc = pointer.current_location();
            // element_under gives where the surface is drawn (CSD shadows included); the grabs work from the
            // window geometry's origin, as an xdg resize request does
            let hit = self.space.element_under(loc).map(|(w, _)| w.clone()).and_then(|w| Some((self.space.element_location(&w)?, w)));
            if let Some((wloc, window)) = hit {
                self.focus_window(Some(&window));
                let start_data = PointerGrabStartData { focus: None, button, location: loc };
                if button == BTN_LEFT {
                    let _ = (start_data, wloc);
                    self.start_move(window, button, serial);
                } else {
                    let size = window.geometry().size;
                    let rel = loc - wloc.to_f64();
                    let mut edges = if rel.x < size.w as f64 / 2.0 { ResizeEdge::LEFT } else { ResizeEdge::RIGHT };
                    edges |= if rel.y < size.h as f64 / 2.0 { ResizeEdge::TOP } else { ResizeEdge::BOTTOM };
                    if let Some(t) = window.toplevel() {
                        t.with_pending_state(|s| s.states.set(xdg_toplevel::State::Resizing));
                        t.send_pending_configure();
                    }
                    let grab = ResizeSurfaceGrab::start(start_data, window, edges, Rectangle::new(wloc, size));
                    pointer.set_grab(self, grab, serial, Focus::Clear);
                }
                return;
            }
        }

        // a window frame: resize from that edge / corner
        if !pointer.is_grabbed() && button == BTN_LEFT && button_state == ButtonState::Pressed {
            let loc = pointer.current_location();
            if let Some((window, edges, wloc)) = self.edge_at(loc) {
                self.focus_window(Some(&window));
                if let Some(t) = window.toplevel() {
                    t.with_pending_state(|s| s.states.set(xdg_toplevel::State::Resizing));
                    t.send_pending_configure();
                }
                let size = window.geometry().size;
                let start_data = PointerGrabStartData { focus: None, button, location: loc };
                let grab = ResizeSurfaceGrab::start(start_data, window, edges, Rectangle::new(wloc, size));
                pointer.set_grab(self, grab, serial, Focus::Clear);
                return;
            }
        }

        // macOS title bar: traffic lights act on release over the same button, drag moves, double click zooms
        if !self.locked && !pointer.is_grabbed() && button == BTN_LEFT {
            let loc = pointer.current_location();
            let layer_over = self.layer_at(loc).is_some();
            if button_state == ButtonState::Released {
                self.bar_drag = None;
            }
            if let Some((window, hit, _)) = self.bar_at(loc).filter(|_| !layer_over) {
                if button_state == ButtonState::Pressed {
                    self.focus_window(Some(&window));
                    let now = std::time::Instant::now();
                    let double = self.bar_click.take().is_some_and(|(a, t)| a == crate::wm::data(&window).addr.get() && now - t < std::time::Duration::from_millis(400));
                    match hit {
                        crate::decor::Hit::Drag if double => self.mist_maximize(&window),
                        crate::decor::Hit::Drag => {
                            self.bar_click = Some((crate::wm::data(&window).addr.get(), now));
                            self.bar_drag = Some((window, loc, button));
                        }
                        h => self.bar_press = Some((crate::wm::data(&window).addr.get(), h)),
                    }
                } else if let Some((addr, h)) = self.bar_press.take() {
                    if addr == crate::wm::data(&window).addr.get() && h == hit {
                        match h {
                            crate::decor::Hit::Close => {
                                if let Some(t) = window.toplevel() {
                                    t.send_close();
                                } else if let Some(x) = window.x11_surface() {
                                    let _ = x.close();
                                }
                            }
                            crate::decor::Hit::Min => self.minimize(&window),
                            crate::decor::Hit::Max => self.mist_maximize(&window),
                            crate::decor::Hit::Drag => {}
                        }
                    }
                }
                self.dirty = true;
                return;
            }
            if button_state == ButtonState::Released {
                self.bar_press = None;
            }
        }

        // the top strip of an app that cannot move itself (decor.rs DRAG_TOP): the press goes to the app, moving
        // past the bar_drag threshold before the release moves the window. The release clears it even while
        // the client's click grab is held
        if button == BTN_LEFT && button_state == ButtonState::Released {
            self.bar_drag = None;
        }
        if !self.locked && button == BTN_LEFT && button_state == ButtonState::Pressed && !pointer.is_grabbed() {
            let loc = pointer.current_location();
            if self.layer_at(loc).is_none() {
                let hit = self.space.element_under(loc).map(|(w, _)| w.clone()).and_then(|w| Some((self.space.element_location(&w)?, w)));
                if let Some((wl, w)) = hit {
                    let top = crate::decor::drag_top(&crate::hyprcompat::app_id(&w)) as f64;
                    let y = loc.y - wl.y as f64;
                    if (0.0..top).contains(&y) {
                        self.bar_drag = Some((w, loc, button));
                    }
                }
            }
        }

        let layer_hit = (ButtonState::Pressed == button_state && !pointer.is_grabbed())
            .then(|| self.layer_at(pointer.current_location()))
            .flatten();
        if let Some(layer) = layer_hit {
            // a click on a shell surface (bar, dock, popup) keeps window focus unless the layer takes keys
            if layer.can_receive_keyboard_focus() {
                keyboard.set_focus(self, Some(layer.wl_surface().clone().into()), serial);
            }
        } else if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
            let hit = self.space.element_under(pointer.current_location()).map(|(w, _)| w.clone());
            // X11 menus (override-redirect) never take the focus
            if hit.as_ref().is_none_or(|w| crate::wm::data(w).addr.get() != 0) {
                self.focus_window(hit.as_ref());
            }
        };

        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }
}

/// Finger travel (touchpad units, about logical px) for a whole workspace slide; Hyprland's
/// workspace_swipe_distance default.
const SWIPE_DISTANCE: f64 = 300.0;

impl MistComp {
    /// Fingers moving left bring the next workspace (macOS, Hyprland's default direction): it is switched at
    /// the first movement and the slide follows the fingers.
    pub fn swipe_update(&mut self) {
        let Some(s) = self.swipe.as_mut() else { return };
        let dx = s.dx;
        // up or down first: the overview (on release)
        if !s.tried && s.dy.abs() > dx.abs() && s.dy.abs() > 12.0 {
            s.tried = true;
            s.vertical = true;
        }
        if s.vertical || !crate::anim::enabled() {
            return;
        }
        if s.on.is_none() {
            if s.tried || dx.abs() < 12.0 {
                return;
            }
            self.swipe.as_mut().unwrap().tried = true;
            let Some(o) = self.active_output() else { return };
            let dir = if dx < 0.0 { 1 } else { -1 };
            let from = self.active_ws(&o);
            let to = self.resolve(&o, crate::config::WsArg::Rel(dir));
            if to == from {
                return;
            }
            self.switch_ws(&o, to);
            // shown on another monitor instead: no slide to follow
            if self.active_ws(&o) != to {
                return;
            }
            self.swipe.as_mut().unwrap().on = Some((o, dir, from));
        }
        let Some((o, dir, _)) = self.swipe.as_ref().and_then(|s| s.on.clone()) else { return };
        self.anim.ws_drag(&o.name(), -dx * dir as f64 / SWIPE_DISTANCE);
        self.dirty = true;
    }

    /// Fingers lifted: past a third of the way (or still moving on) the slide finishes, else the old
    /// workspace slides back from where the fingers left it.
    pub fn swipe_end(&mut self, cancelled: bool) {
        let Some(s) = self.swipe.take() else { return };
        if s.vertical {
            // three fingers up: Mission Control, down: back
            if !cancelled && (s.dy < -50.0) != self.overview_open() && s.dy.abs() > 50.0 {
                self.overview_toggle();
            }
            return;
        }
        let Some((o, dir, from)) = s.on else {
            if !crate::anim::enabled() && !cancelled && s.dx.abs() > 120.0 {
                self.do_action(Action::Workspace(crate::config::WsArg::Rel(if s.dx < 0.0 { 1 } else { -1 })));
            }
            return;
        };
        let p = (-s.dx * dir as f64 / SWIPE_DISTANCE).clamp(0.0, 1.0);
        let onward = -s.last * dir as f64;
        let name = o.name();
        if !cancelled && onward > -4.0 && (p > 0.33 || onward > 6.0) {
            self.anim.ws_release(&name);
        } else {
            self.switch_ws(&o, from);
            self.anim.ws_drag(&name, 1.0 - p);
            self.anim.ws_release(&name);
        }
        self.dirty = true;
    }
}
