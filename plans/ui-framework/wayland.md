# Wayland Surface Bootstrap

How the framework connects to the Wayland compositor, creates surfaces, and runs the event loop. This is the layer between the OS compositor and everything in [backend.md](backend.md).

## Dependencies

```toml
smithay-client-toolkit = "0.19"  # Wayland client protocol, layer shell, seat handling
wayland-client = "0.31"          # Low-level Wayland protocol (re-exported by SCTK)
calloop = "0.14"                 # Event loop (used by SCTK internally)
```

`smithay-client-toolkit` (SCTK) handles all Wayland protocol plumbing — registry binding, surface lifecycle, input seats, and the layer shell protocol. We don't use `wayland-client` directly except where SCTK exposes the underlying types.

## Connection & Registry

At startup the framework connects to the Wayland compositor and binds the globals it needs:

```rust
use smithay_client_toolkit::reexports::calloop;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::shm::ShmState;

struct AppState {
    compositor: CompositorState,
    xdg_shell: XdgShell,
    layer_shell: LayerShell,
    seat: SeatState,
    output: OutputState,
    shm: ShmState,
    // per-surface state, input state, etc.
}
```

SCTK's `registry_handlers!` macro auto-dispatches registry events to bind these globals. The framework binds both `XdgShell` and `LayerShell` at startup so either surface type can be created without reconnecting.

## Surface Types

The framework supports three Wayland surface roles. Each application declares which role it needs at creation time — a surface's role is fixed for its lifetime.

### Toplevel (regular windows)

Standard application windows managed by the compositor's window manager. The compositor controls positioning, stacking, and decorations.

```rust
enum SurfaceRole {
    Toplevel { title: String, app_id: String },
    Layer { .. },
    Popup { .. },
}
```

Created via SCTK's `XdgShell`:

```rust
let surface = compositor.create_surface(&queue_handle);
let xdg_surface = xdg_shell.create_window(
    surface,
    WindowDecorations::RequestServer,  // let compositor draw title bar
    &queue_handle,
);
xdg_surface.set_title(title);
xdg_surface.set_app_id(app_id);
xdg_surface.commit();
```

Use for: settings apps, file managers, any "normal" application.

### Layer Shell (dock, panels, overlays)

Surfaces anchored to screen edges, managed outside the normal window stack. Uses the `wlr-layer-shell-unstable-v1` protocol, supported by KWin (KDE), Sway, Hyprland, and most wlroots compositors.

```rust
use smithay_client_toolkit::shell::wlr_layer::{Layer, Anchor, KeyboardInteractivity};

enum SurfaceRole {
    Toplevel { .. },
    Layer {
        namespace: String,       // e.g. "mars-dock", "mars-launcher"
        layer: Layer,            // Background, Bottom, Top, Overlay
        anchor: Anchor,          // which edges to attach to
        size: (u32, u32),        // requested size (0 = stretch to anchored edges)
        exclusive_zone: i32,     // pixels reserved from screen edge (-1 = no zone)
        keyboard: KeyboardInteractivity,
    },
    Popup { .. },
}
```

Created via SCTK's `LayerShell`:

```rust
let surface = compositor.create_surface(&queue_handle);
let layer_surface = layer_shell.create_layer_surface(
    &queue_handle,
    surface,
    layer,
    Some(namespace),
    None,  // output — None = compositor's choice
);
layer_surface.set_anchor(anchor);
layer_surface.set_size(width, height);
layer_surface.set_exclusive_zone(exclusive_zone);
layer_surface.set_keyboard_interactivity(keyboard);
layer_surface.commit();
```

**Layers** (back to front):
- `Background` — behind all windows (desktop widgets)
- `Bottom` — below windows (desktop icons)
- `Top` — above windows (**dock**, panels, notification popups)
- `Overlay` — above everything (screen lock, screenshots, **app launcher**)

**Anchor + size** define geometry:
- Dock: `Anchor::BOTTOM | LEFT | RIGHT`, height = 68, width = 0 (stretch). `exclusive_zone = 68` reserves 68px at the bottom so windows don't overlap the dock.
- App launcher: `Anchor::TOP`, centered, fixed size. `exclusive_zone = -1` (no reservation — it floats over windows). Or use all four anchors with a centered inner container.
- Notification popup: `Anchor::TOP | RIGHT`, fixed size, `exclusive_zone = -1`.

**Keyboard interactivity:**
- `None` — dock (no keyboard input needed)
- `OnDemand` — launcher (receives keyboard when focused, compositor can unfocus)
- `Exclusive` — screen lock (grabs all keyboard input)

### Popup (menus, tooltips)

Child surfaces attached to a parent toplevel or layer surface. Positioned relative to the parent, dismissed when clicking outside.

