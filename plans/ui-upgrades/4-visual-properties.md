# Visual Properties Implementation Plan

## Overview

This document covers every CSS visual/paint property needed for a comprehensive UI toolkit, mapped to exact Skia APIs via the `skia_safe` Rust crate. The framework already handles solid backgrounds, uniform border-radius, single-color borders (stroke only), uniform box-shadow, opacity layers, backdrop blur, clipping, SVG/image rendering, and canvas translate. This plan covers everything else.

Layout properties (width, height, padding, margin, flexbox, grid) are handled by Taffy and are out of scope. This is purely about painting and visual presentation.

### Current Rendering Pipeline

```
Element tree + LayoutNode tree
    -> display_list.rs: emit DrawCommands
    -> renderer.rs: SkiaRenderer.execute() iterates commands, calls Skia
```

Each `DrawCommand` variant maps to one or more Skia canvas calls. New visual properties require: (1) new fields on `Element`, (2) new `DrawCommand` variants or extended existing ones, (3) new Skia rendering code in `SkiaRenderer`.

---

## Property Categories

### 1. Backgrounds

#### 1.1 Background Color (EXISTING)

Already implemented. `Element.background: Option<Color>` renders via `Paint::set_color` + `Canvas::draw_rrect`.

#### 1.2 Background Gradient

**CSS Spec**: `background-image: linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`, and repeating variants. Gradients define color transitions along a line, from a center point, or around a center point. Color stops have positions (percentages or lengths). Multiple backgrounds stack with the first listed on top.

**Skia Implementation**:

Skia gradients are shaders set on a `Paint` via `paint.set_shader(shader)`.

- **Linear gradient**: `skia_safe::gradient_shader::linear(points, colors, positions, tile_mode)`
  - `points`: `(Point, Point)` — start and end coordinates (computed from element bounds + angle/direction)
  - `colors`: `GradientShaderColors` — `&[Color]` or `&[Color4f]` with color space
  - `positions`: `Option<&[f32]>` — stop positions in [0.0, 1.0], `None` for even distribution
  - `tile_mode`: `TileMode` — `Clamp`, `Repeat`, `Mirror`, `Decal`
  - For CSS angles: `to right` = 90deg, `to bottom` = 180deg (default). Convert angle to start/end points: start = center - direction * half_diagonal, end = center + direction * half_diagonal

- **Radial gradient**: `skia_safe::gradient_shader::radial(center, radius, colors, positions, tile_mode)`
  - `center`: `Point` — center of gradient (default: center of element)
  - `radius`: `f32` — radius of the gradient circle
  - CSS `closest-side`, `farthest-corner` etc. require computing radius from element bounds
  - For elliptical gradients, use a matrix transform on the shader: `shader.with_local_matrix(&matrix)`

- **Conic/Sweep gradient**: `skia_safe::gradient_shader::sweep(center, colors, positions, tile_mode, start_angle, end_angle)`
  - `center`: `Point`
  - `start_angle`, `end_angle`: `Option<f32>` in degrees
  - CSS `conic-gradient(from 45deg, ...)` maps to `start_angle = 45.0`

- **Two-point conical**: `skia_safe::gradient_shader::two_point_conical(start, start_radius, end, end_radius, colors, positions, tile_mode)` — used for focal radial gradients where the focal point differs from the center

- **Repeating gradients**: Set `tile_mode` to `TileMode::Repeat`. The color stops must not span the full [0, 1] range — Skia repeats the defined stop range automatically.

**Rust API Design**:

```rust
#[derive(Debug, Clone)]
pub enum Gradient {
    Linear {
        angle_deg: f32,  // 0 = to top, 90 = to right, 180 = to bottom (CSS default)
        stops: Vec<ColorStop>,
    },
    Radial {
        center: Option<(f32, f32)>,  // None = element center; values are 0.0-1.0 fractions
        radius: GradientRadius,
        stops: Vec<ColorStop>,
    },
    Conic {
        center: Option<(f32, f32)>,
        from_angle_deg: f32,
        stops: Vec<ColorStop>,
    },
}

#[derive(Debug, Clone)]
pub struct ColorStop {
    pub color: Color,
    pub position: Option<f32>,  // 0.0–1.0; None = auto-distribute
}

#[derive(Debug, Clone)]
pub enum GradientRadius {
    Explicit(f32),
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
}

// Element DSL
impl Element {
    pub fn background_gradient(mut self, gradient: Gradient) -> Self { ... }
}
```

For repeating variants, add a `repeating: bool` field to each gradient variant.

**Edge Cases**:
- CSS angle 0deg = "to top" (up), but Skia's linear gradient uses point pairs, not angles — must convert: `start = (cx + sin(angle) * h/2, cy + cos(angle) * h/2)`, `end = (cx - sin(angle) * h/2, cy - cos(angle) * h/2)` (note: CSS angles are clockwise from top)
- Gradient stops without explicit positions must be auto-distributed evenly (CSS spec requirement)
- Two stops at the same position create a hard edge (no transition)
- Elliptical radial gradients need a scale matrix since Skia's `radial()` only does circles
- Conic gradients with stops beyond 360deg wrap around
- `background-clip: text` with gradients requires clipping to text glyphs (advanced, see 1.6)

**Priority**: Must-have (gradients are essential for modern UI)

#### 1.3 Background Image

**CSS Spec**: `background-image: url(...)` draws a raster or vector image as the element background. Combined with `background-size`, `background-position`, `background-repeat`.

**Skia Implementation**: Load image to `skia_safe::Image`, create a shader via `image.to_shader(tile_mode_x, tile_mode_y, sampling, local_matrix)`. Set on paint with `paint.set_shader(shader)`. The local matrix handles positioning and sizing.

- `TileMode::Clamp` for `no-repeat` (pixels at edge extend)
- `TileMode::Repeat` for `repeat`
- `TileMode::Mirror` for (no CSS equivalent but useful)
- `TileMode::Decal` for `no-repeat` with transparent outside

For `background-size: cover/contain`, compute scale factors from image dimensions vs element bounds, then build a local matrix with translation + scale.

**Rust API Design**:

```rust
pub enum BackgroundSize {
    Cover,
    Contain,
    Explicit(Option<f32>, Option<f32>),  // width, height; None = auto
}

pub enum BackgroundRepeat {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    Space,
    Round,
}

impl Element {
    pub fn background_image(mut self, source: ImageSource) -> Self { ... }
    pub fn background_size(mut self, size: BackgroundSize) -> Self { ... }
    pub fn background_position(mut self, x: f32, y: f32) -> Self { ... }  // 0.0-1.0 fractions
    pub fn background_repeat(mut self, repeat: BackgroundRepeat) -> Self { ... }
}
```

**Edge Cases**:
- `Space` repeat mode requires computing the integer tile count and distributing remainder as gaps — not directly supported by Skia, must tile manually
- `Round` repeat mode scales tiles to fit evenly — compute scale factor, apply via local matrix
- SVG backgrounds should be rasterized at the target size for sharpness
- Multiple backgrounds: CSS allows stacking multiple `background-image` values; store as `Vec<BackgroundLayer>`

**Priority**: Nice-to-have (images as backgrounds are less common in app UIs than gradients)

#### 1.4 Background Clip

**CSS Spec**: `background-clip: border-box | padding-box | content-box | text`
Controls where the background is visible.

**Skia Implementation**: Before drawing the background, push a clip that matches the desired box:
- `border-box` (default): clip to border edge (the full element bounds) — already the current behavior
- `padding-box`: clip to bounds inset by border width
- `content-box`: clip to bounds inset by border width + padding
- `text`: clip to text glyph outlines — use `canvas.clip_path(text_path, ClipOp::Intersect, true)` where `text_path` is obtained from `Font::text_to_path()` or `TextBlob` converted to path

**Rust API Design**:
```rust
pub enum BackgroundClip {
    BorderBox,
    PaddingBox,
    ContentBox,
    Text,
}

impl Element {
    pub fn background_clip(mut self, clip: BackgroundClip) -> Self { ... }
}
```

**Edge Cases**:
- `text` clip requires knowing the exact glyph positions — must coordinate with text layout
- When combined with border-radius, the clip must use the inner radius (radius - border_width) for padding-box

**Priority**: Nice-to-have

#### 1.5 Background Origin

**CSS Spec**: `background-origin: border-box | padding-box | content-box`
Defines the coordinate origin for `background-position` and `background-size` calculations.

**Skia Implementation**: Adjust the bounds rectangle used to compute the gradient or image shader's local matrix. For `padding-box`, inset by border width. For `content-box`, inset by border + padding.

**Rust API Design**:
```rust
pub enum BackgroundOrigin {
    BorderBox,
    PaddingBox,
    ContentBox,
}

impl Element {
    pub fn background_origin(mut self, origin: BackgroundOrigin) -> Self { ... }
}
```

**Edge Cases**: Only matters when `background-position` or `background-size` are used. Default is `padding-box`.

**Priority**: Can-skip (rarely needed in app UIs)

#### 1.6 Multiple Backgrounds

**CSS Spec**: Multiple comma-separated backgrounds stack with the first on top.

**Skia Implementation**: Draw backgrounds in reverse order (last listed = bottom). Each background is a separate `draw_rrect` call with its own shader/color.

**Rust API Design**:
```rust
pub struct BackgroundLayer {
    pub color: Option<Color>,
    pub gradient: Option<Gradient>,
    pub image: Option<ImageSource>,
    pub size: BackgroundSize,
    pub position: (f32, f32),
    pub repeat: BackgroundRepeat,
    pub clip: BackgroundClip,
    pub origin: BackgroundOrigin,
    pub blend_mode: Option<BlendMode>,
}

impl Element {
    pub fn background_layers(mut self, layers: Vec<BackgroundLayer>) -> Self { ... }
}
```

