# SVG Rendering Implementation Plan

## Overview

SVG integration spans three tiers of increasing capability:

1. **Tier 1 - Enhanced Rasterized SVG**: Improve the existing resvg pipeline with multi-resolution caching, color tinting, and better memory management.
2. **Tier 2 - Vector SVG Rendering**: Parse SVG and convert to native Skia path/paint calls for resolution-independent rendering.
3. **Tier 3 - SVG DOM**: Lightweight document model enabling dynamic manipulation, style changes, and event handling on SVG elements.

Each tier builds on the previous. Tier 1 is a set of quick wins on the current architecture. Tier 2 is the core investment that unlocks resolution independence and theme-aware rendering. Tier 3 is a future capability for interactive SVG content.

---

## Current Implementation

### What exists

The `ui` crate renders SVG through a rasterization pipeline:

1. `resvg::usvg::Tree::from_data()` parses SVG bytes into a usvg tree
2. `resvg::render()` rasterizes the tree into a `tiny_skia::Pixmap` at a fixed pixel size
3. `pixmap_to_skia_image()` converts the pixmap to a `skia_safe::Image`
4. The Skia image is drawn via `canvas.draw_image_rect()`

Relevant files:
- `ui/src/renderer.rs` - `load_svg()`, `draw_image()`, `pixmap_to_skia_image()`
- `ui/src/element.rs` - `ImageSource::Svg(String)`, `ImageSource::File(String)`
- `dock/src/icons.rs` - icon path resolution from freedesktop icon themes
- `spotlight/src/render.rs` - separate SVG loading for spotlight (uses tiny_skia directly)

### Current limitations

- **Resolution-dependent**: SVG is rasterized once at the element's layout size. No HiDPI scaling, no re-rasterization on resize.
- **Cache key is truncated**: `format!("svg:{}", &s[..s.len().min(64)])` means two SVGs sharing the first 64 bytes get the same cache entry.
- **No cache invalidation**: HashMap grows unbounded. No LRU eviction, no size limits, no re-rasterization when bounds change.
- **No color manipulation**: Cannot tint icons to match theme. No `currentColor` support.
- **No vector rendering**: Everything goes through pixels. Zooming or scaling after rasterization causes blurriness.
- **Synchronous loading**: SVG parsing and rasterization block the render thread.
- **Two separate SVG pipelines**: The `ui` crate uses resvg+skia_safe while `spotlight` uses resvg+tiny_skia directly.

---

## Tier 1: Enhanced Rasterized SVG (Improve Current)

Improve the existing resvg pipeline without changing the rendering approach.

### 1.1 Fix cache key generation

Replace the truncated-string key with a content hash.

```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn svg_cache_key(data: &str, width: u32, height: u32) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    hasher.finish()
}
```

The cache key must include the target dimensions since the same SVG rasterized at different sizes produces different images.

### 1.2 Multi-resolution caching (HiDPI)

Rasterize at `target_size * scale_factor` where scale_factor comes from the Wayland surface's preferred buffer scale. Cache both 1x and 2x variants.

```rust
struct ImageCache {
    entries: HashMap<u64, CacheEntry>,
    total_bytes: usize,
    max_bytes: usize,  // e.g., 64 MB
}

struct CacheEntry {
    image: skia_safe::Image,
    last_used: Instant,
    byte_size: usize,
}
```

When the scale factor changes (e.g., moving between monitors), invalidate and re-rasterize cached SVGs.

### 1.3 LRU cache eviction

Track `last_used` timestamp on each cache entry. When `total_bytes` exceeds `max_bytes`, evict least-recently-used entries until under budget. A reasonable default is 64 MB for the image cache.

### 1.4 Color tinting

Add a `tint` color option to `ImageSource` or as an element property. After rasterizing the SVG, apply a color filter to the resulting image:

```rust
// In draw_image, after getting the skia Image:
if let Some(tint_color) = tint {
    let mut paint = Paint::default();
    let color_filter = skia_safe::color_filters::blend(
        to_skia_color(&tint_color),
        skia_safe::BlendMode::SrcIn,  // tint opaque areas
    );
    paint.set_color_filter(color_filter);
    canvas.draw_image_rect(img, None, dst, &paint);
}
```

This enables monochrome icons to match the current theme color without re-parsing the SVG.

### 1.5 currentColor support via resvg

Before passing SVG data to resvg, preprocess the SVG string to replace `currentColor` with the actual hex color from the element's inherited color:

```rust
fn preprocess_svg(svg_data: &str, current_color: &Color) -> String {
    let hex = format!("#{:02x}{:02x}{:02x}", current_color.r, current_color.g, current_color.b);
    svg_data.replace("currentColor", &hex)
}
```

This is a simple string replacement that works because usvg resolves CSS cascading after parsing.

### 1.6 Async SVG loading

Move SVG parsing and rasterization off the render thread. Return a placeholder (transparent image or shimmer) until the rasterized image is ready.

```rust
enum CacheState {
    Loading,
    Ready(skia_safe::Image),
    Failed,
}
```

Use a dedicated thread or thread pool for rasterization. Signal the main loop to request a re-render when loading completes.

### 1.7 Element API additions

```rust
// New element builder
pub fn icon(svg: &str) -> Element { ... }

// New chainable methods on Element
impl Element {
    pub fn tint(mut self, color: Color) -> Self { ... }
    pub fn image_fit(mut self, fit: ImageFit) -> Self { ... }
}

pub enum ImageFit {
    Contain,    // fit within bounds, maintain aspect ratio (current behavior)
    Cover,      // fill bounds, crop excess
    Fill,       // stretch to fill bounds
    ScaleDown,  // like Contain but never upscale
}
```

### Tier 1 implementation order

1. Fix cache key (hash-based) -- immediate bug fix
2. LRU cache with byte budget
3. Color tinting (SrcIn blend)
4. currentColor preprocessing
5. Multi-resolution rasterization
6. Async loading
7. Element API additions (icon builder, tint, image_fit)

---

## Tier 2: Vector SVG Rendering via Skia Paths

Parse SVG structure and convert to native Skia drawing commands instead of rasterizing through resvg. This achieves resolution-independent rendering and enables dynamic color/style changes without re-rasterization.

### 2.1 Architecture

```
SVG data
  |
  v
usvg::Tree  (resvg's SVG parser -- keep using this)
  |
  v
VectorSvg   (our intermediate representation)
  |
  v
Skia Canvas  (native path/paint drawing)
```

Continue using usvg for parsing. usvg simplifies the full SVG spec into a normalized tree:
- Resolves `<use>` references, CSS cascading, inherited attributes
- Converts all shapes to paths
- Resolves gradients, patterns, clip paths, masks
- Outputs a clean tree of groups, paths, images, and text

