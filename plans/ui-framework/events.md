# Event & Input System

## Overview

The event system routes Wayland input events (pointer, keyboard, touch) through the element tree to handler closures. It builds on the existing layout pass — after Taffy computes bounds for every element, the event system uses those bounds for hit testing and event dispatch.

## Wayland Input

Input arrives through smithay-client-toolkit's `Seat` abstraction:

- **`wl_pointer`** — enter, leave, motion, button, axis (scroll)
- **`wl_keyboard`** — enter, leave, key, modifiers, repeat
- **`wl_touch`** — down, up, motion, cancel (future)

The framework's event loop receives these as raw events with surface-local coordinates and converts them to framework events:

```rust
enum InputEvent {
    PointerMove { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, button: MouseButton, pressed: bool },
    PointerScroll { x: f32, y: f32, delta_x: f32, delta_y: f32 },
    PointerLeave,

    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    TextInput { text: String },          // from wl_keyboard compose/IME

    TouchDown { id: i32, x: f32, y: f32 },
    TouchMove { id: i32, x: f32, y: f32 },
    TouchUp { id: i32 },
}

struct Modifiers {
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_: bool,
}
```

`Key` is an enum wrapping `xkbcommon` keysyms — letters, digits, arrows, function keys, etc. The framework uses `xkb_state` to handle keymap, dead keys, and compose sequences. `TextInput` carries the composed text string for text entry.

## Hit Testing

After layout, every element has resolved bounds (`Rect { x, y, width, height }`) and a tree position. Hit testing finds the target element for a pointer position.

### Algorithm

Walk the element tree **front-to-back** (reverse child order, since later children paint on top):

```
fn hit_test(node: &LayoutNode, point: Point) -> Option<&LayoutNode> {
    // If this node clips, reject points outside clip bounds
    if node.clips && !node.bounds.contains(point) {
        return None;
    }

    // Check children in reverse order (front-to-back)
    for child in node.children.iter().rev() {
        if let Some(hit) = hit_test(child, point) {
            return Some(hit);
        }
    }

    // Check self — only if bounds contain point and node has any handler
    if node.bounds.contains(point) && node.is_interactive() {
        return Some(node);
    }

    None
}
```

A node is **interactive** if it has any event handler (click, hover, drag, scroll, focus) or is a `text_input()`. Non-interactive containers pass through — their children can still be hit.

### Rounded corners

For elements with `rounded()`, hit testing uses the rounded rect shape, not the axis-aligned bounding box. This prevents clicking on the transparent corner area of a pill-shaped button.

```
fn rounded_rect_contains(bounds: Rect, radius: f32, point: Point) -> bool {
    // Fast path: point not in bounding rect
    if !bounds.contains(point) { return false; }
    // Check corner circles for points in corner regions
    // ... (standard rounded rect point-in-shape test)
}
```

### Opacity / visibility

Elements with `opacity(0.0)` are still hit-testable (same as CSS). To make an element non-interactive, skip attaching handlers. A future `.pointer_events(false)` could be added if needed.

## Event Propagation

Events **bubble** from the hit target up through ancestors. No capture phase — this matches the simple mental model of the framework and covers all practical use cases.

### Dispatch order

1. Framework runs hit test to find the target element
2. Walk up from target to root, building the dispatch path: `[target, parent, grandparent, ..., root]`
3. Walk the path — for each element, check if it has a handler for this event type
4. **First element with a handler wins**: fire that handler and stop. The event does not continue bubbling.

This means propagation is implicit — no `Handled` enum, no manual control. The rule is simple:

- **Has handler → handles the event (stops)**
- **No handler → transparent, event passes through to parent**

If you put a button (with `.on_click()`) inside a clickable card (also with `.on_click()`), clicking the button fires only the button's handler. The card never sees it. If you click the card's background (no child handler), the card's handler fires.

This covers the common cases without any API surface for propagation control. An element that wants to observe an event without blocking it should use state at the view level instead (e.g. a parent reads `hovered_id` rather than attaching its own click handler on top of child handlers).

## Pointer Events

### Click

```rust
.on_click(move || handle.update(|s| { ... }))
```

Fires on pointer button release if the pointer is still within the element's bounds (standard click behavior — press outside + release inside, or press inside + release outside, do not fire).

