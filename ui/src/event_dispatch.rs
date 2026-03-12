//! Event dispatching: hover tracking, click detection, drag, scroll.
//!
//! `EventState` tracks pointer state and dispatches `InputEvent`s against
//! the current layout and element tree, invoking event handlers on elements.

use std::collections::HashSet;

use crate::element::Element;
use crate::hit_test::hit_test;
use crate::input::{CursorStyle, InputEvent};
use crate::layout::LayoutNode;

/// Tracks pointer state for event dispatch.
pub struct EventState {
    /// Currently hovered element indices (deepest to root path).
    hovered: Vec<usize>,
    /// Element that received pointer-down (for click detection).
    pressed_element: Option<usize>,
    /// Position at pointer-down (for drag threshold).
    press_pos: Option<(f32, f32)>,
    /// Whether we are in a drag gesture.
    dragging: bool,
    /// Current pointer position.
    pointer_x: f32,
    pointer_y: f32,
    /// Current cursor style to set on the Wayland surface.
    pub current_cursor: CursorStyle,
}

impl EventState {
    pub fn new() -> Self {
        Self {
            hovered: Vec::new(),
            pressed_element: None,
            press_pos: None,
            dragging: false,
            pointer_x: 0.0,
            pointer_y: 0.0,
            current_cursor: CursorStyle::Default,
        }
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
                self.pointer_x = *x;
                self.pointer_y = *y;

                let hit = hit_test(layout, root_element, *x, *y);
                let new_hovered: Vec<usize> = hit
                    .as_ref()
                    .map(|h| h.path.clone())
                    .unwrap_or_default();

                // Compute hover enter/leave
                let old_set: HashSet<usize> = self.hovered.iter().copied().collect();
                let new_set: HashSet<usize> = new_hovered.iter().copied().collect();

                let mut needs_redraw = false;

                // Elements that lost hover
                for &idx in &self.hovered {
                    if !new_set.contains(&idx) {
                        if let Some(element) = get_element_by_preorder(root_element, idx) {
                            if let Some(ref handler) = element.on_hover {
                                handler(false);
                                needs_redraw = true;
                            }
                        }
                    }
                }

                // Elements that gained hover
                for &idx in &new_hovered {
                    if !old_set.contains(&idx) {
                        if let Some(element) = get_element_by_preorder(root_element, idx) {
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

                // Drag threshold detection
                if self.pressed_element.is_some() && !self.dragging {
                    if let Some((px, py)) = self.press_pos {
                        let dx = x - px;
                        let dy = y - py;
                        if (dx * dx + dy * dy).sqrt() > 3.0 {
                            self.dragging = true;
                        }
                    }
                }

                // Drag handling
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

            InputEvent::PointerButton {
                x,
                y,
                button: _,
                pressed,
            } => {
                if *pressed {
                    let hit = hit_test(layout, root_element, *x, *y);
                    self.pressed_element =
                        hit.as_ref().and_then(|h| h.path.first().copied());
                    self.press_pos = Some((*x, *y));
                    self.dragging = false;
                    false
                } else {
                    let mut needs_redraw = false;

                    if !self.dragging {
                        // Click: fire on_click if release is on the same element as press
                        if let Some(pressed_idx) = self.pressed_element {
                            let hit = hit_test(layout, root_element, *x, *y);
                            let release_on_same = hit
                                .as_ref()
                                .map(|h| h.path.contains(&pressed_idx))
                                .unwrap_or(false);

                            if release_on_same {
                                if let Some(element) =
                                    get_element_by_preorder(root_element, pressed_idx)
                                {
                                    if let Some(ref handler) = element.on_click {
                                        handler();
                                        needs_redraw = true;
                                    }
                                }
                            }
                        }
                    }

                    self.pressed_element = None;
                    self.press_pos = None;
                    self.dragging = false;
                    needs_redraw
                }
            }

            InputEvent::PointerScroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                let hit = hit_test(layout, root_element, *x, *y);
                if let Some(result) = hit {
                    // Bubble scroll up the path until an element handles it
                    for &idx in &result.path {
                        if let Some(element) = get_element_by_preorder(root_element, idx) {
                            if let Some(ref handler) = element.on_scroll {
                                handler(*delta_x, *delta_y);
                                return true;
                            }
                        }
                    }
                }
                false
            }

            InputEvent::PointerLeave => {
                // Fire on_hover(false) for all currently hovered elements
                for &idx in &self.hovered {
                    if let Some(element) = get_element_by_preorder(root_element, idx) {
                        if let Some(ref handler) = element.on_hover {
                            handler(false);
                        }
                    }
                }
                self.hovered.clear();
                self.current_cursor = CursorStyle::Default;
                true
            }

            // Keyboard events will be handled in a later phase
            _ => false,
        }
    }
}

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
