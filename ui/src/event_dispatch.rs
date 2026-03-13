//! Event dispatching: three-phase propagation, hover tracking, click detection,
//! double-click synthesis, context menu, drag, scroll, and focus management.
//!
//! `EventState` tracks pointer/focus state and dispatches `InputEvent`s against
//! the current layout and element tree, invoking event handlers on elements
//! through capture -> target -> bubble phases.

use std::collections::{HashMap, HashSet};

use crate::element::Element;
use crate::hit_test::hit_test;
use crate::input::{
    BeforeInputEvent, ClickEvent, ClipboardData, ClipboardEvent, CursorStyle, DragData, DragEvent,
    DropEffect, EventResult, FocusEvent, InputEvent, InputType, KeyCode, KeyValue, KeyboardEvent,
    Modifiers, MouseButton, PointerEvent, PointerType, ScrollEndEvent, ScrollSource,
    TextInputEvent, TouchEvent, WheelEvent,
};
use crate::layout::LayoutNode;

/// Internal clipboard action type.
enum ClipboardAction {
    Copy,
    Cut,
}

/// Internal touch event type for dispatch.
enum TouchEventType {
    Start,
    Move,
    End,
}

/// Drag threshold in pixels -- movement beyond this triggers a drag instead of click.
const DRAG_THRESHOLD: f32 = 3.0;
/// Double-click time threshold in milliseconds.
const DOUBLE_CLICK_TIME_MS: u32 = 400;
/// Double-click distance threshold in pixels.
const DOUBLE_CLICK_DISTANCE: f32 = 5.0;

/// Tracks pointer, focus, and interaction state for event dispatch.
pub struct EventState {
    /// Currently hovered element indices (deepest to root path).
    hovered: Vec<usize>,
    /// Element that received pointer-down (for click detection).
    pressed_element: Option<usize>,
    /// Position at pointer-down (for drag threshold).
    press_pos: Option<(f32, f32)>,
    /// Button pressed at pointer-down.
    press_button: MouseButton,
    /// Time of pointer-down.
    press_time: u32,
    /// Whether we are in a drag gesture.
    dragging: bool,
    /// Current pointer position.
    pointer_x: f32,
    pointer_y: f32,
    /// Current cursor style to set on the Wayland surface.
    pub current_cursor: CursorStyle,
    /// Bitmask of currently pressed buttons.
    buttons: u32,
    /// Current modifier state.
    modifiers: Modifiers,

    // Click synthesis state
    /// Time of last click (for double-click detection).
    last_click_time: u32,
    /// Position of last click (for double-click detection).
    last_click_pos: (f32, f32),
    /// Current click count (1 = single, 2 = double, 3 = triple).
    click_count: u32,

    // Focus state
    /// Currently focused element (pre-order index).
    pub focused: Option<usize>,
    /// Whether focus was set via keyboard (for focus ring visibility).
    pub focus_visible: bool,

    // Pointer capture state
    /// Map from pointer_id -> capturing element index.
    pointer_captures: HashMap<u32, usize>,

    // Composition (IME) state
    /// Whether an IME composition session is active.
    pub is_composing: bool,

    // Clipboard state
    /// Current clipboard data (populated by Copy/Cut handlers or Wayland wl_data_device).
    clipboard_data: ClipboardData,
    /// Clipboard data pending write to Wayland selection (set by copy/cut, read by WaylandState).
    pub pending_clipboard_write: Option<ClipboardData>,

    // Drag and drop state
    /// Whether a DnD drag operation is active (DragStart fired and accepted).
    dnd_active: bool,
    /// The element being dragged (source of DragStart).
    dnd_source: Option<usize>,
    /// Current DnD drag data.
    dnd_data: DragData,
    /// Allowed drop effect (set by drag source in DragStart).
    dnd_effect_allowed: DropEffect,
    /// Current drop target element (for DragEnter/DragLeave tracking).
    dnd_over_element: Option<usize>,

    // Touch state
    /// Active touch points: id -> (x, y, time).
    /// The first touch to go down is the "primary" touch and gets coerced to pointer events.
    active_touches: HashMap<i32, (f32, f32, u32)>,
    /// The ID of the primary touch (first finger down), used for touch-to-pointer coercion.
    primary_touch_id: Option<i32>,

    // Slider drag state
    /// Element index of slider being dragged (if any).
    slider_dragging: Option<usize>,
    /// Cached layout bounds of the slider being dragged (x, width).
    slider_drag_bounds: (f32, f32),
}

impl EventState {
    pub fn new() -> Self {
        Self {
            hovered: Vec::new(),
            pressed_element: None,
            press_pos: None,
            press_button: MouseButton::None,
            press_time: 0,
            dragging: false,
            pointer_x: 0.0,
            pointer_y: 0.0,
            current_cursor: CursorStyle::Default,
            buttons: 0,
            modifiers: Modifiers::default(),
            last_click_time: 0,
            last_click_pos: (0.0, 0.0),
            click_count: 0,
            focused: None,
            focus_visible: false,
            pointer_captures: HashMap::new(),
            is_composing: false,
            clipboard_data: ClipboardData::new(),
            pending_clipboard_write: None,
            dnd_active: false,
            dnd_source: None,
            dnd_data: DragData::new(),
            dnd_effect_allowed: DropEffect::None,
            dnd_over_element: None,
            active_touches: HashMap::new(),
            primary_touch_id: None,
            slider_dragging: None,
            slider_drag_bounds: (0.0, 0.0),
        }
    }

    /// Set the clipboard data from the system clipboard (Wayland wl_data_device).
    /// Called before event dispatch so that paste operations use system clipboard contents.
    pub fn set_system_clipboard(&mut self, data: ClipboardData) {
        self.clipboard_data = data;
    }

    /// Take any pending clipboard write data (from copy/cut operations).
    /// Called after event dispatch so WaylandState can set the Wayland selection.
    pub fn take_pending_clipboard_write(&mut self) -> Option<ClipboardData> {
        self.pending_clipboard_write.take()
    }