The key change: instead of calling `resvg::render()` to rasterize the usvg tree into pixels, walk the tree ourselves and emit Skia drawing calls.

### 2.2 usvg tree node types

usvg normalizes SVG into these node types:

| usvg Node | Description |
|-----------|-------------|
| `Group` | Container with transform, opacity, clip-path, mask, filters |
| `Path` | Fill and/or stroke with path data, paint (solid/gradient/pattern) |
| `Image` | Embedded raster image (PNG/JPEG) or nested SVG |
| `Text` | Text with positioned spans, fonts, decoration |

All SVG shape elements (`<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`) are converted to `Path` nodes by usvg.

### 2.3 SVG path data to Skia Path conversion

SVG path commands map directly to Skia `Path` methods. The usvg tree already provides path data as a sequence of path segments, not raw `d` attribute strings, so parsing is already handled.

#### Complete SVG path command to Skia mapping

| SVG Command | Name | Parameters | Skia Method | Skia Parameters |
|-------------|------|------------|-------------|-----------------|
| `M x y` | Move To (abs) | end point | `path.move_to((x, y))` | `(Point)` |
| `m dx dy` | Move To (rel) | offset | `path.r_move_to((dx, dy))` | `(Vector)` |
| `L x y` | Line To (abs) | end point | `path.line_to((x, y))` | `(Point)` |
| `l dx dy` | Line To (rel) | offset | `path.r_line_to((dx, dy))` | `(Vector)` |
| `H x` | Horizontal Line (abs) | x coordinate | `path.line_to((x, current_y))` | `(Point)` |
| `h dx` | Horizontal Line (rel) | x offset | `path.r_line_to((dx, 0.0))` | `(Vector)` |
| `V y` | Vertical Line (abs) | y coordinate | `path.line_to((current_x, y))` | `(Point)` |
| `v dy` | Vertical Line (rel) | y offset | `path.r_line_to((0.0, dy))` | `(Vector)` |
| `C x1 y1 x2 y2 x y` | Cubic Bezier (abs) | 2 control pts + end | `path.cubic_to((x1,y1), (x2,y2), (x,y))` | `(Point, Point, Point)` |
| `c dx1 dy1 dx2 dy2 dx dy` | Cubic Bezier (rel) | 2 control offsets + end offset | `path.r_cubic_to((dx1,dy1), (dx2,dy2), (dx,dy))` | `(Vector, Vector, Vector)` |
| `S x2 y2 x y` | Smooth Cubic (abs) | reflected cp1 + cp2 + end | Compute cp1 as reflection of previous cp2, then `path.cubic_to(...)` | `(Point, Point, Point)` |
| `s dx2 dy2 dx dy` | Smooth Cubic (rel) | same, relative | Same reflection logic with `r_cubic_to` | `(Vector, Vector, Vector)` |
| `Q x1 y1 x y` | Quadratic Bezier (abs) | control pt + end | `path.quad_to((x1,y1), (x,y))` | `(Point, Point)` |
| `q dx1 dy1 dx dy` | Quadratic Bezier (rel) | control offset + end offset | `path.r_quad_to((dx1,dy1), (dx,dy))` | `(Vector, Vector)` |
| `T x y` | Smooth Quadratic (abs) | reflected cp + end | Compute cp as reflection, then `path.quad_to(...)` | `(Point, Point)` |
| `t dx dy` | Smooth Quadratic (rel) | same, relative | Same reflection logic with `r_quad_to` | `(Vector, Vector)` |
| `A rx ry rot large sweep x y` | Arc (abs) | radii + rotation + flags + end | `path.arc_to(...)` (SVG overload) | `(rx, ry, x_axis_rotate, ArcSize, PathDirection, Point)` |
| `a rx ry rot large sweep dx dy` | Arc (rel) | same, relative | `path.r_arc_to(...)` | `(rx, ry, x_axis_rotate, ArcSize, PathDirection, Vector)` |
| `Z` / `z` | Close Path | none | `path.close()` | none |

#### Arc flag mapping

| SVG `large-arc-flag` | Skia `ArcSize` |
|----------------------|----------------|
| `0` | `ArcSize::Small` |
| `1` | `ArcSize::Large` |

| SVG `sweep-flag` | Skia `PathDirection` |
|-------------------|---------------------|
| `0` | `PathDirection::CCW` |
| `1` | `PathDirection::CW` |

#### Fill rule mapping

| SVG `fill-rule` | Skia `PathFillType` |
|-----------------|---------------------|
| `nonzero` | `PathFillType::Winding` |
| `evenodd` | `PathFillType::EvenOdd` |

#### Notes on smooth curves (S, s, T, t)

usvg normalizes smooth curves into explicit cubic/quadratic curves by computing the reflected control point. So the converter does not need to handle S/s/T/t specially -- usvg already resolves them. Similarly, H/h/V/v are converted to L/l.

#### Skia Path methods not directly from SVG

These Skia path convenience methods have no SVG command equivalent but are useful for programmatic shape construction:

| Skia Method | Description |
|-------------|-------------|
| `path.conic_to(p1, p2, w)` | Conic (weighted quadratic). Weight `w=1` is quadratic, `w<1` approaches line, `w>1` approaches two lines. Used for exact circle arcs. |
| `path.add_rect(rect, dir)` | Add a rectangle as a closed contour |
| `path.add_oval(rect, dir)` | Add an ellipse as a closed contour |
| `path.add_circle(cx, cy, r, dir)` | Add a circle contour |
| `path.add_arc(oval, start_angle, sweep_angle)` | Add an arc (portion of an oval) |
| `path.add_round_rect(rect, rx, ry, dir)` | Add a rounded rectangle contour |
| `path.add_rrect(rrect, dir)` | Add a complex rounded rect (per-corner radii) |
| `path.add_poly(points, close)` | Add a polygon from a list of points |
| `path.add_path(other, dx, dy, mode)` | Append another path with offset |

#### Skia path operations (boolean ops)

| `PathOp` Variant | Description | SVG Equivalent |
|------------------|-------------|----------------|
| `PathOp::Difference` | Subtract second from first | `clip-path` with `clip-rule` |
| `PathOp::Intersect` | Keep only overlap | `clip-path` intersection |
| `PathOp::Union` | Combine both | Merging shapes |
| `PathOp::XOR` | Keep non-overlapping | Even-odd clipping |
| `PathOp::ReverseDifference` | Subtract first from second | Inverted clipping |

Usage: `path1.op(&path2, PathOp::Union)` returns a new path.

### 2.4 SVG paint to Skia Paint conversion

#### Solid color

