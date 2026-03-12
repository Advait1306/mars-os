# Phase 1: Crate Setup, Core Element Tree & Layout

## Goal

Create the `ui` crate with the element builder API and Taffy layout integration. By the end of this phase, you can construct an element tree in code and compute resolved pixel bounds for every element — no rendering, no Wayland.

## Steps

### 1.1 Create the `ui` crate

Initialize the crate using Cargo from the repo root:

```bash
cargo init --lib ui
```

This creates `ui/Cargo.toml` and `ui/src/lib.rs`. Then add the additional source files:

```
ui/src/
  lib.rs       (created by cargo init)
  color.rs
  element.rs
  style.rs
  layout.rs
```

Add the Taffy dependency:

```bash
cd ui && cargo add taffy@0.7
```

### 1.2 Color type

`color.rs` — a simple `Color` struct used throughout the framework.

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

pub const TRANSPARENT: Color = rgba(0, 0, 0, 0);
pub const WHITE: Color = rgba(255, 255, 255, 255);
pub const BLACK: Color = rgba(0, 0, 0, 255);
```

### 1.3 Style types

`style.rs` — enums and structs for layout and visual properties.

```rust
pub enum Direction {
    Row,
    Column,
}

pub enum Alignment {
    Start,
    Center,
    End,
}

pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
}

pub struct Border {
    pub color: Color,
    pub width: f32,
}
```

### 1.4 Element tree

`element.rs` — the `Element` enum and builder functions.

An `Element` is a node in the UI tree. It carries style properties and children. Builder functions (`container()`, `row()`, `text()`, etc.) return an `Element` that is configured via chainable methods.

Core element kinds:

```rust
pub enum ElementKind {
    Container,              // generic box
    Text { content: String },
    Image { source: ImageSource },
    Spacer,
    Divider { thickness: f32 },
}

pub enum ImageSource {
    Svg(String),            // inline SVG string
    File(String),           // filesystem path
}
```

Each `Element` holds:

- `kind: ElementKind`
- `key: Option<String>` — identity for diffing
- `direction: Direction` — flex direction (Row or Column)
- `children: Vec<Element>`
- Layout props: `width`, `height`, `fill_width`, `fill_height`, `padding` (top/right/bottom/left), `gap`, `align_items`, `justify`
- Visual props: `background`, `corner_radius`, `border`, `opacity`, `clip`
- Later phases add: event handlers, animation config, scroll config

Builder functions:

```rust
pub fn container() -> Element { /* direction: Column, defaults */ }
pub fn row() -> Element      { /* direction: Row */ }
pub fn column() -> Element   { /* direction: Column */ }
pub fn stack() -> Element    { /* direction: Column (z-stack behavior handled in layout) */ }
pub fn text(content: &str) -> Element { /* ElementKind::Text */ }
pub fn image(svg: &str) -> Element { /* ElementKind::Image(Svg) */ }
pub fn image_file(path: &str) -> Element { /* ElementKind::Image(File) */ }
pub fn spacer() -> Element   { /* flex_grow: 1.0 */ }
pub fn divider() -> Element  { /* ElementKind::Divider, thickness 1.0 */ }
```

Chainable style methods on `Element` (all return `self`):

```rust
impl Element {
    // Layout
    pub fn width(mut self, w: f32) -> Self { ... }
    pub fn height(mut self, h: f32) -> Self { ... }
    pub fn size(mut self, w: f32, h: f32) -> Self { ... }
    pub fn fill_width(mut self) -> Self { ... }
    pub fn fill_height(mut self) -> Self { ... }
    pub fn padding(mut self, p: f32) -> Self { ... }
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self { ... }
    pub fn padding_edges(mut self, t: f32, r: f32, b: f32, l: f32) -> Self { ... }
    pub fn gap(mut self, g: f32) -> Self { ... }
    pub fn align_items(mut self, a: Alignment) -> Self { ... }
    pub fn justify(mut self, j: Justify) -> Self { ... }

    // Visual
    pub fn background(mut self, c: Color) -> Self { ... }
    pub fn rounded(mut self, r: f32) -> Self { ... }
    pub fn border(mut self, color: Color, width: f32) -> Self { ... }
    pub fn opacity(mut self, o: f32) -> Self { ... }
    pub fn clip(mut self) -> Self { ... }

    // Children
    pub fn child(mut self, child: Element) -> Self { ... }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self { ... }

    // Identity
    pub fn key(mut self, k: &str) -> Self { ... }

    // Text/Image-specific
    pub fn font_size(mut self, s: f32) -> Self { ... }
    pub fn color(mut self, c: Color) -> Self { ... }
}
```

### 1.5 Layout engine (Taffy)

`layout.rs` — converts an element tree to Taffy nodes, computes layout, and outputs resolved bounds.

```rust
pub struct LayoutNode {
    pub bounds: Rect,           // x, y, width, height in pixels
    pub element_index: usize,   // index into flat element list for style lookup
    pub children: Vec<LayoutNode>,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```

The layout pass:

1. **Build Taffy tree** — walk the `Element` tree recursively, creating a `taffy::NodeId` for each element. Map element style props to `taffy::Style`:
   - `width`/`height` → `Size { width: Dimension::Length, height: Dimension::Length }`
   - `fill_width`/`fill_height` → `Size { width: Dimension::Percent(1.0), ... }` + `flex_grow: 1.0`
   - `padding` → `Rect<LengthPercentage>`
   - `gap` → `Size { width: LengthPercentage::Length(gap), height: LengthPercentage::Length(gap) }`
   - `direction` → `FlexDirection::Row` or `Column`
   - `align_items` / `justify` → `AlignItems` / `JustifyContent`
   - `spacer()` → `flex_grow: 1.0`
   - `text()` → measure function that returns text metrics (placeholder for now: fixed height based on font_size, width = content length * rough char width)
   - `image()` → intrinsic size from the image source (or specified via `.size()`)

2. **Compute layout** — `taffy_tree.compute_layout(root, available_space)` where `available_space` is the surface size.

3. **Extract results** — walk the Taffy tree, read `taffy_tree.layout(node_id)` for each node, build a `LayoutNode` tree with resolved pixel positions (converting Taffy's parent-relative coords to absolute).

### 1.6 Public API

`lib.rs` re-exports:

```rust
pub mod color;
pub mod element;
pub mod style;
pub mod layout;

pub use color::*;
pub use element::*;
pub use style::*;
```

### 1.7 Validation

Write unit tests that:

1. Build an element tree (container with children, nested rows/columns)
2. Run layout with a known available space
3. Assert resolved bounds match expected pixel positions
4. Test spacer distributes remaining space
5. Test padding, gap, alignment

## Output

A `ui` crate that compiles and passes layout tests. No rendering, no Wayland. The API matches the design docs and dock can already express its UI tree in the new builder syntax (even though it can't render yet).
