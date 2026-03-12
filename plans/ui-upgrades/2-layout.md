# Layout System Implementation Plan

## Overview

The UI framework currently uses Taffy 0.7.7 for flexbox layout with a limited subset of CSS properties wired through. This plan covers every CSS layout property needed for a comprehensive UI toolkit, how each maps to Taffy's API, what is already implemented, and what needs custom work.

### Current State Summary

**Implemented:**
- `display: flex` (hardcoded for all elements)
- `flex-direction`: Row, Column
- `justify-content`: FlexStart, Center, FlexEnd, SpaceBetween
- `align-items`: FlexStart, Center, FlexEnd
- `flex-grow`
- `gap` (uniform row and column gap)
- `padding` (all four sides via `LengthPercentage::Length`)
- `width`/`height` via `Dimension::Length` only
- `fill_width`/`fill_height` via `Dimension::Percent(1.0)` + `flex_grow: 1.0`
- Scroll containers (custom implementation on top of layout)
- Text measurement callback

**Not implemented:**
- Grid layout
- `flex-shrink`, `flex-basis`, `flex-wrap`, `order`
- `align-self`, `align-content`
- `margin`
- `min-width`/`min-height`/`max-width`/`max-height`
- `aspect-ratio`
- `position: absolute`/`relative` with inset offsets
- `z-index` / stacking contexts
- `overflow` (layout-level; rendering clip exists but not layout-aware)
- `box-sizing`
- Intrinsic sizing (`min-content`, `max-content`, `fit-content`)
- Percentage dimensions
- `display: none` / `display: block`

---

## Part 1: Flexbox Properties

### 1.1 flex-direction

| | Detail |
|---|---|
| **CSS Spec** | `row` / `row-reverse` / `column` / `column-reverse` |
| **Taffy API** | `Style.flex_direction: FlexDirection` with variants `Row`, `RowReverse`, `Column`, `ColumnReverse` |
| **Current Status** | Partially implemented. `Row` and `Column` wired. `RowReverse` and `ColumnReverse` not exposed. |
| **Priority** | Must-have (reverse variants) |

**Rust DSL API:**
```rust
// Already exists
fn direction_row() -> Element  // via row()
fn direction_column() -> Element  // via column()

// Add
impl Element {
    fn direction(self, d: Direction) -> Self;
}

enum Direction {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}
```

**Implementation:** Add `RowReverse` and `ColumnReverse` to the `Direction` enum in `style.rs`, map them in `element_to_taffy_style`.

---

### 1.2 flex-wrap

| | Detail |
|---|---|
| **CSS Spec** | `nowrap` (default) / `wrap` / `wrap-reverse`. Controls whether flex items wrap to new lines. |
| **Taffy API** | `Style.flex_wrap: FlexWrap` with variants `NoWrap`, `Wrap`, `WrapReverse` |
| **Current Status** | Not implemented. Taffy defaults to `NoWrap`. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn wrap(mut self) -> Self;          // FlexWrap::Wrap
    fn wrap_reverse(mut self) -> Self;  // FlexWrap::WrapReverse
    fn no_wrap(mut self) -> Self;       // FlexWrap::NoWrap (explicit)
}
```

**Implementation:** Add `wrap: FlexWrap` field to `Element` (default `NoWrap`), map in `element_to_taffy_style`.

---

### 1.3 justify-content

| | Detail |
|---|---|
| **CSS Spec** | `flex-start` / `flex-end` / `center` / `space-between` / `space-around` / `space-evenly` |
| **Taffy API** | `Style.justify_content: Option<JustifyContent>` with variants `Start`, `End`, `FlexStart`, `FlexEnd`, `Center`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly` |
| **Current Status** | Partially implemented. Missing `SpaceAround` and `SpaceEvenly`. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,   // add
    SpaceEvenly,   // add
}
```

**Implementation:** Add `SpaceAround` and `SpaceEvenly` variants to `Justify` enum, map to `JustifyContent::SpaceAround` and `JustifyContent::SpaceEvenly`.

---

### 1.4 align-items

| | Detail |
|---|---|
| **CSS Spec** | `stretch` (default) / `flex-start` / `flex-end` / `center` / `baseline` |
| **Taffy API** | `Style.align_items: Option<AlignItems>` with variants `Start`, `End`, `FlexStart`, `FlexEnd`, `Center`, `Baseline`, `Stretch` |
| **Current Status** | Partially implemented. Missing `Stretch` and `Baseline`. |
| **Priority** | Must-have (`Stretch` especially -- it is the CSS default) |

**Rust DSL API:**
```rust
enum Alignment {
    Start,
    Center,
    End,
    Stretch,    // add
    Baseline,   // add
}
```

**Implementation:** Add `Stretch` and `Baseline` to `Alignment` enum. Consider changing the default from `Start` to `Stretch` to match CSS behavior, or document the divergence.

---

### 1.5 align-self

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `flex-start` / `flex-end` / `center` / `baseline` / `stretch`. Overrides parent `align-items` for a single child. |
| **Taffy API** | `Style.align_self: Option<AlignSelf>` with variants matching `AlignItems`. `None` means inherit from parent. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn align_self(mut self, a: Alignment) -> Self;
}
```

**Implementation:** Add `align_self: Option<Alignment>` to `Element`, map `Some(a)` to `Some(AlignSelf::...)` in `element_to_taffy_style`.

