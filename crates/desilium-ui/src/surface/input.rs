//! Pointer, keyboard and focus input, and the extraction of `data-action` clicks.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use atomic_refcell::AtomicRefCell;
use blitz_dom::{EventDriver, EventHandler, LocalName, NodeId, local_name};
use blitz_traits::{
    SmolStr,
    events::{
        BlitzImeEvent, BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta,
        BlitzWheelEvent, DomEvent, DomEventData, EventState, KeyState, MouseEventButton,
        MouseEventButtons, Point, PointerCoords, PointerDetails, UiEvent as BlitzUiEvent,
    },
};
use cursor_icon::CursorIcon;
use keyboard_types::{Code, Location};

use super::UiSurface;
use crate::input::{
    CursorHint, HoverTarget, KeyEvent, PointerButton, Rect, UiAction, UiEvent, WheelDelta,
};

/// Ancestor hops allowed when looking for an action element (guards against cycles).
const MAX_DEPTH: usize = 64;
const ACTION_ATTR: &str = "data-action";
const ARG_ATTR: &str = "data-arg";
const TOOLTIP_ATTR: &str = "data-tooltip";
const SUBMIT_ATTR: &str = "data-submit";
const KEY_ATTR: &str = "data-key";

#[derive(Default)]
pub(super) struct PointerState {
    position: Option<(f32, f32)>,
    pressed_action: Option<NodeId>,
    buttons: MouseEventButtons,
}

/// Collects the `input` events Blitz emits while it edits a text field.
#[derive(Clone, Default)]
struct InputCollector(Rc<RefCell<Vec<(NodeId, String)>>>);

impl EventHandler for InputCollector {
    fn handle_event(
        &mut self,
        _chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn blitz_dom::Document,
        _state: &mut EventState,
    ) {
        if let DomEventData::Input(input) = &event.data {
            self.0
                .borrow_mut()
                .push((event.target, input.value.clone()));
        }
    }
}

fn to_button(button: PointerButton) -> MouseEventButton {
    match button {
        PointerButton::Primary => MouseEventButton::Main,
        PointerButton::Secondary => MouseEventButton::Secondary,
        PointerButton::Middle => MouseEventButton::Auxiliary,
    }
}

fn pointer_event(
    (x, y): (f32, f32),
    button: MouseEventButton,
    buttons: MouseEventButtons,
) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods: keyboard_types::Modifiers::empty(),
        details: PointerDetails::default(),
        element: Point { x: 0.0, y: 0.0 },
        active_pointers: Arc::new(AtomicRefCell::new(Vec::new())),
    }
}

fn key_event(event: &KeyEvent, state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: event.key.to_backend(),
        code: Code::Unidentified,
        modifiers: event.modifiers.to_backend(),
        location: Location::Standard,
        is_auto_repeating: event.repeat,
        is_composing: false,
        state,
        text: event.text.as_deref().map(SmolStr::new),
    }
}

impl UiSurface {
    fn attr_of(&self, id: NodeId, name: &str) -> Option<String> {
        let node = self.doc.get_node(id)?;
        node.attr(LocalName::from(name)).map(str::to_owned)
    }

    /// First element at or above `start` that has attribute `name`.
    fn ancestor_with(&self, start: NodeId, name: &str) -> Option<NodeId> {
        let mut current = Some(start);
        for _ in 0..MAX_DEPTH {
            let id = current?;
            let node = self.doc.get_node(id)?;
            if node.attr(LocalName::from(name)).is_some() {
                return Some(id);
            }
            current = node.parent;
        }
        None
    }

    fn is_disabled(&self, id: NodeId) -> bool {
        let Some(node) = self.doc.get_node(id) else {
            return false;
        };
        node.attr(local_name!("disabled")).is_some()
            || self.attr_of(id, "aria-disabled").as_deref() == Some("true")
    }

    fn action_node_at(&mut self, x: f32, y: f32) -> Option<NodeId> {
        self.ensure_layout();
        let hit = self.doc.hit(x, y)?;
        let node = self.ancestor_with(hit.node_id, ACTION_ATTR)?;
        (!self.is_disabled(node)).then_some(node)
    }

