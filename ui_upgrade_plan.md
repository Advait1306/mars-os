# UI Upgrade state

You have to implement plans in ./plans/ui-upgrade. Some plans are already complete check ui_upgrade_plan.md for status. You must implement every plan and make a commit mention which plan was completed.

NOTE: Plan 1 & 2 are already complete before starting this loop.

## Current State
### Plan 3-events: DONE (remaining tasks need VM or rendering pipeline)
Phases 1-6, 8, 10, 11, and 12 of the event system plan (plans/ui-upgrades/3-events.md) are DONE.
The remaining deferred tasks are:
- **Phase 7 Task 6**: Render preedit text (needs rendering pipeline changes)
- **Phase 9 Tasks 6-8**: Drag ghost rendering + external DnD via wl_data_device (needs VM)
- **Phase 10 Tasks 5-7**: Wayland clipboard integration via wl_data_device (needs VM)

All event types, handler slots, dispatch logic, and Wayland protocol handlers are implemented.

### Plan 4-visual-properties: IN PROGRESS
Phase 1 (core visual upgrades) is DONE:
- Per-corner border radius (`CornerRadii` type replacing `corner_radius: f32`)
- Gradient backgrounds (linear, radial, conic via `Gradient` enum + `DrawCommand::GradientRect`)
- Inset box shadows (`DrawCommand::InsetBoxShadow` with Chromium-style EvenOdd path clipping)
- Multiple box shadows (`Vec<BoxShadow>` on Element, both outset and inset)

Phase 2 (borders and outlines) is DONE:
- BorderStyle enum (Solid, Dashed, Dotted, Double, None)
- Per-side borders via FullBorder (BorderSide per side with clip-based drrect rendering)
- Outline support (offset stroke outside element, does not affect layout)
- Border style rendering: dashed via PathEffect::dash, dotted via round caps, double via two strokes

Phase 3 (CSS filters) is DONE:
- Filter enum with all CSS filter functions (blur, brightness, contrast, grayscale, sepia, hue-rotate, invert, opacity, saturate, drop-shadow)
- PushFilter/PopFilter DrawCommands wrapping element content with chained ImageFilters
- ApplyBackdropFilter for backdrop-filter support (blur and all color matrix filters)
- Builder methods: filter_blur(), filter_brightness(), filter_contrast(), etc.
- Chained filter composition via input parameter (each filter feeds into next)

The next phase to work on is **Phase 4** (transforms).

## Completed Phases Summary
- Phase 1: Three-phase event propagation (capture/target/bubble) — DONE
- Phase 2: Focus management and tab navigation — DONE
- Phase 3: Pointer capture — DONE
- Phase 4: Click synthesis (double-click, context menu) — DONE
- Phase 5: Scroll events (WheelEvent, ScrollEnd, source discrimination) — DONE
- Phase 6: Keyboard events (KeyDown/KeyUp, key repeat, modifier tracking) — DONE
- Phase 7: Text Input and IME (Tasks 1-5 done, Task 6 deferred) — DONE
- Phase 8: Drag support (legacy) — DONE
- Phase 9: Drag and Drop (Tasks 1-5 done, Tasks 6-9 deferred) — DONE
- Phase 10: Clipboard (event types, handlers, Ctrl+C/X/V dispatch, default paste for text inputs) — DONE
- Phase 11: Touch Events (TouchEvent type, touch-to-pointer coercion, native dispatch, wl_touch handler) — DONE
- Phase 12: Hit Testing Enhancements — DONE
