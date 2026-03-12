# UI Framework: Surface Resize + Wayland Globals APIs

## Problem

The dock can't use `ui::run()` because of two missing capabilities:

1. **No surface resize** — `run_wayland()` sets the layer-shell surface size once at startup. The dock needs to expand/contract as apps appear/disappear.
2. **No custom Wayland protocol access** — The dock needs `org_kde_plasma_window_management` (KDE-specific). The framework creates the Wayland connection internally and never exposes it.

As a result, the dock duplicates ~400 lines of Wayland bootstrap, event loop, SHM buffer management, and Skia rendering that the framework already handles.

## API 1: Surface Resize via RenderContext

### Design

Add two methods to `RenderContext`:

```rust
impl RenderContext {
    /// Current surface dimensions.
    fn surface_size(&self) -> (u32, u32);

    /// Request the surface be resized. Takes effect next frame.
    fn set_surface_size(&mut self, width: u32, height: u32);
}
```

RenderContext gains two new fields:

```rust
pub struct RenderContext {
    mutations_any: Rc<dyn Any>,
    surface_width: u32,        // new
    surface_height: u32,       // new
    requested_size: Option<(u32, u32)>,  // new
}
```

### Framework integration (`wayland.rs`)

In `draw()`, after `render_fn` returns:

1. Check `cx.requested_size`
2. If different from current `(self.width, self.height)`:
   - Update `self.width` / `self.height`
   - Call `layer_surface.set_size(w, h)` + commit
   - Resize the SHM pool if needed
   - Mark `needs_redraw = true` (current frame may have rendered at old size)

The dock handles the expand-before-animate pattern itself: it requests a surface large enough for the target width before the animation starts, then shrinks after animation settles. This matches how it works today.

### Why RenderContext and not a separate handle

- RenderContext already exists and is passed to every render call
- Surface size is inherently tied to the render cycle — you need to know the size to build the tree, and you set it based on the tree you're building
- No new types or lifetime complexity

## API 2: Wayland Globals Access via View Lifecycle Methods

### Design

Add two lifecycle methods to the `View` trait:

```rust
pub trait View: 'static {
    /// Called once after Wayland connection is established, before the event loop.
    /// Use to bind custom Wayland protocols on the shared connection.
    fn setup(&mut self, _wl: &WaylandContext) {}

    /// Called each frame before render(). Use for polling custom event queues,
    /// updating internal state, etc.
    fn tick(&mut self) {}

    /// Build the element tree for this frame.
    fn render(&self, cx: &mut RenderContext) -> Element;
}
```

`WaylandContext` exposes what's needed to bind protocols:

```rust
pub struct WaylandContext {
    pub connection: Connection,   // Connection is Clone (Arc internally)
    pub globals: GlobalList,      // from registry_queue_init
}
```

### How the dock uses this

```rust
struct DockView {
    tracker: WindowTracker,
    event_queue: Option<EventQueue<WindowTracker>>,
    // ...
}

impl View for DockView {
    fn setup(&mut self, wl: &WaylandContext) {
        // Create a separate event queue for plasma protocols
        let queue = wl.connection.new_event_queue::<WindowTracker>();
        let qh = queue.handle();
        let _plasma_wm: OrgKdePlasmaWindowManagement =
            wl.globals.bind(&qh, 1..=16, ()).expect("plasma wm not available");
        self.event_queue = Some(queue);
    }

    fn tick(&mut self) {
        // Poll plasma events each frame
        if let Some(queue) = &mut self.event_queue {
            queue.dispatch_pending(&mut self.tracker).ok();
        }
    }

    fn render(&self, cx: &mut RenderContext) -> Element {
        let apps = self.tracker.get_dock_apps();
        let needed_width = dock_width(apps.len());
        cx.set_surface_size(needed_width, DOCK_HEIGHT);
        self.build_ui(cx.surface_size(), &apps)
    }
}
```

Key insight: the dock creates its **own event queue** on the shared connection. This means:

- Dispatch impls for plasma protocols live on `WindowTracker` (or a wrapper), not on the framework's `WaylandState`
- No generic complexity on `WaylandState`
- Framework doesn't need to know about custom protocols at all
- Events flow through the same Wayland connection, just dispatched to a different queue

### Framework integration (`wayland.rs`)

In `run_wayland()`:

