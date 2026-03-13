# Form Elements Implementation Plan

## Overview

All form elements are custom-rendered via Skia on a Wayland SHM buffer surface. There are no native OS form controls -- every element is drawn by the framework using `skia_safe` primitives (rounded rects, paths, text, circles) and driven by the existing event dispatch system (`EventState`, `InputEvent`, hit testing).

Each form element is an `ElementKind` variant with its own rendering path in `display_list.rs` and `renderer.rs`, its own state struct (like `TextInputState`), and builder functions that produce `Element` values. The controlled-value pattern already established by `TextInput` (value passed in, `on_change` callback emits new value) is used for all stateful form elements.

### Architecture Decisions

1. **ElementKind per widget** -- each form element gets its own `ElementKind` variant. This keeps layout measurement, display list generation, and rendering clean and type-safe.
2. **Controlled values only** -- the View owns all state. Elements receive current values and fire callbacks. No internal "uncontrolled" mode.
3. **State structs per instance** -- complex elements (TextInput, Textarea, Select) need per-instance state (cursor position, blink timer, scroll offset). These are keyed by element key in a `HashMap` inside `WaylandState`, analogous to the existing `TextInputState`.
4. **Popup surfaces for overlays** -- Select dropdowns and date/color pickers require popup surfaces (see `plans/trash/ui-popup-surfaces.md`). These use Wayland layer-shell popup surfaces.
5. **Focus management** -- a single focused element receives keyboard events. Focus is tracked by element key in `EventState`. Tab/Shift+Tab cycles focus. Click sets focus.

---

## Shared Concepts

### Form Element States

Every form element can be in a combination of these visual states:

| State | Trigger | Visual Effect |
|-------|---------|---------------|
| **Default** | No interaction | Base appearance |
| **Hovered** | Pointer over element | Lighter background, cursor change |
| **Focused** | Element has keyboard focus | Focus ring (2px blue outline) |
| **Active/Pressed** | Mouse down on element | Darkened background, pressed appearance |
| **Disabled** | `.disabled(true)` | 40% opacity, no interaction, `NotAllowed` cursor |
| **Read-only** | `.read_only(true)` | Normal appearance but no editing, `Default` cursor |
| **Invalid/Error** | Validation failure | Red border, error message below |

States are combined: an element can be hovered + focused + invalid simultaneously.

### State Tracking

Add to `Element`:

```rust
pub struct Element {
    // ... existing fields ...
    pub disabled: bool,
    pub read_only: bool,
    pub focused: bool,  // set by framework, not user
    pub tab_index: Option<i32>,  // -1 = not focusable, 0 = natural order, >0 = explicit order
}
```

Add to `EventState`:

```rust
pub struct EventState {
    // ... existing fields ...
    pub focused_key: Option<String>,  // key of the currently focused element
    focus_order: Vec<String>,  // computed tab order from last layout pass
}
```

### Value Binding Model (Controlled)

All form elements follow the same pattern:

```rust
// View state
struct MyView {
    username: Reactive<String>,
    agree: Reactive<bool>,
    volume: Reactive<f32>,
}

// In render()
text_input(cx.handle(), self.username.get(cx))
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.username.set(val))
    })

checkbox(cx.handle(), *self.agree.get(cx))
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.agree.set(val))
    })
```

The `on_change` callback receives the new value and queues a mutation via `Handle`. The framework re-renders with the new value on the next frame.

### Focus Management

#### Focus Ring Rendering

A focused element gets a 2px outline rendered *outside* its bounds:

```
Skia: draw_rrect(expanded_bounds, focus_ring_paint)
- color: rgba(66, 133, 244, 200)  -- blue focus ring
- bounds: element bounds expanded by 2px in each direction
- corner_radius: element corner_radius + 2px
- painted BEFORE the element background (so element covers inner portion)
```

#### Focus Traversal

- **Tab**: move focus to next focusable element in tab order
- **Shift+Tab**: move focus to previous focusable element
- **Click**: focus the clicked element (if focusable), blur the previous
- **Escape**: blur the current element (clear focus)
- Tab order is computed from the layout tree during each render pass, following document order (pre-order traversal) unless overridden by `tab_index`.

#### Focus Events

Add to `Element`:

```rust
pub on_focus: Option<Box<dyn Fn(bool)>>,  // true = gained, false = lost
```

### Validation

Validation is application-level, not framework-level. The framework provides visual hooks:

```rust
text_input(handle, &value)
    .error(if value.is_empty() { Some("Required") } else { None })
```

```rust
pub struct Element {
    // ... existing fields ...
    pub error: Option<String>,
}
```

When `error` is `Some`:
- Border color changes to red `rgba(220, 53, 69, 255)`
- Error text is rendered below the element in red, 12px font
- The error text is a pseudo-child that participates in layout (adds to element height)

### Shared Builder Methods

All form elements gain these chainable methods:

```rust
impl Element {
    pub fn disabled(mut self, d: bool) -> Self { self.disabled = d; self }
    pub fn read_only(mut self, r: bool) -> Self { self.read_only = r; self }
    pub fn tab_index(mut self, i: i32) -> Self { self.tab_index = Some(i); self }
    pub fn error(mut self, e: Option<&str>) -> Self {
        self.error = e.map(|s| s.to_string()); self
    }
    pub fn on_focus(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus = Some(Box::new(f)); self
    }
}
```

### Label Association

Labels are not a separate `ElementKind`. They are regular `text()` elements. The framework does not enforce label-input association -- that is a layout concern handled by the View's `render()` method:

```rust
column().gap(4.0)
    .child(text("Username").font_size(12.0).color(LABEL_COLOR))
    .child(text_input(handle, &value).on_change(on_change))
```

For accessibility (future): labels can be associated via matching `key` values (`label_for` on text, matching `key` on input).

### Fieldset / Grouping

Fieldsets are regular containers with a specific visual style:

```rust
fn fieldset(legend: &str) -> Element {
    column()
        .border(rgba(100, 100, 100, 255), 1.0)
        .rounded(6.0)
        .padding(16.0)
        .gap(12.0)
        .child(text(legend).font_size(12.0).color(LEGEND_COLOR))
}
```

This is a pattern, not a framework primitive.

---