---

### 1.6 align-content

| | Detail |
|---|---|
| **CSS Spec** | Controls alignment of flex lines in a multi-line flex container (when `flex-wrap: wrap`). Values: `stretch` / `flex-start` / `flex-end` / `center` / `space-between` / `space-around` / `space-evenly`. |
| **Taffy API** | `Style.align_content: Option<AlignContent>` with variants `Start`, `End`, `FlexStart`, `FlexEnd`, `Center`, `Stretch`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly` |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have (only relevant with flex-wrap) |

**Rust DSL API:**
```rust
impl Element {
    fn align_content(mut self, a: AlignContent) -> Self;
}

enum AlignContent {
    Start, Center, End, Stretch,
    SpaceBetween, SpaceAround, SpaceEvenly,
}
```

**Implementation:** Add field and mapping. Only meaningful when `flex_wrap != NoWrap`.

---

### 1.7 flex-grow

| | Detail |
|---|---|
| **CSS Spec** | `<number>` >= 0. Default 0. Determines how much a flex item grows relative to siblings when space is available. |
| **Taffy API** | `Style.flex_grow: f32` (default 0.0) |
| **Current Status** | Implemented. |
| **Priority** | Done |

No changes needed.

---

### 1.8 flex-shrink

| | Detail |
|---|---|
| **CSS Spec** | `<number>` >= 0. Default 1. Determines how much a flex item shrinks relative to siblings when space is insufficient. |
| **Taffy API** | `Style.flex_shrink: f32` (default 1.0) |
| **Current Status** | Not implemented. Taffy uses the correct default (1.0) since we do not override it, so default behavior is correct. But no DSL method to change it. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn flex_shrink(mut self, s: f32) -> Self;
}
```

**Implementation:** Add `flex_shrink: Option<f32>` to `Element`, map in style conversion.

---

### 1.9 flex-basis

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `<length>` / `<percentage>` / `content`. The initial main size of a flex item before grow/shrink. |
| **Taffy API** | `Style.flex_basis: Dimension` (default `Dimension::Auto`). Supports `Auto`, `Length(f32)`, `Percent(f32)`. |
| **Current Status** | Not implemented. Taffy defaults to `Auto` which is correct, but no DSL method. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn flex_basis(mut self, b: Dimension) -> Self;
    fn flex_basis_px(mut self, px: f32) -> Self;
    fn flex_basis_pct(mut self, pct: f32) -> Self;
}
```

**Implementation:** Add `flex_basis: Option<Dimension>` to `Element`, map in style conversion.

---

### 1.10 order

| | Detail |
|---|---|
| **CSS Spec** | `<integer>`. Default 0. Controls visual order of flex/grid items without changing DOM order. |
| **Taffy API** | Not directly supported in Taffy 0.7. Taffy lays out children in the order they appear in the tree. |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have |

**Custom Implementation:** Sort children by order value before building the Taffy tree. This is straightforward since we control the tree construction in `build_taffy_node`.

---

### 1.11 gap (row-gap, column-gap)

| | Detail |
|---|---|
| **CSS Spec** | `row-gap` and `column-gap` (shorthand `gap`). Applies to flex and grid containers. Values: `<length>` / `<percentage>` / `normal`. |
| **Taffy API** | `Style.gap: Size<LengthPercentage>` with separate `width` (column-gap) and `height` (row-gap) fields. Supports `Length(f32)` and `Percent(f32)`. |
| **Current Status** | Partially implemented. Only uniform gap (same for row and column). |
| **Priority** | Nice-to-have (separate row/column gap) |

**Rust DSL API:**
```rust
impl Element {
    fn gap(mut self, g: f32) -> Self;              // already exists (uniform)
    fn row_gap(mut self, g: f32) -> Self;          // add
    fn column_gap(mut self, g: f32) -> Self;       // add
    fn gap_xy(mut self, col: f32, row: f32) -> Self; // add
}
```

**Implementation:** Change `gap: f32` to `gap: [f32; 2]` (row, column) in `Element`, or add separate fields.

---

## Part 2: Grid Layout

Taffy 0.7 has full CSS Grid support behind the `grid` feature flag. The `ui` crate depends on `taffy = "0.7"` which includes grid by default.

### 2.1 display: grid

| | Detail |
|---|---|
| **CSS Spec** | Enables CSS Grid layout for children. |
| **Taffy API** | `Style.display = Display::Grid` |
| **Current Status** | Not implemented. All elements hardcoded to `Display::Flex`. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
fn grid() -> Element;  // new constructor, sets display to Grid
```

**Implementation:** Add a `display` field to `Element` (enum with `Flex`, `Grid`, `None`, `Block`). Map to `Display::Flex`, `Display::Grid`, etc. in `element_to_taffy_style`.

---

### 2.2 grid-template-columns / grid-template-rows

| | Detail |
|---|---|
| **CSS Spec** | Defines explicit track sizes. Values: `<track-size>` list, `repeat()`, `minmax()`, `fr`, `auto`, `min-content`, `max-content`, `fit-content()`. |
| **Taffy API** | `Style.grid_template_columns: Vec<TrackSizingFunction>` and `Style.grid_template_rows: Vec<TrackSizingFunction>`. `TrackSizingFunction` is either a single track (`minmax(min, max)`) or a `repeat(count, tracks)`. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have (for grid) |