**Priority**: Nice-to-have

---

### 2. Borders

#### 2.1 Per-Side Border Width

**CSS Spec**: `border-top-width`, `border-right-width`, `border-bottom-width`, `border-left-width`. Each can be a length or keyword (`thin` = 1px, `medium` = 3px, `thick` = 5px).

**Skia Implementation**: The current approach draws borders as a single stroke on the RRect. Per-side borders require a different approach:

Option A (Chromium approach): Draw each side as a filled trapezoid/polygon path. For a top border of width `w`:
```
Path: (x, y) -> (x + width, y) -> (x + width - br_tr, y + w) -> (x + br_tl, y + w) -> close
```
where `br_tl`, `br_tr` are the inner corner radii. Use `Canvas::draw_path` with `PaintStyle::Fill`.

Option B: Draw 4 separate `draw_line` calls with appropriate stroke width and cap, but this leaves ugly corners.

Option C (recommended): Use `Canvas::draw_drrect` (draw between two rounded rects) clipped to each side region. `canvas.draw_drrect(outer_rrect, inner_rrect, &paint)` fills the border ring, then clip to each side's quadrant to color them individually.

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub struct BorderSide {
    pub width: f32,
    pub color: Color,
    pub style: BorderStyle,
}

#[derive(Debug, Clone)]
pub struct FullBorder {
    pub top: Option<BorderSide>,
    pub right: Option<BorderSide>,
    pub bottom: Option<BorderSide>,
    pub left: Option<BorderSide>,
}

impl Element {
    // Existing uniform border
    pub fn border(mut self, color: Color, width: f32) -> Self { ... }
    // Per-side borders
    pub fn border_top(mut self, color: Color, width: f32) -> Self { ... }
    pub fn border_right(mut self, color: Color, width: f32) -> Self { ... }
    pub fn border_bottom(mut self, color: Color, width: f32) -> Self { ... }
    pub fn border_left(mut self, color: Color, width: f32) -> Self { ... }
}
```

**Edge Cases**:
- Different widths on adjacent sides create mitered joins at corners — the standard CSS behavior is to draw diagonal joins
- Zero-width sides should not render
- Border radius interacts with per-side widths: inner radius = max(0, outer_radius - border_width)
- Chromium draws borders as filled shapes (not strokes) to handle the miter correctly

**Priority**: Must-have

#### 2.2 Border Style

**CSS Spec**: `border-style: none | solid | dashed | dotted | double | groove | ridge | inset | outset`

**Skia Implementation**:
- **solid**: Current implementation — `PaintStyle::Stroke` (or filled shape for per-side)
- **dashed**: Use `PathEffect::dash(&intervals, phase)` on the paint. `intervals = [dash_length, gap_length]`. Typical: `[3*width, 3*width]`
- **dotted**: Use `PathEffect::dash(&[0.0, gap], 0.0)` with `StrokeCap::Round` and `stroke_width = border_width`. The zero-length dash with round cap produces dots.
- **double**: Draw two parallel strokes, each 1/3 of total border width, with 1/3 gap between. Two `draw_rrect` calls: outer at original bounds with width/3 stroke, inner inset by 2*width/3 with width/3 stroke.
- **groove**: Two-color illusion. Top/left half drawn darker, bottom/right half drawn lighter (then reversed for inner half). Use `canvas.draw_drrect` with clip regions and two colors.
- **ridge**: Inverse of groove.
- **inset**: Top/left sides darker, bottom/right sides lighter.
- **outset**: Inverse of inset.

For `dash()` in skia_safe:
```rust
use skia_safe::PathEffect;
let intervals = [dash_len, gap_len];
let dash_effect = PathEffect::dash(&intervals, 0.0);
paint.set_path_effect(dash_effect);
```

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl Element {
    pub fn border_style(mut self, style: BorderStyle) -> Self { ... }
    // Or per-side via BorderSide struct
}
```

**Edge Cases**:
- `dotted` with `PathEffect::dash` and round caps can produce slightly non-circular dots at corners — acceptable
- `double` needs minimum 3px width to be visible (each line = 1px, gap = 1px)
- 3D styles (groove, ridge, inset, outset) compute lighter/darker variants of the border color — use `color.lighter(0.3)` and `color.darker(0.3)` helpers
- `none` means no border and 0 computed width (different from `hidden` in table context, which we can ignore)

**Priority**: Must-have (at least solid, dashed, dotted; others nice-to-have)

#### 2.3 Per-Corner Border Radius

**CSS Spec**: `border-top-left-radius`, `border-top-right-radius`, `border-bottom-right-radius`, `border-bottom-left-radius`. Each accepts one value (circular) or two values (elliptical: horizontal/vertical).

**Skia Implementation**: Replace `RRect::new_rect_xy(rect, rx, ry)` with `RRect::new_rect_radii(rect, radii)` where `radii` is `[Point; 4]` — one `(rx, ry)` pair for each corner (top-left, top-right, bottom-right, bottom-left).

```rust
let radii = [
    skia_safe::Point::new(tl_rx, tl_ry),  // top-left
    skia_safe::Point::new(tr_rx, tr_ry),  // top-right
    skia_safe::Point::new(br_rx, br_ry),  // bottom-right
    skia_safe::Point::new(bl_rx, bl_ry),  // bottom-left
];
let rrect = RRect::new_rect_radii(rect, &radii);
```

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Element {
    // Existing uniform radius
    pub fn rounded(mut self, r: f32) -> Self { ... }
    // Per-corner
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self { ... }
    // Elliptical (stretch one axis)
    pub fn rounded_elliptical(mut self, rx: f32, ry: f32) -> Self { ... }
}
```

Internally, change `Element.corner_radius: f32` to `Element.corner_radii: CornerRadii` (or keep both and convert uniform to per-corner internally).

**Edge Cases**:
- CSS requires radii to be scaled down proportionally if the sum of adjacent radii exceeds the side length — `RRect` handles this automatically via Skia's internal clamping
- Elliptical radii (different rx/ry) are rarely used but fully supported by `RRect::new_rect_radii`

**Priority**: Must-have

#### 2.4 Border Image

**CSS Spec**: `border-image-source`, `border-image-slice`, `border-image-width`, `border-image-outset`, `border-image-repeat`. Slices an image into 9 regions and tiles/stretches them around the border.

**Skia Implementation**: Use `Canvas::draw_image_nine(image, center_rect, dst_rect, filter_mode, paint)`. The `center_rect` is the interior rectangle (the 9-slice center). Skia stretches corners, tiles/stretches edges, and tiles/stretches the center. Alternatively, manually draw 9 `draw_image_rect` calls for full control.

For `draw_image_nine`:
```rust
let center = skia_safe::IRect::from_ltrb(left, top, right, bottom);
canvas.draw_image_nine(image, &center, dst_rect, filter_mode, &paint);
```

**Rust API Design**:
```rust
pub struct BorderImage {
    pub source: ImageSource,
    pub slice: [f32; 4],       // top, right, bottom, left (in source image pixels)
    pub width: Option<[f32; 4]>, // None = use border-width
    pub outset: [f32; 4],
    pub repeat: BorderImageRepeat,
    pub fill: bool,            // fill interior
}

pub enum BorderImageRepeat {
    Stretch,
    Repeat,
    Round,
    Space,
}

impl Element {
    pub fn border_image(mut self, image: BorderImage) -> Self { ... }
}
```

**Edge Cases**:
- `round` repeat mode requires computing integer tile count and scaling
- `space` repeat mode requires computing tile count and distributing gaps
- border-image replaces border-style when present
- SVG sources should be rasterized at appropriate resolution

**Priority**: Can-skip (rarely used in app UIs)

---

### 3. Box Shadows

#### 3.1 Multiple Shadows

**CSS Spec**: `box-shadow` accepts a comma-separated list of shadows, drawn back-to-front (last in list drawn first).

**Skia Implementation**: Already have single shadow. For multiple: emit multiple `DrawCommand::BoxShadow` commands in reverse order (last listed = drawn first, behind others).

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

impl Element {
    pub fn shadow(mut self, shadow: BoxShadow) -> Self { ... }       // single
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self { ... } // multiple
}
```

**Priority**: Must-have

#### 3.2 Inset Shadows

**CSS Spec**: `box-shadow: inset ...` draws the shadow inside the element instead of outside.

**Skia Implementation**: Two approaches:

**Approach A (Chromium-style)**: Clip to the element bounds, then draw a large filled RRect outside the element with blur, so the shadow bleeds inward:
```rust
canvas.save();
canvas.clip_rrect(element_rrect, ClipOp::Intersect, true);
// Draw inverted shadow: a large rect with the element shape cut out
let outer = Rect::from_xywh(
    bounds.x - blur * 3.0,
    bounds.y - blur * 3.0,
    bounds.width + blur * 6.0,
    bounds.height + blur * 6.0,
);
let mut path = Path::new();
path.add_rect(outer, None);
path.add_rrect(inner_rrect, None);  // inner_rrect accounts for spread
path.set_fill_type(PathFillType::EvenOdd);
paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, sigma, false));
paint.set_color(shadow_color);
canvas.draw_path(&path, &paint);
canvas.restore();
```

**Approach B**: Use `BlurStyle::Inner` in `MaskFilter::blur` — but this blurs the shape itself inward, not quite the same as CSS inset shadow with offset/spread.

Approach A is correct and matches CSS behavior.

**Rust API Design**: Already covered by the `inset: bool` field on `BoxShadow` above.

**Edge Cases**:
- Inset shadow with spread: positive spread shrinks the inner hole (shadow covers more), negative spread expands it
- Inset shadow must be clipped to the element's border-radius
- Inset shadow renders after background but before content/children
- The blur sigma for Skia's MaskFilter is approximately `blur_radius / 2.0` (CSS blur-radius maps to Skia sigma roughly as `sigma = blur_radius / 2`)