```rust
fn svg_color_to_paint(color: &usvg::Color, opacity: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(skia_safe::Color::from_argb(
        (opacity * 255.0) as u8,
        color.red,
        color.green,
        color.blue,
    ));
    paint
}
```

#### Linear gradient

SVG `<linearGradient>` maps to Skia's `Shader::linear_gradient()`:

| SVG Attribute | Skia Parameter |
|--------------|----------------|
| `x1, y1, x2, y2` | `points: [Point; 2]` |
| `<stop offset="..." stop-color="..." stop-opacity="...">` | `colors: &[Color]`, `positions: &[f32]` |
| `gradientUnits="objectBoundingBox"` | Apply bounding box transform to gradient points |
| `gradientUnits="userSpaceOnUse"` | Use gradient points as-is |
| `gradientTransform` | Pre-multiply into the gradient's local matrix |
| `spreadMethod="pad"` | `TileMode::Clamp` |
| `spreadMethod="reflect"` | `TileMode::Mirror` |
| `spreadMethod="repeat"` | `TileMode::Repeat` |

```rust
let shader = Shader::linear_gradient(
    (start_point, end_point),
    colors.as_ref(),
    positions.as_ref(),
    tile_mode,
    None,  // flags
    Some(&gradient_transform),
);
paint.set_shader(shader);
```

#### Radial gradient

SVG `<radialGradient>` maps to Skia's `Shader::two_point_conical_gradient()`:

| SVG Attribute | Skia Parameter |
|--------------|----------------|
| `cx, cy` | `end` center point |
| `r` | `end_radius` |
| `fx, fy` | `start` center point (focal point) |
| `fr` | `start_radius` (0 if not specified) |
| `spreadMethod` | `TileMode` (same as linear) |
| `gradientTransform` | Local matrix |

```rust
let shader = Shader::two_point_conical_gradient(
    focal_point,    // SVG fx, fy
    focal_radius,   // SVG fr (default 0)
    center_point,   // SVG cx, cy
    radius,         // SVG r
    colors.as_ref(),
    positions.as_ref(),
    tile_mode,
    None,
    Some(&gradient_transform),
);
```

#### Pattern

SVG `<pattern>` maps to rendering the pattern content into a Skia `Picture`, then creating a `Shader::from_picture()` with `TileMode::Repeat`.

#### Stroke attributes

| SVG Attribute | Skia Paint/Stroke Method |
|--------------|--------------------------|
| `stroke` | `paint.set_color(...)` with `paint.set_style(PaintStyle::Stroke)` |
| `stroke-width` | `paint.set_stroke_width(w)` |
| `stroke-linecap="butt"` | `paint.set_stroke_cap(Cap::Butt)` |
| `stroke-linecap="round"` | `paint.set_stroke_cap(Cap::Round)` |
| `stroke-linecap="square"` | `paint.set_stroke_cap(Cap::Square)` |
| `stroke-linejoin="miter"` | `paint.set_stroke_join(Join::Miter)` |
| `stroke-linejoin="round"` | `paint.set_stroke_join(Join::Round)` |
| `stroke-linejoin="bevel"` | `paint.set_stroke_join(Join::Bevel)` |
| `stroke-miterlimit` | `paint.set_stroke_miter(limit)` |
| `stroke-dasharray` | `PathEffect::dash(intervals, phase)` via `paint.set_path_effect(...)` |
| `stroke-dashoffset` | Phase parameter in dash path effect |
| `stroke-opacity` | Multiply into paint alpha |

### 2.5 SVG transforms to Skia Matrix

SVG transforms map to Skia's `Matrix`:

| SVG Transform | Skia Matrix Method |
|--------------|-------------------|
| `translate(tx, ty)` | `Matrix::translate((tx, ty))` |
| `rotate(angle)` | `Matrix::rotate_deg(angle)` |
| `rotate(angle, cx, cy)` | `Matrix::rotate_deg_pivot(angle, (cx, cy))` |
| `scale(sx, sy)` | `Matrix::scale((sx, sy))` |
| `skewX(angle)` | `Matrix::skew((angle_rad.tan(), 0.0))` |
| `skewY(angle)` | `Matrix::skew((0.0, angle_rad.tan()))` |
| `matrix(a, b, c, d, e, f)` | `Matrix::new_all(a, c, e, b, d, f, 0, 0, 1)` |

Note the SVG matrix parameter order `(a, b, c, d, e, f)` maps to the affine matrix:
```
| a  c  e |
| b  d  f |
| 0  0  1 |
```

Apply via `canvas.concat(&matrix)` within a `canvas.save()`/`canvas.restore()` pair.

### 2.6 Clipping and masking

#### clip-path

SVG `<clipPath>` defines a clipping region using paths. Map to:

```rust
canvas.save();
canvas.clip_path(&clip_path, ClipOp::Intersect, true /* anti-alias */);
// draw clipped content
canvas.restore();
```

The clip path's `clip-rule` maps to the path's fill type (`Winding` or `EvenOdd`).

#### mask

SVG `<mask>` uses luminance or alpha of the mask content to control opacity. Map to:

1. Render mask content into an offscreen surface
2. Use the result as an alpha mask via `SaveLayerRec` with the mask image

```rust
// Render mask content to offscreen surface
let mask_image = render_to_image(&mask_node, bounds);

// Apply mask using Skia layer
canvas.save();
let paint = Paint::default();
// Use the mask as a shader for the layer's alpha
let rec = SaveLayerRec::default().paint(&paint);
canvas.save_layer(&rec);
// draw content
canvas.restore();
// apply mask via PorterDuff SrcIn blend
canvas.restore();
```

### 2.7 Walking the usvg tree

```rust
fn render_usvg_node(canvas: &Canvas, node: &usvg::Node, state: &mut RenderState) {
    match node {
        usvg::Node::Group(group) => {
            canvas.save();

            // Apply transform
            let ts = group.transform();
            let matrix = usvg_transform_to_skia(ts);
            canvas.concat(&matrix);

            // Apply opacity via layer
            if group.opacity().get() < 1.0 {
                let mut paint = Paint::default();
                paint.set_alpha_f(group.opacity().get());
                canvas.save_layer(&SaveLayerRec::default().paint(&paint));
            }

            // Apply clip-path
            if let Some(clip) = group.clip_path() {
                let clip_skia_path = usvg_clip_to_skia(clip);
                canvas.clip_path(&clip_skia_path, ClipOp::Intersect, true);
            }

            // Render children
            for child in group.children() {
                render_usvg_node(canvas, child, state);
            }

            // Pop opacity layer
            if group.opacity().get() < 1.0 {
                canvas.restore();
            }

            canvas.restore();
        }

        usvg::Node::Path(path) => {
            let skia_path = usvg_path_to_skia(path.data());

            // Fill
            if let Some(fill) = path.fill() {
                let paint = usvg_paint_to_skia(&fill.paint(), fill.opacity(), state);
                let mut p = skia_path.clone();
                p.set_fill_type(match fill.rule() {
                    usvg::FillRule::NonZero => PathFillType::Winding,
                    usvg::FillRule::EvenOdd => PathFillType::EvenOdd,
                });
                canvas.draw_path(&p, &paint);
            }

            // Stroke
            if let Some(stroke) = path.stroke() {
                let mut paint = usvg_paint_to_skia(&stroke.paint(), stroke.opacity(), state);
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(stroke.width().get());
                // ... set cap, join, dash, miter
                canvas.draw_path(&skia_path, &paint);
            }
        }

        usvg::Node::Image(image) => {
            // Rasterize embedded images via the existing pipeline
        }

        usvg::Node::Text(text) => {
            // Text rendering -- see section 2.9
        }
    }
}
```

