# Plan: Keyboard Events & Element Focus System

## Problem

The UI framework defines `InputEvent::KeyDown`, `InputEvent::KeyUp`, and `InputEvent::TextInput` in `ui/src/input.rs`, but never produces them. `ui/src/wayland.rs` has no `delegate_keyboard!`, no `KeyboardHandler` impl, and `event_dispatch.rs` returns `false` for all keyboard events (line 214: `_ => false`).

A proper focus system is needed for:
- **Text inputs** — clicking a text field focuses it, key presses go there
- **Menu items** — keyboard shortcuts, arrow key navigation
- **Popup dismiss** — surface-level focus loss closes the popup
- **Global shortcuts** — Escape to close popups, handled by the View as a fallback

## Design

### Focus Model

Focus exists at two levels:

1. **Surface focus** — which Wayland surface has compositor keyboard focus. Managed by the compositor. When a popup surface loses this, its `on_focus_lost` fires (see popup plan).

2. **Element focus** — which element *within* a surface receives key events. Managed by `EventState`. Each surface (main + each popup) has its own `EventState` with its own `focused_element`.

Key event routing:
1. Compositor delivers key to the focused surface
2. Framework dispatches to the focused element's `on_key` callback
3. If unhandled (returns `false`) or no focused element, bubble up to parent elements
4. If still unhandled, falls through to `View::on_key_down` as a global catch-all

### Element API additions (`ui/src/element.rs`)

```rust
pub struct Element {
    // ... existing fields ...

    /// Whether this element can receive keyboard focus.
    pub focusable: bool,

    /// Called when this element gains or loses focus.
    pub on_focus: Option<Box<dyn Fn(bool)>>,

    /// Called when a key is pressed while this element is focused.
    /// Return true if handled (stops bubbling).
    pub on_key: Option<Box<dyn Fn(Key, Modifiers) -> bool>>,
}
```

Builder methods:

```rust
impl Element {
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }
    pub fn on_focus(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus = Some(Box::new(f));
        self.focusable = true; // implicitly focusable
        self
    }
    pub fn on_key(mut self, f: impl Fn(Key, Modifiers) -> bool + 'static) -> Self {
        self.on_key = Some(Box::new(f));
        self.focusable = true; // implicitly focusable
        self
    }
}
```

`TextInput` elements are implicitly focusable (set `focusable: true` in the `text_input()` builder).

### View trait addition (`ui/src/app.rs`)

```rust
pub trait View: 'static {
    // ... existing methods ...

    /// Global key handler. Called when a key press is not handled by any
    /// focused element. Return true if handled.
    fn on_key_down(&mut self, key: Key, modifiers: Modifiers) -> bool { false }
}
```

### Focus tracking in EventState (`ui/src/event_dispatch.rs`)

Add to `EventState`:

```rust
pub struct EventState {
    // ... existing fields ...

    /// The pre-order index of the currently focused element, if any.
    focused_element: Option<usize>,
}
```

#### Click-to-focus

When a pointer press lands on a focusable element, focus it. When it lands on a non-focusable element or empty space, clear focus.

In the `PointerButton { pressed: true }` handler:

```rust
let hit = hit_test(layout, root_element, *x, *y);
let clicked_idx = hit.as_ref().and_then(|h| {
    // Walk path from deepest to root, find first focusable
    h.path.iter().find(|&&idx| {
        get_element_by_preorder(root_element, idx)
            .map_or(false, |e| e.focusable)
    }).copied()
});

if clicked_idx != self.focused_element {
    // Fire on_focus(false) for old
    if let Some(old) = self.focused_element {
        if let Some(el) = get_element_by_preorder(root_element, old) {
            if let Some(ref handler) = el.on_focus {
                handler(false);
            }
        }
    }
    // Fire on_focus(true) for new
    if let Some(new) = clicked_idx {
        if let Some(el) = get_element_by_preorder(root_element, new) {
            if let Some(ref handler) = el.on_focus {
                handler(true);
            }
        }
    }
    self.focused_element = clicked_idx;
    needs_redraw = true;
}
```

#### Tab navigation

