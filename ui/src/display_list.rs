use crate::animator::Animator;
use crate::color::Color;
use crate::element::{Element, ElementKind, ButtonVariant, DatePickerVariant, ImageSource, ProgressVariant, RichSpan, ShapeData};
use crate::select_state::SelectOption;
use crate::layout::{LayoutNode, Rect};
use crate::style::{
    Background, BackgroundClip, BlendMode, Border, BorderImage, BorderStyle, CornerRadii,
    DisplayMode, Filter, FullBorder, Gradient, Outline, TextAlign, TextDecorationStyle,
    TextDirection, Transform,
};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect {
        bounds: Rect,
        background: Color,
        corner_radii: CornerRadii,
        border: Option<Border>,
        border_style: BorderStyle,
    },
    PerSideBorder {
        bounds: Rect,
        corner_radii: CornerRadii,
        full_border: FullBorder,
    },
    Outline {
        bounds: Rect,
        corner_radii: CornerRadii,
        outline: Outline,
    },
    Text {
        text: String,
        position: Point,
        font_size: f32,
        color: Color,
        max_width: f32,
        font_family: Option<Vec<String>>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        text_align: Option<TextAlign>,
        max_lines: Option<usize>,
        text_overflow_ellipsis: bool,
        letter_spacing: f32,
        word_spacing: f32,
        underline: bool,
        strikethrough: bool,
        overline: bool,
        text_decoration_style: Option<TextDecorationStyle>,
        text_decoration_color: Option<Color>,
        text_shadow: Vec<(Color, (f32, f32), f64)>,
        font_features: Vec<(String, i32)>,
        font_variations: Vec<(String, f32)>,
        text_direction: Option<TextDirection>,
        locale: Option<String>,
        // Text input state
        cursor_byte_offset: Option<usize>,
        selection_byte_range: Option<(usize, usize)>,
        scroll_offset: f32,
        /// Byte range of preedit (IME composition) text within the display string.
        /// The renderer should draw this range with an underline decoration.
        preedit_byte_range: Option<(usize, usize)>,
    },
    Image {
        source: ImageSource,
        bounds: Rect,
        tint: Option<Color>,
        image_fit: crate::element::ImageFit,
    },
    /// A named icon resolved via IconRegistry at render time.
    Icon {
        name: String,
        bounds: Rect,
        tint: Option<Color>,
        image_fit: crate::element::ImageFit,
    },
    Path {
        data: ShapeData,
        bounds: Rect,
    },
    GradientRect {
        bounds: Rect,
        gradient: Gradient,
        corner_radii: CornerRadii,
    },
    BoxShadow {
        bounds: Rect,
        corner_radii: CornerRadii,
        blur: f32,
        spread: f32,
        color: Color,
        offset: Point,
    },
    InsetBoxShadow {
        bounds: Rect,
        corner_radii: CornerRadii,
        blur: f32,
        spread: f32,
        color: Color,
        offset: Point,
    },
    PushClip {
        bounds: Rect,
        corner_radii: CornerRadii,
    },
    PopClip,
    PushLayer {
        opacity: f32,
    },
    PopLayer,
    PushFilter {
        filters: Vec<Filter>,
    },
    PopFilter,
    ApplyBackdropFilter {
        bounds: Rect,
        corner_radii: CornerRadii,
        filters: Vec<Filter>,
    },
    BackdropBlur {
        bounds: Rect,
        corner_radii: CornerRadii,
        blur_radius: f32,
    },
    PushTranslate {
        offset: Point,
    },
    PopTranslate,
    PushBlendMode {
        mode: BlendMode,
    },
    PopBlendMode,
    PushTransform {
        transforms: Vec<Transform>,
        /// Origin in absolute coordinates (computed from bounds + transform_origin fraction).
        origin: Point,
    },
    PopTransform,
    /// Line segment for cursors, decorations, etc.
    Line {
        from: Point,
        to: Point,
        color: Color,
        width: f32,
    },
    /// Circle (filled or stroked).
    Circle {
        center: Point,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<(Color, f32)>,
    },
    /// Stacked background layers (drawn bottom-to-top).
    BackgroundLayers {
        bounds: Rect,
        layers: Vec<Background>,
        corner_radii: CornerRadii,
        clip: BackgroundClip,
    },
    /// Nine-slice border image.
    BorderImage {
        bounds: Rect,
        image: BorderImage,
    },
    /// Dynamic SVG document.
    SvgDocument {
        document: std::sync::Arc<std::sync::Mutex<crate::svg_render::SvgDocument>>,
        bounds: Rect,
        tint: Option<Color>,
        image_fit: crate::element::ImageFit,
    },
    /// Focus ring drawn around an element.
    FocusRing {
        bounds: Rect,
        corner_radii: CornerRadii,
    },
    RichText {
        spans: Vec<RichSpan>,
        position: Point,
        max_width: f32,
        font_size: f32,
        color: Color,
        font_family: Option<Vec<String>>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        text_align: Option<TextAlign>,
        max_lines: Option<usize>,
        text_overflow_ellipsis: bool,
        letter_spacing: f32,
        word_spacing: f32,
        text_shadow: Vec<(Color, (f32, f32), f64)>,
        font_features: Vec<(String, i32)>,
        font_variations: Vec<(String, f32)>,
        text_direction: Option<TextDirection>,
        locale: Option<String>,
    },
    /// Multiline text area with cursor, selection, and scrolling.
    MultilineText {
        text: String,
        bounds: Rect,
        font_size: f32,
        color: Color,
        font_family: Option<Vec<String>>,
        font_weight: Option<i32>,
        font_italic: bool,
        line_height: Option<f32>,
        letter_spacing: f32,
        word_spacing: f32,
        /// Cursor position as (line, column). None if not focused.
        cursor_pos: Option<(usize, usize)>,
        /// Selection as ((start_line, start_col), (end_line, end_col)). None if no selection.
        selection_range: Option<((usize, usize), (usize, usize))>,
        scroll_offset_y: f32,
        scroll_offset_x: f32,
        show_line_numbers: bool,
    },
}

/// Walk the LayoutNode tree and the corresponding Element tree to emit draw commands.
/// When an `Animator` is provided, animated overrides are applied to keyed elements.
pub fn build_display_list(
    layout: &LayoutNode,
    root_element: &Element,
    animator: Option<&Animator>,
    theme: &Theme,
) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    emit_commands(layout, root_element, animator, theme, &mut commands);
    commands
}