**Priority**: Must-have

#### 3.3 Spread Radius

**CSS Spec**: Positive spread expands the shadow shape; negative spread contracts it.

**Skia Implementation**: Already implemented — the shadow rect is expanded by `spread` in each direction. For inset shadows, spread works inversely (positive = larger shadow = smaller hole = more coverage).

Current code correctly handles this for outset shadows. For inset:
- `inner_rrect` = element bounds inset by `spread` (positive spread = smaller hole)

**Priority**: Already implemented for outset. Need to add for inset.

---

### 4. Outline

#### 4.1 Outline Properties

**CSS Spec**: `outline-color`, `outline-style` (same values as border-style), `outline-width`, `outline-offset`. Unlike border, outline does not affect layout and can be offset from the border edge.

**Skia Implementation**: Draw a stroke around the element, offset outward by `border_width/2 + outline_offset + outline_width/2`:

```rust
let outline_rect = Rect {
    x: bounds.x - outline_offset - outline_width / 2.0,
    y: bounds.y - outline_offset - outline_width / 2.0,
    width: bounds.width + (outline_offset + outline_width / 2.0) * 2.0,
    height: bounds.height + (outline_offset + outline_width / 2.0) * 2.0,
};
let rrect = to_rrect(&outline_rect, corner_radius + outline_offset);
let mut paint = Paint::default();
paint.set_style(PaintStyle::Stroke);
paint.set_stroke_width(outline_width);
paint.set_color(outline_color);
// Apply dash/dot path effect for non-solid styles
canvas.draw_rrect(rrect, &paint);
```

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
    pub style: BorderStyle,  // reuse from borders
    pub offset: f32,
}

impl Element {
    pub fn outline(mut self, color: Color, width: f32) -> Self { ... }
    pub fn outline_full(mut self, outline: Outline) -> Self { ... }
    pub fn outline_offset(mut self, offset: f32) -> Self { ... }
}
```

**Edge Cases**:
- Outline does not follow border-radius by default in CSS, but modern browsers do follow it — we should follow border-radius
- Outline can overlap other elements (it's outside the layout box)
- Negative outline-offset can move outline inside the element
- Outline should not be clipped by parent overflow

**Priority**: Must-have (essential for focus indicators)

---

### 5. CSS Filters

#### 5.1 blur()

**CSS Spec**: `filter: blur(radius)` — Gaussian blur on the element and its content.

**Skia Implementation**: Use `SaveLayerRec` with an `image_filters::blur` set on the paint:

```rust
let filter = image_filters::blur(
    (sigma_x, sigma_y),   // sigma = css_radius / 2 approximately; or use css_radius directly as Skia sigma
    None,                  // tile_mode: None defaults to Clamp
    None,                  // input filter: None
    None,                  // crop_rect: None
).unwrap();
let mut paint = Paint::default();
paint.set_image_filter(filter);
let rec = SaveLayerRec::default().paint(&paint);
canvas.save_layer(&rec);
// ... draw element content ...
canvas.restore();
```

Note: CSS blur radius is the standard deviation. Skia's blur sigma IS the standard deviation, so pass the CSS value directly. (The current box-shadow implementation uses `blur / 2.0` for MaskFilter which is correct for MaskFilter's convention, but ImageFilter blur uses sigma directly.)

**Rust API Design**:
```rust
impl Element {
    pub fn filter_blur(mut self, radius: f32) -> Self { ... }
}
```

**Priority**: Must-have

#### 5.2 brightness()

**CSS Spec**: `filter: brightness(amount)` — 0 = black, 1 = unchanged, >1 = brighter.

**Skia Implementation**: Use a color matrix filter. Brightness multiplies RGB channels:

```rust
let b = amount; // brightness factor
let matrix: [f32; 20] = [
    b, 0.0, 0.0, 0.0, 0.0,
    0.0, b, 0.0, 0.0, 0.0,
    0.0, 0.0, b, 0.0, 0.0,
    0.0, 0.0, 0.0, 1.0, 0.0,
];
let cf = color_filters::matrix_row_major(&matrix);
let filter = image_filters::color_filter(cf, None, None);
paint.set_image_filter(filter);
```

**Rust API Design**:
```rust
impl Element {
    pub fn filter_brightness(mut self, amount: f32) -> Self { ... }
}
```

**Priority**: Nice-to-have

#### 5.3 contrast()

**CSS Spec**: `filter: contrast(amount)` — 0 = gray, 1 = unchanged, >1 = more contrast.

**Skia Implementation**: Color matrix that scales around 0.5:

```rust
let c = amount;
let t = (1.0 - c) / 2.0 * 255.0;  // translate to keep midpoint at 128
let matrix: [f32; 20] = [
    c, 0.0, 0.0, 0.0, t,
    0.0, c, 0.0, 0.0, t,
    0.0, 0.0, c, 0.0, t,
    0.0, 0.0, 0.0, 1.0, 0.0,
];
let cf = color_filters::matrix_row_major(&matrix);
```

Note: The translate values are in [0, 255] range for Skia's color matrix.

**Rust API Design**:
```rust
impl Element {
    pub fn filter_contrast(mut self, amount: f32) -> Self { ... }
}
```

**Priority**: Nice-to-have

#### 5.4 grayscale()

**CSS Spec**: `filter: grayscale(amount)` — 0 = unchanged, 1 = fully grayscale.

**Skia Implementation**: Color matrix using luminance coefficients (Rec. 709):

```rust
let g = 1.0 - amount;  // amount: 0 = full color, 1 = full gray
let r_lum = 0.2126;
let g_lum = 0.7152;
let b_lum = 0.0722;
let matrix: [f32; 20] = [
    r_lum + (1.0 - r_lum) * g,  r_lum * (1.0 - g),          r_lum * (1.0 - g),          0.0, 0.0,
    g_lum * (1.0 - g),          g_lum + (1.0 - g_lum) * g,  g_lum * (1.0 - g),          0.0, 0.0,
    b_lum * (1.0 - g),          b_lum * (1.0 - g),          b_lum + (1.0 - b_lum) * g,  0.0, 0.0,
    0.0,                         0.0,                         0.0,                         1.0, 0.0,
];
```

Wait — the standard CSS grayscale matrix (from the Filter Effects spec) is:

```
amount = clamped to [0, 1]
let s = 1.0 - amount;
matrix = [
    0.2126 + 0.7874 * s,  0.7152 - 0.7152 * s,  0.0722 - 0.0722 * s,  0, 0,
    0.2126 - 0.2126 * s,  0.7152 + 0.2848 * s,  0.0722 - 0.0722 * s,  0, 0,
    0.2126 - 0.2126 * s,  0.7152 - 0.7152 * s,  0.0722 + 0.9278 * s,  0, 0,
    0,                     0,                     0,                     1, 0,
]
```

**Priority**: Nice-to-have

#### 5.5 sepia()

**CSS Spec**: `filter: sepia(amount)` — 0 = unchanged, 1 = fully sepia.

**Skia Implementation**: Color matrix from the Filter Effects specification:

```rust
let s = 1.0 - amount.clamp(0.0, 1.0);
let matrix: [f32; 20] = [
    0.393 + 0.607 * s,  0.769 - 0.769 * s,  0.189 - 0.189 * s,  0.0, 0.0,
    0.349 - 0.349 * s,  0.686 + 0.314 * s,  0.168 - 0.168 * s,  0.0, 0.0,
    0.272 - 0.272 * s,  0.534 - 0.534 * s,  0.131 + 0.869 * s,  0.0, 0.0,
    0.0,                0.0,                0.0,                 1.0, 0.0,
];
```

**Priority**: Nice-to-have

#### 5.6 hue-rotate()

**CSS Spec**: `filter: hue-rotate(angle)` — rotates hue of all colors.

**Skia Implementation**: The CSS spec defines the exact matrix. For angle `a` in radians:

```rust
let cos_a = a.cos();
let sin_a = a.sin();
let matrix: [f32; 20] = [
    0.213 + 0.787 * cos_a - 0.213 * sin_a,
    0.715 - 0.715 * cos_a - 0.715 * sin_a,
    0.072 - 0.072 * cos_a + 0.928 * sin_a,
    0.0, 0.0,
    0.213 - 0.213 * cos_a + 0.143 * sin_a,
    0.715 + 0.285 * cos_a + 0.140 * sin_a,
    0.072 - 0.072 * cos_a - 0.283 * sin_a,
    0.0, 0.0,
    0.213 - 0.213 * cos_a - 0.787 * sin_a,
    0.715 - 0.715 * cos_a + 0.715 * sin_a,
    0.072 + 0.928 * cos_a + 0.072 * sin_a,
    0.0, 0.0,
    0.0, 0.0, 0.0, 1.0, 0.0,
];
```

**Priority**: Nice-to-have

#### 5.7 saturate()

**CSS Spec**: `filter: saturate(amount)` — 0 = desaturated, 1 = unchanged, >1 = super-saturated.

**Skia Implementation**: Same approach as grayscale but with `s = amount`:

```rust
let s = amount;
let matrix: [f32; 20] = [
    0.2126 + 0.7874 * s,  0.7152 - 0.7152 * s,  0.0722 - 0.0722 * s,  0.0, 0.0,
    0.2126 - 0.2126 * s,  0.7152 + 0.2848 * s,  0.0722 - 0.0722 * s,  0.0, 0.0,
    0.2126 - 0.2126 * s,  0.7152 - 0.7152 * s,  0.0722 + 0.9278 * s,  0.0, 0.0,
    0.0,                   0.0,                   0.0,                   1.0, 0.0,
];
```

**Priority**: Nice-to-have

#### 5.8 invert()

**CSS Spec**: `filter: invert(amount)` — 0 = unchanged, 1 = fully inverted.

**Skia Implementation**: Color matrix:

```rust
let i = amount;
let matrix: [f32; 20] = [
    1.0 - 2.0 * i,  0.0,            0.0,            0.0, i * 255.0,
    0.0,            1.0 - 2.0 * i,  0.0,            0.0, i * 255.0,
    0.0,            0.0,            1.0 - 2.0 * i,  0.0, i * 255.0,
    0.0,            0.0,            0.0,            1.0, 0.0,
];
```

**Priority**: Nice-to-have

#### 5.9 opacity() (filter)

**CSS Spec**: `filter: opacity(amount)` — 0 = transparent, 1 = opaque. Different from the `opacity` property because filter opacity creates a stacking context and can be combined with other filters.

**Skia Implementation**: Color matrix that scales alpha:

```rust
let o = amount;
let matrix: [f32; 20] = [
    1.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.0, o,   0.0,
];
```

**Priority**: Nice-to-have (the `opacity` property already exists)

#### 5.10 drop-shadow()

**CSS Spec**: `filter: drop-shadow(offset-x offset-y blur-radius color)` — unlike box-shadow, this follows the element's alpha shape (not the bounding box).

**Skia Implementation**: Use `image_filters::drop_shadow(dx, dy, sigma_x, sigma_y, color, input, crop_rect)`:

```rust
let filter = image_filters::drop_shadow(
    (offset_x, offset_y),     // delta
    (sigma, sigma),            // sigma_x, sigma_y
    skia_color,                // shadow color
    None,                      // input: None = previous layer content
    None,                      // crop_rect
).unwrap();
paint.set_image_filter(filter);
```

Use `image_filters::drop_shadow_only` if you want the shadow without the original content (unusual but available).

**Rust API Design**:
```rust
impl Element {
    pub fn filter_drop_shadow(mut self, x: f32, y: f32, blur: f32, color: Color) -> Self { ... }
}
```

**Edge Cases**:
- drop-shadow follows alpha contour of content (unlike box-shadow which uses the bounding box)
- No spread parameter in CSS drop-shadow (unlike box-shadow)
- Multiple drop-shadows can be chained by composing image filters

**Priority**: Must-have

#### 5.11 Composing Multiple Filters

**CSS Spec**: `filter: blur(5px) brightness(1.2) contrast(1.1)` — filters are applied in order.

**Skia Implementation**: Chain image filters via the `input` parameter. Each filter takes the previous as its input:

```rust
let blur = image_filters::blur((5.0, 5.0), None, None, None);
let brightness_cf = color_filters::matrix_row_major(&brightness_matrix);
let brightness = image_filters::color_filter(brightness_cf, blur, None);  // blur is input
let contrast_cf = color_filters::matrix_row_major(&contrast_matrix);
let combined = image_filters::color_filter(contrast_cf, brightness, None);  // brightness is input
paint.set_image_filter(combined);
```

Alternatively, compose color matrix filters first (multiply matrices), then apply as a single color filter with blur separate.

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub enum Filter {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),  // degrees
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow { x: f32, y: f32, blur: f32, color: Color },
}

impl Element {
    pub fn filters(mut self, filters: Vec<Filter>) -> Self { ... }
}
```