On `KeyDown` with `Key::TAB`:
- Collect all focusable elements by pre-order index
- Find current focused index in that list
- Move to next (or previous with Shift+Tab)
- Fire `on_focus(false)` / `on_focus(true)` for old/new

#### Key dispatch

On `KeyDown`:

```rust
InputEvent::KeyDown { key, modifiers } => {
    let mut handled = false;

    // 1. Try focused element's on_key
    if let Some(focused_idx) = self.focused_element {
        if let Some(el) = get_element_by_preorder(root_element, focused_idx) {
            if let Some(ref handler) = el.on_key {
                handled = handler(*key, *modifiers);
            }
        }
    }

    // 2. If unhandled, return false — caller (wayland.rs) falls through to View::on_key_down
    handled
}
```

#### Surface focus loss

Add a method to clear element focus when the surface loses compositor focus:

```rust
impl EventState {
    pub fn clear_focus(&mut self, root_element: &Element) {
        if let Some(old) = self.focused_element.take() {
            if let Some(el) = get_element_by_preorder(root_element, old) {
                if let Some(ref handler) = el.on_focus {
                    handler(false);
                }
            }
        }
    }
}
```

Called from `KeyboardHandler::leave` for the matching surface's `EventState`.

### Wayland keyboard binding (`ui/src/wayland.rs`)

Add keyboard support alongside the existing pointer handling:

```rust
use smithay_client_toolkit::{
    delegate_keyboard,
    seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
};
```

Add to `WaylandState`:

```rust
struct WaylandState {
    // ... existing fields ...
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_focus_surface: Option<wl_surface::WlSurface>,
    current_modifiers: crate::input::Modifiers,
}
```

In `SeatHandler::new_capability`, acquire keyboard:

```rust
if capability == Capability::Keyboard && self.keyboard.is_none() {
    self.keyboard = Some(
        self.seat_state.get_keyboard(qh, &seat, None).expect("Failed to get keyboard")
    );
}
```

Add `delegate_keyboard!(WaylandState);`

### KeyboardHandler implementation (`ui/src/wayland.rs`)

```rust
impl KeyboardHandler for WaylandState {
    fn enter(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
             _kb: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface,
             _serial: u32, _raw: &[u32], _keysyms: &[Keysym]) {
        self.keyboard_focus_surface = Some(surface.clone());
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
             _kb: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _serial: u32) {
        self.keyboard_focus_surface = None;

        // Clear element focus on the surface that lost compositor focus
        if Some(surface) == self.layer_surface.as_ref().map(|ls| ls.wl_surface()) {
            if let Some(ref elements) = self.last_element_tree {
                self.event_state.clear_focus(elements);
            }
        }

        // For popups: match surface, clear focus, fire on_focus_lost callback
        // (see popup plan for self.popups handling)
    }

    fn press_key(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
                 _kb: &wl_keyboard::WlKeyboard, _serial: u32, event: KeyEvent) {
        self.pending_events.push(InputEvent::KeyDown {
            key: Key(event.keysym.raw()),
            modifiers: self.current_modifiers,
        });
    }

    fn release_key(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
                   _kb: &wl_keyboard::WlKeyboard, _serial: u32, event: KeyEvent) {
        self.pending_events.push(InputEvent::KeyUp {
            key: Key(event.keysym.raw()),
            modifiers: self.current_modifiers,
        });
    }

    fn update_modifiers(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
                        _kb: &wl_keyboard::WlKeyboard, _serial: u32, modifiers: Modifiers,
                        _layout: u32) {
        self.current_modifiers = crate::input::Modifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            super_: modifiers.logo,
        };
    }
}
```

### Event processing in the main loop (`ui/src/wayland.rs`)

In `process_input_events`, dispatch key events through the focus chain:

```rust
InputEvent::KeyDown { key, modifiers } => {
    // 1. Dispatch to focused element via EventState
    let handled = self.event_state.dispatch(event, layout, elements);

    // 2. If unhandled, fall through to View::on_key_down
    if !handled {
        if (self.view_state.on_key_down_fn)(*key, *modifiers) {
            self.needs_redraw = true;
        }
    } else {
        self.needs_redraw = true;
    }
}
```