fn emit_commands(
    node: &LayoutNode,
    element: &Element,
    animator: Option<&Animator>,
    theme: &Theme,
    commands: &mut Vec<DrawCommand>,
) {
    // Skip hidden elements
    if element.display == DisplayMode::None {
        return;
    }

    // Resolve effective bounds and opacity, applying animation overrides for keyed elements.
    let overrides = element
        .key
        .as_ref()
        .and_then(|k| animator.map(|a| a.get_overrides(k)));

    let bounds = if let Some(ref ov) = overrides {
        Rect {
            x: ov.layout_x.unwrap_or(node.bounds.x),
            y: ov.layout_y.unwrap_or(node.bounds.y),
            width: ov.layout_w.unwrap_or(node.bounds.width),
            height: ov.layout_h.unwrap_or(node.bounds.height),
        }
    } else {
        node.bounds.clone()
    };

    let effective_opacity = if let Some(ref ov) = overrides {
        ov.opacity.unwrap_or(element.opacity)
    } else {
        element.opacity
    };

    // Track pops needed at the end
    let mut pop_translate = false;
    let mut pop_transform = false;
    let mut pop_blend = false;
    let mut pop_layer = false;
    let mut pop_clip = false;
    let mut pop_filter = false;

    // Animation offset (translate)
    if let Some(ref ov) = overrides {
        if ov.offset_x.abs() > 0.001 || ov.offset_y.abs() > 0.001 {
            commands.push(DrawCommand::PushTranslate {
                offset: Point {
                    x: ov.offset_x,
                    y: ov.offset_y,
                },
            });
            pop_translate = true;
        }
    }

    // Transforms (applied before clip, after animation translate)
    if !element.transforms.is_empty() {
        let origin = Point {
            x: bounds.x + bounds.width * element.transform_origin.0,
            y: bounds.y + bounds.height * element.transform_origin.1,
        };
        commands.push(DrawCommand::PushTransform {
            transforms: element.transforms.clone(),
            origin,
        });
        pop_transform = true;
    }

    // Clip
    if element.clip {
        commands.push(DrawCommand::PushClip {
            bounds: bounds.clone(),
            corner_radii: element.corner_radii,
        });
        pop_clip = true;
    }

    // Opacity layer
    if effective_opacity < 1.0 {
        commands.push(DrawCommand::PushLayer {
            opacity: effective_opacity,
        });
        pop_layer = true;
    }

    // Blend mode
    if element.blend_mode != BlendMode::Normal {
        commands.push(DrawCommand::PushBlendMode {
            mode: element.blend_mode,
        });
        pop_blend = true;
    }

    // CSS filters (wrap entire element content)
    if !element.filters.is_empty() {
        commands.push(DrawCommand::PushFilter {
            filters: element.filters.clone(),
        });
        pop_filter = true;
    }

    // Backdrop filters (applied to content behind this element)
    if !element.backdrop_filters.is_empty() {
        commands.push(DrawCommand::ApplyBackdropFilter {
            bounds: bounds.clone(),
            corner_radii: element.corner_radii,
            filters: element.backdrop_filters.clone(),
        });
    }

    // Skip drawing for invisible elements (but still recurse children)
    if !element.visible {
        emit_children_sorted(node, element, animator, theme, commands);
        // Pop in reverse order
        if pop_filter { commands.push(DrawCommand::PopFilter); }
        if pop_blend { commands.push(DrawCommand::PopBlendMode); }
        if pop_layer { commands.push(DrawCommand::PopLayer); }
        if pop_clip { commands.push(DrawCommand::PopClip); }
        if pop_transform { commands.push(DrawCommand::PopTransform); }
        if pop_translate { commands.push(DrawCommand::PopTranslate); }
        return;
    }

    // Outset box shadows (drawn behind the element)
    for shadow in &element.box_shadows {
        if !shadow.inset {
            commands.push(DrawCommand::BoxShadow {
                bounds: bounds.clone(),
                corner_radii: element.corner_radii,
                blur: shadow.blur,
                spread: shadow.spread,
                color: shadow.color,
                offset: Point {
                    x: shadow.offset_x,
                    y: shadow.offset_y,
                },
            });
        }
    }

    // Background
    if let Some(bg) = element.background {
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: bg,
            corner_radii: element.corner_radii,
            border: element.border.clone(),
            border_style: element.border_style,
        });
    } else if element.border.is_some() {
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: crate::color::TRANSPARENT,
            corner_radii: element.corner_radii,
            border: element.border.clone(),
            border_style: element.border_style,
        });
    }

    // Per-side borders
    if let Some(ref full_border) = element.full_border {
        commands.push(DrawCommand::PerSideBorder {
            bounds: bounds.clone(),
            corner_radii: element.corner_radii,
            full_border: full_border.clone(),
        });
    }

    // Gradient (drawn on top of solid background, under children)
    if let Some(ref gradient) = element.gradient {
        commands.push(DrawCommand::GradientRect {
            bounds: bounds.clone(),
            gradient: gradient.clone(),
            corner_radii: element.corner_radii,
        });
    }

    // Multiple background layers (stacked on top of solid/gradient backgrounds)
    if !element.backgrounds.is_empty() {
        commands.push(DrawCommand::BackgroundLayers {
            bounds: bounds.clone(),
            layers: element.backgrounds.clone(),
            corner_radii: element.corner_radii,
            clip: element.background_clip,
        });
    }

    // Border image (nine-slice, drawn on top of regular borders)
    if let Some(ref bi) = element.border_image {
        commands.push(DrawCommand::BorderImage {
            bounds: bounds.clone(),
            image: bi.clone(),
        });
    }

    // Inset box shadows (drawn inside the element, on top of background)
    for shadow in &element.box_shadows {
        if shadow.inset {
            commands.push(DrawCommand::InsetBoxShadow {
                bounds: bounds.clone(),
                corner_radii: element.corner_radii,
                blur: shadow.blur,
                spread: shadow.spread,
                color: shadow.color,
                offset: Point {
                    x: shadow.offset_x,
                    y: shadow.offset_y,
                },
            });
        }
    }

    // Text
    if let ElementKind::Text { ref content } = element.kind {
        commands.push(DrawCommand::Text {
            text: content.clone(),
            position: Point {
                x: bounds.x,
                y: bounds.y,
            },
            font_size: element.font_size,
            color: element.color.unwrap_or(theme.text),
            max_width: bounds.width,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            text_align: element.text_align,
            max_lines: element.max_lines,
            text_overflow_ellipsis: element.text_overflow_ellipsis,
            letter_spacing: element.letter_spacing,
            word_spacing: element.word_spacing,
            underline: element.underline,
            strikethrough: element.strikethrough,
            overline: element.overline,
            text_decoration_style: element.text_decoration_style,
            text_decoration_color: element.text_decoration_color,
            text_shadow: element.text_shadow.clone(),
            font_features: element.font_features.clone(),
            font_variations: element.font_variations.clone(),
            text_direction: element.text_direction,
            locale: element.locale.clone(),
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
            preedit_byte_range: None,
        });
    }

    // Rich Text
    if let ElementKind::RichText { ref spans } = element.kind {
        commands.push(DrawCommand::RichText {
            spans: spans.clone(),
            position: Point {
                x: bounds.x,
                y: bounds.y,
            },
            max_width: bounds.width,
            font_size: element.font_size,
            color: element.color.unwrap_or(theme.text),
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            text_align: element.text_align,
            max_lines: element.max_lines,
            text_overflow_ellipsis: element.text_overflow_ellipsis,
            letter_spacing: element.letter_spacing,
            word_spacing: element.word_spacing,
            text_shadow: element.text_shadow.clone(),
            font_features: element.font_features.clone(),
            font_variations: element.font_variations.clone(),
            text_direction: element.text_direction,
            locale: element.locale.clone(),
        });
    }

    // Image
    if let ElementKind::Image { ref source } = element.kind {
        commands.push(DrawCommand::Image {
            source: source.clone(),
            bounds: bounds.clone(),
            tint: element.tint,
            image_fit: element.image_fit,
        });
    }

    // Icon (named, resolved at render time via IconRegistry)
    if let ElementKind::Icon { ref name } = element.kind {
        commands.push(DrawCommand::Icon {
            name: name.clone(),
            bounds: bounds.clone(),
            tint: element.tint,
            image_fit: element.image_fit,
        });
    }

    // SVG Document (dynamic)
    if let ElementKind::SvgDocument { ref document } = element.kind {
        commands.push(DrawCommand::SvgDocument {
            document: document.clone(),
            bounds: bounds.clone(),
            tint: element.tint,
            image_fit: element.image_fit,
        });
    }

    // Shape (vector path)
    if let ElementKind::Shape { ref data } = element.kind {
        commands.push(DrawCommand::Path {
            data: data.clone(),
            bounds: bounds.clone(),
        });
    }

    // TextInput
    if let ElementKind::TextInput { ref value, ref placeholder } = element.kind {
        // Build display text, potentially with preedit text inserted at cursor
        let has_preedit = element.preedit_text.as_ref().map_or(false, |t| !t.is_empty());
        let (display_text, preedit_byte_range) = if has_preedit {
            let preedit = element.preedit_text.as_ref().unwrap();
            let cursor_char = element.cursor_offset.unwrap_or(value.chars().count());
            let insert_byte = value.char_indices()
                .nth(cursor_char)
                .map(|(i, _)| i)
                .unwrap_or(value.len());
            let mut composed = String::with_capacity(value.len() + preedit.len());
            composed.push_str(&value[..insert_byte]);
            let preedit_start = composed.len();
            composed.push_str(preedit);
            let preedit_end = composed.len();
            composed.push_str(&value[insert_byte..]);
            (composed, Some((preedit_start, preedit_end)))
        } else {
            (if value.is_empty() { placeholder.clone() } else { value.clone() }, None)
        };

        let is_placeholder = value.is_empty() && !has_preedit;
        let text_color = if is_placeholder {
            let base = element.color.unwrap_or(theme.text);
            Color {
                r: base.r,
                g: base.g,
                b: base.b,
                a: (base.a as f32 * 0.4) as u8,
            }
        } else {
            element.color.unwrap_or(theme.text)
        };

        let cursor_byte_offset = if has_preedit {
            // Position cursor within the preedit text
            let (preedit_start, _) = preedit_byte_range.unwrap();
            let preedit = element.preedit_text.as_ref().unwrap();
            let preedit_cursor_char = element.preedit_cursor.unwrap_or(preedit.chars().count());
            let offset_in_preedit = preedit.char_indices()
                .nth(preedit_cursor_char)
                .map(|(i, _)| i)
                .unwrap_or(preedit.len());
            Some(preedit_start + offset_in_preedit)
        } else {
            element.cursor_offset.map(|char_idx| {
                if value.is_empty() {
                    0
                } else {
                    value.char_indices()
                        .nth(char_idx)
                        .map(|(byte_idx, _)| byte_idx)
                        .unwrap_or(value.len())
                }
            })
        };

        let selection_byte_range = if !value.is_empty() && !has_preedit {
            element.selection_range.map(|(start_char, end_char)| {
                let start_byte = value.char_indices()
                    .nth(start_char)
                    .map(|(i, _)| i)
                    .unwrap_or(value.len());
                let end_byte = value.char_indices()
                    .nth(end_char)
                    .map(|(i, _)| i)
                    .unwrap_or(value.len());
                (start_byte, end_byte)
            })
        } else {
            None
        };

        commands.push(DrawCommand::Text {
            text: display_text,
            position: Point {
                x: bounds.x,
                y: bounds.y,
            },
            font_size: element.font_size,
            color: text_color,
            max_width: bounds.width,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            text_align: element.text_align,
            max_lines: Some(1),
            text_overflow_ellipsis: false,
            letter_spacing: element.letter_spacing,
            word_spacing: element.word_spacing,
            underline: element.underline,
            strikethrough: element.strikethrough,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            text_shadow: element.text_shadow.clone(),
            font_features: element.font_features.clone(),
            font_variations: element.font_variations.clone(),
            cursor_byte_offset,
            selection_byte_range,
            scroll_offset: element.scroll_offset,
            preedit_byte_range,
        });
    }

    // Textarea
    if let ElementKind::Textarea { ref value, ref placeholder } = element.kind {
        let display_text = if value.is_empty() { placeholder } else { value };
        let text_color = if value.is_empty() {
            let base = element.color.unwrap_or(theme.text);
            Color {
                r: base.r,
                g: base.g,
                b: base.b,
                a: (base.a as f32 * 0.4) as u8,
            }
        } else {
            element.color.unwrap_or(theme.text)
        };

        // Background
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: element.background.unwrap_or(theme.input_bg),
            corner_radii: element.corner_radii,
            border: element.border.clone(),
            border_style: element.border_style,
        });

        // cursor_pos and selection come from element properties set by the view
        let cursor_pos = element.cursor_offset.map(|_| {
            // The view sets scroll_offset_y/x on the element; cursor pos as (line, col)
            // is encoded via cursor_offset (flat char position) and decoded here
            let text = if value.is_empty() { "" } else { value.as_str() };
            if let Some(char_idx) = element.cursor_offset {
                crate::textarea_state::TextareaState::byte_offset_to_pos(
                    text,
                    crate::text_input_state::char_to_byte_pos(text, char_idx),
                )
            } else {
                (0, 0)
            }
        });

        let selection_range = if !value.is_empty() {
            element.selection_range.map(|(start_char, end_char)| {
                let start_byte = crate::text_input_state::char_to_byte_pos(value, start_char);
                let end_byte = crate::text_input_state::char_to_byte_pos(value, end_char);
                let start_pos = crate::textarea_state::TextareaState::byte_offset_to_pos(value, start_byte);
                let end_pos = crate::textarea_state::TextareaState::byte_offset_to_pos(value, end_byte);
                (start_pos, end_pos)
            })
        } else {
            None
        };

        commands.push(DrawCommand::MultilineText {
            text: display_text.clone(),
            bounds: bounds.clone(),
            font_size: element.font_size,
            color: text_color,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            letter_spacing: element.letter_spacing,
            word_spacing: element.word_spacing,
            cursor_pos,
            selection_range,
            scroll_offset_y: element.scroll_offset_y,
            scroll_offset_x: element.scroll_offset_x,
            show_line_numbers: element.show_line_numbers,
        });
    }

    // Button
    if let ElementKind::Button { ref label, variant } = element.kind {
        // Background color based on variant
        let bg = match variant {
            ButtonVariant::Primary => theme.primary,
            ButtonVariant::Secondary => crate::color::TRANSPARENT,
            ButtonVariant::Ghost => crate::color::TRANSPARENT,
            ButtonVariant::Danger => theme.danger,
        };
        if bg.a > 0 {
            commands.push(DrawCommand::Rect {
                bounds: bounds.clone(),
                background: bg,
                corner_radii: element.corner_radii,
                border: element.border.clone(),
                border_style: element.border_style,
            });
        }
        // Secondary variant gets a border
        if variant == ButtonVariant::Secondary && element.border.is_none() {
            commands.push(DrawCommand::Rect {
                bounds: bounds.clone(),
                background: crate::color::TRANSPARENT,
                corner_radii: element.corner_radii,
                border: Some(crate::style::Border {
                    color: theme.primary,
                    width: 1.0,
                }),
                border_style: element.border_style,
            });
        }
        // Label text centered
        let text_color = match variant {
            ButtonVariant::Secondary => theme.primary,
            _ => element.color.unwrap_or(theme.text),
        };
        commands.push(DrawCommand::Text {
            text: label.clone(),
            position: Point { x: bounds.x, y: bounds.y },
            font_size: element.font_size,
            color: text_color,
            max_width: bounds.width,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight.or(Some(500)),
            font_italic: false,
            line_height: Some(bounds.height),
            text_align: Some(TextAlign::Center),
            max_lines: Some(1),
            text_overflow_ellipsis: true,
            letter_spacing: element.letter_spacing,
            word_spacing: 0.0,
            underline: false,
            strikethrough: false,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            text_shadow: Vec::new(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            text_direction: None,
            locale: None,
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
            preedit_byte_range: None,
        });
    }

    // Checkbox
    if let ElementKind::Checkbox { checked, indeterminate, ref label } = element.kind {
        let box_size = 18.0_f32;
        let box_x = bounds.x;
        let box_y = bounds.y + (bounds.height - box_size) / 2.0;
        let fill_color = if checked || indeterminate {
            theme.check_fill
        } else {
            crate::color::TRANSPARENT
        };
        let border_color = if checked || indeterminate {
            theme.check_fill
        } else {
            theme.check_border
        };
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: box_x, y: box_y, width: box_size, height: box_size },
            background: fill_color,
            corner_radii: CornerRadii::uniform(3.0),
            border: Some(crate::style::Border { color: border_color, width: 1.0 }),
            border_style: BorderStyle::Solid,
        });
        if checked {
            // Checkmark path
            commands.push(DrawCommand::Path {
                data: ShapeData {
                    path_data: format!(
                        "M {} {} L {} {} L {} {}",
                        box_x + 4.0, box_y + 9.0,
                        box_x + 7.5, box_y + 12.5,
                        box_x + 13.0, box_y + 5.5
                    ),
                    fill: None,
                    stroke: Some((theme.check_mark, 2.0)),
                    viewbox: None,
                },
                bounds: Rect { x: box_x, y: box_y, width: box_size, height: box_size },
            });
        } else if indeterminate {
            // Horizontal dash
            commands.push(DrawCommand::Rect {
                bounds: Rect { x: box_x + 4.0, y: box_y + 8.0, width: 10.0, height: 2.0 },
                background: theme.check_mark,
                corner_radii: CornerRadii::ZERO,
                border: None,
                border_style: BorderStyle::Solid,
            });
        }
        // Label text
        if let Some(ref text) = label {
            commands.push(DrawCommand::Text {
                text: text.clone(),
                position: Point { x: box_x + box_size + 8.0, y: bounds.y },
                font_size: element.font_size,
                color: element.color.unwrap_or(theme.text),
                max_width: bounds.width - box_size - 8.0,
                font_family: element.font_family.clone(),
                font_weight: element.font_weight,
                font_italic: false,
                line_height: Some(bounds.height),
                text_align: None,
                max_lines: Some(1),
                text_overflow_ellipsis: true,
                letter_spacing: element.letter_spacing,
                word_spacing: 0.0,
                underline: false,
                strikethrough: false,
                overline: false,
                text_decoration_style: None,
                text_decoration_color: None,
                text_shadow: Vec::new(),
                font_features: Vec::new(),
                font_variations: Vec::new(),
                text_direction: None,
                locale: None,
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
                preedit_byte_range: None,
            });
        }
    }

    // Radio button
    if let ElementKind::Radio { selected, ref label, .. } = element.kind {
        let circle_size = 18.0_f32;
        let cx_pos = bounds.x + circle_size / 2.0;
        let cy_pos = bounds.y + bounds.height / 2.0;
        let border_color = if selected {
            theme.check_fill
        } else {
            theme.check_border
        };
        let border_width = if selected { 2.0 } else { 1.0 };
        // Outer circle
        commands.push(DrawCommand::Circle {
            center: Point { x: cx_pos, y: cy_pos },
            radius: 9.0,
            fill: None,
            stroke: Some((border_color, border_width)),
        });
        // Inner circle (selected)
        if selected {
            commands.push(DrawCommand::Circle {
                center: Point { x: cx_pos, y: cy_pos },
                radius: 4.0,
                fill: Some(theme.check_fill),
                stroke: None,
            });
        }
        // Label text
        if let Some(ref text) = label {
            commands.push(DrawCommand::Text {
                text: text.clone(),
                position: Point { x: bounds.x + circle_size + 8.0, y: bounds.y },
                font_size: element.font_size,
                color: element.color.unwrap_or(theme.text),
                max_width: bounds.width - circle_size - 8.0,
                font_family: element.font_family.clone(),
                font_weight: element.font_weight,
                font_italic: false,
                line_height: Some(bounds.height),
                text_align: None,
                max_lines: Some(1),
                text_overflow_ellipsis: true,
                letter_spacing: element.letter_spacing,
                word_spacing: 0.0,
                underline: false,
                strikethrough: false,
                overline: false,
                text_decoration_style: None,
                text_decoration_color: None,
                text_shadow: Vec::new(),
                font_features: Vec::new(),
                font_variations: Vec::new(),
                text_direction: None,
                locale: None,
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
                preedit_byte_range: None,
            });
        }
    }

    // Switch / Toggle
    if let ElementKind::Switch { on, ref label } = element.kind {
        let track_w = 44.0_f32;
        let track_h = 24.0_f32;
        let track_x = bounds.x;
        let track_y = bounds.y + (bounds.height - track_h) / 2.0;
        let track_color = if on {
            theme.switch_track_on
        } else {
            theme.switch_track_off
        };
        // Track (pill shape)
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: track_x, y: track_y, width: track_w, height: track_h },
            background: track_color,
            corner_radii: CornerRadii::uniform(track_h / 2.0),
            border: None,
            border_style: BorderStyle::Solid,
        });
        // Thumb (circle)
        let thumb_x = if on { track_x + 22.0 } else { track_x + 2.0 };
        commands.push(DrawCommand::Circle {
            center: Point { x: thumb_x + 10.0, y: track_y + 12.0 },
            radius: 10.0,
            fill: Some(theme.switch_thumb),
            stroke: None,
        });
        // Label
        if let Some(ref text) = label {
            commands.push(DrawCommand::Text {
                text: text.clone(),
                position: Point { x: track_x + track_w + 8.0, y: bounds.y },
                font_size: element.font_size,
                color: element.color.unwrap_or(theme.text),
                max_width: bounds.width - track_w - 8.0,
                font_family: element.font_family.clone(),
                font_weight: element.font_weight,
                font_italic: false,
                line_height: Some(bounds.height),
                text_align: None,
                max_lines: Some(1),
                text_overflow_ellipsis: true,
                letter_spacing: element.letter_spacing,
                word_spacing: 0.0,
                underline: false,
                strikethrough: false,
                overline: false,
                text_decoration_style: None,
                text_decoration_color: None,
                text_shadow: Vec::new(),
                font_features: Vec::new(),
                font_variations: Vec::new(),
                text_direction: None,
                locale: None,
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
                preedit_byte_range: None,
            });
        }
    }

    // Slider
    if let ElementKind::Slider { value, min, max, .. } = element.kind {
        let track_y = bounds.y + bounds.height / 2.0;
        let track_h = 4.0_f32;
        let ratio = if (max - min).abs() > f64::EPSILON {
            ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let thumb_x = bounds.x + ratio * bounds.width;
        let thumb_radius = 8.0_f32;
        let track_fill_color = element.progress_color.unwrap_or(theme.slider_track_fill);
        let track_empty_color = element.track_color.unwrap_or(theme.slider_track);

        // Track (empty)
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: bounds.x, y: track_y - track_h / 2.0, width: bounds.width, height: track_h },
            background: track_empty_color,
            corner_radii: CornerRadii::uniform(track_h / 2.0),
            border: None,
            border_style: BorderStyle::Solid,
        });
        // Track (filled)
        let fill_w = (thumb_x - bounds.x).max(track_h);
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: bounds.x, y: track_y - track_h / 2.0, width: fill_w, height: track_h },
            background: track_fill_color,
            corner_radii: CornerRadii::uniform(track_h / 2.0),
            border: None,
            border_style: BorderStyle::Solid,
        });
        // Thumb shadow
        commands.push(DrawCommand::Circle {
            center: Point { x: thumb_x, y: track_y },
            radius: thumb_radius + 1.0,
            fill: Some(Color::new(0, 0, 0, 40)),
            stroke: None,
        });
        // Thumb
        commands.push(DrawCommand::Circle {
            center: Point { x: thumb_x, y: track_y },
            radius: thumb_radius,
            fill: Some(theme.slider_thumb),
            stroke: None,
        });
    }

    // Range Slider
    if let ElementKind::RangeSlider { low, high, min, max, .. } = element.kind {
        let track_y = bounds.y + bounds.height / 2.0;
        let track_h = 4.0_f32;
        let range = (max - min).max(f64::EPSILON);
        let low_ratio = ((low - min) / range).clamp(0.0, 1.0) as f32;
        let high_ratio = ((high - min) / range).clamp(0.0, 1.0) as f32;
        let low_x = bounds.x + low_ratio * bounds.width;
        let high_x = bounds.x + high_ratio * bounds.width;
        let thumb_radius = 8.0_f32;
        let track_fill_color = element.progress_color.unwrap_or(theme.slider_track_fill);
        let track_empty_color = element.track_color.unwrap_or(theme.slider_track);

        // Track (empty, full width)
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: bounds.x, y: track_y - track_h / 2.0, width: bounds.width, height: track_h },
            background: track_empty_color,
            corner_radii: CornerRadii::uniform(track_h / 2.0),
            border: None,
            border_style: BorderStyle::Solid,
        });
        // Track (filled, between thumbs)
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: low_x, y: track_y - track_h / 2.0, width: (high_x - low_x).max(0.0), height: track_h },
            background: track_fill_color,
            corner_radii: CornerRadii::uniform(track_h / 2.0),
            border: None,
            border_style: BorderStyle::Solid,
        });
        // Low thumb
        commands.push(DrawCommand::Circle {
            center: Point { x: low_x, y: track_y },
            radius: thumb_radius,
            fill: Some(theme.slider_thumb),
            stroke: None,
        });
        // High thumb
        commands.push(DrawCommand::Circle {
            center: Point { x: high_x, y: track_y },
            radius: thumb_radius,
            fill: Some(theme.slider_thumb),
            stroke: None,
        });
    }

    // Progress bar / Spinner
    if let ElementKind::Progress { value, variant } = element.kind {
        let fill_color = element.progress_color.unwrap_or(theme.progress_fill);
        let track_color_val = element.track_color.unwrap_or(theme.progress_track);
        match variant {
            ProgressVariant::Bar => {
                let radius = bounds.height / 2.0;
                // Track
                commands.push(DrawCommand::Rect {
                    bounds: bounds.clone(),
                    background: track_color_val,
                    corner_radii: CornerRadii::uniform(radius),
                    border: None,
                    border_style: BorderStyle::Solid,
                });
                // Fill
                if let Some(v) = value {
                    let fill_width = (v as f32 * bounds.width).max(bounds.height);
                    commands.push(DrawCommand::Rect {
                        bounds: Rect { x: bounds.x, y: bounds.y, width: fill_width, height: bounds.height },
                        background: fill_color,
                        corner_radii: CornerRadii::uniform(radius),
                        border: None,
                        border_style: BorderStyle::Solid,
                    });
                } else {
                    // Indeterminate: draw a 30% width segment at a fixed position
                    // (animation would be handled by the runtime)
                    let seg_w = bounds.width * 0.3;
                    commands.push(DrawCommand::Rect {
                        bounds: Rect { x: bounds.x, y: bounds.y, width: seg_w, height: bounds.height },
                        background: fill_color,
                        corner_radii: CornerRadii::uniform(radius),
                        border: None,
                        border_style: BorderStyle::Solid,
                    });
                }
            }
            ProgressVariant::Circular => {
                // Spinner: draw track circle + arc
                let cx_pos = bounds.x + bounds.width / 2.0;
                let cy_pos = bounds.y + bounds.height / 2.0;
                let radius = (bounds.width.min(bounds.height) / 2.0) - 2.0;
                // Track circle
                commands.push(DrawCommand::Circle {
                    center: Point { x: cx_pos, y: cy_pos },
                    radius,
                    fill: None,
                    stroke: Some((track_color_val, 2.0)),
                });
                // Arc (spinner uses a 270-degree arc; without animation, draw as a partial path)
                // For now emit the arc as a path command
                commands.push(DrawCommand::Circle {
                    center: Point { x: cx_pos, y: cy_pos },
                    radius,
                    fill: None,
                    stroke: Some((fill_color, 2.0)),
                });
            }
        }
    }

    // Select / Dropdown
    if let ElementKind::Select { ref options, selected, ref placeholder } = element.kind {
        emit_select(element, &bounds, options, selected, placeholder, theme, commands);
    }

    // Color picker (closed state: swatch showing current color)
    if let ElementKind::ColorPicker { value } = element.kind {
        // Swatch: rounded square showing the current color
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: value,
            corner_radii: CornerRadii::uniform(6.0),
            border: Some(Border {
                color: if value.is_dark() {
                    crate::color::rgba(255, 255, 255, 60)
                } else {
                    crate::color::rgba(0, 0, 0, 40)
                },
                width: 1.0,
            }),
            border_style: BorderStyle::Solid,
        });
    }

    // Date/time picker (closed state: input showing formatted value)
    if let ElementKind::DatePicker { ref value, variant } = element.kind {
        let bg = element.background.unwrap_or(theme.input_bg);
        let border_color = if element.error.is_some() { theme.danger } else { theme.input_border };

        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: bg,
            corner_radii: CornerRadii::uniform(6.0),
            border: Some(Border { color: border_color, width: 1.0 }),
            border_style: BorderStyle::Solid,
        });

        let display_text = value.as_deref().unwrap_or(match variant {
            DatePickerVariant::Date => "YYYY-MM-DD",
            DatePickerVariant::Time => "HH:MM",
            DatePickerVariant::DateTime => "YYYY-MM-DD HH:MM",
        });
        let text_color = if value.is_some() { theme.text } else { theme.text_placeholder };

        commands.push(DrawCommand::Text {
            text: display_text.to_string(),
            position: Point { x: bounds.x + 12.0, y: bounds.y + 8.0 },
            font_size: element.font_size,
            color: text_color,
            max_width: bounds.width - 40.0,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            text_align: None,
            max_lines: Some(1),
            text_overflow_ellipsis: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            underline: false,
            strikethrough: false,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            text_shadow: Vec::new(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            text_direction: None,
            locale: None,
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
            preedit_byte_range: None,
        });

        // Calendar/clock icon (small indicator on the right)
        let icon_x = bounds.x + bounds.width - 28.0;
        let icon_cy = bounds.y + bounds.height / 2.0;
        let icon_path = match variant {
            DatePickerVariant::Date | DatePickerVariant::DateTime => {
                // Simple calendar icon
                format!(
                    "M {} {} h 12 v 12 h -12 Z M {} {} h 12 M {} {} v 12 M {} {} v 12",
                    icon_x, icon_cy - 6.0,
                    icon_x, icon_cy - 2.0,
                    icon_x + 4.0, icon_cy - 6.0,
                    icon_x + 8.0, icon_cy - 6.0
                )
            }
            DatePickerVariant::Time => {
                // Simple clock icon (circle + hands)
                format!(
                    "M {} {} m 6 0 a 6 6 0 1 0 -12 0 a 6 6 0 1 0 12 0 M {} {} v -4 M {} {} h 3",
                    icon_x, icon_cy,
                    icon_x, icon_cy,
                    icon_x, icon_cy,
                )
            }
        };
        commands.push(DrawCommand::Path {
            data: ShapeData {
                path_data: icon_path,
                fill: None,
                stroke: Some((theme.text_secondary, 1.5)),
                viewbox: None,
            },
            bounds: Rect { x: icon_x - 2.0, y: icon_cy - 8.0, width: 16.0, height: 16.0 },
        });
    }

    // File input (button + filename label)
    if let ElementKind::FileInput { ref files, ref accept, multiple } = element.kind {
        let bg = element.background.unwrap_or(theme.primary);

        // Button area
        let button_w = 100.0_f32.min(bounds.width * 0.4);
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: bounds.x, y: bounds.y, width: button_w, height: bounds.height },
            background: bg,
            corner_radii: CornerRadii {
                top_left: 6.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 6.0,
            },
            border: Some(Border { color: theme.input_border, width: 1.0 }),
            border_style: BorderStyle::Solid,
        });
        commands.push(DrawCommand::Text {
            text: if *multiple { "Choose files" } else { "Choose file" }.to_string(),
            position: Point { x: bounds.x + 8.0, y: bounds.y + 8.0 },
            font_size: element.font_size,
            color: crate::color::WHITE,
            max_width: button_w - 16.0,
            font_family: element.font_family.clone(),
            font_weight: Some(500),
            font_italic: false,
            line_height: element.line_height,
            text_align: None,
            max_lines: Some(1),
            text_overflow_ellipsis: true,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            underline: false,
            strikethrough: false,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            text_shadow: Vec::new(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            text_direction: None,
            locale: None,
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
            preedit_byte_range: None,
        });

        // File name label area
        let label_x = bounds.x + button_w;
        let label_w = bounds.width - button_w;
        commands.push(DrawCommand::Rect {
            bounds: Rect { x: label_x, y: bounds.y, width: label_w, height: bounds.height },
            background: theme.input_bg,
            corner_radii: CornerRadii {
                top_left: 0.0, top_right: 6.0, bottom_right: 6.0, bottom_left: 0.0,
            },
            border: Some(Border { color: theme.input_border, width: 1.0 }),
            border_style: BorderStyle::Solid,
        });
        let label_text = if files.is_empty() {
            "No file chosen".to_string()
        } else if files.len() == 1 {
            // Show just the filename, not the full path
            files[0].rsplit('/').next().unwrap_or(&files[0]).to_string()
        } else {
            format!("{} files", files.len())
        };
        commands.push(DrawCommand::Text {
            text: label_text,
            position: Point { x: label_x + 8.0, y: bounds.y + 8.0 },
            font_size: element.font_size,
            color: if files.is_empty() { theme.text_placeholder } else { theme.text },
            max_width: label_w - 16.0,
            font_family: element.font_family.clone(),
            font_weight: element.font_weight,
            font_italic: element.font_italic,
            line_height: element.line_height,
            text_align: None,
            max_lines: Some(1),
            text_overflow_ellipsis: true,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            underline: false,
            strikethrough: false,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            text_shadow: Vec::new(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            text_direction: None,
            locale: None,
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
            preedit_byte_range: None,
        });
    }

    // Focus ring (drawn for any focused, focusable element)
    // This is emitted after the element's own rendering but before children
    // The runtime sets a flag; here we check disabled
    if element.focusable == Some(true) && !element.disabled {
        // Focus ring will be drawn by the runtime when the element is actually focused
        // We emit the FocusRing command data here for potential use
    }

    // Recurse into children, sorted by z-index
    emit_children_sorted(node, element, animator, theme, commands);

    // Outline (drawn on top of children, outside the element box)
    if let Some(ref outline) = element.outline {
        commands.push(DrawCommand::Outline {
            bounds: bounds.clone(),
            corner_radii: element.corner_radii,
            outline: outline.clone(),
        });
    }

    // Pop in reverse order of push
    if pop_filter {
        commands.push(DrawCommand::PopFilter);
    }
    if pop_blend {
        commands.push(DrawCommand::PopBlendMode);
    }
    if pop_layer {
        commands.push(DrawCommand::PopLayer);
    }
    if pop_clip {
        commands.push(DrawCommand::PopClip);
    }
    if pop_transform {
        commands.push(DrawCommand::PopTransform);
    }
    if pop_translate {
        commands.push(DrawCommand::PopTranslate);
    }
}