## Text Input

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    TextInput {
        value: String,
        placeholder: String,
        variant: TextInputVariant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextInputVariant {
    Text,
    Password,
    Email,
    Url,
    Search,
    Number,
    Tel,
}
```

### Builder Functions

```rust
pub fn text_input(value: &str) -> Element { /* variant: Text */ }
pub fn password_input(value: &str) -> Element { /* variant: Password */ }
pub fn email_input(value: &str) -> Element { /* variant: Email */ }
pub fn url_input(value: &str) -> Element { /* variant: Url */ }
pub fn search_input(value: &str) -> Element { /* variant: Search */ }
pub fn number_input(value: &str) -> Element { /* variant: Number */ }
pub fn tel_input(value: &str) -> Element { /* variant: Tel */ }
```

All return `Element` with `kind: ElementKind::TextInput { .. }` and `cursor: Some(CursorStyle::Text)`.

Additional chainable methods:

```rust
impl Element {
    pub fn placeholder(mut self, text: &str) -> Self { /* already exists */ }
    pub fn max_length(mut self, n: usize) -> Self { /* new */ }
    pub fn min_length(mut self, n: usize) -> Self { /* new, for validation hint */ }
    pub fn pattern(mut self, regex: &str) -> Self { /* new, for validation hint */ }
    pub fn autocomplete(mut self, on: bool) -> Self { /* new */ }
    pub fn select_on_focus(mut self, on: bool) -> Self { /* new */ }

    // Number input specific
    pub fn min_value(mut self, v: f64) -> Self { /* new */ }
    pub fn max_value(mut self, v: f64) -> Self { /* new */ }
    pub fn step(mut self, v: f64) -> Self { /* new */ }
}
```

### TextInputState (Enhanced)

Replace the existing `TextInputState` with a much more capable version:

```rust
pub struct TextInputState {
    // Cursor
    pub cursor_position: usize,       // character index
    pub cursor_visible: bool,
    pub blink_timer_ms: f32,
    pub scroll_offset: f32,           // horizontal pixel offset for scrolling

    // Selection
    pub selection_anchor: Option<usize>,  // where selection started (None = no selection)

    // IME Composition
    pub composing: bool,
    pub compose_text: String,         // preedit string from IME
    pub compose_cursor: usize,        // cursor within preedit

    // Undo/Redo
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub undo_group_timer_ms: f32,     // group rapid edits into single undo entry

    // Interaction tracking
    pub click_count: u32,             // for double/triple-click detection
    pub last_click_time_ms: f32,
    pub last_click_position: usize,
    pub mouse_selecting: bool,        // mouse is held down and dragging to select
}

pub struct UndoEntry {
    pub value: String,
    pub cursor_position: usize,
    pub selection_anchor: Option<usize>,
}
```

### Cursor Movement

All cursor movement resets the blink timer (cursor becomes visible).

| Action | Keys | Behavior |
|--------|------|----------|
| Move left 1 char | Left | `cursor_position -= 1` (clamp 0). If selection exists, collapse to left edge. |
| Move right 1 char | Right | `cursor_position += 1` (clamp len). If selection exists, collapse to right edge. |
| Move left 1 word | Ctrl+Left | Move to start of previous word boundary (skip whitespace, then skip non-whitespace). |
| Move right 1 word | Ctrl+Right | Move to end of next word boundary (skip non-whitespace, then skip whitespace). |
| Move to line start | Home | `cursor_position = 0` |
| Move to line end | End | `cursor_position = text.chars().count()` |
| Select left 1 char | Shift+Left | Set `selection_anchor` if None (to current position), then move cursor left. |
| Select right 1 char | Shift+Right | Set `selection_anchor` if None, then move cursor right. |
| Select left 1 word | Ctrl+Shift+Left | Set anchor if None, move cursor to prev word boundary. |
| Select right 1 word | Ctrl+Shift+Right | Set anchor if None, move cursor to next word boundary. |
| Select to start | Shift+Home | Set anchor if None, move cursor to 0. |
| Select to end | Shift+End | Set anchor if None, move cursor to end. |
| Select all | Ctrl+A | `selection_anchor = Some(0)`, `cursor_position = len`. |

Word boundary detection: iterate chars, a word boundary is a transition between `char::is_alphanumeric()` and `!char::is_alphanumeric()`.

### Selection

**Selection model**: `selection_anchor` is where the selection started; `cursor_position` is where it ends. The selected range is `min(anchor, cursor)..max(anchor, cursor)`. The anchor stays fixed while the cursor moves.

**Mouse selection**:
- **Single click**: set `cursor_position` to the character index at click x-coordinate. Clear selection. Set `click_count = 1`.
- **Double click** (within 400ms of single click at same position): select the word under the cursor. Set `selection_anchor` to word start, `cursor_position` to word end. Set `click_count = 2`.
- **Triple click** (within 400ms of double click): select all text. `selection_anchor = 0`, `cursor_position = len`. Set `click_count = 3`.
- **Shift+click**: extend selection from current anchor (or cursor) to clicked position.
- **Click+drag**: set anchor at mousedown position, update cursor to current mouse position on each PointerMove while mouse is held.

Character index from x-coordinate: measure each character prefix with `font.measure_str()`, find the boundary closest to `click_x - element_x + scroll_offset`.

```rust
fn char_index_at_x(text: &str, x: f32, font_size: f32) -> usize {
    let font = make_font(font_size);
    let mut prev_width = 0.0;
    for (i, _ch) in text.char_indices() {
        let prefix = &text[..text.ceil_char_boundary(i + 1)]; // next char boundary
        let (width, _) = font.measure_str(prefix, None);
        let midpoint = (prev_width + width) / 2.0;
        if x < midpoint {
            return text[..i].chars().count();
        }
        prev_width = width;
    }
    text.chars().count()
}
```

### Editing Operations

| Action | Keys | Behavior |
|--------|------|----------|
| Insert text | Character keys / TextInput event | If selection exists, delete selected text first. Insert at cursor. Push undo. Fire `on_change`. |
| Backspace | Backspace | If selection, delete selected. Else delete char before cursor. |
| Delete forward | Delete | If selection, delete selected. Else delete char after cursor. |
| Delete word back | Ctrl+Backspace | Delete from cursor to previous word boundary. |
| Delete word forward | Ctrl+Delete | Delete from cursor to next word boundary. |
| Cut | Ctrl+X | Copy selection to clipboard, delete selection. |
| Copy | Ctrl+C | Copy selection to clipboard. |
| Paste | Ctrl+V | Read clipboard, insert at cursor (replacing selection if any). |
| Undo | Ctrl+Z | Pop undo stack, push current state to redo stack. |
| Redo | Ctrl+Shift+Z or Ctrl+Y | Pop redo stack, push current state to undo stack. |

**Delete selection**:

```rust
fn delete_selection(&mut self, current: &str) -> String {
    let (start, end) = self.selection_range();
    let byte_start = char_to_byte_pos(current, start);
    let byte_end = char_to_byte_pos(current, end);
    let mut result = String::with_capacity(current.len());
    result.push_str(&current[..byte_start]);
    result.push_str(&current[byte_end..]);
    self.cursor_position = start;
    self.selection_anchor = None;
    result
}
```

### Clipboard Integration (Wayland)

Clipboard on Wayland uses the `wl_data_device` protocol:

**Copy/Cut** (setting clipboard):
1. Create a `wl_data_source` offering MIME type `text/plain;charset=utf-8`.
2. Call `wl_data_device.set_selection(source, serial)`.
3. When the compositor sends `data_source.send(fd)`, write the selected text to the fd.

**Paste** (reading clipboard):
1. Get the current `wl_data_offer` from `data_device.data_offer`.
2. Call `data_offer.receive("text/plain;charset=utf-8", write_fd)` and read from `read_fd`.
3. The read must be done asynchronously (the compositor writes to the pipe on the next roundtrip).

Implementation in `WaylandState`:
- Bind `wl_data_device_manager` in `setup`.
- Get `wl_data_device` for the seat.
- Expose clipboard read/write through a `ClipboardState` struct accessible from event handlers.
- `Handle` gains a `clipboard_write(text)` method and event handlers can call it.
- For paste, the framework reads the clipboard in the event loop and delivers it as a `InputEvent::Paste { text: String }` event to the focused element.

Add to `InputEvent`:

```rust
pub enum InputEvent {
    // ... existing variants ...
    Paste { text: String },
}
```

### IME Composition (Wayland text-input-v3)

IME (Input Method Editor) is critical for CJK input and must be supported.

**Protocol**: `zwp_text_input_v3` from `text-input-unstable-v3`.

**Flow**:
1. When a text input gains focus, call `text_input.enable()` then `text_input.commit()`.
2. Set content type: `text_input.set_content_type(hint, purpose)` -- e.g., `purpose: Normal` for text, `purpose: Password` for passwords (disables IME), `purpose: Number` for number inputs.
3. Set cursor rectangle: `text_input.set_cursor_rectangle(x, y, w, h)` so the IME popup positions correctly.
4. Receive `preedit_string(text, cursor_begin, cursor_end)` -- the composing text. Render it inline with an underline, with the IME cursor at `cursor_begin`.
5. Receive `commit_string(text)` -- finalized text. Insert it, clear preedit.
6. Receive `delete_surrounding_text(before_length, after_length)` -- delete text around cursor before commit.
7. When text input loses focus, call `text_input.disable()` then `text_input.commit()`.

**Rendering preedit**:
- Draw the preedit string inline at the cursor position with a dotted underline.
- The preedit cursor is drawn within the preedit string (thin line).
- The preedit text has a subtle background highlight `rgba(66, 133, 244, 30)`.

**State additions to `TextInputState`**:

```rust
pub composing: bool,
pub compose_text: String,
pub compose_cursor_begin: i32,  // byte offset within compose_text, -1 = end
pub compose_cursor_end: i32,
```

**When to disable IME**: Password variant should set purpose to `Password`, which tells the compositor to bypass the IME.

### Password Masking

For `TextInputVariant::Password`:
- Display each character as a bullet: `\u{2022}` (Unicode BULLET).
- The mask character string is generated: `"\u{2022}".repeat(value.chars().count())`.
- Cursor positioning and selection use the masked string for rendering but the real string for editing.
- Optionally, briefly show the last typed character for 1 second before masking it (like mobile behavior). Controlled by a timer in `TextInputState`:

```rust
pub password_reveal_char: Option<usize>,  // index of char to show
pub password_reveal_timer_ms: f32,        // counts down from 1000
```

### Placeholder Text

Already partially implemented. Enhancements:
- Placeholder text uses 40% opacity of the text color (already done).
- Placeholder disappears entirely when the input has focus and any text is entered.
- Placeholder should be ellipsized if it overflows the input width.

### Text Scrolling Within Fixed-Width Input

When the text is wider than the input element:

1. Measure total text width: `font.measure_str(value, None).0`.
2. Measure text up to cursor: `font.measure_str(&value[..cursor_byte_pos], None).0`.
3. Compute visible window: `[scroll_offset, scroll_offset + element_width - padding]`.
4. Adjust `scroll_offset` so cursor is always visible:
   - If `cursor_x < scroll_offset`: `scroll_offset = cursor_x`
   - If `cursor_x > scroll_offset + visible_width`: `scroll_offset = cursor_x - visible_width`
5. Clip the text rendering to the element bounds (already supported via `PushClip`).
6. Translate text by `-scroll_offset` before drawing.

### Selection Highlight Rendering

Draw a filled rectangle behind the selected text:

```rust
fn draw_selection(canvas, text, font, selection_start, selection_end, y, height) {
    let x_start = font.measure_str(&text[..start_byte], None).0;
    let x_end = font.measure_str(&text[..end_byte], None).0;
    let rect = Rect::from_xywh(
        element_x + x_start - scroll_offset,
        element_y,
        x_end - x_start,
        element_height,
    );
    let mut paint = Paint::default();
    paint.set_color(SELECTION_COLOR);  // rgba(66, 133, 244, 80)
    canvas.draw_rect(rect, &paint);
}
```

Selection highlight is drawn BEFORE the text, so text is readable on top.

### Cursor Rendering

- Vertical line, 2px wide, full height of the input.
- Color: text color at full opacity.
- Position: after the character at `cursor_position`.
- Blink: 530ms on, 530ms off (already implemented).
- Reset to visible on any cursor movement or text edit.

### Number Input Specifics

- Accepts only digits, `-`, `.` (for decimals).
- Up/Down arrow keys increment/decrement by `step` (default 1).
- Shift+Up/Down increments by 10x step.
- Renders small up/down arrow buttons on the right side of the input.
- Arrow buttons are 16x16px each, stacked vertically, visible on hover.
- Mouse wheel over the input increments/decrements (when focused).
- `min_value` and `max_value` clamp the value.

Number input arrow rendering:

```rust
// Up arrow: small triangle path
let up_path = Path::new()
    .move_to(center_x, top + 4.0)
    .line_to(center_x - 4.0, top + 10.0)
    .line_to(center_x + 4.0, top + 10.0)
    .close();

// Down arrow: inverted triangle
let down_path = Path::new()
    .move_to(center_x, bottom - 4.0)
    .line_to(center_x - 4.0, bottom - 10.0)
    .line_to(center_x + 4.0, bottom - 10.0)
    .close();
```

### Search Input Specifics

- Renders a magnifying glass icon on the left (12x12 SVG).
- Renders a clear (X) button on the right when text is non-empty.
- Clear button click sets value to empty string and fires `on_change("")`.
- Padding-left is increased to accommodate the search icon.

### Skia Rendering

The text input renders as a `DrawCommand` sequence:

1. `PushClip` with element bounds and corner_radius
2. `Rect` -- background fill (input background color, e.g. `rgba(30, 30, 30, 255)`)
3. `Rect` -- border (1px, color varies by state: default gray, focused blue, error red)
4. Selection highlight rect (if selection exists)
5. `Text` -- the display text (value, placeholder, or password mask), offset by `-scroll_offset`
6. IME preedit underline + highlight (if composing)
7. Cursor line (if focused and cursor visible)
8. `PopClip`
9. Focus ring (if focused, drawn outside the clip)
10. Error text below (if error exists)

New `DrawCommand` variants needed:

```rust
pub enum DrawCommand {
    // ... existing variants ...
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f32,
    },
    Path {
        commands: Vec<PathCommand>,
        fill: Option<Color>,
        stroke: Option<(Color, f32)>,
    },
}

pub enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    ArcTo(f32, f32, f32, f32, f32),
    Close,
}
```

### Rust API Design

Complete text input API:

```rust
// Create
let input = text_input("current value")
    .key("username")
    .placeholder("Enter username")
    .max_length(50)
    .width(300.0)
    .height(36.0)
    .font_size(14.0)
    .color(WHITE)
    .background(rgba(30, 30, 30, 255))
    .rounded(6.0)
    .border(rgba(80, 80, 80, 255), 1.0)
    .select_on_focus(true)
    .on_change(|new_val| { /* ... */ })
    .on_submit(|| { /* ... */ })  // Enter key
    .on_focus(|focused| { /* ... */ })
    .error(if invalid { Some("Invalid username") } else { None })
    .disabled(is_loading);

// Password
let pw = password_input("hunter2")
    .key("password")
    .placeholder("Enter password");

// Number
let num = number_input("42")
    .key("count")
    .min_value(0.0)
    .max_value(100.0)
    .step(1.0);

// Search
let search = search_input(&query)
    .key("search")
    .placeholder("Search...");
