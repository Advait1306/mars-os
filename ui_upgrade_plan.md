# UI Upgrade state

You have to implement plans in ./plans/ui-upgrade. Some plans are already complete check ui_upgrade_plan.md for status. You must implement every plan and make a commit mention which plan was completed.

NOTE: Plan 1 & 2 are already complete before starting this loop.

### Plan 1-text-layout: COMPLETE
Phases 1-7 were already complete. Phase 8 is now DONE:
- `font_features: Vec<(String, i32)>` field on Element and TextSpan
- `font_variations: Vec<(String, f32)>` field on Element and TextSpan
- `.font_feature("tnum", 1)` builder method for OpenType features (tabular numbers, ligature control)
- `.font_variation("wght", 700.0)` builder method for variable font axes (weight, width, etc.)
- Applied via `TextStyle::add_font_feature()` in renderer for both Text and RichText
- Variable fonts via `TextStyle::set_font_arguments()` with `VariationPosition` coordinates
- Per-span font features and variations in RichText for mixed typography
- Threaded through DrawCommand::Text and DrawCommand::RichText

Phase 9 (BiDi/i18n) is DONE:
- `TextDirection` enum (Ltr, Rtl) in `style.rs`
- `text_direction` and `locale` fields on Element with `.text_direction()`, `.rtl()`, `.locale()` builder methods
- Same fields on `TextSpan` for per-span BiDi/locale in RichText
- Threaded through `DrawCommand::Text` and `DrawCommand::RichText`
- `ParagraphStyle::set_text_direction()` for RTL/LTR paragraph direction
- `TextStyle::set_locale()` for proper CJK/Arabic line breaking rules
- Applied in both `draw_text()` and `draw_rich_text()` renderer functions

Phase 10 (Inline Placeholders) is DONE:
- `PlaceholderAlignment` enum (Baseline, AboveBaseline, BelowBaseline, Top, Bottom, Middle)
- `InlinePlaceholder` struct with width, height, alignment, optional ImageSource
- `RichSpan` enum replacing `TextSpan` in RichText: `Text(TextSpan)` | `Placeholder(InlinePlaceholder)`
- `ParagraphBuilder::add_placeholder()` with Skia `PlaceholderStyle`
- `paragraph.get_rects_for_placeholders()` to position inline elements after layout
- Images rendered at placeholder positions via `draw_image()`
- `.inline_placeholder()` and `.inline_image()` builder methods on Element

Phase 11 (Performance Optimization) is DONE:
- Paragraph cache: `HashMap<u64, ParagraphCacheEntry>` keyed by content+style hash (excludes position/width/cursor state)
- Cache hit with same width: reuse paragraph directly (zero rebuild cost)
- Cache hit with different width: re-layout only (skip expensive shaping/building)
- Cache miss: build, layout, cache for next frame
- Frame-based LRU eviction: entries unused for 60+ frames swept every 120 frames
- Hard cap at 512 entries with full clear on overflow
- `begin_frame()` method on SkiaRenderer for frame counter and eviction
- FontCollection pre-warming: resolves `sans-serif`, `serif`, `monospace`, `system-ui` at startup
- Text input overlay extracted to `draw_text_input_overlay_static()` for cache-compatible rendering
- `Hash` + `Eq` derived on `Color`, `TextAlign`, `TextDirection`, `TextDecorationStyle` for cache key computation

**Plan 1 is now COMPLETE.** All 11 phases implemented.

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

Phase 7 (Polish and Optimization) is DONE:
- **Multiple backgrounds**: `Background` enum (Solid/Gradient/Image) with `Vec<Background>` on Element, `add_background()` builder
- **Background clip/origin**: `BackgroundClip` enum (BorderBox/PaddingBox/ContentBox), `BackgroundOrigin` enum, builders
- **Border image**: `BorderImage` struct with nine-slice rendering via `draw_image_nine()`, `border_image()` and `border_image_width()` builders
- **Gradient shader cache**: `HashMap<u64, Shader>` keyed by gradient+bounds hash, 256-entry cap, avoids redundant shader creation
- **Image loading helper**: `load_image()` method extracted for reuse across border-image and background-image
- `BackgroundLayers` and `BorderImage` DrawCommand variants with full renderer implementations

**Plan 4 is now COMPLETE.** All 7 phases implemented.

### Plan 5-svg-rendering: COMPLETE
Phase 1 (Tier 1 improvements) is DONE:
- Fixed cache key generation: content hash + dimensions instead of truncated string
- LRU cache eviction with 64MB byte budget
- Color tinting via `tint(color)` builder using SrcIn blend color filter
- ImageFit enum (Contain, Cover, Fill, ScaleDown) with `image_fit()` builder
- Proper image fit computation for all modes

Phase 2 (SVG path parser) is DONE:
- SVG path `d` attribute parser supporting M/m, L/l, H/h, V/v, C/c, Q/qx, Z/z commands
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

Phase 4 (Icon System) is DONE:
- `icon_registry.rs` module with `IconRegistry`, `IconPack`, `ResolvedIcon` types
- Multi-tier icon resolution: inline SVG → icon packs → freedesktop themes → pixmaps fallback
- `IconRegistry::register()` for inline SVG icons
- `IconRegistry::register_pack()` and `register_pack_from_dir()` for icon pack loading
- Freedesktop theme search across multiple sizes, categories, and themes (breeze-dark, hicolor)
- `ElementKind::Icon` with `icon()` builder function (default 24x24)
- `DrawCommand::Icon` resolved at render time via IconRegistry
- Auto-selects vector SVG for .svg files, rasterized for .png
- Tint and image fit support on icons
- Gray placeholder for unresolved icons