### 2.8 SVG viewBox and preserveAspectRatio

The SVG `viewBox` attribute defines the coordinate system. `preserveAspectRatio` controls how the viewBox maps to the viewport.

usvg provides `tree.size()` (the viewport) and `tree.view_box()` (viewBox + preserveAspectRatio). The view box transform should be applied as the root transform before rendering any nodes:

```rust
fn compute_viewbox_transform(
    view_box: &usvg::ViewBox,
    viewport_width: f32,
    viewport_height: f32,
) -> Matrix {
    // usvg resolves this for us via tree.view_box().transform
    // which encodes the preserveAspectRatio scaling/translation
}
```

### 2.9 Text rendering

SVG text is complex. usvg resolves text layout (positioning, tspan offsets, textPath) into a flattened list of positioned character clusters. Each cluster has:
- Glyph position (x, y)
- Font properties (family, size, weight, style)
- Fill and stroke paints
- Decoration (underline, overline, line-through)

Map to Skia text rendering:

```rust
// For each text chunk from usvg:
let typeface = font_mgr.match_family_style(family, font_style);
let font = Font::from_typeface(typeface, font_size);
let mut paint = usvg_paint_to_skia(&fill_paint, opacity, state);
canvas.draw_str(&text, (x, y), &font, &paint);
```

For `textPath` (text along a path), use Skia's `canvas.draw_text_on_path()` or manually position each glyph using `PathMeasure::get_pos_tan()` to compute position and rotation at each offset along the path.

### 2.10 Cached vector SVG

Parse the usvg tree once and store the converted Skia commands as a `Picture` (recorded drawing):

```rust
struct VectorSvg {
    picture: skia_safe::Picture,
    bounds: Rect,
    intrinsic_size: (f32, f32),  // from SVG viewBox/width/height
}

impl VectorSvg {
    fn from_svg(data: &[u8]) -> Option<Self> {
        let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;

        // Record all drawing into a Picture
        let bounds = skia_safe::Rect::from_wh(
            tree.size().width(),
            tree.size().height(),
        );
        let mut recorder = PictureRecorder::new();
        let canvas = recorder.begin_recording(bounds, None);

        let mut state = RenderState::default();
        for child in tree.root().children() {
            render_usvg_node(canvas, child, &mut state);
        }

        let picture = recorder.finish_recording_as_picture(Some(&bounds))?;
        Some(Self {
            picture,
            bounds: Rect { x: 0.0, y: 0.0, width: bounds.width(), height: bounds.height() },
            intrinsic_size: (tree.size().width(), tree.size().height()),
        })
    }

    fn draw(&self, canvas: &Canvas, dest: &Rect) {
        canvas.save();
        let sx = dest.width / self.intrinsic_size.0;
        let sy = dest.height / self.intrinsic_size.1;
        canvas.translate((dest.x, dest.y));
        canvas.scale((sx, sy));
        canvas.draw_picture(&self.picture, None, None);
        canvas.restore();
    }
}
```

A `Picture` is resolution-independent -- Skia replays the recorded draw commands at whatever transform is current, producing sharp output at any scale. This eliminates the rasterization-at-fixed-size problem entirely.

### 2.11 When to use vector vs rasterized

| Scenario | Approach | Reason |
|----------|----------|--------|
| Small icons (16-48px) | Rasterized (Tier 1) | Faster, pixel-hinted, cache-friendly |
| Icons that change color | Vector (Tier 2) | Re-render with different paint, no re-parse |
| Large illustrations | Vector (Tier 2) | Resolution independent |
| Complex SVGs with many filters | Rasterized (Tier 1) | Filter → Skia mapping is complex |
| Animated SVG elements | Vector (Tier 2) | Can modify individual paths per frame |
| Static decorative elements | Either | Cache as Picture or rasterize once |

### 2.12 Performance comparison

**Rasterized (resvg):**
- Parse + rasterize once: ~1-5ms per icon
- Draw cached image: ~0.01ms (GPU texture blit)
- Memory: pixel buffer (w * h * 4 bytes per cached size)
- Re-render on size change: full re-rasterize

**Vector (Skia Picture):**
- Parse + record once: ~0.5-2ms per icon
- Replay Picture: ~0.05-0.2ms (GPU path rendering)
- Memory: command buffer (typically smaller than pixels for simple SVGs)
- Re-render on size change: free (just change transform)

For a dock with 10-20 icons, the vector approach uses less memory and handles HiDPI/animations seamlessly. For a file manager displaying hundreds of thumbnails, rasterized caching is more efficient.

---

## Tier 3: SVG DOM (Future)

A lightweight SVG document model that supports querying and modifying SVG elements after parsing. This enables dynamic SVG manipulation without re-parsing from string.

### 3.1 Use cases

- **Themed icons**: Change fill colors of specific paths based on theme
- **State-driven icons**: Toggle visibility of sub-elements (e.g., checkbox checked/unchecked)
- **Animated icons**: Interpolate path data or transforms per frame
- **Interactive diagrams**: Hit-test SVG elements, highlight on hover
- **Data visualization**: Update chart paths/text when data changes

### 3.2 Document model

```rust
pub struct SvgDocument {
    tree: usvg::Tree,       // parsed tree (source of truth)
    elements: Vec<SvgElement>,  // flattened element list with IDs
    id_map: HashMap<String, usize>,  // id -> element index
    dirty: bool,            // needs re-recording
    picture: Option<skia_safe::Picture>,  // cached rendering
}

pub struct SvgElement {
    id: Option<String>,
    kind: SvgElementKind,
    transform: Matrix,
    opacity: f32,
    fill: Option<SvgPaint>,
    stroke: Option<SvgStroke>,
    visible: bool,
    children: Vec<usize>,   // indices into elements vec
}

pub enum SvgElementKind {
    Group,
    Path { data: skia_safe::Path },
    Text { content: String, position: Point },
    Image { source: ImageSource },
}

pub enum SvgPaint {
    Solid(Color),
    LinearGradient { /* ... */ },
    RadialGradient { /* ... */ },
}
```