Implementation: on `PointerButton { pressed: true }`, record the hit target as `pressed_element`. On `PointerButton { pressed: false }`, if the current hit target is the same element (or a descendant), fire the click.

### Hover

```rust
.on_hover(move |hovered: bool| handle.update(|s| { ... }))
```

Fires with `true` when the pointer enters the element's bounds, `false` when it leaves. Derived from hit testing on every `PointerMove`:

1. Run hit test at new pointer position → `new_hovered` (the target and all its ancestors)
2. Compare with `prev_hovered` set
3. Elements in `prev_hovered` but not `new_hovered` → fire `on_hover(false)`
4. Elements in `new_hovered` but not `prev_hovered` → fire `on_hover(true)`

This means hover is **not exclusive** — a parent and child can both be hovered simultaneously (the parent contains the child). This is correct behavior: hovering over a dock icon also means hovering over the dock bar.

The dock example becomes:

```rust
container()
    .key(&app.id)
    .child(image(&app.icon))
    .on_hover({
        let id = app.id.clone();
        move |hovered| handle.update(|s| {
            s.hovered_id.set(if hovered { Some(id.clone()) } else { None })
        })
    })
    .on_click(move || handle.update(|s| s.launch(&app.id)))
```

### Drag

```rust
.on_drag(move |delta: Point| handle.update(|s| { ... }))
```

Fires on `PointerMove` while the pointer button is held down and the element was the press target. `delta` is the movement since the last event (not since press start). The framework tracks `drag_origin` and `drag_element` internally.

A drag threshold of 3px prevents accidental drags from sloppy clicks — `on_drag` only starts firing after the pointer moves 3px from the press point.

### Scroll

```rust
.on_scroll(move |delta: Point| handle.update(|s| { ... }))
```

Fires on `PointerScroll` when the element (or a descendant) is under the pointer. `delta` is the scroll amount (positive = down/right). Bubbles up if unhandled, so a scrollable list inside a scrollable page works naturally.

### Cursor style

```rust
.cursor(CursorStyle::Pointer)
```

```rust
enum CursorStyle {
    Default,
    Pointer,   // hand — clickable
    Text,      // I-beam — text field
    Grab,      // open hand — draggable
    Grabbing,  // closed hand — dragging
    NotAllowed,
}
```

The framework sets the Wayland cursor shape based on the topmost hovered element's cursor style. `text_input()` defaults to `CursorStyle::Text`.

## Keyboard Input & Focus

### Focus model

One element at a time holds keyboard focus. Focus is a framework-level state, not per-view:

```rust
// Framework internal state
struct FocusState {
    focused: Option<ElementId>,
}
```

An element must be **focusable** to receive keyboard focus:

```rust
.focusable()            // makes any element focusable
```

`text_input()` is implicitly focusable.

### Gaining focus

- **Click**: clicking a focusable element focuses it. Clicking a non-focusable element clears focus.
- **Tab**: pressing Tab moves focus to the next focusable element in tree order. Shift+Tab moves backward.
- **Programmatic**: `cx.focus(element_id)` sets focus explicitly.

### Tab order

Default tab order follows the element tree (depth-first, top-to-bottom, left-to-right). Override with `.tab_index()`:

```rust
.focusable().tab_index(2)   // explicit order
.focusable().tab_index(-1)  // focusable by click, but skipped by Tab
```

Matches the HTML `tabindex` semantics: positive values define explicit order (lower first), `0` means natural order, `-1` means not tab-reachable.

### Focus ring

The framework draws a focus ring around the focused element automatically — a 2px outline with the accent color, offset 2px from the element bounds. This can be styled:

```rust
.focus_ring(FocusRing::None)                    // hide it
.focus_ring(FocusRing::Custom { color, width, offset })
```

The focus ring is only shown when focus was gained via keyboard (Tab). Mouse-click focus does not show the ring (same as `:focus-visible` in CSS).

### Key event handlers

```rust
.on_key_down(move |key: Key, mods: Modifiers| {
    handle.update(|s| { ... });
})
```

Key events route to the focused element first. If the focused element has an `on_key_down` handler, it fires and the event stops. If not, the event bubbles up to the parent, and so on. If nothing handles it, the framework checks for global shortcuts (registered at the view level).

