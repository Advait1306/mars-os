# Event System Implementation Plan

## Overview

This document specifies the complete event system for the UI framework. The framework currently has basic pointer events (move, button, scroll), hover tracking, click detection with drag threshold, basic drag support, keyboard events (KeyDown, KeyUp, TextInput), and cursor style management. It lacks: event propagation phases (capture/bubble), stopPropagation/preventDefault, focus management, tab navigation, pointer capture, double-click/right-click/context menu, touch events, drag-and-drop (data transfer), clipboard events, IME composition events, and gesture recognition.

The target is a Wayland-native desktop toolkit rendered with Skia. All events originate from Wayland protocol messages (wl_pointer, wl_keyboard, wl_touch, zwp_text_input_v3, wl_data_device) and are translated into framework-level event objects that propagate through the element tree.

### Architecture Summary

```
Wayland Protocol Messages
        |
        v
  Event Translation Layer (wayland.rs)
    - Converts wl_pointer/wl_keyboard/wl_touch/etc into framework InputEvents
    - Handles protocol-level state (keyboard repeat, axis frames, touch frames)
        |
        v
  Event Dispatch Engine (event_dispatch.rs)
    - Hit testing to find target element
    - Builds event path (root -> ... -> target)
    - Runs capture phase (root to target)
    - Runs target phase
    - Runs bubble phase (target to root)
    - Manages focus state, pointer capture, hover tracking
        |
        v
  Element Event Handlers
    - on_pointer_down, on_pointer_up, on_pointer_move, etc.
    - Return EventResult (handled/propagate/prevent_default)
```

---

## Event Propagation Model

### Current State

The existing system has no propagation phases. Events are dispatched directly to the deepest hit element, with scroll being the only event that "bubbles" (iterates the hit path until a handler is found). There is no capture phase, no stopPropagation, no preventDefault.

### Target: Three-Phase Dispatch

Following the W3C DOM Events model, every event dispatched to an element traverses three phases:

1. **Capture phase**: The event travels from the root element down through each ancestor to the target's parent. Handlers registered for the capture phase execute in root-to-target order. This allows ancestors to intercept events before they reach descendants.

2. **Target phase**: The event arrives at the target element itself. Both capture and bubble handlers on the target fire (in registration order).

3. **Bubble phase**: The event travels back up from the target's parent to the root. Handlers registered for the bubble phase execute in target-to-root order. This is the default phase for most handlers.

Not all events bubble. Events like PointerEnter, PointerLeave, Focus, and Blur do NOT bubble (they fire only on the target). Their bubbling counterparts (PointerOver/PointerOut, FocusIn/FocusOut) exist for when ancestors need to observe these state changes.

### Event Path

The event path is the ordered list of elements from root to target. It is computed once per dispatch and stored on the event context. For a tree like:

```
Root -> Panel -> Button -> Label
```

If the target is Label, the event path is `[Root, Panel, Button, Label]`.

The path is derived from the hit test result, which already computes pre-order indices from deepest to root. We reverse this to get root-to-target order for capture phase traversal.

### Propagation Control

#### `stop_propagation()`

Prevents the event from continuing to the next element in the current phase and all subsequent phases. Handlers already registered on the CURRENT element still execute.

#### `stop_immediate_propagation()`

Prevents the event from continuing AND prevents any remaining handlers on the current element from executing. This is relevant when multiple handlers are registered on the same element.

In our system, elements have at most one handler per event type, so `stop_immediate_propagation` is equivalent to `stop_propagation` unless we later support multiple handlers per element per event type.

#### `prevent_default()`

Signals that the default action for this event should NOT occur. Only meaningful for cancelable events. Examples:
- Calling `prevent_default()` on a PointerDown prevents the element from gaining focus
- Calling `prevent_default()` on a KeyDown for Tab prevents focus from moving
- Calling `prevent_default()` on a BeforeInput prevents the text insertion
- Calling `prevent_default()` on a Wheel event prevents scrolling

### Rust API for Propagation

```rust
/// Result returned by event handlers to control propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// Event was not handled; continue propagation.
    Continue,
    /// Event was handled; stop propagation (equivalent to stopPropagation).
    Stop,
    /// Event was handled AND default action should be prevented.
    StopAndPreventDefault,
    /// Continue propagation but prevent the default action.
    PreventDefault,
}

/// Mutable context passed to event handlers during dispatch.
pub struct EventContext<'a> {
    /// Whether propagation has been stopped.
    propagation_stopped: bool,
    /// Whether the default action has been prevented.
    default_prevented: bool,
    /// The event path (root to target).
    path: &'a [ElementId],
    /// Current phase.
    phase: EventPhase,
    /// The target element.
    target: ElementId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}
```

### Handler Registration

Elements register handlers with an optional phase parameter. The default is bubble phase.

```rust
impl Element {
    /// Register a handler for the bubble phase (default).
    pub fn on_pointer_down(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.handlers.pointer_down = Some(Box::new(f));
        self
    }

    /// Register a handler for the capture phase.
    pub fn on_pointer_down_capture(
        mut self,
        f: impl Fn(&PointerEvent) -> EventResult + 'static,
    ) -> Self {
        self.handlers.pointer_down_capture = Some(Box::new(f));
        self
    }
}
```

### Dispatch Algorithm (Pseudocode)

```
fn dispatch_event(event, path, root_element):
    let target = path.last()
    let mut ctx = EventContext::new(path, target)

    // 1. Capture phase (root -> target's parent)
    ctx.phase = Capture
    for element in path[0..path.len()-1]:
        if ctx.propagation_stopped: break
        if let Some(handler) = element.capture_handler_for(event.type):
            let result = handler(event, &ctx)
            apply_result(&mut ctx, result)

    // 2. Target phase
    ctx.phase = Target
    if !ctx.propagation_stopped:
        // Fire capture handler on target (if any)
        if let Some(handler) = target.capture_handler_for(event.type):
            let result = handler(event, &ctx)
            apply_result(&mut ctx, result)
        // Fire bubble handler on target (if any)
        if !ctx.propagation_stopped:
            if let Some(handler) = target.bubble_handler_for(event.type):
                let result = handler(event, &ctx)
                apply_result(&mut ctx, result)

    // 3. Bubble phase (target's parent -> root)
    if event.bubbles:
        ctx.phase = Bubble
        for element in path[0..path.len()-1].reverse():
            if ctx.propagation_stopped: break
            if let Some(handler) = element.bubble_handler_for(event.type):
                let result = handler(event, &ctx)
                apply_result(&mut ctx, result)

    // 4. Execute default action (unless prevented)
    if !ctx.default_prevented:
        execute_default_action(event, target)
```

---

## Pointer Events

### Event Types

| Event | Fires When | Bubbles | Cancelable | Default Action |
|-------|-----------|---------|------------|----------------|
| PointerDown | Pointer button pressed or touch contact begins | Yes | Yes | Set focus to target element |
| PointerUp | Pointer button released or touch contact ends | Yes | Yes | None |
| PointerMove | Pointer changes position | Yes | Yes | None |
| PointerEnter | Pointer enters element's bounds (NOT from child) | **No** | No | None |
| PointerLeave | Pointer leaves element's bounds (NOT to child) | **No** | No | None |
| PointerOver | Pointer enters element's bounds (including from child) | Yes | Yes | None |
| PointerOut | Pointer leaves element's bounds (including to child) | Yes | Yes | None |
| PointerCancel | System cancels pointer interaction | Yes | No | None |

#### Enter/Leave vs Over/Out

This is a critical distinction from the DOM model:

- **PointerEnter/PointerLeave** do NOT bubble and do NOT fire when moving between a parent and its children. If the pointer moves from Parent to Child, Parent does NOT get PointerLeave. These track whether the pointer is "within the element's subtree."

- **PointerOver/PointerOut** DO bubble and DO fire when moving between parent and child. If the pointer moves from Parent to Child, Parent gets PointerOut (because the pointer is now directly over the child, not the parent). These track which element the pointer is "directly over."

In practice, Enter/Leave is what most UI elements want (hover state). Over/Out is useful for more specialized patterns like dropdown menus.

For the initial implementation, we keep the current `on_hover(bool)` pattern which maps to Enter/Leave semantics. Over/Out can be added later if needed.

### PointerEvent Data