Internally, build the chained `ImageFilter` from the list.

**Performance**: Composing many image filters creates a chain of offscreen buffers. Optimize by merging adjacent color matrix filters into a single matrix multiplication. Chromium does this optimization.

**Priority**: Must-have (the infrastructure for individual filters)

---

### 6. Backdrop Filters

#### 6.1 Full Backdrop Filter Support

**CSS Spec**: `backdrop-filter` accepts the same functions as `filter` but applies them to the area behind the element.

**Skia Implementation**: Already have backdrop blur. Extend to support all filter types:

```rust
canvas.save();
canvas.clip_rrect(element_rrect, ClipOp::Intersect, true);
// Build the image filter (blur, color matrix, etc.)
let filter = build_image_filter(&backdrop_filters);
let mut paint = Paint::default();
paint.set_image_filter(filter);
let rec = SaveLayerRec::default().paint(&paint);
canvas.save_layer(&rec);
canvas.restore(); // this applies the filter to the backdrop
canvas.restore(); // restore clip
```

The key insight: `save_layer` with an image filter on the paint captures the backdrop (what's already drawn) and applies the filter to it.

**Rust API Design**:
```rust
impl Element {
    pub fn backdrop_filters(mut self, filters: Vec<Filter>) -> Self { ... }
    // Keep existing convenience method
    pub fn backdrop_blur(mut self, radius: f32) -> Self { ... }
}
```

**Edge Cases**:
- Backdrop filter only sees content already rendered (z-order matters)
- Performance: every backdrop-filtered element requires reading back the framebuffer — expensive. Limit use.
- Combined with opacity layer: backdrop filter should apply before the element's own opacity

**Priority**: Must-have (already partially implemented)

---

### 7. Transforms

#### 7.1 translate

**CSS Spec**: `transform: translate(x, y)`, `translateX(x)`, `translateY(y)`

**Skia Implementation**: Already have `PushTranslate`. Use `canvas.translate((x, y))`.

**Priority**: Already implemented.

#### 7.2 rotate

**CSS Spec**: `transform: rotate(angle)`

**Skia Implementation**:
```rust
canvas.save();
// Translate to transform-origin, rotate, translate back
canvas.translate((origin_x, origin_y));
canvas.rotate(angle_degrees, None);
canvas.translate((-origin_x, -origin_y));
// ... draw content ...
canvas.restore();
```

Or use `canvas.rotate(degrees, Some(Point::new(origin_x, origin_y)))` which handles the translate internally.

**Rust API Design**:
```rust
impl Element {
    pub fn rotate(mut self, degrees: f32) -> Self { ... }
}
```

**Priority**: Must-have

#### 7.3 scale

**CSS Spec**: `transform: scale(x, y)`, `scaleX(x)`, `scaleY(y)`

**Skia Implementation**:
```rust
canvas.save();
canvas.translate((origin_x, origin_y));
canvas.scale((sx, sy));
canvas.translate((-origin_x, -origin_y));
// ... draw content ...
canvas.restore();
```

**Rust API Design**:
```rust
impl Element {
    pub fn scale(mut self, s: f32) -> Self { ... }              // uniform
    pub fn scale_xy(mut self, sx: f32, sy: f32) -> Self { ... } // non-uniform
}
```

**Edge Cases**:
- Scale affects layout visually but not the layout tree (transforms are post-layout)
- Scale 0 makes element invisible but it still participates in layout
- Negative scale flips the element

**Priority**: Must-have

#### 7.4 skew

**CSS Spec**: `transform: skew(x, y)`, `skewX(angle)`, `skewY(angle)`

**Skia Implementation**: Use `canvas.skew(sx, sy)` where sx/sy are tangent of the skew angles:
```rust
canvas.save();
canvas.translate((origin_x, origin_y));
canvas.skew(angle_x_rad.tan(), angle_y_rad.tan());
canvas.translate((-origin_x, -origin_y));
canvas.restore();
```

**Rust API Design**:
```rust
impl Element {
    pub fn skew(mut self, x_deg: f32, y_deg: f32) -> Self { ... }
}
```

**Priority**: Nice-to-have

#### 7.5 General Matrix Transform

**CSS Spec**: `transform: matrix(a, b, c, d, e, f)` — 2D affine transform.

**Skia Implementation**: Use `canvas.concat(&matrix)` with a `skia_safe::Matrix`:
```rust
let mut m = skia_safe::Matrix::new_identity();
m.set_all(a, c, e, b, d, f, 0.0, 0.0, 1.0);
// Note: CSS matrix(a,b,c,d,e,f) maps to:
//   | a c e |
//   | b d f |
//   | 0 0 1 |
canvas.concat(&m);
```

**Rust API Design**:
```rust
impl Element {
    pub fn transform_matrix(mut self, m: [f32; 6]) -> Self { ... }
}
```

**Priority**: Nice-to-have

#### 7.6 transform-origin

**CSS Spec**: `transform-origin: x y` — the point around which transforms are applied. Default is `center center` (50% 50%).

**Skia Implementation**: All transforms must translate to origin, apply transform, translate back (shown above). Store the origin as a fraction of element size.

**Rust API Design**:
```rust
impl Element {
    pub fn transform_origin(mut self, x: f32, y: f32) -> Self { ... }  // 0.0-1.0 fractions
}
```

Default: `(0.5, 0.5)` = center.

**Priority**: Must-have (needed for correct rotate/scale behavior)

#### 7.7 Combined Transforms

**CSS Spec**: `transform: translateX(10px) rotate(45deg) scale(1.5)` — applied right to left.

**Skia Implementation**: Build a combined `Matrix` by multiplying individual transform matrices in order (left to right in CSS = right to left multiplication). Or apply sequentially via canvas calls:

```rust
canvas.save();
canvas.translate((origin_x, origin_y));
// Apply in CSS order (which is matrix multiplication order)
canvas.translate((tx, ty));
canvas.rotate(angle, None);
canvas.scale((sx, sy));
canvas.translate((-origin_x, -origin_y));
// ... draw ...
canvas.restore();
```

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub enum Transform {
    Translate(f32, f32),
    Rotate(f32),         // degrees
    Scale(f32, f32),
    Skew(f32, f32),      // degrees
    Matrix([f32; 6]),
}

impl Element {
    pub fn transforms(mut self, transforms: Vec<Transform>) -> Self { ... }
}
```

**Edge Cases**:
- Transform order matters: translate then rotate is different from rotate then translate
- Transforms create a new stacking context
- Transforms affect hit testing — need to apply inverse transform to pointer coordinates
- 3D transforms (perspective, rotateX/Y, translate3d) are out of scope for a 2D Skia renderer, but `perspective` can be approximated with Matrix44

**Priority**: Must-have

#### 7.8 DrawCommand for Transforms

Extend the existing `PushTranslate`/`PopTranslate` to a general transform:

```rust
pub enum DrawCommand {
    // Replace PushTranslate/PopTranslate with:
    PushTransform {
        matrix: [f32; 9],  // 3x3 affine matrix
    },
    PopTransform,
    // ... keep other variants
}
```

In the renderer:
```rust
DrawCommand::PushTransform { matrix } => {
    canvas.save();
    let m = skia_safe::Matrix::from_affine(&matrix[..6]); // or build from 9 values
    canvas.concat(&m);
}
DrawCommand::PopTransform => {
    canvas.restore();
}
```

---

### 8. Blend Modes

#### 8.1 mix-blend-mode

**CSS Spec**: `mix-blend-mode` controls how an element blends with its backdrop. Values: normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity.

**Skia Implementation**: Use `SaveLayerRec` with a paint that has a blend mode:

```rust
let mut paint = Paint::default();
paint.set_blend_mode(to_skia_blend_mode(mode));
let rec = SaveLayerRec::default().paint(&paint);
canvas.save_layer(&rec);
// ... draw element content ...
canvas.restore();
```

Mapping CSS to Skia `BlendMode`:
| CSS | Skia |
|-----|------|
| normal | BlendMode::SrcOver |
| multiply | BlendMode::Multiply |
| screen | BlendMode::Screen |
| overlay | BlendMode::Overlay |
| darken | BlendMode::Darken |
| lighten | BlendMode::Lighten |
| color-dodge | BlendMode::ColorDodge |
| color-burn | BlendMode::ColorBurn |
| hard-light | BlendMode::HardLight |
| soft-light | BlendMode::SoftLight |
| difference | BlendMode::Difference |
| exclusion | BlendMode::Exclusion |
| hue | BlendMode::Hue |
| saturation | BlendMode::Saturation |
| color | BlendMode::Color |
| luminosity | BlendMode::Luminosity |

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Element {
    pub fn blend_mode(mut self, mode: BlendMode) -> Self { ... }
}
```

**Edge Cases**:
- Creates an isolated stacking context (requires save_layer)
- Performance: save_layer allocates an offscreen buffer — avoid when blend_mode is Normal
- Combined with opacity: apply opacity to the same save_layer paint

**Priority**: Nice-to-have

#### 8.2 background-blend-mode

**CSS Spec**: `background-blend-mode` controls how background layers blend with each other.

**Skia Implementation**: When drawing multiple background layers, set `paint.set_blend_mode(mode)` on each background paint (except the bottom-most layer which should use `SrcOver`).

**Priority**: Can-skip

---

### 9. Overflow

#### 9.1 overflow: hidden

**CSS Spec**: `overflow: hidden` — clips content to the element's padding box. `overflow: visible` — no clipping (default).

**Skia Implementation**: Already implemented as `Element.clip: bool` which pushes a `PushClip` command. This is equivalent to `overflow: hidden`.

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Element {
    pub fn overflow(mut self, overflow: Overflow) -> Self { ... }
    pub fn overflow_xy(mut self, x: Overflow, y: Overflow) -> Self { ... }
}
```

`Scroll` and `Auto` should integrate with the existing `ScrollDirection` system: set clip = true and enable scroll event handling.

**Edge Cases**:
- `overflow-x: hidden; overflow-y: scroll` requires directional clipping — use a clip rect that is infinite in the scroll direction (or just clip to bounds and handle scroll offset separately, which is the current approach)
- `overflow: visible` is the default and means content can paint outside the element bounds

**Priority**: Must-have (partially implemented)

---

### 10. Visibility and Display

#### 10.1 visibility: hidden

**CSS Spec**: `visibility: hidden` — element is invisible but still takes up space in layout.

**Skia Implementation**: Skip all draw commands for the element but keep its layout contribution. In `emit_commands`, check `element.visibility` and skip drawing (but still recurse for children with their own visibility).

```rust
if element.visible {
    // ... emit background, text, image draw commands
}
// Always recurse children (they may have visibility: visible)
for (child_layout, child_element) in ... {
    emit_commands(child_layout, child_element, animator, commands);
}
```

**Rust API Design**:
```rust
impl Element {
    pub fn visible(mut self, v: bool) -> Self { ... }
    pub fn hidden(mut self) -> Self { self.visible = false; self }
}
```

**Edge Cases**:
- Children of `visibility: hidden` can override with `visibility: visible`
- Events should not fire on hidden elements
- Animations should still run on hidden elements (for transition back to visible)

**Priority**: Must-have

#### 10.2 display: none

**CSS Spec**: `display: none` — element is removed from layout and rendering entirely.

**Skia Implementation**: This is a layout concern — the element should not be passed to Taffy for layout computation. In the element tree, skip elements with `display_none: true` when building the layout tree.

**Rust API Design**:
```rust
impl Element {
    pub fn display_none(mut self) -> Self { ... }
}
```

**Edge Cases**:
- Must also skip in event handling
- Animations: transitioning to/from display:none is where exit/enter animations come in (already supported via the animation system)

**Priority**: Must-have

---

### 11. Cursor Styles

#### 11.1 Extended Cursor Styles

**CSS Spec**: `cursor: auto | default | pointer | text | move | wait | progress | not-allowed | crosshair | grab | grabbing | col-resize | row-resize | n-resize | s-resize | e-resize | w-resize | ne-resize | nw-resize | se-resize | sw-resize | zoom-in | zoom-out | none`

**Skia Implementation**: Cursors are not a Skia concept — they're a Wayland/windowing concept. The current `CursorStyle` enum is set on elements and propagated to the compositor.

**Rust API Design**: Extend the existing enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Auto,
    Default,
    None,
    Pointer,
    Text,
    Move,
    Wait,
    Progress,
    NotAllowed,
    Crosshair,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    NResize,
    SResize,
    EResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    ZoomIn,
    ZoomOut,
}
```

Map to Wayland cursor names (`wp_cursor_shape_v1` protocol or fallback to `xcursor` names):
| CursorStyle | Wayland name |
|-------------|-------------|
| Default | "default" |
| Pointer | "pointer" |
| Text | "text" |
| Move | "move" or "all-scroll" |
| Wait | "wait" |
| Grab | "grab" |
| Grabbing | "grabbing" |
| ColResize | "col-resize" |
| RowResize | "row-resize" |
| NResize | "n-resize" |
| etc. | etc. |

**Priority**: Must-have

---

### 12. Pointer Events

#### 12.1 pointer-events: none

**CSS Spec**: `pointer-events: none` — element does not receive pointer events; they pass through to elements behind it. `pointer-events: auto` — normal behavior.

**Skia Implementation**: Not a rendering property. In the hit-testing logic, skip elements with `pointer_events_none: true`.

**Rust API Design**:
```rust
impl Element {
    pub fn pointer_events_none(mut self) -> Self { ... }
    pub fn pointer_events(mut self, enabled: bool) -> Self { ... }
}
```

**Edge Cases**:
- Children of `pointer-events: none` can override with `pointer-events: auto` (in CSS; for simplicity we may not support this)
- The element is still visible and takes space, just doesn't intercept events

**Priority**: Must-have

---

### 13. Object Fit and Position (Images)

#### 13.1 object-fit

**CSS Spec**: `object-fit: fill | contain | cover | none | scale-down`
Controls how replaced content (images) is sized within its box.

**Skia Implementation**: When drawing images, compute source and destination rects:

- **fill** (default): Stretch to fill — `draw_image_rect(img, None, dst_rect, paint)` (current behavior)
- **contain**: Scale uniformly to fit within bounds, letterbox:
  ```rust
  let scale = (dst_w / src_w).min(dst_h / src_h);
  let w = src_w * scale;
  let h = src_h * scale;
  let x = dst_x + (dst_w - w) * align_x;  // align_x from object-position
  let y = dst_y + (dst_h - h) * align_y;
  ```
- **cover**: Scale uniformly to cover bounds, crop overflow:
  ```rust
  let scale = (dst_w / src_w).max(dst_h / src_h);
  // compute visible portion of source as src_rect
  ```
- **none**: No scaling, center in box (or position per object-position)
- **scale-down**: Use `none` or `contain`, whichever is smaller

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl Element {
    pub fn object_fit(mut self, fit: ObjectFit) -> Self { ... }
    pub fn object_position(mut self, x: f32, y: f32) -> Self { ... }  // 0.0-1.0
}
```

**Edge Cases**:
- SVGs have intrinsic aspect ratios from their viewBox
- Images without intrinsic sizes (broken images) use the element's size
- object-position default is `50% 50%` (centered)

**Priority**: Must-have

---

### 14. Text Properties

#### 14.1 Text Decoration

**CSS Spec**: `text-decoration-line: none | underline | overline | line-through`, `text-decoration-color`, `text-decoration-style: solid | double | dotted | dashed | wavy`, `text-decoration-thickness`, `text-underline-offset`.

**Skia Implementation**: Draw lines relative to the text baseline using font metrics:

```rust
let (_, metrics) = font.metrics();
let baseline_y = pos.y - metrics.ascent;

// Underline
if decoration.underline {
    let y = baseline_y + metrics.underline_position.unwrap_or(metrics.descent * 0.5);
    let thickness = decoration.thickness.unwrap_or(
        metrics.underline_thickness.unwrap_or(font_size / 14.0)
    );
    let mut paint = Paint::default();
    paint.set_color(decoration.color);
    paint.set_stroke_width(thickness);
    paint.set_style(PaintStyle::Stroke);
    // Apply style (dashed, dotted, wavy)
    match decoration.style {
        DecorationStyle::Dashed => {
            paint.set_path_effect(PathEffect::dash(&[thickness * 3.0, thickness * 2.0], 0.0));
        }
        DecorationStyle::Dotted => {
            paint.set_stroke_cap(StrokeCap::Round);
            paint.set_path_effect(PathEffect::dash(&[0.0, thickness * 2.0], 0.0));
        }
        DecorationStyle::Wavy => {
            // Draw a wavy path instead of a line
            let mut path = Path::new();
            let wave_len = thickness * 4.0;
            let amplitude = thickness;
            // Build sine wave path along the underline
            path.move_to((pos.x, y));
            let mut x = pos.x;
            while x < pos.x + text_width {
                path.quad_to(
                    (x + wave_len / 4.0, y - amplitude),
                    (x + wave_len / 2.0, y),
                );
                path.quad_to(
                    (x + wave_len * 3.0 / 4.0, y + amplitude),
                    (x + wave_len, y),
                );
                x += wave_len;
            }
            canvas.draw_path(&path, &paint);
            // skip draw_line below
        }
        DecorationStyle::Double => {
            let gap = thickness;
            canvas.draw_line((pos.x, y - gap / 2.0), (pos.x + text_width, y - gap / 2.0), &paint);
            canvas.draw_line((pos.x, y + gap / 2.0), (pos.x + text_width, y + gap / 2.0), &paint);
        }
        DecorationStyle::Solid => {
            canvas.draw_line((pos.x, y), (pos.x + text_width, y), &paint);
        }
    }
}

// Overline: y = baseline_y + metrics.ascent (top of text)
// Line-through: y = baseline_y + metrics.strikeout_position
```

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub struct TextDecoration {
    pub line: TextDecorationLine,
    pub style: DecorationStyle,
    pub color: Option<Color>,        // None = use text color
    pub thickness: Option<f32>,      // None = from font metrics
}

#[derive(Debug, Clone, Copy)]
pub enum TextDecorationLine {
    None,
    Underline,
    Overline,
    LineThrough,
    UnderlineLineThrough,  // combined
}

#[derive(Debug, Clone, Copy)]
pub enum DecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

impl Element {
    pub fn text_decoration(mut self, decoration: TextDecoration) -> Self { ... }
    // Convenience
    pub fn underline(mut self) -> Self { ... }
    pub fn line_through(mut self) -> Self { ... }
}
```

**Edge Cases**:
- Underline should skip descenders (ink skipping) — this is complex. Chromium creates a clip path from glyph outlines and gaps the underline. For v1, skip ink skipping.
- Wavy lines need to tile cleanly — ensure wave pattern starts at a consistent phase
- text-underline-offset shifts the underline position (positive = down)

**Priority**: Must-have

#### 14.2 Text Shadow

**CSS Spec**: `text-shadow: offset-x offset-y blur-radius color` — can have multiple comma-separated shadows.

**Skia Implementation**: Draw the text multiple times, once for each shadow (behind the main text), with blur applied via MaskFilter:

```rust
for shadow in shadows.iter().rev() {
    let mut shadow_paint = Paint::default();
    shadow_paint.set_color(shadow.color);
    shadow_paint.set_anti_alias(true);
    if shadow.blur > 0.0 {
        shadow_paint.set_mask_filter(
            MaskFilter::blur(BlurStyle::Normal, shadow.blur / 2.0, false)
        );
    }
    canvas.draw_str(
        text,
        (pos.x + shadow.offset_x, baseline_y + shadow.offset_y),
        &font,
        &shadow_paint,
    );
}
// Then draw main text on top
canvas.draw_str(text, (pos.x, baseline_y), &font, &paint);
```

**Rust API Design**:
```rust
#[derive(Debug, Clone)]
pub struct TextShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub color: Color,
}

impl Element {
    pub fn text_shadow(mut self, shadow: TextShadow) -> Self { ... }
    pub fn text_shadows(mut self, shadows: Vec<TextShadow>) -> Self { ... }
}
```

**Priority**: Nice-to-have

#### 14.3 Text Overflow

**CSS Spec**: `text-overflow: clip | ellipsis` — how overflowed text is indicated.

**Skia Implementation**: This requires text layout awareness:

1. Measure text width against available width
2. If overflowing and `text-overflow: ellipsis`:
   - Binary search or iterate to find how many characters fit with "..." appended
   - Measure `text[..n] + "..."` until it fits
   - Draw the truncated string

```rust
if text_overflow == TextOverflow::Ellipsis && text_width > available_width {
    let ellipsis = "...";
    let (ew, _) = font.measure_str(ellipsis, None);
    let target = available_width - ew;
    // Find truncation point
    let mut n = text.len();
    loop {
        let (w, _) = font.measure_str(&text[..n], None);
        if w <= target || n == 0 { break; }
        // Step back by one char
        n = text[..n].char_indices().rev().nth(0).map(|(i, _)| i).unwrap_or(0);
    }
    let truncated = format!("{}...", &text[..n]);
    canvas.draw_str(&truncated, pos, &font, &paint);
}
```

**Rust API Design**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}