```

### Edge Cases

- **Empty string**: cursor at position 0, placeholder shown, no selection possible.
- **Unicode**: cursor moves by grapheme cluster (Rust `unicode-segmentation` crate). A single emoji or composed character is one cursor step.
- **RTL text**: not supported in v1. Text is always LTR.
- **Very long text**: horizontal scroll ensures cursor is always visible. Performance: only measure/render visible portion.
- **Paste with newlines**: for single-line input, replace `\n` and `\r` with space.
- **Max length enforcement**: reject input that would exceed `max_length`. Show remaining count optionally.
- **Concurrent mutations**: `on_change` fires with the new value; if the View doesn't update the Reactive, the input appears to reject the edit. This is the controlled-input pattern.
- **Focus + disabled toggle**: if an input is focused and becomes disabled, blur it immediately.
- **Tab in text input**: Tab should move focus to next element, NOT insert a tab character (unlike textarea).

---

## Textarea

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Textarea {
        value: String,
        placeholder: String,
    },
}
```

### Builder Function

```rust
pub fn textarea(value: &str) -> Element {
    Element {
        kind: ElementKind::Textarea {
            value: value.to_string(),
            placeholder: String::new(),
        },
        cursor: Some(CursorStyle::Text),
        width: Some(300.0),
        height: Some(120.0),
        ..Default::default()
    }
}
```

Additional chainable methods:

```rust
impl Element {
    pub fn rows(mut self, r: u32) -> Self { /* sets height = r * line_height + padding */ }
    pub fn cols(mut self, c: u32) -> Self { /* sets width = c * char_width + padding */ }
    pub fn wrap(mut self, w: TextWrap) -> Self { /* soft/hard/off */ }
    pub fn resize(mut self, r: TextareaResize) -> Self { /* none/vertical/horizontal/both */ }
    pub fn line_numbers(mut self, on: bool) -> Self { /* show line numbers */ }
    pub fn tab_size(mut self, n: u32) -> Self { /* spaces per tab */ }
    pub fn auto_resize(mut self, on: bool) -> Self { /* grow height with content */ }
}

#[derive(Debug, Clone, Copy)]
pub enum TextWrap {
    Soft,   // visual wrapping only, no newlines inserted
    Hard,   // insert newlines at wrap points (cols)
    Off,    // no wrapping, horizontal scroll
}

#[derive(Debug, Clone, Copy)]
pub enum TextareaResize {
    None,
    Vertical,
    Horizontal,
    Both,
}
```

### TextareaState

```rust
pub struct TextareaState {
    // Inherits all TextInputState fields plus:
    pub cursor_line: usize,           // current line number (0-indexed)
    pub cursor_column: usize,         // column within current line
    pub scroll_offset_y: f32,         // vertical scroll in pixels
    pub scroll_offset_x: f32,         // horizontal scroll (when wrap is Off)
    pub desired_column: usize,        // "sticky" column for up/down movement

    // Line cache
    pub line_starts: Vec<usize>,      // byte offset of each line start

    // All TextInputState fields:
    pub selection_anchor: Option<(usize, usize)>,  // (line, column)
    pub cursor_visible: bool,
    pub blink_timer_ms: f32,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub composing: bool,
    pub compose_text: String,
    pub compose_cursor_begin: i32,
    pub compose_cursor_end: i32,
    pub click_count: u32,
    pub last_click_time_ms: f32,
    pub mouse_selecting: bool,
}
```

### Cursor Movement (Textarea-specific)

| Action | Keys | Behavior |
|--------|------|----------|
| Move up | Up | Move to same column on previous line. Use `desired_column` for shorter lines. |
| Move down | Down | Move to same column on next line. |
| Move to line start | Home | Move to column 0 of current line. |
| Move to line end | End | Move to end of current line. |
| Move to text start | Ctrl+Home | Move to (0, 0). |
| Move to text end | Ctrl+End | Move to last line, last column. |
| Page up | PageUp | Move up by `visible_lines` count. |
| Page down | PageDown | Move down by `visible_lines` count. |
| Select up/down | Shift+Up/Down | Extend selection vertically. |
| Select page | Shift+PageUp/Down | Extend selection by page. |

**Desired column**: when moving up/down through lines of different lengths, the cursor should try to stay at the column it was on when vertical movement started. If a line is shorter, the cursor goes to the end, but the `desired_column` is preserved. Any horizontal movement or edit resets `desired_column`.

### Line Wrapping

**Soft wrap** (default):
- Split text into visual lines based on element width.
- Word-wrap: break at word boundaries (space, hyphen) that fit within the width.
- If a single word is wider than the element, break at character boundary.
- `line_starts` tracks the byte offset of each visual line.
- Wrapping is recalculated when the value or element width changes.

**Hard wrap**:
- Same as soft wrap, but `\n` characters are inserted into the value at wrap points when the user types.

**Off**:
- No wrapping. Horizontal scroll when lines exceed width.

### Vertical Scrolling

- Total content height = `line_count * line_height`.
- Visible height = element height - vertical padding.
- `scroll_offset_y` is clamped to `[0, max(0, total_height - visible_height)]`.
- Mouse wheel scrolls vertically (3 lines per tick).
- Cursor movement adjusts scroll to keep cursor visible (same logic as TextInput but vertical).
- Optional: render a scrollbar on the right side when content overflows.

### Scrollbar

- Thin track (6px wide) on the right edge, rounded.
- Thumb proportional to visible/total ratio.
- Track: `rgba(255, 255, 255, 10)`.
- Thumb: `rgba(255, 255, 255, 40)`, `rgba(255, 255, 255, 80)` on hover.
- Thumb is draggable for scroll.
- Auto-hide: fade out after 1 second of no scroll activity.

### Tab Handling

- **Tab key** in textarea inserts spaces (based on `tab_size`, default 4).
- **Shift+Tab** dedents the current line (remove up to `tab_size` leading spaces).
- **Tab with selection**: indent all selected lines.
- **Shift+Tab with selection**: dedent all selected lines.

### Auto-resize

When `auto_resize` is enabled:
- Element height grows with content (no vertical scrollbar).
- Minimum height is `rows * line_height` (default 3 rows).
- Maximum height can be capped with `.height()` as a max.
- On each render, calculate required height and request resize via `cx.set_surface_size()` if the textarea is the primary content.

### Line Numbers

When `line_numbers(true)`:
- Reserve a gutter on the left (width = `digit_count * char_width + 16px` padding).
- Draw line numbers right-aligned in the gutter, dimmed color.
- Gutter has a subtle border-right or different background.
- Line numbers follow scroll offset.

### Skia Rendering

1. `PushClip` with element bounds
2. `Rect` -- background
3. `Rect` -- border (state-dependent)
4. If line numbers: draw gutter background + line numbers
5. `PushClip` for text area (excluding gutter)
6. `PushTranslate` by `(-scroll_offset_x, -scroll_offset_y)`
7. For each visible line:
   a. Selection highlight rect (if line is partially or fully selected)
   b. `Text` -- line content
8. `PopTranslate`
9. Cursor line (positioned at cursor line/column, accounting for scroll)
10. `PopClip` (text area)
11. Scrollbar (if overflowing)
12. `PopClip` (element)
13. Focus ring
14. Error text

### API Design

```rust
let editor = textarea(&code)
    .key("code-editor")
    .rows(20)
    .cols(80)
    .font_size(13.0)
    .wrap(TextWrap::Off)
    .line_numbers(true)
    .tab_size(4)
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.code.set(val))
    });
```

### Edge Cases

- **Very large documents**: only render visible lines. Measure and cache line positions.
- **Empty textarea**: show placeholder, cursor at (0, 0).
- **Paste multiline text**: insert as-is (preserving newlines).
- **Select all + delete**: results in empty string.
- **Resize handle**: when `resize != None`, draw a drag handle in the bottom-right corner (diagonal lines icon). Drag changes element dimensions. Fire a separate `on_resize` callback.
- **Undo across multiline**: undo entries store the full value (for v1; diff-based undo is a future optimization).

---

## Select / Dropdown

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Select {
        value: Option<String>,          // currently selected value (None = no selection)
        options: Vec<SelectOption>,
        placeholder: String,
        multiple: bool,
        selected_values: Vec<String>,   // for multiple mode
        searchable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub group: Option<String>,  // optgroup label
    pub disabled: bool,
}
```

### Builder Function

```rust
pub fn select(value: Option<&str>) -> Element {
    Element {
        kind: ElementKind::Select {
            value: value.map(|s| s.to_string()),
            options: Vec::new(),
            placeholder: String::new(),
            multiple: false,
            selected_values: Vec::new(),
            searchable: false,
        },
        cursor: Some(CursorStyle::Pointer),
        width: Some(200.0),
        height: Some(36.0),
        ..Default::default()
    }
}