1. After creating `Connection` + `GlobalList`, construct `WaylandContext`
2. Call `view.setup(&wl_context)` before entering the event loop
3. In the main loop, call `tick_fn()` (type-erased) each frame before render

`ViewState` gains:

```rust
struct ViewState {
    render_fn: Box<dyn Fn(&mut RenderContext) -> Element>,
    apply_fn: Box<dyn Fn()>,
    has_mutations_fn: Box<dyn Fn() -> bool>,
    tick_fn: Box<dyn Fn()>,           // new
    mutations_rc: Rc<dyn Any>,
}
```

## Also: SurfaceConfig margin

The dock needs `set_margin(0, 0, 8, 0)`. Add an optional margin field to `SurfaceConfig::LayerShell`:

```rust
SurfaceConfig::LayerShell {
    // ...existing fields...
    margin: (i32, i32, i32, i32),  // top, right, bottom, left
}
```

## Files to modify

| File                 | Changes                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------- |
| `ui/src/app.rs`      | Add `setup()`, `tick()` to View trait; add `WaylandContext`; add margin to SurfaceConfig      |
| `ui/src/reactive.rs` | Add `surface_size()`, `set_surface_size()`, new fields to RenderContext                       |
| `ui/src/wayland.rs`  | Construct WaylandContext, call setup/tick, handle resize requests, pass size to RenderContext |
| `ui/src/lib.rs`      | Re-export `WaylandContext`                                                                    |

## Dock Migration

### Current structure (what gets deleted)

The dock currently duplicates framework responsibilities:

- **Wayland bootstrap** (`dock.rs:119-185`): Connection, registry, compositor, layer-shell, SHM pool, event loop — all handled by `ui::run()`
- **Smithay handler impls** (`dock.rs:627-747`): CompositorHandler, OutputHandler, LayerShellHandler, SeatHandler, ShmHandler, ProvidesRegistryState, delegate macros — all in `wayland.rs` already
- **Skia surface + SHM buffer** (`dock.rs:469-532`): create surface, render, copy pixels, attach buffer — handled by framework's `draw()`
- **Manual animation system** (`dock.rs:64-77, 100-106, 219-438`): AnimSlot, x/y springs, width spring, phased tick logic — replaced by framework's `Animator` with keyed elements

### New structure

**`main.rs`** — minimal:
```rust
fn main() {
    env_logger::init();
    ui::run(dock::DockView::new(), SurfaceConfig::LayerShell {
        namespace: "dock".into(),
        layer: Layer::Top,
        anchor: Anchor::BOTTOM,
        size: (200, 64),
        exclusive_zone: 0,
        keyboard: KeyboardInteractivity::None,
        margin: (0, 0, 8, 0),
    });
}
```

**`dock.rs`** — becomes a View impl (~100 lines instead of ~750):
```rust
pub struct DockView {
    tracker: PlasmaState,
    event_queue: Option<EventQueue<PlasmaState>>,
}

impl View for DockView {
    fn setup(&mut self, wl: &WaylandContext) { /* bind plasma protocol */ }
    fn tick(&mut self) { /* dispatch_pending on plasma queue */ }
    fn render(&self, cx: &mut RenderContext) -> Element { /* build element tree */ }
}
```

**`windows.rs`** — mostly unchanged, but Dispatch forwarding impls move from `Dock` to `PlasmaState`:
```rust
/// Wrapper state type for the plasma event queue.
/// WindowTracker's generic Dispatch impls work with any D: AsMut<WindowTracker>.
struct PlasmaState {
    tracker: WindowTracker,
}
impl AsMut<WindowTracker> for PlasmaState { ... }

// Forwarding Dispatch impls (same pattern as current Dock impls,
// just on PlasmaState instead)
impl Dispatch<OrgKdePlasmaWindowManagement, ()> for PlasmaState { ... }
impl Dispatch<OrgKdePlasmaWindow, u32> for PlasmaState { ... }
```

**`icons.rs`** — icon path lookup stays (needed to pass paths to `image_file()`), but the Skia image loading (`load_icon`, `load_svg`, `load_raster`) is deleted since the framework's renderer handles `ImageSource::File`.

### Element tree (render)

The entire dock UI becomes declarative. Icons are `image_file()` elements with animation directives — no more direct Skia drawing.