```rust
pub struct PointerEvent {
    /// Unique pointer ID (distinguishes multiple pointers for multi-touch).
    pub pointer_id: u32,
    /// Pointer type: mouse, pen, touch.
    pub pointer_type: PointerType,
    /// Position in surface-local coordinates.
    pub x: f32,
    pub y: f32,
    /// Which button changed state (for PointerDown/PointerUp).
    pub button: MouseButton,
    /// Bitmask of currently pressed buttons.
    pub buttons: u32,
    /// Timestamp in milliseconds from Wayland.
    pub time: u32,
    /// Whether this is the primary pointer of its type.
    pub is_primary: bool,
    /// Modifier keys held during this event.
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    None,
    Left,      // button 272 / BTN_LEFT
    Right,     // button 273 / BTN_RIGHT
    Middle,    // button 274 / BTN_MIDDLE
    Back,      // button 275 / BTN_SIDE
    Forward,   // button 276 / BTN_EXTRA
}
```

### Button Bitmask

| Bit | Button |
|-----|--------|
| 0x01 | Left |
| 0x02 | Right |
| 0x04 | Middle |
| 0x08 | Back |
| 0x10 | Forward |

### High-Level Pointer Events (Synthesized)

These are synthesized from the raw pointer event sequence:

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| Click | PointerDown then PointerUp on same element, no drag, left button | Yes | Yes |
| DoubleClick | Two clicks within 400ms and 5px distance | Yes | Yes |
| AuxClick | Click with non-primary button (middle, back, forward) | Yes | Yes |
| ContextMenu | Right-click release (or long-press on touch) | Yes | Yes |

#### Click Detection Algorithm

```
State:
  pressed_element: Option<ElementId>
  press_position: (f32, f32)
  press_time: u32
  press_button: MouseButton
  dragging: bool
  last_click_time: u32
  last_click_position: (f32, f32)
  click_count: u32

On PointerDown:
  pressed_element = hit_test(x, y)
  press_position = (x, y)
  press_time = event.time
  press_button = event.button
  dragging = false

On PointerMove:
  if pressed_element.is_some() && !dragging:
    if distance(current, press_position) > DRAG_THRESHOLD (3px):
      dragging = true
      fire DragStart on pressed_element

On PointerUp:
  if !dragging && pressed_element == hit_test(x, y):
    if event.button == Left:
      // Check for double-click
      if event.time - last_click_time < 400
         && distance(current, last_click_position) < 5:
        click_count += 1
      else:
        click_count = 1

      last_click_time = event.time
      last_click_position = (x, y)

      fire Click { count: click_count }
      if click_count == 2:
        fire DoubleClick
    else if event.button == Right:
      fire ContextMenu
    else:
      fire AuxClick { button: event.button }
```

### Pointer Capture

Pointer capture redirects all pointer events for a specific pointer to a designated element, regardless of where the pointer actually is. This is essential for:
- Drag operations (element keeps receiving moves after pointer leaves it)
- Sliders, scrollbars, resize handles
- Any interaction that starts on an element and must track the pointer globally

#### API

```rust
impl Element {
    /// Capture all pointer events for the given pointer ID to this element.
    /// Call from within a PointerDown handler.
    pub fn set_pointer_capture(&self, pointer_id: u32);

    /// Release pointer capture for the given pointer ID.
    pub fn release_pointer_capture(&self, pointer_id: u32);

    /// Check if this element has capture for the given pointer.
    pub fn has_pointer_capture(&self, pointer_id: u32) -> bool;
}
```

#### Capture Behavior

When pointer capture is active for a pointer:
1. All PointerMove, PointerUp, PointerCancel events for that pointer are retargeted to the capturing element
2. PointerOver, PointerOut, PointerEnter, PointerLeave fire as if the pointer is always over the capturing element
3. Hit testing is bypassed for that pointer -- the capturing element is always the target
4. A `GotPointerCapture` event fires on the capturing element
5. When capture is released (explicitly or on PointerUp/PointerCancel), a `LostPointerCapture` event fires

#### Implicit Capture for Touch

When a touch pointer fires PointerDown, the framework automatically sets pointer capture on the target element. This matches browser behavior and ensures touch interactions don't lose their target. The element can call `release_pointer_capture()` to opt out.

#### Implementation in EventState

```rust
pub struct EventState {
    // ... existing fields ...

    /// Map from pointer_id -> capturing element ID.
    pointer_captures: HashMap<u32, ElementId>,
}

impl EventState {
    fn resolve_target(&self, pointer_id: u32, hit_element: ElementId) -> ElementId {
        self.pointer_captures
            .get(&pointer_id)
            .copied()
            .unwrap_or(hit_element)
    }
}
```

### Event Coalescing

Wayland can deliver many PointerMove events per frame. The framework should coalesce multiple PointerMove events for the same pointer between frames, keeping only the latest position but accumulating all intermediate positions for applications that need them (e.g., drawing apps).

```rust
pub struct PointerEvent {
    // ... other fields ...

    /// Coalesced events that were merged into this one.
    /// Only populated for PointerMove events.
    pub coalesced_events: Vec<PointerEvent>,
}
```

Strategy:
- Between `wl_pointer::frame` events, collect all motion events
- On frame, deliver a single PointerMove with the final position
- Store intermediate positions in `coalesced_events` for apps that call `get_coalesced_events()`

### Wayland Pointer Protocol Mapping

| Wayland Event | Framework Event(s) |
|--------------|-------------------|
| `wl_pointer::enter(serial, surface, x, y)` | PointerMove (to seed position); internal: surface has pointer |
| `wl_pointer::leave(serial, surface)` | PointerLeave on all hovered elements; PointerCancel if button was pressed |
| `wl_pointer::motion(time, x, y)` | PointerMove -> triggers Enter/Leave/Over/Out as needed |
| `wl_pointer::button(serial, time, button, state)` | PointerDown (state=pressed) or PointerUp (state=released) |
| `wl_pointer::axis(time, axis, value)` | Accumulated into scroll event (see Scroll section) |
| `wl_pointer::frame` | Flush all accumulated pointer state as one atomic update |
| `wl_pointer::axis_source(source)` | Stored; used to determine scroll event properties |
| `wl_pointer::axis_stop(time, axis)` | ScrollEnd event |
| `wl_pointer::axis_discrete(axis, discrete)` | Stored; used for discrete scroll steps |
| `wl_pointer::axis_value120(axis, value120)` | High-resolution scroll (1 notch = 120 units) |
| `wl_pointer::axis_relative_direction(axis, dir)` | Stored; natural vs inverted scrolling detection |

---

## Keyboard Events

### Event Types

| Event | Fires When | Bubbles | Cancelable | Default Action |
|-------|-----------|---------|------------|----------------|
| KeyDown | Key is pressed or auto-repeats | Yes | Yes | Text input, focus navigation, activation |
| KeyUp | Key is released | Yes | Yes | None |

### KeyboardEvent Data

```rust
pub struct KeyboardEvent {
    /// Logical key value based on current keyboard layout.
    /// Examples: "a", "A", "Enter", "Tab", "Escape", "ArrowUp", "F1"
    pub key: KeyValue,
    /// Physical key code, layout-independent.
    /// Examples: "KeyA", "Enter", "Tab", "ArrowUp"
    pub code: KeyCode,
    /// Whether this event is from key auto-repeat.
    pub repeat: bool,
    /// Modifier key state.
    pub modifiers: Modifiers,
    /// Whether this event fires during IME composition.
    pub is_composing: bool,
    /// Wayland serial for this key event.
    pub serial: u32,
    /// Timestamp in milliseconds.
    pub time: u32,
}

/// Logical key value (layout-dependent).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyValue {
    /// Printable character.
    Character(String),
    /// Named keys.
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Insert,
    /// Function keys.
    F(u8),  // F1-F24
    /// Modifier keys (fired as KeyDown/KeyUp but also tracked in Modifiers).
    Control,
    Shift,
    Alt,
    Super,
    CapsLock,
    NumLock,
    /// Media keys.
    MediaPlay,
    MediaPause,
    MediaStop,
    MediaNext,
    MediaPrev,
    AudioVolumeUp,
    AudioVolumeDown,
    AudioVolumeMute,
    /// Unknown/unrecognized key with raw keysym.
    Unknown(u32),
}

/// Physical key code (layout-independent).
/// Based on USB HID usage codes, matching W3C KeyboardEvent.code values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u32);

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    /// Returns true if the platform "command" modifier is held.
    /// On Linux/Wayland this is Ctrl. (On macOS it would be Super.)
    pub fn command(&self) -> bool {
        self.ctrl
    }
}
```

### Key vs Code

This distinction is critical for correct keyboard handling:

- **`key` (KeyValue)**: The logical key, affected by the current keyboard layout and modifier state. On a QWERTY keyboard, pressing the Y position gives "y". On a QWERTZ layout, same position gives "z". Use `key` for: text shortcuts (Ctrl+C means "copy"), command handling, UI actions.

