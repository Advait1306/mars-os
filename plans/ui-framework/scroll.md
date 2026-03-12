# Scroll

## Element

`scroll()` is a container that clips its children and offsets them by a scroll position. It behaves like a `column()` whose content can exceed the viewport.

```rust
scroll()
    .fill_width()
    .fill_height()
    .children(
        self.items.get(cx).iter().map(|item| {
            container()
                .padding(12.0)
                .child(text(&item.name))
        })
    )
```

### Direction

Vertical by default. Horizontal and bidirectional variants:

```rust
scroll()              // vertical (default)
scroll_x()            // horizontal
scroll_xy()           // both axes
```

These are shorthand for `scroll().direction(ScrollDirection::Vertical)`, etc.

### Style methods

All standard container style methods work (`.padding()`, `.background()`, `.rounded()`, etc.). Additional scroll-specific methods:

```rust
scroll()
    .scroll_padding(EdgeInsets)   // inset content from scroll edges (sticky headers, FABs)
    .overscroll(false)            // disable rubber-band effect (default: true)
    .scroll_bar(false)            // hide scroll indicator (default: true)
```

## Layout

Taffy handles the heavy lifting. The scroll container's node uses `overflow: Scroll`, which tells Taffy to lay out children at their intrinsic size regardless of the container's bounds.

```rust
let style = Style {
    overflow: Point { x: Overflow::Hidden, y: Overflow::Scroll },
    // ... normal size/padding/gap from chained methods
    ..Default::default()
};
```

After `compute_layout()`:
- **Container bounds** — `tree.layout(scroll_node)` gives the viewport size
- **Content size** — `tree.layout(content_node)` gives the full content height/width (the single flex child that wraps all scroll children)
- **Max scroll offset** — `content_height - viewport_height` (clamped to 0)

## Rendering

Two new display list commands:

```rust
enum DrawCommand {
    // ... existing commands ...
    PushTranslate { offset: Point },
    PopTranslate,
}
```

Scroll container emit sequence:

```
PushClip { bounds: viewport, corner_radius }
PushTranslate { offset: Point(0.0, -scroll_y) }
  // ... child draw commands ...
PopTranslate
PopClip
```

Skia execution:

```rust
DrawCommand::PushTranslate { offset } => {
    canvas.save();
    canvas.translate(offset);
}
DrawCommand::PopTranslate => {
    canvas.restore();
}
```

### Damage tracking

When `scroll_offset` changes, the damage region is the entire viewport rect. Skia clips to this region, so only visible children are drawn. Children fully outside the viewport can be culled from the display list entirely during tree walk — skip any child whose resolved `y + height < scroll_offset` or `y > scroll_offset + viewport_height`.

## Scroll State

Scroll offset is internal state, not exposed as `Reactive<T>` on the view. The framework owns it:

```rust
struct ScrollState {
    offset: f32,              // current visual offset (includes overscroll)
    velocity: f32,            // current scroll velocity (px/sec)
    phase: ScrollPhase,
    max_offset: f32,          // content_size - viewport_size, clamped ≥ 0
    content_size: f32,        // from layout
    viewport_size: f32,       // from layout
}

enum ScrollPhase {
    Idle,
    Tracking,      // finger/wheel actively scrolling
    Momentum,      // finger lifted, decelerating
    OverscrollSnap, // spring back from overscroll
}
```

## Input

Wayland scroll events arrive as `wl_pointer.axis` (discrete wheel ticks) and `wl_pointer.axis_source` (wheel vs finger). The framework translates these to scroll deltas:

### Trackpad / touchscreen (finger source)

Three-phase gesture:

1. **Tracking** — 1:1 offset tracking with pointer deltas. If the user scrolls past the edge, apply rubber-band resistance: `visual_delta = delta * 0.3`.

2. **Momentum** — On `axis_stop`, the framework applies exponential deceleration from the current velocity:
   ```
   velocity *= deceleration_rate   // per frame, rate ≈ 0.97
   offset += velocity * dt
   ```
   Stop when `|velocity| < 0.5 px/s`. If offset exceeds bounds during momentum, transition to overscroll snap.

3. **Overscroll snap** — Spring animation back to the nearest edge (0 or max_offset). Uses `Animation::snappy()` physics. The spring's initial velocity comes from the current momentum velocity for a smooth handoff.

### Mouse wheel (wheel source)

