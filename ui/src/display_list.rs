use crate::animator::Animator;
use crate::color::Color;
use crate::element::{Element, ElementKind, ButtonVariant, ImageSource, ProgressVariant, ShapeData, TextSpan};
use crate::layout::{LayoutNode, Rect};
use crate::style::{
    BlendMode, Border, BorderStyle, CornerRadii, DisplayMode, Filter, FullBorder, Gradient,
    Outline, TextAlign, TextDecorationStyle, Transform,
};

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
        // Text input state
        cursor_byte_offset: Option<usize>,
        selection_byte_range: Option<(usize, usize)>,
        scroll_offset: f32,
    },
    Image {
        source: ImageSource,
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
    /// Focus ring drawn around an element.
    FocusRing {
        bounds: Rect,
        corner_radii: CornerRadii,
    },
    RichText {
        spans: Vec<TextSpan>,
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
) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    emit_commands(layout, root_element, animator, &mut commands);
    commands
}

fn emit_commands(
    node: &LayoutNode,
    element: &Element,
    animator: Option<&Animator>,
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
        emit_children_sorted(node, element, animator, commands);
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
            color: element.color.unwrap_or(crate::color::WHITE),
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
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
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
            color: element.color.unwrap_or(crate::color::WHITE),
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

    // Shape (vector path)
    if let ElementKind::Shape { ref data } = element.kind {
        commands.push(DrawCommand::Path {
            data: data.clone(),
            bounds: bounds.clone(),
        });
    }

    // TextInput
    if let ElementKind::TextInput { ref value, ref placeholder } = element.kind {
        let display_text = if value.is_empty() { placeholder } else { value };
        let text_color = if value.is_empty() {
            let base = element.color.unwrap_or(crate::color::WHITE);
            Color {
                r: base.r,
                g: base.g,
                b: base.b,
                a: (base.a as f32 * 0.4) as u8,
            }
        } else {
            element.color.unwrap_or(crate::color::WHITE)
        };

        let cursor_byte_offset = element.cursor_offset.map(|char_idx| {
            if value.is_empty() {
                0
            } else {
                value.char_indices()
                    .nth(char_idx)
                    .map(|(byte_idx, _)| byte_idx)
                    .unwrap_or(value.len())
            }
        });

        let selection_byte_range = if !value.is_empty() {
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
            text: display_text.clone(),
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
            cursor_byte_offset,
            selection_byte_range,
            scroll_offset: element.scroll_offset,
        });
    }

    // Textarea
    if let ElementKind::Textarea { ref value, ref placeholder } = element.kind {
        let display_text = if value.is_empty() { placeholder } else { value };
        let text_color = if value.is_empty() {
            let base = element.color.unwrap_or(crate::color::WHITE);
            Color {
                r: base.r,
                g: base.g,
                b: base.b,
                a: (base.a as f32 * 0.4) as u8,
            }
        } else {
            element.color.unwrap_or(crate::color::WHITE)
        };

        // Background
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: element.background.unwrap_or(Color { r: 30, g: 30, b: 34, a: 255 }),
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
            ButtonVariant::Primary => Color { r: 66, g: 133, b: 244, a: 255 },
            ButtonVariant::Secondary => Color { r: 0, g: 0, b: 0, a: 0 },
            ButtonVariant::Ghost => Color { r: 0, g: 0, b: 0, a: 0 },
            ButtonVariant::Danger => Color { r: 220, g: 53, b: 69, a: 255 },
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
                    color: Color { r: 66, g: 133, b: 244, a: 255 },
                    width: 1.0,
                }),
                border_style: element.border_style,
            });
        }
        // Label text centered
        let text_color = match variant {
            ButtonVariant::Secondary => Color { r: 66, g: 133, b: 244, a: 255 },
            _ => element.color.unwrap_or(crate::color::WHITE),
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
            cursor_byte_offset: None,
            selection_byte_range: None,
            scroll_offset: 0.0,
        });
    }

    // Checkbox
    if let ElementKind::Checkbox { checked, indeterminate, ref label } = element.kind {
        let box_size = 18.0_f32;
        let box_x = bounds.x;
        let box_y = bounds.y + (bounds.height - box_size) / 2.0;
        let fill_color = if checked || indeterminate {
            Color { r: 66, g: 133, b: 244, a: 255 }
        } else {
            crate::color::TRANSPARENT
        };
        let border_color = if checked || indeterminate {
            Color { r: 66, g: 133, b: 244, a: 255 }
        } else {
            Color { r: 120, g: 120, b: 120, a: 255 }
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
                    stroke: Some((crate::color::WHITE, 2.0)),
                    viewbox: None,
                },
                bounds: Rect { x: box_x, y: box_y, width: box_size, height: box_size },
            });
        } else if indeterminate {
            // Horizontal dash
            commands.push(DrawCommand::Rect {
                bounds: Rect { x: box_x + 4.0, y: box_y + 8.0, width: 10.0, height: 2.0 },
                background: crate::color::WHITE,
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
                color: element.color.unwrap_or(crate::color::WHITE),
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
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
            });
        }
    }

    // Radio button
    if let ElementKind::Radio { selected, ref label, .. } = element.kind {
        let circle_size = 18.0_f32;
        let cx_pos = bounds.x + circle_size / 2.0;
        let cy_pos = bounds.y + bounds.height / 2.0;
        let border_color = if selected {
            Color { r: 66, g: 133, b: 244, a: 255 }
        } else {
            Color { r: 120, g: 120, b: 120, a: 255 }
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
                fill: Some(Color { r: 66, g: 133, b: 244, a: 255 }),
                stroke: None,
            });
        }
        // Label text
        if let Some(ref text) = label {
            commands.push(DrawCommand::Text {
                text: text.clone(),
                position: Point { x: bounds.x + circle_size + 8.0, y: bounds.y },
                font_size: element.font_size,
                color: element.color.unwrap_or(crate::color::WHITE),
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
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
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
            Color { r: 66, g: 133, b: 244, a: 255 }
        } else {
            Color { r: 80, g: 80, b: 80, a: 255 }
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
            fill: Some(crate::color::WHITE),
            stroke: None,
        });
        // Label
        if let Some(ref text) = label {
            commands.push(DrawCommand::Text {
                text: text.clone(),
                position: Point { x: track_x + track_w + 8.0, y: bounds.y },
                font_size: element.font_size,
                color: element.color.unwrap_or(crate::color::WHITE),
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
                cursor_byte_offset: None,
                selection_byte_range: None,
                scroll_offset: 0.0,
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
        let track_fill_color = element.progress_color.unwrap_or(Color { r: 66, g: 133, b: 244, a: 255 });
        let track_empty_color = element.track_color.unwrap_or(Color { r: 80, g: 80, b: 80, a: 255 });

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
            fill: Some(Color { r: 0, g: 0, b: 0, a: 40 }),
            stroke: None,
        });
        // Thumb
        commands.push(DrawCommand::Circle {
            center: Point { x: thumb_x, y: track_y },
            radius: thumb_radius,
            fill: Some(crate::color::WHITE),
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
        let track_fill_color = element.progress_color.unwrap_or(Color { r: 66, g: 133, b: 244, a: 255 });
        let track_empty_color = element.track_color.unwrap_or(Color { r: 80, g: 80, b: 80, a: 255 });

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
            fill: Some(crate::color::WHITE),
            stroke: None,
        });
        // High thumb
        commands.push(DrawCommand::Circle {
            center: Point { x: high_x, y: track_y },
            radius: thumb_radius,
            fill: Some(crate::color::WHITE),
            stroke: None,
        });
    }

    // Progress bar / Spinner
    if let ElementKind::Progress { value, variant } = element.kind {
        let fill_color = element.progress_color.unwrap_or(Color { r: 66, g: 133, b: 244, a: 255 });
        let track_color_val = element.track_color.unwrap_or(Color { r: 80, g: 80, b: 80, a: 255 });
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

    // Focus ring (drawn for any focused, focusable element)
    // This is emitted after the element's own rendering but before children
    // The runtime sets a flag; here we check disabled
    if element.focusable == Some(true) && !element.disabled {
        // Focus ring will be drawn by the runtime when the element is actually focused
        // We emit the FocusRing command data here for potential use
    }

    // Recurse into children, sorted by z-index
    emit_children_sorted(node, element, animator, commands);

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

/// Emit children sorted by z-index: negative z first, then no-z (tree order), then positive z.
fn emit_children_sorted(
    node: &LayoutNode,
    element: &Element,
    animator: Option<&Animator>,
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
            emit_commands(child_layout, child_element, animator, commands);
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
            emit_commands(l, e, animator, commands);
        }
        for (_, l, e) in &normal {
            emit_commands(l, e, animator, commands);
        }
        for (_, _, l, e) in &positive {
            emit_commands(l, e, animator, commands);
        }
    }
}