- **`code` (KeyCode)**: The physical key position, independent of layout. The physical Y position always reports the same code regardless of layout. Use `code` for: game input, positional shortcuts (WASD), anything where the physical position matters.

### Auto-Repeat

When a key is held, Wayland sends repeat events according to `wl_keyboard::repeat_info(rate, delay)`. The compositor provides the rate (keys/sec) and delay (ms before repeat starts). The framework must implement client-side key repeat:

```rust
pub struct KeyRepeatState {
    /// Key currently repeating (if any).
    active_key: Option<(KeyValue, KeyCode, u32)>,  // key, code, keysym
    /// Time until first repeat (from repeat_info delay).
    delay_ms: u32,
    /// Interval between repeats (from repeat_info rate: 1000/rate ms).
    interval_ms: u32,
    /// Time accumulator.
    elapsed_ms: f32,
    /// Whether we've passed the initial delay.
    repeating: bool,
}
```

The repeat timer fires in the event loop. Each repeat generates a `KeyDown { repeat: true, .. }` event dispatched through the normal propagation path to the focused element.

### Keyboard Shortcuts

The framework should support a shortcut/keybinding system layered on top of keyboard events:

```rust
pub struct KeyBinding {
    pub key: KeyValue,
    pub modifiers: Modifiers,
    pub action: Box<dyn Fn()>,
}
```

Shortcut matching happens during the capture phase at the root level. If a shortcut matches and its action executes, the event is consumed (stopPropagation + preventDefault).

### Default Actions for Keyboard Events

| Key | Default Action |
|-----|---------------|
| Tab | Move focus to next focusable element |
| Shift+Tab | Move focus to previous focusable element |
| Enter | Activate focused element (fire Click) |
| Space | Activate focused element (fire Click), toggle checkboxes |
| Escape | Close popups, cancel drag, blur focused element |
| ArrowUp/Down | Navigate within lists, adjust sliders |
| ArrowLeft/Right | Move cursor in text input |
| Home/End | Move cursor to start/end of text |
| Ctrl+A | Select all (in text input) |
| Ctrl+C | Copy selection |
| Ctrl+X | Cut selection |
| Ctrl+V | Paste |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |

### Wayland Keyboard Protocol Mapping

| Wayland Event | Framework Action |
|--------------|-----------------|
| `wl_keyboard::keymap(format, fd, size)` | Load XKB keymap from file descriptor; initialize xkb_state for key-to-symbol translation |
| `wl_keyboard::enter(serial, surface, keys)` | Surface gained keyboard focus; fire KeyDown for any already-held keys in `keys` array |
| `wl_keyboard::leave(serial, surface)` | Surface lost keyboard focus; cancel any active repeat; fire KeyUp for held keys |
| `wl_keyboard::key(serial, time, key, state)` | Translate scancode through xkb_state to get keysym and UTF-8 text; fire KeyDown (state=pressed) or KeyUp (state=released); start/stop repeat timer |
| `wl_keyboard::modifiers(serial, mods_depressed, mods_latched, mods_locked, group)` | Update xkb_state modifier state; update Modifiers struct |
| `wl_keyboard::repeat_info(rate, delay)` | Store repeat parameters; rate=0 means compositor handles repeat |

#### XKB Integration

The framework uses `xkbcommon` to translate Wayland key events:

1. Receive keymap via `wl_keyboard::keymap` (XKB v1 format, shared via fd)
2. Create `xkb_keymap` and `xkb_state` from the keymap data
3. On each `wl_keyboard::key`, use `xkb_state_key_get_one_sym()` to get the keysym
4. Use `xkb_state_key_get_utf8()` to get the UTF-8 text representation
5. On `wl_keyboard::modifiers`, call `xkb_state_update_mask()` to sync modifier state
6. Map keysym to `KeyValue` enum variant
7. Map raw scancode + 8 to `KeyCode` (X11 keycode = evdev scancode + 8)

---

## Focus System

### Current State

There is no focus management. Keyboard events have no target element. Text input elements have no concept of being "focused."

### Focus Model

Every element has an implicit focusability state:

| Element Type | Focusable by Default | Tabbable by Default |
|-------------|---------------------|-------------------|
| TextInput | Yes | Yes |
| Button (has on_click) | Yes | Yes |
| Container | No | No |
| Text | No | No |
| Image | No | No |

**Focusable**: Can receive focus programmatically or by clicking.
**Tabbable**: Included in the Tab key navigation order.

Elements can override these defaults:

```rust
impl Element {
    /// Make this element focusable (can receive keyboard events).
    /// tab_index controls tab order:
    ///   None = not tabbable (but still focusable by click/programmatic)
    ///   Some(0) = tabbable in document order
    ///   Some(n > 0) = tabbable with explicit order (lower numbers first)
    pub fn focusable(mut self, tab_index: Option<i32>) -> Self {
        self.focusable = true;
        self.tab_index = tab_index;
        self
    }

    /// Make this element not focusable.
    pub fn not_focusable(mut self) -> Self {
        self.focusable = false;
        self.tab_index = None;
        self
    }
}
```

### Focus Events

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| Focus | Element gains focus | **No** | No |
| Blur | Element loses focus | **No** | No |
| FocusIn | Element gains focus (bubbling version) | Yes | No |
| FocusOut | Element loses focus (bubbling version) | Yes | No |

#### FocusEvent Data

```rust
pub struct FocusEvent {
    /// The element that is the "other side" of the focus change.
    /// For Focus/FocusIn: the element that lost focus (if any).
    /// For Blur/FocusOut: the element that gained focus (if any).
    pub related_target: Option<ElementId>,
}
```

#### Focus Event Order

When focus moves from element A to element B:

1. `FocusOut` fires on A (related_target = B) -- bubbles
2. `Blur` fires on A (related_target = B) -- does NOT bubble
3. `FocusIn` fires on B (related_target = A) -- bubbles
4. `Focus` fires on B (related_target = A) -- does NOT bubble

### Focus State in EventState

```rust
pub struct EventState {
    // ... existing fields ...

    /// Currently focused element.
    focused: Option<ElementId>,
    /// Ordered list of tabbable elements, rebuilt each render.
    tab_order: Vec<ElementId>,
}
```

### Tab Navigation

On each render, the framework builds a tab order list by walking the element tree in pre-order and collecting all tabbable elements:

1. First, elements with `tab_index > 0`, sorted by tab_index value (ascending), then by document order for ties
2. Then, elements with `tab_index == 0` (or default tabbable), in document order

Tab key moves focus forward through this list. Shift+Tab moves backward. The list wraps around.

#### Focus Trapping

Modals and dialogs need to trap focus so Tab never escapes:

```rust
impl Element {
    /// Trap focus within this element's subtree.
    /// Tab/Shift+Tab will cycle only among focusable descendants.
    pub fn focus_trap(mut self) -> Self {
        self.focus_trap = true;
        self
    }
}
```

When a focus trap is active:
- Tab order is restricted to focusable elements within the trap
- Attempting to tab past the last element wraps to the first element in the trap
- Attempting to shift-tab past the first wraps to the last
- Focus CANNOT move outside the trap via Tab (but CAN be programmatically moved)

### Programmatic Focus

```rust
/// Move focus to a specific element by ID.
fn focus(element_id: ElementId);

/// Remove focus from the currently focused element.
fn blur();
```

These are exposed through a context object passed to event handlers or available on the reactive Handle.

### Focus Ring Rendering

When an element has focus, a focus ring is rendered around it. The focus ring:
- Is only visible when focus was gained via keyboard (Tab), not mouse click
- Uses a 2px outline, 2px offset, with the system accent color
- Can be customized per element via `focus_ring_style`

```rust
pub struct FocusRingStyle {
    pub color: Color,
    pub width: f32,
    pub offset: f32,
    pub radius: Option<f32>,  // None = inherit element's corner_radius
}

impl Element {
    /// Customize the focus ring appearance.
    pub fn focus_ring(mut self, style: FocusRingStyle) -> Self {
        self.focus_ring_style = Some(style);
        self
    }

    /// Hide the focus ring (element still receives focus).
    pub fn hide_focus_ring(mut self) -> Self {
        self.show_focus_ring = false;
        self
    }
}
```

#### Keyboard vs Pointer Focus

Track whether the last focus change was caused by keyboard or pointer:

```rust
pub struct EventState {
    // ...
    focus_visible: bool,  // true if focus was set via keyboard
}
```

- On Tab/Shift+Tab: `focus_visible = true`
- On PointerDown that changes focus: `focus_visible = false`
- Focus ring renders only when `focus_visible` is true

This matches the CSS `:focus-visible` behavior.

---

## Input and Composition Events

### Input Events