pub fn multi_select(values: &[&str]) -> Element {
    Element {
        kind: ElementKind::Select {
            value: None,
            options: Vec::new(),
            placeholder: String::new(),
            multiple: true,
            selected_values: values.iter().map(|s| s.to_string()).collect(),
            searchable: false,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

impl Element {
    pub fn options(mut self, opts: Vec<SelectOption>) -> Self { /* set options */ }
    pub fn option(mut self, value: &str, label: &str) -> Self { /* add single option */ }
    pub fn option_group(mut self, group: &str, opts: Vec<(&str, &str)>) -> Self { /* add group */ }
    pub fn searchable(mut self, on: bool) -> Self { /* enable filter-as-you-type */ }
    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self { /* for single */ }
    pub fn on_multi_change(mut self, f: impl Fn(Vec<String>) + 'static) -> Self { /* for multiple */ }
}
```

### SelectState

```rust
pub struct SelectState {
    pub open: bool,
    pub highlighted_index: usize,     // keyboard-highlighted option
    pub search_query: String,         // filter text (when searchable)
    pub scroll_offset: f32,           // scroll within dropdown
    pub type_ahead_buffer: String,    // for non-searchable: type-ahead to jump to option
    pub type_ahead_timer_ms: f32,     // clear buffer after 1 second
}
```

### Closed State Rendering

The select element when closed looks like:
- Rounded rect background (same as text input)
- Selected option label (or placeholder if no selection)
- Chevron-down icon on the right side (8x8 path: `V` shape)

```
+-----------------------------------+
| Selected option          [V]      |
+-----------------------------------+
```

### Dropdown Popup

When the select is clicked or receives Enter/Space while focused:

1. Open a popup surface (Wayland layer-shell, `Layer::Overlay`).
2. Position: directly below the select element (aligned to left edge).
3. Size: same width as select, height = `min(option_count * item_height, max_dropdown_height)`.
4. The popup uses the same render pipeline as described in `ui-popup-surfaces.md`.

Dropdown rendering:
- Background: `rgba(40, 40, 40, 255)` with 8px corner radius.
- Box shadow: `blur=12, spread=2, color=rgba(0,0,0,0.4)`.
- Each option: 36px height, padding-left 12px.
- Hovered option: `rgba(66, 133, 244, 30)` background.
- Selected option: checkmark icon on the right, bold text.
- Disabled option: 40% opacity, not hoverable.
- Group headers: smaller font, uppercase, dimmed, 8px top padding, not selectable.

```
+-----------------------------------+
| [search input if searchable]      |
+-----------------------------------+
| Option A                     [✓]  |
| Option B                          |
|--- Group Header ------------------|
| Option C                          |
| Option D (disabled)               |
+-----------------------------------+
```

### Keyboard Navigation (Dropdown Open)

| Key | Action |
|-----|--------|
| Down | Highlight next option (skip disabled, wrap to top) |
| Up | Highlight previous option (skip disabled, wrap to bottom) |
| Enter / Space | Select highlighted option, close dropdown |
| Escape | Close dropdown without changing selection |
| Home | Highlight first option |
| End | Highlight last option |
| PageUp | Move highlight up by 10 options |
| PageDown | Move highlight down by 10 options |
| Any printable char | Type-ahead: jump to first option starting with typed string |

### Keyboard Navigation (Dropdown Closed)

| Key | Action |
|-----|--------|
| Enter / Space / Down / Up | Open dropdown |
| Type-ahead chars | Open dropdown and filter (if searchable) or jump (if not) |

### Search/Filter (Searchable Mode)

When `searchable` is true:
- The dropdown includes a text input at the top.
- As the user types, options are filtered (case-insensitive substring match).
- The filter text input auto-focuses when the dropdown opens.
- Pressing Down from the search input moves highlight to the first visible option.
- Pressing Escape clears the search first; pressing Escape again closes the dropdown.

### Multi-Select

When `multiple` is true:
- Each option has a checkbox on the left.
- Clicking an option toggles its selection (does not close the dropdown).
- The closed display shows comma-separated selected labels, or "N selected" if too many.
- The clear (X) button clears all selections.
- Keyboard Enter toggles the highlighted option's selection.

### Skia Rendering (Closed)

1. `Rect` -- background
2. `Rect` -- border
3. `Text` -- selected label or placeholder
4. `Path` -- chevron-down icon
5. Focus ring (if focused)

### Skia Rendering (Dropdown Popup)

On a separate popup surface:

1. `BoxShadow` -- drop shadow
2. `Rect` -- dropdown background
3. `PushClip` -- dropdown bounds
4. For each visible option:
   a. `Rect` -- option highlight (if hovered/highlighted)
   b. `Text` -- option label
   c. `Path` -- checkmark (if selected)
   d. `Rect` -- checkbox (if multi-select)
5. Group headers: `Text` with separator line
6. `PopClip`
7. Scrollbar (if options overflow)

### API Design

```rust
let country = select(self.country.get(cx).as_deref())
    .key("country")
    .placeholder("Select country...")
    .searchable(true)
    .options(vec![
        SelectOption { value: "us".into(), label: "United States".into(), group: Some("Americas".into()), disabled: false },
        SelectOption { value: "ca".into(), label: "Canada".into(), group: Some("Americas".into()), disabled: false },
        SelectOption { value: "uk".into(), label: "United Kingdom".into(), group: Some("Europe".into()), disabled: false },
    ])
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.country.set(Some(val)))
    });
```

### Edge Cases

- **Dropdown positioning**: if there is not enough space below the select, open above instead. Check available space using surface dimensions.
- **Long option labels**: ellipsize with `...` at the element width.
- **Empty options list**: show "No options" placeholder in dropdown.
- **Dropdown + scroll**: if the select is in a scrollable container, the dropdown should still position correctly (absolute positioning relative to surface).
- **Rapid open/close**: debounce to prevent flicker.
- **Click outside dropdown**: close the dropdown (handled by popup surface focus-loss).

---

## Checkbox

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Checkbox {
        checked: bool,
        indeterminate: bool,
        label: Option<String>,
    },
}
```

### Builder Function

```rust
pub fn checkbox(checked: bool) -> Element {
    Element {
        kind: ElementKind::Checkbox {
            checked,
            indeterminate: false,
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

impl Element {
    pub fn indeterminate(mut self, i: bool) -> Self { /* set indeterminate */ }
    pub fn label(mut self, text: &str) -> Self { /* set label for checkbox/radio */ }
    pub fn on_change(mut self, f: impl Fn(bool) + 'static) -> Self { /* toggle callback */ }
}
```

### Visual Design

The checkbox is a 18x18px box with optional label text to the right.

**States**:

| State | Box | Content |
|-------|-----|---------|
| Unchecked | 1px gray border, transparent fill | Empty |
| Checked | Filled blue (`rgba(66, 133, 244, 255)`), rounded 3px | White checkmark path |
| Indeterminate | Filled blue, rounded 3px | White horizontal dash (10x2px) |
| Disabled unchecked | 1px dark-gray border, `rgba(60, 60, 60, 255)` fill | Empty |
| Disabled checked | Dim blue fill `rgba(66, 133, 244, 100)` | Dim white checkmark |
| Hovered | Brighter border / fill | Same content |
| Focused | Focus ring around box | Same content |

### Checkmark Path

```rust
// Checkmark: two connected line segments
let checkmark = Path::new()
    .move_to(box_x + 4.0, box_y + 9.0)
    .line_to(box_x + 7.5, box_y + 12.5)
    .line_to(box_x + 13.0, box_y + 5.5);
// Stroke: white, 2px, round cap
```

### Indeterminate Dash

```rust
// Horizontal dash centered in box
let dash = Rect::from_xywh(box_x + 4.0, box_y + 8.0, 10.0, 2.0);
// Fill: white
```

### Layout

- Box: 18x18px, fixed.
- Label: right of box, 8px gap.
- Total element: row layout, align-items center.
- Clicking anywhere on the element (box or label) toggles the checkbox.

### Interaction

| Action | Effect |
|--------|--------|
| Click | Toggle: `on_change(!checked)`. Indeterminate becomes checked. |
| Space (focused) | Same as click. |
| Tab | Move focus to next element. |

### Animation

- Check/uncheck: the checkmark draws in with a brief animation (100ms ease-out, path from 0% to 100% stroke length). Uses Skia `PathEffect::dash` with animated interval.
- Background fill fades in (100ms).

### Skia Rendering

1. `Rect` -- checkbox box with rounded corners
2. `Path` -- checkmark or dash (if checked/indeterminate)
3. `Text` -- label (right of box)
4. Focus ring around box only (not label)

### API Design

```rust
let agree = checkbox(*self.agreed.get(cx))
    .key("agree")
    .label("I agree to the terms")
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.agreed.set(val))
    });

// "Select all" with indeterminate
let select_all = checkbox(all_selected)
    .indeterminate(some_but_not_all_selected)
    .label("Select all")
    .on_change(|val| { /* toggle all items */ });
```

### Edge Cases

- **Indeterminate + click**: transitions to checked (not unchecked). Indeterminate is a visual state, not a third boolean value.
- **Label click**: the entire row is clickable. Hit test includes the label.
- **Disabled**: no toggle, no hover effects, dimmed appearance.
- **Fast double-click**: should toggle twice (check then uncheck).

---

## Radio Button

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Radio {
        selected: bool,
        group: String,         // radio group name
        value: String,         // this radio's value
        label: Option<String>,
    },
}
```

### Builder Function

```rust
pub fn radio(selected: bool, group: &str, value: &str) -> Element {
    Element {
        kind: ElementKind::Radio {
            selected,
            group: group.to_string(),
            value: value.to_string(),
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}
```

### Visual Design

18x18px circle.

| State | Outer Circle | Inner Circle |
|-------|-------------|--------------|
| Unselected | 1px gray border, transparent fill | None |
| Selected | 2px blue border, transparent fill | 8px blue filled circle centered |
| Disabled unselected | 1px dark-gray border, dim fill | None |
| Disabled selected | 2px dim-blue border | 8px dim-blue circle |
| Hovered | Brighter border | Same |
| Focused | Focus ring | Same |

### Rendering

```rust
// Outer circle
canvas.draw_circle((cx, cy), 9.0, &border_paint);

// Inner circle (selected)
if selected {
    let mut fill = Paint::default();
    fill.set_color(BLUE);
    fill.set_anti_alias(true);
    canvas.draw_circle((cx, cy), 4.0, &fill);
}
```

### Layout

Same as checkbox: circle + label in a row with 8px gap.

### Interaction

| Action | Effect |
|--------|--------|
| Click | Fire `on_change(this_radio_value)`. View updates which radio is selected. |
| Space (focused) | Same as click. |
| Up/Left (focused) | Move to previous radio in group and select it. |
| Down/Right (focused) | Move to next radio in group and select it. |

**Radio group behavior**: The framework does NOT enforce mutual exclusion. The View is responsible for setting `selected` on exactly one radio per group. Arrow keys within a group are navigated by finding adjacent elements with the same `group` value.

### Animation

- Inner circle scales from 0 to 1 on selection (100ms spring).
- Previous selection's inner circle scales from 1 to 0.

### API Design

```rust
let size_options = column().gap(8.0)
    .child(radio(*self.size.get(cx) == "sm", "size", "sm")
        .label("Small")
        .on_change({
            let h = cx.handle::<Self>();
            move |val| h.update(move |v| v.size.set(val))
        }))
    .child(radio(*self.size.get(cx) == "md", "size", "md")
        .label("Medium")
        .on_change({
            let h = cx.handle::<Self>();
            move |val| h.update(move |v| v.size.set(val))
        }))
    .child(radio(*self.size.get(cx) == "lg", "size", "lg")
        .label("Large")
        .on_change({
            let h = cx.handle::<Self>();
            move |val| h.update(move |v| v.size.set(val))
        }));
```

### Edge Cases

- **No selection**: all radios in a group can be unselected initially. First Tab into the group focuses the first radio.
- **Single radio**: works the same as a checkbox visually but semantically different.
- **Arrow key wrapping**: Down from last radio wraps to first radio in the group.

---

## Toggle / Switch

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Switch {
        on: bool,
        label: Option<String>,
    },
}
```

### Builder Function

```rust
pub fn switch(on: bool) -> Element {
    Element {
        kind: ElementKind::Switch {
            on,
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}
```

### Visual Design

A pill-shaped track (44x24px) with a circular thumb (20x20px).

| State | Track | Thumb Position |
|-------|-------|----------------|
| Off | `rgba(80, 80, 80, 255)` | Left (x=2) |
| On | `rgba(66, 133, 244, 255)` (blue) | Right (x=22) |
| Disabled off | `rgba(50, 50, 50, 255)` | Left, dimmed |
| Disabled on | `rgba(66, 133, 244, 100)` | Right, dimmed |
| Hovered | Slightly brighter track | Same |
| Active (pressed) | Same | Thumb slightly wider (22x20px squish) |

### Animation

- Thumb slides from left to right (or right to left) with a spring animation (snappy, ~150ms).
- Track color interpolates between off and on colors during the animation.

### Rendering

```rust
// Track (pill shape)
let track_rect = RRect::new_rect_xy(
    Rect::from_xywh(x, y, 44.0, 24.0),
    12.0, 12.0,  // fully rounded ends
);
canvas.draw_rrect(track_rect, &track_paint);

// Thumb (circle with slight shadow)
let thumb_x = if on { x + 22.0 } else { x + 2.0 };  // animated
let thumb_center = (thumb_x + 10.0, y + 12.0);
// Shadow
let mut shadow = Paint::default();
shadow.set_color(rgba(0, 0, 0, 60));
shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 2.0, false));
canvas.draw_circle(thumb_center, 10.0, &shadow);
// Thumb
canvas.draw_circle(thumb_center, 10.0, &white_paint);
```

### Layout

- Track: 44x24px fixed.
- Label: right of track, 8px gap.
- Total element: row, align-items center.

### Interaction

| Action | Effect |
|--------|--------|
| Click (on track or label) | Toggle: `on_change(!on)` |
| Space (focused) | Toggle |
| Drag thumb | Drag to toggle; if released past midpoint, toggle. |

### API Design

```rust
let dark_mode = switch(*self.dark_mode.get(cx))
    .key("dark-mode")
    .label("Dark mode")
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.dark_mode.set(val))
    });
```

---

## Slider / Range

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
        orientation: SliderOrientation,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}
```

### Builder Function

```rust
pub fn slider(value: f64, min: f64, max: f64) -> Element {
    Element {
        kind: ElementKind::Slider {
            value,
            min,
            max,
            step: None,
            orientation: SliderOrientation::Horizontal,
        },
        cursor: Some(CursorStyle::Pointer),
        height: Some(24.0),
        ..Default::default()
    }
}

impl Element {
    pub fn step(mut self, s: f64) -> Self { /* set step for snapping */ }
    pub fn vertical(mut self) -> Self { /* set orientation to Vertical, swap width/height */ }
    pub fn tick_marks(mut self, ticks: Vec<f64>) -> Self { /* show tick marks at these values */ }
    pub fn on_change(mut self, f: impl Fn(f64) + 'static) -> Self { /* value changed */ }
    pub fn show_value(mut self, on: bool) -> Self { /* show value tooltip while dragging */ }
}
```

### Visual Design

**Horizontal slider** (default):

```
         [===thumb===]
|----[===|===]-------------------|
track-fill  track-empty
```

- Track: 4px tall, full width, rounded ends, centered vertically.
- Track filled portion (left of thumb): `rgba(66, 133, 244, 255)` (blue).
- Track empty portion (right of thumb): `rgba(80, 80, 80, 255)` (gray).
- Thumb: 16px diameter circle, white with subtle shadow.
- Total element height: 24px (track centered, thumb centered on track).

**Vertical slider**: same but rotated 90 degrees. Track is 4px wide, full height. Thumb moves vertically. Value increases upward.

### Tick Marks

When tick marks are specified:
- Small vertical lines (1px wide, 8px tall) below the track at each tick value's position.
- Color: `rgba(150, 150, 150, 255)`.
- Tick marks at min and max are always rendered.

### Value Tooltip

When `show_value` is true and the user is dragging:
- A small rounded rect above the thumb showing the current value.
- Background: `rgba(30, 30, 30, 240)`, text: white, font_size: 11.
- Positioned centered above the thumb, 8px gap.
- Fades in/out with 100ms animation.

### Interaction

| Action | Effect |
|--------|--------|
| Click on track | Jump thumb to click position. Fire `on_change`. |
| Drag thumb | Move thumb with pointer. Fire `on_change` continuously. |
| Left/Down arrow (focused) | Decrease by step (or by `(max-min)/100` if no step). |
| Right/Up arrow (focused) | Increase by step. |
| Home (focused) | Set to min. |
| End (focused) | Set to max. |
| PageDown (focused) | Decrease by 10 steps. |
| PageUp (focused) | Increase by 10 steps. |

**Step snapping**: when `step` is set, the value snaps to the nearest multiple of step from min. During drag, the thumb snaps visually and the reported value is quantized.

**Value from position**:

```rust
fn value_from_x(x: f32, track_start: f32, track_width: f32, min: f64, max: f64, step: Option<f64>) -> f64 {
    let ratio = ((x - track_start) / track_width).clamp(0.0, 1.0) as f64;
    let raw = min + ratio * (max - min);
    if let Some(s) = step {
        (((raw - min) / s).round() * s + min).clamp(min, max)
    } else {
        raw
    }
}
```

### Range Slider (Two Thumbs)

For selecting a range:

```rust
pub fn range_slider(low: f64, high: f64, min: f64, max: f64) -> Element {
    Element {
        kind: ElementKind::RangeSlider {
            low, high, min, max,
            step: None,
            orientation: SliderOrientation::Horizontal,
        },
        ..Default::default()
    }
}

pub enum ElementKind {
    RangeSlider {
        low: f64,
        high: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
        orientation: SliderOrientation,
    },
}
```

- Two thumbs, filled track between them.
- Dragging a thumb moves only that thumb.
- Thumbs cannot cross each other (low <= high).
- Callback: `on_range_change(low, high)`.
- Clicking the track moves the nearest thumb to the click position.

### Rendering

```rust
fn draw_slider(canvas, bounds, value, min, max) {
    let track_y = bounds.y + bounds.height / 2.0;
    let track_height = 4.0;
    let thumb_radius = 8.0;

    let ratio = ((value - min) / (max - min)) as f32;
    let thumb_x = bounds.x + ratio * bounds.width;

    // Track (empty, full width)
    let track_rect = RRect::new_rect_xy(
        Rect::from_xywh(bounds.x, track_y - 2.0, bounds.width, track_height),
        2.0, 2.0,
    );
    canvas.draw_rrect(track_rect, &track_empty_paint);

    // Track (filled, left of thumb)
    let filled_rect = RRect::new_rect_xy(
        Rect::from_xywh(bounds.x, track_y - 2.0, thumb_x - bounds.x, track_height),
        2.0, 2.0,
    );
    canvas.draw_rrect(filled_rect, &track_filled_paint);

    // Thumb shadow
    canvas.draw_circle((thumb_x, track_y), thumb_radius + 1.0, &shadow_paint);

    // Thumb
    canvas.draw_circle((thumb_x, track_y), thumb_radius, &thumb_paint);
}
```

### API Design

```rust
let volume = slider(*self.volume.get(cx), 0.0, 100.0)
    .key("volume")
    .step(1.0)
    .show_value(true)
    .on_change({
        let h = cx.handle::<Self>();
        move |val| h.update(move |v| v.volume.set(val))
    });

let price_range = range_slider(
    *self.price_min.get(cx),
    *self.price_max.get(cx),
    0.0, 1000.0,
)
    .key("price")
    .step(10.0)
    .tick_marks(vec![0.0, 250.0, 500.0, 750.0, 1000.0])
    .on_range_change({
        let h = cx.handle::<Self>();
        move |low, high| h.update(move |v| {
            v.price_min.set(low);
            v.price_max.set(high);
        })
    });
```

### Edge Cases

- **min == max**: thumb centered, not interactive.
- **step larger than range**: single position (min).
- **Floating point precision**: snap to step with `round()`, not `floor()`.
- **Touch/drag beyond track ends**: clamp to min/max.
- **Vertical slider in horizontal layout**: requires explicit width/height swap.

---

## Button

Buttons are already achievable with the existing `container().on_click()` pattern, but a dedicated `ElementKind` provides proper semantics, keyboard interaction, and default styling.

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Button {
        label: String,
        variant: ButtonVariant,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ButtonVariant {
    Primary,    // filled blue background
    Secondary,  // outlined, no fill
    Ghost,      // no background, no border, just text
    Danger,     // filled red background
}
```

### Builder Function

```rust
pub fn button(label: &str) -> Element {
    Element {
        kind: ElementKind::Button {
            label: label.to_string(),
            variant: ButtonVariant::Primary,
        },
        cursor: Some(CursorStyle::Pointer),
        padding: [8.0, 16.0, 8.0, 16.0],
        corner_radius: 6.0,
        ..Default::default()
    }
}

impl Element {
    pub fn variant(mut self, v: ButtonVariant) -> Self { /* set variant */ }
    pub fn icon(mut self, svg: &str) -> Self { /* leading icon */ }
    pub fn loading(mut self, on: bool) -> Self { /* show spinner, disable interaction */ }
}
```

### Visual Design

| Variant | Default | Hovered | Active | Disabled |
|---------|---------|---------|--------|----------|
| Primary | Blue bg, white text | Lighter blue | Darker blue | 40% opacity |
| Secondary | Transparent bg, blue border, blue text | Light blue tint bg | Darker tint | 40% opacity |
| Ghost | Transparent bg, white text | `rgba(255,255,255,10)` bg | `rgba(255,255,255,20)` bg | 40% opacity |
| Danger | Red bg, white text | Lighter red | Darker red | 40% opacity |

Colors:
- Primary bg: `rgba(66, 133, 244, 255)`, hover: `rgba(85, 148, 255, 255)`, active: `rgba(50, 115, 220, 255)`
- Danger bg: `rgba(220, 53, 69, 255)`, hover: `rgba(235, 70, 85, 255)`, active: `rgba(200, 40, 55, 255)`

### Loading State

When `loading(true)`:
- Disable interaction (like disabled but visually active).
- Show a spinning circle animation replacing (or next to) the label.
- Spinner: 16x16 arc path, rotating 360 degrees every 1 second.

### Interaction

| Action | Effect |
|--------|--------|
| Click | Fire `on_click` |
| Enter (focused) | Fire `on_click` |
| Space (focused) | Fire `on_click` on key-up (like HTML buttons) |
| Tab | Move focus |

### Rendering

1. `Rect` -- background (variant-dependent color, state-dependent shade)
2. `Rect` -- border (if Secondary variant)
3. `Image` -- leading icon (if set)
4. `Text` -- label, centered
5. Focus ring (if focused)

### API Design

```rust
let submit = button("Submit")
    .key("submit")
    .variant(ButtonVariant::Primary)
    .on_click({
        let h = cx.handle::<Self>();
        move || h.update(|v| v.submit())
    })
    .loading(*self.submitting.get(cx))
    .disabled(*self.submitting.get(cx));

let cancel = button("Cancel")
    .variant(ButtonVariant::Ghost)
    .on_click(|| { /* ... */ });

let delete = button("Delete")
    .variant(ButtonVariant::Danger)
    .icon(TRASH_ICON_SVG)
    .on_click(|| { /* ... */ });
```

---

## Progress Bar

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    Progress {
        value: Option<f64>,   // None = indeterminate, Some(0.0..=1.0) = determinate
        variant: ProgressVariant,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ProgressVariant {
    Bar,
    Circular,
}
```

### Builder Function

```rust
pub fn progress(value: f64) -> Element {
    Element {
        kind: ElementKind::Progress {
            value: Some(value.clamp(0.0, 1.0)),
            variant: ProgressVariant::Bar,
        },
        height: Some(6.0),
        ..Default::default()
    }
}

pub fn progress_indeterminate() -> Element {
    Element {
        kind: ElementKind::Progress {
            value: None,
            variant: ProgressVariant::Bar,
        },
        height: Some(6.0),
        ..Default::default()
    }
}

pub fn spinner() -> Element {
    Element {
        kind: ElementKind::Progress {
            value: None,
            variant: ProgressVariant::Circular,
        },
        width: Some(24.0),
        height: Some(24.0),
        ..Default::default()
    }
}

impl Element {
    pub fn progress_color(mut self, c: Color) -> Self { /* fill color */ }
    pub fn track_color(mut self, c: Color) -> Self { /* track color */ }
}
```

### Visual Design

**Determinate bar**:
- Track: full width, 6px tall, rounded ends, `rgba(80, 80, 80, 255)`.
- Fill: left portion (width = `value * total_width`), blue, rounded ends.
- Animated: fill width transitions smoothly (spring animation on width change).

**Indeterminate bar**:
- Track: same as above.
- Fill: a shorter segment (30% width) that slides back and forth continuously.
- Animation: ease-in-out, 2 second cycle, left-to-right then right-to-left.

**Circular (spinner)**:
- Arc path: 270-degree arc, 2px stroke, blue.
- Rotates 360 degrees per second.
- Track: full circle, 2px stroke, `rgba(80, 80, 80, 255)`.

### Rendering

```rust
// Determinate bar
fn draw_progress_bar(canvas, bounds, value: f64) {
    let radius = bounds.height / 2.0;

    // Track
    let track = RRect::new_rect_xy(
        Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height),
        radius, radius,
    );
    canvas.draw_rrect(track, &track_paint);

    // Fill
    let fill_width = (value as f32 * bounds.width).max(bounds.height); // min width = height for rounded
    let fill = RRect::new_rect_xy(
        Rect::from_xywh(bounds.x, bounds.y, fill_width, bounds.height),
        radius, radius,
    );
    canvas.draw_rrect(fill, &fill_paint);
}

// Spinner
fn draw_spinner(canvas, center, radius, rotation_angle) {
    // Track circle
    canvas.draw_circle(center, radius, &track_paint);  // stroke

    // Arc
    let mut path = Path::new();
    path.add_arc(
        Rect::from_xywh(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0),
        rotation_angle - 135.0,
        270.0,
    );
    canvas.draw_path(&path, &arc_paint);  // stroke
}
```

### API Design

```rust
let upload_progress = progress(*self.upload_pct.get(cx))
    .fill_width()
    .progress_color(rgba(76, 175, 80, 255));  // green

let loading = progress_indeterminate()
    .fill_width();

let save_spinner = spinner()
    .width(16.0)
    .height(16.0)
    .progress_color(WHITE);
```

### Edge Cases

- **Value > 1.0 or < 0.0**: clamp to [0.0, 1.0].
- **Value transitions**: animate smoothly between values using spring animation.
- **Zero width element**: skip rendering.

---

## Color Picker

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    ColorPicker {
        value: Color,
    },
}
```

### Builder Function

```rust
pub fn color_picker(value: Color) -> Element {
    Element {
        kind: ElementKind::ColorPicker { value },
        cursor: Some(CursorStyle::Pointer),
        width: Some(36.0),
        height: Some(36.0),
        ..Default::default()
    }
}

impl Element {
    pub fn on_change(mut self, f: impl Fn(Color) + 'static) -> Self { /* color changed */ }
    pub fn show_alpha(mut self, on: bool) -> Self { /* include alpha slider */ }
}
```

### Visual Design

**Closed state**: a 36x36px rounded square showing the current color. Bordered for contrast against dark backgrounds (especially for near-black colors).

**Open state (popup)**:
A popup surface containing:

```
+----------------------------------+
|  [Saturation/Value square]       |
|  250x200px                       |
|  x-axis: saturation 0-100%      |
|  y-axis: value 100%-0%          |
|  Hue determines base color      |
|                                  |
+----------------------------------+
|  [Hue slider]                    |
|  Full width, 12px tall           |
|  Rainbow gradient left to right  |
+----------------------------------+
|  [Alpha slider] (if show_alpha)  |
|  Full width, 12px tall           |
|  Checkerboard to transparent     |
+----------------------------------+
|  [Hex input]  [Preview swatch]   |
|  #RRGGBB      current color      |
+----------------------------------+
```

### HSV Color Space

Internally convert between RGB and HSV for the picker:

```rust
struct Hsv {
    h: f32,  // 0-360
    s: f32,  // 0-1
    v: f32,  // 0-1
}

fn rgb_to_hsv(c: Color) -> Hsv { /* standard conversion */ }
fn hsv_to_rgb(hsv: Hsv) -> Color { /* standard conversion */ }
```

### Saturation/Value Square

- Width: saturation (0 at left, 1 at right).
- Height: value (1 at top, 0 at bottom).
- Background: horizontal gradient from white to the full-saturation hue color, then a vertical gradient from transparent to black overlaid on top.
- Rendered with two overlapping gradients in Skia:

```rust
// Horizontal: white to hue color
let h_shader = Shader::linear_gradient(
    (left, top), (right, top),
    &[WHITE, hue_color],
    None, TileMode::Clamp, None,
);

// Vertical: transparent to black
let v_shader = Shader::linear_gradient(
    (left, top), (left, bottom),
    &[TRANSPARENT, BLACK],
    None, TileMode::Clamp, None,
);
```

- Draw h_shader first, then v_shader on top (multiply blend or just sequential).
- Selection indicator: small circle (8px) with white border at the (s, v) position.

### Hue Slider

- Horizontal gradient through all hues (0-360).
- Rendered as a linear gradient with 7 stops: red, yellow, green, cyan, blue, magenta, red.
- Thumb: small vertical line or circle indicating current hue.

### Hex Input

- A small text input accepting `#RRGGBB` or `#RRGGBBAA` format.
- Typing a valid hex color updates the picker immediately.
- Invalid input shows error state.

### Interaction

| Action | Effect |
|--------|--------|
| Click swatch | Open popup |
| Drag in SV square | Update saturation and value |
| Drag on hue slider | Update hue |
| Drag on alpha slider | Update alpha |
| Type in hex input | Update all channels |
| Click outside popup | Close popup |
| Escape | Close popup |

### API Design

```rust
let bg_color = color_picker(*self.bg_color.get(cx))
    .key("bg-color")
    .show_alpha(true)
    .on_change({
        let h = cx.handle::<Self>();
        move |color| h.update(move |v| v.bg_color.set(color))
    });
```

---

## Date/Time Picker

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    DatePicker {
        value: Option<NaiveDate>,     // from chrono crate
        variant: DatePickerVariant,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DatePickerVariant {
    Date,           // date only
    Time,           // time only
    DateTime,       // date + time
}
```

### Builder Function

```rust
pub fn date_picker(value: Option<NaiveDate>) -> Element { /* Date variant */ }
pub fn time_picker(value: Option<NaiveTime>) -> Element { /* Time variant */ }
pub fn datetime_picker(value: Option<NaiveDateTime>) -> Element { /* DateTime variant */ }

impl Element {
    pub fn min_date(mut self, d: NaiveDate) -> Self { /* earliest selectable */ }
    pub fn max_date(mut self, d: NaiveDate) -> Self { /* latest selectable */ }
    pub fn on_change(mut self, f: impl Fn(Option<NaiveDate>) + 'static) -> Self { /* ... */ }
}
```

### Closed State

Looks like a text input displaying the formatted date/time:
- Date: `YYYY-MM-DD`
- Time: `HH:MM`
- DateTime: `YYYY-MM-DD HH:MM`
- Calendar icon on the right (for date), clock icon (for time).

### Date Picker Popup

```
+--------------------------------+
|  [<]   March 2026   [>]        |
+--------------------------------+
|  Mo  Tu  We  Th  Fr  Sa  Su   |
|                          1     |
|   2   3   4   5   6   7   8   |
|   9  10  11 [12] 13  14  15   |
|  16  17  18  19  20  21  22   |
|  23  24  25  26  27  28  29   |
|  30  31                        |
+--------------------------------+
```

- Header: month/year with left/right navigation arrows.
- Click month/year text to switch to month picker view, then year picker view.
- Grid: 7 columns (days of week), 5-6 rows.
- Today: blue circle outline.
- Selected date: blue filled circle.
- Dates outside valid range (min_date/max_date): dimmed, not clickable.
- Dates from adjacent months: lighter text color.

### Time Picker Popup

Two scroll-wheel columns or input fields:
- Hours (00-23 or 1-12 with AM/PM)
- Minutes (00-59)
- Optional seconds

Alternatively, a simpler approach: two number inputs side by side with up/down arrows.

### Interaction

| Action | Effect |
|--------|--------|
| Click input | Open popup |
| Click day | Select date, close popup |
| Click `<`/`>` | Navigate month |
| Arrow keys in calendar | Move selection through days |
| Enter | Confirm selection |
| Escape | Close without changing |
| Type in input | Direct text entry (parse on blur) |

### Rendering

Calendar grid rendering:
- Each cell is `(popup_width / 7)` wide, 36px tall.
- Day number centered in cell.
- Selection: blue circle behind number.
- Today: blue circle outline.
- Weekday headers: smaller font, dimmed.

### API Design

```rust
let birthday = date_picker(self.birthday.get(cx).clone())
    .key("birthday")
    .min_date(NaiveDate::from_ymd(1900, 1, 1))
    .max_date(chrono::Local::now().date_naive())
    .on_change({
        let h = cx.handle::<Self>();
        move |date| h.update(move |v| v.birthday.set(date))
    });
```

### Edge Cases

- **Locale**: default to ISO format. Locale-aware formatting is a future enhancement.
- **Invalid typed dates**: parse on blur; if invalid, revert to previous value and show error.
- **Feb 29**: handle leap years.
- **Timezone**: `NaiveDate`/`NaiveTime` are timezone-unaware. Application handles timezone.

---

## File Input

### ElementKind

```rust
pub enum ElementKind {
    // ... existing variants ...
    FileInput {
        file_name: Option<String>,  // selected file name for display
        accept: Option<String>,     // MIME type filter, e.g. "image/*"
        multiple: bool,
    },
}
```

### Builder Function

```rust
pub fn file_input() -> Element {
    Element {
        kind: ElementKind::FileInput {
            file_name: None,
            accept: None,
            multiple: false,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

impl Element {
    pub fn accept(mut self, mime: &str) -> Self { /* filter file types */ }
    pub fn multiple(mut self, m: bool) -> Self { /* allow multiple files */ }
    pub fn on_file_select(mut self, f: impl Fn(Vec<String>) + 'static) -> Self { /* paths */ }
}
```

### Visual Design

```
+------------------------------------------+
|  [Choose File]   No file selected         |
+------------------------------------------+
```

- A button ("Choose File") on the left.
- File name (or "No file selected") as text on the right.
- When files are selected: comma-separated names, or "N files selected".

### File Dialog Integration

On click, open the OS file dialog. On Wayland/Linux, use one of:
1. **xdg-desktop-portal** via D-Bus `org.freedesktop.portal.FileChooser.OpenFile` -- the preferred method, works with sandboxed apps.
2. **kdialog** or **zenity** as a subprocess fallback.

Implementation:

```rust
fn open_file_dialog(accept: Option<&str>, multiple: bool) -> Vec<String> {
    // Try xdg-desktop-portal first (async D-Bus call)
    // Fallback to kdialog --getopenfilename
    let mut cmd = Command::new("kdialog");
    cmd.arg(if multiple { "--getopenfilename" } else { "--getopenfilename" });
    cmd.arg(".");
    if let Some(filter) = accept {
        cmd.arg(filter);
    }
    if multiple {
        cmd.arg("--multiple");
        cmd.arg("--separate-output");
    }
    // Parse stdout as file paths
}
```

The file dialog runs asynchronously. The result is delivered via the event loop (callback fires on next frame after dialog closes).

### API Design

```rust
let avatar = file_input()
    .key("avatar")
    .accept("image/*")
    .on_file_select({
        let h = cx.handle::<Self>();
        move |paths| h.update(move |v| v.avatar_path.set(paths.first().cloned()))
    });
```

### Edge Cases

- **Dialog blocks**: the file dialog is a separate process/portal, so it doesn't block the Wayland event loop. The UI continues to render.
- **Cancelled dialog**: callback is not fired (or fired with empty vec).
- **Large file names**: ellipsize display.

---

## Implementation Order

Priority is based on dependency chain and utility for the existing apps (dock, header, spotlight).

### Phase 1: Focus Management and Keyboard Events

**Prerequisites for all interactive form elements.**

1. Add `focused_key` tracking to `EventState`.
2. Implement keyboard event dispatch (KeyDown/KeyUp/TextInput) to the focused element.
3. Implement Tab/Shift+Tab focus traversal.
4. Add focus ring rendering.
5. Add `disabled`, `read_only`, `tab_index` to `Element`.

Files changed:
- `ui/src/element.rs` -- new fields
- `ui/src/event_dispatch.rs` -- focus tracking, keyboard dispatch
- `ui/src/input.rs` -- ensure Key constants (Tab, Escape, etc.)
- `ui/src/display_list.rs` -- focus ring draw commands
- `ui/src/renderer.rs` -- render focus ring

### Phase 2: Enhanced Text Input

**The hardest single element. Foundation for Textarea and Search.**

1. Upgrade `TextInputState` with selection, undo/redo, word movement.
2. Add `TextInputVariant` enum.
3. Implement mouse click-to-place-cursor (character index from x-coordinate).
4. Implement mouse selection (click+drag, double-click, triple-click).
5. Implement keyboard selection (Shift+arrows, Ctrl+Shift+arrows, Ctrl+A).
6. Implement clipboard read/write via Wayland `wl_data_device`.
7. Implement selection highlight rendering.
8. Implement horizontal text scrolling within fixed-width input.
9. Implement password masking.
10. Implement number input variant (up/down arrows, step).
11. Implement search input variant (search icon, clear button).

Files changed:
- `ui/src/text_input_state.rs` -- major rewrite
- `ui/src/element.rs` -- `TextInputVariant`, new fields
- `ui/src/display_list.rs` -- selection highlight, cursor, new draw commands (Line, Path)
- `ui/src/renderer.rs` -- render Line, Path, selection
- `ui/src/event_dispatch.rs` -- text input keyboard/mouse handling
- `ui/src/wayland.rs` -- clipboard (wl_data_device binding)

### Phase 3: Button, Checkbox, Radio, Switch

**Simple elements with clear visual specs.**

1. Button element with variants.
2. Checkbox with check/indeterminate rendering.
3. Radio button with circle rendering.
4. Switch/toggle with thumb animation.

Files changed:
- `ui/src/element.rs` -- new `ElementKind` variants
- `ui/src/display_list.rs` -- rendering for each
- `ui/src/renderer.rs` -- Path rendering (checkmark, circles)
- `ui/src/layout.rs` -- layout for checkbox+label, radio+label, switch+label
- `ui/src/event_dispatch.rs` -- click/space handlers for each

### Phase 4: Slider

**Requires drag interaction which already exists.**

1. Slider element with track + thumb rendering.
2. Drag interaction for thumb.
3. Click-on-track to jump.
4. Step snapping.
5. Range slider variant.
6. Tick marks.
7. Value tooltip.

Files changed:
- `ui/src/element.rs` -- `Slider`, `RangeSlider` kinds
- `ui/src/display_list.rs` -- track/thumb/tick rendering
- `ui/src/renderer.rs` -- gradient rendering for tracks
- `ui/src/event_dispatch.rs` -- slider drag handling

### Phase 5: Progress Bar and Spinner

**No interaction, rendering only.**

1. Determinate progress bar.
2. Indeterminate progress bar animation.
3. Circular spinner animation.

Files changed:
- `ui/src/element.rs` -- `Progress` kind
- `ui/src/display_list.rs` -- progress rendering
- `ui/src/renderer.rs` -- arc rendering for spinner

### Phase 6: Popup Infrastructure

**Required for Select, Color Picker, Date Picker.**

Implement the popup surface system described in `plans/trash/ui-popup-surfaces.md`:
1. `PopupConfig` and `RenderContext` popup API.
2. Multi-surface rendering in `WaylandState`.
3. Multi-surface pointer dispatch.
4. Keyboard focus-loss handling for popups.

Files changed:
- `ui/src/app.rs` -- `PopupConfig`
- `ui/src/reactive.rs` -- popup methods on `RenderContext`
- `ui/src/wayland.rs` -- popup surface management

### Phase 7: Select / Dropdown

**Depends on Phase 6 (popups) and Phase 2 (text input for searchable mode).**

1. Select element (closed state rendering).
2. Dropdown popup with options list.
3. Keyboard navigation (arrow keys, type-ahead).
4. Searchable mode with filter input.
5. Multi-select mode with checkboxes.
6. Grouped options.

Files changed:
- `ui/src/element.rs` -- `Select` kind, `SelectOption`
- `ui/src/display_list.rs` -- select/dropdown rendering
- New: `ui/src/select_state.rs` -- select state management

### Phase 8: Textarea — DONE

**Depends on Phase 2 (text input) for shared editing logic.**

1. ✅ Textarea element with multiline text (`ElementKind::Textarea`).
2. ✅ Line wrapping (soft/off via `TextWrap` enum).
3. ✅ Vertical + horizontal scrolling via `scroll_offset_y`/`scroll_offset_x`.
4. ✅ Line numbers (`show_line_numbers` flag, rendered with separator).
5. ✅ Tab handling (indent via `insert_tab` with configurable `tab_size`).
6. ✅ Multiline cursor movement (up/down/page/home/end with desired_column).
7. ✅ Auto-resize mode (`auto_resize` flag).
8. ✅ Selection across lines, undo/redo, backspace joining lines.
9. ✅ 18 unit tests covering state, cursor movement, selection, unicode.

**Remaining (deferred):** Hard wrap mode, scrollbar thumb rendering, resize drag handles.

Files changed:
- `ui/src/element.rs` -- `Textarea` kind, `TextWrap`, `TextareaResize` enums, builder + chainable methods
- New: `ui/src/textarea_state.rs` -- textarea state management with full test suite
- `ui/src/display_list.rs` -- `DrawCommand::MultilineText` + textarea display list generation
- `ui/src/renderer.rs` -- `draw_multiline_text` + `measure_char_range_in_line` methods
- `ui/src/event_dispatch.rs` -- Textarea recognized as text input for focus/tab/paste
- `ui/src/layout.rs` -- Textarea leaf node with text measurement
- `ui/src/lib.rs` -- `textarea_state` module export

### Phase 9: IME Support

**Depends on Phase 2 (text input) and Phase 8 (textarea).**

1. Bind `zwp_text_input_v3` protocol in `wayland.rs`.
2. Implement preedit rendering (underlined composing text).
3. Handle commit, delete-surrounding.
4. Set content type (Normal, Password, Number, etc.).
5. Update cursor rectangle for IME popup positioning.

Files changed:
- `ui/src/wayland.rs` -- text-input-v3 binding
- `ui/src/text_input_state.rs` -- preedit state
- `ui/src/display_list.rs` -- preedit underline rendering

### Phase 10: Color Picker

**Depends on Phase 6 (popups), Phase 2 (text input for hex input).**

1. Color swatch (closed state).
2. HSV picker popup with saturation/value square.
3. Hue slider.
4. Alpha slider.
5. Hex input.
6. HSV <-> RGB conversion.

Files changed:
- `ui/src/element.rs` -- `ColorPicker` kind
- New: `ui/src/color_picker_state.rs`
- `ui/src/renderer.rs` -- gradient shaders for SV square and hue bar

### Phase 11: Date/Time Picker

**Depends on Phase 6 (popups).**

1. Date input (closed state).
2. Calendar popup (month grid).
3. Month/year navigation.
4. Time picker (hour/minute inputs).
5. DateTime combined mode.

Files changed:
- `ui/src/element.rs` -- `DatePicker` kind
- New: `ui/src/date_picker_state.rs`
- `Cargo.toml` -- add `chrono` dependency

### Phase 12: File Input

**Depends on Phase 3 (button).**

1. File input element with button + label.
2. `xdg-desktop-portal` D-Bus integration for file dialog.
3. `kdialog` fallback.
4. Async result delivery.

Files changed:
- `ui/src/element.rs` -- `FileInput` kind
- New: `ui/src/file_dialog.rs` -- file dialog integration

---

## Theming

### Theme Struct

All form elements pull their colors from a centralized theme:

```rust
pub struct Theme {
    // Backgrounds
    pub input_bg: Color,              // rgba(30, 30, 30, 255)
    pub input_bg_hover: Color,        // rgba(40, 40, 40, 255)
    pub input_bg_disabled: Color,     // rgba(20, 20, 20, 255)

    // Borders
    pub input_border: Color,          // rgba(80, 80, 80, 255)
    pub input_border_hover: Color,    // rgba(100, 100, 100, 255)
    pub input_border_focus: Color,    // rgba(66, 133, 244, 255) -- same as primary
    pub input_border_error: Color,    // rgba(220, 53, 69, 255)

    // Primary action color (buttons, selections, focus rings)
    pub primary: Color,               // rgba(66, 133, 244, 255)
    pub primary_hover: Color,         // rgba(85, 148, 255, 255)
    pub primary_active: Color,        // rgba(50, 115, 220, 255)

    // Danger
    pub danger: Color,                // rgba(220, 53, 69, 255)
    pub danger_hover: Color,          // rgba(235, 70, 85, 255)

    // Text
    pub text: Color,                  // rgba(255, 255, 255, 255)
    pub text_secondary: Color,        // rgba(180, 180, 180, 255)
    pub text_placeholder: Color,      // rgba(255, 255, 255, 100)
    pub text_disabled: Color,         // rgba(120, 120, 120, 255)
    pub text_error: Color,            // rgba(220, 53, 69, 255)

    // Selection
    pub selection_bg: Color,          // rgba(66, 133, 244, 80)

    // Focus ring
    pub focus_ring: Color,            // rgba(66, 133, 244, 200)
    pub focus_ring_width: f32,        // 2.0

    // Slider
    pub slider_track: Color,          // rgba(80, 80, 80, 255)
    pub slider_track_fill: Color,     // rgba(66, 133, 244, 255)
    pub slider_thumb: Color,          // rgba(255, 255, 255, 255)

    // Switch
    pub switch_track_off: Color,      // rgba(80, 80, 80, 255)
    pub switch_track_on: Color,       // rgba(66, 133, 244, 255)
    pub switch_thumb: Color,          // rgba(255, 255, 255, 255)

    // Checkbox/Radio
    pub check_border: Color,          // rgba(120, 120, 120, 255)
    pub check_fill: Color,            // rgba(66, 133, 244, 255)
    pub check_mark: Color,            // rgba(255, 255, 255, 255)

    // Progress
    pub progress_track: Color,        // rgba(80, 80, 80, 255)
    pub progress_fill: Color,         // rgba(66, 133, 244, 255)

    // Popup/dropdown
    pub popup_bg: Color,              // rgba(40, 40, 40, 255)
    pub popup_border: Color,          // rgba(60, 60, 60, 255)
    pub popup_shadow: Color,          // rgba(0, 0, 0, 100)
    pub option_hover_bg: Color,       // rgba(66, 133, 244, 30)

    // Fonts
    pub font_size: f32,               // 14.0
    pub font_size_small: f32,         // 12.0
    pub font_size_large: f32,         // 16.0

    // Sizing
    pub input_height: f32,            // 36.0
    pub input_corner_radius: f32,     // 6.0
    pub input_padding_x: f32,         // 12.0
    pub input_padding_y: f32,         // 8.0
    pub checkbox_size: f32,           // 18.0
    pub radio_size: f32,              // 18.0
    pub switch_width: f32,            // 44.0
    pub switch_height: f32,           // 24.0
    pub slider_track_height: f32,     // 4.0
    pub slider_thumb_radius: f32,     // 8.0
}
```

### Applying the Theme

The theme is provided via `RenderContext`:

```rust
impl RenderContext {
    pub fn theme(&self) -> &Theme { &self.theme }
}
```

Set on the `WaylandState` at initialization. All form element rendering reads from the theme instead of hardcoded colors.

Views can override the theme:

```rust
impl View for MyApp {
    fn theme(&self) -> Theme {
        Theme {
            primary: rgba(76, 175, 80, 255),  // green primary
            ..Theme::dark()
        }
    }
}
```

### Built-in Themes

```rust
impl Theme {
    pub fn dark() -> Self { /* defaults shown above */ }
    pub fn light() -> Self {
        Self {
            input_bg: rgba(255, 255, 255, 255),
            input_border: rgba(200, 200, 200, 255),
            text: rgba(0, 0, 0, 255),
            // ...
        }
    }
}
```

### Per-Element Style Overrides

Elements can override theme colors via existing builder methods:

```rust
text_input(&value)
    .background(rgba(50, 50, 50, 255))  // override input_bg
    .border(rgba(200, 100, 0, 255), 1.0)  // override input_border
    .color(rgba(255, 200, 0, 255))  // override text color
```

These take precedence over the theme. The theme provides defaults for elements that don't specify overrides.

---

## New DrawCommand Variants Summary

The following new `DrawCommand` variants are needed across all form elements:

```rust
pub enum DrawCommand {
    // ... existing variants ...

    /// Straight line between two points.
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f32,
        cap: LineCap,  // Butt, Round, Square
    },

    /// General path (for checkmarks, arrows, arcs).
    Path {
        commands: Vec<PathCommand>,
        fill: Option<Color>,
        stroke: Option<StrokeStyle>,
    },

    /// Circle (for radio buttons, slider thumbs, spinners).
    Circle {
        center: Point,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<StrokeStyle>,
    },

    /// Arc (for spinners, circular progress).
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,  // degrees
        sweep_angle: f32,  // degrees
        stroke: StrokeStyle,
    },

    /// Linear gradient rect (for color picker, hue slider).
    GradientRect {
        bounds: Rect,
        corner_radius: f32,
        start: Point,
        end: Point,
        colors: Vec<Color>,
        positions: Option<Vec<f32>>,  // None = evenly spaced
    },
}

pub enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),      // control, end
    CubicTo(f32, f32, f32, f32, f32, f32),  // c1, c2, end
    ArcTo(f32, f32, f32, bool, bool, f32, f32),  // rx, ry, rotation, large_arc, sweep, end_x, end_y
    Close,
}

#[derive(Debug, Clone)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
}

#[derive(Debug, Clone, Copy)]
pub enum LineCap { Butt, Round, Square }

#[derive(Debug, Clone, Copy)]
pub enum LineJoin { Miter, Round, Bevel }
```

### Renderer Implementation

Each new `DrawCommand` maps directly to Skia:

```rust
DrawCommand::Line { from, to, color, width, cap } => {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(to_skia_color(color));
    paint.set_stroke_width(*width);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_cap(match cap {
        LineCap::Butt => skia_safe::paint::Cap::Butt,
        LineCap::Round => skia_safe::paint::Cap::Round,
        LineCap::Square => skia_safe::paint::Cap::Square,
    });
    canvas.draw_line((from.x, from.y), (to.x, to.y), &paint);
}

DrawCommand::Circle { center, radius, fill, stroke } => {
    if let Some(f) = fill {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(f));
        canvas.draw_circle((center.x, center.y), *radius, &paint);
    }
    if let Some(s) = stroke {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_color(to_skia_color(&s.color));
        paint.set_stroke_width(s.width);
        canvas.draw_circle((center.x, center.y), *radius, &paint);
    }
}

DrawCommand::Path { commands, fill, stroke } => {
    let mut path = skia_safe::Path::new();
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(x, y) => { path.move_to((*x, *y)); }
            PathCommand::LineTo(x, y) => { path.line_to((*x, *y)); }
            PathCommand::QuadTo(cx, cy, x, y) => { path.quad_to((*cx, *cy), (*x, *y)); }
            PathCommand::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                path.cubic_to((*c1x, *c1y), (*c2x, *c2y), (*x, *y));
            }
            PathCommand::Close => { path.close(); }
            _ => {}
        }
    }
    if let Some(f) = fill {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(to_skia_color(f));
        canvas.draw_path(&path, &paint);
    }
    if let Some(s) = stroke {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_color(to_skia_color(&s.color));
        paint.set_stroke_width(s.width);
        canvas.draw_path(&path, &paint);
    }
}

