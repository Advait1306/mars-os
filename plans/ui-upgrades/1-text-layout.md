# Text Layout & Rendering Implementation Plan

## Overview

Migration from basic `canvas.draw_str()` single-line text rendering to Skia's SkParagraph API (`skia_safe::textlayout`), enabling rich text, line wrapping, font fallback, text selection, hit testing, bidirectional text, and CSS-equivalent text styling.

### Current State

The framework currently uses:
- `canvas.draw_str(text, pos, font, paint)` for all text rendering (single line, single style)
- `FontMgr::default()` + `legacy_make_typeface(None, FontStyle::default())` for font loading (no named fonts, no fallback)
- `font.measure_str()` for width measurement, `font.metrics()` for ascent/descent
- `char_width = font_size * 0.6` rough estimate for Taffy layout measurement
- `textlayout` feature is enabled in `Cargo.toml` but none of its types are imported or used
- Text input uses character-index-based cursor positioning without proper glyph awareness
- No line wrapping, no rich text, no font fallback, no bidi support

### Target State

- All text rendered through `Paragraph` objects
- `FontCollection` with system font manager + custom font provider for fallback
- Accurate text measurement feeding into Taffy layout via `Paragraph::layout()` + intrinsic width queries
- Rich text (multiple styles per text block) via `ParagraphBuilder::push_style()`
- Line wrapping, max lines, ellipsis truncation
- Text selection and cursor positioning via `get_glyph_position_at_coordinate()` and `get_rects_for_range()`
- Bidirectional text, complex script shaping, emoji via ICU + HarfBuzz (bundled in Skia)

---

## Skia Paragraph API Reference

All types live in `skia_safe::textlayout::*`. The `textlayout` feature is already enabled.

### FontCollection

`FontCollection` manages font sources and fallback resolution. It is ref-counted (`RCHandle`) and should be created once and shared across all paragraph builders.

```rust
use skia_safe::textlayout::FontCollection;
use skia_safe::FontMgr;
```

#### Construction

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn new() -> FontCollection` | Create empty collection |

#### Font Manager Registration

A `FontCollection` searches its registered managers in order: asset, dynamic, test, default. Fallback uses the default manager.

| Method | Signature | Description |
|--------|-----------|-------------|
| `set_asset_font_manager` | `fn(&mut self, impl Into<Option<FontMgr>>)` | Set asset (bundled/custom) font manager |
| `set_dynamic_font_manager` | `fn(&mut self, impl Into<Option<FontMgr>>)` | Set dynamic (runtime-loaded) font manager |
| `set_test_font_manager` | `fn(&mut self, impl Into<Option<FontMgr>>)` | Set test font manager |
| `set_default_font_manager` | `fn(&mut self, impl Into<Option<FontMgr>>, impl Into<Option<&str>>)` | Set default manager with optional default family name |
| `set_default_font_manager_and_family_names` | `fn(&mut self, impl Into<Option<FontMgr>>, &[impl AsRef<str>])` | Set default manager with multiple fallback family names |

#### Querying

| Method | Signature | Description |
|--------|-----------|-------------|
| `font_managers_count()` | `fn(&self) -> usize` | Number of registered managers |
| `fallback_manager()` | `fn(&self) -> Option<FontMgr>` | Get the fallback manager |
| `find_typefaces` | `fn(&mut self, &[impl AsRef<str>], FontStyle) -> Vec<Typeface>` | Find typefaces matching family names and style |
| `find_typefaces_with_font_arguments` | `fn(&mut self, &[impl AsRef<str>], FontStyle, Option<&FontArguments>) -> Vec<Typeface>` | Find typefaces with font arguments (variable font axes) |
| `default_fallback_char` | `fn(&mut self, Unichar, FontStyle, impl AsRef<str>) -> Option<Typeface>` | Find fallback typeface for a specific character |
| `default_fallback` | `fn(&mut self) -> Option<Typeface>` | Get default fallback typeface |
| `default_emoji_fallback` | `fn(&mut self, Unichar, FontStyle, impl AsRef<str>) -> Option<Typeface>` | Find emoji fallback typeface |

#### Fallback Control

| Method | Signature | Description |
|--------|-----------|-------------|
| `enable_font_fallback()` | `fn(&mut self)` | Enable font fallback (default) |
| `disable_font_fallback()` | `fn(&mut self)` | Disable font fallback |
| `font_fallback_enabled()` | `fn(&self) -> bool` | Check if fallback is enabled |

#### Cache

| Method | Signature | Description |
|--------|-----------|-------------|
| `paragraph_cache()` | `fn(&self) -> &ParagraphCache` | Access paragraph cache (read) |
| `paragraph_cache_mut()` | `fn(&mut self) -> &mut ParagraphCache` | Access paragraph cache (write) |
| `clear_caches()` | `fn(&mut self)` | Clear all caches |

#### Typical Setup

```rust
let mut font_collection = FontCollection::new();
font_collection.set_default_font_manager(FontMgr::default(), None);
// font_collection is Clone (ref-counted), share across builders
```

### TypefaceFontProvider

A custom `FontMgr` that lets you register typefaces by family name. Derefs to `FontMgr`, so it can be passed anywhere a `FontMgr` is expected. Use this for bundled/custom fonts.

```rust
use skia_safe::textlayout::TypefaceFontProvider;
```

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn() -> TypefaceFontProvider` | Create empty provider |
| `register_typeface` | `fn(&mut self, Typeface, impl Into<Option<&str>>) -> usize` | Register typeface with optional alias name. Returns count. |

#### Usage for Custom Fonts

```rust
let mut provider = TypefaceFontProvider::new();
let typeface = Typeface::from_data(Data::new_copy(&font_bytes), None).unwrap();
provider.register_typeface(typeface, Some("MyCustomFont"));

let mut font_collection = FontCollection::new();
font_collection.set_asset_font_manager(Some(provider.into())); // into FontMgr
font_collection.set_default_font_manager(FontMgr::default(), None); // system fallback
```

### TypefaceFontStyleSet

Groups multiple typefaces under one family name. Derefs to `FontStyleSet`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `new(impl AsRef<str>)` | `fn(family_name) -> TypefaceFontStyleSet` | Create with family name |
| `family_name()` | `fn(&self) -> &str` | Get family name |
| `alias()` | `fn(&self) -> &str` | Get alias |
| `append_typeface` | `fn(&mut self, Typeface) -> &mut Self` | Add a typeface to the set |

### ParagraphStyle

Controls paragraph-level properties: alignment, direction, max lines, ellipsis, strut style.

```rust
use skia_safe::textlayout::ParagraphStyle;
```

#### Construction

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn() -> ParagraphStyle` | Create with defaults (LTR, left-aligned, unlimited lines) |

#### Text Style (Default)

| Method | Signature | Description |
|--------|-----------|-------------|
| `text_style()` | `fn(&self) -> &TextStyle` | Get default text style |
| `set_text_style` | `fn(&mut self, &TextStyle) -> &mut Self` | Set default text style |

#### Alignment

| Method | Signature | Description |
|--------|-----------|-------------|
| `text_align()` | `fn(&self) -> TextAlign` | Get alignment |
| `set_text_align` | `fn(&mut self, TextAlign) -> &mut Self` | Set alignment |
| `effective_align()` | `fn(&self) -> TextAlign` | Resolved alignment (Start/End resolved to Left/Right based on direction) |

**TextAlign enum values:**
| Value | Description |
|-------|-------------|
| `TextAlign::Left` | Left-aligned |
| `TextAlign::Right` | Right-aligned |
| `TextAlign::Center` | Center-aligned |
| `TextAlign::Justify` | Justified (inter-word spacing expanded to fill width) |
| `TextAlign::Start` | Start of text direction (Left for LTR, Right for RTL) |
| `TextAlign::End` | End of text direction (Right for LTR, Left for RTL) |

#### Direction

| Method | Signature | Description |
|--------|-----------|-------------|
| `text_direction()` | `fn(&self) -> TextDirection` | Get text direction |
| `set_text_direction` | `fn(&mut self, TextDirection) -> &mut Self` | Set text direction |

**TextDirection enum values:**
| Value | Description |
|-------|-------------|
| `TextDirection::LTR` | Left to right (default) |
| `TextDirection::RTL` | Right to left |

#### Line Limits

| Method | Signature | Description |
|--------|-----------|-------------|
| `max_lines()` | `fn(&self) -> Option<usize>` | Get max lines (`None` = unlimited) |
| `set_max_lines` | `fn(&mut self, impl Into<Option<usize>>) -> &mut Self` | Set max lines |
| `unlimited_lines()` | `fn(&self) -> bool` | Check if unlimited |

#### Ellipsis

| Method | Signature | Description |
|--------|-----------|-------------|
| `ellipsis()` | `fn(&self) -> &str` | Get ellipsis string |
| `set_ellipsis` | `fn(&mut self, impl AsRef<str>) -> &mut Self` | Set ellipsis string (e.g., `"\u{2026}"` for "...") |
| `ellipsized()` | `fn(&self) -> bool` | Check if ellipsis is set |

#### Height

| Method | Signature | Description |
|--------|-----------|-------------|
| `height()` | `fn(&self) -> scalar` | Get paragraph height |
| `set_height` | `fn(&mut self, scalar) -> &mut Self` | Set paragraph height |

#### Text Height Behavior

| Method | Signature | Description |
|--------|-----------|-------------|
| `text_height_behavior()` | `fn(&self) -> TextHeightBehavior` | Get height behavior |
| `set_text_height_behavior` | `fn(&mut self, TextHeightBehavior) -> &mut Self` | Set height behavior |

**TextHeightBehavior enum values:**
| Value | Description |
|-------|-------------|
| `TextHeightBehavior::All` | Apply height to all lines (default) |
| `TextHeightBehavior::DisableFirstAscent` | Disable extra height on first line ascent |
| `TextHeightBehavior::DisableLastDescent` | Disable extra height on last line descent |
| `TextHeightBehavior::DisableAll` | Disable both first ascent and last descent |

#### Strut Style

| Method | Signature | Description |
|--------|-----------|-------------|
| `strut_style()` | `fn(&self) -> &StrutStyle` | Get strut style |
| `set_strut_style` | `fn(&mut self, StrutStyle) -> &mut Self` | Set strut style |

#### Other

| Method | Signature | Description |
|--------|-----------|-------------|
| `hinting_is_on()` | `fn(&self) -> bool` | Check if hinting is enabled |
| `turn_hinting_off()` | `fn(&mut self) -> &mut Self` | Disable hinting |
| `replace_tab_characters()` | `fn(&self) -> bool` | Check if tabs are replaced with spaces |
| `set_replace_tab_characters` | `fn(&mut self, bool) -> &mut Self` | Enable/disable tab replacement |
| `apply_rounding_hack()` | `fn(&self) -> bool` | Check if rounding hack is applied |
| `set_apply_rounding_hack` | `fn(&mut self, bool) -> &mut Self` | Enable/disable rounding hack |

### StrutStyle

Strut defines a minimum line height based on a reference font, ensuring consistent line spacing even when the paragraph contains mixed font sizes. Named after the typographic concept of a "strut" (an invisible fixed-height element).

```rust
use skia_safe::textlayout::StrutStyle;
```

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn() -> StrutStyle` | Create default (disabled) |
| `font_families()` | `fn(&self) -> FontFamilies` | Get strut font families |
| `set_font_families` | `fn(&mut self, &[impl AsRef<str>]) -> &mut Self` | Set strut font families |
| `font_style()` | `fn(&self) -> FontStyle` | Get strut font style |
| `set_font_style` | `fn(&mut self, FontStyle) -> &mut Self` | Set strut font style |
| `font_size()` | `fn(&self) -> scalar` | Get strut font size |
| `set_font_size` | `fn(&mut self, scalar) -> &mut Self` | Set strut font size |
| `height()` | `fn(&self) -> scalar` | Get height multiplier |
| `set_height` | `fn(&mut self, scalar) -> &mut Self` | Set height multiplier |
| `leading()` | `fn(&self) -> scalar` | Get leading (extra space between lines) |
| `set_leading` | `fn(&mut self, scalar) -> &mut Self` | Set leading |
| `strut_enabled()` | `fn(&self) -> bool` | Check if strut is enabled |
| `set_strut_enabled` | `fn(&mut self, bool) -> &mut Self` | Enable/disable strut |
| `force_strut_height()` | `fn(&self) -> bool` | Check if strut forces all line heights |
| `set_force_strut_height` | `fn(&mut self, bool) -> &mut Self` | Force all lines to use strut height |
| `height_override()` | `fn(&self) -> bool` | Check if height override is enabled |
| `set_height_override` | `fn(&mut self, bool) -> &mut Self` | Enable height override |
| `half_leading()` | `fn(&self) -> bool` | Check if half-leading is used |
| `set_half_leading` | `fn(&mut self, bool) -> &mut Self` | Use half-leading distribution |

