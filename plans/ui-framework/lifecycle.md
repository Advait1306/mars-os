# View Lifecycle & Composition

## Entry Point

```rust
fn main() {
    ui::run(DockView::new(), SurfaceConfig::layer_shell(/* ... */));
}
```

`ui::run()` takes any `impl View + 'static` and a surface configuration. It creates the Wayland connection, surface, and event loop, calls the initial `render()`, and enters the frame loop.

```rust
enum SurfaceConfig {
    /// Dock, launcher, notification overlay — anchored to screen edges
    LayerShell { anchor: Anchor, size: (u32, u32), exclusive_zone: i32 },
    /// Regular application window
    Toplevel { title: String, min_size: Option<(u32, u32)> },
}
```

## The View Trait

```rust
trait View: 'static {
    fn render(&self, cx: &mut RenderContext) -> Element;
}
```

No `init()`, `update()`, `did_mount()`, or `will_unmount()`.

- **Construction** — plain `fn new() -> Self`. You build the struct, the framework doesn't.
- **Teardown** — Rust's `Drop` trait. Implement it if you need cleanup.
- **State** — `Reactive<T>` fields (see [animations.md](animations.md)).
- **Side effects** — `cx.spawn()` for async, `cx.interval()` for timers.

## Composition

Two patterns: **component functions** (stateless) and **child views** (stateful).

### Component Functions

The default way to decompose UI. A plain function that takes data and returns an Element.

```rust
fn dock_icon(app: &App, is_hovered: bool, handle: &Handle<DockView>) -> Element {
    let h = handle.clone();
    let id = app.id.clone();
    container()
        .key(&app.id)
        .size(48.0, 48.0)
        .opacity(if is_hovered { 1.0 } else { 0.7 })
        .animate(Animation::smooth())
        .child(image(&app.icon))
        .on_click(move || h.update(|s| s.launch(&id)))
}

// Used in DockView::render():
impl View for DockView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        row().gap(4.0).children(
            self.apps.get(cx).iter().map(|app| {
                let is_hovered = self.hovered_id.get(cx) == Some(app.id.clone());
                dock_icon(app, is_hovered, &handle)
            })
        )
    }
}
```

No trait, no lifecycle, no framework involvement. Use these for anything that doesn't need its own encapsulated mutable state.

### Child Views

When a subsection of UI has its **own independent state** (e.g., a text input managing cursor position, a collapsible sidebar tracking open/closed), extract it into a child `View`.

The parent stores child views as fields and embeds them with `cx.embed()`:

```rust
struct AppWindow {
    sidebar: SidebarView,
    content: ContentView,
}

impl View for AppWindow {
    fn render(&self, cx: &mut RenderContext) -> Element {
        row()
            .child(cx.embed(&self.sidebar))
            .child(cx.embed(&self.content))
    }
}
```

`cx.embed(&view)` does three things:

1. Creates a **reactive scope boundary** — the child re-renders independently when its own `Reactive` state changes, without re-rendering the parent.
2. Calls `view.render()` with a scoped `RenderContext`.
3. Returns the resulting `Element` to slot into the parent's tree.

### Dynamic Child Views

For collections where children come and go, the parent manages a `Vec` or `HashMap`:

```rust
struct TabContainer {
    tabs: Vec<TabView>,
    active: Reactive<usize>,
}

impl View for TabContainer {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let i = *self.active.get(cx);
        column()
            .child(tab_bar(&self.tabs, i, &cx.handle()))
            .child(cx.embed(&self.tabs[i]))
    }
}
```

Adding/removing from `self.tabs` is normal Rust mutation — push, remove, swap, etc. When a child view is removed from the collection, it is dropped.

### When to Use Which

| Pattern            | State                            | Example                                              |
| ------------------ | -------------------------------- | ---------------------------------------------------- |
| Component function | Stateless — parent owns all data | `dock_icon()`, `app_row()`, `divider_with_label()`   |
| Child view         | Has its own `Reactive<T>` fields | `TextInputView`, `SidebarView`, `ScrollableListView` |

Rule of thumb: start with component functions. Extract a child view only when you find yourself wanting `Reactive` state that the parent shouldn't know about.

## Teardown

### Drop

Views are Rust structs. When the parent removes them or the application exits, `Drop` runs:

```rust
impl Drop for AudioPlayerView {
    fn drop(&mut self) {
        self.stream.stop();
    }
}
```

Most views won't need a `Drop` impl — `Reactive<T>` fields and spawned tasks are cleaned up automatically (see below).

### Reactive State Cleanup

When a view is dropped, its `Reactive<T>` fields drop with it. The framework removes their dependency tracking entries. No manual unsubscribe.

### Async Task Cancellation

Async work is tied to a view's lifetime via `cx.spawn()`:

```rust
fn send_message(&self, cx: &mut RenderContext) {
    let handle = cx.handle();
    cx.spawn(async move {
        let response = api::send(msg).await;
        handle.update(|s| s.messages.push(response));
    });
}
```

When the view is dropped, pending `cx.spawn()` futures are cancelled. Stale `handle.update()` calls on a dropped view are no-ops.

### Exit Animations

When a keyed element disappears from the render output:

1. The framework snapshots the element's last rendered state.
2. The `.exit()` animation runs on the snapshot.
3. The snapshot is removed from the display list when the animation settles.

The parent view is still alive — only the _element_ is exiting. The snapshot is frozen; it doesn't re-render.

If the parent view itself is being dropped (e.g., tab closed), exit animations on its elements still complete — the framework holds the snapshot until the animation finishes, even though the view is gone.

## Frame Lifecycle

Each frame follows this sequence:

```
Event → Handler → Dirty check → Re-render → Diff → Layout → Display list → Paint
```

1. **Event arrives** — pointer move, key press, timer tick, async completion.
2. **Handler runs** — mutates `Reactive<T>` via `handle.update(|s| ...)`.
3. **Dirty check** — framework identifies which views have dirty reactive state.
4. **Re-render** — calls `render()` only on dirty views, not the entire tree.
5. **Diff** — compares new element tree against previous for each re-rendered view. Matches elements by `.key()` where present, by position otherwise. Detects enters, exits, and property changes.
6. **Layout** — runs Taffy on changed subtrees.
7. **Display list** — emits `DrawCommand`s for damaged regions.
8. **Paint** — Skia executes commands, compositor presents.

Multiple dirty views are batched into a single frame. Steps 5–8 run once per frame, not once per view.