### 3.3 Manipulation API

```rust
impl SvgDocument {
    /// Load from SVG data
    pub fn from_data(svg: &[u8]) -> Result<Self, Error>;

    /// Query element by SVG id attribute
    pub fn element_by_id(&self, id: &str) -> Option<&SvgElement>;
    pub fn element_by_id_mut(&mut self, id: &str) -> Option<&mut SvgElement>;

    /// Modify element attributes
    pub fn set_fill(&mut self, id: &str, color: Color);
    pub fn set_stroke(&mut self, id: &str, color: Color, width: f32);
    pub fn set_opacity(&mut self, id: &str, opacity: f32);
    pub fn set_transform(&mut self, id: &str, transform: Matrix);
    pub fn set_visible(&mut self, id: &str, visible: bool);
    pub fn set_text(&mut self, id: &str, content: &str);

    /// Re-record the Skia Picture after modifications
    pub fn update(&mut self);

    /// Draw to canvas
    pub fn draw(&self, canvas: &Canvas, bounds: &Rect);

    /// Hit testing
    pub fn hit_test(&self, point: Point) -> Option<&str>;  // returns element id
}
```

### 3.4 Event handling on SVG elements

Extend the framework's event system to dispatch events to SVG sub-elements:

```rust
// In the element builder:
pub fn svg_document(doc: SvgDocument) -> Element {
    Element {
        kind: ElementKind::SvgDocument(doc),
        ..Default::default()
    }
}

// SVG-level event callbacks:
impl Element {
    pub fn on_svg_click(mut self, f: impl Fn(&str) + 'static) -> Self {
        // f receives the SVG element id that was clicked
    }
    pub fn on_svg_hover(mut self, f: impl Fn(Option<&str>) + 'static) -> Self {
        // f receives Some(id) on enter, None on leave
    }
}
```

Hit testing walks the element tree in reverse paint order, testing each path with `path.contains(point)`.

---

## SVG as Drawing API

Expose Skia's path building through an SVG-like API for constructing custom shapes in the element tree without needing SVG files.

### 4.1 Custom shape element

```rust
pub fn shape(build: impl FnOnce(&mut ShapeBuilder)) -> Element {
    let mut builder = ShapeBuilder::new();
    build(&mut builder);
    Element {
        kind: ElementKind::Shape(builder.finish()),
        ..Default::default()
    }
}

pub struct ShapeBuilder {
    path: skia_safe::Path,
    fill: Option<Color>,
    stroke: Option<(Color, f32)>,
}

impl ShapeBuilder {
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self;
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self;
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self;
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> &mut Self;
    pub fn arc_to(&mut self, rx: f32, ry: f32, rotation: f32, large: bool, sweep: bool, x: f32, y: f32) -> &mut Self;
    pub fn close(&mut self) -> &mut Self;

    // Convenience shapes
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self;
    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> &mut Self;
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32) -> &mut Self;
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> &mut Self;

    // Paint
    pub fn fill(&mut self, color: Color) -> &mut Self;
    pub fn stroke(&mut self, color: Color, width: f32) -> &mut Self;

    // From SVG path string
    pub fn svg_path(&mut self, d: &str) -> &mut Self;
}
```

### 4.2 SVG path string parser

Parse SVG `d` attribute strings directly into Skia paths. This is useful for embedding small SVG path snippets inline in Rust code (common for icon libraries).

```rust
/// Parse an SVG path `d` attribute string into a Skia Path.
pub fn parse_svg_path(d: &str) -> Option<skia_safe::Path> {
    // Use a lightweight parser (or usvg's path parser)
    // to convert M/L/C/Q/A/Z commands to Skia path methods.
}

// Usage:
let check_icon = shape(|s| {
    s.svg_path("M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z")
     .fill(Color::WHITE);
});
```

### 4.3 Path animation

Skia's `Path::interpolate()` enables morphing between two paths with the same verb structure:

```rust
if path_a.is_interpolatable(&path_b) {
    let mut result = Path::new();
    path_a.interpolate(&path_b, t, &mut result);  // t in 0.0..1.0
    canvas.draw_path(&result, &paint);
}
```

For paths with different structures, decompose both into a common set of cubic curves first, then interpolate point-by-point.

Skia's `PathMeasure` enables animating along a path:

```rust
let measure = PathMeasure::new(&path, false);
let length = measure.get_length();

// Get position and tangent at distance along path
let distance = t * length;  // t in 0.0..1.0
let (pos, tan) = measure.get_pos_tan(distance);

// Or get a transformation matrix for positioning an element along the path
let matrix = measure.get_matrix(distance, MatrixFlags::GET_POS_AND_TAN);
```

---

## SVG Filter to Skia Filter Mapping

SVG filter primitives map to Skia's `image_filters` module. Each SVG filter primitive element becomes one or more Skia `ImageFilter` objects, composed via their `input` parameter.

### Complete mapping table

