# Phase 6: Animation System

## Goal

Add spring and timed animations: property animation scoped by reactive deps, layout animation for position/size changes, and enter/exit animations for keyed elements. By the end, the framework handles all the animation complexity that the dock currently manages manually.

## Steps

### 6.1 Spring physics

`src/spring.rs` — port and enhance the dock's existing `Spring`.

```rust
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
}

pub struct SpringState {
    pub value: f32,
    pub target: f32,
    pub velocity: f32,
}
```

The existing dock Spring (stiffness=680, damping=52) becomes `Animation::default()`. Add the preset configs:

```rust
impl Animation {
    pub fn default() -> Self { Spring(SpringConfig { stiffness: 680.0, damping: 52.0 }) }
    pub fn snappy() -> Self  { Spring(SpringConfig { stiffness: 1200.0, damping: 70.0 }) }
    pub fn smooth() -> Self  { Spring(SpringConfig { stiffness: 300.0, damping: 35.0 }) }
    pub fn bouncy() -> Self  { Spring(SpringConfig { stiffness: 600.0, damping: 25.0 }) }
}
```

### 6.2 Animation config

`src/animation.rs`:

```rust
pub enum Animation {
    Spring(SpringConfig),
    Timed { duration_ms: u32, easing: Easing },
}

pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}
```

Delay support:

```rust
impl Animation {
    pub fn delay_ms(mut self, ms: u32) -> Self { ... }
}
```

### 6.3 Animatable properties

Define which element properties can be animated:

- `opacity` (f32)
- `background` (Color — interpolate each channel)
- `corner_radius` (f32)
- `offset_x`, `offset_y` (f32 — visual offset, doesn't affect layout)
- `scale` (f32 — visual scale around center)
- Position and size (for layout animations)

Each animated property gets its own `SpringState` (for spring animations) or progress tracker (for timed animations).

### 6.4 Element diffing with `.key()`

`src/diff.rs` — compare old and new element trees to detect enters, exits, and property changes.

```rust
pub struct DiffResult {
    pub entered: Vec<(String, Element)>,    // key → new element
    pub exited: Vec<(String, Element)>,     // key → old element
    pub updated: Vec<(String, PropChanges)>, // key → changed props
    pub reordered: Vec<String>,             // keys that moved position
}
```

Matching algorithm:
1. Build a map of `key → element` for old and new trees
2. Keys in new but not old → entered
3. Keys in old but not new → exited
4. Keys in both → compare props for changes
5. Unkeyed elements match by position index

### 6.5 Property animation: `.animate()`

Builder method on Element:

```rust
impl Element {
    pub fn animate(mut self, animation: Animation) -> Self { ... }
}
```

At render time, when the framework detects a prop change on an element:
1. Look up which `Reactive` fields were read to produce that prop value (via dependency tracking from Phase 4)
2. Start an animation from the old value to the new value
3. Each frame, step the animation and produce intermediate values
4. Apply intermediate values during the display list build (override the element's declared values)

The animation state lives in a per-element animation map maintained by the framework, keyed by element identity (tree position + `.key()`).

### 6.6 Layout animation: `.animate_layout()`

```rust
impl Element {
    pub fn animate_layout(mut self) -> Self { ... }
    pub fn animate_layout_with(mut self, animation: Animation) -> Self { ... }
}
```

When layout produces new bounds for a keyed element:
1. Compare old bounds vs new bounds
2. If position or size changed, spring-animate from old to new
3. During animation, the element renders at its interpolated position/size

This replaces the dock's manual `x_spring` management.

### 6.7 Enter/exit animations: `.initial()` / `.exit()`

```rust
pub struct From { /* opacity, offset_x, offset_y, scale, etc. */ }
pub struct To { /* same fields */ }

impl Element {
    pub fn initial(mut self, from: From) -> Self { ... }
    pub fn exit(mut self, to: To) -> Self { ... }
}
```

**Enter:** When a keyed element appears in the tree (diff detects it as new):
1. Start with the `From` values
2. Animate toward the element's declared values
3. Uses `Animation::default()` unless `From` specifies `.animation()`

**Exit:** When a keyed element disappears:
1. The framework keeps a snapshot of the element (frozen — no re-renders)
2. Animate from current values toward the `To` values
3. Remove the snapshot when the animation settles

This replaces the dock's `AnimSlot` entering/leaving state machine.

### 6.8 Animation frame scheduling

When any animation is in flight:
1. Request a Wayland `frame` callback every frame
2. On each frame: step all active animations (`dt` from timestamp delta)
3. Re-render with interpolated values
4. When all animations settle, stop requesting frames → zero CPU

### 6.9 Interruptibility

Spring animations are interruptible: if the target changes mid-flight, the spring continues from its current value and velocity toward the new target. No restart, no discontinuity.

Timed animations snap to the new target and restart.

### 6.10 Validation

Test the dock's animation pattern using the framework:

```rust
struct AnimTestView {
    items: Reactive<Vec<String>>,
}

impl View for AnimTestView {
    fn render(&self, cx: &mut RenderContext) -> Element {
        row().gap(8.0).children(
            self.items.get(cx).iter().map(|id| {
                container()
                    .key(id)
                    .size(48.0, 48.0)
                    .background(rgba(60, 60, 60, 255))
                    .rounded(8.0)
                    .animate_layout()
                    .initial(From::offset_y(20.0).opacity(0.0))
                    .exit(To::offset_y(-20.0).opacity(0.0))
            })
        )
    }
}
```

Add/remove items with a timer. Verify smooth enter/exit/reorder animations — the same choreography the dock does today, but declarative.

## Output

The animation system handles springs, timed animations, layout transitions, and enter/exit sequences. Manual spring management (`AnimSlot`, `width_spring`, phase tracking) is no longer needed — the framework handles it all from declarative `.animate()`, `.initial()`, `.exit()`.