**Taffy types involved:**
- `TrackSizingFunction::Single(NonRepeatedTrackSizingFunction)` -- a single track with min/max sizing
- `TrackSizingFunction::Repeat(GridTrackRepetition, Vec<NonRepeatedTrackSizingFunction>)` -- repeated tracks
- `NonRepeatedTrackSizingFunction` = `MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>`
- `MinTrackSizingFunction`: `Fixed(LengthPercentage)`, `MinContent`, `MaxContent`, `Auto`
- `MaxTrackSizingFunction`: `Fixed(LengthPercentage)`, `MinContent`, `MaxContent`, `Auto`, `FitContent(LengthPercentage)`, `Fraction(f32)` (the `fr` unit)
- `GridTrackRepetition`: `AutoFill`, `AutoFit`, `Count(u16)`

**Rust DSL API:**
```rust
impl Element {
    fn grid_template_columns(mut self, tracks: Vec<TrackSize>) -> Self;
    fn grid_template_rows(mut self, tracks: Vec<TrackSize>) -> Self;
}

// Convenience helpers (wrapping Taffy's style_helpers)
fn fr(n: f32) -> TrackSize;              // fractional unit
fn px(n: f32) -> TrackSize;              // fixed length
fn pct(n: f32) -> TrackSize;             // percentage
fn minmax(min: TrackMin, max: TrackMax) -> TrackSize;
fn repeat(count: u16, tracks: Vec<TrackSize>) -> TrackSize;
fn auto_fill(tracks: Vec<TrackSize>) -> TrackSize;
fn auto_fit(tracks: Vec<TrackSize>) -> TrackSize;
```

**Implementation:** Store as `Vec<TrackSizingFunction>` directly or create a wrapper type. Taffy already provides `fr()`, `minmax()`, `repeat()`, `evenly_sized_tracks()` in `style_helpers`.

---

### 2.3 grid-auto-rows / grid-auto-columns

| | Detail |
|---|---|
| **CSS Spec** | Size of implicitly-created grid tracks (when items are placed beyond explicit grid). |
| **Taffy API** | `Style.grid_auto_rows: Vec<NonRepeatedTrackSizingFunction>`, `Style.grid_auto_columns: Vec<NonRepeatedTrackSizingFunction>` |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have |

**Rust DSL API:**
```rust
impl Element {
    fn grid_auto_rows(mut self, tracks: Vec<TrackSize>) -> Self;
    fn grid_auto_columns(mut self, tracks: Vec<TrackSize>) -> Self;
}
```

---

### 2.4 grid-auto-flow

| | Detail |
|---|---|
| **CSS Spec** | `row` / `column` / `row dense` / `column dense`. Controls auto-placement algorithm. |
| **Taffy API** | `Style.grid_auto_flow: GridAutoFlow` with variants `Row`, `Column`, `RowDense`, `ColumnDense` |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have |

**Rust DSL API:**
```rust
impl Element {
    fn grid_auto_flow(mut self, flow: GridAutoFlow) -> Self;
}
```

---

### 2.5 grid-column / grid-row (placement)

| | Detail |
|---|---|
| **CSS Spec** | `grid-column-start` / `grid-column-end` / `grid-row-start` / `grid-row-end`. Values: `auto`, `<integer>`, `span <integer>`. |
| **Taffy API** | `Style.grid_column: Line<GridPlacement>` and `Style.grid_row: Line<GridPlacement>`. `GridPlacement` supports `Auto`, `Line(i16)`, `Span(u16)`. `Line` has `.start` and `.end` fields. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have (for grid) |

**Taffy helpers:** `line(i16)` creates `GridPlacement::Line`, `span(u16)` creates `GridPlacement::Span`.

**Rust DSL API:**
```rust
impl Element {
    fn grid_column(mut self, start: i16, end: i16) -> Self;
    fn grid_column_span(mut self, start: i16, span: u16) -> Self;
    fn grid_row(mut self, start: i16, end: i16) -> Self;
    fn grid_row_span(mut self, start: i16, span: u16) -> Self;
}
```

---

### 2.6 grid-template-areas

| | Detail |
|---|---|
| **CSS Spec** | Named grid areas defined as strings. |
| **Taffy API** | **Not supported.** Taffy does not implement named grid areas. |
| **Current Status** | Not available. |
| **Priority** | Won't implement. Named areas are syntactic sugar over line-based placement; use explicit line numbers instead. |

---

### 2.7 Grid alignment properties

Grid containers support additional alignment axes compared to flex:

| CSS Property | Taffy Field | Purpose |
|---|---|---|
| `justify-items` | `Style.justify_items: Option<AlignItems>` | Inline-axis alignment of all grid items within their cells |
| `justify-self` | `Style.justify_self: Option<AlignSelf>` | Inline-axis alignment of a single grid item |
| `align-items` | `Style.align_items` | Block-axis alignment of all grid items |
| `align-self` | `Style.align_self` | Block-axis alignment of a single grid item |
| `justify-content` | `Style.justify_content` | Alignment of the grid within the container (inline axis) |
| `align-content` | `Style.align_content` | Alignment of the grid within the container (block axis) |
| `place-items` | N/A (shorthand) | Shorthand for `align-items` + `justify-items` |
| `place-content` | N/A (shorthand) | Shorthand for `align-content` + `justify-content` |