### TextStyle

Controls the visual appearance of a text run: font, size, color, decoration, spacing. Multiple TextStyles can be pushed onto a ParagraphBuilder to create rich text.

```rust
use skia_safe::textlayout::TextStyle;
```

#### Construction

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn() -> TextStyle` | Create with defaults (14px, black, no decoration) |
| `clone_for_placeholder()` | `fn(&self) -> TextStyle` | Clone for use as placeholder style |

#### Comparison

| Method | Signature | Description |
|--------|-----------|-------------|
| `equals` | `fn(&self, &TextStyle) -> bool` | Full equality |
| `equals_by_fonts` | `fn(&self, &TextStyle) -> bool` | Compare font properties only |
| `match_one_attribute` | `fn(&self, StyleType, &TextStyle) -> bool` | Match a single attribute |

**StyleType enum values:**
| Value | Description |
|-------|-------------|
| `StyleType::None` | No style |
| `StyleType::AllAttributes` | All attributes |
| `StyleType::Font` | Font family, size, style |
| `StyleType::Foreground` | Foreground color/paint |
| `StyleType::Background` | Background color/paint |
| `StyleType::Shadow` | Text shadows |
| `StyleType::Decorations` | Underline, overline, line-through |
| `StyleType::LetterSpacing` | Letter spacing |
| `StyleType::WordSpacing` | Word spacing |

#### Color

| Method | Signature | Description |
|--------|-----------|-------------|
| `color()` | `fn(&self) -> Color` | Get text color |
| `set_color` | `fn(&mut self, impl Into<Color>) -> &mut Self` | Set text color |

#### Foreground/Background Paint

For advanced effects (gradients, shaders, etc.) you can set a full `Paint` instead of a simple color.

| Method | Signature | Description |
|--------|-----------|-------------|
| `has_foreground()` | `fn(&self) -> bool` | Check if foreground paint is set |
| `foreground()` | `fn(&self) -> Paint` | Get foreground paint |
| `set_foreground_paint` | `fn(&mut self, &Paint) -> &mut Self` | Set foreground paint |
| `clear_foreground_color()` | `fn(&mut self) -> &mut Self` | Clear foreground paint |
| `has_background()` | `fn(&self) -> bool` | Check if background paint is set |
| `background()` | `fn(&self) -> Paint` | Get background paint |
| `set_background_paint` | `fn(&mut self, &Paint) -> &mut Self` | Set background paint (highlight behind text) |
| `clear_background_color()` | `fn(&mut self) -> &mut Self` | Clear background paint |

#### Font Properties

| Method | Signature | Description |
|--------|-----------|-------------|
| `font_size()` | `fn(&self) -> scalar` | Get font size in points |
| `set_font_size` | `fn(&mut self, scalar) -> &mut Self` | Set font size |
| `font_families()` | `fn(&self) -> FontFamilies` | Get font families (fallback chain) |
| `set_font_families` | `fn(&mut self, &[impl AsRef<str>]) -> &mut Self` | Set font families |
| `font_style()` | `fn(&self) -> FontStyle` | Get font style (weight, width, slant) |
| `set_font_style` | `fn(&mut self, FontStyle) -> &mut Self` | Set font style |
| `typeface()` | `fn(&self) -> Option<Typeface>` | Get explicit typeface override |
| `set_typeface` | `fn(&mut self, impl Into<Option<Typeface>>) -> &mut Self` | Set explicit typeface (bypasses family resolution) |

**FontStyle construction** (from `skia_safe::FontStyle`):
```rust
use skia_safe::FontStyle;
// FontStyle::new(weight, width, slant)
FontStyle::normal()           // Weight::NORMAL, Width::NORMAL, Slant::Upright
FontStyle::bold()             // Weight::BOLD, Width::NORMAL, Slant::Upright
FontStyle::italic()           // Weight::NORMAL, Width::NORMAL, Slant::Italic
FontStyle::bold_italic()      // Weight::BOLD, Width::NORMAL, Slant::Italic

