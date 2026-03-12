# Animations

## Reactive State

State fields use `Reactive<T>` wrappers, enabling automatic dependency tracking and re-render triggering.

```rust
struct MyView {
    hovered: Reactive<bool>,
    active: Reactive<bool>,
    count: Reactive<i32>,
}
```

- **During render:** `self.field.get(cx)` reads the value and registers a dependency between the current element's style props and this reactive field.
- **In event handlers:** `s.field.set(value)` sets the value, marks it dirty, and triggers a re-render.

This serves two purposes:
1. **Re-render triggering** — framework knows when to call `render()` again
2. **Animation scoping** — framework knows which props to animate when a specific reactive value changes (like SwiftUI's `value:` parameter, but automatic)

## Animation Config

Animations can be spring-based (physically modeled) or duration-based (timed with easing).

```rust
enum Animation {
    Spring(SpringConfig),
    Timed { duration_ms: u32, easing: Easing },
}

struct SpringConfig {
    stiffness: f32,
    damping: f32,
}

enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}
```

### Presets

```rust
Animation::default()                // Spring(680, 52) — critically damped, fast settle
Animation::snappy()                 // Stiffer spring, very fast
Animation::smooth()                 // Softer spring, more glide
Animation::bouncy()                 // Underdamped — playful overshoot
Animation::ease(duration_ms)        // Timed with EaseInOut
Animation::linear(duration_ms)      // Timed with Linear
```

### Custom

```rust
Animation::spring(400.0, 30.0)                         // Custom stiffness/damping
Animation::bezier(300, 0.34, 1.56, 0.64, 1.0)          // Custom cubic bezier curve
```

### Interruptibility

**Spring animations are interruptible.** When the target changes mid-animation, the spring continues from its current value and velocity toward the new target — no discontinuity, no restart. This is the primary reason springs are the default.

**Timed animations are not interruptible.** If a timed animation's target changes mid-flight, it snaps to the new target and restarts. Use springs for any property that can change during user interaction (hover, press, drag, etc.). Reserve timed animations for fire-and-forget sequences (loading spinners, one-shot transitions).

### Delay

All animation types support `.delay_ms()` — on `Animation`, `From`, and `To`:

```rust
Animation::smooth().delay_ms(80)
```

## Layout Animation

`.animate_layout()` spring-animates position and size changes when the element tree is re-rendered. Requires `.key()` for element identity across renders.

```rust
row().gap(8.0).children(
    self.items.get(cx).iter().map(|item| {
        container()
            .key(&item.id)
            .size(48.0, 48.0)
            .animate_layout()
            .child(image(&item.icon))
    })
)
```

Uses `Animation::default()` (critically damped spring). Override with:

```rust
.animate_layout_with(Animation::bouncy())
```

## Property Animation

`.animate()` animates style property changes between renders, automatically scoped via `Reactive<T>` dependency tracking.

```rust
container()
    .opacity(if self.hovered.get(cx) { 1.0 } else { 0.6 })
    .background(if self.active.get(cx) { BLUE } else { GRAY })
    .animate(Animation::smooth())
```

The framework tracks that `opacity` was read from `self.hovered` and `background` was read from `self.active` during render. When `hovered` changes, only opacity animates. When `active` changes, only background animates. No manual scoping needed.

## Enter/Exit Animations

Enter/exit animations use `From` and `To` builders to declare the start state (on enter) and end state (on exit) as explicit prop values. When a keyed element is removed from the tree, the framework keeps it alive and renders it during its exit animation, then cleans it up.

### `From` — enter animation

Declares where the element comes from when it first appears. It animates from these values toward its actual declared styles.

```rust
// Fade + slide up on enter
.initial(From::opacity(0.0).offset_y(20.0))

// Scale in from nothing
.initial(From::opacity(0.0).scale(0.8))
```

### `To` — exit animation

Declares where the element goes when removed. It animates from its current styles toward these values, then gets cleaned up.

```rust
// Fade + slide down on exit
.exit(To::opacity(0.0).offset_y(-20.0))

// Shrink out
.exit(To::opacity(0.0).scale(0.5))
```

### Available style props on `From` / `To`

Same style methods available on elements: `opacity()`, `offset_x()`, `offset_y()`, `scale()`, `background()`, `rounded()`, etc. Static builder methods are the starting point — no empty `From::new()`:

```rust
From::opacity(0.0)                          // start with one prop
From::opacity(0.0).offset_y(20.0).scale(0.9) // chain more
```

### Custom animation and delay

```rust
.initial(From::opacity(0.0).offset_y(20.0).animation(Animation::smooth()))
.exit(To::opacity(0.0).animation(Animation::ease(150)).delay_ms(80))
```

Without an explicit `.animation()`, `From` and `To` use `Animation::default()`.

## Full Example — Animated Dock

```rust
struct DockView {
    apps: Reactive<Vec<App>>,
    hovered_id: Reactive<Option<String>>,
}

impl View for DockView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        let handle = cx.handle();
        row().gap(4.0).children(
            self.apps.get(cx).iter().map(|app| {
                let is_hovered = self.hovered_id.get(cx) == Some(app.id.clone());
                container()
                    .key(&app.id)
                    .size(48.0, 48.0)
                    .opacity(if is_hovered { 1.0 } else { 0.7 })
                    .animate(Animation::smooth())
                    .animate_layout()
                    .initial(From::offset_y(20.0).opacity(0.0).delay_ms(80))
                    .exit(To::offset_y(-20.0).opacity(0.0))
                    .child(image(&app.icon))
                    .on_click(move || handle.update(|s| s.launch(&app.id)))
            })
        )
    }
}
```

Compared to the current dock implementation (manual spring management, two-phase state machine, `AnimSlot` bookkeeping), this achieves the same enter/exit choreography in a declarative style.