### ViewState additions (`ui/src/wayland.rs`)

Add type-erased closure for `View::on_key_down`:

```rust
struct ViewState {
    // ... existing fields ...
    on_key_down_fn: Box<dyn Fn(Key, Modifiers) -> bool>,
}
```

### Key constants (`ui/src/input.rs`)

```rust
impl Key {
    pub const ESCAPE: Key = Key(0xff1b);     // XKB_KEY_Escape
    pub const RETURN: Key = Key(0xff0d);     // XKB_KEY_Return
    pub const BACKSPACE: Key = Key(0xff08);  // XKB_KEY_BackSpace
    pub const TAB: Key = Key(0xff09);        // XKB_KEY_Tab
    pub const LEFT: Key = Key(0xff51);       // XKB_KEY_Left
    pub const RIGHT: Key = Key(0xff53);      // XKB_KEY_Right
    pub const UP: Key = Key(0xff52);         // XKB_KEY_Up
    pub const DOWN: Key = Key(0xff54);       // XKB_KEY_Down
}
```

### Hit testing update (`ui/src/hit_test.rs`)

Elements with `focusable: true` or `on_key.is_some()` should be considered interactive for hit testing:

```rust
fn is_interactive(element: &Element) -> bool {
    // ... existing checks ...
    || element.focusable
    || element.on_key.is_some()
}
```

## Usage Examples

### Header — Escape to close popups (View-level)

```rust
impl View for HeaderView {
    fn on_key_down(&mut self, key: Key, _mods: Modifiers) -> bool {
        if key == Key::ESCAPE {
            self.volume_popup_open = false;
            self.menu_open = false;
            true
        } else { false }
    }
}
```

### Text input — focused typing (element-level)

```rust
text_input(&self.search_query)
    .on_key({
        let handle = cx.handle::<Self>();
        move |key, mods| {
            if key == Key::RETURN {
                handle.update(|v| v.submit_search());
                true
            } else { false } // let framework handle normal text input
        }
    })
    .on_change({
        let handle = cx.handle::<Self>();
        move |val| handle.update(|v| v.search_query = val)
    })
```

### Menu item — keyboard shortcut (element-level)

```rust
row()
    .focusable()
    .on_key({
        let handle = cx.handle::<Self>();
        move |key, _| {
            if key == Key::RETURN {
                handle.update(|v| v.activate_menu_item(idx));
                true
            } else { false }
        }
    })
    .on_click({
        let handle = cx.handle::<Self>();
        move || handle.update(|v| v.activate_menu_item(idx))
    })
    .child(text("Log Out"))
```

## File Changes

| File                       | Changes                                                                                                                   |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `ui/src/element.rs`        | Add `focusable`, `on_focus`, `on_key` fields + builder methods. `text_input()` sets `focusable: true`                     |
| `ui/src/input.rs`          | Add `Key` constants                                                                                                       |
| `ui/src/app.rs`            | Add `on_key_down()` to `View` trait                                                                                       |
| `ui/src/event_dispatch.rs` | Add `focused_element` to `EventState`, click-to-focus, tab navigation, key dispatch with bubbling, `clear_focus()` method |
| `ui/src/hit_test.rs`       | Add `focusable` / `on_key` to interactivity check                                                                         |
| `ui/src/wayland.rs`        | Add `delegate_keyboard!`, `KeyboardHandler` impl, `keyboard` field, `current_modifiers`, `keyboard_focus_surface`, `ViewState` key closure, key event processing with focus-then-view fallthrough |

## Integration with Popup Plan

Each popup has its own `EventState` (already in the popup plan). This means each popup surface tracks its own `focused_element` independently.

When `KeyboardHandler::leave` fires for a popup surface:
1. Call `popup.event_state.clear_focus(elements)` — clears element focus, fires `on_focus(false)`
2. Fire `popup.on_focus_lost` callback — View sets flag to close the popup
3. Next render: View calls `cx.close_popup(key)`, framework destroys the surface

See `ui-popup-surfaces.md` for the popup-side implementation.
