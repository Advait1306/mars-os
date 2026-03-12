# Phase 3: Wayland Bootstrap & Event Loop

## Goal

Connect to the Wayland compositor, create surfaces, and present rendered frames on screen. By the end of this phase, a minimal program can display a static UI on a Wayland desktop using `ui::run()`.

## Dependencies added

```bash
cargo add smithay-client-toolkit@0.19 --features calloop
cargo add wayland-client@0.31
cargo add calloop@0.14
cargo add calloop-wayland-source@0.4
cargo add ash@0.38  # Vulkan (stretch — SHM first)
```

## Steps

### 3.1 Application config types

`src/app.rs` — surface configuration and entry point.

```rust
pub enum SurfaceConfig {
    LayerShell {
        namespace: String,
        layer: Layer,
        anchor: Anchor,
        size: (u32, u32),
        exclusive_zone: i32,
        keyboard: KeyboardInteractivity,
    },
    Toplevel {
        title: String,
        app_id: String,
    },
}
```

### 3.2 Wayland connection & state

`src/wayland.rs` — SCTK setup, globals binding, surface creation.

```rust
struct WaylandState {
    registry: RegistryState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    xdg_shell: XdgShell,
    seat: SeatState,
    output: OutputState,
    shm: Shm,

    surface: Surface,           // active surface (layer or toplevel)
    pool: SlotPool,             // SHM buffer pool
    width: u32,
    height: u32,
    scale_factor: i32,
    configured: bool,
    needs_redraw: bool,
}
```

Implement all the SCTK handler traits (`CompositorHandler`, `LayerShellHandler`, `SeatHandler`, `OutputHandler`, `ShmHandler`, `ProvidesRegistryState`), similar to the current dock code but generalized — not dock-specific.

### 3.3 SHM presentation (CPU rendering)

Start with shared memory buffers — same approach as the current dock but using Skia's raster backend instead of tiny-skia.

Frame flow:
1. Create a raster `SkSurface` backed by the SHM buffer
2. Run render pipeline: element tree → layout → display list → SkiaRenderer → canvas
3. Attach buffer to `wl_surface`, damage, commit

Pixel format conversion: Skia raster surfaces use RGBA/BGRA; Wayland SHM uses ARGB8888 (BGRA in little-endian). Handle the conversion at buffer commit time, same as dock's current `draw()`.

### 3.4 Frame scheduling

Integrate with Wayland's `frame` callback for vsync:

1. After commit, request a `frame` callback
2. When callback fires, check if dirty → re-render
3. When idle (no dirty state), don't request frame callbacks → zero CPU

The calloop event loop blocks on `dispatch()` when nothing is happening.

### 3.5 `ui::run()` entry point

`src/lib.rs` — the public entry point.

```rust
pub fn run<V: View + 'static>(view: V, config: SurfaceConfig) {
    // 1. Connect to Wayland
    // 2. Bind globals
    // 3. Create surface with requested role
    // 4. Create SHM pool + raster SkSurface
    // 5. Initial render: view.render() → layout → display list → paint
    // 6. Enter calloop event loop
}
```

For this phase, `View` is a minimal trait:

```rust
pub trait View: 'static {
    fn render(&self) -> Element;
}
```

(No `RenderContext` yet — that comes in Phase 4 with reactive state.)

### 3.6 DPI scaling

Read `scale_factor` from `OutputState`. Multiply logical dimensions by scale factor when creating the `SkSurface`. Set `wl_surface.set_buffer_scale()`. All layout math stays in logical pixels.

Fractional scaling support (`wp_fractional_scale_v1`) can be added later — integer scaling is sufficient to start.

### 3.7 Surface resize handling

For layer shell: compositor sends `configure` with new size → resize SHM pool, recreate `SkSurface`, re-render.

For toplevel: handle `configure` events for user-initiated resizes.

### 3.8 Validation

Write a minimal test app:

```rust
struct HelloView;

impl View for HelloView {
    fn render(&self) -> Element {
        container()
            .background(rgba(30, 30, 30, 255))
            .rounded(12.0)
            .padding(20.0)
            .child(text("Hello from ui framework").font_size(18.0).color(WHITE))
    }
}

fn main() {
    ui::run(HelloView, SurfaceConfig::LayerShell {
        namespace: "test".into(),
        layer: Layer::Top,
        anchor: Anchor::BOTTOM,
        size: (300, 60),
        exclusive_zone: 0,
        keyboard: KeyboardInteractivity::None,
    });
}
```

Transfer to VM, build, run — verify a styled text label appears on screen.

## Stretch: Vulkan GPU backend

After SHM works, optionally add Vulkan surface creation:
1. Create `VkInstance` + `VkDevice` via `ash`
2. Create `VkSurface` from the Wayland surface
3. Create `GrDirectContext` from the Vulkan backend
4. Create GPU-backed `SkSurface` targeting swapchain images
5. Same display list, same renderer — just a different `SkSurface`

This can also be deferred to a later phase since SHM + Skia's CPU rasterizer is fast enough for the dock.

## Output

A working `ui::run()` that displays a static element tree on a Wayland compositor. The full pipeline is wired end-to-end: builder API → layout → display list → Skia → Wayland surface.