    fn rect_of(&self, id: NodeId) -> Rect {
        self.doc
            .get_client_bounding_rect(id)
            .map(|r| Rect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .unwrap_or_default()
    }

    fn action_for(&self, id: NodeId) -> Option<UiAction> {
        Some(UiAction {
            action: self.attr_of(id, ACTION_ATTR)?,
            arg: self.attr_of(id, ARG_ATTR),
            element_id: self.attr_of(id, "id"),
            rect: self.rect_of(id),
        })
    }

    fn dispatch(&mut self, event: BlitzUiEvent) {
        let collector = InputCollector::default();
        let seen = collector.0.clone();
        let mut driver = EventDriver::new(&mut self.doc, collector);
        driver.handle_ui_event(event);
        for (node, value) in seen.take() {
            self.events.push_back(UiEvent::TextChanged {
                element_id: self.attr_of(node, "id"),
                key: self.attr_of(node, KEY_ATTR),
                value,
            });
        }
    }

    /// Pointer moved to `(x, y)` in logical pixels; updates hover styles.
    pub fn pointer_move(&mut self, x: f64, y: f64) {
        let pos = (x as f32, y as f32);
        self.ensure_layout();
        let before = self.doc.get_hover_node_id();
        self.pointer.position = Some(pos);
        let event = pointer_event(pos, MouseEventButton::Main, self.pointer.buttons);
        self.dispatch(BlitzUiEvent::PointerMove(event));
        if self.doc.get_hover_node_id() != before {
            self.mark_dirty();
        }
    }

    /// Pointer left the surface; clears hover styles.
    pub fn pointer_leave(&mut self) {
        let pos = self.pointer.position.take().unwrap_or_default();
        let event = pointer_event(pos, MouseEventButton::Main, MouseEventButtons::None);
        self.pointer.pressed_action = None;
        self.pointer.buttons = MouseEventButtons::None;
        self.dispatch(BlitzUiEvent::PointerLeave(event));
        self.mark_dirty();
    }

    /// A pointer button was pressed at `(x, y)`.
    pub fn pointer_down(&mut self, x: f64, y: f64, button: PointerButton) {
        let pos = (x as f32, y as f32);
        let native = to_button(button);
        self.pointer.buttons |= MouseEventButtons::from(native);
        self.pointer.pressed_action = match button {
            PointerButton::Primary => self.action_node_at(pos.0, pos.1),
            _ => None,
        };
        let event = pointer_event(pos, native, self.pointer.buttons);
        self.dispatch(BlitzUiEvent::PointerDown(event));
        self.mark_dirty();
    }

    /// A pointer button was released at `(x, y)`; completes a click on an action element.
    pub fn pointer_up(&mut self, x: f64, y: f64, button: PointerButton) {
        let pos = (x as f32, y as f32);
        let native = to_button(button);
        self.pointer.buttons.remove(MouseEventButtons::from(native));
        let event = pointer_event(pos, native, self.pointer.buttons);
        self.dispatch(BlitzUiEvent::PointerUp(event));
        self.mark_dirty();
        let pressed = self.pointer.pressed_action.take();
        if button != PointerButton::Primary || pressed.is_none() {
            return;
        }
        if self.action_node_at(pos.0, pos.1) != pressed {
            return;
        }
        if let Some(action) = pressed.and_then(|id| self.action_for(id)) {
            self.events.push_back(UiEvent::Action(action));
        }
    }

    /// Wheel or touchpad scroll at `(x, y)`.
    pub fn wheel(&mut self, x: f64, y: f64, delta: WheelDelta) {
        let pos = (x as f32, y as f32);
        let delta = match delta {
            WheelDelta::Lines { x, y } => BlitzWheelDelta::Lines(x, y),
            WheelDelta::Pixels { x, y } => BlitzWheelDelta::Pixels(x, y),
        };
        let coords = pointer_event(pos, MouseEventButton::Main, self.pointer.buttons).coords;
        self.dispatch(BlitzUiEvent::Wheel(BlitzWheelEvent {
            delta,
            coords,
            buttons: self.pointer.buttons,
            mods: keyboard_types::Modifiers::empty(),
            element: Point { x: 0.0, y: 0.0 },
        }));
        self.mark_dirty();
    }

    /// A key was pressed (or repeated). Typing goes to the focused `<input>`.
    pub fn key_down(&mut self, event: &KeyEvent) {
        let submit = self.submit_event(event);
        self.dispatch(BlitzUiEvent::KeyDown(key_event(event, KeyState::Pressed)));
        self.events.extend(submit);
        self.mark_dirty();
    }

    /// A key was released.
    pub fn key_up(&mut self, event: &KeyEvent) {
        self.dispatch(BlitzUiEvent::KeyUp(key_event(event, KeyState::Released)));
    }

    /// Input-method composition text (underlined preedit) for the focused input.
    ///
    /// `cursor` is the byte range of the caret inside `text`, `None` hides it.
    /// An empty `text` clears the preedit.
    pub fn ime_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        self.dispatch(BlitzUiEvent::Ime(BlitzImeEvent::Preedit(
            text.to_owned(),
            cursor,
        )));
        self.mark_dirty();
    }

