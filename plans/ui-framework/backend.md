# Rendering Backend

## Choice: skia-safe

The rendering backend is **skia-safe** (Rust bindings to Skia). Skia handles both GPU and CPU rendering through a unified API — no need for separate backends or tiny-skia.

### Why Skia

- Battle-tested (Chrome, Flutter, Android)
- Full feature set out of the box: blur, backdrop blur, shadows, rounded rects, anti-aliased paths, subpixel text, opacity layers, clipping, image filters
- GPU acceleration via Vulkan or OpenGL — same API for CPU fallback
- Builds natively on Linux ARM64/x86_64 (no cross-compilation needed, we build in the VM)

## GPU Context

### Primary: Vulkan

On Wayland, the framework creates a Vulkan surface and hands it to Skia's GPU backend (Ganesh).

```
Wayland surface → VkSurface → Skia GrDirectContext → SkSurface (GPU-backed)
```

Setup:
1. Get Wayland `wl_surface` from the compositor
2. Create a Vulkan instance + device (via `ash` or Skia's built-in Vulkan setup)
3. Create `GrDirectContext` from the Vulkan backend context
4. Create GPU-backed `SkSurface` targeting the swapchain images
5. All `Canvas` drawing calls are now GPU-accelerated

### Fallback: CPU

If Vulkan/GL isn't available, Skia falls back to its software rasterizer — same `Canvas` API, just backed by a raster `SkSurface`. The framework code doesn't change.

```
Wayland surface → shared memory buffer → SkSurface (raster-backed)
```

The fallback is automatic — detect GPU availability at startup, create the appropriate `SkSurface`.

## Rendering Pipeline

Each frame follows this pipeline:

```
Element tree → Layout → Display list → Skia Canvas calls → GPU/CPU → Wayland buffer
```

### 1. Element Tree

`render()` produces the element tree (see [components.md](components.md)).

### 2. Layout (Taffy)

Layout is handled by **Taffy** — a production-grade flexbox/grid layout engine in pure Rust. Used by GPUI (Zed), Dioxus, and Servo.

Each element in the tree maps to a Taffy node. Style properties (`.width()`, `.padding()`, `.gap()`, `.align_items()`, `.justify()`, `.fill_width()`) translate directly to Taffy's `Style` struct:

```rust
use taffy::prelude::*;

fn build_layout(tree: &mut TaffyTree, element: &Element) -> NodeId {
    let style = Style {
        display: Display::Flex,
        flex_direction: match element.direction {
            Direction::Row => FlexDirection::Row,
            Direction::Column => FlexDirection::Column,
        },
        size: Size {
            width: element.width.map_or(Dimension::Auto, Dimension::Length),
            height: element.height.map_or(Dimension::Auto, Dimension::Length),
        },
        padding: Rect::from_lengths(element.padding),
        gap: Size { width: LengthPercentage::Length(element.gap), ..Default::default() },
        align_items: element.align_items.map(Into::into),
        justify_content: element.justify.map(Into::into),
        ..Default::default()
    };

    let children: Vec<NodeId> = element.children.iter()
        .map(|child| build_layout(tree, child))
        .collect();

    tree.new_with_children(style, &children).unwrap()
}
```

After `tree.compute_layout(root, available_space)`, each node has resolved `x`, `y`, `width`, `height` via `tree.layout(node_id)`. Output is a flat list of `LayoutNode` with resolved bounds.

### 3. Display List

The laid-out tree is walked to produce an ordered list of draw commands:

```rust
enum DrawCommand {
    Rect { bounds: Rect, background: Color, corner_radius: f32, border: Option<Border> },
    Text { text: String, position: Point, font_size: f32, color: Color },
    Image { source: ImageSource, bounds: Rect },
    BoxShadow { bounds: Rect, corner_radius: f32, blur: f32, spread: f32, color: Color, offset: Point },
    PushClip { bounds: Rect, corner_radius: f32 },
    PopClip,
    PushLayer { opacity: f32 },
    PopLayer,
    BackdropBlur { bounds: Rect, corner_radius: f32, blur_radius: f32 },
}
```

The display list is the boundary between the UI framework and the renderer. The framework never calls Skia directly — it emits `DrawCommand`s and the renderer consumes them.

### 4. Skia Execution

A `SkiaRenderer` walks the display list and maps each command to Skia `Canvas` calls:

```rust
impl SkiaRenderer {
    fn execute(&mut self, canvas: &Canvas, commands: &[DrawCommand]) {
        for cmd in commands {
            match cmd {
                DrawCommand::Rect { bounds, background, corner_radius, border } => {
                    let rrect = RRect::new_rect_xy(bounds, *corner_radius, *corner_radius);
                    let mut paint = Paint::new(background, None);
                    paint.set_anti_alias(true);
                    canvas.draw_rrect(rrect, &paint);

                    if let Some(border) = border {
                        let mut stroke = Paint::new(&border.color, None);
                        stroke.set_style(PaintStyle::Stroke);
                        stroke.set_stroke_width(border.width);
                        canvas.draw_rrect(rrect, &stroke);
                    }
                }

                DrawCommand::BackdropBlur { bounds, corner_radius, blur_radius } => {
                    // Save, clip to region, apply backdrop image filter, restore
                    canvas.save();
                    let rrect = RRect::new_rect_xy(bounds, *corner_radius, *corner_radius);
                    canvas.clip_rrect(rrect, ClipOp::Intersect, true);
                    let blur = image_filters::blur(
                        (*blur_radius, *blur_radius), TileMode::Clamp, None, None
                    );
                    let paint = Paint::default();
                    paint.set_image_filter(blur);
                    canvas.save_layer_alpha_f(bounds, 1.0); // applies backdrop filter
                    canvas.restore();
                    canvas.restore();
                }

                DrawCommand::BoxShadow { bounds, corner_radius, blur, color, offset, .. } => {
                    let shadow_bounds = bounds.with_offset(*offset);
                    let rrect = RRect::new_rect_xy(&shadow_bounds, *corner_radius, *corner_radius);
                    let mut paint = Paint::new(color, None);
                    paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, *blur));
                    canvas.draw_rrect(rrect, &paint);
                }

                DrawCommand::PushLayer { opacity } => {
                    canvas.save_layer_alpha_f(&canvas.local_clip_bounds().unwrap(), *opacity);
                }
                DrawCommand::PopLayer => { canvas.restore(); }

                DrawCommand::PushClip { bounds, corner_radius } => {
                    canvas.save();
                    let rrect = RRect::new_rect_xy(bounds, *corner_radius, *corner_radius);
                    canvas.clip_rrect(rrect, ClipOp::Intersect, true);
                }
                DrawCommand::PopClip => { canvas.restore(); }

                // Text, Image — similar direct Canvas calls
                _ => {}
            }
        }
    }
}
```

## Text Rendering

Skia's text stack handles shaping (via HarfBuzz/ICU internally), font fallback, and subpixel positioning.

```rust
DrawCommand::Text { .. } => {
    let font = Font::new(typeface, font_size);
    let mut paint = Paint::new(color, None);
    paint.set_anti_alias(true);
    canvas.draw_str(text, position, &font, &paint);
}
```

Font loading: use Skia's `FontMgr` to load system fonts. Cache `Typeface` objects — they're expensive to create, cheap to reuse.

## Frame Scheduling

The framework drives rendering off Wayland's `frame` callback for vsync alignment:

1. Wayland compositor sends `frame` callback → framework knows it can draw
2. If dirty (reactive state changed or animation in flight): run layout, build display list, execute on canvas
3. Swap buffers / commit Wayland surface
4. Request next `frame` callback

During animations, the framework requests a frame callback every frame. When idle (no animations, no state changes), no frames are drawn — zero CPU/GPU usage.

## Damage Tracking

Optimization for partial redraws:

1. When reactive state changes, the framework knows which elements are affected
2. Compute the bounding rect of changed elements (union of old and new bounds)
3. Set the damage region on the Wayland surface (`wl_surface.damage_buffer`)
4. Clip Skia's canvas to the damage region before drawing
5. Only elements intersecting the damage region are re-rendered

Full-screen blur/transparency effects may defeat damage tracking (changing one element behind a blur affects the blur output). In those cases, fall back to full redraw — Skia on a GPU handles this fine at 60fps.

## Dependencies

```toml
[dependencies]
skia-safe = { version = "0.82", features = ["vulkan", "textlayout"] }  # GPU + HarfBuzz text shaping
ash = "0.38"                    # Vulkan bindings (for surface setup)
smithay-client-toolkit = "0.19" # Wayland client
taffy = "0.7"                   # Flexbox/grid layout engine
```

- `skia-safe` with `vulkan` pulls in Skia's Ganesh Vulkan backend. The `textlayout` feature enables HarfBuzz shaping, paragraph layout, and font fallback. The build downloads pre-built Skia binaries or compiles from source — on Linux ARM64/x86_64 this works out of the box.
- `taffy` is a pure Rust layout engine implementing CSS Flexbox and CSS Grid. Zero dependencies, no C++ code. Used in production by GPUI (Zed editor), Dioxus, and Servo.