Discrete steps. Each tick scrolls a fixed distance (e.g. 48px) animated with `Animation::default()` spring to the target offset. No momentum phase — each tick is a discrete spring animation to `current_target ± 48`. Rapid ticks accumulate (the spring retargets to the new offset, which is why springs matter here — interruptible).

### Interruption

Any new scroll input during momentum or spring-back immediately cancels the current animation and transitions to `Tracking`. This is why scroll uses springs and velocity-preserving transitions — no jarring stops.

## Scroll Indicators

Thin overlay bars that show scroll position. Rendered on top of content (after the scroll content's `PopClip`), inside the viewport bounds.

```
Track: none (invisible)
Thumb: 4px wide, rounded, semi-transparent white
```

### Visibility

- **Hidden** while idle
- **Fade in** (`Animation::ease(150)`) when scrolling starts
- **Fade out** (`Animation::ease(300)`, 800ms delay after scroll stops)
- Always visible during active trackpad drag

### Sizing

```
thumb_height = viewport_height * (viewport_height / content_height)
thumb_y      = scroll_offset / max_offset * (viewport_height - thumb_height)
```

Minimum thumb height: 24px (so it's always grabbable, even with very long content).

## Programmatic Scrolling

Views can control scroll position through a `ScrollHandle`:

```rust
struct MyView {
    scroll: ScrollHandle,
    items: Reactive<Vec<Item>>,
}

impl View for MyView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        scroll()
            .handle(&self.scroll)
            .fill_width()
            .fill_height()
            .children(/* ... */)
    }
}

// In an event handler:
handle.update(|s| {
    s.scroll.to_top();                          // spring to offset 0
    s.scroll.to_bottom();                       // spring to max offset
    s.scroll.to_offset(200.0);                  // spring to specific offset
    s.scroll.to_item("item-key");               // spring to bring keyed element into view
    s.scroll.to_offset_immediate(0.0);          // jump without animation
});
```

`to_item()` finds the keyed element's layout bounds and springs to the minimum offset that makes it fully visible. If it's already visible, no-op.

All programmatic scrolls use `Animation::default()` spring. They are interruptible — user scroll input cancels them immediately.

## Nested Scrolling

When scroll containers are nested (e.g. a horizontal carousel inside a vertical list):

1. Scroll input goes to the **innermost** scroll container under the pointer
2. If the inner container is at its scroll limit (top/bottom edge) and the scroll direction matches the outer container's axis, the event **propagates** to the outer container
3. During momentum, if the inner container hits its limit, remaining velocity transfers to the outer container

This matches platform behavior (iOS/Android nested scroll).

Cross-axis events pass through: vertical scroll events inside a `scroll_x()` propagate to a parent `scroll()` without interference.

## Example — App Launcher

```rust
struct LauncherView {
    query: Reactive<String>,
    results: Reactive<Vec<App>>,
    scroll: ScrollHandle,
}

impl View for LauncherView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        column()
            .width(600.0)
            .height(480.0)
            .background(rgba(20, 20, 20, 230))
            .rounded(16.0)
            .clip()
            .child(
                // Fixed search bar
                container()
                    .padding(16.0)
                    .child(
                        text_input(&self.query)
                            .placeholder("Search apps...")
                            .font_size(18.0)
                            .on_change(move |text| handle.update(|s| {
                                s.query.set(text);
                                s.scroll.to_top();
                            }))
                    )
            )
            .child(divider().color(rgba(255, 255, 255, 15)))
            .child(
                // Scrollable results
                scroll()
                    .handle(&self.scroll)
                    .fill_width()
                    .fill_height()
                    .padding_xy(8.0, 4.0)
                    .children(
                        self.results.get(cx).iter().map(|app| {
                            container()
                                .key(&app.id)
                                .fill_width()
                                .padding(12.0)
                                .rounded(8.0)
                                .background(if self.selected.get(cx) == Some(&app.id) {
                                    rgba(255, 255, 255, 15)
                                } else {
                                    Color::TRANSPARENT
                                })
                                .animate(Animation::snappy())
                                .child(row().gap(12.0).align_items(Alignment::Center)
                                    .child(image_file(&app.icon).size(32.0, 32.0))
                                    .child(column().gap(2.0)
                                        .child(text(&app.name).font_size(14.0))
                                        .child(text(&app.description)
                                            .font_size(12.0)
                                            .color(rgba(255, 255, 255, 120)))
                                    )
                                )
                                .on_click(move || handle.update(|s| s.launch(&app.id)))
                        })
                    )
            )
    }
}
```