DrawCommand::GradientRect { bounds, corner_radius, start, end, colors, positions } => {
    let skia_colors: Vec<_> = colors.iter().map(to_skia_color).collect();
    let shader = skia_safe::Shader::linear_gradient(
        ((start.x, start.y), (end.x, end.y)),
        skia_colors.as_slice(),
        positions.as_deref(),
        skia_safe::TileMode::Clamp,
        None,
        None,
    );
    if let Some(shader) = shader {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_shader(shader);
        let rrect = to_rrect(bounds, *corner_radius);
        canvas.draw_rrect(rrect, &paint);
    }
}
```

---

## Element Fields Summary

New fields added to `Element`:

```rust
pub struct Element {
    // ... all existing fields ...

    // Form state
    pub disabled: bool,
    pub read_only: bool,
    pub tab_index: Option<i32>,
    pub error: Option<String>,

    // Form event handlers
    pub on_focus: Option<Box<dyn Fn(bool)>>,
    pub on_change_bool: Option<Box<dyn Fn(bool)>>,        // checkbox, switch
    pub on_change_f64: Option<Box<dyn Fn(f64)>>,          // slider
    pub on_range_change: Option<Box<dyn Fn(f64, f64)>>,   // range slider
    pub on_change_color: Option<Box<dyn Fn(Color)>>,      // color picker
    pub on_file_select: Option<Box<dyn Fn(Vec<String>)>>, // file input
    pub on_multi_change: Option<Box<dyn Fn(Vec<String>)>>,// multi-select

