# Phase 2: Display List & Skia Renderer

## Goal

Take a laid-out element tree and produce pixels. This phase introduces the `DrawCommand` display list as the abstraction boundary, and a `SkiaRenderer` that executes commands on a Skia canvas. By the end, you can render an element tree to an image buffer in a test.

## Dependencies added

```bash
cargo add skia-safe@0.82 --features vulkan,textlayout
```

Replaces `tiny-skia`. The `textlayout` feature gives us HarfBuzz shaping and proper font metrics (needed for text measurement in the layout pass from Phase 1).

## Steps

### 2.1 Display list

`src/display_list.rs` — the draw command enum.

```rust
pub enum DrawCommand {
    Rect {
        bounds: Rect,
        background: Color,
        corner_radius: f32,
        border: Option<Border>,
    },
    Text {
        text: String,
        position: Point,
        font_size: f32,
        color: Color,
    },
    Image {
        source: ImageSource,
        bounds: Rect,
    },
    BoxShadow {
        bounds: Rect,
        corner_radius: f32,
        blur: f32,
        spread: f32,
        color: Color,
        offset: Point,
    },
    PushClip {
        bounds: Rect,
        corner_radius: f32,
    },
    PopClip,
    PushLayer {
        opacity: f32,
    },
    PopLayer,
    BackdropBlur {
        bounds: Rect,
        corner_radius: f32,
        blur_radius: f32,
    },
    PushTranslate {
        offset: Point,
    },
    PopTranslate,
}

pub struct Point {
    pub x: f32,
    pub y: f32,
}
```

### 2.2 Display list builder

`src/display_list.rs` — walk the `LayoutNode` tree and emit draw commands.

```rust
pub fn build_display_list(root: &LayoutNode, elements: &[Element]) -> Vec<DrawCommand> { ... }
```

For each node:

1. If `clip` is set → `PushClip`
2. If `opacity < 1.0` → `PushLayer { opacity }`
3. If `background` is set → `DrawCommand::Rect` with bounds, corner_radius, border
4. Recurse into children
5. If the element is `Text` → `DrawCommand::Text` at the node's position
6. If the element is `Image` → `DrawCommand::Image`
7. Pop any pushed layers/clips in reverse order

### 2.3 Skia renderer

`src/renderer.rs` — executes draw commands on a `skia_safe::Canvas`.

```rust
pub struct SkiaRenderer {
    font_mgr: FontMgr,
    typeface_cache: HashMap<String, Typeface>,
}

impl SkiaRenderer {
    pub fn new() -> Self { ... }
    pub fn execute(&mut self, canvas: &Canvas, commands: &[DrawCommand]) { ... }
}
```

Command mapping (as specified in `backend.md`):

| DrawCommand     | Skia calls                                                                                |
| --------------- | ----------------------------------------------------------------------------------------- |
| `Rect`          | `canvas.draw_rrect(rrect, &fill_paint)` + optional stroke                                 |
| `Text`          | `canvas.draw_str(text, position, &font, &paint)`                                          |
| `Image`         | Load SVG via `resvg` → rasterize to Skia `Image`, or load PNG. `canvas.draw_image_rect()` |
| `BoxShadow`     | offset rrect + `MaskFilter::blur()`                                                       |
| `PushClip`      | `canvas.save()` + `canvas.clip_rrect()`                                                   |
| `PopClip`       | `canvas.restore()`                                                                        |
| `PushLayer`     | `canvas.save_layer_alpha_f()`                                                             |
| `PopLayer`      | `canvas.restore()`                                                                        |
| `BackdropBlur`  | `canvas.save()` + clip + `image_filters::blur()` + `save_layer` + restore                 |
| `PushTranslate` | `canvas.save()` + `canvas.translate()`                                                    |
| `PopTranslate`  | `canvas.restore()`                                                                        |

### 2.4 Text measurement

Update the layout pass (Phase 1) to use Skia for accurate text measurement instead of placeholder heuristics.

```rust
pub fn measure_text(text: &str, font_size: f32, font_mgr: &FontMgr) -> (f32, f32) {
    let typeface = font_mgr.legacy_make_typeface(None, FontStyle::default()).unwrap();
    let font = Font::new(typeface, font_size);
    let (width, _) = font.measure_str(text, None);
    let metrics = font.metrics();
    let height = metrics.1.descent - metrics.1.ascent;
    (width, height)
}
```

Taffy's `MeasureFunc` for text nodes calls this to get intrinsic size.

### 2.5 Image loading

`src/image_loader.rs` — load SVG and raster images into Skia `Image` objects.

```rust
pub fn load_svg(data: &[u8], target_size: (u32, u32)) -> Option<skia_safe::Image> { ... }
pub fn load_png(data: &[u8]) -> Option<skia_safe::Image> { ... }
pub fn load_image_file(path: &str, target_size: (u32, u32)) -> Option<skia_safe::Image> { ... }
```

SVG loading reuses the `resvg` approach from the current dock code but renders into a Skia surface instead of a tiny-skia pixmap.

Cache loaded images by path to avoid reloading every frame.

### 2.6 Validation

Write tests that:

1. Build an element tree → layout → display list → verify expected command sequence
2. Create a raster `SkSurface`, execute commands, verify the output isn't blank (basic smoke test)
3. Test clipping: child outside parent bounds is not visible
4. Test opacity layers
5. Test text rendering produces non-zero pixels at expected positions

## Output

The rendering pipeline is complete: element tree → layout → display list → Skia → pixel buffer. This can be tested offline without Wayland. The framework never calls Skia directly — all rendering goes through the display list abstraction.