```rust
fn render(&self, cx: &mut RenderContext) -> Element {
    let apps = self.tracker.tracker.get_dock_apps();

    // Request surface wide enough for all icons
    let target_width = dock_width(apps.len());
    cx.set_surface_size(target_width, DOCK_HEIGHT);
    let (sw, sh) = cx.surface_size();

    row()
        .size(sw as f32, sh as f32)
        .justify(Justify::Center)
        .align_items(Alignment::Center)
        .child(
            row()
                .key("dock-bg")
                .animate_layout()  // width animates when icons added/removed
                .padding(DOCK_PADDING)
                .gap(ICON_PADDING)
                .background(rgba(30, 30, 30, 190))
                .rounded(DOCK_RADIUS)
                .border(rgba(255, 255, 255, 30), 1.0)
                .clip()
                .align_items(Alignment::Center)
                .children(apps.iter().map(|app| {
                    let icon = icons::find_icon_path(&app.icon_name);
                    let el = match icon {
                        Some(path) => image_file(&path).size(ICON_SIZE, ICON_SIZE),
                        None => container()
                            .size(ICON_SIZE, ICON_SIZE)
                            .rounded(ICON_SIZE / 2.0)
                            .background(rgba(80, 80, 80, 200)),
                    };
                    el.key(&app.app_id)
                      .initial(From::new().offset_y(DOCK_HEIGHT as f32))
                      .exit(To::new().offset_y(20.0))
                      .animate_layout()
                }))
        )
}
```

### Animation behavior change

Current dock has **phased** animations (expand first, then icon slides in). To replicate this without a full sequencing system, add a `delay_ms` field to `From`:

```rust
el.key(&app.app_id)
  .initial(From::new().offset_y(DOCK_HEIGHT as f32).delay_ms(150))
  .exit(To::new().offset_y(20.0))
  .animate_layout()
```

The Animator holds the icon at its `From` state for the delay duration (while the background's `animate_layout` expands the dock), then starts the slide-up. This gives the phased look: dock expands → icon slides in.

Implementation in `Animator`: add `delay_remaining: f32` to `ElementAnim`. During `step()`, decrement the delay first; only start stepping the actual property animations once the delay reaches zero.

Files affected: `ui/src/animation.rs` (add `delay_ms` to `From`/`To`), `ui/src/animator.rs` (delay logic in `ElementAnim::step`).

### What's deleted

| File | Lines removed | Reason |
|------|--------------|--------|
| `dock.rs` | ~650 of 748 | Wayland bootstrap, handler impls, manual animation, direct Skia rendering |
| `icons.rs` | ~50 of 100 | Skia image loading (framework renderer handles it) |

### What's kept

| File | What stays |
|------|-----------|
| `windows.rs` | WindowTracker + Dispatch impls (re-targeted to PlasmaState) |
| `icons.rs` | `find_icon_path()` (resolves icon name → filesystem path) |

## Files to modify

| File | Changes |
|------|---------|
| `ui/src/app.rs` | Add `setup()`, `tick()` to View trait; add `WaylandContext`; add margin to SurfaceConfig |
| `ui/src/reactive.rs` | Add `surface_size()`, `set_surface_size()`, new fields to RenderContext |
| `ui/src/wayland.rs` | Construct WaylandContext, call setup/tick, handle resize requests, pass size to RenderContext |
| `ui/src/lib.rs` | Re-export `WaylandContext` |
| `dock/src/main.rs` | Replace `dock::Dock::run()` with `ui::run(DockView, config)` |
| `dock/src/dock.rs` | Rewrite: ~750 lines → ~100 lines (View impl + element tree) |
| `dock/src/windows.rs` | Move forwarding Dispatch impls from Dock to PlasmaState wrapper |
| `dock/src/icons.rs` | Delete Skia loading, keep path resolution |
| `dock/Cargo.toml` | Remove direct deps now handled by framework (skia-safe, calloop, calloop-wayland-source, smithay-client-toolkit) |

## Verification

1. Build ui on VM: `cd /root/os && cargo build --release -p ui`
2. Build dock on VM: `cargo build --release -p dock`
3. Install + run: kill old dock, copy binary, launch as mars user
4. Open apps (dolphin, konsole), verify icons appear with enter animation
5. Close apps, verify exit animation
6. Check active indicator dot on focused app
