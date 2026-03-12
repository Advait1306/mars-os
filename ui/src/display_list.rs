use crate::animator::Animator;
use crate::color::Color;
use crate::element::{Element, ElementKind, ImageSource};
use crate::layout::{LayoutNode, Rect};
use crate::style::Border;

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
        corner_radius: f32,
        border: Option<Border>,
    },
    Text {
        text: String,
        position: Point,
        font_size: f32,
        color: Color,
    },
    Image {
        source: ImageSource,
        bounds: Rect,
    },
    BoxShadow {
        bounds: Rect,
        corner_radius: f32,
        blur: f32,
        spread: f32,
        color: Color,
        offset: Point,
    },
    PushClip {
        bounds: Rect,
        corner_radius: f32,
    },
    PopClip,
    PushLayer {
        opacity: f32,
    },
    PopLayer,
    BackdropBlur {
        bounds: Rect,
        corner_radius: f32,
        blur_radius: f32,
    },
    PushTranslate {
        offset: Point,
    },
    PopTranslate,
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
    let mut pop_layer = false;
    let mut pop_clip = false;

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

    // Clip
    if element.clip {
        commands.push(DrawCommand::PushClip {
            bounds: bounds.clone(),
            corner_radius: element.corner_radius,
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

    // Background
    if let Some(bg) = element.background {
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: bg,
            corner_radius: element.corner_radius,
            border: element.border.clone(),
        });
    } else if element.border.is_some() {
        // Border without background
        commands.push(DrawCommand::Rect {
            bounds: bounds.clone(),
            background: crate::color::TRANSPARENT,
            corner_radius: element.corner_radius,
            border: element.border.clone(),
        });
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
        });
    }

    // Image
    if let ElementKind::Image { ref source } = element.kind {
        commands.push(DrawCommand::Image {
            source: source.clone(),
            bounds: bounds.clone(),
        });
    }

    // Recurse into children
    for (child_layout, child_element) in node.children.iter().zip(element.children.iter()) {
        emit_commands(child_layout, child_element, animator, commands);
    }

    // Pop in reverse order of push
    if pop_layer {
        commands.push(DrawCommand::PopLayer);
    }
    if pop_clip {
        commands.push(DrawCommand::PopClip);
    }
    if pop_translate {
        commands.push(DrawCommand::PopTranslate);
    }
}