// Custom: FontStyle::new(weight, width, slant)
use skia_safe::font_style::{Weight, Width, Slant};
FontStyle::new(Weight::from(600), Width::NORMAL, Slant::Upright) // SemiBold
```

**Weight values** (`skia_safe::font_style::Weight`):
| Constant | Value | CSS Equivalent |
|----------|-------|----------------|
| `Weight::INVISIBLE` | 0 | - |
| `Weight::THIN` | 100 | font-weight: 100 |
| `Weight::EXTRA_LIGHT` | 200 | font-weight: 200 |
| `Weight::LIGHT` | 300 | font-weight: 300 |
| `Weight::NORMAL` | 400 | font-weight: 400 / normal |
| `Weight::MEDIUM` | 500 | font-weight: 500 |
| `Weight::SEMI_BOLD` | 600 | font-weight: 600 |
| `Weight::BOLD` | 700 | font-weight: 700 / bold |
| `Weight::EXTRA_BOLD` | 800 | font-weight: 800 |
| `Weight::BLACK` | 900 | font-weight: 900 |
| `Weight::EXTRA_BLACK` | 1000 | - |

**Width values** (`skia_safe::font_style::Width`):
| Constant | Value | CSS Equivalent |
|----------|-------|----------------|
| `Width::ULTRA_CONDENSED` | 1 | font-stretch: ultra-condensed (50%) |
| `Width::EXTRA_CONDENSED` | 2 | font-stretch: extra-condensed (62.5%) |
| `Width::CONDENSED` | 3 | font-stretch: condensed (75%) |
| `Width::SEMI_CONDENSED` | 4 | font-stretch: semi-condensed (87.5%) |
| `Width::NORMAL` | 5 | font-stretch: normal (100%) |
| `Width::SEMI_EXPANDED` | 6 | font-stretch: semi-expanded (112.5%) |
| `Width::EXPANDED` | 7 | font-stretch: expanded (125%) |
| `Width::EXTRA_EXPANDED` | 8 | font-stretch: extra-expanded (150%) |
| `Width::ULTRA_EXPANDED` | 9 | font-stretch: ultra-expanded (200%) |

**Slant values** (`skia_safe::font_style::Slant`):
| Constant | CSS Equivalent |
|----------|----------------|
| `Slant::Upright` | font-style: normal |
| `Slant::Italic` | font-style: italic |
| `Slant::Oblique` | font-style: oblique |

#### Decoration

| Method | Signature | Description |
|--------|-----------|-------------|
| `decoration()` | `fn(&self) -> &Decoration` | Get decoration struct |
| `decoration_type()` | `fn(&self) -> TextDecoration` | Get decoration type |
| `decoration_mode()` | `fn(&self) -> TextDecorationMode` | Get decoration mode |
| `decoration_color()` | `fn(&self) -> Color` | Get decoration color |
| `decoration_style()` | `fn(&self) -> TextDecorationStyle` | Get decoration style |
| `decoration_thickness_multiplier()` | `fn(&self) -> scalar` | Get thickness multiplier |
| `set_decoration` | `fn(&mut self, &Decoration)` | Set full decoration struct |
| `set_decoration_type` | `fn(&mut self, TextDecoration)` | Set decoration type |
| `set_decoration_mode` | `fn(&mut self, TextDecorationMode)` | Set decoration mode |
| `set_decoration_color` | `fn(&mut self, impl Into<Color>)` | Set decoration color |
| `set_decoration_style` | `fn(&mut self, TextDecorationStyle)` | Set decoration style |
| `set_decoration_thickness_multiplier` | `fn(&mut self, scalar)` | Set thickness multiplier |

**TextDecoration bitflags** (can be combined with `|`):
| Value | Description |
|-------|-------------|
| `TextDecoration::NO_DECORATION` | None (default) |
| `TextDecoration::UNDERLINE` | Underline |
| `TextDecoration::OVERLINE` | Overline |
| `TextDecoration::LINE_THROUGH` | Strikethrough |

**TextDecorationStyle enum:**
| Value | CSS Equivalent |
|-------|----------------|
| `TextDecorationStyle::Solid` | text-decoration-style: solid |
| `TextDecorationStyle::Double` | text-decoration-style: double |
| `TextDecorationStyle::Dotted` | text-decoration-style: dotted |
| `TextDecorationStyle::Dashed` | text-decoration-style: dashed |
| `TextDecorationStyle::Wavy` | text-decoration-style: wavy |

**TextDecorationMode enum:**
| Value | Description |
|-------|-------------|
| `TextDecorationMode::Gaps` | Skip gaps over descenders (default) |
| `TextDecorationMode::Through` | Draw through descenders |

**Decoration struct:**
```rust
Decoration {
    ty: TextDecoration,           // which decorations
    mode: TextDecorationMode,     // gaps or through
    color: Color,                 // decoration color (TRANSPARENT = use text color)
    style: TextDecorationStyle,   // solid/double/dotted/dashed/wavy
    thickness_multiplier: scalar, // 1.0 = default thickness
}
```

#### Spacing

| Method | Signature | Description |
|--------|-----------|-------------|
| `letter_spacing()` | `fn(&self) -> scalar` | Get letter spacing (extra space between characters) |
| `set_letter_spacing` | `fn(&mut self, scalar) -> &mut Self` | Set letter spacing |
| `word_spacing()` | `fn(&self) -> scalar` | Get word spacing (extra space between words) |
| `set_word_spacing` | `fn(&mut self, scalar) -> &mut Self` | Set word spacing |

#### Line Height

| Method | Signature | Description |
|--------|-----------|-------------|
| `height()` | `fn(&self) -> scalar` | Get height multiplier (0.0 if not overridden) |
| `set_height` | `fn(&mut self, scalar) -> &mut Self` | Set height multiplier (e.g., 1.5 = 150% of font size) |
| `height_override()` | `fn(&self) -> bool` | Check if height override is active |
| `set_height_override` | `fn(&mut self, bool) -> &mut Self` | Enable/disable height override |
| `half_leading()` | `fn(&self) -> bool` | Use half-leading distribution |
| `set_half_leading` | `fn(&mut self, bool) -> &mut Self` | Set half-leading |

**Height behavior:** When `height_override` is true, the `height` value is used as a multiplier of font size to determine line height. The `half_leading` flag distributes extra space equally above and below (like CSS `line-height` behavior), rather than adding it all below.

#### Baseline

| Method | Signature | Description |
|--------|-----------|-------------|
| `text_baseline()` | `fn(&self) -> TextBaseline` | Get text baseline |
| `set_text_baseline` | `fn(&mut self, TextBaseline) -> &mut Self` | Set text baseline |
| `baseline_shift()` | `fn(&self) -> scalar` | Get baseline shift (vertical offset) |
| `set_baseline_shift` | `fn(&mut self, scalar) -> &mut Self` | Set baseline shift |

**TextBaseline enum:**
| Value | Description |
|-------|-------------|
| `TextBaseline::Alphabetic` | Standard Latin baseline (default) |
| `TextBaseline::Ideographic` | CJK ideographic baseline (bottom of character cell) |

#### Shadows

| Method | Signature | Description |
|--------|-----------|-------------|
| `shadows()` | `fn(&self) -> &[TextShadow]` | Get all shadows |
| `add_shadow` | `fn(&mut self, TextShadow) -> &mut Self` | Add a shadow |
| `reset_shadows()` | `fn(&mut self) -> &mut Self` | Remove all shadows |

**TextShadow struct:**
```rust
TextShadow {
    color: Color,         // shadow color
    offset: Point,        // (dx, dy) offset
    blur_sigma: f64,      // blur radius (gaussian sigma)
}
// Constructor:
TextShadow::new(color: impl Into<Color>, offset: impl Into<Point>, blur_sigma: f64)
```

#### Font Features

| Method | Signature | Description |
|--------|-----------|-------------|
| `font_features()` | `fn(&self) -> &[FontFeature]` | Get all features |
| `add_font_feature` | `fn(&mut self, impl AsRef<str>, i32)` | Add feature (e.g., `"liga"`, 1) |
| `reset_font_features()` | `fn(&mut self)` | Remove all features |

**FontFeature:** Read-only struct with `name() -> &str` and `value() -> i32`.

Common font feature tags:
| Tag | Description |
|-----|-------------|
| `"liga"` | Standard ligatures (fi, fl) |
| `"clig"` | Contextual ligatures |
| `"dlig"` | Discretionary ligatures |
| `"kern"` | Kerning |
| `"tnum"` | Tabular (monospaced) figures |
| `"pnum"` | Proportional figures |
| `"onum"` | Old-style figures |
| `"lnum"` | Lining figures |
| `"smcp"` | Small capitals |
| `"c2sc"` | Capitals to small capitals |
| `"frac"` | Fractions |
| `"zero"` | Slashed zero |
| `"ss01"`-`"ss20"` | Stylistic sets |
| `"swsh"` | Swash |
| `"calt"` | Contextual alternates |

#### Font Arguments (Variable Fonts)

| Method | Signature | Description |
|--------|-----------|-------------|
| `font_arguments()` | `fn(&self) -> Option<&FontArguments>` | Get font arguments |
| `set_font_arguments` | `fn(&mut self, Option<&FontArguments>)` | Set font arguments |

**FontArguments** from `skia_safe::textlayout::FontArguments`:
| Method | Signature | Description |
|--------|-----------|-------------|
| `clone_typeface` | `fn(&self, impl Into<Typeface>) -> Option<Typeface>` | Clone typeface with these arguments applied |

For variable font axes, construct via `skia_safe::FontArguments`:
```rust
use skia_safe::FontArguments;
let mut fa = FontArguments::default();
fa.set_variation_design_position(&[
    // tag, value pairs for variable font axes
    // wght, wdth, ital, slnt, opsz are common named axes
]);
```

#### Locale

| Method | Signature | Description |
|--------|-----------|-------------|
| `locale()` | `fn(&self) -> &str` | Get locale string |
| `set_locale` | `fn(&mut self, impl AsRef<str>) -> &mut Self` | Set locale (e.g., `"en"`, `"zh-Hans"`, `"ar"`) |

#### Font Metrics

| Method | Signature | Description |
|--------|-----------|-------------|
| `font_metrics()` | `fn(&self) -> FontMetrics` | Get font metrics for this text style |

#### Placeholder

| Method | Signature | Description |
|--------|-----------|-------------|
| `is_placeholder()` | `fn(&self) -> bool` | Check if this is a placeholder style |
| `set_placeholder()` | `fn(&mut self) -> &mut Self` | Mark as placeholder style |

### ParagraphBuilder

Builds a `Paragraph` by accumulating styled text runs and placeholders.

```rust
use skia_safe::textlayout::{ParagraphBuilder, ParagraphStyle, FontCollection};
```

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `fn(style: &ParagraphStyle, font_collection: impl Into<FontCollection>) -> Self` | Create builder. Also initializes ICU if `embed-icudtl` feature is enabled. |
| `push_style` | `fn(&mut self, &TextStyle) -> &mut Self` | Push a text style onto the style stack. Subsequent `add_text` calls use this style. |
| `pop()` | `fn(&mut self) -> &mut Self` | Pop the top style from the stack, reverting to previous style. |
| `peek_style()` | `fn(&mut self) -> TextStyle` | Get a copy of the current top-of-stack style. |
| `add_text` | `fn(&mut self, impl AsRef<str>) -> &mut Self` | Add UTF-8 text using the current style. |
| `add_placeholder` | `fn(&mut self, &PlaceholderStyle) -> &mut Self` | Add an inline placeholder (for images, icons in text flow). |
| `build()` | `fn(&mut self) -> Paragraph` | Build the paragraph (consumes accumulated text/styles). |
| `get_text()` | `fn(&mut self) -> &str` | Get the accumulated text so far. |
| `get_paragraph_style()` | `fn(&self) -> ParagraphStyle` | Get a copy of the paragraph style. |
| `reset()` | `fn(&mut self)` | Reset builder for reuse (clears text and styles). |

#### Rich Text Example

```rust
let mut style = ParagraphStyle::new();
style.set_text_align(TextAlign::Left);

let mut builder = ParagraphBuilder::new(&style, font_collection.clone());

// Normal text
let mut normal = TextStyle::new();
normal.set_font_size(16.0);
normal.set_color(Color::WHITE);
normal.set_font_families(&["Inter", "system-ui"]);
builder.push_style(&normal);
builder.add_text("Hello ");

// Bold text
let mut bold = normal.clone();
bold.set_font_style(FontStyle::bold());
builder.push_style(&bold);
builder.add_text("world");
builder.pop();

// Back to normal
builder.add_text("!");

let mut paragraph = builder.build();
paragraph.layout(available_width);
paragraph.paint(canvas, (x, y));
```

### PlaceholderStyle

Defines an inline non-text element (image, icon, widget) within the text flow.

```rust
use skia_safe::textlayout::{PlaceholderStyle, PlaceholderAlignment, TextBaseline};
```

**Fields:**
```rust
PlaceholderStyle {
    width: scalar,                      // width of placeholder
    height: scalar,                     // height of placeholder
    alignment: PlaceholderAlignment,    // how to align with text
    baseline: TextBaseline,             // which baseline to use
    baseline_offset: scalar,            // offset from baseline
}
```

**PlaceholderAlignment enum:**
| Value | Description |
|-------|-------------|
| `PlaceholderAlignment::Baseline` | Align placeholder baseline with text baseline |
| `PlaceholderAlignment::AboveBaseline` | Sit on top of the baseline |
| `PlaceholderAlignment::BelowBaseline` | Hang below the baseline |
| `PlaceholderAlignment::Top` | Align top edge with font top |
| `PlaceholderAlignment::Bottom` | Align bottom edge with font bottom |
| `PlaceholderAlignment::Middle` | Align middle with text middle |

**Constructor:**
```rust
PlaceholderStyle::new(
    width: scalar,
    height: scalar,
    alignment: PlaceholderAlignment,
    baseline: TextBaseline,
    offset: scalar,
)
```

#### Inline Image Example

```rust
builder.push_style(&text_style);
builder.add_text("Click the ");
builder.add_placeholder(&PlaceholderStyle::new(
    16.0, 16.0,  // 16x16 icon
    PlaceholderAlignment::Middle,
    TextBaseline::Alphabetic,
    0.0,
));
builder.add_text(" icon to continue.");