impl Element {
    pub fn text_overflow(mut self, overflow: TextOverflow) -> Self { ... }
}
```

**Edge Cases**:
- Requires `overflow: hidden` and `white-space: nowrap` to be meaningful
- Ellipsis measurement must account for the font's actual "..." glyph width
- Multi-line ellipsis (`-webkit-line-clamp`) is more complex — measure line-by-line

**Priority**: Must-have

#### 14.4 Font Weight and Style

**CSS Spec**: `font-weight: 100-900 | normal | bold`, `font-style: normal | italic | oblique`

**Skia Implementation**: Use `FontStyle` when loading the typeface:

```rust
let font_style = FontStyle::new(
    match weight {
        100 => Weight::THIN,
        200 => Weight::EXTRA_LIGHT,
        300 => Weight::LIGHT,
        400 => Weight::NORMAL,
        500 => Weight::MEDIUM,
        600 => Weight::SEMI_BOLD,
        700 => Weight::BOLD,
        800 => Weight::EXTRA_BOLD,
        900 => Weight::BLACK,
        _ => Weight::NORMAL,
    },
    Width::NORMAL,
    match style {
        FontStyleType::Italic => Slant::Italic,
        FontStyleType::Oblique => Slant::Oblique,
        _ => Slant::Upright,
    },
);
let typeface = font_mgr.legacy_make_typeface(family_name, font_style);
```

Cache typefaces by (family, weight, style) tuple.

**Rust API Design**:
```rust
impl Element {
    pub fn font_weight(mut self, weight: u16) -> Self { ... }  // 100-900
    pub fn bold(mut self) -> Self { self.font_weight(700) }
    pub fn italic(mut self) -> Self { ... }
    pub fn font_family(mut self, family: &str) -> Self { ... }
}
```

**Priority**: Must-have

#### 14.5 Letter Spacing and Word Spacing

**CSS Spec**: `letter-spacing: normal | <length>`, `word-spacing: normal | <length>`

**Skia Implementation**: For letter-spacing, draw each character individually with added spacing, or use `Font::set_spacing` if available. In skia_safe, use `TextBlobBuilder` with explicit per-glyph positioning:

Simpler approach — Skia's `Font` does not have a letter-spacing setter. Instead, use the paragraph/shaper API, or manually place glyphs:

```rust
// For letter spacing:
let mut x = pos.x;
for ch in text.chars() {
    let s = ch.to_string();
    let (advance, _) = font.measure_str(&s, None);
    canvas.draw_str(&s, (x, baseline_y), &font, &paint);
    x += advance + letter_spacing;
}
```

For word spacing, only add extra space on space characters.

Better approach: Use Skia's `Shaper` or `Paragraph` API (from `skia_safe::textlayout`) which supports letter and word spacing natively.

**Priority**: Nice-to-have

---

### 15. Colors and Color Spaces

#### 15.1 Extended Color Support

**CSS Spec**: Colors in sRGB, Display P3, Lab, OKLab, LCH, OKLCH.

**Skia Implementation**: Skia supports wide-gamut colors via `Color4f` with a `ColorSpace`:

```rust
use skia_safe::{Color4f, ColorSpace};

