# Phase 8: Dock Migration

## Goal

Rewrite the dock to use the `ui` framework, replacing all raw rendering, manual spring management, and Wayland boilerplate. The dock becomes a thin view on top of the framework.

## What changes

| Current dock code                                                               | Replaced by                                                                 |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `render.rs` — tiny-skia drawing, rounded rects, circles, clip masks             | Framework's display list + SkiaRenderer                                     |
| `animation.rs` — manual `Spring` struct                                         | Framework's animation system (`.animate_layout()`, `.initial()`, `.exit()`) |
| `dock.rs` — `AnimSlot`, `width_spring`, phase tracking, SHM pool, SCTK handlers | `ui::run()` + `DockView` implementing `View`                                |
| `main.rs` — `Dock::run()`                                                       | `ui::run(DockView::new(), SurfaceConfig::LayerShell { ... })`               |

## What stays

| File                                                                       | Reason                                                               |
| -------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `windows.rs` — `WindowTracker`, `DockApp`, plasma window protocol dispatch | Domain logic, not rendering. Stays as-is, imported by the dock view. |
| Icon loading logic                                                         | Moves to a utility in the ui crate or stays as dock-specific code    |

## Steps

### 8.1 Update dock dependencies

```bash
cd dock
cargo add ui --path ../ui
cargo remove tiny-skia image resvg
```

Keep: `wayland-protocols-plasma` (still needed for `WindowTracker`).

### 8.2 DockView struct

```rust
use ui::*;

struct DockView {
    apps: Reactive<Vec<DockApp>>,
    hovered_id: Reactive<Option<String>>,
    window_tracker: WindowTracker,
}

impl DockView {
    fn new() -> Self {
        Self {
            apps: Reactive::new(vec![]),
            hovered_id: Reactive::new(None),
            window_tracker: WindowTracker::new(),
        }
    }
}
```

### 8.3 DockView::render()

```rust
impl View for DockView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        let apps = self.apps.get(cx);

        // Outer container — the dock bar
        container()
            .background(rgba(30, 30, 30, 190))
            .rounded(18.0)
            .border(rgba(255, 255, 255, 30), 1.0)
            .padding_xy(12.0, 0.0)
            .clip()
            .child(
                row().gap(8.0).align_items(Alignment::Center).children(
                    apps.iter().map(|app| {
                        dock_icon(app, self.hovered_id.get(cx), &handle)
                    })
                )
            )
    }
}
```

### 8.4 dock_icon component function

```rust
fn dock_icon(app: &DockApp, hovered_id: &Option<String>, handle: &Handle<DockView>) -> Element {
    let is_hovered = hovered_id.as_deref() == Some(&app.app_id);
    let id = app.app_id.clone();

    column().gap(2.0).align_items(Alignment::Center)
        .key(&app.app_id)
        .child(
            container()
                .size(44.0, 44.0)
                .opacity(if is_hovered { 1.0 } else { 0.8 })
                .animate(Animation::smooth())
                .child(image_file(&icon_path(&app.icon_name)).size(44.0, 44.0))
        )
        .child(
            // Active indicator dot
            if app.is_active {
                container()
                    .size(5.0, 5.0)
                    .rounded(2.5)
                    .background(rgba(255, 255, 255, 220))
            } else {
                spacer().height(5.0)
            }
        )
        .animate_layout()
        .initial(From::offset_y(20.0).opacity(0.0))
        .exit(To::offset_y(-20.0).opacity(0.0))
        .on_hover({
            let handle = handle.clone();
            move |hovered| handle.update(|s| {
                s.hovered_id.set(if hovered { Some(id.clone()) } else { None });
            })
        })
        .cursor(CursorStyle::Pointer)
}
```

This replaces all of:

- `AnimSlot` (entering/leaving state, x_spring, y_spring)
- `render_dock()` (manual pixel drawing)
- `recompute_x_targets()` (manual position calculation)
- `tick_animations()` (manual spring stepping, phase transitions)

### 8.5 Window tracker integration

The `WindowTracker` needs to communicate with the view. Options:

**Option A: Wayland protocol extension** — Register the plasma window management globals through the ui framework's Wayland connection. The framework provides a hook for custom protocol dispatch:

```rust
ui::run_with(DockView::new(), config, |globals, qh| {
    // Bind plasma window management
    let plasma_wm: OrgKdePlasmaWindowManagement = globals.bind(qh, 1..=16, ()).unwrap();
});
```

The `WindowTracker` dispatch handlers update the tracker, then call `handle.update()` to push new apps into the reactive state.

**Option B: Background thread** — Run the window tracker on a separate thread with its own Wayland connection, send updates to the view via a channel that the calloop event loop polls.

Option A is simpler and matches the current architecture.

### 8.6 Entry point

```rust
fn main() {
    env_logger::init();

    ui::run(DockView::new(), SurfaceConfig::LayerShell {
        namespace: "dock".into(),
        layer: Layer::Top,
        anchor: Anchor::BOTTOM,
        size: (0, 64),  // width=0 means auto-size based on content
        exclusive_zone: 0,
        keyboard: KeyboardInteractivity::None,
    });
}
```

### 8.7 Dock width auto-sizing

The current dock manually calculates width and resizes the layer surface. With the framework, the dock's element tree declares its content — the framework computes layout and tells the compositor the surface size.

Two approaches:

1. **Framework auto-sizes:** after layout, if the root element's natural size differs from the surface size, resize the layer surface. The framework handles the `set_size()` + `commit()`.
2. **Fixed surface, centered content:** use a large fixed surface and center the dock visually (current approach). Simpler but wastes compositor resources.

Approach 1 is cleaner. The framework's layer shell integration resizes the surface to fit the root element's computed width (plus margins).

### 8.8 Delete replaced files

Remove from `dock/src/`:

- `render.rs` — all rendering is now the framework's job
- `animation.rs` — spring physics live in the ui crate

Keep:

- `windows.rs` — unchanged
- `main.rs` — simplified entry point
- `dock.rs` → becomes `view.rs` (just the `DockView` impl)

### 8.9 Validation

1. Transfer to VM, build, run
2. Verify: dock appears at bottom of screen with rounded background
3. Open/close windows → icons appear/disappear with enter/exit animations
4. Hover icons → opacity change with smooth spring animation
5. Active window indicator dot shows/hides correctly
6. Multiple rapid window opens/closes → animations interrupt cleanly, no glitches
7. Compare visually with the current dock — should be identical or better

## Output

The dock is ~80 lines of view code instead of ~600 lines of manual rendering, animation, and Wayland plumbing. All complexity lives in the `ui` framework, reusable by the launcher and every other MarsOS application.