| SVG Filter Primitive | Skia ImageFilter/ColorFilter | Notes |
|---------------------|------------------------------|-------|
| `<feGaussianBlur stdDeviation="sx sy">` | `image_filters::blur((sx, sy), tile_mode, input, crop)` | If only one stdDeviation value, use it for both axes |
| `<feOffset dx="dx" dy="dy">` | `image_filters::offset((dx, dy), input, crop)` | Simple translation of input image |
| `<feColorMatrix type="matrix" values="...">` | `image_filters::color_filter(ColorFilters::matrix(values), input, crop)` | 4x5 matrix in row-major order (20 floats) |
| `<feColorMatrix type="saturate" values="s">` | `image_filters::color_filter(ColorFilters::matrix(saturate_matrix(s)), ...)` | Build 4x5 matrix from saturation value |
| `<feColorMatrix type="hueRotate" values="deg">` | `image_filters::color_filter(ColorFilters::matrix(hue_rotate_matrix(deg)), ...)` | Build 4x5 matrix from angle |
| `<feColorMatrix type="luminanceToAlpha">` | `image_filters::color_filter(ColorFilters::matrix(lum_to_alpha_matrix()), ...)` | Fixed matrix: R*0.2126 + G*0.7152 + B*0.0722 |
| `<feComponentTransfer>` with `<feFuncR/G/B/A>` | `image_filters::color_filter(ColorFilters::table_argb(a, r, g, b), ...)` | Build 256-entry lookup tables per component |
| `<feComposite operator="over">` | `image_filters::blend(BlendMode::SrcOver, bg, fg, crop)` | Standard alpha compositing |
| `<feComposite operator="in">` | `image_filters::blend(BlendMode::SrcIn, bg, fg, crop)` | Keep fg where bg is opaque |
| `<feComposite operator="out">` | `image_filters::blend(BlendMode::SrcOut, bg, fg, crop)` | Keep fg where bg is transparent |
| `<feComposite operator="atop">` | `image_filters::blend(BlendMode::SrcATop, bg, fg, crop)` | fg atop bg |
| `<feComposite operator="xor">` | `image_filters::blend(BlendMode::Xor, bg, fg, crop)` | XOR compositing |
| `<feComposite operator="arithmetic" k1 k2 k3 k4>` | `image_filters::arithmetic(k1, k2, k3, k4, enforce_pm, bg, fg, crop)` | `result = k1*in1*in2 + k2*in1 + k3*in2 + k4` |
| `<feBlend mode="normal">` | `image_filters::blend(BlendMode::SrcOver, bg, fg, crop)` | Same as composite over |
| `<feBlend mode="multiply">` | `image_filters::blend(BlendMode::Multiply, bg, fg, crop)` | |
| `<feBlend mode="screen">` | `image_filters::blend(BlendMode::Screen, bg, fg, crop)` | |
| `<feBlend mode="darken">` | `image_filters::blend(BlendMode::Darken, bg, fg, crop)` | |
| `<feBlend mode="lighten">` | `image_filters::blend(BlendMode::Lighten, bg, fg, crop)` | |
| `<feMorphology operator="erode" radius="rx ry">` | `image_filters::erode((rx, ry), input, crop)` | Shrink opaque areas |
| `<feMorphology operator="dilate" radius="rx ry">` | `image_filters::dilate((rx, ry), input, crop)` | Expand opaque areas |
| `<feDropShadow dx dy stdDeviation="s" flood-color flood-opacity>` | `image_filters::drop_shadow((dx, dy), (s, s), color, input, crop)` | Combined offset + blur + color |
| `<feFlood flood-color flood-opacity>` | `image_filters::shader(Shader::color(color), crop)` | Fill region with solid color |
| `<feMerge>` with N `<feMergeNode>` inputs | `image_filters::merge(filters, crop)` | Composite N inputs in order |
| `<feImage href="...">` | `image_filters::image(image, src, dst, sampling)` | Embedded image as filter input |
| `<feTile>` | `image_filters::tile(src_rect, dst_rect, input)` | Tile input across region |
| `<feTurbulence type="turbulence" baseFrequency seed>` | `image_filters::shader(Shader::make_fractal_noise(...), crop)` | Perlin fractal noise |
| `<feTurbulence type="fractalNoise" baseFrequency seed>` | `image_filters::shader(Shader::make_turbulence(...), crop)` | Perlin turbulence |
| `<feConvolveMatrix>` | `image_filters::matrix_convolution(...)` | Kernel convolution |
| `<feDisplacementMap>` | `image_filters::displacement_map(x_sel, y_sel, scale, displacement, color, crop)` | Pixel displacement |
| `<feDiffuseLighting>` | No direct Skia equivalent | Must implement custom or approximate |
| `<feSpecularLighting>` | No direct Skia equivalent | Must implement custom or approximate |

### SVG feColorMatrix type="matrix" format

The SVG 4x5 matrix has 20 values in row-major order:

```
| R' |   | a00 a01 a02 a03 a04 |   | R |
| G' | = | a10 a11 a12 a13 a14 | * | G |
| B' |   | a20 a21 a22 a23 a24 |   | B |
| A' |   | a30 a31 a32 a33 a34 |   | A |
                                    | 1 |
```

Skia's `ColorFilters::matrix()` takes the same 20-float row-major format.

### SVG feComponentTransfer function types

| SVG `type` | Implementation |
|-----------|----------------|
| `identity` | `table[i] = i` |
| `table` | Interpolate between `tableValues` entries |
| `discrete` | Step function from `tableValues` |
| `linear` | `table[i] = slope * i + intercept` |
| `gamma` | `table[i] = amplitude * pow(i/255, exponent) + offset` |

Build a 256-entry lookup table for each channel, then use `ColorFilters::table_argb()`.

### Filter region and coordinate systems

SVG filters operate in a filter region defined by `x`, `y`, `width`, `height` on the `<filter>` element. The `filterUnits` attribute determines whether these are in `objectBoundingBox` (default, 0-1 range relative to element) or `userSpaceOnUse` (absolute coordinates).

Map to Skia's `CropRect` parameter on image filters:

```rust
let crop = skia_safe::IRect::from_xywh(x, y, w, h);
```

### Lighting filters (approximate)

SVG's `feDiffuseLighting` and `feSpecularLighting` use the alpha channel as a height map for 3D lighting effects. Skia has no direct equivalent. Options:

1. **Skip**: Most UI icons don't use lighting filters
2. **Approximate**: Use `feColorMatrix` + `feGaussianBlur` to simulate the effect
3. **Custom shader**: Write a Skia shader that implements the lighting math

Recommendation: skip for Tier 2, implement if needed in Tier 3.

---

## Icon System Design

### 5.1 Icon element type

```rust
// Builder function
pub fn icon(name: &str) -> Element {
    Element {
        kind: ElementKind::Icon {
            name: name.to_string(),
            source: None,  // resolved lazily
        },
        width: Some(24.0),   // default icon size
        height: Some(24.0),
        ..Default::default()
    }
}

// Usage:
icon("folder")
    .size(32.0, 32.0)
    .tint(theme.icon_color)
```

### 5.2 Icon resolution pipeline

```
icon("folder")
  |
  v
IconRegistry lookup
  |-- registered inline SVG?  --> use it
  |-- registered icon pack?   --> load from pack
  |-- freedesktop theme?      --> find_icon_path() from theme dirs
  |-- fallback                --> placeholder/missing icon
  |
  v
Load SVG data
  |
  v
Render (vector or rasterized based on complexity/size)
```

### 5.3 Icon registry

```rust
pub struct IconRegistry {
    /// Inline SVG data by name
    inline: HashMap<String, String>,
    /// Icon pack directories
    packs: Vec<IconPack>,
    /// Freedesktop theme search paths
    theme_paths: Vec<String>,
    /// Active theme name
    theme: String,
}

pub struct IconPack {
    name: String,
    /// Map of icon name -> SVG string
    icons: HashMap<String, String>,
}

impl IconRegistry {
    /// Register a single inline SVG icon
    pub fn register(&mut self, name: &str, svg: &str);

    /// Register an icon pack from a directory of SVG files
    pub fn register_pack(&mut self, name: &str, dir: &str);

    /// Resolve an icon name to SVG data
    pub fn resolve(&self, name: &str) -> Option<String>;
}
```