    // Text input specific
    pub max_length: Option<usize>,
    pub min_length: Option<usize>,
    pub select_on_focus: bool,

    // Number input specific
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub step_value: Option<f64>,

    // Textarea specific
    pub rows: Option<u32>,
    pub cols: Option<u32>,
    pub text_wrap: Option<TextWrap>,
    pub textarea_resize: Option<TextareaResize>,
    pub line_numbers: bool,
    pub tab_size: u32,
    pub auto_resize: bool,

    // Slider specific
    pub tick_marks: Option<Vec<f64>>,
    pub show_value: bool,

    // Button specific
    pub loading: bool,

    // Color picker specific
    pub show_alpha: bool,

    // File input specific
    pub accept: Option<String>,
    pub multiple_files: bool,
}
```

Note: this grows `Element` significantly. An alternative is to use `HashMap<String, Box<dyn Any>>` for uncommon props, or split into `ElementProps` sub-structs per kind. For v1, flat fields are fine -- most are `Option` types that add minimal memory overhead.

---

## ElementKind Variants Summary

All new variants:

```rust
pub enum ElementKind {
    // Existing
    Container,
    Text { content: String },
    Image { source: ImageSource },
    Spacer,
    Divider { thickness: f32 },
    TextInput { value: String, placeholder: String },  // enhanced with variant

