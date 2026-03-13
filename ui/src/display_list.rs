use crate::animator::Animator;
use crate::color::Color;
use crate::element::{Element, ElementKind, ImageSource, TextSpan};
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