These events fire on the focused text input element when its content changes.

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| BeforeInput | Before text content is modified | Yes | Yes |
| Input | After text content is modified | Yes | No |

#### InputEvent Data

```rust
pub struct TextInputEvent {
    /// The text being inserted (empty for deletions).
    pub data: Option<String>,
    /// The type of input operation.
    pub input_type: InputType,
    /// Whether this event fires during IME composition.
    pub is_composing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    InsertText,
    InsertLineBreak,
    InsertFromPaste,
    InsertFromDrop,
    DeleteContentBackward,  // Backspace
    DeleteContentForward,   // Delete key
    DeleteWordBackward,     // Ctrl+Backspace
    DeleteWordForward,      // Ctrl+Delete
    DeleteSoftLineBackward, // Cmd+Backspace
    DeleteSoftLineForward,  // Cmd+Delete
    DeleteByCut,
    HistoryUndo,
    HistoryRedo,
    FormatBold,
    FormatItalic,
    FormatUnderline,
}
```

#### Event Sequence for Text Input

When the user types "a" into a text input:

1. `KeyDown { key: Character("a"), .. }` fires on focused element
2. If not prevented: `BeforeInput { data: "a", input_type: InsertText }` fires
3. If not prevented: text content is modified
4. `Input { data: "a", input_type: InsertText }` fires
5. `on_change(new_value)` callback fires on the text input element

When the user presses Backspace:

1. `KeyDown { key: Backspace, .. }` fires
2. `BeforeInput { data: None, input_type: DeleteContentBackward }` fires
3. Text content is modified
4. `Input { data: None, input_type: DeleteContentBackward }` fires
5. `on_change(new_value)` fires

### Composition Events (IME)

Input Method Editors are used for CJK languages, emoji input, and other complex text input. The Wayland `zwp_text_input_v3` protocol provides IME support.

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| CompositionStart | IME composition session begins | Yes | Yes |
| CompositionUpdate | IME preedit text changes | Yes | No |
| CompositionEnd | IME composition session ends (text committed or cancelled) | Yes | No |

#### CompositionEvent Data

```rust
pub struct CompositionEvent {
    /// The current composition text.
    /// Empty for CompositionStart, committed text for CompositionEnd.
    pub data: String,
    /// Cursor position within the preedit string (byte offset).
    pub cursor_begin: Option<usize>,
    pub cursor_end: Option<usize>,
}
```

#### Event Sequence for IME Input

Example: typing a Chinese character using Pinyin input:

1. User presses "n": `CompositionStart { data: "" }`
2. Preedit appears: `CompositionUpdate { data: "n", .. }`
3. User presses "i": `CompositionUpdate { data: "ni", .. }`
4. User selects candidate "你": `CompositionEnd { data: "你" }`
5. Text "你" is inserted: `Input { data: "你", input_type: InsertText, is_composing: false }`

During composition, `KeyDown` and `KeyUp` events fire with `is_composing: true`. Applications should generally ignore keyboard events where `is_composing` is true to avoid interfering with the IME.

### Wayland Text Input Protocol Mapping

| Wayland Event | Framework Event |
|--------------|----------------|
| `zwp_text_input_v3::enter(surface)` | Text input focus available; enable protocol |
| `zwp_text_input_v3::leave(surface)` | Text input focus lost; disable protocol |
| `zwp_text_input_v3::preedit_string(text, cursor_begin, cursor_end)` | CompositionUpdate (or CompositionStart if first preedit) |
| `zwp_text_input_v3::commit_string(text)` | CompositionEnd + Input |
| `zwp_text_input_v3::delete_surrounding_text(before, after)` | BeforeInput + Input with appropriate InputType |
| `zwp_text_input_v3::done(serial)` | Apply all pending events atomically |

When a text input element gains focus, the framework:
1. Calls `zwp_text_input_v3::enable` on the text input object
2. Sends `set_surrounding_text` with current text and cursor position
3. Sends `set_content_type` with appropriate hints (e.g., email, number)
4. Sends `set_cursor_rectangle` with the cursor's position in surface coordinates
5. Calls `commit` to apply the state

On each text change, the framework updates `set_surrounding_text` and `set_cursor_rectangle`.

---

## Drag and Drop

### Internal DnD (Within the Application)

For drag operations within the same application (e.g., reordering list items, moving widgets), the framework provides a lightweight internal DnD system that does NOT involve Wayland's data device protocol.

#### DnD Event Types

| Event | Fires On | Bubbles | Cancelable |
|-------|---------|---------|------------|
| DragStart | Dragged element | Yes | Yes |
| Drag | Dragged element | Yes | No |
| DragEnd | Dragged element | Yes | No |
| DragEnter | Drop target | Yes | No |
| DragLeave | Drop target | Yes | No |
| DragOver | Drop target | Yes | Yes |
| Drop | Drop target | Yes | Yes |

#### DragEvent Data

```rust
pub struct DragEvent {
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// The data being dragged.
    pub data: DragData,
    /// Allowed drop effects.
    pub effect_allowed: DropEffect,
    /// Current drop effect (set by drop target during DragOver).
    pub drop_effect: DropEffect,
}

pub struct DragData {
    /// MIME type -> data pairs.
    entries: HashMap<String, Vec<u8>>,
}

impl DragData {
    pub fn set(&mut self, mime_type: &str, data: impl Into<Vec<u8>>);
    pub fn get(&self, mime_type: &str) -> Option<&[u8]>;
    pub fn types(&self) -> &[String];
    pub fn set_text(&mut self, text: &str);
    pub fn get_text(&self) -> Option<&str>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropEffect {
    None,
    Copy,
    Move,
    Link,
}
```

#### Internal DnD Lifecycle

1. User presses pointer on draggable element and moves beyond drag threshold (3px)
2. `DragStart` fires on the dragged element. Handler sets drag data and effect_allowed. If prevented, drag is cancelled.
3. `Drag` fires repeatedly on the dragged element as pointer moves
4. As pointer enters/leaves potential drop targets (hit tested each move):
   - `DragEnter` fires when entering a drop target
   - `DragOver` fires repeatedly while over a drop target. The handler must set `drop_effect` to accept the drop; default is `None` (reject).
   - `DragLeave` fires when leaving a drop target
5. When pointer is released over a valid drop target (one that set a non-None drop_effect):
   - `Drop` fires on the drop target. Handler reads drag data and performs the action.
   - `DragEnd` fires on the dragged element (with the final drop_effect).
6. If released over no valid target or drag is cancelled:
   - `DragEnd` fires on the dragged element with `drop_effect = None`

#### Drag Visual Feedback

During drag, the framework renders a ghost image of the dragged element at a fixed offset from the pointer. The original element can optionally reduce its opacity. This is handled at the rendering level:

```rust
impl Element {
    /// Make this element draggable with the given data.
    pub fn draggable(mut self, f: impl Fn(&mut DragData) + 'static) -> Self {
        self.drag_data_fn = Some(Box::new(f));
        self
    }

    /// Mark this element as a drop target.
    pub fn on_drop(mut self, f: impl Fn(&DragData) -> DropEffect + 'static) -> Self {
        self.on_drop = Some(Box::new(f));
        self
    }
}
```

### External DnD (Cross-Application via Wayland)

For drag operations between applications, the Wayland `wl_data_device` protocol is used.

#### Wayland DnD Protocol Lifecycle

**As drag source (user drags from our app to another):**

1. Create `wl_data_source`, call `offer()` for each MIME type
2. Call `wl_data_source::set_actions()` with supported actions (copy, move)
3. Call `wl_data_device::start_drag(source, origin_surface, icon_surface, serial)`
4. Compositor takes over pointer; our app receives `wl_data_source` events:
   - `target(mime_type)`: destination accepted/rejected a MIME type
   - `action(action)`: compositor negotiated the action
   - `dnd_drop_performed`: user dropped
   - `send(mime_type, fd)`: destination wants data; write to fd, close fd
   - `dnd_finished`: transfer complete; if action=move, delete source data
   - `cancelled`: drag rejected or aborted

**As drop target (user drags from another app to ours):**

1. Receive `wl_data_device::data_offer(offer)` with a new `wl_data_offer`
2. Receive `wl_data_offer::offer(mime_type)` for each available type
3. Receive `wl_data_device::enter(serial, surface, x, y, offer)` when pointer enters our surface
4. Call `wl_data_offer::set_actions(supported, preferred)` to negotiate
5. Receive `wl_data_device::motion(time, x, y)` during drag
6. For each motion, hit test and fire DragOver on the appropriate element
7. Call `wl_data_offer::accept(serial, mime_type)` to indicate willingness
8. Receive `wl_data_device::drop` when user releases
9. Call `wl_data_offer::receive(mime_type, fd)` to read data from fd
10. Call `wl_data_offer::finish()` to complete the transfer
11. Receive `wl_data_device::leave` if pointer exits without dropping