// P3 color
let p3_space = ColorSpace::new_srgb(); // or ColorSpace::make_srgb() — P3 needs ICC profile
let color = Color4f::new(1.0, 0.0, 0.0, 1.0); // linear RGB values
paint.set_color4f(color, &p3_space);
```

For gradient shaders with color spaces:
```rust
gradient_shader::linear_with_interpolation(
    points,
    &GradientShaderColors::ColorsInSpace(&colors4f, color_space),
    positions,
    tile_mode,
    interpolation,
    local_matrix,
);
```

**Rust API Design**:
```rust
impl Color {
    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self { ... }
    pub fn from_hsla(h: f32, s: f32, l: f32, a: f32) -> Self { ... }
    pub fn from_hex(hex: &str) -> Self { ... }
    pub fn with_alpha(self, a: f32) -> Self { ... }
    pub fn lighter(self, amount: f32) -> Self { ... }
    pub fn darker(self, amount: f32) -> Self { ... }
}
```

**Priority**: Must-have (color helpers), Can-skip (P3/Lab color spaces for now)

---

### 16. Font Rendering

#### 16.1 Font Family Selection

**CSS Spec**: `font-family: "Helvetica Neue", Arial, sans-serif`

**Skia Implementation**: Use `FontMgr::match_family_style(family, style)` with fallback chain:

```rust
fn resolve_typeface(
    font_mgr: &FontMgr,
    families: &[&str],
    style: FontStyle,
    cache: &mut HashMap<String, Typeface>,
) -> Typeface {
    let cache_key = format!("{:?}:{:?}", families, style);
    if let Some(tf) = cache.get(&cache_key) {
        return tf.clone();
    }
    for family in families {
        if let Some(tf) = font_mgr.match_family_style(family, style) {
            cache.insert(cache_key, tf.clone());
            return tf;
        }
    }
    // Fallback to default
    font_mgr.legacy_make_typeface(None, style).unwrap()
}
```

**Priority**: Must-have

#### 16.2 Line Height

**CSS Spec**: `line-height: normal | <number> | <length> | <percentage>`

**Skia Implementation**: Not a Skia paint property — affects text layout. When positioning multi-line text, use `line_height * font_size` as the vertical advance between lines.

```rust
let line_height_px = match line_height {
    LineHeight::Normal => metrics.descent - metrics.ascent + metrics.leading,
    LineHeight::Factor(f) => font_size * f,
    LineHeight::Pixels(px) => px,
};
```

**Rust API Design**:
```rust
impl Element {
    pub fn line_height(mut self, lh: f32) -> Self { ... }  // multiplier (1.0 = normal)
    pub fn line_height_px(mut self, px: f32) -> Self { ... }
}
```

**Priority**: Must-have (for multi-line text)

#### 14.6 Text Alignment

**CSS Spec**: `text-align: left | center | right | justify`

**Skia Implementation**: Affects text positioning within the element bounds:

```rust
let text_x = match text_align {
    TextAlign::Left => bounds.x,
    TextAlign::Center => bounds.x + (bounds.width - text_width) / 2.0,
    TextAlign::Right => bounds.x + bounds.width - text_width,
    TextAlign::Justify => bounds.x, // space distribution between words
};
```

**Rust API Design**:
```rust
impl Element {
    pub fn text_align(mut self, align: TextAlign) -> Self { ... }
}
```

**Priority**: Must-have

---

## Rendering Pipeline Changes

### display_list.rs Changes

1. **Extend `DrawCommand::Rect`** to include:
   - `gradient: Option<Gradient>` for gradient backgrounds
   - `corner_radii: CornerRadii` replacing `corner_radius: f32`
   - `border: Option<FullBorder>` replacing `Option<Border>` for per-side borders

2. **Add new commands**:
   - `DrawCommand::InsetBoxShadow { bounds, corner_radii, blur, spread, color, offset }`
   - `DrawCommand::PushTransform { matrix: [f32; 9] }` replacing `PushTranslate`
   - `DrawCommand::PopTransform` replacing `PopTranslate`
   - `DrawCommand::Outline { bounds, corner_radii, color, width, style, offset }`
   - `DrawCommand::PushFilter { filters: Vec<Filter> }` — wraps content in filter layer
   - `DrawCommand::PopFilter`
   - `DrawCommand::PushBlendMode { mode: BlendMode }`
   - `DrawCommand::PopBlendMode`

3. **Extend `DrawCommand::Text`** to include:
   - `decoration: Option<TextDecoration>`
   - `shadows: Vec<TextShadow>`
   - `overflow: TextOverflow`
   - `max_width: Option<f32>` — for ellipsis computation
   - `font_weight: u16`
   - `font_style: FontStyleType`
   - `font_family: Option<String>`
   - `text_align: TextAlign`
   - `line_height: f32`
   - `letter_spacing: f32`

4. **Extend `DrawCommand::Image`** to include:
   - `object_fit: ObjectFit`
   - `object_position: (f32, f32)`

5. **Extend `emit_commands`** to:
   - Check `element.visible` before emitting draw commands (but always recurse children)
   - Check `element.display_none` and skip entirely (no draw, no recurse)
   - Emit filter/blend mode push/pop around element content
   - Emit transforms with full matrix support
   - Emit outlines after children (outlines paint on top)
   - Emit backdrop filters before background

### renderer.rs Changes

1. **Gradient rendering**: In `draw_rect`, check for gradient, build shader, set on paint
2. **Per-side borders**: Replace single stroke with per-side drawing using `draw_drrect` + clips or filled path segments
3. **Border styles**: Apply `PathEffect::dash` for dashed/dotted, draw double lines for double
4. **Per-corner radii**: Replace `RRect::new_rect_xy` with `RRect::new_rect_radii` everywhere
5. **Inset shadows**: New `draw_inset_shadow` method using EvenOdd path clipping
6. **Filters**: Build chained `ImageFilter` from `Vec<Filter>`, apply via `SaveLayerRec`
7. **Transforms**: Replace translate with general `Matrix` concat
8. **Blend modes**: Set blend mode on `SaveLayerRec` paint
9. **Text decoration**: Draw underline/overline/line-through in `draw_text`
10. **Text shadow**: Draw shadow passes before main text in `draw_text`
11. **Text overflow**: Measure and truncate with ellipsis in `draw_text`
12. **Image object-fit**: Compute source/destination rects in `draw_image`
13. **Font resolution**: Cache typefaces by (family, weight, style), resolve fallback chain
14. **Outline rendering**: New `draw_outline` method similar to border but offset outward

### Performance Considerations

- **Gradient shader creation**: Cache gradient shaders by parameters. Shader creation is not free but shaders themselves are GPU-efficient.
- **Image filters**: Each filter in a chain allocates an intermediate surface. Merge adjacent color matrix filters into a single matrix multiplication.
- **save_layer calls**: Each creates an offscreen buffer. Minimize by combining opacity + blend_mode into one layer. Skip layer for opacity: 1.0 and blend_mode: Normal.
- **Per-side borders with different colors**: Requires multiple draw calls. Optimize the common case (uniform border) by detecting equal sides and using single stroke.
- **Text measurement**: Cache text measurements by (text, font_size, font_family, weight). `Font::measure_str` is relatively fast but adds up.
- **Typeface loading**: Cache aggressively. `FontMgr::match_family_style` does filesystem access.

---

## Animatable Properties

Properties that can be smoothly interpolated and should be supported by the animation system:

### Numeric Interpolation (lerp)
| Property | Type | Notes |
|----------|------|-------|
| opacity | f32 | Already animated |
| background color | Color | Lerp each RGBA channel |
| border color | Color | Lerp each RGBA channel |
| border width | f32 | Per-side |
| corner radius | f32 | Per-corner |
| box-shadow blur | f32 | |
| box-shadow spread | f32 | |
| box-shadow offset | (f32, f32) | |
| box-shadow color | Color | |
| outline width | f32 | |
| outline color | Color | |
| outline offset | f32 | |
| transform translate | (f32, f32) | Already animated via offset_x/y |
| transform rotate | f32 | Interpolate angle |
| transform scale | (f32, f32) | Already partially supported |
| filter blur | f32 | |
| filter brightness | f32 | |
| filter contrast | f32 | |
| filter grayscale | f32 | |
| filter saturate | f32 | |
| filter hue-rotate | f32 | |
| filter sepia | f32 | |
| filter invert | f32 | |
| text-decoration-color | Color | |
| letter-spacing | f32 | |

### Gradient Interpolation
Gradients can be interpolated if they have the same type and same number of stops:
- Lerp each stop's color (RGBA)
- Lerp each stop's position
- Lerp angle (linear) or center/radius (radial)

### Non-Animatable (Discrete)
These properties snap to the new value at 50% of the transition:
| Property | Notes |
|----------|-------|
| border-style | Discrete change |
| text-decoration-line | Discrete |
| text-decoration-style | Discrete |
| visibility | Snaps at start (hidden->visible) or end (visible->hidden) |
| overflow | Discrete |
| cursor | Discrete |
| object-fit | Discrete |
| blend-mode | Discrete |
| pointer-events | Discrete |
| display | Discrete (none vs. other) |

### Animation System Integration

Extend `From` and `To` structs to support new properties:

```rust
pub struct From {
    pub opacity: Option<f32>,
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub scale: Option<f32>,
    pub rotation: Option<f32>,        // NEW
    pub background: Option<Color>,    // NEW
    pub blur: Option<f32>,            // NEW: filter blur
    pub animation: Option<Animation>,
    pub delay_ms: Option<u32>,
}
```

Extend `AnimationOverrides` (in the animator) to carry these additional interpolated values.

---

## Implementation Order

Recommended sequence, with each phase building on the previous:

### Phase 1: Core Visual Upgrades (foundation)
1. **Per-corner border radius** — Change `corner_radius: f32` to `CornerRadii`, update `to_rrect` helper, update all draw commands. Backward compatible via the existing `.rounded()` method.
2. **Gradient backgrounds** — Add `Gradient` enum, extend `DrawCommand::Rect`, implement shader creation in renderer. Start with linear gradients, then radial, then conic.
3. **Inset box shadows** — Extend `BoxShadow` with `inset: bool`, add renderer logic with EvenOdd path.
4. **Multiple box shadows** — Store `Vec<BoxShadow>` on elements, emit multiple draw commands.

### Phase 2: Borders and Outlines
5. **Border style** — Add `BorderStyle` enum, implement dashed/dotted via `PathEffect::dash`, double via two strokes.
6. **Per-side borders** — Extend `Border` to `FullBorder`, implement per-side drawing with clip regions or filled paths.
7. **Outline** — Add `Outline` struct and draw command, implement as offset stroke.

### Phase 3: Filters
8. **CSS filter infrastructure** — Add `Filter` enum, build chained `ImageFilter`, implement `PushFilter`/`PopFilter` draw commands.
9. **Individual filters** — Implement blur, brightness, contrast, grayscale, sepia, hue-rotate, invert, saturate, drop-shadow using color matrices and image filters.
10. **Extended backdrop filters** — Generalize existing backdrop blur to support all filter types.

### Phase 4: Transforms
11. **General transforms** — Replace `PushTranslate` with `PushTransform`, implement rotate, scale, skew via canvas matrix operations.
12. **Transform origin** — Add origin field, apply translate-transform-untranslate pattern.
13. **Transform hit testing** — Apply inverse transform to pointer coordinates during event dispatch.

### Phase 5: Text Enhancements
14. **Font family and weight** — Add fields to Element, implement typeface resolution with caching and fallback chains.
15. **Text decoration** — Implement underline/overline/line-through with style variants.
16. **Text overflow with ellipsis** — Measure text, truncate with "..." when overflowing.
17. **Text shadow** — Draw shadow passes behind main text.
18. **Text alignment and line height** — Position text within bounds, handle multi-line.
19. **Letter spacing** — Per-character positioning or shaper API.

### Phase 6: Remaining Properties
20. **Visibility and display:none** — Skip rendering/layout for invisible elements.
21. **Extended cursor styles** — Expand `CursorStyle` enum, map to Wayland cursor names.
22. **Pointer events** — Add `pointer_events` field, skip in hit testing.
23. **Object-fit/object-position** — Compute image source/dest rects for contain/cover modes.
24. **Blend modes** — Implement `mix-blend-mode` via `SaveLayerRec` with blend mode on paint.
25. **Color utilities** — Add `from_hsl`, `from_hex`, `lighter`, `darker`, `with_alpha` to Color.

### Phase 7: Polish and Optimization
26. **Multiple backgrounds** — Support stacked background layers.
27. **Background clip/origin** — Adjust clip and coordinate origin for background rendering.
28. **Border image** — Nine-slice image rendering for borders.
29. **Animation extensions** — Add new animatable properties to `From`/`To`/`AnimationOverrides`.
30. **Performance** — Cache gradient shaders, merge color matrix filters, optimize uniform border fast path, cache typeface resolution, cache text measurements.

---

## Appendix: Skia API Quick Reference

### Paint Configuration Methods
```rust
paint.set_color(color: Color)
paint.set_color4f(color: Color4f, color_space: &ColorSpace)
paint.set_alpha_f(alpha: f32)
paint.set_style(style: PaintStyle)              // Fill, Stroke, StrokeAndFill
paint.set_stroke_width(width: f32)
paint.set_stroke_cap(cap: StrokeCap)            // Butt, Round, Square
paint.set_stroke_join(join: StrokeJoin)         // Miter, Round, Bevel
paint.set_stroke_miter(miter: f32)
paint.set_anti_alias(aa: bool)
paint.set_dither(dither: bool)
paint.set_shader(shader: Option<Shader>)
paint.set_color_filter(filter: Option<ColorFilter>)
paint.set_image_filter(filter: Option<ImageFilter>)
paint.set_mask_filter(filter: Option<MaskFilter>)
paint.set_path_effect(effect: Option<PathEffect>)
paint.set_blend_mode(mode: BlendMode)
```

### Canvas State Methods
```rust
canvas.save() -> usize
canvas.restore()
canvas.save_layer(rec: &SaveLayerRec) -> usize
canvas.translate(delta: impl Into<Vector>)
canvas.rotate(degrees: f32, point: Option<Point>)
canvas.scale(scale: impl Into<(f32, f32)>)
canvas.skew(sx: f32, sy: f32)
canvas.concat(matrix: &Matrix)
canvas.clip_rect(rect: impl AsRef<Rect>, op: ClipOp, do_anti_alias: bool)
canvas.clip_rrect(rrect: impl AsRef<RRect>, op: ClipOp, do_anti_alias: bool)
canvas.clip_path(path: &Path, op: ClipOp, do_anti_alias: bool)
```

### Canvas Drawing Methods
```rust
canvas.draw_rect(rect: impl AsRef<Rect>, paint: &Paint)
canvas.draw_rrect(rrect: impl AsRef<RRect>, paint: &Paint)
canvas.draw_drrect(outer: impl AsRef<RRect>, inner: impl AsRef<RRect>, paint: &Paint)
canvas.draw_path(path: &Path, paint: &Paint)
canvas.draw_line(p1: impl Into<Point>, p2: impl Into<Point>, paint: &Paint)
canvas.draw_circle(center: impl Into<Point>, radius: f32, paint: &Paint)
canvas.draw_str(text: &str, origin: impl Into<Point>, font: &Font, paint: &Paint)
canvas.draw_text_blob(blob: &TextBlob, origin: impl Into<Point>, paint: &Paint)
canvas.draw_image(image: &Image, left_top: impl Into<Point>, paint: Option<&Paint>)
canvas.draw_image_rect(image: &Image, src: Option<(&Rect, SrcRectConstraint)>, dst: impl AsRef<Rect>, paint: &Paint)
canvas.draw_image_nine(image: &Image, center: &IRect, dst: impl AsRef<Rect>, filter: FilterMode, paint: Option<&Paint>)
```

### Shader Creation
```rust
// Gradients
gradient_shader::linear(points, colors, positions, tile_mode) -> Option<Shader>
gradient_shader::radial(center, radius, colors, positions, tile_mode) -> Option<Shader>
gradient_shader::sweep(center, colors, positions, tile_mode, start_angle, end_angle) -> Option<Shader>
gradient_shader::two_point_conical(start, start_r, end, end_r, colors, positions, tile_mode) -> Option<Shader>

