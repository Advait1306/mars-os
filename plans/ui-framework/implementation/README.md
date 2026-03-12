# UI Framework Implementation Plan

Step-by-step phases for building the `ui` crate and migrating the dock.

## Phases

| Phase | Name | What it delivers |
|-------|------|-----------------|
| [1](phase-1-core-and-layout.md) | Core Element Tree & Layout | `ui` crate, builder API (`container`, `row`, `text`, etc.), Taffy flexbox layout |
| [2](phase-2-display-list-and-renderer.md) | Display List & Skia Renderer | `DrawCommand` abstraction, `SkiaRenderer`, text measurement, image loading |
| [3](phase-3-wayland-bootstrap.md) | Wayland Bootstrap & Event Loop | SCTK connection, surface creation (layer shell + toplevel), `ui::run()`, SHM presentation |
| [4](phase-4-reactive-state.md) | Reactive State & View Lifecycle | `Reactive<T>`, dependency tracking, `Handle`, `RenderContext`, `View` trait, `cx.embed()` |
| [5](phase-5-events.md) | Event & Input System | Hit testing, pointer events (click/hover/drag), keyboard/focus, cursor styles |
| [6](phase-6-animations.md) | Animation System | Springs, timed animations, `.animate()`, `.animate_layout()`, `.initial()`/`.exit()`, element diffing |
| [7](phase-7-scroll-and-text-input.md) | Scroll & Text Input | Scroll containers with physics, `text_input()`, clipboard |
| [8](phase-8-dock-migration.md) | Dock Migration | Rewrite dock as `DockView`, delete raw rendering code |

## Dependency graph

```
Phase 1 ─→ Phase 2 ─→ Phase 3 ─→ Phase 4 ─→ Phase 5 ─→ Phase 6
                                     │                      │
                                     └──────→ Phase 7 ←─────┘
                                                 │
                                              Phase 8
```

Phases are sequential — each builds on the previous. Phase 7 depends on both Phase 4 (reactive state for scroll/text state) and Phase 6 (spring animations for scroll physics).

## Key decisions

- **skia-safe replaces tiny-skia** — GPU acceleration, proper text shaping, blur/backdrop effects
- **Taffy for layout** — production-grade flexbox, used by Zed/Dioxus/Servo
- **Display list as boundary** — framework never calls Skia directly, enabling future backend swaps
- **SHM first, Vulkan later** — CPU rendering works immediately, GPU is an optimization
- **WindowTracker stays unchanged** — it's domain logic, not UI framework concern