The framework translates Wayland data_device events into the same DragEnter/DragOver/DragLeave/Drop framework events, so application code handles both internal and external DnD through the same handlers.

---

## Clipboard

### Clipboard Events

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| Copy | User presses Ctrl+C or triggers copy | Yes | Yes |
| Cut | User presses Ctrl+X or triggers cut | Yes | Yes |
| Paste | User presses Ctrl+V or triggers paste | Yes | Yes |

#### ClipboardEvent Data

```rust
pub struct ClipboardEvent {
    /// The clipboard data (for reading on Paste, for writing on Copy/Cut).
    pub clipboard_data: ClipboardData,
}

pub struct ClipboardData {
    entries: HashMap<String, Vec<u8>>,
}

impl ClipboardData {
    pub fn set_text(&mut self, text: &str);
    pub fn get_text(&self) -> Option<String>;
    pub fn set(&mut self, mime_type: &str, data: Vec<u8>);
    pub fn get(&self, mime_type: &str) -> Option<&[u8]>;
    pub fn types(&self) -> Vec<String>;
}
```

#### Clipboard Lifecycle

**Copy:**
1. User presses Ctrl+C (or application triggers copy)
2. `Copy` event fires on focused element
3. Handler populates `clipboard_data` with content to copy
4. If not prevented, framework sets the Wayland selection

**Cut:**
1. User presses Ctrl+X
2. `Cut` event fires on focused element
3. Handler populates `clipboard_data` and removes content from source
4. Framework sets the Wayland selection

**Paste:**
1. User presses Ctrl+V
2. Framework reads the current Wayland selection
3. `Paste` event fires on focused element with the clipboard data
4. If not prevented, focused text input inserts the text

### Default Clipboard for Text Inputs

Text input elements have built-in clipboard behavior:
- **Copy**: Copy selected text to clipboard
- **Cut**: Copy selected text to clipboard and delete it
- **Paste**: Insert clipboard text at cursor position

Custom handlers can override this by handling the event and calling prevent_default.

### Wayland Clipboard Protocol

Clipboard uses the same `wl_data_device` / `wl_data_source` / `wl_data_offer` interfaces as DnD, but via `set_selection` instead of `start_drag`.

**Setting clipboard (copy/cut):**

1. Create `wl_data_source`
2. Call `offer("text/plain")` (and other MIME types as needed)
3. Call `wl_data_device::set_selection(source, serial)`
4. When another client pastes, we receive `wl_data_source::send(mime_type, fd)`
5. Write clipboard data to fd, close fd

**Reading clipboard (paste):**

1. When our surface gains keyboard focus, we receive `wl_data_device::selection(offer)`
2. The `wl_data_offer` describes available MIME types via `offer(mime_type)` events
3. When we want to paste, call `wl_data_offer::receive("text/plain", fd)`
4. Read data from fd until EOF

**Primary selection** (middle-click paste on Linux) uses the separate `zwp_primary_selection_device_v1` protocol with an identical pattern.

---

## Scroll Events

### Current State

The framework has `PointerScroll { x, y, delta_x, delta_y }` and `ScrollState` with momentum/rubber-band physics. There is no distinction between wheel scroll and touchpad scroll, no discrete steps, and no ScrollEnd event.

### Scroll Event Types

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| Wheel | Mouse wheel or touchpad scroll gesture | Yes | Yes |
| ScrollStart | Scroll gesture begins (touchpad finger down) | Yes | No |
| Scroll | Scroll position changed (any source) | Yes | No |
| ScrollEnd | Scroll gesture ends (touchpad finger lifted, wheel stopped) | Yes | No |

#### WheelEvent Data

```rust
pub struct WheelEvent {
    /// Pointer position.
    pub x: f32,
    pub y: f32,
    /// Scroll deltas in pixels.
    pub delta_x: f32,
    pub delta_y: f32,
    /// Original discrete steps (for mouse wheel: 1 notch = 120 units).
    pub delta_x_discrete: Option<i32>,
    pub delta_y_discrete: Option<i32>,
    /// Source of the scroll event.
    pub source: ScrollSource,
    /// Modifier keys.
    pub modifiers: Modifiers,
    /// Timestamp.
    pub time: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSource {
    /// Mouse wheel with discrete steps.
    Wheel,
    /// Touchpad / trackpad with continuous motion.
    Finger,
    /// Continuous scroll from a knob or dial.
    Continuous,
    /// Tilt of a scroll wheel.
    WheelTilt,
}
```

### Scroll Handling Strategy

Different scroll sources require different treatment:

**Mouse wheel (ScrollSource::Wheel):**
- Discrete steps, no momentum
- Each notch scrolls a fixed amount (e.g., 3 lines or ~45px)
- Use spring animation to smoothly animate to target offset
- No rubber-band overscroll (stop at bounds)

**Touchpad finger (ScrollSource::Finger):**
- Continuous 1:1 tracking while finger is down
- On finger lift (axis_stop), apply momentum deceleration
- Allow rubber-band overscroll with spring snap-back
- `axis_stop` signals finger lift -> transition to momentum phase

**Kinetic scrolling (momentum phase):**
- Decelerate using friction coefficient: `velocity *= 0.97^(dt * 60)`
- Stop when velocity < 1px/s
- If overscrolled, snap back with spring physics

### Wayland Axis Event Grouping

Wayland axis events come in groups delimited by `frame` events. A single frame may contain:
- `axis_source` (wheel, finger, continuous, wheel_tilt)
- `axis_value120` (high-res discrete value, 120 = 1 notch)
- `axis` (pixel-level continuous value)
- `axis_stop` (finger lifted)
- `axis_relative_direction` (natural vs inverted)

The framework accumulates these within a single `wl_pointer::frame` call, then produces the appropriate Wheel/ScrollEnd events.

### Scroll Bubbling

Scroll events bubble up the element tree. If the innermost scroll container has reached its scroll limit (e.g., scrolled to the bottom), the scroll event should propagate to the parent scroll container. This is "scroll chaining."

```rust
impl ScrollContainer {
    fn handle_wheel(&mut self, event: &WheelEvent) -> EventResult {
        let at_top = self.scroll_state.offset <= 0.0;
        let at_bottom = self.scroll_state.offset >= self.scroll_state.max_offset;

        if event.delta_y < 0.0 && at_top {
            return EventResult::Continue;  // Propagate to parent
        }
        if event.delta_y > 0.0 && at_bottom {
            return EventResult::Continue;  // Propagate to parent
        }

        self.scroll_state.on_scroll(event.delta_y);
        EventResult::Stop
    }
}
```

---

## Touch Events

### Wayland Touch Protocol

Wayland delivers touch events through `wl_touch`:

| Wayland Event | Parameters |
|--------------|-----------|
| `down(serial, time, surface, id, x, y)` | New touch point |
| `up(serial, time, id)` | Touch point released |
| `motion(time, id, x, y)` | Touch point moved |
| `frame` | Group of touch events complete |
| `cancel` | All active touches cancelled |
| `shape(id, major, minor)` | Touch contact ellipse (optional) |
| `orientation(id, orientation)` | Touch contact angle (optional) |

### Touch-to-Pointer Coercion

For simplicity, the primary touch (first finger down) is coerced into pointer events with `pointer_type: Touch`. This means most UI elements work with touch without any special handling.

| Touch Event | Pointer Event |
|------------|---------------|
| `touch::down` (first finger) | PointerDown with implicit capture |
| `touch::motion` (first finger) | PointerMove |
| `touch::up` (first finger) | PointerUp + release capture |
| `touch::cancel` | PointerCancel |

Additional fingers (multi-touch) generate separate pointer event streams with unique `pointer_id` values.

### Native Touch Events

For applications that need multi-touch (pinch-zoom, rotation), raw touch events are also dispatched:

```rust
pub struct TouchEvent {
    /// Unique ID for this touch point.
    pub touch_id: i32,
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// Contact area (if available from Wayland shape event).
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// Contact angle in degrees (if available from orientation event).
    pub orientation: Option<f32>,
    /// Timestamp.
    pub time: u32,
}
```

| Event | Fires When | Bubbles | Cancelable |
|-------|-----------|---------|------------|
| TouchStart | New touch point contacts surface | Yes | Yes |
| TouchMove | Touch point moves | Yes | Yes |
| TouchEnd | Touch point lifted | Yes | Yes |
| TouchCancel | System cancels all touches | Yes | No |