// Image shader
image.to_shader(tile_x, tile_y, sampling, local_matrix) -> Option<Shader>
```

### Image Filter Creation
```rust
image_filters::blur(sigma, tile_mode, input, crop_rect) -> Option<ImageFilter>
image_filters::drop_shadow(delta, sigma, color, input, crop_rect) -> Option<ImageFilter>
image_filters::drop_shadow_only(delta, sigma, color, input, crop_rect) -> Option<ImageFilter>
image_filters::color_filter(cf, input, crop_rect) -> Option<ImageFilter>
image_filters::compose(outer, inner) -> Option<ImageFilter>
image_filters::blend(mode, background, foreground, crop_rect) -> Option<ImageFilter>
image_filters::dilate(radius, input, crop_rect) -> Option<ImageFilter>
image_filters::erode(radius, input, crop_rect) -> Option<ImageFilter>
image_filters::offset(delta, input, crop_rect) -> Option<ImageFilter>
image_filters::merge(filters, crop_rect) -> Option<ImageFilter>
```

### Color Filter Creation
```rust
color_filters::blend(color, mode) -> Option<ColorFilter>
color_filters::matrix_row_major(matrix: &[f32; 20]) -> Option<ColorFilter>
color_filters::compose(outer, inner) -> Option<ColorFilter>
color_filters::lighting(multiply, add) -> Option<ColorFilter>
color_filters::linear_to_srgb_gamma() -> Option<ColorFilter>
color_filters::srgb_to_linear_gamma() -> Option<ColorFilter>
color_filters::table(table: &[u8; 256]) -> Option<ColorFilter>
color_filters::table_argb(a: &[u8; 256], r: &[u8; 256], g: &[u8; 256], b: &[u8; 256]) -> Option<ColorFilter>
```

### Path Effect Creation
```rust
PathEffect::dash(intervals: &[f32], phase: f32) -> Option<PathEffect>
PathEffect::corner_path(radius: f32) -> Option<PathEffect>
PathEffect::discrete(seg_length: f32, dev: f32, seed: u32) -> Option<PathEffect>
// compose: apply inner then outer
PathEffect::compose(outer: PathEffect, inner: PathEffect) -> Option<PathEffect>
// sum: draw both independently
PathEffect::sum(first: PathEffect, second: PathEffect) -> Option<PathEffect>
```

### RRect Construction
```rust
RRect::new_rect_xy(rect, rx, ry)                    // uniform radius
RRect::new_rect_radii(rect, radii: &[Point; 4])     // per-corner (TL, TR, BR, BL)
RRect::new_rect(rect)                                // zero radius (plain rect)
```

### Color Matrix Format (5x4, row-major)
```
[  R_scale, R_from_G, R_from_B, R_from_A, R_translate,
   G_from_R, G_scale,  G_from_B, G_from_A, G_translate,
   B_from_R, B_from_G, B_scale,  B_from_A, B_translate,
   A_from_R, A_from_G, A_from_B, A_scale,  A_translate ]