### 5.4 Icon coloring strategies

| Strategy | When to use | How |
|----------|-------------|-----|
| **Tint (SrcIn blend)** | Monochrome icons | Apply color filter to entire rasterized image |
| **currentColor replacement** | SVG icons using `currentColor` | String-replace before parsing |
| **CSS variable replacement** | Multi-color themed icons | Replace named colors in SVG string |
| **Per-element color** | Tier 3 DOM manipulation | Modify specific element fills/strokes |

### 5.5 Icon sizing and alignment

Icons should be treated as fixed-aspect-ratio elements. The icon's viewBox defines its intrinsic aspect ratio. When placed in a layout:

- Default size: 24x24 (Material Design standard) or configurable per icon pack
- Scale to fit container while preserving aspect ratio (Contain mode)
- Align within container using the parent's `align_items` and `justify`

### 5.6 Popular icon libraries

| Library | Icons | License | Format |
|---------|-------|---------|--------|
| Material Symbols | 3000+ | Apache 2.0 | SVG, icon font |
| Lucide | 1500+ | ISC | SVG |
| Phosphor | 7000+ | MIT | SVG |
| Tabler Icons | 4000+ | MIT | SVG |
| Heroicons | 300+ | MIT | SVG |
| Freedesktop (Breeze, Adwaita) | 2000+ | LGPL/GPL | SVG, installed on system |

For a Linux desktop environment, the freedesktop icon theme (Breeze for KDE) is the primary source. The icon registry should fall back to it after checking custom/inline icons.

### 5.7 Symbol + use pattern

SVG sprite sheets use `<symbol>` + `<use>` to define multiple icons in a single SVG file:

```xml
<svg xmlns="http://www.w3.org/2000/svg">
  <symbol id="icon-folder" viewBox="0 0 24 24">
    <path d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/>
  </symbol>
  <symbol id="icon-file" viewBox="0 0 24 24">
    <path d="M..."/>
  </symbol>
</svg>
```

usvg resolves `<use>` references during parsing, so sprite sheets work transparently. For the icon registry, parse the sprite SVG once, then extract individual `<symbol>` elements by id.

---

## SVG Elements Reference

Complete list of SVG elements and their rendering status across tiers.

### Shape elements (all converted to paths by usvg)

| SVG Element | Attributes | usvg | Tier 1 | Tier 2 |
|-------------|-----------|------|--------|--------|
| `<rect>` | x, y, width, height, rx, ry | Path | Yes | Yes |
| `<circle>` | cx, cy, r | Path | Yes | Yes |
| `<ellipse>` | cx, cy, rx, ry | Path | Yes | Yes |
| `<line>` | x1, y1, x2, y2 | Path | Yes | Yes |
| `<polyline>` | points | Path | Yes | Yes |
| `<polygon>` | points | Path | Yes | Yes |
| `<path>` | d | Path | Yes | Yes |

### Container elements

| SVG Element | Purpose | usvg | Tier 1 | Tier 2 |
|-------------|---------|------|--------|--------|
| `<svg>` | Root / nested viewport | Group | Yes | Yes |
| `<g>` | Group with shared attributes | Group | Yes | Yes |
| `<defs>` | Non-rendered definitions | Resolved | Yes | Yes |
| `<symbol>` | Reusable template | Resolved via use | Yes | Yes |
| `<use>` | Reference to symbol/element | Inlined | Yes | Yes |

### Paint servers

| SVG Element | Skia Equivalent | Tier 1 | Tier 2 |
|-------------|----------------|--------|--------|
| `<linearGradient>` | `Shader::linear_gradient()` | Yes (resvg) | Yes |
| `<radialGradient>` | `Shader::two_point_conical_gradient()` | Yes (resvg) | Yes |
| `<pattern>` | `Shader::from_picture()` with repeat | Yes (resvg) | Yes |
| `<stop>` | Gradient color/position array entries | Yes (resvg) | Yes |

### Clip, mask, filter

| SVG Element | Skia Equivalent | Tier 1 | Tier 2 |
|-------------|----------------|--------|--------|
| `<clipPath>` | `canvas.clip_path()` | Yes (resvg) | Yes |
| `<mask>` | SaveLayer + alpha mask | Yes (resvg) | Yes |
| `<filter>` | Chain of `ImageFilter` objects | Yes (resvg) | Partial |
| `<marker>` | Instantiated at path vertices | Yes (resvg) | Deferred |

### Text elements

| SVG Element | Tier 1 | Tier 2 | Tier 3 |
|-------------|--------|--------|--------|
| `<text>` | Yes (resvg) | Yes | Yes |
| `<tspan>` | Yes (resvg) | Yes | Yes |
| `<textPath>` | Yes (resvg) | Deferred | Yes |

### Elements to skip

| SVG Element | Reason |
|-------------|--------|
| `<animate>`, `<animateTransform>`, `<animateMotion>`, `<set>` | SMIL animation -- use framework animation system instead |
| `<script>` | JavaScript execution -- not applicable |
| `<foreignObject>` | Embeds HTML -- not applicable in this context |
| `<a>` | Hyperlinks -- handle at framework level if needed |
| `<switch>` | Feature detection -- not relevant |

---

## SVG Animation (SMIL) -- Not Implemented

SVG defines SMIL-based animation elements (`<animate>`, `<animateTransform>`, `<animateMotion>`, `<set>`). These are **not implemented** and should not be. Reasons:

1. Chrome deprecated SMIL (then un-deprecated but stopped adding features)
2. The framework already has its own animation system (springs, enter/exit, layout animations)
3. SMIL requires a running timeline tied to the SVG document, conflicting with the framework's declarative render model

Instead, SVG animation is achieved by:
- Using the framework's animation system to interpolate element properties (opacity, transform, position)
- In Tier 3, modifying SVG element attributes from animation callbacks
- Using `Path::interpolate()` for path morphing animations

---

## CSS in SVG

SVG elements can be styled via CSS using `<style>` blocks or inline `style` attributes. usvg resolves all CSS styling during parsing, converting to presentation attributes. This means:

- CSS-in-SVG works transparently through usvg in both Tier 1 and Tier 2
- No CSS engine is needed in the renderer
- `currentColor` is the one CSS value that needs runtime substitution (see Tier 1.5)

### Supported CSS properties in SVG context

These are the CSS properties that can style SVG elements (all resolved by usvg):