**Current Status:** None of the grid-specific alignment properties are wired. `align-items` and `justify-content` exist but only for flex.

**Priority:** Must-have when grid is implemented. The DSL methods from flex (`align_items`, `justify`) work for grid too -- just need to also expose `justify_items` and `justify_self`.

**Rust DSL API:**
```rust
impl Element {
    fn justify_items(mut self, a: Alignment) -> Self;
    fn justify_self(mut self, a: Alignment) -> Self;
}
```

---

## Part 3: Box Model

### 3.1 width / height

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `<length>` / `<percentage>` / `min-content` / `max-content` / `fit-content` / `fit-content(<length>)` |
| **Taffy API** | `Style.size: Size<Dimension>`. `Dimension` variants: `Auto`, `Length(f32)`, `Percent(f32)`. For intrinsic sizes, Taffy supports `Dimension` via `min_content()`, `max_content()`, `fit_content(LengthPercentage)` helper functions. |
| **Current Status** | Only `Length` implemented. No `Auto`, `Percent`, or intrinsic sizing. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn width(mut self, w: f32) -> Self;         // exists (Length)
    fn height(mut self, h: f32) -> Self;        // exists (Length)
    fn width_pct(mut self, pct: f32) -> Self;   // add (Percent)
    fn height_pct(mut self, pct: f32) -> Self;  // add (Percent)
    fn width_auto(mut self) -> Self;            // add (Auto)
    fn height_auto(mut self) -> Self;           // add (Auto)
}
```

**Implementation:** Change `width: Option<f32>` and `height: Option<f32>` to a richer type, or use `Dimension` directly. Remove `fill_width`/`fill_height` in favor of `width_pct(1.0)` + `flex_grow(1.0)`.

---

### 3.2 min-width / min-height / max-width / max-height

| | Detail |
|---|---|
| **CSS Spec** | Same value types as width/height. Constrain the final computed size. |
| **Taffy API** | `Style.min_size: Size<Dimension>` and `Style.max_size: Size<Dimension>` |
| **Current Status** | Not implemented. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn min_width(mut self, w: f32) -> Self;
    fn min_height(mut self, h: f32) -> Self;
    fn max_width(mut self, w: f32) -> Self;
    fn max_height(mut self, h: f32) -> Self;
    fn min_width_pct(mut self, pct: f32) -> Self;
    fn min_height_pct(mut self, pct: f32) -> Self;
    fn max_width_pct(mut self, pct: f32) -> Self;
    fn max_height_pct(mut self, pct: f32) -> Self;
}
```

**Implementation:** Add `min_size` and `max_size` fields to `Element` (as `[Option<Dimension>; 2]` for width/height), map to `Style.min_size` and `Style.max_size`.

---

### 3.3 padding

| | Detail |
|---|---|
| **CSS Spec** | `<length>` / `<percentage>` per side. No `auto`. |
| **Taffy API** | `Style.padding: Rect<LengthPercentage>`. Supports `Length(f32)` and `Percent(f32)`. |
| **Current Status** | Implemented with `Length` only. No percentage support. |
| **Priority** | Implemented (percentage support is nice-to-have) |

No immediate changes needed. Percentage padding can be added later.

---

### 3.4 margin