### Gesture Recognition

On top of raw touch events, the framework recognizes common gestures. This follows Flutter's gesture arena model:

#### Gesture Arena

When multiple gesture recognizers could match the same touch sequence, they enter a "gesture arena" and compete:

1. On TouchStart, all gesture recognizers registered on the hit-tested element and its ancestors enter the arena
2. As more events arrive, recognizers either:
   - **Reject**: Remove themselves from the arena (e.g., horizontal drag recognizer rejects when movement is vertical)
   - **Accept**: Declare victory (e.g., tap recognizer accepts when finger lifts within threshold)
3. If only one recognizer remains, it wins by default
4. The winning recognizer receives all subsequent events; losers are cancelled

#### Built-in Gesture Recognizers

```rust
pub enum GestureType {
    Tap,         // Quick touch and release
    DoubleTap,   // Two taps in quick succession
    LongPress,   // Touch held for >500ms
    Pan,         // Single finger drag
    PinchZoom,   // Two finger pinch
    Rotation,    // Two finger rotation
}
```

For the initial implementation, gesture recognition is deferred. The touch-to-pointer coercion handles the common case. Gesture recognizers can be added incrementally as needed.

---

## Hit Testing

### Current State

Hit testing walks the layout tree in reverse child order (front-to-back), checking `bounds_contains()` and returns the deepest interactive element. It does not handle transforms, clipping regions, or pointer-events-none.

### Enhancements Needed

#### 1. Clipping-Aware Hit Testing

Elements with `clip: true` create clip regions. Hit testing must respect these: a point outside the clip region should not match any descendant of the clipped element.

```rust
fn hit_test_recursive(node, element, x, y, ...) -> bool {
    if !bounds_contains(&node.bounds, x, y) {
        return false;
    }

    // If this element clips, check if point is within clip bounds
    if element.clip && !bounds_contains(&node.bounds, x, y) {
        return false;
    }

    // ... check children ...
}
```

(Currently this is implicitly handled since clip regions match bounds, but with scroll offsets and transforms this will diverge.)

#### 2. Scroll Offset

Scroll containers offset their children. Hit testing must account for the current scroll offset:

```rust
fn hit_test_recursive(node, element, x, y, ...) -> bool {
    let (hit_x, hit_y) = if element.scroll_direction.is_some() {
        // Adjust point by scroll offset
        (x + scroll_offset_x, y + scroll_offset_y)
    } else {
        (x, y)
    };

    // Check children with adjusted coordinates
}
```

#### 3. Transform-Aware Hit Testing

If the framework adds CSS-like transforms (translate, rotate, scale), hit testing must apply the inverse transform to the test point before checking bounds:

```rust
fn hit_test_recursive(node, element, x, y, ...) -> bool {
    let (local_x, local_y) = if let Some(transform) = &node.transform {
        transform.inverse().map_point(x, y)
    } else {
        (x, y)
    };

    if !bounds_contains(&node.bounds, local_x, local_y) {
        return false;
    }
    // ...
}
```

#### 4. Pointer-Events-None

Elements should be able to opt out of hit testing:

```rust
impl Element {
    /// This element and its children are invisible to pointer events.
    /// Pointer events pass through to elements below.
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events = PointerEvents::None;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    /// Normal: this element and children participate in hit testing.
    Auto,
    /// Skip this element and children in hit testing.
    None,
    /// Skip this element but still test children.
    PassThrough,
}
```

#### 5. Z-Index-Aware Hit Testing

If overlapping siblings have explicit z-index values, hit testing should test higher z-index elements first. Currently the reverse child order serves as implicit z-ordering, which is correct for non-overlapping flex layouts. For absolute-positioned or stacked elements, explicit z-index ordering is needed:

```rust
fn hit_test_children(node, element, x, y, ...) -> bool {
    // Sort children by z-index (descending) for hit testing
    let mut child_order: Vec<usize> = (0..node.children.len()).collect();
    child_order.sort_by(|a, b| {
        let z_a = element.children[*a].z_index;
        let z_b = element.children[*b].z_index;
        z_b.cmp(&z_a)  // Higher z-index tested first
    });

    for i in child_order {
        if hit_test_recursive(&node.children[i], &element.children[i], x, y, ...) {
            return true;
        }
    }
    false
}
```

---

## Rust API Design

### Event Handler Signatures

All event handlers follow a consistent pattern: they receive a reference to the event data and return an `EventResult`.

```rust
// Pointer events
pub fn on_pointer_down(self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self;
pub fn on_pointer_up(self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self;
pub fn on_pointer_move(self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self;
pub fn on_pointer_enter(self, f: impl Fn(&PointerEvent) + 'static) -> Self;  // No result (doesn't bubble)
pub fn on_pointer_leave(self, f: impl Fn(&PointerEvent) + 'static) -> Self;  // No result (doesn't bubble)

// Synthesized pointer events
pub fn on_click(self, f: impl Fn(&ClickEvent) -> EventResult + 'static) -> Self;
pub fn on_double_click(self, f: impl Fn(&ClickEvent) -> EventResult + 'static) -> Self;
pub fn on_context_menu(self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self;

// Keyboard events
pub fn on_key_down(self, f: impl Fn(&KeyboardEvent) -> EventResult + 'static) -> Self;
pub fn on_key_up(self, f: impl Fn(&KeyboardEvent) -> EventResult + 'static) -> Self;

// Focus events
pub fn on_focus(self, f: impl Fn(&FocusEvent) + 'static) -> Self;     // No result (doesn't bubble)
pub fn on_blur(self, f: impl Fn(&FocusEvent) + 'static) -> Self;      // No result (doesn't bubble)
pub fn on_focus_in(self, f: impl Fn(&FocusEvent) -> EventResult + 'static) -> Self;
pub fn on_focus_out(self, f: impl Fn(&FocusEvent) -> EventResult + 'static) -> Self;

// Input events (text input)
pub fn on_before_input(self, f: impl Fn(&TextInputEvent) -> EventResult + 'static) -> Self;
pub fn on_input(self, f: impl Fn(&TextInputEvent) -> EventResult + 'static) -> Self;
pub fn on_change(self, f: impl Fn(&str) + 'static) -> Self;  // Convenience: fires with new value

// Composition events
pub fn on_composition_start(self, f: impl Fn(&CompositionEvent) -> EventResult + 'static) -> Self;
pub fn on_composition_update(self, f: impl Fn(&CompositionEvent) -> EventResult + 'static) -> Self;
pub fn on_composition_end(self, f: impl Fn(&CompositionEvent) -> EventResult + 'static) -> Self;

// Scroll events
pub fn on_wheel(self, f: impl Fn(&WheelEvent) -> EventResult + 'static) -> Self;

// DnD events
pub fn on_drag_start(self, f: impl Fn(&mut DragEvent) -> EventResult + 'static) -> Self;
pub fn on_drag_over(self, f: impl Fn(&DragEvent) -> EventResult + 'static) -> Self;
pub fn on_drop(self, f: impl Fn(&DragEvent) -> EventResult + 'static) -> Self;
pub fn on_drag_enter(self, f: impl Fn(&DragEvent) + 'static) -> Self;
pub fn on_drag_leave(self, f: impl Fn(&DragEvent) + 'static) -> Self;

// Clipboard events
pub fn on_copy(self, f: impl Fn(&mut ClipboardEvent) -> EventResult + 'static) -> Self;
pub fn on_cut(self, f: impl Fn(&mut ClipboardEvent) -> EventResult + 'static) -> Self;
pub fn on_paste(self, f: impl Fn(&ClipboardEvent) -> EventResult + 'static) -> Self;
```

### Backward Compatibility

No need to make anything backwards compatible, we'll be going and fixing individual 
```

### Element Handler Storage

```rust
pub struct ElementHandlers {
    // Pointer (bubble phase)
    pub pointer_down: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub pointer_up: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub pointer_move: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub pointer_enter: Option<Box<dyn Fn(&PointerEvent)>>,
    pub pointer_leave: Option<Box<dyn Fn(&PointerEvent)>>,

    // Pointer (capture phase)
    pub pointer_down_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub pointer_up_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub pointer_move_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,

    // Synthesized
    pub click: Option<Box<dyn Fn(&ClickEvent) -> EventResult>>,
    pub double_click: Option<Box<dyn Fn(&ClickEvent) -> EventResult>>,
    pub context_menu: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,

    // Keyboard
    pub key_down: Option<Box<dyn Fn(&KeyboardEvent) -> EventResult>>,
    pub key_up: Option<Box<dyn Fn(&KeyboardEvent) -> EventResult>>,