`fill`, `fill-opacity`, `fill-rule`, `stroke`, `stroke-dasharray`, `stroke-dashoffset`, `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`, `stroke-opacity`, `stroke-width`, `opacity`, `display`, `visibility`, `clip-path`, `clip-rule`, `mask`, `filter`, `color`, `font-family`, `font-size`, `font-style`, `font-weight`, `font-variant`, `text-anchor`, `text-decoration`, `letter-spacing`, `word-spacing`, `writing-mode`, `direction`, `dominant-baseline`, `alignment-baseline`, `baseline-shift`, `stop-color`, `stop-opacity`, `flood-color`, `flood-opacity`, `lighting-color`, `color-interpolation`, `color-interpolation-filters`, `paint-order`, `pointer-events`, `shape-rendering`, `image-rendering`, `text-rendering`, `vector-effect`

---

## Skia Path Effects Reference

Path effects modify how paths are stroked. Relevant for SVG `stroke-dasharray` and decorative effects.

| Skia PathEffect | Description | SVG Equivalent |
|----------------|-------------|----------------|
| `DashPathEffect::new(intervals, phase)` | Dashed/dotted line | `stroke-dasharray`, `stroke-dashoffset` |
| `CornerPathEffect::new(radius)` | Round sharp corners | No direct SVG equivalent |
| `DiscretePathEffect::new(seg_length, deviation)` | Random displacement | No direct SVG equivalent |
| `PathEffect::sum(first, second)` | Apply both effects to original, sum results | No direct SVG equivalent |
| `PathEffect::compose(outer, inner)` | Apply inner first, then outer to result | No direct SVG equivalent |
| `TrimPathEffect::new(start, stop, mode)` | Draw only a portion of the path | Useful for line-drawing animations |

### Dash pattern conversion

SVG `stroke-dasharray="5 3 2 3"` with `stroke-dashoffset="2"`:

```rust
let intervals = [5.0, 3.0, 2.0, 3.0];
let phase = 2.0;  // dashoffset
let effect = DashPathEffect::new(&intervals, phase);
paint.set_path_effect(effect);
```

If the SVG dasharray has an odd number of values, it is repeated to produce an even-length array (per SVG spec).

---

## Skia PathMeasure Reference

PathMeasure provides tools for measuring and sampling paths. Essential for text-on-path, path animations, and progress indicators.

| Method | Parameters | Returns | Use |
|--------|-----------|---------|-----|
| `new(path, force_closed)` | Path, bool | PathMeasure | Create from path |
| `get_length()` | -- | f32 | Total path length |
| `get_pos_tan(distance)` | f32 | (Point, Vector) | Position and tangent at distance |
| `get_segment(start, stop, dst, start_with_move)` | f32, f32, &mut Path, bool | bool | Extract sub-path |
| `is_closed()` | -- | bool | Whether contour is closed |
| `next_contour()` | -- | bool | Advance to next contour |
| `get_matrix(distance, flags)` | f32, MatrixFlags | Matrix | Transform at distance |

`MatrixFlags`: `GET_POSITION`, `GET_TANGENT`, or both (`GET_POS_AND_TAN`).

---

## Implementation Order

### Phase 1: Tier 1 improvements (estimated: 2-3 days)
1. Fix cache key to use content hash + dimensions
2. Add LRU eviction with byte budget
3. Add `tint` color support on image/icon elements
4. Add `currentColor` preprocessing

### Phase 2: SVG path parser (estimated: 1-2 days)
1. Implement `parse_svg_path()` for inline SVG path strings
2. Add `shape()` element builder with `ShapeBuilder`
3. Add `DrawCommand::Path` variant to display list
4. Render paths in `SkiaRenderer::execute()`

### Phase 3: Vector SVG renderer (estimated: 3-5 days)
1. Walk usvg tree and emit Skia drawing calls
2. Handle solid fills, strokes, transforms
3. Handle gradients (linear, radial)
4. Handle clip paths
5. Record into `Picture` for caching
6. Add `VectorSvg` type and cache

### Phase 4: Icon system (estimated: 2-3 days)
1. `IconRegistry` with inline and pack registration
2. `icon()` element builder
3. Freedesktop theme fallback (reuse existing `find_icon_path`)
4. Auto-select rasterized vs vector based on size/complexity

### Phase 5: Filters (estimated: 3-4 days)
1. Map common filter primitives (blur, offset, color matrix, composite)
2. Handle filter chains (compose filter pipeline)
3. Map blend modes
4. Handle morphology (erode/dilate)
5. Skip lighting filters

### Phase 6: Advanced features (estimated: 2-3 days)
1. Masks
2. Patterns
3. Text rendering from usvg
4. Multi-resolution caching / HiDPI
5. Async loading

### Phase 7: SVG DOM -- Tier 3 (estimated: 5-7 days)
1. `SvgDocument` type with element tree
2. ID-based lookup and attribute modification
3. Re-recording on changes (dirty flag)
4. Hit testing
5. SVG element event dispatch

### Total estimated effort: 18-27 days

---

## Performance Considerations

### When to rasterize vs vector render

- **Rasterize** when: icon is small (under 48px), static, displayed many times at same size, or has complex filters
- **Vector render** when: icon changes color frequently, displayed at multiple sizes, needs to be resolution-independent, or is animated

### Caching hierarchy

```
Level 1: VectorSvg (parsed usvg tree + recorded Picture)
  - Keyed by SVG content hash
  - Resolution-independent
  - Cheap to re-draw at any size

Level 2: Rasterized image cache (skia_safe::Image)
  - Keyed by content hash + target size + scale factor
  - GPU texture, fastest to draw
  - LRU eviction with byte budget

Level 3: Disk cache (optional, future)
  - Pre-rasterized PNGs at common sizes
  - Avoids SVG parsing on app startup
```

### Memory budget guidelines

| Cache | Budget | Rationale |
|-------|--------|-----------|
| Vector SVG (Pictures) | 16 MB | Pictures are compact; 100 icons ~ 1-2 MB |
| Rasterized images | 64 MB | 48x48 RGBA = 9 KB each; 64 MB = ~7000 icons |
| Icon registry (SVG strings) | 8 MB | Raw SVG text is small |

### GPU considerations

- Skia `Picture` replay leverages GPU path rendering when backed by a GPU canvas
- Complex paths with many cubic segments may be slower on GPU than a pre-rasterized texture
- For icons drawn every frame (e.g., in an animation), prefer `Picture` over rasterized to avoid texture upload latency
- For static icons drawn once, rasterized is fine since the texture stays resident

### Profiling approach

Add timing instrumentation to:
1. SVG parse time (`usvg::Tree::from_data`)
2. Picture record time (tree walk + Skia calls)
3. Picture replay time (`canvas.draw_picture`)
4. Rasterization time (`resvg::render`)
5. Cache hit/miss rates

Compare vector vs rasterized for the actual icon set used by the dock and system apps to determine the optimal default strategy.
