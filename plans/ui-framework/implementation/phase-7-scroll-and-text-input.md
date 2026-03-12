# Phase 7: Scroll Containers & Text Input

## Goal

Add scrollable containers with trackpad/wheel physics and the `text_input()` element with cursor, selection, and clipboard support. These are the two remaining interactive primitives needed before migrating the dock (and building the launcher).

## Steps

### 7.1 Scroll element

`src/scroll.rs` — scroll container builders.

```rust
pub fn scroll() -> Element    { /* vertical scroll */ }
pub fn scroll_x() -> Element  { /* horizontal scroll */ }
pub fn scroll_xy() -> Element { /* bidirectional */ }
```

Style methods:

```rust
impl Element {
    pub fn scroll_padding(mut self, edges: EdgeInsets) -> Self { ... }
    pub fn overscroll(mut self, enabled: bool) -> Self { ... }
    pub fn scroll_bar(mut self, visible: bool) -> Self { ... }
    pub fn handle(mut self, handle: &ScrollHandle) -> Self { ... }
}
```

### 7.2 Scroll layout

Taffy integration: scroll container node uses `overflow: Scroll`.

After `compute_layout()`:
- Viewport size from `tree.layout(scroll_node)`
- Content size from `tree.layout(content_wrapper_node)`
- `max_offset = content_size - viewport_size` (clamped ≥ 0)

### 7.3 Scroll state & physics

Internal state (owned by the framework, not exposed as `Reactive`):

```rust
struct ScrollState {
    offset: f32,
    velocity: f32,
    phase: ScrollPhase,
    max_offset: f32,
    content_size: f32,
    viewport_size: f32,
}

enum ScrollPhase {
    Idle,
    Tracking,
    Momentum,
    OverscrollSnap,
}
```

**Trackpad (finger source):**
1. Tracking — 1:1 with delta, rubber-band at edges (`delta * 0.3`)
2. Momentum — exponential deceleration (`velocity *= 0.97` per frame)
3. Overscroll snap — spring back to nearest edge

**Mouse wheel:**
- Each tick: spring to `current_target ± 48px`
- Rapid ticks accumulate (spring retargets — interruptible)

**Interruption:** any new input cancels momentum/spring and enters Tracking.

### 7.4 Scroll rendering

Two new display list commands (already defined in Phase 2):
- `PushTranslate { offset: Point(0, -scroll_y) }` inside a `PushClip`
- Children outside viewport are culled from display list

Damage tracking: scroll offset change → damage entire viewport rect.

### 7.5 Scroll indicators

Thin overlay rendered after scroll content:
- 4px wide, rounded, semi-transparent white
- Fade in on scroll start, fade out 800ms after scroll stops
- `thumb_height = viewport_h * (viewport_h / content_h)`, min 24px
- `thumb_y = scroll_offset / max_offset * (viewport_h - thumb_h)`

### 7.6 ScrollHandle

```rust
pub struct ScrollHandle { /* internal channel to scroll state */ }

impl ScrollHandle {
    pub fn to_top(&self) { ... }
    pub fn to_bottom(&self) { ... }
    pub fn to_offset(&self, offset: f32) { ... }
    pub fn to_item(&self, key: &str) { ... }
    pub fn to_offset_immediate(&self, offset: f32) { ... }
}
```

All programmatic scrolls use spring animation, are interruptible by user input.

### 7.7 Nested scrolling

- Innermost scroll container under pointer gets the event
- At scroll limits, propagate to parent if axes match
- Cross-axis events pass through

### 7.8 Text input element

`src/text_input.rs`:

```rust
pub fn text_input(value: &str) -> Element {
    // ElementKind::TextInput
    // Implicitly focusable
    // Default cursor: CursorStyle::Text
}
```

Builder methods:

```rust
impl Element {
    pub fn placeholder(mut self, text: &str) -> Self { ... }
    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self { ... }
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self { ... }
}
```

### 7.9 Text input internals

Internal state:
- `cursor_position: usize` (byte offset in UTF-8 string)
- `selection: Option<(usize, usize)>` (start, end)
- `cursor_blink: bool` (toggled by timer)
- `scroll_offset: f32` (horizontal scroll for long text)

Key handling:
- Arrow keys → cursor movement (with Shift for selection)
- Backspace/Delete → delete text
- Ctrl+A → select all
- Ctrl+C/X/V → clipboard via `wl_data_device`
- Enter → fire `on_submit`
- Any character → insert at cursor, fire `on_change`

### 7.10 Clipboard

Wayland clipboard via `wl_data_device_manager`:
- **Copy:** set `wl_data_source` with `text/plain;charset=utf-8` mime type
- **Paste:** request `wl_data_offer`, read from fd

SCTK provides helpers for this via `DataDeviceHandler`.

### 7.11 Validation

Test app — scrollable list with search:

```rust
struct ListTestView {
    items: Reactive<Vec<String>>,
    scroll: ScrollHandle,
}

impl View for ListTestView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        column()
            .size(400.0, 300.0)
            .background(rgba(20, 20, 20, 255))
            .child(
                text_input("")
                    .placeholder("Filter...")
                    .on_change(move |text| { /* filter items */ })
            )
            .child(
                scroll()
                    .handle(&self.scroll)
                    .fill_width()
                    .fill_height()
                    .children(
                        self.items.get(cx).iter().map(|item| {
                            container()
                                .padding(12.0)
                                .child(text(item).color(WHITE))
                        })
                    )
            )
    }
}
```

Verify scrolling works with trackpad and wheel, text input accepts keyboard input, and the list filters.

## Output

The framework has all interactive primitives: scroll containers with proper physics and text input with clipboard. Ready for the launcher UI and any other text-heavy application.