    /// Text committed by an input method; inserted at the caret of the focused input.
    pub fn ime_commit(&mut self, text: &str) {
        self.dispatch(BlitzUiEvent::Ime(BlitzImeEvent::Commit(text.to_owned())));
        self.mark_dirty();
    }

    fn submit_event(&self, event: &KeyEvent) -> Option<UiEvent> {
        if event.key != crate::input::Key::Enter || event.repeat {
            return None;
        }
        let focused = self.doc.get_focussed_node_id()?;
        let action = self.attr_of(focused, SUBMIT_ATTR)?;
        Some(UiEvent::Submit {
            action,
            value: self.input_value_of(focused).unwrap_or_default(),
        })
    }

    fn input_value_of(&self, id: NodeId) -> Option<String> {
        let data = self.doc.get_node(id)?.element_data()?.text_input_data()?;
        Some(data.editor.text().to_string())
    }

    /// Keyboard focus entered (`true`) or left (`false`) the surface.
    ///
    /// Losing focus hides the caret and remembers the focused element; gaining
    /// it back restores that element.
    pub fn focus_changed(&mut self, focused: bool) {
        if focused {
            if let Some(id) = self.focus_before_leave.take() {
                self.doc.set_focus_to(id);
            }
        } else {
            self.focus_before_leave = self.doc.get_focussed_node_id();
            self.doc.clear_focus();
        }
        self.mark_dirty();
    }

    /// Focuses the element matching `selector` (typically a search `<input>`).
    pub fn focus(&mut self, selector: &str) -> Result<(), crate::UiError> {
        let id = *self
            .find_all(selector)?
            .first()
            .ok_or_else(|| crate::UiError::NoMatch(selector.to_owned()))?;
        self.doc.set_focus_to(id);
        self.mark_dirty();
        Ok(())
    }

    /// Current text of the `<input>` matching `selector`.
    pub fn input_value(&self, selector: &str) -> Result<String, crate::UiError> {
        let ids = self.find_all(selector)?;
        let first = ids.first().copied();
        first
            .and_then(|id| self.input_value_of(id))
            .ok_or_else(|| crate::UiError::NoMatch(selector.to_owned()))
    }

    /// Replaces the text of the `<input>` matching `selector`.
    pub fn set_input_value(&mut self, selector: &str, value: &str) -> Result<(), crate::UiError> {
        self.set_attr(selector, "value", value)
    }

    /// Drains pending events (actions, text changes, submits), oldest first.
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        self.events.drain(..).collect()
    }

    /// Drains pending `data-action` clicks; other event kinds stay queued for [`take_events`](Self::take_events).
    pub fn take_actions(&mut self) -> Vec<UiAction> {
        let mut actions = Vec::new();
        let mut rest = std::collections::VecDeque::new();
        for event in self.events.drain(..) {
            match event {
                UiEvent::Action(action) => actions.push(action),
                other => rest.push_back(other),
            }
        }
        self.events = rest;
        actions
    }

    /// Element under the pointer that has a tooltip or an action, for hover info.
    pub fn hover_target(&self) -> Option<HoverTarget> {
        let hovered = self.doc.get_hover_node_id()?;
        let carrier = self
            .ancestor_with(hovered, TOOLTIP_ATTR)
            .or_else(|| self.ancestor_with(hovered, ACTION_ATTR))?;
        Some(HoverTarget {
            tooltip: self.attr_of(carrier, TOOLTIP_ATTR),
            rect: self.rect_of(carrier),
            element_id: self.attr_of(carrier, "id"),
            action: self.attr_of(carrier, ACTION_ATTR),
        })
    }

    /// Cursor shape for the current hover position, from CSS `cursor`.
    pub fn cursor_hint(&self) -> CursorHint {
        match self.doc.get_cursor() {
            Some(CursorIcon::Pointer) => CursorHint::Pointer,
            Some(CursorIcon::Text) => CursorHint::Text,
            _ => CursorHint::Default,
        }
    }
}
