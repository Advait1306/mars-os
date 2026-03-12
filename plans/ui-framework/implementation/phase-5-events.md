# Phase 5: Event & Input System

## Goal

Route Wayland input events (pointer, keyboard) through the element tree to handler closures. By the end, elements respond to clicks, hovers, drags, and keyboard input.

## Steps

### 5.1 Event handler storage on Element

Add optional handler fields to `Element`:

```rust
// In element.rs
pub struct Element {
    // ... existing fields ...
    pub on_click: Option<Box<dyn Fn()>>,
    pub on_hover: Option<Box<dyn Fn(bool)>>,
    pub on_drag: Option<Box<dyn Fn(Point)>>,
    pub on_scroll: Option<Box<dyn Fn(Point)>>,
    pub on_key_down: Option<Box<dyn Fn(Key, Modifiers)>>,
    pub cursor: Option<CursorStyle>,
    pub focusable: bool,
    pub tab_index: Option<i32>,
}
```

Chainable builder methods:

```rust
impl Element {
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self { ... }
    pub fn on_hover(mut self, f: impl Fn(bool) + 'static) -> Self { ... }
    pub fn on_drag(mut self, f: impl Fn(Point) + 'static) -> Self { ... }
    pub fn on_scroll(mut self, f: impl Fn(Point) + 'static) -> Self { ... }
    pub fn on_key_down(mut self, f: impl Fn(Key, Modifiers) + 'static) -> Self { ... }
    pub fn cursor(mut self, style: CursorStyle) -> Self { ... }
    pub fn focusable(mut self) -> Self { ... }
    pub fn tab_index(mut self, index: i32) -> Self { ... }
}
```

### 5.2 Input event types

`src/input.rs` — framework-level input events converted from Wayland raw events.

```rust
pub enum InputEvent {
    PointerMove { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, button: MouseButton, pressed: bool },
    PointerScroll { x: f32, y: f32, delta_x: f32, delta_y: f32 },
    PointerLeave,
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    TextInput { text: String },
}

pub enum MouseButton { Left, Right, Middle }

pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}

pub enum CursorStyle {
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    NotAllowed,
}
```

`Key` wraps `xkbcommon` keysyms.

### 5.3 Wayland input handling

Update SCTK seat handling to convert raw Wayland events to `InputEvent`:

- `wl_pointer` → `PointerMove`, `PointerButton`, `PointerScroll`, `PointerLeave`
- `wl_keyboard` → `KeyDown`, `KeyUp`, `TextInput` (via `xkb_state`)

Bind `wl_pointer` and `wl_keyboard` when seat capabilities are advertised. Use `xkbcommon` for keymap handling.

New dependency:

```bash
cargo add xkbcommon@0.8
```

### 5.4 Hit testing

`src/hit_test.rs` — find the target element for a pointer position.

```rust
pub fn hit_test(node: &LayoutNode, point: Point) -> Option<&LayoutNode> {
    // If clips, reject points outside bounds
    // Check children in reverse order (front-to-back)
    // Check self: bounds.contains(point) && node.is_interactive()
    // Rounded corner check for elements with corner_radius
}
```

An element is **interactive** if it has any event handler attached.

### 5.5 Event dispatch

`src/event_dispatch.rs` — routes events through the tree.

**Pointer events:**

1. Hit test at pointer position → target element
2. Build dispatch path: `[target, parent, grandparent, ..., root]`
3. Walk path — first element with a handler for this event type fires, then stop

**Hover tracking:**

- Maintain `hovered_set: HashSet<ElementId>` (target + ancestors)
- On `PointerMove`: diff new vs old hovered set → fire `on_hover(true/false)`
- On `PointerLeave`: fire `on_hover(false)` on all

**Click detection:**

- On `PointerButton { pressed: true }`: record `pressed_element`
- On `PointerButton { pressed: false }`: if current hit target == pressed_element (or descendant) → fire `on_click`

**Drag:**

- On `PointerMove` while pressed: if distance from press point > 3px threshold → fire `on_drag(delta)` on `pressed_element`

**Keyboard events:**

- Route to focused element first
- If no handler, bubble up
- If nothing handles it, check view-level handlers

### 5.6 Focus management

`src/focus.rs`:

```rust
struct FocusState {
    focused: Option<ElementId>,
    focus_via_keyboard: bool,  // for focus-visible ring
}
```

- Click a focusable element → focus it (no focus ring)
- Tab/Shift+Tab → move focus in tree order (show focus ring)
- Click non-focusable → clear focus

### 5.7 Cursor management

After hit test, find the topmost hovered element with a `cursor` style. Set the Wayland cursor shape via `wl_pointer.set_cursor()` or `wp_cursor_shape_v1`.

### 5.8 Validation

Test app:

```rust
struct ClickView {
    label: Reactive<String>,
}

impl View for ClickView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        container()
            .background(rgba(40, 40, 40, 255))
            .rounded(12.0)
            .padding(20.0)
            .child(text(self.label.get(cx)).font_size(18.0).color(WHITE))
            .on_click(move || handle.update(|s| s.label.set("Clicked!".into())))
            .on_hover({
                let handle = handle.clone();
                move |hovered| handle.update(|s| {
                    if hovered { s.label.set("Hovering".into()) }
                    else { s.label.set("Click me".into()) }
                })
            })
            .cursor(CursorStyle::Pointer)
    }
}
```

Verify hover/click change the label, cursor changes to hand pointer on hover.

## Output

The framework handles all pointer and keyboard input. Elements respond to user interaction through handler closures that mutate `Reactive` state, triggering re-renders.