// After layout, get placeholder positions:
let rects = paragraph.get_rects_for_placeholders();
// rects[0].rect gives the bounds where you should draw the icon
```

### Paragraph

The laid-out paragraph. Created by `ParagraphBuilder::build()`, must be laid out with `layout(width)` before use.

```rust
use skia_safe::textlayout::Paragraph;
```

#### Layout and Paint

| Method | Signature | Description |
|--------|-----------|-------------|
| `layout` | `fn(&mut self, width: scalar)` | Perform text layout within the given width. Must be called before any query or paint. |
| `paint` | `fn(&self, canvas: &Canvas, p: impl Into<Point>)` | Paint the paragraph at position (x, y). |
| `mark_dirty()` | `fn(&mut self)` | Mark dirty (forces re-layout on next `layout()` call). |

#### Metrics After Layout

| Method | Signature | Description |
|--------|-----------|-------------|
| `max_width()` | `fn(&self) -> scalar` | The width passed to `layout()`. |
| `height()` | `fn(&self) -> scalar` | Total height of the laid-out paragraph. |
| `min_intrinsic_width()` | `fn(&self) -> scalar` | Minimum width to avoid word breaks (width of longest word). |
| `max_intrinsic_width()` | `fn(&self) -> scalar` | Width needed to lay out without any line breaks. |
| `alphabetic_baseline()` | `fn(&self) -> scalar` | Alphabetic baseline of the first line. |
| `ideographic_baseline()` | `fn(&self) -> scalar` | Ideographic baseline of the first line. |
| `longest_line()` | `fn(&self) -> scalar` | Width of the longest line (may be less than max_width). |
| `did_exceed_max_lines()` | `fn(&self) -> bool` | Whether content was truncated by max_lines. |
| `line_number()` | `fn(&self) -> usize` | Total number of lines. |

#### Selection Rectangles

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_rects_for_range` | `fn(&self, Range<usize>, RectHeightStyle, RectWidthStyle) -> Vec<TextBox>` | Get bounding rectangles for a text range (byte offsets). Used for selection highlighting. |
| `get_rects_for_placeholders()` | `fn(&self) -> Vec<TextBox>` | Get bounding rectangles for all placeholders. |

**RectHeightStyle enum:**
| Value | Description |
|-------|-------------|
| `RectHeightStyle::Tight` | Tight bounding boxes per run (default) |
| `RectHeightStyle::Max` | Maximum height of all runs in the line |
| `RectHeightStyle::IncludeLineSpacingMiddle` | Include line spacing, split half above / half below |
| `RectHeightStyle::IncludeLineSpacingTop` | Include line spacing above |
| `RectHeightStyle::IncludeLineSpacingBottom` | Include line spacing below |
| `RectHeightStyle::Strut` | Use strut height |

**RectWidthStyle enum:**
| Value | Description |
|-------|-------------|
| `RectWidthStyle::Tight` | Tight widths per run (default) |
| `RectWidthStyle::Max` | Extend last rect of each line to widest rect across all lines |

**TextBox struct:**
```rust
TextBox {
    rect: Rect,              // bounding rectangle
    direct: TextDirection,   // text direction in this box
}
```

#### Hit Testing

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_glyph_position_at_coordinate` | `fn(&self, impl Into<Point>) -> PositionWithAffinity` | Get text position from (x, y) coordinate. Core of click-to-position. |
| `get_word_boundary` | `fn(&self, offset: u32) -> Range<usize>` | Get word boundaries around a text offset. For double-click selection. |
| `get_line_number_at` | `fn(&self, TextIndex) -> Option<usize>` | Get line number for a UTF-8 text index. |
| `get_line_number_at_utf16_offset` | `fn(&self, TextIndex) -> Option<usize>` | Get line number for a UTF-16 offset. |

**PositionWithAffinity struct:**
```rust
PositionWithAffinity {
    position: i32,        // character index (UTF-8 byte offset)
    affinity: Affinity,   // which side of the position the cursor is on
}
```

**Affinity enum:**
| Value | Description |
|-------|-------------|
| `Affinity::Upstream` | Cursor is on the trailing edge of the previous character |
| `Affinity::Downstream` | Cursor is on the leading edge of the next character |

Affinity matters at line breaks: a position at the end of line N has the same byte offset as the beginning of line N+1. Upstream affinity means "end of line N", downstream means "beginning of line N+1".

#### Glyph Cluster Info

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_glyph_cluster_at` | `fn(&self, TextIndex) -> Option<GlyphClusterInfo>` | Get glyph cluster info at a text index. |
| `get_closest_glyph_cluster_at` | `fn(&self, impl Into<Point>) -> Option<GlyphClusterInfo>` | Get closest glyph cluster to a point. |
| `get_glyph_info_at_utf16_offset` | `fn(&mut self, usize) -> Option<GlyphInfo>` | Get glyph info at UTF-16 offset. |
| `get_closest_utf16_glyph_info_at` | `fn(&mut self, impl Into<Point>) -> Option<GlyphInfo>` | Get closest glyph info to a point (UTF-16 indices). |

**GlyphClusterInfo struct:**
```rust
GlyphClusterInfo {
    bounds: Rect,                // bounding rect of the glyph cluster
    text_range: TextRange,       // byte range in the text (Range<usize>)
    position: TextDirection,     // direction of this cluster
}
```

**GlyphInfo struct:**
```rust
GlyphInfo {
    grapheme_layout_bounds: Rect,           // bounding rect
    grapheme_cluster_text_range: TextRange, // byte range
    text_direction: TextDirection,          // direction
    is_ellipsis: bool,                      // whether this glyph is part of ellipsis
}
```

#### Line Metrics

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_line_metrics()` | `fn(&self) -> Vec<LineMetrics>` | Get metrics for all lines. |
| `get_line_metrics_at` | `fn(&self, usize) -> Option<LineMetrics>` | Get metrics for a specific line number. |
| `get_actual_text_range` | `fn(&self, line_number: usize, include_spaces: bool) -> TextRange` | Get visible text range for a line. |

**LineMetrics struct:**
```rust
LineMetrics {
    start_index: usize,              // byte offset where line starts
    end_index: usize,                // byte offset where line ends
    end_excluding_whitespaces: usize, // end excluding trailing whitespace
    end_including_newline: usize,     // end including newline character
    hard_break: bool,                // whether line ends with a hard break (\n)
    ascent: f64,                     // ascent (positive value, distance above baseline)
    descent: f64,                    // descent (positive value, distance below baseline)
    unscaled_ascent: f64,            // ascent before scaling
    height: f64,                     // cumulative height (top of paragraph to bottom of this line)
    width: f64,                      // width of the line
    left: f64,                       // left edge of the line
    baseline: f64,                   // y position of baseline from top of paragraph
    line_number: usize,              // zero-indexed line number
}
```

**Style metrics on LineMetrics:**
| Method | Signature | Description |
|--------|-----------|-------------|
| `get_style_metrics_count` | `fn(&self, Range<usize>) -> usize` | Count style runs in range |
| `get_style_metrics` | `fn(&self, Range<usize>) -> Vec<(usize, &StyleMetrics)>` | Get style runs with font metrics |

**StyleMetrics struct:**
```rust
StyleMetrics {
    text_style: &TextStyle,       // the text style for this run
    font_metrics: FontMetrics,    // resolved font metrics
}
```

#### Font Queries

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_font_at` | `fn(&self, TextIndex) -> Font` | Get resolved font at a text index. |
| `get_font_at_utf16_offset` | `fn(&mut self, usize) -> Font` | Get resolved font at UTF-16 offset. |
| `get_fonts()` | `fn(&self) -> Vec<FontInfo>` | Get all fonts used in the paragraph. |

**FontInfo struct:**
```rust
FontInfo {
    font: Font,              // the resolved font
    text_range: TextRange,   // byte range this font covers
}
```

#### Unresolved Glyphs

| Method | Signature | Description |
|--------|-----------|-------------|
| `unresolved_glyphs()` | `fn(&mut self) -> Option<usize>` | Count of glyphs that couldn't be resolved. `None` if not yet shaped. |
| `unresolved_codepoints()` | `fn(&mut self) -> Vec<Unichar>` | Get the actual unresolved codepoints. |

#### Visitor API

Low-level glyph-by-glyph access for custom rendering.

| Method | Signature | Description |
|--------|-----------|-------------|
| `visit` | `fn(&mut self, F: FnMut(usize, Option<&VisitorInfo>))` | Visit each glyph run. Called per run; `None` info signals end of line. |
| `extended_visit` | `fn(&mut self, F: FnMut(usize, Option<&ExtendedVisitorInfo>))` | Visit with extended info including per-glyph bounds. |

**VisitorInfo fields:**
- `font() -> &Font` - font used for this run
- `origin() -> Point` - origin position of this run
- `advance_x() -> scalar` - total advance width
- `count() -> usize` - number of glyphs
- `glyphs() -> &[u16]` - glyph IDs
- `positions() -> &[Point]` - glyph positions
- `utf8_starts() -> &[u32]` - UTF-8 byte offsets (count + 1 entries)
- `flags() -> VisitorFlags` - flags (e.g., `VisitorFlags::WHITE_SPACE`)

**ExtendedVisitorInfo** adds:
- `advance() -> Size` - advance as Size (width, height)
- `bounds() -> &[Rect]` - per-glyph bounding rectangles

