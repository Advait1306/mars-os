# Plan: Multi-Surface / Popup Support for UI Framework

## Problem

The header uses 3 separate Wayland layer-shell surfaces: main bar, volume popup, menu popup. The UI framework (`ui/src/wayland.rs`) manages only a single `LayerSurface`. Popups need their own surfaces for:
- Independent positioning/anchoring (e.g. volume popup anchored TOP|RIGHT with margin)
- Exclusive keyboard interactivity (popups grab keyboard to detect Escape/focus-loss)
- Overlay layer (popups sit above the Top layer bar)

## Design

Add a popup API that lets a `View` open/close additional layer-shell surfaces, each running their own mini render pipeline within the same event loop. Popups are identified by a string key and configured via a `PopupConfig`.

### API Surface

In `ui/src/app.rs`, add:

```rust
pub struct PopupConfig {
    pub anchor: Anchor,
    pub size: (u32, u32),
    pub margin: (i32, i32, i32, i32),
    pub keyboard: KeyboardInteractivity,
    /// Called when this popup's surface loses keyboard focus (e.g. user clicked outside).
    /// Typically used to close the popup.
    pub on_focus_lost: Option<Box<dyn Fn()>>,
}
```

On `RenderContext`, add methods:

```rust
impl RenderContext {
    /// Open a popup surface. If already open with this key, no-op.
    pub fn open_popup(&mut self, key: &str, config: PopupConfig);

    /// Close a popup surface by key. No-op if not open.
    pub fn close_popup(&mut self, key: &str);

    /// Returns true if a popup with this key is currently open and configured.
    pub fn is_popup_open(&self, key: &str) -> bool;

    /// Provide the element tree for an open popup. Called during render().
    /// If the popup isn't open, this is a no-op.
    pub fn render_popup(&mut self, key: &str, element: Element);
}
```

Usage in a View:

```rust
fn render(&self, cx: &mut RenderContext) -> Element {
    // Main header bar content
    let header = row()
        .child(mars_icon.on_click({
            let handle = cx.handle::<Self>();
            move || handle.update(|v| v.toggle_menu())
        }))
        .child(spacer())
        .child(volume_row.on_click({
            let handle = cx.handle::<Self>();
            move || handle.update(|v| v.toggle_volume_popup())
        }))
        .child(time_text);

    // Render popup content if open
    if self.volume_popup_open {
        cx.open_popup("volume", PopupConfig {
            anchor: Anchor::TOP | Anchor::RIGHT,
            size: (240, 48),
            margin: (24, right_margin, 0, 0),
            keyboard: KeyboardInteractivity::Exclusive,
            on_focus_lost: Some(Box::new({
                let handle = cx.handle::<Self>();
                move || handle.update(|v| v.volume_popup_open = false)
            })),
        });
        cx.render_popup("volume", volume_popup_element());
    } else {
        cx.close_popup("volume");
    }

    header
}
```

### Internal Implementation

#### 1. RenderContext changes (`ui/src/reactive.rs`)

Add fields to `RenderContext`:

```rust
pub struct RenderContext {
    // ... existing fields ...
    popup_opens: Vec<(String, PopupConfig)>,
    popup_closes: Vec<String>,
    popup_elements: Vec<(String, Element)>,
    open_popups: HashSet<String>,  // snapshot of currently open popup keys
}
```

The `open_popup`, `close_popup`, `render_popup` methods push onto these vecs. After `render()` returns, `WaylandState::draw()` reads them.

#### 2. PopupState struct (`ui/src/wayland.rs`)

```rust
struct PopupState {
    surface: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,
    // Per-popup render state
    last_layout: Option<LayoutNode>,
    last_element_tree: Option<Element>,
    animator: Animator,
    event_state: EventState,
    // Focus-loss callback (fired by KeyboardHandler::leave when this popup's surface loses focus)
    on_focus_lost: Option<Box<dyn Fn()>>,
}
```

`WaylandState` gains:

```rust
struct WaylandState {
    // ... existing fields ...
    popups: HashMap<String, PopupState>,
}
```

#### 3. Draw loop changes (`ui/src/wayland.rs`)

In `WaylandState::draw()`, after rendering the main surface:

1. Process `popup_opens` — create new `LayerSurface` via `self.layer_shell.create_layer_surface()` with `Layer::Overlay`, configure anchor/size/margin/keyboard, insert into `self.popups`.
2. Process `popup_closes` — drop the `PopupState` (dropping `LayerSurface` destroys the Wayland surface).
3. Process `popup_elements` — for each `(key, element)`, run the per-popup render pipeline:
   - `compute_layout(&element, popup.width, popup.height)`
   - `collect_keyed_elements` + `popup.animator.diff_and_update`
   - `build_display_list` with the popup's animator
   - Render to Skia surface → copy to popup's SHM buffer → commit

#### 4. Pointer dispatch for popups

In `PointerHandler::pointer_frame`, the events include the `wl_surface` they occurred on. Currently the framework ignores this since there's only one surface.

Change: match `event.surface` against `self.layer_surface` and each `self.popups[key].surface.wl_surface()`. Dispatch `InputEvent`s to the matching surface's `EventState` and element tree.

This requires storing the `wl_surface` reference or id in each `PopupState` for comparison. The `LayerSurface` already provides `.wl_surface()`.

#### 5. Keyboard focus-loss handling

Requires the keyboard plan (`ui-keyboard-events.md`) to be implemented first so `KeyboardHandler` exists.

In `KeyboardHandler::leave`, match the departing surface against `self.popups`:

```rust
fn leave(&mut self, ..., surface: &wl_surface::WlSurface, ...) {
    for (key, popup) in &self.popups {
        if surface == popup.surface.wl_surface() {
            // 1. Clear element focus within the popup
            if let Some(ref elements) = popup.last_element_tree {
                popup.event_state.clear_focus(elements);
            }
            // 2. Fire the popup's on_focus_lost callback
            if let Some(ref cb) = popup.on_focus_lost {
                cb();
                self.needs_redraw = true;
            }
            break;
        }
    }
}
```

This clears any focused element inside the popup (firing `on_focus(false)`), then fires the View's closure which typically sets a flag like `self.volume_popup_open = false`. On the next render, the View calls `cx.close_popup("volume")`, and the framework destroys the surface.

#### 6. LayerShellHandler changes

`configure` and `closed` callbacks receive a `&LayerSurface`. Match against popups:
- `configure`: update popup's width/height/configured, trigger redraw
- `closed`: remove popup from `self.popups`

#### 7. Passing open popup state to RenderContext

Before calling `render()`, populate `RenderContext.open_popups` with the keys from `self.popups` so `is_popup_open()` works.

### File Changes

| File | Changes |
|------|---------|
| `ui/src/app.rs` | Add `PopupConfig` struct |
| `ui/src/reactive.rs` | Add popup methods to `RenderContext`, add popup request fields |
| `ui/src/wayland.rs` | Add `PopupState`, `HashMap<String, PopupState>`, popup creation/destruction in draw loop, multi-surface pointer dispatch, multi-surface configure/closed handling |

### Edge Cases

- **Popup requests during same frame**: `open_popup` + `render_popup` in a single `render()` call. The surface won't be configured until the compositor responds, so the popup element is stored and rendered on the next frame once `configured = true`.
- **Closing a popup that has keyboard grab**: dropping the surface returns keyboard focus to the compositor. The header relies on this for "click outside to close" — keyboard leave fires when focus moves away.
- **Multiple popups simultaneously**: supported by the HashMap, but the header only ever has one popup open at a time (it closes volume before opening menu and vice versa). The View manages this logic.
