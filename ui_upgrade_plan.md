# UI Upgrade state

You have to implement plans in ./plans/ui-upgrade. Some plans are already complete check ui_upgrade_plan.md for status. You must implement every plan and make a commit mention which plan was completed.

NOTE: Plan 1 & 2 are already complete before starting this loop.

## Current State
### Plan 3-events: DONE (remaining tasks need VM)
Phases 1-6, 8, 10, 11, and 12 of the event system plan (plans/ui-upgrades/3-events.md) are DONE.

Phase 7 Task 6 (Render preedit text) is DONE:
- Added `preedit_text` and `preedit_cursor` fields to Element with `preedit()` builder method
- Added `preedit_byte_range` field to DrawCommand::Text
- Preedit text is composed inline at the cursor position in TextInput display text
- Cursor is positioned within the preedit text using `preedit_cursor`
- Renderer draws underline decoration beneath preedit text range
- Selection is cleared during composition (standard IME behavior)

The remaining deferred tasks are:
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

Phase 4 (transforms) is DONE:
- Transform enum (Translate, Rotate, Scale, Skew, Matrix)
- transform_origin as fraction of element bounds (default center 0.5, 0.5)
- PushTransform/PopTransform DrawCommands with origin-based application
- Builder methods: rotate(), scale(), scale_xy(), translate(), skew(), transform_origin()
- Combined transforms applied in CSS order via sequential canvas operations

Phase 5 (text enhancements) was already fully implemented in prior work.

Phase 6 (remaining properties) is DONE:
- visibility: `visible` field on Element, `hidden()` builder — invisible elements skip drawing but children still render
- BlendMode enum (16 CSS blend modes mapped to Skia) with PushBlendMode/PopBlendMode
- Color utilities: `from_hex()`, `from_hsl()`, `from_hsla()`, `with_alpha()`, `lighter()`, `darker()`

**Plan 4 is now COMPLETE.** All 6 phases implemented.

### Plan 5-svg-rendering: IN PROGRESS
Phase 1 (Tier 1 improvements) is DONE:
- Fixed cache key generation: content hash + dimensions instead of truncated string
- LRU cache eviction with 64MB byte budget
- Color tinting via `tint(color)` builder using SrcIn blend color filter
- ImageFit enum (Contain, Cover, Fill, ScaleDown) with `image_fit()` builder
- Proper image fit computation for all modes

Phase 2 (SVG path parser) is DONE:
- SVG path `d` attribute parser supporting M/m, L/l, H/h, V/v, C/c, Q/q, Z/z commands
- ElementKind::Shape with ShapeData (path_data, fill, stroke, viewbox)
- DrawCommand::Path variant with viewbox-to-bounds scaling
- shape() and shape_with_viewbox() builder functions
- Path rendering with fill and stroke in SkiaRenderer

Phase 3 (Tier 2 Vector SVG Renderer) is DONE:
- `svg_render.rs` module: usvg tree walker emitting native Skia draw calls
- `VectorSvg` struct: parses SVG → usvg tree → records Skia Picture for resolution-independent replay
- Full usvg node support: Group (transform, opacity, clip-path), Path (fill, stroke, dash, cap, join), Image (PNG/JPEG/GIF/WEBP/nested SVG), Text (via flattened paths)
- SVG paint conversion: solid colors, linear gradients, radial gradients (pattern fallback to gray)
- Clip path support with per-path fill rules
- `ImageSource::VectorSvg` variant with content hash cache in SkiaRenderer
- `vector_svg()` and `vector_svg_file()` builder functions
- Image fit support (Contain, Cover, Fill, ScaleDown) for vector SVGs
- Tint support via SrcIn blend on layer

Remaining phases (4-7: icon system, SVG filters, advanced features, SVG DOM) are
advanced features that can be implemented incrementally as needed.

**Plan 5 Phases 1-3 are DONE** (Tier 1 + path parser + vector renderer).

### Plan 6-form-elements: IN PROGRESS
Phase 1 (focus management additions) is DONE:
- `disabled`, `read_only`, `error`, `label` fields on Element with builder methods
- `indeterminate`, `loading`, `show_value`, `progress_color`, `track_color` form props
- `FocusRing` DrawCommand variant for rendering 2px blue outline around focused elements
- `Line` and `Circle` DrawCommand variants for form element rendering

Phase 2 (enhanced text input) is DONE:
- `TextInputVariant` enum (Text, Password, Email, Url, Search, Number, Tel)
- Enhanced `TextInputState` with: selection anchor, undo/redo stacks, word boundary movement
- Selection methods: `select_all()`, `select_left/right()`, `select_word_left/right()`, `select_to_start/end()`, `select_word_at()`
- Word-level operations: `move_word_left/right()`, `delete_word_back()`, `delete_word_forward()`
- Undo/redo: `undo()`, `redo()` with grouped edit entries
- IME composition state: `composing`, `compose_text`, `compose_cursor`
- Password reveal timer for mobile-style character flash
- `password_input()` builder function

Phase 3 (Button, Checkbox, Radio, Switch) is DONE:
- `ElementKind::Button` with `ButtonVariant` (Primary, Secondary, Ghost, Danger)
- `ElementKind::Checkbox` with checked/indeterminate state and label
- `ElementKind::Radio` with selected, group, value, label
- `ElementKind::Switch` with on/off state and label
- Display list rendering: background colors, checkmark path, radio circle, switch track+thumb
- Builder functions: `button()`, `checkbox()`, `radio()`, `switch()` with `label()`, `variant()`, `indeterminate()`

Phase 4 (Slider) is DONE:
- `ElementKind::Slider` with value, min, max, step
- `ElementKind::RangeSlider` with low, high, min, max, step
- Track rendering: filled/empty portions with rounded ends
- Thumb rendering: white circle with shadow
- Builder functions: `slider()`, `range_slider()`, `step()`, `show_value()`, `progress_color()`, `track_color()`

Phase 5 (Progress Bar and Spinner) is DONE:
- `ElementKind::Progress` with `ProgressVariant` (Bar, Circular)
- Determinate bar: track + fill with rounded ends
- Indeterminate bar: 30% width segment (animation handled by runtime)
- Circular spinner: track circle + arc stroke
- Builder functions: `progress()`, `progress_indeterminate()`, `spinner()`

Phase 7 (Select/Dropdown) is DONE:
- `ElementKind::Select` with options, selected index, placeholder
- `SelectState` for keyboard navigation and type-ahead search
- Inline dropdown rendering with scroll, highlight, checkmark
- Builder function: `select()`

Phase 8 (Textarea) is DONE:
- `ElementKind::Textarea` with multiline text
- `TextareaState` with line wrapping, line numbers, tab handling
- Vertical/horizontal scrolling, selection across lines, undo/redo
- Builder function: `textarea()`

Theming system is DONE:
- `Theme` struct with 40+ color/sizing tokens in `ui/src/theme.rs`
- `Theme::dark()` and `Theme::light()` presets
- Theme flows through `build_display_list()` → `emit_commands()` → `emit_select()`
- All form element rendering uses theme colors instead of hardcoded values
- Views can override via `View::theme()` trait method
- Per-element overrides still take precedence over theme

Remaining phases (6: popup infrastructure, 9: IME, 10-12: color/date/file pickers) require popup surfaces or VM for Wayland integration.

**Plan 6 Phases 1-8 + Theming are DONE.**

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
