# Phase 4: Reactive State & View Lifecycle

## Goal

Add `Reactive<T>` for automatic state tracking and re-rendering. Introduce `RenderContext` and `Handle` so views can mutate state from event handlers and trigger targeted re-renders. By the end, a view can update state and see the UI re-render automatically.

## Steps

### 4.1 Reactive<T>

`src/reactive.rs` — the core reactive primitive.

```rust
pub struct Reactive<T> {
    value: T,
    id: ReactiveId,  // unique identifier for dependency tracking
}
```

- `get(&self, cx: &RenderContext) -> &T` — reads the value and registers a dependency (this reactive field was read during this render pass, by the current element's style props)
- `set(&mut self, value: T)` — sets the value, marks it dirty
- `get_untracked(&self) -> &T` — reads without tracking (for use outside render)

`ReactiveId` is a lightweight handle (u64 counter) used by the dependency tracker.

### 4.2 Dependency tracker

`src/reactive.rs` — tracks which reactive fields were read during render.

```rust
struct DependencyTracker {
    /// For each reactive field, which views depend on it
    deps: HashMap<ReactiveId, HashSet<ViewId>>,
    /// Which reactive fields are dirty (set() was called since last render)
    dirty: HashSet<ReactiveId>,
}
```

When `Reactive::set()` is called:
1. Mark the ReactiveId as dirty
2. Look up which views depend on it
3. Mark those views for re-render

When `Reactive::get(cx)` is called during render:
1. Record that the current view depends on this ReactiveId

### 4.3 RenderContext

`src/context.rs` — passed to `View::render()`, provides access to framework services.

```rust
pub struct RenderContext<'a> {
    tracker: &'a mut DependencyTracker,
    view_id: ViewId,
    // future phases add: spawn, interval, embed
}

impl<'a> RenderContext<'a> {
    pub fn handle<V: View>(&self) -> Handle<V> { ... }
}
```

### 4.4 Handle<V>

`src/context.rs` — a clonable handle for mutating view state from closures.

```rust
pub struct Handle<V> {
    view_id: ViewId,
    updater: Sender<Box<dyn FnOnce(&mut V)>>,  // channel to send mutations
}

impl<V: View> Handle<V> {
    pub fn update(&self, f: impl FnOnce(&mut V) + 'static) { ... }
}
```

`update()` sends the mutation closure to the event loop. The event loop applies it to the view, which may call `Reactive::set()`, which triggers dirty tracking and schedules a re-render.

Implementation options for the channel:
- `calloop::channel` integrates directly with the event loop
- Or a simple `Rc<RefCell<VecDeque<...>>>` since everything is single-threaded

### 4.5 Updated View trait

```rust
pub trait View: 'static {
    fn render(&self, cx: &mut RenderContext) -> Element;
}
```

Now takes `cx` so that `Reactive::get(cx)` can register dependencies.

### 4.6 Frame loop integration

Update the event loop from Phase 3:

```
Event arrives (Wayland input, timer, Handle::update channel)
  → Apply pending mutations to view
  → Check dirty reactive fields
  → If any dirty: call render() on affected views
  → Diff new element tree vs previous (basic — just replace entire tree for now, element-level diffing comes in Phase 6 with animations)
  → Layout → display list → paint
  → Clear dirty flags
```

### 4.7 View composition: cx.embed()

`src/context.rs` — embed a child view with an independent reactive scope.

```rust
impl RenderContext<'_> {
    pub fn embed<V: View>(&mut self, view: &V) -> Element { ... }
}
```

Creates a scoped `RenderContext` for the child. The child's `Reactive` dependencies are tracked separately — when child state changes, only the child re-renders, not the parent.

### 4.8 Async: cx.spawn()

`src/context.rs` — spawn async work tied to the view's lifetime.

```rust
impl RenderContext<'_> {
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) { ... }
}
```

Uses `calloop`'s async support or a simple `block_on` executor. Spawned futures are cancelled when the view is dropped.

### 4.9 Validation

Test app with reactive state:

```rust
struct CounterView {
    count: Reactive<i32>,
}

impl View for CounterView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        column().gap(12.0).padding(20.0)
            .child(text(&format!("Count: {}", self.count.get(cx))).font_size(24.0).color(WHITE))
            .child(
                container()
                    .background(rgba(60, 60, 60, 255))
                    .rounded(8.0)
                    .padding(12.0)
                    .child(text("+1").color(WHITE))
                    // Click handling comes in Phase 5, but we can test
                    // re-rendering by mutating state via a timer:
            )
    }
}
```

Use a calloop timer to call `handle.update(|s| s.count.set(s.count.get_untracked() + 1))` every second. Verify the displayed count updates.

## Output

Views can hold `Reactive<T>` state, read it during render, mutate it from closures, and the framework automatically re-renders only the affected views.