    // Focus
    pub focus: Option<Box<dyn Fn(&FocusEvent)>>,
    pub blur: Option<Box<dyn Fn(&FocusEvent)>>,
    pub focus_in: Option<Box<dyn Fn(&FocusEvent) -> EventResult>>,
    pub focus_out: Option<Box<dyn Fn(&FocusEvent) -> EventResult>>,

    // Input
    pub before_input: Option<Box<dyn Fn(&TextInputEvent) -> EventResult>>,
    pub input: Option<Box<dyn Fn(&TextInputEvent) -> EventResult>>,
    pub change: Option<Box<dyn Fn(&str)>>,

    // Composition
    pub composition_start: Option<Box<dyn Fn(&CompositionEvent) -> EventResult>>,
    pub composition_update: Option<Box<dyn Fn(&CompositionEvent) -> EventResult>>,
    pub composition_end: Option<Box<dyn Fn(&CompositionEvent) -> EventResult>>,

    // Scroll
    pub wheel: Option<Box<dyn Fn(&WheelEvent) -> EventResult>>,

    // DnD
    pub drag_start: Option<Box<dyn Fn(&mut DragEvent) -> EventResult>>,
    pub drag_over: Option<Box<dyn Fn(&DragEvent) -> EventResult>>,
    pub drop: Option<Box<dyn Fn(&DragEvent) -> EventResult>>,
    pub drag_enter: Option<Box<dyn Fn(&DragEvent)>>,
    pub drag_leave: Option<Box<dyn Fn(&DragEvent)>>,