Phase 5 (SVG Filters) is DONE:
- Full filter pipeline: `build_filters()` composes usvg filter chains into Skia `ImageFilter`s
- Filter primitives mapped: GaussianBlur, Offset, ColorMatrix (matrix/saturate/hueRotate/luminanceToAlpha), ComponentTransfer, Composite (over/in/out/atop/xor/arithmetic), Blend (all 16 SVG modes), Morphology (erode/dilate), DropShadow, Flood, Merge, Tile
- Named result tracking for cross-referenced filter inputs (`result`/`in` attributes)
- SourceGraphic and SourceAlpha input resolution
- Color matrix helpers: `saturate_matrix()`, `hue_rotate_matrix()`, `luminance_to_alpha_matrix()`
- Transfer function table builder for all 5 types (identity, table, discrete, linear, gamma)
- Applied via `SaveLayerRec` with `ImageFilter` paint in `render_group()`
- Skipped: lighting filters (feDiffuseLighting/feSpecularLighting), feTurbulence, feImage, feConvolveMatrix, feDisplacementMap (uncommon in UI icons)

Phase 6 (Advanced SVG Features — partial) is DONE:
- **Masks**: `apply_mask()` renders mask content with `DstIn` blend mode
  - Luminance masks convert RGB to alpha via color matrix filter
  - Alpha masks use mask content alpha directly
  - Nested/chained masks supported via recursion
- **Patterns**: `usvg_paint_to_skia()` now renders pattern content into Skia `Picture`
  - `Picture::to_shader()` with `TileMode::Repeat` for tiling
  - Pattern transform applied via local matrix
  - Replaces previous gray fallback
- Text rendering from usvg was already done (flattened paths in Phase 3)
- Remaining: multi-resolution caching/HiDPI, async loading (lower priority)

Phase 7 (SVG DOM — Tier 3) is DONE:
- **SvgDocument**: Lightweight document model wrapping usvg tree with flat element list and ID map
- **SvgElement**: Per-element overrides for fill, stroke, opacity, visibility, transform
- **SvgElementKind**: Group, Path, Text, Image variants
- **SvgPaint/SvgStroke**: Simplified paint types for element-level overrides
- **from_data()**: Parses SVG → usvg tree → walks and builds element list with ID indexing
- **Query API**: `element_by_id()`, `element_by_id_mut()`, `element_ids()`, `element_count()`
- **Modification API**: `set_fill()`, `set_stroke()`, `set_opacity()`, `set_visible()`, `set_transform()`, `set_text()`
- **Dirty flag**: Modifications mark document dirty; `update()` re-records Skia Picture only when needed
- **Override rendering**: `render_with_overrides()` applies element-level overrides during re-recording
- **Hit testing**: `hit_test()` and `hit_test_in_bounds()` for bounds-based element detection
- **ElementKind::SvgDocument**: Arc<Mutex<SvgDocument>> for shared ownership with `svg_document()` builder
- **DrawCommand::SvgDocument**: Renders via `draw_fit()` with tint and image fit support
- **SVG event callbacks**: `on_svg_click()` and `on_svg_hover()` builder methods for SVG sub-element events
- Exported: `SvgDocument`, `SvgElement`, `SvgElementKind`, `SvgPaint`, `SvgStroke` from lib.rs

**Plan 5 is now COMPLETE.** All 7 phases implemented.

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

Phase 10 (Color Picker — pre-built) is DONE (partial):
- `Hsv` struct with `to_color()`, `to_color_with_alpha()`, `hue_color()` methods
- `Color::to_hsv()`, `Color::from_hsv()`, `Color::from_hsva()` conversions
- `Color::to_hex()`, `Color::to_hsl()`, `Color::lerp()` utilities
- `Color::luminance()`, `Color::is_dark()`, `Color::contrast_text()` for accessibility
- `ElementKind::ColorPicker` with `color_picker()` builder
- Closed-state rendering: color swatch + hex label + chevron
- `ColorPickerState` with full HSV state machine (open/close, SV drag, hue drag, alpha drag)
- `ColorPickerDrag` enum for tracking active drag target
- `parse_hex_color()` supporting #RGB, #RGBA, #RRGGBB, #RRGGBBAA
- Hex input with live validation, commit/revert, sync-while-unfocused
- Position query methods: `sv_position()`, `hue_position()`, `alpha_position()`
- 26 unit tests covering state, drags, hex parsing, color roundtrips
- Remaining: popup surface with SV gradient, hue slider, alpha slider (needs VM)

Phase 11 (Date/Time Picker — pre-built) is DONE (partial):
- `DatePickerVariant` enum (Date, Time, DateTime)
- `ElementKind::DatePicker` with `date_picker()`, `time_picker()`, `datetime_picker()` builders
- Closed-state rendering: formatted value or placeholder + calendar/clock icon
- Remaining: popup calendar/time selector UI (needs VM)

Phase 12 (File Input — pre-built) is DONE (partial):
- `ElementKind::FileInput` with `file_input()` builder, `accept` filter, `multiple` flag
- Closed-state rendering: "Choose File" button + file count label
- Remaining: native file dialog integration via DBus/portal (needs VM)

Remaining phases needing VM: Phase 6 (popup infrastructure), Phase 9 (IME protocol binding), popup portions of Phases 10-12.

**Plan 6 Phases 1-8 + Theming + Phases 10-12 (pre-built) are DONE.**

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