#### Path Extraction

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_path_at` | `fn(&mut self, line_number: usize) -> (usize, Path)` | Get path for a line. Returns (unconverted_glyph_count, path). |
| `get_path` (static) | `fn(text_blob: &mut TextBlob) -> Path` | Get path from a text blob. |

#### Emoji Detection

| Method | Signature | Description |
|--------|-----------|-------------|
| `contains_emoji` | `fn(&mut self, &mut TextBlob) -> bool` | Check if text blob contains emoji. |
| `contains_color_font_or_bitmap` | `fn(&mut self, &mut TextBlob) -> bool` | Check for color fonts or bitmaps. |

### ParagraphCache

Built-in caching for paragraph layout results. Accessed via `FontCollection::paragraph_cache()`.

| Method | Signature | Description |
|--------|-----------|-------------|
| `new()` | `fn() -> ParagraphCache` | Create new cache |
| `abandon()` | `fn(&mut self)` | Abandon cache |
| `reset()` | `fn(&mut self)` | Clear cache |
| `print_statistics()` | `fn(&mut self)` | Print cache stats to stdout |
| `turn_on` | `fn(&mut self, bool)` | Enable/disable caching |
| `count()` | `fn(&mut self) -> i32` | Number of cached entries |

---

## CSS Text Properties to SkParagraph Mapping

### Font Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `font-family` | `"Inter", sans-serif` | `TextStyle::set_font_families(&["Inter", "sans-serif"])` | P0 |
| `font-size` | `16px` | `TextStyle::set_font_size(16.0)` | P0 |
| `font-weight` | `100`-`900`, `normal`, `bold` | `TextStyle::set_font_style(FontStyle::new(Weight::from(n), ..))` | P0 |
| `font-style` | `normal`, `italic`, `oblique` | `TextStyle::set_font_style(FontStyle::new(.., .., Slant::Italic))` | P0 |
| `font-stretch` | `condensed`-`expanded` | `TextStyle::set_font_style(FontStyle::new(.., Width::CONDENSED, ..))` | P2 |
| `font-variant` | `small-caps`, etc. | `TextStyle::add_font_feature("smcp", 1)` | P2 |
| `font-feature-settings` | `"liga" 1, "tnum" 1` | `TextStyle::add_font_feature("liga", 1)` per feature | P2 |
| `font-variation-settings` | `"wght" 600` | `TextStyle::set_font_arguments(...)` | P3 |

### Text Layout Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `text-align` | `left`, `center`, `right`, `justify` | `ParagraphStyle::set_text_align(TextAlign::Center)` | P0 |
| `text-indent` | `2em` | No direct API. Prepend spaces or use placeholder. | P3 |
| `text-transform` | `uppercase`, `lowercase`, `capitalize` | Not in Skia. Transform text string before passing to builder. | P2 |
| `white-space: normal` | Collapse whitespace, wrap | Default paragraph behavior | P0 |
| `white-space: nowrap` | Collapse whitespace, no wrap | `ParagraphStyle::set_max_lines(1)` + very large width | P1 |
| `white-space: pre` | Preserve whitespace, no wrap | `ParagraphStyle::set_replace_tab_characters(false)` + newlines in text | P2 |
| `white-space: pre-wrap` | Preserve whitespace, wrap | Same as pre but with normal wrapping width | P2 |
| `white-space: pre-line` | Collapse whitespace, preserve newlines | Normalize whitespace but keep `\n` | P2 |
| `word-break` | `normal`, `break-all`, `keep-all` | SkParagraph handles via ICU line breaking. No direct flag exposed. | P3 |
| `overflow-wrap` | `normal`, `break-word`, `anywhere` | SkParagraph breaks words when necessary by default. | P2 |
| `hyphens` | `none`, `manual`, `auto` | Not directly exposed. Could use Shaper's hyphenation if available. | P3 |
| `tab-size` | `4`, `8` | `ParagraphStyle::set_replace_tab_characters(true/false)` | P3 |

### Line Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `line-height` | `1.5`, `24px`, `normal` | `TextStyle::set_height(1.5)` + `set_height_override(true)` | P0 |
| `line-clamp` / `max-lines` | `3` | `ParagraphStyle::set_max_lines(3)` | P1 |
| `vertical-align` | `baseline`, `super`, `sub` | `TextStyle::set_baseline_shift(offset)` | P2 |

### Spacing Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `letter-spacing` | `0.05em`, `2px` | `TextStyle::set_letter_spacing(2.0)` | P1 |
| `word-spacing` | `4px` | `TextStyle::set_word_spacing(4.0)` | P2 |

### Decoration Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `text-decoration-line` | `underline`, `overline`, `line-through` | `TextStyle::set_decoration_type(TextDecoration::UNDERLINE)` | P1 |
| `text-decoration-color` | `red`, `#ff0000` | `TextStyle::set_decoration_color(Color::RED)` | P1 |
| `text-decoration-style` | `solid`, `double`, `dotted`, `dashed`, `wavy` | `TextStyle::set_decoration_style(TextDecorationStyle::Wavy)` | P1 |
| `text-decoration-thickness` | `2px`, `from-font` | `TextStyle::set_decoration_thickness_multiplier(2.0)` | P2 |
| `text-underline-offset` | Not directly available in SkParagraph | Not mapped. Would need custom rendering. | P3 |
| `text-underline-position` | `under`, `from-font` | Not directly available. Default is `from-font`. | P3 |

### Overflow Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `text-overflow: ellipsis` | `clip`, `ellipsis` | `ParagraphStyle::set_ellipsis("\u{2026}")` + `set_max_lines(n)` | P0 |
| `-webkit-line-clamp` | `3` | `ParagraphStyle::set_max_lines(3)` + `set_ellipsis(...)` | P1 |

### Direction Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `direction` | `ltr`, `rtl` | `ParagraphStyle::set_text_direction(TextDirection::RTL)` | P1 |
| `unicode-bidi` | `normal`, `embed`, `override` | Handled automatically by ICU BiDi in SkParagraph | P3 |
| `writing-mode` | `horizontal-tb`, `vertical-rl` | Not supported by SkParagraph (horizontal only) | P3 |
| `text-orientation` | `mixed`, `upright` | Not supported | P3 |

### Shadow Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `text-shadow` | `2px 2px 4px rgba(0,0,0,0.5)` | `TextStyle::add_shadow(TextShadow::new(color, (2,2), 4.0))` | P1 |

Multiple shadows: call `add_shadow()` multiple times.

### Selection Properties

| CSS Property | Values | SkParagraph API | Priority |
|-------------|--------|-----------------|----------|
| `::selection` | `background`, `color` | Use `get_rects_for_range()` to get selection rects, then draw custom background. Override text color by re-rendering selected range with different TextStyle. | P1 |
| `user-select` | `none`, `text`, `all` | Framework-level flag, not an SkParagraph property | P2 |

---

## Rust DSL API Design

### Text Element Enhancements

Extend the existing `Element` and builder API:

```rust
// Current (keep working):
text("Hello").font_size(16.0).color(WHITE)

// New capabilities:
text("Hello world")
    .font_family("Inter")           // primary font family
    .font_families(&["Inter", "Noto Sans", "sans-serif"]) // full fallback chain
    .font_weight(700)               // numeric weight (100-900)
    .bold()                         // shorthand for font_weight(700)
    .italic()                       // font_style: italic
    .font_size(16.0)
    .line_height(1.5)               // multiplier
    .letter_spacing(0.5)
    .text_align(TextAlign::Center)
    .max_lines(3)
    .ellipsis()                     // text-overflow: ellipsis
    .underline()                    // text-decoration: underline
    .strikethrough()                // text-decoration: line-through
    .text_shadow(Color::BLACK, (1.0, 1.0), 2.0)
    .color(WHITE)

// Rich text (multiple styles in one text block):
rich_text()
    .span("Hello ", |s| s.color(WHITE))
    .span("world", |s| s.color(RED).bold())
    .span("!", |s| s.color(WHITE))
    .font_size(16.0)
    .line_height(1.5)
```

### New Style Properties on Element

```rust
pub struct Element {
    // ... existing fields ...

    // Text properties (new)
    pub font_family: Option<Vec<String>>,       // font-family fallback chain
    pub font_weight: Option<i32>,               // 100-900
    pub font_italic: bool,                      // font-style: italic
    pub line_height: Option<f32>,               // line-height multiplier
    pub letter_spacing: f32,                    // letter-spacing in px
    pub word_spacing: f32,                      // word-spacing in px
    pub text_align: Option<TextAlign>,          // text-align
    pub max_lines: Option<usize>,               // -webkit-line-clamp / max-lines
    pub text_overflow_ellipsis: bool,           // text-overflow: ellipsis
    pub text_decoration: TextDecoration,        // underline/overline/line-through
    pub text_decoration_style: TextDecorationStyle,
    pub text_decoration_color: Option<Color>,
    pub text_shadow: Vec<(Color, (f32, f32), f64)>,
    pub user_select: bool,                      // user-select: none vs text
}
```

### New ElementKind for Rich Text

```rust
pub enum ElementKind {
    // ... existing variants ...
    RichText { spans: Vec<TextSpan> },
}

pub struct TextSpan {
    pub content: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub font_weight: Option<i32>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub font_family: Option<Vec<String>>,
    pub letter_spacing: Option<f32>,
    pub background: Option<Color>,  // highlight
}
```

---

## Font Management

### System Font Discovery (Linux via fontconfig)

On Linux, `FontMgr::default()` returns the fontconfig-based font manager. It automatically discovers all system-installed fonts. Skia's fontconfig integration:

- Reads `/etc/fonts/fonts.conf` and related configs
- Scans font directories (`/usr/share/fonts/`, `~/.local/share/fonts/`, etc.)
- Supports font aliases (`sans-serif` -> `DejaVu Sans`, etc.)
- Handles font matching (weight, width, slant)

Common system font locations on Debian/MarsOS:
- `/usr/share/fonts/truetype/dejavu/` - DejaVu family
- `/usr/share/fonts/truetype/noto/` - Noto family
- `/usr/share/fonts/truetype/liberation/` - Liberation family
- `/usr/share/fonts/opentype/noto/` - Noto CJK

### Custom Font Loading