    // Clipboard
    pub copy: Option<Box<dyn Fn(&mut ClipboardEvent) -> EventResult>>,
    pub cut: Option<Box<dyn Fn(&mut ClipboardEvent) -> EventResult>>,
    pub paste: Option<Box<dyn Fn(&ClipboardEvent) -> EventResult>>,
}
```

### ClickEvent Data

```rust
pub struct ClickEvent {
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// Which button was clicked.
    pub button: MouseButton,
    /// Click count (1 = single, 2 = double, 3 = triple).
    pub count: u32,
    /// Modifier keys.
    pub modifiers: Modifiers,
    /// Pointer type that generated this click.
    pub pointer_type: PointerType,
}
```

---

## Wayland Protocol Mapping (Complete Reference)

### wl_pointer Events

| Wayland Event | Parameters | Framework Event(s) |
|--------------|-----------|-------------------|
| enter | serial, surface, x, y | PointerMove (seed position), internal pointer-entered-surface flag |
| leave | serial, surface | PointerLeave on all hovered, PointerCancel if button held |
| motion | time, x, y | PointerMove -> triggers Enter/Leave/Over/Out on path changes |
| button | serial, time, button, state | PointerDown (pressed) or PointerUp (released); may trigger Click, DoubleClick, ContextMenu, AuxClick |
| axis | time, axis, value | Accumulated -> WheelEvent (pixel delta) |
| frame | (none) | Flush accumulated pointer state; emit combined events |
| axis_source | source | Stored: wheel(0), finger(1), continuous(2), wheel_tilt(3) -> ScrollSource |
| axis_stop | time, axis | ScrollEnd event (finger lifted from touchpad) |
| axis_discrete | axis, discrete | Stored for discrete scroll step info (legacy) |
| axis_value120 | axis, value120 | High-res discrete: 120 = 1 notch. Stored for WheelEvent.delta_discrete |
| axis_relative_direction | axis, direction | Natural(0) vs inverted(1) scrolling. Stored for scroll direction. |

### wl_keyboard Events

| Wayland Event | Parameters | Framework Action |
|--------------|-----------|-----------------|
| keymap | format, fd, size | Load XKB keymap; create xkb_state; map keysyms to KeyValue |
| enter | serial, surface, keys | Surface gained kbd focus; emit KeyDown for held keys |
| leave | serial, surface | Surface lost kbd focus; cancel repeat; emit KeyUp for held keys |
| key | serial, time, key, state | Translate via xkb; emit KeyDown/KeyUp; start/stop repeat; emit TextInput for printable chars; trigger shortcuts |
| modifiers | serial, depressed, latched, locked, group | Update xkb_state; update Modifiers struct |
| repeat_info | rate, delay | Store repeat timing (rate=0 means server-side repeat) |

### wl_touch Events

| Wayland Event | Parameters | Framework Event(s) |
|--------------|-----------|-------------------|
| down | serial, time, surface, id, x, y | TouchStart; for primary touch: PointerDown (with implicit capture) |
| up | serial, time, id | TouchEnd; for primary: PointerUp (release capture) |
| motion | time, id, x, y | TouchMove; for primary: PointerMove |
| frame | (none) | Flush touch events atomically |
| cancel | (none) | TouchCancel + PointerCancel for all active touches |
| shape | id, major, minor | Store for TouchEvent.width/height |
| orientation | id, orientation | Store for TouchEvent.orientation |

### zwp_text_input_v3 Events

| Wayland Event | Parameters | Framework Event(s) |
|--------------|-----------|-------------------|
| enter | surface | IME available; enable if text input focused |
| leave | surface | IME no longer available; disable |
| preedit_string | text, cursor_begin, cursor_end | CompositionStart (if new) or CompositionUpdate |
| commit_string | text | CompositionEnd + Input(InsertText) |
| delete_surrounding_text | before_length, after_length | BeforeInput + Input(DeleteContentBackward/Forward) |
| done | serial | Apply all pending composition events atomically |

### wl_data_device Events (DnD + Clipboard)

| Wayland Event | Parameters | Framework Event(s) |
|--------------|-----------|-------------------|
| data_offer | offer | Store offer for upcoming enter/selection |
| enter | serial, surface, x, y, offer | DragEnter on hit-tested element |
| leave | (none) | DragLeave; destroy offer |
| motion | time, x, y | DragOver on hit-tested element |
| drop | (none) | Drop on current target |
| selection | offer | Clipboard content changed; store for paste |

### wl_data_source Events (when we are the source)

| Wayland Event | Parameters | Framework Action |
|--------------|-----------|-----------------|
| target | mime_type | Feedback: target accepted/rejected type |
| send | mime_type, fd | Write drag/clipboard data to fd, close fd |
| cancelled | (none) | DragEnd with effect=None; destroy source |
| dnd_drop_performed | (none) | Drop physically happened (but not yet finished) |
| dnd_finished | (none) | Transfer complete; DragEnd with final effect |
| action | action | Store negotiated action (copy/move/link) |

---

## Implementation Order

### Phase 1: Event Propagation Foundation -- DONE

**Goal**: Replace the flat dispatch model with capture/bubble phases.

1. [x] Define `EventResult` enum and `EventPhase` enum
2. [x] Add `ElementId` (use pre-order index for now, opaque ID later)
3. [x] Refactor `EventState::dispatch()` to build event path and run capture -> target -> bubble
4. [x] Add capture-phase handler slots to Element
5. [x] Update existing handlers to return `EventResult` (with backward-compatible wrappers)
6. [x] Implement `stop_propagation` and `prevent_default` via EventResult returns
7. Tests: verify capture fires before bubble, stopPropagation halts traversal, preventDefault prevents default actions

### Phase 2: Rich Pointer Events -- DONE

**Goal**: Full pointer event taxonomy with proper data.

1. [x] Define `PointerEvent` struct with all fields
2. [x] Replace `InputEvent::PointerMove/PointerButton` with richer pointer events
3. [x] Implement PointerEnter/PointerLeave (non-bubbling, current on_hover semantics)
4. Implement PointerOver/PointerOut (bubbling) -- deferred, rarely needed
5. Implement pointer event coalescing (accumulate between frames) -- deferred
6. [x] Add `pointer_id`, `pointer_type`, `buttons` bitmask tracking
7. Tests: enter/leave vs over/out behavior, coalescing

### Phase 3: Pointer Capture -- DONE

**Goal**: Full pointer capture API.

1. [x] Add `pointer_captures: HashMap<u32, ElementId>` to EventState
2. [x] Implement `set_pointer_capture()`, `release_pointer_capture()`, `has_pointer_capture()`
3. [x] When capture is set, bypass hit testing for that pointer (PointerMove redirected to capturing element)
4. Fire GotPointerCapture/LostPointerCapture events -- deferred (rarely needed)
5. Implement implicit capture for touch pointers on PointerDown -- deferred (no touch yet)
6. [x] Auto-release capture on PointerUp when all buttons released
7. Tests: capture redirects events, implicit touch capture, release on up

### Phase 4: Click Synthesis (Double-Click, Context Menu, AuxClick) -- DONE

**Goal**: Synthesized high-level pointer events.

1. [x] Implement click count tracking (time + distance threshold)
2. [x] Fire Click events with count field
3. [x] Fire DoubleClick on count == 2
4. [x] Fire ContextMenu on right-click release
5. [x] Fire AuxClick on middle/back/forward button clicks
6. Tests: single click, double click timing, triple click, right-click context menu

### Phase 5: Focus System -- DONE

**Goal**: Complete focus management with tab navigation.

1. [x] Add `focused: Option<ElementId>` to EventState
2. [x] Add `focusable`, `tab_index` properties to Element
3. [x] Implement Focus, Blur, FocusIn, FocusOut events
4. [x] PointerDown default action: set focus to target (if focusable)
5. [x] Build tab order from element tree on each render
6. [x] Handle Tab/Shift+Tab to move focus through tab order
7. [x] Implement focus trapping for modals (focus_trap property on Element)
8. [x] Implement focus_visible tracking (keyboard vs pointer focus)
9. Render focus ring on focused elements (when focus_visible) -- deferred to display_list phase
10. [x] Route keyboard events to focused element
11. Tests: click to focus, tab navigation, shift+tab, focus trap, focus ring visibility

### Phase 6: Keyboard Enhancement -- DONE

**Goal**: Full keyboard event system with XKB integration.

1. [x] Implement SCTK keyboard handler (delegate_keyboard)
2. [x] Parse XKB keymap from wl_keyboard::keymap (handled by SCTK internally)
3. [x] Translate keysyms to KeyValue/KeyCode
4. Client-side key repeat -- deferred (SCTK handles repeat internally)
5. [x] Track modifier state from wl_keyboard::modifiers (SCTK Modifiers -> framework Modifiers)
6. [x] Dispatch KeyDown/KeyUp through three-phase propagation to focused element
7. Implement keyboard shortcuts system (capture phase at root) -- deferred to app layer
8. [x] Default actions: Tab moves focus
9. Tests: key translation, repeat behavior, modifier tracking, shortcuts

### Phase 7: Text Input and IME -- IN PROGRESS

**Goal**: Full text editing support with IME composition.

1. Integrate zwp_text_input_v3 protocol via SCTK
2. Enable/disable text input when text input elements gain/lose focus
3. [x] Implement BeforeInput/Input events for text modifications
4. Send surrounding text and cursor position to compositor
5. Handle preedit_string for IME composition display
6. Handle commit_string for IME text insertion
7. Handle delete_surrounding_text
8. [x] Fire CompositionStart/Update/End events (event types + handlers defined)
9. [x] Set is_composing on keyboard events during composition
10. Render preedit text with underline styling in text input elements
11. Tests: basic typing, backspace, IME composition lifecycle

### Phase 8: Scroll Enhancement -- DONE

**Goal**: Rich scroll events with source awareness.

1. [x] Restructure Wayland axis event handling to extract source/discrete/stop from SCTK
2. [x] Distinguish scroll source (wheel vs finger vs continuous vs wheel_tilt)
3. [x] Emit WheelEvent with delta, source, and is_discrete flag
4. [x] Emit ScrollEnd from axis_stop (on_scroll_end handler on Element)
5. Different physics for wheel (spring to target) vs finger (momentum) -- already in scroll.rs
6. Implement scroll chaining (bubble when at scroll limit) -- deferred
7. Handle axis_value120 for high-resolution wheel scrolling -- deferred (SCTK handles this)
8. Tests: wheel scroll, touchpad scroll, scroll chaining, momentum

### Phase 9: Drag and Drop -- IN PROGRESS

**Goal**: Internal and external DnD.

1. [x] Implement internal DnD state machine in EventState
2. [x] DragStart detection (reuse existing drag threshold)
3. [x] DragOver/DragEnter/DragLeave via hit testing during drag
4. [x] Drop event with data transfer
5. [x] DragEnd event with final effect
6. Render drag ghost image during drag
7. Integrate wl_data_device for external DnD (receiving drops from other apps)
8. Integrate wl_data_device for external DnD (initiating drags to other apps)
9. Tests: internal DnD lifecycle, drop acceptance/rejection

### Phase 10: Clipboard -- DONE (Wayland integration deferred)

**Goal**: Copy, cut, paste support.

1. [x] Add ClipboardData/ClipboardEvent types to input.rs
2. [x] Add on_copy/on_cut/on_paste handler slots on Element
3. [x] Fire Copy/Cut/Paste events on Ctrl+C/X/V
4. [x] Default clipboard behavior for text input elements
5. Integrate wl_data_device::set_selection for setting clipboard -- deferred (needs VM)
6. Integrate wl_data_device::selection for reading clipboard -- deferred (needs VM)
7. Implement primary selection (zwp_primary_selection_device_v1) for middle-click paste -- deferred (needs VM)
8. Tests: copy text, paste text, custom clipboard handlers

### Phase 11: Touch Events -- DONE (shape/orientation stubbed)

**Goal**: Native touch support beyond pointer coercion.

1. [x] Implement SCTK touch handler (delegate_touch) in wayland.rs
2. [x] Coerce primary touch to pointer events (PointerMove + PointerDown/Up with MouseButton::Left)
3. [x] Fire native TouchStart/Move/End/Cancel events for all touches via target+bubble dispatch
4. [x] Handle multi-touch (active_touches HashMap, primary_touch_id tracking)
5. [x] Store touch shape/orientation data -- stubbed in TouchHandler (TODO when needed)
6. Tests: single touch, multi-touch, touch cancel -- deferred (needs VM for build)

### Phase 12: Hit Testing Enhancements -- DONE

**Goal**: Production-quality hit testing.

1. [x] Pointer-events-none support (PointerEvents enum: Auto, None, PassThrough)
2. Scroll-offset-aware hit testing -- deferred (needs scroll state plumbing)
3. Transform-aware hit testing -- deferred (no transforms yet)
4. [x] Z-index-aware child ordering in hit testing
5. Tests: pointer-events-none, scroll offset hit testing, transformed elements

---

## Edge Cases and Gotchas

### Pointer Capture + Focus Interaction
- PointerDown sets pointer capture AND moves focus. The capture handler may prevent focus change by returning PreventDefault.
- If pointer capture is active and the pointer moves to a different focusable element, focus does NOT automatically change. Focus only changes on PointerDown.

### Keyboard Events During IME
- When `is_composing` is true, applications should generally ignore KeyDown events to avoid interfering with IME candidate selection.
- The framework's built-in text input widget handles this automatically.
- Shortcuts (Ctrl+C, etc.) should still work during composition if the IME doesn't consume them.

### Focus and Pointer Leave
- If the pointer leaves the surface (wl_pointer::leave), focus is NOT lost. Focus only changes on user action (click, tab) or programmatically.
- If keyboard focus leaves the surface (wl_keyboard::leave), the framework should NOT blur the focused element. When keyboard focus returns (wl_keyboard::enter), the same element should still be focused.

### Nested Scroll Containers
- Inner scroll container consumes scroll events when it has room to scroll.
- When inner container reaches its limit (top or bottom), events propagate to the outer container (scroll chaining).
- During momentum scrolling in the inner container, if it hits the limit, momentum should NOT transfer to the outer container (this prevents unexpected scrolling).

### Drag Threshold and Click
- If the pointer moves less than 3px between PointerDown and PointerUp, it is a click, not a drag.
- If it moves more than 3px, it is a drag, and no Click event fires.
- A ContextMenu event fires on right-button PointerUp regardless of drag distance.

### Re-entrancy
- Event handlers may mutate application state, which triggers re-render, which rebuilds the element tree.
- Event dispatch must NOT be in progress when re-render happens. Process all events for the current frame first, then re-render.
- This is already the case: `process_input_events()` runs before `draw()`.

### Element Identity Across Renders
- The current system uses pre-order indices as element IDs. These change when the tree structure changes.
- Focus, pointer capture, and hover tracking all reference element IDs. When the tree re-renders, these IDs must be re-mapped.
- Solution: use the `key` property (already on Element) for stable identity. Elements with keys retain their focus/capture/hover state across re-renders. Elements without keys use positional matching (same index in same parent).

### Wayland Serial Numbers
- Many Wayland operations require a serial number from a recent input event (e.g., set_selection, start_drag, show popup).
- The framework must track the most recent serial from pointer/keyboard/touch events and pass it to operations that need it.
- Store `last_pointer_serial`, `last_keyboard_serial`, `last_touch_serial` in the Wayland state.

### Double-Click on Touch
- Touch does not have a native "double-click" concept. Two quick taps at the same location within 400ms should still generate a DoubleClick event via the same click-counting mechanism used for mouse.
- Long press (>500ms) on touch generates a ContextMenu event instead of Click.