/// Emit draw commands for a Select/Dropdown element.
fn emit_select(
    element: &Element,
    bounds: &Rect,
    options: &[SelectOption],
    selected: Option<usize>,
    placeholder: &str,
    theme: &Theme,
    commands: &mut Vec<DrawCommand>,
) {
    let border_color = if element.select_open {
        theme.input_border_focus
    } else {
        theme.input_border
    };
    let bg_color = element.background.unwrap_or(theme.input_bg);
    let radii = if element.corner_radii.is_zero() {
        CornerRadii::uniform(6.0)
    } else {
        element.corner_radii
    };

    // Trigger background
    commands.push(DrawCommand::Rect {
        bounds: bounds.clone(),
        background: bg_color,
        corner_radii: radii,
        border: Some(Border { color: border_color, width: 1.0 }),
        border_style: BorderStyle::Solid,
    });

    // Selected text or placeholder
    let h_padding = 12.0_f32;
    let chevron_space = 24.0_f32;
    let text_max_w = (bounds.width - h_padding * 2.0 - chevron_space).max(0.0);
    let (display_text, text_color) = if let Some(idx) = selected {
        if idx < options.len() {
            (
                options[idx].label.clone(),
                element.color.unwrap_or(theme.text),
            )
        } else {
            (
                placeholder.to_string(),
                theme.text_placeholder,
            )
        }
    } else {
        (
            placeholder.to_string(),
            theme.text_placeholder,
        )
    };

    commands.push(DrawCommand::Text {
        text: display_text,
        position: Point { x: bounds.x + h_padding, y: bounds.y },
        font_size: element.font_size,
        color: text_color,
        max_width: text_max_w,
        font_family: element.font_family.clone(),
        font_weight: element.font_weight,
        font_italic: false,
        line_height: Some(bounds.height),
        text_align: None,
        max_lines: Some(1),
        text_overflow_ellipsis: true,
        letter_spacing: element.letter_spacing,
        word_spacing: 0.0,
        underline: false,
        strikethrough: false,
        overline: false,
        text_decoration_style: None,
        text_decoration_color: None,
        text_shadow: Vec::new(),
        font_features: Vec::new(),
        font_variations: Vec::new(),
        text_direction: None,
        locale: None,
        cursor_byte_offset: None,
        selection_byte_range: None,
        scroll_offset: 0.0,
        preedit_byte_range: None,
    });

    // Chevron (downward arrow)
    let chevron_x = bounds.x + bounds.width - chevron_space;
    let chevron_cy = bounds.y + bounds.height / 2.0;
    let chevron_size = 4.0_f32;
    commands.push(DrawCommand::Path {
        data: ShapeData {
            path_data: format!(
                "M {} {} L {} {} L {} {}",
                chevron_x, chevron_cy - chevron_size,
                chevron_x + chevron_size * 1.5, chevron_cy + chevron_size * 0.5,
                chevron_x + chevron_size * 3.0, chevron_cy - chevron_size,
            ),
            fill: None,
            stroke: Some((theme.text_placeholder, 1.5)),
            viewbox: None,
        },
        bounds: Rect {
            x: chevron_x,
            y: chevron_cy - chevron_size,
            width: chevron_size * 3.0,
            height: chevron_size * 2.0,
        },
    });

    // Dropdown list (only when open)
    if element.select_open {
        let item_height = 32.0_f32;
        let dropdown_gap = 4.0_f32;
        let visible_count = options.len().min(element.select_max_visible);
        let dropdown_h = visible_count as f32 * item_height + 8.0; // 4px top + 4px bottom padding
        let dropdown_y = bounds.y + bounds.height + dropdown_gap;
        let dropdown_bounds = Rect {
            x: bounds.x,
            y: dropdown_y,
            width: bounds.width,
            height: dropdown_h,
        };

        // Dropdown shadow
        commands.push(DrawCommand::BoxShadow {
            bounds: dropdown_bounds.clone(),
            corner_radii: radii,
            blur: 8.0,
            spread: 0.0,
            color: theme.popup_shadow,
            offset: Point { x: 0.0, y: 2.0 },
        });

        // Dropdown background
        commands.push(DrawCommand::Rect {
            bounds: dropdown_bounds.clone(),
            background: theme.popup_bg,
            corner_radii: radii,
            border: Some(Border {
                color: theme.popup_border,
                width: 1.0,
            }),
            border_style: BorderStyle::Solid,
        });

        // Clip dropdown content
        commands.push(DrawCommand::PushClip {
            bounds: dropdown_bounds.clone(),
            corner_radii: radii,
        });

        // Render visible options
        let scroll_offset = element.select_scroll_offset;
        let content_y = dropdown_y + 4.0; // top padding

        for (i, opt) in options.iter().enumerate() {
            let item_y = content_y + i as f32 * item_height - scroll_offset;

            // Skip items outside viewport
            if item_y + item_height < dropdown_y || item_y > dropdown_y + dropdown_h {
                continue;
            }

            // Highlight background
            let is_highlighted = element.select_highlighted == Some(i);
            let is_selected = selected == Some(i);
            if is_highlighted {
                commands.push(DrawCommand::Rect {
                    bounds: Rect {
                        x: bounds.x + 4.0,
                        y: item_y,
                        width: bounds.width - 8.0,
                        height: item_height,
                    },
                    background: theme.option_hover_bg,
                    corner_radii: CornerRadii::uniform(4.0),
                    border: None,
                    border_style: BorderStyle::Solid,
                });
            }

            // Option text
            let opt_color = if opt.disabled {
                theme.text_disabled
            } else if is_selected {
                theme.primary
            } else {
                element.color.unwrap_or(theme.text)
            };

            commands.push(DrawCommand::Text {
                text: opt.label.clone(),
                position: Point { x: bounds.x + h_padding, y: item_y },
                font_size: element.font_size,
                color: opt_color,
                max_width: text_max_w,
                font_family: element.font_family.clone(),
                font_weight: if is_selected { Some(500) } else { element.font_weight },
                font_italic: false,
                line_height: Some(item_height),
                text_align: None,
                max_lines: Some(1),
                text_overflow_ellipsis: true,
                letter_spacing: element.letter_spacing,
                word_spacing: 0.0,
                underline: false,
                strikethrough: false,
                overline: false,
                text_decoration_style: None,
                text_decoration_color: None,
                text_shadow: Vec::new(),
                font_features: Vec::new(),
                font_variations: Vec::new(),
                text_direction: None,
                locale: None,
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
                preedit_byte_range: None,
            });

            // Checkmark for selected option
            if is_selected {
                let check_x = bounds.x + bounds.width - 28.0;
                let check_cy = item_y + item_height / 2.0;
                commands.push(DrawCommand::Path {
                    data: ShapeData {
                        path_data: format!(
                            "M {} {} L {} {} L {} {}",
                            check_x, check_cy,
                            check_x + 3.0, check_cy + 3.0,
                            check_x + 8.0, check_cy - 4.0
                        ),
                        fill: None,
                        stroke: Some((theme.primary, 1.5)),
                        viewbox: None,
                    },
                    bounds: Rect {
                        x: check_x,
                        y: check_cy - 4.0,
                        width: 8.0,
                        height: 7.0,
                    },
                });
            }
        }

        commands.push(DrawCommand::PopClip);
    }
}

