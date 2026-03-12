# UI Components

## Elements

### container()

The universal building block. All visual properties are set via chainable style methods.

```rust
container()
    .background(rgba(30, 30, 30, 190))
    .rounded(18.0)
    .border(rgba(255, 255, 255, 30), 1.0)
    .padding(12.0)
    .size(200.0, 64.0)
    .opacity(0.8)
    .clip()
    .child(/* ... */)
```

A circle is just `container().size(20.0, 20.0).rounded(10.0).background(white)`.

### row(), column(), stack()

Shorthand for `container().direction(Direction)`.

```rust
row()   // container with horizontal flex layout
column() // container with vertical flex layout
stack()  // container with z-axis overlay (children stacked on top of each other)
```

Layout props: `.gap()`, `.align_items()`, `.justify()`.

### text()

Single-line text rendering.

```rust
text("Hello")
    .font_size(14.0)
    .color(rgba(255, 255, 255, 200))
```

### image()

Renders SVG or raster images. Supports inline SVG strings and file paths.

```rust
// Inline SVG
image(svg_str)

// Embedded at compile time
image(include_str!("assets/logo.svg"))

// Load from filesystem (SVG or PNG)
image_file("/usr/share/icons/hicolor/48x48/apps/firefox.svg")
```

### text_input()

Text input field with change handler.

```rust
text_input(&self.query)
    .placeholder("Search apps...")
    .font_size(20.0)
    .color(rgba(255, 255, 255, 230))
    .on_change(move |text| cx.update(|s| s.query = text))
```

### spacer()

Flexible space that expands to fill available room in a row/column.

```rust
row()
    .child(text("Left"))
    .child(spacer())
    .child(text("Right"))
```

### divider()

Thin line separator.

```rust
divider()
    .color(rgba(255, 255, 255, 15))
    .thickness(1.0)
    .margin_x(16.0)  // horizontal inset
```

## Style Methods (chainable on any element)

### Layout
- `.width(f32)` / `.height(f32)` / `.size(w, h)` — fixed dimensions
- `.fill_width()` / `.fill_height()` — expand to fill parent
- `.padding(f32)` / `.padding_xy(x, y)` / `.padding_edges(t, r, b, l)` — inner spacing
- `.gap(f32)` — spacing between children
- `.align_items(Alignment)` — cross-axis alignment (Start, Center, End)
- `.justify(Justify)` — main-axis justification (Start, Center, End, SpaceBetween)

### Visual
- `.background(Color)` — background color
- `.rounded(f32)` — corner radius
- `.border(Color, f32)` — border color and width
- `.opacity(f32)` — 0.0 to 1.0
- `.clip()` — clip children to this element's bounds

### Interaction
- `.on_click(move || cx.update(|s| { ... }))` — click handler
- `.on_drag(move |delta| cx.update(|s| { ... }))` — drag handler
- `.key("id")` — identity for animation diffing

### Animation
- `.animate_layout()` — spring-animate position/size changes
- `.animate(Animation)` — animate style prop changes, auto-scoped via `Reactive<T>`
- `.initial(From)` / `.exit(To)` — enter/exit animations for keyed elements
- See [animations.md](animations.md) for the full animation spec

## State Management

Views own their state using `Reactive<T>` fields for automatic dependency tracking and re-render triggering. `render(&self)` returns the element tree. Event handler closures mutate state through a `cx.handle()`:

```rust
struct MyView {
    count: Reactive<i32>,
}

impl View for MyView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        container()
            .child(text(&format!("Count: {}", self.count.get(cx))))
            .child(
                container()
                    .background(rgba(60, 60, 60, 255))
                    .rounded(8.0)
                    .padding(12.0)
                    .child(text("Click me"))
                    .on_click(move || handle.update(|s| s.count.set(s.count.get() + 1)))
            )
    }
}
```

`Reactive<T>` enables automatic animation scoping — see [animations.md](animations.md) for details.
```
