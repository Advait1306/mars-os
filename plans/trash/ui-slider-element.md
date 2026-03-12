# Plan: Slider Element for UI Framework

## Problem

The header's volume popup has a slider control (track + fill + draggable knob). The UI framework has no slider primitive.

## Design

Add a `slider(value)` builder that creates a **visual-only** slider element. The framework handles rendering (track, fill, knob). Interaction is handled by the View attaching `on_drag` to the element — no special slider logic in EventState.

### API

```rust
// In ui/src/element.rs
pub fn slider(value: f32) -> Element {
    Element {
        kind: ElementKind::Slider { value: value.clamp(0.0, 1.0) },
        height: Some(24.0),  // default height, overridable
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}
```

Usage:

```rust
slider(self.volume)
    .width(160.0)
    .on_drag({
        let handle = cx.handle::<Self>();
        let initial = self.volume;
        let width = 160.0;
        move |dx, _dy| {
            let new_val = (initial + dx / width).clamp(0.0, 1.0);
            handle.update(move |v| {
                v.volume = new_val;
                controls::set_volume(new_val);
            });
        }
    })
```

This works because each render captures the current `self.volume` as `initial`. When a drag starts, `EventState` records the press position and `on_drag` receives `(delta_x, delta_y)` from there. So `initial + dx / width` gives the correct value.

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Slider { value: f32 },
}
```

### Rendering (`ui/src/display_list.rs`)

The slider renders as three draw commands:

1. **Track background** — full-width rounded rect, ~4px tall, centered vertically
2. **Track fill** — rounded rect from left edge to `value * width`
3. **Knob** — circle (rounded rect with `corner_radius = half size`) at the value position

```rust
ElementKind::Slider { value } => {
    let bounds = &layout_node.bounds;
    let track_h = 4.0;
    let knob_r = 6.0;
    let cy = bounds.y + bounds.height / 2.0;
    let track_y = cy - track_h / 2.0;
    let usable_w = bounds.width - knob_r * 2.0;
    let knob_x = bounds.x + knob_r + usable_w * value;

    // Track background
    commands.push(DrawCommand::Rect {
        x: bounds.x, y: track_y, w: bounds.width, h: track_h,
        color: Color::rgba(255, 255, 255, 30),
        corner_radius: 2.0, border: None,
    });
    // Track fill
    let fill_w = knob_x - bounds.x;
    commands.push(DrawCommand::Rect {
        x: bounds.x, y: track_y, w: fill_w, h: track_h,
        color: Color::rgba(255, 255, 255, 180),
        corner_radius: 2.0, border: None,
    });
    // Knob
    commands.push(DrawCommand::Rect {
        x: knob_x - knob_r, y: cy - knob_r, w: knob_r * 2.0, h: knob_r * 2.0,
        color: Color::rgba(255, 255, 255, 230),
        corner_radius: knob_r, border: None,
    });
}
```

No new `DrawCommand` variants needed.

### Layout (`ui/src/layout.rs`)

Treat `ElementKind::Slider` as a leaf node in `build_taffy_tree` — uses `width`/`height` properties for sizing. No custom measure function needed.

### Hit testing (`ui/src/hit_test.rs`)

Add `ElementKind::Slider` to the interactivity check so it's always hittable:

```rust
fn is_interactive(element: &Element) -> bool {
    // ... existing checks ...
    || matches!(element.kind, ElementKind::Slider { .. })
}
```

## File Changes

| File                     | Changes                                                                             |
| ------------------------ | ----------------------------------------------------------------------------------- |
| `ui/src/element.rs`      | Add `Slider { value: f32 }` to `ElementKind`, add `slider(value)` builder function |
| `ui/src/display_list.rs` | Add slider rendering (track bg, fill, knob) as `DrawCommand::Rect` entries          |
| `ui/src/layout.rs`       | Handle `Slider` as a leaf node in taffy tree building                               |
| `ui/src/hit_test.rs`     | Add `Slider` to interactivity check                                                |

## Styling Future Extension

The slider colors are hardcoded initially (matching the header's dark theme). A future pass could add `.track_color()`, `.fill_color()`, `.knob_color()` builder methods to `Element` if other uses need different styling.