| | Detail |
|---|---|
| **CSS Spec** | `<length>` / `<percentage>` / `auto` per side. `auto` margins are used for centering (e.g., `margin: 0 auto` centers horizontally). Margins can be negative. |
| **Taffy API** | `Style.margin: Rect<LengthPercentageAuto>`. Supports `Length(f32)`, `Percent(f32)`, `Auto`. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn margin(mut self, m: f32) -> Self;                       // uniform
    fn margin_xy(mut self, x: f32, y: f32) -> Self;           // horizontal, vertical
    fn margin_edges(mut self, t: f32, r: f32, b: f32, l: f32) -> Self;
    fn margin_x_auto(mut self) -> Self;                         // auto left+right (centering)
}
```

**Implementation:** Add `margin: [LengthPercentageAuto; 4]` to `Element` (default all zero), map to `Style.margin`.

---

### 3.5 border (layout)

| | Detail |
|---|---|
| **CSS Spec** | Border width contributes to element size (in border-box model). |
| **Taffy API** | `Style.border: Rect<LengthPercentage>`. This is the border *thickness* for layout purposes (not visual styling). |
| **Current Status** | Not wired to Taffy layout. The visual `Border` struct exists but does not affect layout calculations. |
| **Priority** | Must-have |

**Implementation:** When `element.border` is `Some(Border { width, .. })`, set `style.border` in Taffy to include that width on all sides. This ensures border width is accounted for in layout.

---

### 3.6 box-sizing

| | Detail |
|---|---|
| **CSS Spec** | `content-box` (default) / `border-box`. Controls whether `width`/`height` include padding and border. |
| **Taffy API** | `Style.box_sizing: BoxSizing` with variants `ContentBox`, `BorderBox` |
| **Current Status** | Not implemented. Taffy default depends on version. |
| **Priority** | Nice-to-have. Most UI frameworks default to `border-box` behavior. Consider defaulting to `BorderBox`. |

**Rust DSL API:**
```rust
impl Element {
    fn box_sizing(mut self, bs: BoxSizing) -> Self;
}
```

**Implementation:** Set `style.box_sizing = BoxSizing::BorderBox` as default in `element_to_taffy_style`, matching modern CSS best practice (`* { box-sizing: border-box }`).

---

### 3.7 aspect-ratio

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `<ratio>` (e.g., `16/9`). Maintains width/height ratio. Requires at least one dimension to be `auto`. |
| **Taffy API** | `Style.aspect_ratio: Option<f32>` -- the ratio as width/height. `None` = auto. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have |

**Rust DSL API:**
```rust
impl Element {
    fn aspect_ratio(mut self, ratio: f32) -> Self;  // e.g., 16.0/9.0
}
```

**Implementation:** Add `aspect_ratio: Option<f32>` to `Element`, map directly to `Style.aspect_ratio`.

---

## Part 4: Positioning

### 4.1 position

| | Detail |
|---|---|
| **CSS Spec** | `static` (default) / `relative` / `absolute` / `fixed` / `sticky` |
| **Taffy API** | `Style.position: Position` with variants `Relative` (default) and `Absolute`. Taffy does NOT support `fixed` or `sticky`. |
| **Current Status** | Not implemented. All elements use Taffy's default (`Relative`). |
| **Priority** | Must-have (`relative`, `absolute`); custom implementation needed for `fixed`; `sticky` is nice-to-have |

**How Taffy handles positioning:**
- `Position::Relative`: Element participates in normal layout flow. `inset` offsets are applied as a visual correction after layout (element still reserves its original space).
- `Position::Absolute`: Element is removed from the flow. Positioned relative to its nearest positioned ancestor (or the root). Size determined by `inset` values and/or explicit `size`.

**What Taffy does NOT handle:**
- `fixed`: Must be implemented at the framework level. Fixed elements are positioned relative to the viewport, ignoring scroll. Implementation: maintain a separate list of fixed-position elements, lay them out against the viewport dimensions, render them last.
- `sticky`: Must be implemented at the framework level. Requires scroll position awareness to toggle between relative and fixed behavior. Implementation: during rendering, compute whether the element has scrolled past its sticky threshold and adjust its rendered position.

**Rust DSL API:**
```rust
impl Element {
    fn position_relative(mut self) -> Self;
    fn position_absolute(mut self) -> Self;
    fn position_fixed(mut self) -> Self;    // custom
    fn inset(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self;
    fn top(mut self, v: f32) -> Self;
    fn right(mut self, v: f32) -> Self;
    fn bottom(mut self, v: f32) -> Self;
    fn left(mut self, v: f32) -> Self;
}
```

---

### 4.2 inset (top / right / bottom / left)

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `<length>` / `<percentage>`. Offset from containing block edges for positioned elements. |
| **Taffy API** | `Style.inset: Rect<LengthPercentageAuto>` with per-side `top`, `right`, `bottom`, `left` fields. Default `Auto` on all sides. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have (required for absolute positioning) |

**Implementation:** Add `inset: [Option<f32>; 4]` to `Element` (or use `LengthPercentageAuto` directly). Map to `Style.inset`.

---

### 4.3 Absolute positioning inside flex containers

Per CSS spec and Taffy behavior: absolutely positioned children of a flex container are removed from the flex flow. They do not participate in flex sizing or alignment. The flex container serves as the containing block if it has `position: relative`.

**Blink/Chromium behavior:**
- Absolutely positioned flex children are laid out after all in-flow children
- They can still use `align-self` and `justify-self` for placement within the container
- Their `order` property is ignored
- `margin: auto` on absolute children absorbs free space (centering trick)

**Implementation notes:**
- Taffy handles this correctly for `Position::Absolute` -- no special work needed
- Ensure the `extract_layout` function handles the different coordinate space for absolute children (Taffy computes their position relative to the containing block)

---

## Part 5: Stacking and Z-Index

### 5.1 z-index

| | Detail |
|---|---|
| **CSS Spec** | `auto` / `<integer>`. Only applies to positioned elements. Creates stacking contexts. |
| **Taffy API** | **Not supported.** Taffy is a layout engine, not a rendering engine. Z-index is a rendering concern. |
| **Current Status** | Not implemented. |
| **Priority** | Must-have |

**Custom Implementation Required.**

**CSS painting order within a stacking context (simplified):**
1. Background and borders of the stacking context root
2. Positioned descendants with negative z-index (back to front)
3. Non-positioned block-level descendants (in tree order)
4. Non-positioned float descendants
5. Inline-level descendants
6. Positioned descendants with `z-index: auto` or `z-index: 0` (tree order)
7. Positioned descendants with positive z-index (back to front)

**Proposed approach for the framework:**

Since this is a UI toolkit (not a browser), simplify to:
1. Add `z_index: Option<i32>` to `Element`
2. During display list generation (`display_list.rs`), sort children by z-index before emitting draw commands
3. Elements without z-index render in tree order
4. Each element with an explicit z-index creates a stacking context (its children are painted as a group relative to siblings)

**Rust DSL API:**
```rust
impl Element {
    fn z_index(mut self, z: i32) -> Self;
}
```

**Implementation phases:**
1. Add z-index field to Element
2. In display list builder, partition children into: negative z-index, no z-index, positive z-index
3. Render in order: negative (sorted), normal (tree order), positive (sorted)
4. Each z-indexed group is clipped/transformed as a unit

---

### 5.2 Stacking context triggers

In CSS, stacking contexts are created by:
- `position: relative/absolute` + `z-index` not `auto`
- `position: fixed/sticky` (always)
- `opacity` < 1
- `transform`, `filter`, `backdrop-filter` (any non-none value)
- `clip-path`
- `will-change`

For the framework, stacking contexts should be created by:
- Explicit `z-index` value
- `opacity` < 1.0 (already renders as a group via Skia layer)
- Clip containers (already group children)
- Scroll containers (already group children)

This is simpler than the full CSS model and sufficient for a UI toolkit.

---

## Part 6: Overflow and Scrolling

### 6.1 overflow

| | Detail |
|---|---|
| **CSS Spec** | `visible` / `hidden` / `clip` / `scroll` / `auto`, per-axis (`overflow-x`, `overflow-y`). |
| **Taffy API** | `Style.overflow: Point<Overflow>` with per-axis values. Variants: `Visible`, `Clip`, `Hidden`, `Scroll`. |
| **Current Status** | Not wired to Taffy. The `clip` field on `Element` controls rendering clipping but does not inform Taffy's layout algorithm. Scroll containers exist but overflow mode is not set in Taffy. |
| **Priority** | Must-have |

**Why this matters for layout:** Overflow mode affects:
- **Minimum size calculation**: `overflow: visible` uses content size as automatic minimum; `overflow: hidden/scroll` uses 0
- **Scrollbar space**: `overflow: scroll` reserves space for scrollbar (via `scrollbar_width`)

**Taffy `Overflow` enum semantics:**
- `Visible`: Content overflows, contributes to parent scroll. Automatic min size = content size.
- `Clip`: Content clipped, does not contribute to parent scroll. Automatic min size = content size.
- `Hidden`: Content clipped, does not contribute to parent scroll. Automatic min size = 0.
- `Scroll`: Like Hidden, but reserves scrollbar space.

**Rust DSL API:**
```rust
impl Element {
    fn overflow(mut self, o: Overflow) -> Self;           // both axes
    fn overflow_x(mut self, o: Overflow) -> Self;
    fn overflow_y(mut self, o: Overflow) -> Self;
}

enum Overflow {
    Visible,
    Hidden,
    Clip,
    Scroll,
}
```

**Implementation:**
- Add `overflow_x` and `overflow_y` to `Element`
- Map to `Style.overflow` point
- For scroll containers, set `Overflow::Scroll` and configure `scrollbar_width` if showing scrollbars
- Integrate with existing `scroll_direction` logic -- scroll containers should set `overflow: scroll` on their scroll axis and `overflow: hidden` on the other axis

---

### 6.2 scrollbar-width

| | Detail |
|---|---|
| **CSS Spec** | Not a CSS property directly. In Taffy, `scrollbar_width: f32` reserves space on the cross axis for a scrollbar. |
| **Taffy API** | `Style.scrollbar_width: f32` (default 0.0) |
| **Current Status** | Not implemented. Current scroll containers use overlay indicators that do not affect layout. |
| **Priority** | Nice-to-have. Only needed if implementing non-overlay scrollbars. |

---

## Part 7: Display Modes

### 7.1 display

| | Detail |
|---|---|
| **CSS Spec** | `block` / `flex` / `grid` / `inline` / `inline-flex` / `inline-grid` / `none` / `contents` |
| **Taffy API** | `Style.display: Display` with variants: `Block`, `Flex`, `Grid`, `None`. No `inline`, `contents`. |
| **Current Status** | Hardcoded to `Flex`. |
| **Priority** | Must-have (`None`, `Grid`); Nice-to-have (`Block`) |

**What each mode means for the framework:**

- **`Flex`** (current default): Children laid out via flexbox algorithm. This is appropriate for most UI components.
- **`Grid`**: Children laid out via CSS Grid algorithm. Needed for dashboard layouts, form layouts, etc.
- **`Block`**: Children laid out via block flow (vertical stacking, full width). Less useful in a UI toolkit since `flex` with `direction: column` covers most cases.
- **`None`**: Element and its children are not rendered and take no space. Useful for conditional rendering.
- **`inline`/`contents`**: Not applicable to this framework. Elements are always block-level boxes.

**Rust DSL API:**
```rust
impl Element {
    fn hidden(mut self) -> Self;  // display: none
    fn visible(mut self) -> Self; // undo hidden
}
```

**Implementation:** Add `display: DisplayMode` to `Element` with variants `Flex`, `Grid`, `None`. Map in `element_to_taffy_style`.

---

## Part 8: Intrinsic Sizing

### 8.1 min-content / max-content / fit-content

| | Detail |
|---|---|
| **CSS Spec** | Intrinsic sizing keywords for width/height. `min-content` = smallest size without overflow. `max-content` = ideal size given infinite space. `fit-content` = clamp between min-content and max-content. |
| **Taffy API** | Taffy prelude exports `min_content()`, `max_content()`, `fit_content(LengthPercentage)` helper functions that return `Dimension` values. These work in `size`, `min_size`, `max_size`. |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have |

**How Blink handles intrinsic sizing:**
- `min-content`: Perform layout with width = 0, measure overflow. For text, this is the width of the longest word.
- `max-content`: Perform layout with width = infinity, measure result. For text, this is the full unwrapped line width.
- `fit-content`: `min(max-content, max(min-content, available-width))`

**Taffy handles this via the measure callback.** Our text measurement function already supports this -- Taffy calls the measure function with different `AvailableSpace` values to determine intrinsic sizes. The current text measure function is simplistic (char_width * len) but directionally correct.

**Rust DSL API:**
```rust
impl Element {
    fn width_min_content(mut self) -> Self;
    fn width_max_content(mut self) -> Self;
    fn width_fit_content(mut self, limit: f32) -> Self;
    // same for height
}
```

**Implementation:** Extend the dimension setting methods to accept Taffy's intrinsic dimension values. The text measure callback should be improved to do proper text shaping for accurate min-content/max-content.

---

## Part 9: Writing Modes and Direction

### 9.1 direction (ltr/rtl)

| | Detail |
|---|---|
| **CSS Spec** | `ltr` / `rtl`. Affects inline direction, flex-start/end meaning, and margin/padding logical properties. |
| **Taffy API** | Not directly in `Style`. Taffy uses physical properties (left/right) not logical ones (start/end). The `Start`/`End` variants of alignment enums are physical-direction-aware in the context of flex-direction. |
| **Current Status** | Not implemented. |
| **Priority** | Nice-to-have. Can be deferred until internationalization is needed. |

**Implementation approach:** When RTL is needed, swap left/right padding/margin/inset before passing to Taffy, and use `FlexDirection::RowReverse` instead of `Row` for RTL default flow.

---

## Part 10: Multi-Column Layout

| | Detail |
|---|---|
| **CSS Spec** | `columns`, `column-count`, `column-width`, `column-gap`, `column-rule`, `column-span`. |
| **Taffy API** | **Not supported.** |
| **Current Status** | Not implemented. |
| **Priority** | Won't implement. Multi-column layout is primarily for text-heavy document rendering. CSS Grid covers the use cases relevant to a UI toolkit. |

---

## Part 11: Comparison with Flutter's Layout Model

Flutter uses a constraint-based single-pass layout where constraints flow down and sizes flow up. Key differences from our CSS-based approach:

| Concept | Flutter | Our Framework (CSS/Taffy) |
|---|---|---|
| Layout model | BoxConstraints (minW, maxW, minH, maxH) | CSS box model with flex/grid algorithms |
| Absolute positioning | `Stack` + `Positioned` widgets | `position: absolute` + inset |
| Flex layout | `Row`, `Column`, `Flex` with `Expanded`/`Flexible` | `display: flex` with flex-grow/shrink/basis |
| Grid layout | Custom via `GridView` / `Wrap` | `display: grid` with full CSS Grid |
| Intrinsic sizing | `IntrinsicWidth`/`IntrinsicHeight` (expensive extra pass) | `min-content`/`max-content` (handled by Taffy) |
| Overflow | `ClipRect`, `OverflowBox`, `UnconstrainedBox` | `overflow: hidden/scroll/clip` |
| Z-ordering | `Stack` widget paint order, no z-index | `z-index` property on positioned elements |

**Takeaway:** Our CSS-based model via Taffy is more expressive than Flutter's constraint model for complex layouts, but Flutter's single-pass approach is more predictable. Taffy handles the complexity internally.

---

## Implementation Order

### Phase 1: Core Box Model (estimated: 2-3 days)
1. **Margin** -- add to Element, wire to Taffy
2. **Min/max size** -- add min_width, min_height, max_width, max_height
3. **Aspect ratio** -- add to Element, wire to Taffy
4. **Percentage dimensions** -- width_pct, height_pct
5. **Box-sizing** -- default to border-box, wire border width to Taffy layout
6. **Display: None** -- hide elements from layout

### Phase 2: Complete Flexbox (estimated: 2 days)
1. **flex-shrink** -- add DSL method
2. **flex-basis** -- add DSL method
3. **flex-wrap** -- add to Element, wire to Taffy
4. **align-self** -- add to Element, wire to Taffy
5. **align-content** -- add to Element, wire to Taffy
6. **Direction reverse** -- add RowReverse, ColumnReverse
7. **SpaceAround, SpaceEvenly** -- add to Justify enum
8. **Stretch, Baseline** -- add to Alignment enum
9. **Separate row-gap/column-gap**

### Phase 3: Positioning (estimated: 3-4 days)
1. **Position relative + inset** -- wire to Taffy, element_to_taffy_style
2. **Position absolute** -- wire to Taffy, test coordinate extraction
3. **Z-index** -- add to Element, implement stacking context sort in display list
4. **Position fixed** -- custom implementation, separate render pass against viewport

### Phase 4: Overflow Integration (estimated: 1-2 days)
1. Wire `overflow` to Taffy's `Style.overflow`
2. Integrate with scroll container detection
3. Set `scrollbar_width` for scroll containers if needed
4. Ensure overflow affects min-size calculation

### Phase 5: CSS Grid (estimated: 3-4 days)
1. **display: grid** -- add Grid display mode
2. **grid-template-columns/rows** -- DSL API with helper functions
3. **grid-column/row placement** -- line and span placement
4. **Grid alignment** -- justify-items, justify-self
5. **grid-auto-flow, grid-auto-rows/columns**

### Phase 6: Polish (estimated: 2 days)
1. **Intrinsic sizing** -- min-content, max-content, fit-content DSL
2. **Improved text measurement** -- proper text shaping for intrinsic sizes
3. **Order** -- child sorting by order value
4. **RTL support** -- if needed

---

## Appendix A: Complete Taffy Style Field Reference

Every field on `taffy::style::Style` and whether we use it:

| Taffy Field | Type | Used? | Plan |
|---|---|---|---|
| `display` | `Display` | No (hardcoded Flex) | Phase 1 (None), Phase 5 (Grid) |
| `item_is_table` | `bool` | No | Won't use |
| `box_sizing` | `BoxSizing` | No | Phase 1 |
| `overflow` | `Point<Overflow>` | No | Phase 4 |
| `scrollbar_width` | `f32` | No | Phase 4 |
| `position` | `Position` | No | Phase 3 |
| `inset` | `Rect<LengthPercentageAuto>` | No | Phase 3 |
| `size` | `Size<Dimension>` | Partial (Length only) | Phase 1 (Percent, Auto) |
| `min_size` | `Size<Dimension>` | No | Phase 1 |
| `max_size` | `Size<Dimension>` | No | Phase 1 |
| `aspect_ratio` | `Option<f32>` | No | Phase 1 |
| `margin` | `Rect<LengthPercentageAuto>` | No | Phase 1 |
| `padding` | `Rect<LengthPercentage>` | Yes (Length only) | Done (Percent later) |
| `border` | `Rect<LengthPercentage>` | No | Phase 1 |
| `align_items` | `Option<AlignItems>` | Partial | Phase 2 (Stretch, Baseline) |
| `align_self` | `Option<AlignSelf>` | No | Phase 2 |
| `justify_items` | `Option<AlignItems>` | No | Phase 5 (Grid) |
| `justify_self` | `Option<AlignSelf>` | No | Phase 5 (Grid) |
| `align_content` | `Option<AlignContent>` | No | Phase 2 |
| `justify_content` | `Option<JustifyContent>` | Partial | Phase 2 (SpaceAround, SpaceEvenly) |
| `gap` | `Size<LengthPercentage>` | Partial (uniform only) | Phase 2 (separate row/col) |
| `text_align` | `TextAlign` | No | Won't use (we handle text rendering directly) |
| `flex_direction` | `FlexDirection` | Partial | Phase 2 (reverse variants) |
| `flex_wrap` | `FlexWrap` | No | Phase 2 |
| `flex_basis` | `Dimension` | No | Phase 2 |
| `flex_grow` | `f32` | Yes | Done |
| `flex_shrink` | `f32` | No | Phase 2 |
| `grid_template_rows` | `Vec<TrackSizingFunction>` | No | Phase 5 |
| `grid_template_columns` | `Vec<TrackSizingFunction>` | No | Phase 5 |
| `grid_auto_rows` | `Vec<NonRepeatedTrackSizingFunction>` | No | Phase 5 |
| `grid_auto_columns` | `Vec<NonRepeatedTrackSizingFunction>` | No | Phase 5 |
| `grid_auto_flow` | `GridAutoFlow` | No | Phase 5 |
| `grid_row` | `Line<GridPlacement>` | No | Phase 5 |
| `grid_column` | `Line<GridPlacement>` | No | Phase 5 |

---

## Appendix B: CSS Properties NOT in Taffy (Custom Implementation Needed)

| CSS Property | Custom Work Required |
|---|---|
| `z-index` | Sort children in display list by z-index |
| `position: fixed` | Separate layout pass against viewport; render on top of scroll |
| `position: sticky` | Track scroll position; toggle between relative and fixed offset during render |
| `display: inline`, `inline-block`, `inline-flex`, `inline-grid` | Not needed for UI toolkit |
| `display: contents` | Not needed |
| `float` | Not needed for UI toolkit |
| `columns` (multi-column) | Not needed |
| `grid-template-areas` (named areas) | Not needed; use line-based placement |
| `writing-mode` / `direction: rtl` | Swap left/right in our mapping layer |
| `order` | Sort children before building Taffy tree |

---

## Appendix C: Element Struct Changes Summary

New fields to add to `Element`:

```rust
pub struct Element {
    // Existing fields...