```
- Scale/from values multiply the input channel
- Translate values are added (in 0-255 range for Skia)
- Identity matrix: diagonal = 1.0, rest = 0.0
- Matrices can be composed by multiplication for adjacent color matrix filters (optimization)

### BlendMode Mapping (CSS to Skia)
```rust
fn to_skia_blend_mode(mode: BlendMode) -> skia_safe::BlendMode {
    match mode {
        BlendMode::Normal     => skia_safe::BlendMode::SrcOver,
        BlendMode::Multiply   => skia_safe::BlendMode::Multiply,
        BlendMode::Screen     => skia_safe::BlendMode::Screen,
        BlendMode::Overlay    => skia_safe::BlendMode::Overlay,
        BlendMode::Darken     => skia_safe::BlendMode::Darken,
        BlendMode::Lighten    => skia_safe::BlendMode::Lighten,
        BlendMode::ColorDodge => skia_safe::BlendMode::ColorDodge,
        BlendMode::ColorBurn  => skia_safe::BlendMode::ColorBurn,
        BlendMode::HardLight  => skia_safe::BlendMode::HardLight,
        BlendMode::SoftLight  => skia_safe::BlendMode::SoftLight,
        BlendMode::Difference => skia_safe::BlendMode::Difference,
        BlendMode::Exclusion  => skia_safe::BlendMode::Exclusion,
        BlendMode::Hue        => skia_safe::BlendMode::Hue,
        BlendMode::Saturation => skia_safe::BlendMode::Saturation,
        BlendMode::Color      => skia_safe::BlendMode::Color,
        BlendMode::Luminosity => skia_safe::BlendMode::Luminosity,
    }
}
```