```rust
// From file
let data = std::fs::read("/path/to/font.ttf").unwrap();
let typeface = Typeface::from_data(
    skia_safe::Data::new_copy(&data),
    None,  // face index for .ttc files
).unwrap();

// Register in provider
let mut provider = TypefaceFontProvider::new();
provider.register_typeface(typeface, Some("MyFont"));

// Add to collection
let mut fc = FontCollection::new();
fc.set_asset_font_manager(Some(provider.into()));
fc.set_default_font_manager(FontMgr::default(), None);
```

### Font Fallback Chain

When a glyph is not found in the primary font, SkParagraph automatically falls back:

1. Try each family in `TextStyle::font_families()` in order
2. If `FontCollection::font_fallback_enabled()`, try `FontMgr::matchFamilyStyleCharacter()` for the missing codepoint
3. Use locale hint from `TextStyle::locale()` to prefer appropriate CJK variants
4. Fall back to `FontCollection::default_fallback()`

**Best practice for font family lists:**
```rust
ts.set_font_families(&[
    "Inter",            // preferred font
    "Noto Sans",        // secondary
    "Noto Color Emoji", // emoji support
    "Noto Sans CJK SC", // Chinese fallback
    "sans-serif",       // system sans-serif
]);
```

### Variable Fonts

Variable fonts contain multiple styles in one file, controlled by axes:

| Axis Tag | Name | Range | Description |
|----------|------|-------|-------------|
| `wght` | Weight | 1-1000 | font-weight equivalent |
| `wdth` | Width | 25-200 | font-stretch equivalent (percentage) |
| `ital` | Italic | 0-1 | italic (0=normal, 1=italic) |
| `slnt` | Slant | -90-90 | oblique angle in degrees |
| `opsz` | Optical Size | varies | Adjust design for target size |

Variable font axes are set through `FontArguments`:
```rust
let mut fa = skia_safe::FontArguments::default();
// Set variation design position for custom axis values
// Then use TextStyle::set_font_arguments() with the textlayout FontArguments wrapper
```

### Font Features

OpenType font features control typographic refinements:

```rust
let mut ts = TextStyle::new();
// Enable tabular (monospaced) figures
ts.add_font_feature("tnum", 1);
// Disable standard ligatures
ts.add_font_feature("liga", 0);
// Enable stylistic set 1
ts.add_font_feature("ss01", 1);
```

---

## Rich Text

### Multiple Styles in One Text Block

The `ParagraphBuilder` style stack allows arbitrary style changes within a paragraph:

```rust
let mut builder = ParagraphBuilder::new(&para_style, font_collection);

// Push base style
let mut base = TextStyle::new();
base.set_font_size(16.0);
base.set_color(Color::WHITE);
base.set_font_families(&["Inter"]);
builder.push_style(&base);
builder.add_text("Regular text ");

// Bold word
let mut bold = base.clone();
bold.set_font_style(FontStyle::bold());
builder.push_style(&bold);
builder.add_text("bold");
builder.pop();  // back to base style

// Colored text
let mut red = base.clone();
red.set_color(Color::RED);
builder.push_style(&red);
builder.add_text(" red text");
builder.pop();

// Underlined
let mut underlined = base.clone();
underlined.set_decoration_type(TextDecoration::UNDERLINE);
builder.push_style(&underlined);
builder.add_text(" underlined");
builder.pop();

let mut paragraph = builder.build();
```

### Inline Elements (Placeholders)

For images, icons, or custom widgets inline with text:

```rust
builder.add_text("Click ");

// Add a 16x16 inline icon placeholder
builder.add_placeholder(&PlaceholderStyle::new(
    16.0, 16.0,
    PlaceholderAlignment::Middle,
    TextBaseline::Alphabetic,
    0.0,
));

builder.add_text(" to save.");

let mut para = builder.build();
para.layout(width);

// After layout, find where to draw the icon
let placeholder_rects = para.get_rects_for_placeholders();
if let Some(tb) = placeholder_rects.first() {
    // Draw icon at tb.rect
    canvas.draw_image(&icon, (tb.rect.left, tb.rect.top), None);
}
```

### Style Boundaries and Measurement

SkParagraph handles style boundaries automatically during shaping. Each style run becomes a separate shaping segment. Things to be aware of:

- Kerning between different style runs may not be applied (depends on font)
- Line breaking considers the full paragraph text regardless of style boundaries
- `get_rects_for_range()` returns separate rectangles for each style run within the requested range
- `get_fonts()` returns the resolved font for each text segment

---

## Text Selection & Hit Testing

### Click to Cursor Position

```rust
// User clicks at (click_x, click_y) relative to paragraph origin
let pos = paragraph.get_glyph_position_at_coordinate((click_x, click_y));
let cursor_byte_offset = pos.position as usize;
let affinity = pos.affinity; // Upstream or Downstream

// Convert byte offset to character index if needed
let cursor_char_index = text[..cursor_byte_offset].chars().count();
```

### Selection Highlight Rendering

```rust
// User has selected from byte offset `start` to `end`
let rects = paragraph.get_rects_for_range(
    start..end,
    RectHeightStyle::Tight,     // or Max for full-line height selection
    RectWidthStyle::Tight,
);

// Draw selection highlight
let mut paint = Paint::default();
paint.set_color(Color::from_argb(80, 100, 150, 255)); // semi-transparent blue

for text_box in &rects {
    canvas.draw_rect(text_box.rect, &paint);
}

// Then paint the paragraph on top
paragraph.paint(canvas, origin);
```

### Double-Click Word Selection

```rust
let pos = paragraph.get_glyph_position_at_coordinate((click_x, click_y));
let word_range = paragraph.get_word_boundary(pos.position as u32);
// word_range is Range<usize> of byte offsets
// Set selection to word_range.start..word_range.end
```

### Triple-Click Line Selection

```rust
let pos = paragraph.get_glyph_position_at_coordinate((click_x, click_y));
let byte_offset = pos.position as usize;

if let Some(line_num) = paragraph.get_line_number_at(byte_offset) {
    if let Some(lm) = paragraph.get_line_metrics_at(line_num) {
        // Select entire line
        let selection_start = lm.start_index;
        let selection_end = lm.end_excluding_whitespaces;
    }
}
```

### Cursor Rectangle Computation

```rust
// Get the rectangle for the cursor at byte_offset
let rects = paragraph.get_rects_for_range(
    cursor_byte_offset..cursor_byte_offset,
    RectHeightStyle::Tight,
    RectWidthStyle::Tight,
);

// If empty range returns no rects, use glyph cluster info
if rects.is_empty() {
    if let Some(info) = paragraph.get_glyph_cluster_at(cursor_byte_offset) {
        // Draw cursor at info.bounds left edge (or right for RTL)
        let cursor_x = match info.position {
            TextDirection::LTR => info.bounds.left,
            TextDirection::RTL => info.bounds.right,
        };
    }
}
```

### Selection Across Multiple Text Elements

For document-level selection spanning multiple paragraphs:

1. Track selection as (start_element_id, start_offset, end_element_id, end_offset)
2. For the start element: highlight from start_offset to end of text
3. For middle elements: highlight entire text
4. For the end element: highlight from start to end_offset
5. Each element uses its own Paragraph's `get_rects_for_range()`

---

## Line Breaking & Wrapping

### Default Behavior