```rust
enum SurfaceRole {
    Toplevel { .. },
    Layer { .. },
    Popup {
        parent: SurfaceId,       // which surface this popup belongs to
        position: (i32, i32),    // offset from parent
        size: (u32, u32),
    },
}
```

Created via SCTK's xdg-popup protocol. The compositor positions the popup relative to the parent and auto-dismisses on outside interaction (via the `grab` mechanism).

Use for: right-click context menus, dock icon tooltips, dropdown pickers.

## Event Loop

The framework uses `calloop` (SCTK's event loop) as the single event loop for both Wayland protocol events and application timers.

```rust
fn run(mut state: AppState) {
    let mut event_loop = calloop::EventLoop::try_new().unwrap();

    // SCTK's WaylandSource drives the Wayland connection
    let wayland_source = WaylandSource::new(state.connection.clone());
    event_loop.handle().insert_source(wayland_source, |_, queue, state| {
        queue.dispatch_pending(state).unwrap();
    }).unwrap();

    // Main loop — blocks until events arrive
    loop {
        event_loop.dispatch(None, &mut state).unwrap();

        // After processing events, check if any surface needs a redraw
        for surface in &mut state.surfaces {
            if surface.needs_redraw() {
                surface.render();
                surface.commit();
            }
        }
    }
}
```

### Frame Callback Integration

The event loop integrates with Wayland's frame callback mechanism (from [backend.md](backend.md)):

1. Surface state changes (reactive update, animation tick) → mark surface dirty
2. If dirty, request a `frame` callback via `wl_surface.frame()`
3. Compositor fires the callback at vsync → `calloop` dispatches it
4. Framework runs: layout → display list → Skia render → `wl_surface.commit()`
5. If still animating, request another frame callback

When no surface is dirty, the event loop blocks on `dispatch(None, ..)` — zero CPU usage until the next input event or timer.

### Timers

`calloop` supports timer sources for deferred work:

```rust
use calloop::timer::Timer;

// Auto-hide launcher after 200ms of inactivity
let timer = Timer::from_duration(Duration::from_millis(200));
event_loop.handle().insert_source(timer, |_, _, state| {
    state.hide_launcher();
    calloop::timer::TimeoutAction::Drop
}).unwrap();
```

## Application Entry Point

An application declares its surface role and view, then hands off to the framework:

```rust
fn main() {
    ui::run(AppConfig {
        surface: SurfaceRole::Layer {
            namespace: "mars-dock".into(),
            layer: Layer::Top,
            anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            size: (0, 68),
            exclusive_zone: 68,
            keyboard: KeyboardInteractivity::None,
        },
        view: DockView::new(),
    });
}
```

```rust
fn main() {
    ui::run(AppConfig {
        surface: SurfaceRole::Toplevel {
            title: "Settings".into(),
            app_id: "mars-settings".into(),
        },
        view: SettingsView::new(),
    });
}
```

The `run()` function:
1. Connects to Wayland, binds globals
2. Creates the surface with the requested role
3. Sets up the GPU context (Vulkan or SHM fallback) on the surface
4. Mounts the view, runs initial render
5. Enters the event loop

## Multi-Surface

Each surface is an independent `ui::run()` invocation — a separate process. The dock and launcher are separate binaries, each with their own Wayland connection and event loop.

This is the simplest correct approach:
- No shared mutable state between surfaces
- A crash in the launcher doesn't take down the dock
- Each process gets its own Vulkan context (GPU memory is isolated)
- IPC between them (if needed) uses standard mechanisms — D-Bus, Unix sockets, or Wayland protocols

If a single application needs multiple surfaces (e.g., a main window with a detached toolbar), it can create multiple surfaces on the same Wayland connection. But for MarsOS shell components, separate processes is the right default.

## Output Handling

SCTK's `OutputState` tracks connected monitors. The framework uses this for:

- **DPI scaling:** Each output has a `scale_factor`. The framework multiplies layout dimensions and creates appropriately-sized Skia surfaces. Wayland's `wl_surface.set_buffer_scale()` or `wp_fractional_scale_v1` handles fractional scaling.
- **Output selection:** Layer surfaces can target a specific output or let the compositor choose (`None`). For a multi-monitor dock, spawn one dock process per output.
- **Hot-plug:** When outputs are added/removed, SCTK fires events. The framework resizes or repositions surfaces accordingly.

```rust
// Fractional scaling support
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::*;

// Integer scale from wl_output
let scale = output_state.info(&output).unwrap().scale_factor;

// For fractional: bind wp_fractional_scale_manager_v1, attach to surface
// Compositor sends preferred_scale events (e.g., 1.25, 1.5)
```

The Skia surface is created at `physical_size = logical_size * scale_factor`. All layout math operates in logical pixels; the scale transform is applied once when creating the `SkSurface`.