    // New layout fields (Phase 1: Box Model)
    pub margin: [f32; 4],                    // top, right, bottom, left
    pub margin_auto: [bool; 4],              // per-side auto margin
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub display: DisplayMode,                // Flex, Grid, None

    // New layout fields (Phase 2: Flexbox)
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dimension>,
    pub flex_wrap: FlexWrap,
    pub align_self: Option<Alignment>,
    pub align_content: Option<AlignContent>,

    // New layout fields (Phase 3: Positioning)
    pub position: PositionType,              // Relative, Absolute, Fixed
    pub inset: [Option<f32>; 4],             // top, right, bottom, left
    pub z_index: Option<i32>,

    // New layout fields (Phase 4: Overflow)
    pub overflow_x: OverflowMode,
    pub overflow_y: OverflowMode,

    // New layout fields (Phase 5: Grid)
    pub grid_template_columns: Vec<TrackSizingFunction>,
    pub grid_template_rows: Vec<TrackSizingFunction>,
    pub grid_column: Option<(GridPlacement, GridPlacement)>,
    pub grid_row: Option<(GridPlacement, GridPlacement)>,
    pub grid_auto_flow: GridAutoFlow,
    pub grid_auto_columns: Vec<NonRepeatedTrackSizingFunction>,
    pub grid_auto_rows: Vec<NonRepeatedTrackSizingFunction>,
    pub justify_items: Option<Alignment>,
    pub justify_self: Option<Alignment>,
}
```