SkParagraph uses ICU line breaking by default, which follows the Unicode Line Breaking Algorithm (UAX #14):

- Breaks at whitespace boundaries
- Breaks after hyphens
- Emergency breaks within words when a word exceeds the available width
- Respects non-breaking spaces (`\u{00A0}`)
- Handles CJK line breaking (break between any CJK characters)

### Max Lines and Ellipsis

```rust
let mut style = ParagraphStyle::new();
style.set_max_lines(3);
style.set_ellipsis("\u{2026}"); // Unicode ellipsis character

// After layout:
if paragraph.did_exceed_max_lines() {
    // Content was truncated
}
```

### Single-Line Ellipsis (text-overflow: ellipsis)

```rust
let mut style = ParagraphStyle::new();
style.set_max_lines(1);
style.set_ellipsis("\u{2026}");
```

### No Wrapping (white-space: nowrap)

```rust
let mut style = ParagraphStyle::new();
style.set_max_lines(1);
// Layout with a very large width, then clip
paragraph.layout(f32::MAX);
// Use actual width: paragraph.longest_line()
```

### CJK Line Breaking

CJK text can break between any character by default (per Unicode UAX #14). No special configuration needed. Set locale for proper behavior:

```rust
let mut ts = TextStyle::new();
ts.set_locale("zh-Hans"); // Simplified Chinese
// or "ja" for Japanese, "ko" for Korean
```

---

## Bidirectional Text

### How SkParagraph Handles BiDi

SkParagraph uses ICU's BiDi algorithm (UAX #9) automatically:

1. Detects text direction per-run based on Unicode character properties
2. Reorders runs for visual display
3. Handles nested directional embeddings
4. `ParagraphStyle::set_text_direction()` sets the base direction

### Mixed Direction Content

```rust
let mut style = ParagraphStyle::new();
style.set_text_direction(TextDirection::LTR); // base direction

let mut builder = ParagraphBuilder::new(&style, fc);
let mut ts = TextStyle::new();
ts.set_font_families(&["Inter", "Noto Sans Arabic"]);
builder.push_style(&ts);

// Mixed LTR and RTL text - BiDi algorithm handles it
builder.add_text("Hello \u{0645}\u{0631}\u{062D}\u{0628}\u{0627} World");
```

### TextDirection in Selection

`TextBox::direct` in selection results tells you the direction of each box. This matters for:
- Cursor movement (logical vs visual)
- Selection rendering (may be non-contiguous visually)
- Keyboard shortcuts (Home/End behavior)

### Complex Scripts

SkParagraph uses HarfBuzz for text shaping, which handles:
- **Arabic**: right-to-left, contextual shaping (initial/medial/final/isolated forms), ligatures
- **Devanagari**: complex conjuncts, reordering of vowel marks
- **Thai**: no word spaces, line breaking requires dictionary
- **Khmer**: complex stacking
- **Tibetan**: vertical stacking

No special API calls needed - HarfBuzz handles this during `Paragraph::layout()`.

---

## Text Decoration

### Underline

```rust
let mut ts = TextStyle::new();
ts.set_decoration_type(TextDecoration::UNDERLINE);
ts.set_decoration_style(TextDecorationStyle::Solid);
ts.set_decoration_color(Color::RED);
ts.set_decoration_thickness_multiplier(1.5);
```

### Multiple Decorations

```rust
ts.set_decoration_type(TextDecoration::UNDERLINE | TextDecoration::LINE_THROUGH);
```

### Decoration Mode

```rust
// Skip gaps over descenders (default, looks better)
ts.set_decoration_mode(TextDecorationMode::Gaps);

// Draw through descenders (simpler)
ts.set_decoration_mode(TextDecorationMode::Through);
```

### Text Shadow

```rust
let mut ts = TextStyle::new();
// Drop shadow
ts.add_shadow(TextShadow::new(
    Color::from_argb(128, 0, 0, 0),  // 50% black
    (2.0, 2.0),                       // offset
    3.0,                               // blur sigma
));
// Glow effect (second shadow)
ts.add_shadow(TextShadow::new(
    Color::from_argb(100, 0, 128, 255), // blue glow
    (0.0, 0.0),                          // no offset
    8.0,                                  // large blur
));
```

---

## Text Input Integration

### Current Text Input (TextInputState)

The existing `TextInputState` tracks cursor_position as a character index and uses basic char-by-char measurement. This needs to be upgraded to use SkParagraph for accurate cursor positioning.

### Cursor Positioning with SkParagraph

```rust
// Build paragraph from input text
let mut builder = ParagraphBuilder::new(&para_style, fc);
builder.push_style(&text_style);
builder.add_text(&input_value);
let mut para = builder.build();
para.layout(input_width);

// Get cursor x position
// Convert character index to byte offset
let byte_offset = input_value.char_indices()
    .nth(cursor_position)
    .map(|(i, _)| i)
    .unwrap_or(input_value.len());

// Get cursor rect
let rects = para.get_rects_for_range(
    byte_offset..byte_offset,
    RectHeightStyle::Tight,
    RectWidthStyle::Tight,
);
```

### Click to Cursor in Text Input

```rust
// User clicks at (x, y) relative to the text input element
let pos = para.get_glyph_position_at_coordinate((x - text_x, y - text_y));
let new_cursor_byte = pos.position as usize;
let new_cursor_char = input_value[..new_cursor_byte].chars().count();
text_input_state.cursor_position = new_cursor_char;
```

### Selection Rendering in Text Input

```rust
if let Some((start, end)) = text_input_state.selection {
    let start_byte = char_to_byte_offset(&input_value, start);
    let end_byte = char_to_byte_offset(&input_value, end);
    let rects = para.get_rects_for_range(
        start_byte..end_byte,
        RectHeightStyle::Tight,
        RectWidthStyle::Tight,
    );
    // Draw selection highlight rectangles
}
```

### IME Composition Rendering

For input methods (Chinese, Japanese, Korean, etc.), the preedit text needs special rendering:

```rust
// During IME composition:
let committed_text = &input_value[..commit_offset];
let preedit_text = &ime_preedit;
let remaining_text = &input_value[commit_offset..];

let full_display = format!("{}{}{}", committed_text, preedit_text, remaining_text);

// Build paragraph with special style for preedit region
builder.push_style(&normal_style);
builder.add_text(committed_text);

// Preedit text with underline
let mut preedit_style = normal_style.clone();
preedit_style.set_decoration_type(TextDecoration::UNDERLINE);
preedit_style.set_decoration_style(TextDecorationStyle::Dotted);
builder.push_style(&preedit_style);
builder.add_text(preedit_text);
builder.pop();

builder.add_text(remaining_text);
```

### Scroll Position Calculation

When the cursor moves beyond the visible area of a text input:

```rust
// Get cursor x position from paragraph
let cursor_x = get_cursor_x_from_paragraph(&para, cursor_byte_offset);

// Scroll to keep cursor visible
let visible_width = input_width - padding * 2.0;
if cursor_x - text_input_state.scroll_offset > visible_width {
    text_input_state.scroll_offset = cursor_x - visible_width;
}
if cursor_x - text_input_state.scroll_offset < 0.0 {
    text_input_state.scroll_offset = cursor_x;
}

// Apply scroll offset when painting
para.paint(canvas, (text_x - text_input_state.scroll_offset, text_y));
```

---

## Layout Integration with Taffy

### Current Problem

Text measurement for Taffy layout currently uses `char_width = font_size * 0.6`, which is inaccurate. It doesn't account for variable-width characters, kerning, or line wrapping.

### Solution: Paragraph-Based Measurement

Replace the Taffy measure function to use real SkParagraph layout:

```rust
// In the Taffy measure callback:
|known_dimensions, available_space, _node_id, node_context, _style| {
    if let Some(ctx) = node_context {
        // Build a Paragraph for measurement
        let mut builder = ParagraphBuilder::new(&ctx.para_style, ctx.font_collection.clone());
        builder.push_style(&ctx.text_style);
        builder.add_text(&ctx.content);
        let mut para = builder.build();

        // Determine available width
        let available_width = known_dimensions.width.unwrap_or_else(|| {
            match available_space.width {
                AvailableSpace::Definite(w) => w,
                AvailableSpace::MinContent => 0.0,  // will give min_intrinsic_width
                AvailableSpace::MaxContent => f32::MAX, // will give max_intrinsic_width
            }
        });

        para.layout(available_width);

        let width = known_dimensions.width.unwrap_or_else(|| {
            match available_space.width {
                AvailableSpace::MinContent => para.min_intrinsic_width(),
                AvailableSpace::MaxContent => para.max_intrinsic_width(),
                _ => para.longest_line().ceil(),
            }
        });

        let height = known_dimensions.height.unwrap_or(para.height());

        Size { width, height }
    } else {
        Size::ZERO
    }
}
```

### Intrinsic Sizing

SkParagraph provides two key intrinsic width measurements:

- **`min_intrinsic_width()`**: The minimum width needed without breaking words. Corresponds to CSS `min-content`.
- **`max_intrinsic_width()`**: The width needed with no line breaks. Corresponds to CSS `max-content`.

These feed directly into Taffy's `AvailableSpace::MinContent` and `AvailableSpace::MaxContent`.

### TextMeasure Context Update

The `TextMeasure` struct used as Taffy node context needs to carry the full style information:

```rust
struct TextMeasure {
    content: String,
    font_collection: FontCollection,  // ref-counted, cheap to clone
    para_style: ParagraphStyle,
    text_style: TextStyle,
}
```

### Caching Paragraph Objects for Measurement

Creating a `Paragraph` for every measure call is expensive. Cache strategy:

1. Cache the built `Paragraph` object in the `TextMeasure` context
2. Only rebuild when text content or style changes
3. Re-layout (cheap) when available width changes
4. Store the last measured paragraph for use during rendering

---

## Performance

### Paragraph Layout Caching

SkParagraph has a built-in `ParagraphCache` (accessed via `FontCollection::paragraph_cache()`):

- Caches layout results keyed by text content + styles + width
- Calling `layout()` with the same width on an unchanged paragraph is nearly free
- Call `mark_dirty()` to force re-layout
- Can be disabled via `ParagraphCache::turn_on(false)` for debugging

### FontCollection Setup

`FontCollection::new()` is cheap, but `set_default_font_manager(FontMgr::default(), None)` triggers fontconfig initialization on first use. Strategy:

1. Create one `FontCollection` at app startup
2. Share it across all `ParagraphBuilder` instances (it's ref-counted)
3. Pre-warm by building and laying out a small paragraph during initialization

### Paragraph Object Lifecycle

- **Build cost**: Moderate (text shaping via HarfBuzz)
- **Layout cost**: Moderate first time, near-free on re-layout with same width
- **Paint cost**: Low (draws pre-shaped glyph runs)

**Reuse strategy:**
1. Keep `Paragraph` objects alive for text that hasn't changed
2. When text changes, rebuild via `ParagraphBuilder`
3. When only width changes, call `layout(new_width)` on existing paragraph
4. For text input: rebuild paragraph on every keystroke (fast enough for < 10KB text)

### When to Rebuild vs Re-Layout

| Change | Action |
|--------|--------|
| Text content changed | Rebuild (new ParagraphBuilder) |
| Style changed (color, size, font) | Rebuild |
| Available width changed | Re-layout only (`paragraph.layout(new_width)`) |
| Nothing changed | Re-use as-is |

### Impact of Complex Scripts

Complex scripts (Arabic, Devanagari, Thai) are slower to shape due to HarfBuzz processing. For typical UI text (< 1000 characters), this is negligible. For large text blocks:

- Consider breaking into per-paragraph Paragraph objects
- Layout off the main thread if needed
- Use `ParagraphCache` to avoid redundant shaping

### Memory Considerations

- Each `Paragraph` holds shaped glyph data, proportional to text length
- `FontCollection` holds font data (shared across paragraphs)
- Clear unused paragraph objects promptly
- For long scrolling text, consider virtualizing (only build/layout visible paragraphs)

---

## Emoji Rendering

### How SkParagraph Handles Emoji

SkParagraph handles emoji through font fallback:

1. When a color emoji codepoint is encountered and the current font lacks it
2. FontCollection falls back to a color emoji font (e.g., Noto Color Emoji)
3. Color emoji fonts use COLR/CPAL or CBDT/CBLC tables
4. Skia renders color glyphs natively

### Emoji Sequences

Multi-codepoint emoji (skin tones, ZWJ sequences, flags) work automatically:
- Family emoji: `U+1F468 U+200D U+1F469 U+200D U+1F467` (man, ZWJ, woman, ZWJ, girl)
- Skin tone: `U+1F44B U+1F3FD` (waving hand + medium skin tone)
- Flags: `U+1F1FA U+1F1F8` (regional indicators U + S = US flag)

HarfBuzz shaping treats ZWJ sequences as single glyph clusters.

### Ensuring Emoji Support

```rust
let mut ts = TextStyle::new();
ts.set_font_families(&[
    "Inter",             // primary text font
    "Noto Color Emoji",  // emoji fallback
]);
```

On Debian/MarsOS, install `fonts-noto-color-emoji` for color emoji support.

---

## Subpixel Text Rendering

### LCD Antialiasing

Skia supports subpixel (LCD) text rendering for sharper text on LCD displays:

```rust
let mut font = Font::from_typeface(typeface, size);
font.set_subpixel(true);        // enable subpixel positioning
font.set_edging(FontEdging::SubpixelAntiAlias); // LCD antialiasing
```

However, on Wayland compositors, subpixel rendering may cause color fringing during compositor-level transforms. Recommendation:

- Use grayscale antialiasing for composited UI elements
- Use subpixel only for static text on opaque backgrounds
- SkParagraph uses the font settings from the resolved `Font` object

### Text Rendering Hints

```rust
// For high-DPI displays, subpixel positioning is important
font.set_subpixel(true);

// Hinting options
font.set_hinting(FontHinting::Slight); // or None, Normal, Full
```

---

## Implementation Order

### Phase 1: FontCollection & Basic Paragraph Rendering (P0)

**Goal:** Replace `draw_str()` with `Paragraph::paint()` for all text rendering.

1. Create a shared `FontCollection` in `SkiaRenderer`:
   ```rust
   pub struct SkiaRenderer {
       font_collection: FontCollection,
       // ... existing fields ...
   }
   ```
   Initialize with `FontMgr::default()` as default font manager.

2. Replace `SkiaRenderer::draw_text()` to use Paragraph:
   - Build `ParagraphStyle` + `TextStyle` from `DrawCommand::Text` fields
   - Create `ParagraphBuilder`, add text, build, layout, paint
   - Initially: only use font_size and color from the command

3. Add `DrawCommand::Text` fields for font_family (optional):
   ```rust
   DrawCommand::Text {
       text: String,
       position: Point,
       font_size: f32,
       color: Color,
       font_family: Option<Vec<String>>,  // new
   }
   ```

4. Update `Element` with `font_family` field and builder method.

5. Remove `renderer::measure_text()` hack.

**Deliverables:** All text renders through SkParagraph. Visual output should be nearly identical to before.

### Phase 2: Accurate Text Measurement for Taffy (P0)

**Goal:** Replace `char_width * 0.6` estimation with real paragraph-based measurement.

1. Update `TextMeasure` struct to carry `FontCollection` and style info.

2. In `layout.rs` measure callback:
   - Build a `Paragraph` from the `TextMeasure` context
   - Use `min_intrinsic_width()` / `max_intrinsic_width()` for intrinsic sizing
   - Use `paragraph.height()` after `layout(width)` for height
   - Cache the paragraph for reuse in rendering

3. Test with various text lengths and verify layout accuracy.

**Deliverables:** Text elements size correctly based on actual glyph measurements.

### Phase 3: Text Style Properties (P0-P1)

**Goal:** Expose core text styling through the Element API.

1. Add `Element` fields:
   - `font_weight: Option<i32>`
   - `font_italic: bool`
   - `line_height: Option<f32>`
   - `text_align: Option<TextAlign>`
   - `max_lines: Option<usize>`
   - `text_overflow_ellipsis: bool`

2. Add builder methods on `Element`:
   - `.bold()`, `.italic()`, `.font_weight(n)`
   - `.line_height(1.5)`, `.text_align(TextAlign::Center)`
   - `.max_lines(3)`, `.ellipsis()`

3. Thread these through `DrawCommand::Text` and the display list builder.

4. Apply in `SkiaRenderer::draw_text()`:
   - Font weight/italic via `TextStyle::set_font_style()`
   - Line height via `TextStyle::set_height()` + `set_height_override(true)`
   - Text align via `ParagraphStyle::set_text_align()`
   - Max lines + ellipsis via `ParagraphStyle`

**Deliverables:** Text can be styled with weight, italic, line height, alignment, and truncation.

### Phase 4: Text Decoration & Shadow (P1)

**Goal:** Support underline, strikethrough, and text shadows.

1. Add `Element` fields and builder methods:
   - `.underline()`, `.strikethrough()`, `.overline()`
   - `.text_decoration_style(TextDecorationStyle::Wavy)`
   - `.text_decoration_color(RED)`
   - `.text_shadow(color, offset, blur)`

2. Apply via `TextStyle` decoration and shadow APIs.

**Deliverables:** Text decoration and shadows render correctly.

### Phase 5: Text Input Upgrade (P1)

**Goal:** Migrate TextInput from character-width estimation to SkParagraph-based cursor positioning.

1. Build a `Paragraph` from the input text during display list generation.

2. Use `get_glyph_position_at_coordinate()` for click-to-cursor.

3. Use `get_rects_for_range()` for selection highlighting.

4. Use paragraph metrics for cursor rectangle computation.

5. Update `TextInputState` to work with byte offsets (SkParagraph uses UTF-8 byte offsets).

6. Fix scroll offset calculation using paragraph measurement.

**Deliverables:** Text input has accurate cursor positioning, selection rendering, and scroll behavior.

### Phase 6: Rich Text (P1)

**Goal:** Support multiple styled runs within a single text element.

1. Add `ElementKind::RichText { spans: Vec<TextSpan> }`.

2. Add `rich_text()` builder and `.span()` method.

3. In the display list builder, emit a new `DrawCommand::RichText` variant.

4. In the renderer, build a `ParagraphBuilder` with `push_style/pop` for each span.

5. Extend Taffy measurement to handle rich text (build paragraph with all spans).

**Deliverables:** Text blocks with mixed bold, italic, colored, and underlined runs.

### Phase 7: Letter/Word Spacing (P2)

**Goal:** Fine-grained spacing control.

1. Add `Element` fields and builder methods for `letter_spacing` and `word_spacing`.
2. Apply via `TextStyle::set_letter_spacing()` and `set_word_spacing()`.

**Deliverables:** Adjustable spacing between characters and words.

### Phase 8: Font Features & Advanced Typography (P2-P3)

**Goal:** Expose OpenType features and variable font axes.

1. Add `font_features: Vec<(String, i32)>` to Element.
2. Add `.font_feature("tnum", 1)` builder method.
3. Apply via `TextStyle::add_font_feature()`.
4. Variable font support via `TextStyle::set_font_arguments()`.

**Deliverables:** Tabular figures, ligature control, and variable font axis support.

### Phase 9: BiDi & Internationalization (P2-P3)

**Goal:** Proper RTL and complex script support.

1. Add `text_direction` to Element for explicit RTL control.
2. Apply via `ParagraphStyle::set_text_direction()`.
3. Add `locale` to Element for proper CJK/Arabic line breaking.
4. Apply via `TextStyle::set_locale()`.
5. Ensure emoji font fallback works (test with Noto Color Emoji).

**Deliverables:** Mixed LTR/RTL text, CJK line breaking, emoji rendering.

### Phase 10: Inline Placeholders (P3)

**Goal:** Support inline non-text elements (icons, images) within text flow.

1. Add placeholder support to rich text spans.
2. Use `ParagraphBuilder::add_placeholder()`.
3. After layout, use `get_rects_for_placeholders()` to position inline elements.
4. Render inline elements at the placeholder positions.

**Deliverables:** Icons and images inline with text, properly wrapped and aligned.

### Phase 11: Performance Optimization (Ongoing)

1. Cache `Paragraph` objects per text element (keyed by content + style hash).
2. Only rebuild on content/style change; re-layout on width change.
3. Measure `FontCollection` initialization cost and pre-warm if needed.
4. Profile paragraph build + layout times for typical UI text.
5. Consider paragraph pooling for frequently changing text (e.g., clock display).

---

## Appendix: Complete Type Reference Quick-Lookup

### Enums

| Type | Values |
|------|--------|
| `TextAlign` | `Left`, `Right`, `Center`, `Justify`, `Start`, `End` |
| `TextDirection` | `LTR`, `RTL` |
| `TextBaseline` | `Alphabetic`, `Ideographic` |
| `TextHeightBehavior` | `All`, `DisableFirstAscent`, `DisableLastDescent`, `DisableAll` |
| `TextDecoration` | `NO_DECORATION`, `UNDERLINE`, `OVERLINE`, `LINE_THROUGH` (bitflags) |
| `TextDecorationStyle` | `Solid`, `Double`, `Dotted`, `Dashed`, `Wavy` |
| `TextDecorationMode` | `Gaps`, `Through` |
| `PlaceholderAlignment` | `Baseline`, `AboveBaseline`, `BelowBaseline`, `Top`, `Bottom`, `Middle` |
| `RectHeightStyle` | `Tight`, `Max`, `IncludeLineSpacingMiddle`, `IncludeLineSpacingTop`, `IncludeLineSpacingBottom`, `Strut` |
| `RectWidthStyle` | `Tight`, `Max` |
| `Affinity` | `Upstream`, `Downstream` |
| `StyleType` | `None`, `AllAttributes`, `Font`, `Foreground`, `Background`, `Shadow`, `Decorations`, `LetterSpacing`, `WordSpacing` |

### Structs

| Type | Key Fields |
|------|------------|
| `TextBox` | `rect: Rect`, `direct: TextDirection` |
| `TextShadow` | `color: Color`, `offset: Point`, `blur_sigma: f64` |
| `Decoration` | `ty`, `mode`, `color`, `style`, `thickness_multiplier` |
| `PlaceholderStyle` | `width`, `height`, `alignment`, `baseline`, `baseline_offset` |
| `PositionWithAffinity` | `position: i32`, `affinity: Affinity` |
| `GlyphClusterInfo` | `bounds: Rect`, `text_range: TextRange`, `position: TextDirection` |
| `GlyphInfo` | `grapheme_layout_bounds`, `grapheme_cluster_text_range`, `text_direction`, `is_ellipsis` |
| `LineMetrics` | `start_index`, `end_index`, `ascent`, `descent`, `height`, `width`, `left`, `baseline`, `line_number` |
| `StyleMetrics` | `text_style: &TextStyle`, `font_metrics: FontMetrics` |
| `FontInfo` | `font: Font`, `text_range: TextRange` |
| `Block` | `range: TextRange`, `style: TextStyle` |

### Type Aliases

| Type | Definition |
|------|-----------|
| `TextIndex` | `usize` |
| `TextRange` | `Range<usize>` |
| `BlockIndex` | `usize` |
| `BlockRange` | `Range<usize>` |