    /// Returns true if the currently focused element is a text input.
    pub fn focused_element_is_text_input(&self, root_element: &Element) -> bool {
        if let Some(idx) = self.focused {
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                return matches!(element.kind, crate::element::ElementKind::TextInput { .. } | crate::element::ElementKind::Textarea { .. });
            }
        }
        false
    }

    /// Process an input event against the current layout and element tree.
    /// Returns `true` if the event was handled and a redraw should occur.
    pub fn dispatch(
        &mut self,
        event: &InputEvent,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        match event {
            InputEvent::PointerMove { x, y } => {
                self.handle_pointer_move(*x, *y, layout, root_element)
            }

            InputEvent::PointerButton {
                x,
                y,
                button,
                pressed,
                time,
            } => {
                if *pressed {
                    self.handle_pointer_down(*x, *y, *button, *time, layout, root_element)
                } else {
                    self.handle_pointer_up(*x, *y, *button, *time, layout, root_element)
                }
            }

            InputEvent::PointerScroll {
                x,
                y,
                delta_x,
                delta_y,
                source,
                discrete_x: _,
                discrete_y: _,
                stop,
                time,
            } => {
                if *stop {
                    self.handle_scroll_end(*x, *y, layout, root_element)
                } else {
                    self.handle_scroll(*x, *y, *delta_x, *delta_y, *source, *time, layout, root_element)
                }
            }

            InputEvent::PointerLeave => self.handle_pointer_leave(root_element),

            InputEvent::KeyDown { key, modifiers } => {
                self.modifiers = *modifiers;
                self.handle_key_down(key.clone(), *modifiers, root_element)
            }

            InputEvent::KeyUp { key, modifiers } => {
                self.modifiers = *modifiers;
                self.handle_key_up(key.clone(), *modifiers, root_element)
            }

            InputEvent::TextInput { text } => {
                self.handle_text_input(text, root_element)
            }

            InputEvent::CompositionStart => {
                self.handle_composition_start(root_element)
            }

            InputEvent::CompositionUpdate { text, cursor_begin, cursor_end } => {
                self.handle_composition_update(text, *cursor_begin, *cursor_end, root_element)
            }

            InputEvent::CompositionEnd { text } => {
                self.handle_composition_end(text, root_element)
            }

            InputEvent::TouchDown { id, x, y, time } => {
                self.handle_touch_down(*id, *x, *y, *time, layout, root_element)
            }

            InputEvent::TouchMotion { id, x, y, time } => {
                self.handle_touch_motion(*id, *x, *y, *time, layout, root_element)
            }

            InputEvent::TouchUp { id, time } => {
                self.handle_touch_up(*id, *time, layout, root_element)
            }

            InputEvent::TouchCancel => {
                self.handle_touch_cancel(root_element)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pointer move
    // -----------------------------------------------------------------------

    fn handle_pointer_move(
        &mut self,
        x: f32,
        y: f32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        self.pointer_x = x;
        self.pointer_y = y;

        // If pointer is captured, redirect events to the capturing element
        if let Some(&capture_idx) = self.pointer_captures.get(&0) {
            let pe = PointerEvent::from_move(x, y, self.buttons, self.modifiers);
            let mut needs_redraw = false;
            if let Some(element) = get_element_by_preorder(root_element, capture_idx) {
                if let Some(ref handler) = element.on_pointer_move {
                    handler(&pe);
                    needs_redraw = true;
                }
            }
            // Still update drag state
            if self.pressed_element.is_some() && !self.dragging {
                if let Some((px, py)) = self.press_pos {
                    let dx = x - px;
                    let dy = y - py;
                    if (dx * dx + dy * dy).sqrt() > DRAG_THRESHOLD {
                        self.dragging = true;
                    }
                }
            }
            return needs_redraw;
        }

        let hit = hit_test(layout, root_element, x, y);
        let new_hovered: Vec<usize> = hit
            .as_ref()
            .map(|h| h.path.clone())
            .unwrap_or_default();

        // Compute hover enter/leave
        let old_set: HashSet<usize> = self.hovered.iter().copied().collect();
        let new_set: HashSet<usize> = new_hovered.iter().copied().collect();

        let mut needs_redraw = false;

        let pe = PointerEvent::from_move(x, y, self.buttons, self.modifiers);

        // Elements that lost hover -- fire on_pointer_leave and legacy on_hover(false)
        for &idx in &self.hovered {
            if !new_set.contains(&idx) {
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    if let Some(ref handler) = element.on_pointer_leave {
                        handler(&pe);
                        needs_redraw = true;
                    }
                    if let Some(ref handler) = element.on_hover {
                        handler(false);
                        needs_redraw = true;
                    }
                }
            }
        }

        // Elements that gained hover -- fire on_pointer_enter and legacy on_hover(true)
        for &idx in &new_hovered {
            if !old_set.contains(&idx) {
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    if let Some(ref handler) = element.on_pointer_enter {
                        handler(&pe);
                        needs_redraw = true;
                    }
                    if let Some(ref handler) = element.on_hover {
                        handler(true);
                        needs_redraw = true;
                    }
                }
            }
        }

        // Update cursor: use the deepest element's cursor style
        self.current_cursor = CursorStyle::Default;
        for &idx in &new_hovered {
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(cursor) = element.cursor {
                    self.current_cursor = cursor;
                    break;
                }
            }
        }

        self.hovered = new_hovered;

        // Dispatch PointerMove through three-phase propagation
        if let Some(ref hit_result) = hit {
            if !hit_result.path.is_empty() {
                let result = dispatch_pointer_event_three_phase(
                    &pe,
                    &hit_result.path,
                    root_element,
                    PointerEventType::Move,
                );
                if result {
                    needs_redraw = true;
                }
            }
        }

        // Drag threshold detection
        if self.pressed_element.is_some() && !self.dragging {
            if let Some((px, py)) = self.press_pos {
                let dx = x - px;
                let dy = y - py;
                if (dx * dx + dy * dy).sqrt() > DRAG_THRESHOLD {
                    self.dragging = true;

                    // Check if the pressed element has on_drag_start — initiate DnD
                    if let Some(pressed_idx) = self.pressed_element {
                        if let Some(element) = get_element_by_preorder(root_element, pressed_idx) {
                            if element.on_drag_start.is_some() {
                                let mut drag_event = DragEvent {
                                    x,
                                    y,
                                    data: DragData::new(),
                                    effect_allowed: DropEffect::Move,
                                    drop_effect: DropEffect::None,
                                };
                                // Fire DragStart — handler sets drag data and effect_allowed
                                let result = (element.on_drag_start.as_ref().unwrap())(&mut drag_event);
                                match result {
                                    EventResult::StopAndPreventDefault | EventResult::PreventDefault => {
                                        // DragStart was prevented — cancel DnD, stay in legacy drag mode
                                    }
                                    _ => {
                                        // DnD accepted
                                        self.dnd_active = true;
                                        self.dnd_source = Some(pressed_idx);
                                        self.dnd_data = drag_event.data;
                                        self.dnd_effect_allowed = drag_event.effect_allowed;
                                        needs_redraw = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // DnD drag-over handling
        if self.dnd_active {
            let hit = hit_test(layout, root_element, x, y);
            let new_target = hit.as_ref().and_then(|h| {
                // Find the deepest element in the hit path that has on_drag_over or on_drop
                for &idx in &h.path {
                    if let Some(element) = get_element_by_preorder(root_element, idx) {
                        if element.on_drag_over.is_some() || element.on_drop.is_some() {
                            return Some(idx);
                        }
                    }
                }
                None
            });

            let old_target = self.dnd_over_element;

            // DragLeave on old target if changed
            if old_target != new_target {
                if let Some(old_idx) = old_target {
                    if let Some(element) = get_element_by_preorder(root_element, old_idx) {
                        if let Some(ref handler) = element.on_drag_leave {
                            let drag_event = DragEvent {
                                x,
                                y,
                                data: self.dnd_data.clone(),
                                effect_allowed: self.dnd_effect_allowed,
                                drop_effect: DropEffect::None,
                            };
                            handler(&drag_event);
                            needs_redraw = true;
                        }
                    }
                }

                // DragEnter on new target
                if let Some(new_idx) = new_target {
                    if let Some(element) = get_element_by_preorder(root_element, new_idx) {
                        if let Some(ref handler) = element.on_drag_enter {
                            let drag_event = DragEvent {
                                x,
                                y,
                                data: self.dnd_data.clone(),
                                effect_allowed: self.dnd_effect_allowed,
                                drop_effect: DropEffect::None,
                            };
                            handler(&drag_event);
                            needs_redraw = true;
                        }
                    }
                }

                self.dnd_over_element = new_target;
            }

            // DragOver on current target (fires repeatedly during drag)
            if let Some(target_idx) = new_target {
                if let Some(element) = get_element_by_preorder(root_element, target_idx) {
                    if let Some(ref handler) = element.on_drag_over {
                        let mut drag_event = DragEvent {
                            x,
                            y,
                            data: self.dnd_data.clone(),
                            effect_allowed: self.dnd_effect_allowed,
                            drop_effect: DropEffect::None,
                        };
                        handler(&mut drag_event);
                        needs_redraw = true;
                    }
                }
            }

            return needs_redraw;
        }

        // Slider drag handling — update value continuously during drag
        if let Some(slider_idx) = self.slider_dragging {
            if let Some(element) = get_element_by_preorder(root_element, slider_idx) {
                let (bx, bw) = self.slider_drag_bounds;
                match &element.kind {
                    crate::element::ElementKind::Slider { min, max, step, .. } => {
                        if let Some(ref handler) = element.on_float_change {
                            let val = slider_value_from_x(x, bx, bw, *min, *max, *step);
                            handler(val);
                            needs_redraw = true;
                        }
                    }
                    crate::element::ElementKind::RangeSlider {
                        low, high, min, max, step, ..
                    } => {
                        if let Some(ref handler) = element.on_range_change {
                            let click_val = slider_value_from_x(x, bx, bw, *min, *max, *step);
                            let dist_low = (click_val - low).abs();
                            let dist_high = (click_val - high).abs();
                            let (new_low, new_high) = if dist_low <= dist_high {
                                (click_val.min(*high), *high)
                            } else {
                                (*low, click_val.max(*low))
                            };
                            handler(new_low, new_high);
                            needs_redraw = true;
                        }
                    }
                    _ => {}
                }
            }
            return needs_redraw;
        }

        // Legacy drag handling (non-DnD drag)
        if self.dragging {
            if let Some(pressed_idx) = self.pressed_element {
                if let Some(element) = get_element_by_preorder(root_element, pressed_idx) {
                    if let Some(ref handler) = element.on_drag {
                        if let Some((px, py)) = self.press_pos {
                            handler(x - px, y - py);
                            needs_redraw = true;
                        }
                    }
                }
            }
        }

        needs_redraw
    }

    // -----------------------------------------------------------------------
    // Pointer down
    // -----------------------------------------------------------------------

    fn handle_pointer_down(
        &mut self,
        x: f32,
        y: f32,
        button: MouseButton,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        self.buttons |= button.to_bitmask();
        let hit = hit_test(layout, root_element, x, y);
        self.pressed_element = hit.as_ref().and_then(|h| h.path.first().copied());
        self.press_pos = Some((x, y));
        self.press_button = button;
        self.press_time = time;
        self.dragging = false;

        let pe = PointerEvent::from_button(x, y, button, self.buttons, time, self.modifiers);

        let mut needs_redraw = false;

        // Three-phase dispatch of PointerDown
        if let Some(ref hit_result) = hit {
            if !hit_result.path.is_empty() {
                let (handled, default_prevented) = dispatch_pointer_event_three_phase_full(
                    &pe,
                    &hit_result.path,
                    root_element,
                    PointerEventType::Down,
                );
                if handled {
                    needs_redraw = true;
                }

                // Default action: set focus to target (unless prevented)
                if !default_prevented {
                    let target = hit_result.path[0];
                    let target_element = get_element_by_preorder(root_element, target);
                    let is_focusable = target_element
                        .map(|e| element_is_focusable(e))
                        .unwrap_or(false);

                    if is_focusable && self.focused != Some(target) {
                        if self.set_focus(Some(target), root_element) {
                            needs_redraw = true;
                        }
                        self.focus_visible = false; // pointer focus, no focus ring
                    } else if !is_focusable && self.focused.is_some() {
                        // Clicked on non-focusable: blur current
                        if self.set_focus(None, root_element) {
                            needs_redraw = true;
                        }
                    }
                }
            }
        }

        // Slider: start drag and fire initial value on click-on-track
        if button == MouseButton::Left {
            if let Some(pressed_idx) = self.pressed_element {
                if let Some(element) = get_element_by_preorder(root_element, pressed_idx) {
                    if !element.disabled {
                        match &element.kind {
                            crate::element::ElementKind::Slider { min, max, step, .. } => {
                                if let Some((bx, _by, bw, _bh)) =
                                    find_layout_node_bounds(layout, pressed_idx)
                                {
                                    self.slider_dragging = Some(pressed_idx);
                                    self.slider_drag_bounds = (bx, bw);
                                    if let Some(ref handler) = element.on_float_change {
                                        let val = slider_value_from_x(x, bx, bw, *min, *max, *step);
                                        handler(val);
                                        needs_redraw = true;
                                    }
                                }
                            }
                            crate::element::ElementKind::RangeSlider {
                                low, high, min, max, step, ..
                            } => {
                                if let Some((bx, _by, bw, _bh)) =
                                    find_layout_node_bounds(layout, pressed_idx)
                                {
                                    self.slider_dragging = Some(pressed_idx);
                                    self.slider_drag_bounds = (bx, bw);
                                    if let Some(ref handler) = element.on_range_change {
                                        // Move the nearest thumb
                                        let click_val = slider_value_from_x(x, bx, bw, *min, *max, *step);
                                        let dist_low = (click_val - low).abs();
                                        let dist_high = (click_val - high).abs();
                                        let (new_low, new_high) = if dist_low <= dist_high {
                                            (click_val.min(*high), *high)
                                        } else {
                                            (*low, click_val.max(*low))
                                        };
                                        handler(new_low, new_high);
                                        needs_redraw = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        needs_redraw
    }

    // -----------------------------------------------------------------------
    // Pointer up
    // -----------------------------------------------------------------------

    fn handle_pointer_up(
        &mut self,
        x: f32,
        y: f32,
        button: MouseButton,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        self.buttons &= !button.to_bitmask();
        let pe = PointerEvent::from_button(x, y, button, self.buttons, time, self.modifiers);

        let mut needs_redraw = false;

        // Three-phase dispatch of PointerUp
        let hit = hit_test(layout, root_element, x, y);
        if let Some(ref hit_result) = hit {
            if !hit_result.path.is_empty() {
                if dispatch_pointer_event_three_phase(
                    &pe,
                    &hit_result.path,
                    root_element,
                    PointerEventType::Up,
                ) {
                    needs_redraw = true;
                }
            }
        }

        // DnD drop / drag-end handling
        if self.dnd_active {
            let mut final_drop_effect = DropEffect::None;

            // Fire Drop on current drag-over target
            if let Some(over_idx) = self.dnd_over_element {
                if let Some(element) = get_element_by_preorder(root_element, over_idx) {
                    if let Some(ref handler) = element.on_drop {
                        let drag_event = DragEvent {
                            x,
                            y,
                            data: self.dnd_data.clone(),
                            effect_allowed: self.dnd_effect_allowed,
                            drop_effect: self.dnd_effect_allowed, // offer the allowed effect
                        };
                        let result = handler(&drag_event);
                        match result {
                            EventResult::StopAndPreventDefault | EventResult::PreventDefault => {
                                // Drop rejected
                            }
                            _ => {
                                final_drop_effect = self.dnd_effect_allowed;
                            }
                        }
                        needs_redraw = true;
                    }
                }
            }

            // Fire DragEnd on the source element
            if let Some(source_idx) = self.dnd_source {
                if let Some(element) = get_element_by_preorder(root_element, source_idx) {
                    if let Some(ref handler) = element.on_drag_start {
                        // DragEnd is informational; we reuse on_drag_start to keep it simple.
                        // The source can check drop_effect to know if the drop was accepted.
                        // In a more complete implementation, we'd have a separate on_drag_end handler.
                        let _ = handler;
                    }
                }
            }

            // Reset DnD state
            self.dnd_active = false;
            self.dnd_source = None;
            self.dnd_data = DragData::new();
            self.dnd_effect_allowed = DropEffect::None;
            self.dnd_over_element = None;
            self.pressed_element = None;
            self.press_pos = None;
            self.dragging = false;
            if self.buttons == 0 {
                self.pointer_captures.remove(&0);
            }
            return needs_redraw;
        }

        // Click synthesis
        if !self.dragging {
            if let Some(pressed_idx) = self.pressed_element {
                let release_on_same = hit
                    .as_ref()
                    .map(|h| h.path.contains(&pressed_idx))
                    .unwrap_or(false);

                if release_on_same {
                    match button {
                        MouseButton::Left => {
                            // Check for double/triple click
                            if time.wrapping_sub(self.last_click_time) < DOUBLE_CLICK_TIME_MS {
                                let dx = x - self.last_click_pos.0;
                                let dy = y - self.last_click_pos.1;
                                if (dx * dx + dy * dy).sqrt() < DOUBLE_CLICK_DISTANCE {
                                    self.click_count += 1;
                                } else {
                                    self.click_count = 1;
                                }
                            } else {
                                self.click_count = 1;
                            }

                            self.last_click_time = time;
                            self.last_click_pos = (x, y);

                            let click_event = ClickEvent {
                                x,
                                y,
                                button,
                                count: self.click_count,
                                modifiers: self.modifiers,
                                pointer_type: PointerType::Mouse,
                            };

                            // Dispatch Click through three-phase
                            if let Some(ref hit_result) = hit {
                                if dispatch_click_event_three_phase(
                                    &click_event,
                                    &hit_result.path,
                                    root_element,
                                ) {
                                    needs_redraw = true;
                                }
                            }

                            // Fire form element callbacks for the clicked element
                            if let Some(element) = get_element_by_preorder(root_element, pressed_idx) {
                                if !element.disabled {
                                    if dispatch_form_element_click(element) {
                                        needs_redraw = true;
                                    }
                                }
                            }

                            // Fire DoubleClick if count == 2
                            if self.click_count == 2 {
                                if let Some(ref hit_result) = hit {
                                    if dispatch_double_click_event(
                                        &click_event,
                                        &hit_result.path,
                                        root_element,
                                    ) {
                                        needs_redraw = true;
                                    }
                                }
                            }
                        }
                        MouseButton::Right => {
                            // ContextMenu on right-click release
                            if let Some(ref hit_result) = hit {
                                if dispatch_context_menu_event(
                                    &pe,
                                    &hit_result.path,
                                    root_element,
                                ) {
                                    needs_redraw = true;
                                }
                            }
                        }
                        MouseButton::Middle | MouseButton::Back | MouseButton::Forward => {
                            // AuxClick for non-primary buttons
                            let click_event = ClickEvent {
                                x,
                                y,
                                button,
                                count: 1,
                                modifiers: self.modifiers,
                                pointer_type: PointerType::Mouse,
                            };
                            if let Some(ref hit_result) = hit {
                                if dispatch_click_event_three_phase(
                                    &click_event,
                                    &hit_result.path,
                                    root_element,
                                ) {
                                    needs_redraw = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        self.pressed_element = None;
        self.press_pos = None;
        self.dragging = false;
        self.slider_dragging = None;

        // Auto-release pointer capture when all buttons are released
        if self.buttons == 0 {
            self.pointer_captures.remove(&0);
        }

        needs_redraw
    }

    // -----------------------------------------------------------------------
    // Scroll
    // -----------------------------------------------------------------------

    fn handle_scroll(
        &mut self,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        source: Option<ScrollSource>,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        let hit = hit_test(layout, root_element, x, y);
        if let Some(result) = hit {
            let resolved_source = source.unwrap_or(ScrollSource::Wheel);
            let is_discrete = matches!(resolved_source, ScrollSource::Wheel | ScrollSource::WheelTilt);

            let wheel_event = WheelEvent {
                x,
                y,
                delta_x,
                delta_y,
                source: resolved_source,
                is_discrete,
                modifiers: self.modifiers,
                time,
            };

            // Build root-to-target path for three-phase dispatch
            let path_root_to_target: Vec<usize> = result.path.iter().copied().rev().collect();

            // Capture phase (root to target's parent)
            let mut stopped = false;
            let mut default_prevented = false;
            let target_idx = path_root_to_target.len() - 1;

            for (i, &idx) in path_root_to_target.iter().enumerate() {
                if stopped {
                    break;
                }
                if i == target_idx {
                    break; // Skip target in capture phase
                }
                // No capture handlers for wheel yet
            }

            // Target phase
            if !stopped {
                let target = path_root_to_target[target_idx];
                if let Some(element) = get_element_by_preorder(root_element, target) {
                    if let Some(ref handler) = element.on_wheel {
                        let r = handler(&wheel_event);
                        apply_event_result(r, &mut stopped, &mut default_prevented);
                    }
                }
            }

            // Bubble phase (target's parent to root) -- also try legacy on_scroll
            if !stopped {
                for &idx in path_root_to_target[..target_idx].iter().rev() {
                    if stopped {
                        break;
                    }
                    if let Some(element) = get_element_by_preorder(root_element, idx) {
                        if let Some(ref handler) = element.on_wheel {
                            let r = handler(&wheel_event);
                            apply_event_result(r, &mut stopped, &mut default_prevented);
                        }
                    }
                }
            }

            if stopped {
                return true;
            }

            // Legacy on_scroll: bubble up path until handled
            for &idx in &result.path {
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    if let Some(ref handler) = element.on_scroll {
                        handler(delta_x, delta_y);
                        return true;
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Scroll end (finger lifted from touchpad)
    // -----------------------------------------------------------------------

    fn handle_scroll_end(
        &mut self,
        x: f32,
        y: f32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        let hit = hit_test(layout, root_element, x, y);
        if let Some(result) = hit {
            let scroll_end_event = ScrollEndEvent {
                x,
                y,
                modifiers: self.modifiers,
            };

            // Fire on_scroll_end on the target, then bubble up
            for &idx in &result.path {
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    if let Some(ref handler) = element.on_scroll_end {
                        handler(&scroll_end_event);
                        return true;
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Pointer leave
    // -----------------------------------------------------------------------

    fn handle_pointer_leave(&mut self, root_element: &Element) -> bool {
        // If a DnD operation is active, cancel it
        if self.dnd_active {
            // Fire DragLeave on current over-element
            if let Some(over_idx) = self.dnd_over_element {
                if let Some(element) = get_element_by_preorder(root_element, over_idx) {
                    if let Some(ref handler) = element.on_drag_leave {
                        let drag_event = DragEvent {
                            x: self.pointer_x,
                            y: self.pointer_y,
                            data: self.dnd_data.clone(),
                            effect_allowed: self.dnd_effect_allowed,
                            drop_effect: DropEffect::None,
                        };
                        handler(&drag_event);
                    }
                }
            }
            // Reset DnD state (cancelled — drop_effect = None)
            self.dnd_active = false;
            self.dnd_source = None;
            self.dnd_data = DragData::new();
            self.dnd_effect_allowed = DropEffect::None;
            self.dnd_over_element = None;
        }

        let pe = PointerEvent::from_move(
            self.pointer_x,
            self.pointer_y,
            self.buttons,
            self.modifiers,
        );

        for &idx in &self.hovered {
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(ref handler) = element.on_pointer_leave {
                    handler(&pe);
                }
                if let Some(ref handler) = element.on_hover {
                    handler(false);
                }
            }
        }
        self.hovered.clear();
        self.current_cursor = CursorStyle::Default;
        true
    }

    // -----------------------------------------------------------------------
    // Focus management
    // -----------------------------------------------------------------------

    /// Set focus to a new element. Returns true if focus changed.
    fn set_focus(&mut self, new_focus: Option<usize>, root_element: &Element) -> bool {
        let old_focus = self.focused;
        if old_focus == new_focus {
            return false;
        }

        // Fire blur/focus-out on old element
        if let Some(old_idx) = old_focus {
            let focus_event = FocusEvent {
                related_target: new_focus,
            };
            if let Some(element) = get_element_by_preorder(root_element, old_idx) {
                if let Some(ref handler) = element.on_blur {
                    handler(&focus_event);
                }
                if let Some(ref handler) = element.on_focus_out {
                    handler(&focus_event);
                }
            }
        }

        self.focused = new_focus;

        // Fire focus/focus-in on new element
        if let Some(new_idx) = new_focus {
            let focus_event = FocusEvent {
                related_target: old_focus,
            };
            if let Some(element) = get_element_by_preorder(root_element, new_idx) {
                if let Some(ref handler) = element.on_focus {
                    handler(&focus_event);
                }
                if let Some(ref handler) = element.on_focus_in {
                    handler(&focus_event);
                }
            }
        }

        true
    }

    // -----------------------------------------------------------------------
    // Keyboard dispatch
    // -----------------------------------------------------------------------

    fn handle_key_down(
        &mut self,
        key: crate::input::Key,
        modifiers: Modifiers,
        root_element: &Element,
    ) -> bool {
        let key_value = keysym_to_key_value(key.0);
        let kb_event = KeyboardEvent {
            key: key_value.clone(),
            code: KeyCode(key.0),
            repeat: false,
            modifiers,
            is_composing: self.is_composing,
            time: 0,
        };

        // Tab navigation (default action)
        if key_value == KeyValue::Tab && !modifiers.ctrl && !modifiers.alt && !modifiers.super_ {
            // Dispatch KeyDown to focused element first
            let (_, default_prevented) = self.dispatch_keyboard_event(&kb_event, root_element, true);
            if !default_prevented {
                let reverse = modifiers.shift;
                if self.move_focus(reverse, root_element) {
                    self.focus_visible = true;
                    return true;
                }
            }
            return true;
        }

        // Clipboard shortcuts: Ctrl+C (copy), Ctrl+X (cut), Ctrl+V (paste)
        if modifiers.ctrl && !modifiers.alt && !modifiers.super_ {
            match &key_value {
                KeyValue::Character(c) if c == "c" || c == "C" => {
                    // Dispatch KeyDown first, then fire Copy if not prevented
                    let (_, default_prevented) =
                        self.dispatch_keyboard_event(&kb_event, root_element, true);
                    if !default_prevented {
                        if let Some(data) = self.handle_clipboard_copy(root_element) {
                            self.clipboard_data = data.clone();
                            self.pending_clipboard_write = Some(data);
                        }
                    }
                    return true;
                }
                KeyValue::Character(c) if c == "x" || c == "X" => {
                    let (_, default_prevented) =
                        self.dispatch_keyboard_event(&kb_event, root_element, true);
                    if !default_prevented {
                        if let Some(data) = self.handle_clipboard_cut(root_element) {
                            self.clipboard_data = data.clone();
                            self.pending_clipboard_write = Some(data);
                        }
                    }
                    return true;
                }
                KeyValue::Character(c) if c == "v" || c == "V" => {
                    let (_, default_prevented) =
                        self.dispatch_keyboard_event(&kb_event, root_element, true);
                    if !default_prevented {
                        self.handle_clipboard_paste(root_element);
                    }
                    return true;
                }
                _ => {}
            }
        }

        // Space/Enter activates focused form elements (button, checkbox, switch, radio)
        if matches!(key_value, KeyValue::Character(ref c) if c == " ")
            || matches!(key_value, KeyValue::Enter)
        {
            if let Some(focused_idx) = self.focused {
                // Dispatch KeyDown first
                let (_, default_prevented) =
                    self.dispatch_keyboard_event(&kb_event, root_element, true);
                if !default_prevented {
                    if let Some(element) = get_element_by_preorder(root_element, focused_idx) {
                        if !element.disabled {
                            if dispatch_form_element_activate(element) {
                                return true;
                            }
                        }
                    }
                }
                return true;
            }
        }

        // Arrow keys for focused slider elements
        if let Some(focused_idx) = self.focused {
            if let Some(element) = get_element_by_preorder(root_element, focused_idx) {
                if !element.disabled {
                    if let crate::element::ElementKind::Slider {
                        value,
                        min,
                        max,
                        step,
                    } = &element.kind
                    {
                        let direction = match &key_value {
                            KeyValue::ArrowRight | KeyValue::ArrowUp => Some(1),
                            KeyValue::ArrowLeft | KeyValue::ArrowDown => Some(-1),
                            KeyValue::Home => {
                                if let Some(ref handler) = element.on_float_change {
                                    handler(*min);
                                }
                                return true;
                            }
                            KeyValue::End => {
                                if let Some(ref handler) = element.on_float_change {
                                    handler(*max);
                                }
                                return true;
                            }
                            _ => None,
                        };
                        if let Some(dir) = direction {
                            let multiplier = if modifiers.shift { 10 } else { 1 };
                            if let Some(ref handler) = element.on_float_change {
                                let new_val = slider_step_value(
                                    *value,
                                    *min,
                                    *max,
                                    *step,
                                    dir * multiplier,
                                );
                                handler(new_val);
                            }
                            return true;
                        }
                    }
                }
            }
        }

        // Dispatch KeyDown to focused element
        let (handled, _) = self.dispatch_keyboard_event(&kb_event, root_element, true);
        handled
    }

    fn handle_key_up(
        &mut self,
        key: crate::input::Key,
        modifiers: Modifiers,
        root_element: &Element,
    ) -> bool {
        let key_value = keysym_to_key_value(key.0);
        let kb_event = KeyboardEvent {
            key: key_value,
            code: KeyCode(key.0),
            repeat: false,
            modifiers,
            is_composing: self.is_composing,
            time: 0,
        };

        let (handled, _) = self.dispatch_keyboard_event(&kb_event, root_element, false);
        handled
    }

    fn handle_text_input(&mut self, text: &str, root_element: &Element) -> bool {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return false,
        };

        let is_composing = self.is_composing;

        // 1. Fire BeforeInput (cancelable) through three-phase propagation
        let before_input = BeforeInputEvent {
            data: Some(text.to_string()),
            input_type: InputType::InsertText,
            is_composing,
        };

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return false;
        }

        let (_, default_prevented) =
            dispatch_before_input_event(&before_input, &path, root_element);

        if default_prevented {
            return true; // Event was handled but text insertion prevented
        }

        // 2. Fire Input (not cancelable) through three-phase propagation
        let input_event = TextInputEvent {
            data: Some(text.to_string()),
            input_type: InputType::InsertText,
            is_composing,
        };
        dispatch_input_event(&input_event, &path, root_element);

        // 3. Legacy fallback: fire on_change if present
        if let Some(element) = get_element_by_preorder(root_element, focused_idx) {
            if let Some(ref handler) = element.on_change {
                handler(text.to_string());
            }
        }

        true
    }

    // -----------------------------------------------------------------------
    // Composition (IME)
    // -----------------------------------------------------------------------

    fn handle_composition_start(&mut self, root_element: &Element) -> bool {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return false,
        };

        self.is_composing = true;

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return false;
        }

        let event = crate::input::CompositionEvent {
            data: String::new(),
            cursor_begin: None,
            cursor_end: None,
        };
        dispatch_composition_event(&event, CompositionPhase::Start, &path, root_element);
        true
    }

    fn handle_composition_update(
        &mut self,
        text: &str,
        cursor_begin: Option<usize>,
        cursor_end: Option<usize>,
        root_element: &Element,
    ) -> bool {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return false,
        };

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return false;
        }

        let event = crate::input::CompositionEvent {
            data: text.to_string(),
            cursor_begin,
            cursor_end,
        };
        dispatch_composition_event(&event, CompositionPhase::Update, &path, root_element);
        true
    }

    fn handle_composition_end(&mut self, text: &str, root_element: &Element) -> bool {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return false,
        };

        self.is_composing = false;

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return false;
        }

        // Fire CompositionEnd
        let event = crate::input::CompositionEvent {
            data: text.to_string(),
            cursor_begin: None,
            cursor_end: None,
        };
        dispatch_composition_event(&event, CompositionPhase::End, &path, root_element);

        // After composition ends, fire a normal text input event for the committed text
        if !text.is_empty() {
            self.handle_text_input(text, root_element);
        }

        true
    }

    // -----------------------------------------------------------------------
    // Clipboard
    // -----------------------------------------------------------------------

    /// Handle Ctrl+C: fire Copy event on focused element (bubble phase).
    /// Returns the clipboard data if a handler populated it.
    fn handle_clipboard_copy(&self, root_element: &Element) -> Option<ClipboardData> {
        self.dispatch_clipboard_event(root_element, ClipboardAction::Copy)
    }

    /// Handle Ctrl+X: fire Cut event on focused element (bubble phase).
    /// Returns the clipboard data if a handler populated it.
    fn handle_clipboard_cut(&self, root_element: &Element) -> Option<ClipboardData> {
        self.dispatch_clipboard_event(root_element, ClipboardAction::Cut)
    }

    /// Handle Ctrl+V: fire Paste event on focused element (bubble phase).
    fn handle_clipboard_paste(&self, root_element: &Element) -> bool {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return false,
        };

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return false;
        }

        // Create paste event with current clipboard data.
        // WaylandState populates clipboard_data from wl_data_device selection before dispatch.
        let event = ClipboardEvent {
            clipboard_data: self.clipboard_data.clone(),
        };

        let mut stopped = false;
        let mut default_prevented = false;

        // Target phase
        let target_pos = path.len() - 1;
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            if let Some(h) = element.on_paste.as_ref() {
                apply_event_result(h(&event), &mut stopped, &mut default_prevented);
            }
        }

        // Bubble phase
        if !stopped {
            for &idx in path[..target_pos].iter().rev() {
                if stopped {
                    break;
                }
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    if let Some(h) = element.on_paste.as_ref() {
                        apply_event_result(h(&event), &mut stopped, &mut default_prevented);
                    }
                }
            }
        }

        // If not prevented, default behavior for text inputs:
        // insert clipboard text content.
        if !default_prevented {
            if let Some(text) = event.clipboard_data.get_text() {
                if !text.is_empty() {
                    if let Some(element) = get_element_by_preorder(root_element, focused_idx) {
                        if matches!(element.kind, crate::element::ElementKind::TextInput { .. } | crate::element::ElementKind::Textarea { .. }) {
                            // Fire through text input pipeline
                            let before_input = BeforeInputEvent {
                                data: Some(text.to_string()),
                                input_type: InputType::InsertFromPaste,
                                is_composing: false,
                            };
                            let (_, bp) =
                                dispatch_before_input_event(&before_input, &path, root_element);
                            if !bp {
                                let input_event = TextInputEvent {
                                    data: Some(text.to_string()),
                                    input_type: InputType::InsertFromPaste,
                                    is_composing: false,
                                };
                                dispatch_input_event(&input_event, &path, root_element);
                                if let Some(ref handler) = element.on_change {
                                    handler(text.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        true
    }

    /// Dispatch a Copy or Cut clipboard event through target + bubble phases.
    /// Returns the populated clipboard data if a handler set any.
    fn dispatch_clipboard_event(
        &self,
        root_element: &Element,
        action: ClipboardAction,
    ) -> Option<ClipboardData> {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return None,
        };

        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return None;
        }

        let mut event = ClipboardEvent {
            clipboard_data: ClipboardData::new(),
        };

        let mut stopped = false;
        let mut default_prevented = false;

        // Target phase
        let target_pos = path.len() - 1;
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            let handler = match action {
                ClipboardAction::Copy => element.on_copy.as_ref(),
                ClipboardAction::Cut => element.on_cut.as_ref(),
            };
            if let Some(h) = handler {
                apply_event_result(h(&mut event), &mut stopped, &mut default_prevented);
            }
        }

        // Bubble phase
        if !stopped {
            for &idx in path[..target_pos].iter().rev() {
                if stopped {
                    break;
                }
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    let handler = match action {
                        ClipboardAction::Copy => element.on_copy.as_ref(),
                        ClipboardAction::Cut => element.on_cut.as_ref(),
                    };
                    if let Some(h) = handler {
                        apply_event_result(h(&mut event), &mut stopped, &mut default_prevented);
                    }
                }
            }
        }

        if default_prevented || event.clipboard_data.is_empty() {
            None
        } else {
            Some(event.clipboard_data)
        }
    }

    /// Dispatch a keyboard event to the focused element with three-phase propagation.
    /// Returns (any_handler_invoked, default_prevented).
    fn dispatch_keyboard_event(
        &self,
        event: &KeyboardEvent,
        root_element: &Element,
        is_key_down: bool,
    ) -> (bool, bool) {
        let focused_idx = match self.focused {
            Some(idx) => idx,
            None => return (false, false),
        };

        // Build path from root to focused element
        let path = build_path_to_element(root_element, focused_idx);
        if path.is_empty() {
            return (false, false);
        }

        let target_pos = path.len() - 1;
        let mut stopped = false;
        let mut default_prevented = false;
        let mut handled = false;

        // Capture phase (root to target's parent) — keyboard capture handlers not yet supported
        // Target phase
        if !stopped {
            let target = path[target_pos];
            if let Some(element) = get_element_by_preorder(root_element, target) {
                let handler = if is_key_down {
                    element.on_key_down.as_ref()
                } else {
                    element.on_key_up.as_ref()
                };
                if let Some(h) = handler {
                    let r = h(event);
                    apply_event_result(r, &mut stopped, &mut default_prevented);
                    handled = true;
                }
            }
        }

        // Bubble phase (target's parent to root)
        if !stopped {
            for &idx in path[..target_pos].iter().rev() {
                if stopped {
                    break;
                }
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    let handler = if is_key_down {
                        element.on_key_down.as_ref()
                    } else {
                        element.on_key_up.as_ref()
                    };
                    if let Some(h) = handler {
                        let r = h(event);
                        apply_event_result(r, &mut stopped, &mut default_prevented);
                        handled = true;
                    }
                }
            }
        }

        (handled, default_prevented)
    }

    // -----------------------------------------------------------------------
    // Tab navigation
    // -----------------------------------------------------------------------

    /// Move focus to the next (or previous if reverse) focusable element.
    /// Returns true if focus moved.
    fn move_focus(&mut self, reverse: bool, root_element: &Element) -> bool {
        let tab_order = build_tab_order(root_element, self.focused);

        if tab_order.is_empty() {
            return false;
        }

        let current_pos = self
            .focused
            .and_then(|f| tab_order.iter().position(|&idx| idx == f));

        let next = match current_pos {
            Some(pos) => {
                if reverse {
                    if pos == 0 {
                        tab_order.len() - 1
                    } else {
                        pos - 1
                    }
                } else {
                    if pos == tab_order.len() - 1 {
                        0
                    } else {
                        pos + 1
                    }
                }
            }
            None => {
                if reverse {
                    tab_order.len() - 1
                } else {
                    0
                }
            }
        };

        self.set_focus(Some(tab_order[next]), root_element)
    }

    // -----------------------------------------------------------------------
    // Pointer capture
    // -----------------------------------------------------------------------

    /// Set pointer capture for a pointer ID to a specific element.
    pub fn set_pointer_capture(&mut self, pointer_id: u32, element_idx: usize) {
        self.pointer_captures.insert(pointer_id, element_idx);
    }

    /// Release pointer capture for a pointer ID.
    pub fn release_pointer_capture(&mut self, pointer_id: u32) {
        self.pointer_captures.remove(&pointer_id);
    }

    /// Check if an element has pointer capture.
    pub fn has_pointer_capture(&self, pointer_id: u32, element_idx: usize) -> bool {
        self.pointer_captures.get(&pointer_id) == Some(&element_idx)
    }

    /// Resolve the event target, considering pointer capture.
    fn resolve_target(&self, pointer_id: u32, hit_element: usize) -> usize {
        self.pointer_captures
            .get(&pointer_id)
            .copied()
            .unwrap_or(hit_element)
    }

    // -----------------------------------------------------------------------
    // Touch events
    // -----------------------------------------------------------------------

    /// Handle a new touch point contacting the surface.
    /// The first touch becomes the "primary" touch and is coerced to pointer events.
    fn handle_touch_down(
        &mut self,
        id: i32,
        x: f32,
        y: f32,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        self.active_touches.insert(id, (x, y, time));

        let is_primary = self.primary_touch_id.is_none();
        if is_primary {
            self.primary_touch_id = Some(id);
            // Coerce primary touch to pointer events (PointerMove + PointerDown)
            self.handle_pointer_move(x, y, layout, root_element);
            self.handle_pointer_down(x, y, MouseButton::Left, time, layout, root_element);
        }

        // Dispatch native TouchStart event
        let touch_event = TouchEvent {
            touch_id: id,
            x,
            y,
            width: None,
            height: None,
            orientation: None,
            time,
        };
        self.dispatch_touch_event(&touch_event, layout, root_element, TouchEventType::Start);

        true
    }

    /// Handle a touch point moving.
    fn handle_touch_motion(
        &mut self,
        id: i32,
        x: f32,
        y: f32,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        self.active_touches.insert(id, (x, y, time));

        if self.primary_touch_id == Some(id) {
            // Coerce primary touch motion to pointer move
            self.handle_pointer_move(x, y, layout, root_element);
        }

        let touch_event = TouchEvent {
            touch_id: id,
            x,
            y,
            width: None,
            height: None,
            orientation: None,
            time,
        };
        self.dispatch_touch_event(&touch_event, layout, root_element, TouchEventType::Move);

        true
    }

    /// Handle a touch point being lifted.
    fn handle_touch_up(
        &mut self,
        id: i32,
        time: u32,
        layout: &LayoutNode,
        root_element: &Element,
    ) -> bool {
        let pos = self.active_touches.remove(&id);

        if self.primary_touch_id == Some(id) {
            // Coerce primary touch up to pointer up
            let (x, y) = pos.map(|(x, y, _)| (x, y)).unwrap_or((self.pointer_x, self.pointer_y));
            self.handle_pointer_up(x, y, MouseButton::Left, time, layout, root_element);
            self.primary_touch_id = None;
        }

        let (x, y) = pos.map(|(x, y, _)| (x, y)).unwrap_or((0.0, 0.0));
        let touch_event = TouchEvent {
            touch_id: id,
            x,
            y,
            width: None,
            height: None,
            orientation: None,
            time,
        };
        self.dispatch_touch_event(&touch_event, layout, root_element, TouchEventType::End);

        true
    }

    /// Handle all active touches being cancelled by the system.
    fn handle_touch_cancel(&mut self, root_element: &Element) -> bool {
        // Fire TouchCancel on all elements that have active touches
        let touch_event = TouchEvent {
            touch_id: 0,
            x: 0.0,
            y: 0.0,
            width: None,
            height: None,
            orientation: None,
            time: 0,
        };

        // Notify any hovered elements via their cancel handlers
        for &idx in &self.hovered {
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(h) = element.on_touch_cancel.as_ref() {
                    h(&touch_event);
                }
            }
        }

        // If there was a primary touch, simulate pointer leave
        if self.primary_touch_id.is_some() {
            self.handle_pointer_leave(root_element);
        }

        // Clear all touch state
        self.active_touches.clear();
        self.primary_touch_id = None;

        true
    }

    /// Dispatch a native touch event via target + bubble phases.
    fn dispatch_touch_event(
        &self,
        event: &TouchEvent,
        layout: &LayoutNode,
        root_element: &Element,
        event_type: TouchEventType,
    ) {
        // Hit test to find the target element
        let hit_path = hit_test(layout, root_element, event.x, event.y);
        if hit_path.is_empty() {
            return;
        }

        let target_idx = hit_path[0];
        let path = build_path_to_element(root_element, target_idx);
        if path.is_empty() {
            return;
        }

        let mut stopped = false;

        // Target phase
        let target_pos = path.len() - 1;
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            let handler = match event_type {
                TouchEventType::Start => element.on_touch_start.as_ref(),
                TouchEventType::Move => element.on_touch_move.as_ref(),
                TouchEventType::End => element.on_touch_end.as_ref(),
            };
            if let Some(h) = handler {
                let mut default_prevented = false;
                apply_event_result(h(event), &mut stopped, &mut default_prevented);
            }
        }

        // Bubble phase
        if !stopped {
            for &idx in path[..target_pos].iter().rev() {
                if stopped {
                    break;
                }
                if let Some(element) = get_element_by_preorder(root_element, idx) {
                    let handler = match event_type {
                        TouchEventType::Start => element.on_touch_start.as_ref(),
                        TouchEventType::Move => element.on_touch_move.as_ref(),
                        TouchEventType::End => element.on_touch_end.as_ref(),
                    };
                    if let Some(h) = handler {
                        let mut default_prevented = false;
                        apply_event_result(h(event), &mut stopped, &mut default_prevented);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Three-phase event dispatch helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum PointerEventType {
    Down,
    Up,
    Move,
}

/// Apply an EventResult to stopped/default_prevented flags.
fn apply_event_result(result: EventResult, stopped: &mut bool, default_prevented: &mut bool) {
    match result {
        EventResult::Continue => {}
        EventResult::Stop => *stopped = true,
        EventResult::StopAndPreventDefault => {
            *stopped = true;
            *default_prevented = true;
        }
        EventResult::PreventDefault => *default_prevented = true,
    }
}

/// Dispatch a pointer event through capture -> target -> bubble phases.
/// Returns true if any handler was invoked.
fn dispatch_pointer_event_three_phase(
    pe: &PointerEvent,
    path_deepest_to_root: &[usize],
    root_element: &Element,
    event_type: PointerEventType,
) -> bool {
    let (handled, _) =
        dispatch_pointer_event_three_phase_full(pe, path_deepest_to_root, root_element, event_type);
    handled
}

/// Dispatch a pointer event through capture -> target -> bubble phases.
/// Returns (any_handler_invoked, default_prevented).
fn dispatch_pointer_event_three_phase_full(
    pe: &PointerEvent,
    path_deepest_to_root: &[usize],
    root_element: &Element,
    event_type: PointerEventType,
) -> (bool, bool) {
    if path_deepest_to_root.is_empty() {
        return (false, false);
    }

    // Build root-to-target path
    let path: Vec<usize> = path_deepest_to_root.iter().copied().rev().collect();
    let target_pos = path.len() - 1;
    let mut stopped = false;
    let mut default_prevented = false;
    let mut handled = false;

    // Get the capture handler accessor and bubble handler accessor
    let capture_handler = match event_type {
        PointerEventType::Down => |e: &Element| e.on_pointer_down_capture.as_ref(),
        PointerEventType::Up => |e: &Element| e.on_pointer_up_capture.as_ref(),
        PointerEventType::Move => |e: &Element| e.on_pointer_move_capture.as_ref(),
    };

    let bubble_handler = match event_type {
        PointerEventType::Down => |e: &Element| e.on_pointer_down.as_ref(),
        PointerEventType::Up => |e: &Element| e.on_pointer_up.as_ref(),
        PointerEventType::Move => |e: &Element| e.on_pointer_move.as_ref(),
    };

    // 1. Capture phase (root to target's parent)
    for &idx in &path[..target_pos] {
        if stopped {
            break;
        }
        if let Some(element) = get_element_by_preorder(root_element, idx) {
            if let Some(handler) = capture_handler(element) {
                let r = handler(pe);
                apply_event_result(r, &mut stopped, &mut default_prevented);
                handled = true;
            }
        }
    }

    // 2. Target phase
    if !stopped {
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            // Fire capture handler on target
            if let Some(handler) = capture_handler(element) {
                let r = handler(pe);
                apply_event_result(r, &mut stopped, &mut default_prevented);
                handled = true;
            }
            // Fire bubble handler on target
            if !stopped {
                if let Some(handler) = bubble_handler(element) {
                    let r = handler(pe);
                    apply_event_result(r, &mut stopped, &mut default_prevented);
                    handled = true;
                }
            }
        }
    }

    // 3. Bubble phase (target's parent to root)
    if !stopped {
        for &idx in path[..target_pos].iter().rev() {
            if stopped {
                break;
            }
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(handler) = bubble_handler(element) {
                    let r = handler(pe);
                    apply_event_result(r, &mut stopped, &mut default_prevented);
                    handled = true;
                }
            }
        }
    }

    (handled, default_prevented)
}

/// Dispatch a Click event through three-phase propagation.
/// Returns true if any handler was invoked.
fn dispatch_click_event_three_phase(
    ce: &ClickEvent,
    path_deepest_to_root: &[usize],
    root_element: &Element,
) -> bool {
    if path_deepest_to_root.is_empty() {
        return false;
    }

    let path: Vec<usize> = path_deepest_to_root.iter().copied().rev().collect();
    let target_pos = path.len() - 1;
    let mut stopped = false;
    let mut default_prevented = false;
    let mut handled = false;

    // No capture handlers for click -- go straight to target
    // Target phase
    if !stopped {
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            if let Some(ref handler) = element.on_click {
                let r = handler(ce);
                apply_event_result(r, &mut stopped, &mut default_prevented);
                handled = true;
            }
        }
    }

    // Bubble phase
    if !stopped {
        for &idx in path[..target_pos].iter().rev() {
            if stopped {
                break;
            }
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(ref handler) = element.on_click {
                    let r = handler(ce);
                    apply_event_result(r, &mut stopped, &mut default_prevented);
                    handled = true;
                }
            }
        }
    }

    handled
}

/// Dispatch a DoubleClick event (bubble only, target + ancestors).
fn dispatch_double_click_event(
    ce: &ClickEvent,
    path_deepest_to_root: &[usize],
    root_element: &Element,
) -> bool {
    if path_deepest_to_root.is_empty() {
        return false;
    }

    // path is deepest-to-root, so first is target
    for &idx in path_deepest_to_root {
        if let Some(element) = get_element_by_preorder(root_element, idx) {
            if let Some(ref handler) = element.on_double_click {
                let r = handler(ce);
                if r == EventResult::Stop || r == EventResult::StopAndPreventDefault {
                    return true;
                }
                return true;
            }
        }
    }
    false
}

/// Dispatch a ContextMenu event (bubble, target + ancestors).
fn dispatch_context_menu_event(
    pe: &PointerEvent,
    path_deepest_to_root: &[usize],
    root_element: &Element,
) -> bool {
    for &idx in path_deepest_to_root {
        if let Some(element) = get_element_by_preorder(root_element, idx) {
            if let Some(ref handler) = element.on_context_menu {
                let r = handler(pe);
                if r == EventResult::Stop || r == EventResult::StopAndPreventDefault {
                    return true;
                }
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Text input event dispatch helpers
// ---------------------------------------------------------------------------

/// Dispatch a BeforeInput event through three-phase propagation.
/// `path` is root-to-target order.
/// Returns (any_handler_invoked, default_prevented).
fn dispatch_before_input_event(
    event: &BeforeInputEvent,
    path: &[usize],
    root_element: &Element,
) -> (bool, bool) {
    if path.is_empty() {
        return (false, false);
    }

    let target_pos = path.len() - 1;
    let mut stopped = false;
    let mut default_prevented = false;
    let mut handled = false;

    // Capture phase — no capture handlers for BeforeInput yet

    // Target phase
    if !stopped {
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            if let Some(ref handler) = element.on_before_input {
                let r = handler(event);
                apply_event_result(r, &mut stopped, &mut default_prevented);
                handled = true;
            }
        }
    }

    // Bubble phase (target's parent to root)
    if !stopped {
        for &idx in path[..target_pos].iter().rev() {
            if stopped {
                break;
            }
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(ref handler) = element.on_before_input {
                    let r = handler(event);
                    apply_event_result(r, &mut stopped, &mut default_prevented);
                    handled = true;
                }
            }
        }
    }

    (handled, default_prevented)
}

/// Dispatch an Input event through three-phase propagation.
/// `path` is root-to-target order. Input is not cancelable.
fn dispatch_input_event(
    event: &TextInputEvent,
    path: &[usize],
    root_element: &Element,
) {
    if path.is_empty() {
        return;
    }

    let target_pos = path.len() - 1;
    let mut stopped = false;

    // Capture phase — no capture handlers for Input yet

    // Target phase
    if !stopped {
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            if let Some(ref handler) = element.on_input {
                handler(event);
            }
        }
    }

    // Bubble phase (target's parent to root)
    if !stopped {
        for &idx in path[..target_pos].iter().rev() {
            if stopped {
                break;
            }
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                if let Some(ref handler) = element.on_input {
                    handler(event);
                    // Input is not cancelable but still propagates; we don't stop
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Composition event dispatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum CompositionPhase {
    Start,
    Update,
    End,
}

fn dispatch_composition_event(
    event: &crate::input::CompositionEvent,
    phase: CompositionPhase,
    path: &[usize],
    root_element: &Element,
) {
    if path.is_empty() {
        return;
    }

    let target_pos = path.len() - 1;
    let mut stopped = false;

    // Target phase
    {
        let target = path[target_pos];
        if let Some(element) = get_element_by_preorder(root_element, target) {
            match phase {
                CompositionPhase::Start => {
                    if let Some(ref handler) = element.on_composition_start {
                        let r = handler(event);
                        let mut dp = false;
                        apply_event_result(r, &mut stopped, &mut dp);
                    }
                }
                CompositionPhase::Update => {
                    if let Some(ref handler) = element.on_composition_update {
                        handler(event);
                    }
                }
                CompositionPhase::End => {
                    if let Some(ref handler) = element.on_composition_end {
                        handler(event);
                    }
                }
            }
        }
    }

    // Bubble phase
    if !stopped {
        for &idx in path[..target_pos].iter().rev() {
            if stopped {
                break;
            }
            if let Some(element) = get_element_by_preorder(root_element, idx) {
                match phase {
                    CompositionPhase::Start => {
                        if let Some(ref handler) = element.on_composition_start {
                            let r = handler(event);
                            let mut dp = false;
                            apply_event_result(r, &mut stopped, &mut dp);
                        }
                    }
                    CompositionPhase::Update => {
                        if let Some(ref handler) = element.on_composition_update {
                            handler(event);
                        }
                    }
                    CompositionPhase::End => {
                        if let Some(ref handler) = element.on_composition_end {
                            handler(event);
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Focus helpers
// ---------------------------------------------------------------------------

/// Check if an element is focusable.
fn element_is_focusable(element: &Element) -> bool {
    // Explicit override
    if let Some(focusable) = element.focusable {
        return focusable;
    }
    // Default: text inputs, textareas, interactive form elements, and elements with click handlers
    matches!(
        element.kind,
        crate::element::ElementKind::TextInput { .. }
            | crate::element::ElementKind::Textarea { .. }
            | crate::element::ElementKind::Button { .. }
            | crate::element::ElementKind::Checkbox { .. }
            | crate::element::ElementKind::Radio { .. }
            | crate::element::ElementKind::Switch { .. }
            | crate::element::ElementKind::Slider { .. }
            | crate::element::ElementKind::RangeSlider { .. }
            | crate::element::ElementKind::Select { .. }
    ) || element.on_click.is_some()
}

// ---------------------------------------------------------------------------
// Element lookup
// ---------------------------------------------------------------------------

/// Get an element by its pre-order index in the tree.
fn get_element_by_preorder(root: &Element, target: usize) -> Option<&Element> {
    let mut counter = 0;
    get_element_recursive(root, target, &mut counter)
}

fn get_element_recursive<'a>(
    element: &'a Element,
    target: usize,
    counter: &mut usize,
) -> Option<&'a Element> {
    if *counter == target {
        return Some(element);
    }
    *counter += 1;
    for child in &element.children {
        if let Some(found) = get_element_recursive(child, target, counter) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Path building (root to target)
// ---------------------------------------------------------------------------

/// Build the path from root to a target element by pre-order index.
/// Returns pre-order indices from root to target.
fn build_path_to_element(root: &Element, target: usize) -> Vec<usize> {
    let mut path = Vec::new();
    build_path_recursive(root, target, &mut 0, &mut path);
    path
}

fn build_path_recursive(
    element: &Element,
    target: usize,
    counter: &mut usize,
    path: &mut Vec<usize>,
) -> bool {
    let current = *counter;
    if current == target {
        path.push(current);
        return true;
    }
    *counter += 1;
    for child in &element.children {
        path.push(current);
        if build_path_recursive(child, target, counter, path) {
            return true;
        }
        path.pop();
    }
    false
}

// ---------------------------------------------------------------------------
// Tab order
// ---------------------------------------------------------------------------

/// Build ordered list of tabbable elements. If focus is inside a focus trap,
/// only elements within that trap are included.
fn build_tab_order(root: &Element, focused: Option<usize>) -> Vec<usize> {
    // First check if the focused element is inside a focus trap
    let trap_root = focused.and_then(|f| find_focus_trap_ancestor(root, f));

    let mut explicit: Vec<(i32, usize)> = Vec::new(); // (tab_index, preorder)
    let mut auto: Vec<usize> = Vec::new();

    collect_tabbable(root, &mut 0, &mut explicit, &mut auto, trap_root);

    // Sort explicit by tab_index ascending, then document order
    explicit.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut order: Vec<usize> = explicit.into_iter().map(|(_, idx)| idx).collect();
    order.extend(auto);
    order
}

fn collect_tabbable(
    element: &Element,
    counter: &mut usize,
    explicit: &mut Vec<(i32, usize)>,
    auto: &mut Vec<usize>,
    trap_root: Option<usize>,
) {
    let idx = *counter;

    // If there's a focus trap, only collect elements at or below the trap root
    let in_scope = trap_root.map_or(true, |trap| idx >= trap);

    if in_scope && element_is_tabbable(element) {
        match element.tab_index {
            Some(n) if n > 0 => explicit.push((n, idx)),
            _ => auto.push(idx),
        }
    }

    *counter += 1;
    for child in &element.children {
        collect_tabbable(child, counter, explicit, auto, trap_root);
    }
}

fn element_is_tabbable(element: &Element) -> bool {
    // Disabled elements are not tabbable
    if element.disabled {
        return false;
    }
    // Explicitly set
    if let Some(focusable) = element.focusable {
        if !focusable {
            return false;
        }
        // focusable=true + tab_index != None means tabbable
        // focusable=true + tab_index == None means focusable but not tabbable
        return element.tab_index.is_some();
    }
    // Default tabbable: text inputs, textareas, interactive form elements, and elements with on_click
    matches!(
        element.kind,
        crate::element::ElementKind::TextInput { .. }
            | crate::element::ElementKind::Textarea { .. }
            | crate::element::ElementKind::Button { .. }
            | crate::element::ElementKind::Checkbox { .. }
            | crate::element::ElementKind::Radio { .. }
            | crate::element::ElementKind::Switch { .. }
            | crate::element::ElementKind::Slider { .. }
            | crate::element::ElementKind::RangeSlider { .. }
            | crate::element::ElementKind::Select { .. }
    ) || element.on_click.is_some()
}

/// Find the nearest focus_trap ancestor for a given element index.
fn find_focus_trap_ancestor(root: &Element, target: usize) -> Option<usize> {
    let path = build_path_to_element(root, target);
    // Walk from root toward target looking for focus_trap
    for &idx in path.iter().rev() {
        if let Some(element) = get_element_by_preorder(root, idx) {
            if element.focus_trap {
                return Some(idx);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Keysym to KeyValue translation
// ---------------------------------------------------------------------------

/// Convert an XKB keysym (u32) to a KeyValue.
fn keysym_to_key_value(keysym: u32) -> KeyValue {
    // XKB keysym constants
    match keysym {
        0xff0d => KeyValue::Enter,
        0xff09 => KeyValue::Tab,
        0xff1b => KeyValue::Escape,
        0xff08 => KeyValue::Backspace,
        0xffff => KeyValue::Delete,
        0xff52 => KeyValue::ArrowUp,
        0xff54 => KeyValue::ArrowDown,
        0xff51 => KeyValue::ArrowLeft,
        0xff53 => KeyValue::ArrowRight,
        0xff50 => KeyValue::Home,
        0xff57 => KeyValue::End,
        0xff55 => KeyValue::PageUp,
        0xff56 => KeyValue::PageDown,
        0x0020 => KeyValue::Space,
        0xff63 => KeyValue::Insert,
        // Function keys F1-F24
        k if (0xffbe..=0xffd5).contains(&k) => KeyValue::F((k - 0xffbe + 1) as u8),
        // Modifier keys
        0xffe1 | 0xffe2 => KeyValue::Shift,
        0xffe3 | 0xffe4 => KeyValue::Control,
        0xffe9 | 0xffea => KeyValue::Alt,
        0xffeb | 0xffec => KeyValue::Super,
        0xffe5 => KeyValue::CapsLock,
        0xff7f => KeyValue::NumLock,
        // Media keys
        0x1008ff14 => KeyValue::MediaPlay,
        0x1008ff31 => KeyValue::MediaPause,
        0x1008ff15 => KeyValue::MediaStop,
        0x1008ff17 => KeyValue::MediaNext,
        0x1008ff16 => KeyValue::MediaPrev,
        0x1008ff13 => KeyValue::AudioVolumeUp,
        0x1008ff11 => KeyValue::AudioVolumeDown,
        0x1008ff12 => KeyValue::AudioVolumeMute,
        // Printable characters (Latin-1 range)
        k if (0x0020..=0x007e).contains(&k) => {
            KeyValue::Character(char::from_u32(k).unwrap_or('?').to_string())
        }
        k => KeyValue::Unknown(k),
    }
}

// ---------------------------------------------------------------------------
// Form element click/keyboard dispatch
// ---------------------------------------------------------------------------

/// Dispatch a click on a form element, firing the appropriate typed callback.
/// Returns true if a callback was fired.
fn dispatch_form_element_click(element: &Element) -> bool {
    use crate::element::ElementKind;
    match &element.kind {
        ElementKind::Checkbox {
            checked,
            indeterminate,
            ..
        } => {
            if let Some(ref handler) = element.on_bool_change {
                let new_val = if *indeterminate { true } else { !checked };
                handler(new_val);
                return true;
            }
        }
        ElementKind::Switch { on, .. } => {
            if let Some(ref handler) = element.on_bool_change {
                handler(!on);
                return true;
            }
        }
        ElementKind::Radio { value, .. } => {
            if let Some(ref handler) = element.on_change {
                handler(value.clone());
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Dispatch a keyboard activation (Space/Enter) on a focused form element.
/// Returns true if a callback was fired.
fn dispatch_form_element_activate(element: &Element) -> bool {
    use crate::element::ElementKind;
    match &element.kind {
        ElementKind::Button { .. } => {
            if let Some(ref handler) = element.on_click {
                let ce = ClickEvent {
                    x: 0.0,
                    y: 0.0,
                    button: MouseButton::Left,
                    count: 1,
                    modifiers: Modifiers::default(),
                    pointer_type: PointerType::Mouse,
                };
                handler(&ce);
                return true;
            }
        }
        ElementKind::Checkbox { .. } | ElementKind::Switch { .. } | ElementKind::Radio { .. } => {
            return dispatch_form_element_click(element);
        }
        _ => {}
    }
    false
}

/// Calculate a slider value from a pointer x-position within the element bounds.
pub fn slider_value_from_x(
    x: f32,
    bounds_x: f32,
    bounds_width: f32,
    min: f64,
    max: f64,
    step: Option<f64>,
) -> f64 {
    let ratio = ((x - bounds_x) / bounds_width).clamp(0.0, 1.0) as f64;
    let raw = min + ratio * (max - min);
    if let Some(s) = step {
        if s > 0.0 {
            (((raw - min) / s).round() * s + min).clamp(min, max)
        } else {
            raw
        }
    } else {
        raw
    }
}

/// Find the layout node for a given element pre-order index.
fn find_layout_node_bounds(layout: &LayoutNode, target_idx: usize) -> Option<(f32, f32, f32, f32)> {
    if layout.element_index == target_idx {
        return Some((layout.bounds.x, layout.bounds.y, layout.bounds.width, layout.bounds.height));
    }
    for child in &layout.children {
        if let Some(bounds) = find_layout_node_bounds(child, target_idx) {
            return Some(bounds);
        }
    }
    None
}

/// Adjust a slider value by a step increment (for arrow key handling).
pub fn slider_step_value(
    current: f64,
    min: f64,
    max: f64,
    step: Option<f64>,
    direction: i32,
) -> f64 {
    let step_size = step.unwrap_or((max - min) / 100.0);
    let new_val = current + (direction as f64) * step_size;
    new_val.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    // --- Slider math tests ---

    #[test]
    fn test_slider_value_from_x_center() {
        let val = slider_value_from_x(150.0, 100.0, 200.0, 0.0, 100.0, None);
        assert!((val - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_at_start() {
        let val = slider_value_from_x(100.0, 100.0, 200.0, 0.0, 100.0, None);
        assert!((val - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_at_end() {
        let val = slider_value_from_x(300.0, 100.0, 200.0, 0.0, 100.0, None);
        assert!((val - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_clamped_below() {
        let val = slider_value_from_x(50.0, 100.0, 200.0, 0.0, 100.0, None);
        assert!((val - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_clamped_above() {
        let val = slider_value_from_x(400.0, 100.0, 200.0, 0.0, 100.0, None);
        assert!((val - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_with_step() {
        // At x=160 (ratio=0.3), raw=30.0, step=10 -> snaps to 30.0
        let val = slider_value_from_x(160.0, 100.0, 200.0, 0.0, 100.0, Some(10.0));
        assert!((val - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_step_snapping() {
        // At x=155 (ratio=0.275), raw=27.5, step=10 -> rounds to 30.0
        let val = slider_value_from_x(155.0, 100.0, 200.0, 0.0, 100.0, Some(10.0));
        assert!((val - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_value_from_x_custom_range() {
        // Range -50..50, at midpoint
        let val = slider_value_from_x(200.0, 100.0, 200.0, -50.0, 50.0, None);
        assert!((val - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_step_value_increment() {
        let val = slider_step_value(50.0, 0.0, 100.0, Some(5.0), 1);
        assert!((val - 55.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_step_value_decrement() {
        let val = slider_step_value(50.0, 0.0, 100.0, Some(5.0), -1);
        assert!((val - 45.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_step_value_clamp_max() {
        let val = slider_step_value(98.0, 0.0, 100.0, Some(5.0), 1);
        assert!((val - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_step_value_clamp_min() {
        let val = slider_step_value(2.0, 0.0, 100.0, Some(5.0), -1);
        assert!((val - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_slider_step_value_no_step() {
        // Default step = (100-0)/100 = 1.0
        let val = slider_step_value(50.0, 0.0, 100.0, None, 1);
        assert!((val - 51.0).abs() < 0.01);
    }

    // --- Form element click dispatch tests ---

    #[test]
    fn test_checkbox_click_toggles() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = checkbox(false).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        let fired = dispatch_form_element_click(&elem);
        assert!(fired);
        assert_eq!(*toggled.borrow(), Some(true));
    }

    #[test]
    fn test_checkbox_click_unchecks() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = checkbox(true).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        let fired = dispatch_form_element_click(&elem);
        assert!(fired);
        assert_eq!(*toggled.borrow(), Some(false));
    }

    #[test]
    fn test_checkbox_indeterminate_becomes_checked() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = checkbox(false).indeterminate(true).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_click(&elem);
        assert_eq!(*toggled.borrow(), Some(true));
    }

    #[test]
    fn test_switch_click_toggles_on() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = switch(false).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_click(&elem);
        assert_eq!(*toggled.borrow(), Some(true));
    }

    #[test]
    fn test_switch_click_toggles_off() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = switch(true).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_click(&elem);
        assert_eq!(*toggled.borrow(), Some(false));
    }

    #[test]
    fn test_radio_click_fires_value() {
        let selected = Rc::new(RefCell::new(None));
        let selected_clone = selected.clone();

        let elem = radio(false, "size", "large").on_change(move |val| {
            *selected_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_click(&elem);
        assert_eq!(*selected.borrow(), Some("large".to_string()));
    }

    #[test]
    fn test_checkbox_no_handler_returns_false() {
        let elem = checkbox(false);
        assert!(!dispatch_form_element_click(&elem));
    }

    #[test]
    fn test_container_click_returns_false() {
        let elem = container();
        assert!(!dispatch_form_element_click(&elem));
    }

    // --- Form element keyboard activation tests ---

    #[test]
    fn test_button_activate_fires_click() {
        let clicked = Rc::new(RefCell::new(false));
        let clicked_clone = clicked.clone();

        let elem = button("Submit").on_click(move |_| {
            *clicked_clone.borrow_mut() = true;
            EventResult::Stop
        });

        let fired = dispatch_form_element_activate(&elem);
        assert!(fired);
        assert!(*clicked.borrow());
    }

    #[test]
    fn test_checkbox_activate_toggles() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = checkbox(false).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_activate(&elem);
        assert_eq!(*toggled.borrow(), Some(true));
    }

    #[test]
    fn test_switch_activate_toggles() {
        let toggled = Rc::new(RefCell::new(None));
        let toggled_clone = toggled.clone();

        let elem = switch(true).on_toggle(move |val| {
            *toggled_clone.borrow_mut() = Some(val);
        });

        dispatch_form_element_activate(&elem);
        assert_eq!(*toggled.borrow(), Some(false));
    }

    // --- Focusability tests ---

    #[test]
    fn test_button_is_focusable() {
        let elem = button("Click me");
        assert!(element_is_focusable(&elem));
    }

    #[test]
    fn test_checkbox_is_focusable() {
        let elem = checkbox(false);
        assert!(element_is_focusable(&elem));
    }

    #[test]
    fn test_radio_is_focusable() {
        let elem = radio(false, "g", "v");
        assert!(element_is_focusable(&elem));
    }

    #[test]
    fn test_switch_is_focusable() {
        let elem = switch(false);
        assert!(element_is_focusable(&elem));
    }

    #[test]
    fn test_slider_is_focusable() {
        let elem = slider(50.0, 0.0, 100.0);
        assert!(element_is_focusable(&elem));
    }

    #[test]
    fn test_disabled_not_focusable() {
        let elem = button("Click me").disabled(true);
        assert!(!element_is_focusable(&elem));
    }

    #[test]
    fn test_container_not_focusable_by_default() {
        let elem = container();
        assert!(!element_is_focusable(&elem));
    }

    // --- Tabbability tests ---

    #[test]
    fn test_button_is_tabbable() {
        let elem = button("Click me");
        assert!(element_is_tabbable(&elem));
    }

    #[test]
    fn test_disabled_not_tabbable() {
        let elem = checkbox(false).disabled(true);
        assert!(!element_is_tabbable(&elem));
    }

    #[test]
    fn test_slider_is_tabbable() {
        let elem = slider(50.0, 0.0, 100.0);
        assert!(element_is_tabbable(&elem));
    }
}
