//! Hit testing through the layout tree.
//!
//! Given a pointer position, walks the layout + element trees to find the
//! deepest interactive element under the cursor. Returns the path from
//! deepest element to root as pre-order indices into the element tree.

use crate::element::Element;
use crate::input::PointerEvents;
use crate::layout::{LayoutNode, Rect};

/// Result of a hit test -- the path from root to the deepest hit element.
pub struct HitResult {
    /// Pre-order indices into the element tree, from deepest to root.
    pub path: Vec<usize>,
}

/// Perform hit testing: find the deepest interactive element at (x, y).
/// Returns `Some(HitResult)` with the path from deepest element to root,
/// or `None` if no interactive element is under the pointer.
pub fn hit_test(
    layout: &LayoutNode,
    element: &Element,
    x: f32,
    y: f32,
) -> Option<HitResult> {
    let mut path = Vec::new();
    if hit_test_recursive(layout, element, x, y, &mut path, 0) {
        Some(HitResult { path })
    } else {
        None
    }
}

fn hit_test_recursive(
    node: &LayoutNode,
    element: &Element,
    x: f32,
    y: f32,
    path: &mut Vec<usize>,
    index: usize,
) -> bool {
    // pointer_events: None — skip this element and all children
    if element.pointer_events == PointerEvents::None {
        return false;
    }

    // Check if point is inside this node's bounds
    if !bounds_contains(&node.bounds, x, y) {
        return false;
    }

    // Compute pre-order indices for each child
    let mut child_indices = Vec::with_capacity(node.children.len());
    let mut ci = index + 1;
    for child_element in &element.children {
        child_indices.push(ci);
        ci += count_elements(child_element);
    }

    // Build child order: sort by z-index (descending) for hit testing
    let mut child_order: Vec<usize> = (0..node.children.len()).collect();
    child_order.sort_by(|a, b| {
        let z_a = element.children.get(*a).and_then(|e| e.z_index).unwrap_or(0);
        let z_b = element.children.get(*b).and_then(|e| e.z_index).unwrap_or(0);
        z_b.cmp(&z_a).then_with(|| b.cmp(a)) // higher z-index first, then reverse order
    });

    // Check children in z-index-aware order
    for i in child_order {
        if i < element.children.len() {
            if hit_test_recursive(
                &node.children[i],
                &element.children[i],
                x,
                y,
                path,
                child_indices[i],
            ) {
                path.push(index);
                return true;
            }
        }
    }

    // pointer_events: PassThrough — skip self but children were already checked
    if element.pointer_events == PointerEvents::PassThrough {
        return false;
    }

    // No child matched -- check if this element itself is interactive
    if is_interactive(element) {
        path.push(index);
        return true;
    }

    false
}

fn bounds_contains(bounds: &Rect, x: f32, y: f32) -> bool {
    x >= bounds.x
        && x < bounds.x + bounds.width
        && y >= bounds.y
        && y < bounds.y + bounds.height
}

fn is_interactive(element: &Element) -> bool {
    // New event handlers
    element.on_pointer_down.is_some()
        || element.on_pointer_up.is_some()
        || element.on_pointer_move.is_some()
        || element.on_pointer_enter.is_some()
        || element.on_pointer_leave.is_some()
        || element.on_pointer_down_capture.is_some()
        || element.on_pointer_up_capture.is_some()
        || element.on_pointer_move_capture.is_some()
        || element.on_click.is_some()
        || element.on_double_click.is_some()
        || element.on_context_menu.is_some()
        || element.on_key_down.is_some()
        || element.on_key_up.is_some()
        || element.on_focus.is_some()
        || element.on_blur.is_some()
        || element.on_focus_in.is_some()
        || element.on_focus_out.is_some()
        || element.on_wheel.is_some()
        || element.on_scroll_end.is_some()
        // Text input
        || element.on_before_input.is_some()
        || element.on_input.is_some()
        // Composition
        || element.on_composition_start.is_some()
        || element.on_composition_update.is_some()
        || element.on_composition_end.is_some()
        // Legacy handlers
        || element.on_hover.is_some()
        || element.on_drag.is_some()
        || element.on_scroll.is_some()
        || element.on_change.is_some()
        // Other interactive properties
        || element.scroll_direction.is_some()
        || element.cursor.is_some()
        || element.focusable == Some(true)
}

fn count_elements(element: &Element) -> usize {
    1 + element
        .children
        .iter()
        .map(|c| count_elements(c))
        .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::*;
    use crate::input::{CursorStyle, EventResult, PointerEvents};
    use crate::layout::compute_layout;
    use skia_safe::textlayout::FontCollection;
    use skia_safe::FontMgr;

    fn test_fc() -> FontCollection {
        let mut fc = FontCollection::new();
        fc.set_default_font_manager(FontMgr::default(), None);
        fc
    }

    #[test]
    fn test_hit_interactive_element() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .child(
                container()
                    .width(50.0)
                    .height(50.0)
                    .on_click(|_| EventResult::Stop),
            );
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let result = hit_test(&layout, &tree, 25.0, 25.0);
        assert!(result.is_some());
        let path = result.unwrap().path;
        // Deepest element (the clickable child) is first
        assert!(!path.is_empty());
    }

    #[test]
    fn test_miss_outside_bounds() {
        let tree = container()
            .width(100.0)
            .height(100.0)
            .on_click(|_| EventResult::Stop);
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let result = hit_test(&layout, &tree, 200.0, 200.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_non_interactive_not_hit() {
        let tree = container().width(100.0).height(100.0);
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let result = hit_test(&layout, &tree, 50.0, 50.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_cursor_style_makes_interactive() {
        let tree = container()
            .width(100.0)
            .height(100.0)
            .cursor(CursorStyle::Pointer);
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let result = hit_test(&layout, &tree, 50.0, 50.0);
        assert!(result.is_some());
    }
}