### Text input

`text_input()` internally handles:
- `KeyDown` → insert/delete text, move cursor
- `TextInput` → insert composed text (IME, dead keys)
- Arrow keys → cursor movement
- Ctrl+A/C/V/X → select all, copy, paste, cut (via `wl_data_device` for clipboard)
- Focus → show cursor blink, border highlight
- Blur → hide cursor, commit value

```rust
text_input(&self.query)
    .placeholder("Search apps...")
    .on_change(move |text| handle.update(|s| s.query = text))
    .on_submit(move || handle.update(|s| s.execute_search()))
```

`.on_submit()` fires on Enter key.

## Internal Architecture

### Event dispatch pipeline

Each frame where input is received:

```
Wayland event
  → InputEvent
  → Hit test (pointer events) or focus lookup (keyboard events)
  → Build dispatch path: [target, parent, grandparent, ..., root]
  → Walk path, fire first handler found for this event type, stop
  → If state mutated: mark dirty, schedule re-render
```

### Hover tracking state

The framework maintains a set of currently-hovered element IDs. On every `PointerMove`, the hit test result determines the new hovered set (the target element and all its ancestors up to root). Diffing old vs new triggers `on_hover(true/false)` callbacks.

On `PointerLeave` (cursor exits the Wayland surface entirely), all hovered elements receive `on_hover(false)`.

### Pressed state

On pointer button down, the framework records `pressed_element`. This is used for:
- **Click detection**: button up on the same element → click
- **Drag tracking**: pointer move while pressed → drag events to `pressed_element`
- **Press visual feedback**: elements can check pressed state (future `.on_press_change()` if needed)

On pointer button up, `pressed_element` is cleared.

### Element identity for events

Event handlers are attached to elements during `render()`. Since the element tree is rebuilt each frame, handler identity is tied to the element's position in the tree (and `.key()` if present). The framework matches handlers across renders using tree position + key, same as the animation diffing system.

## Full Example — App Launcher

Shows focus, keyboard navigation, hover, click, scroll, and text input together:

```rust
struct LauncherView {
    query: Reactive<String>,
    results: Reactive<Vec<App>>,
    selected_index: Reactive<usize>,
}

impl View for LauncherView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        let selected = *self.selected_index.get(cx);

        column().gap(8.0).padding(16.0)
            .on_key_down({
                let handle = handle.clone();
                move |key, _mods| match key {
                    Key::ArrowDown => handle.update(|s| {
                        let max = s.results.get().len().saturating_sub(1);
                        s.selected_index.set((selected + 1).min(max));
                    }),
                    Key::ArrowUp => handle.update(|s| {
                        s.selected_index.set(selected.saturating_sub(1));
                    }),
                    Key::Enter => handle.update(|s| s.launch_selected()),
                    _ => {}
                }
            })
            .child(
                text_input(&self.query)
                    .placeholder("Search apps...")
                    .font_size(20.0)
                    .on_change(move |text| handle.update(|s| {
                        s.query.set(text.clone());
                        s.results.set(s.filter_apps(&text));
                        s.selected_index.set(0);
                    }))
            )
            .child(
                column().gap(2.0).fill_width().children(
                    self.results.get(cx).iter().enumerate().map(|(i, app)| {
                        let is_selected = i == selected;
                        container()
                            .key(&app.id)
                            .fill_width()
                            .padding(10.0)
                            .rounded(8.0)
                            .background(if is_selected { rgba(255, 255, 255, 20) } else { TRANSPARENT })
                            .animate(Animation::snappy())
                            .on_hover({
                                let handle = handle.clone();
                                move |hovered| if hovered {
                                    handle.update(|s| s.selected_index.set(i));
                                }
                            })
                            .on_click(move || handle.update(|s| s.launch(&app.id)))
                            .cursor(CursorStyle::Pointer)
                            .child(row().gap(12.0).align_items(Alignment::Center)
                                .child(image(&app.icon).size(32.0, 32.0))
                                .child(column().gap(2.0)
                                    .child(text(&app.name).font_size(14.0).color(WHITE))
                                    .child(text(&app.description).font_size(12.0).color(MUTED))
                                )
                            )
                    })
                )
            )
    }
}
```