/// Emit children sorted by z-index: negative z first, then no-z (tree order), then positive z.
fn emit_children_sorted(
    node: &LayoutNode,
    element: &Element,
    animator: Option<&Animator>,
    theme: &Theme,
    commands: &mut Vec<DrawCommand>,
) {
    let children: Vec<(usize, &LayoutNode, &Element)> = node
        .children
        .iter()
        .zip(element.children.iter())
        .enumerate()
        .map(|(i, (l, e))| (i, l, e))
        .collect();

    if children.is_empty() {
        return;
    }

    // Check if any children have z-index set
    let has_z_index = children.iter().any(|(_, _, e)| e.z_index.is_some());

    if !has_z_index {
        // Fast path: no z-index, emit in tree order
        for (_, child_layout, child_element) in &children {
            emit_commands(child_layout, child_element, animator, theme, commands);
        }
    } else {
        // Partition into: negative z, no z (tree order), positive z
        let mut negative: Vec<(i32, usize, &LayoutNode, &Element)> = Vec::new();
        let mut normal: Vec<(usize, &LayoutNode, &Element)> = Vec::new();
        let mut positive: Vec<(i32, usize, &LayoutNode, &Element)> = Vec::new();

        for (i, l, e) in &children {
            match e.z_index {
                Some(z) if z < 0 => negative.push((z, *i, l, e)),
                Some(z) if z > 0 => positive.push((z, *i, l, e)),
                _ => normal.push((*i, l, e)),
            }
        }

        // Sort by z-index, stable within same z
        negative.sort_by_key(|(z, i, _, _)| (*z, *i));
        positive.sort_by_key(|(z, i, _, _)| (*z, *i));

        for (_, _, l, e) in &negative {
            emit_commands(l, e, animator, theme, commands);
        }
        for (_, l, e) in &normal {
            emit_commands(l, e, animator, theme, commands);
        }
        for (_, _, l, e) in &positive {
            emit_commands(l, e, animator, theme, commands);
        }
    }
}