    // New
    Textarea { value: String, placeholder: String },
    Select {
        value: Option<String>,
        options: Vec<SelectOption>,
        placeholder: String,
        multiple: bool,
        selected_values: Vec<String>,
        searchable: bool,
    },
    Checkbox { checked: bool, indeterminate: bool, label: Option<String> },
    Radio { selected: bool, group: String, value: String, label: Option<String> },
    Switch { on: bool, label: Option<String> },
    Slider { value: f64, min: f64, max: f64, step: Option<f64>, orientation: SliderOrientation },
    RangeSlider { low: f64, high: f64, min: f64, max: f64, step: Option<f64>, orientation: SliderOrientation },
    Button { label: String, variant: ButtonVariant },
    Progress { value: Option<f64>, variant: ProgressVariant },
    ColorPicker { value: Color },
    DatePicker { value: Option<String>, variant: DatePickerVariant },  // String ISO format for simplicity
    FileInput { file_name: Option<String>, accept: Option<String>, multiple: bool },
}
```

---

## Dependencies

New Rust crate dependencies:

| Crate | Purpose | Phase |
|-------|---------|-------|
| `unicode-segmentation` | Grapheme cluster iteration for cursor movement | Phase 2 |
| `chrono` | Date/time types for date picker | Phase 11 |

Wayland protocol extensions:

| Protocol | Purpose | Phase |
|----------|---------|-------|
| `wl_data_device_manager` / `wl_data_device` | Clipboard (copy/paste) | Phase 2 |
| `zwp_text_input_v3` | IME composition input | Phase 9 |
| `xdg-desktop-portal` (D-Bus) | Native file dialog | Phase 12 |
