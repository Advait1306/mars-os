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
pub fn build_display_list(layout: &LayoutNode, root_element: &Element) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    emit_commands(layout, root_element, &mut commands);
    commands
}

fn emit_commands(node: &LayoutNode, element: &Element, commands: &mut Vec<DrawCommand>) {
    let mut pop_count = 0;

    // Clip
    if element.clip {
        commands.push(DrawCommand::PushClip {
            bounds: node.bounds.clone(),
            corner_radius: element.corner_radius,
        });
        pop_count += 1;
    }

    // Opacity layer
    if element.opacity < 1.0 {
        commands.push(DrawCommand::PushLayer {
            opacity: element.opacity,
        });
        pop_count += 1;
    }

    // Background
    if let Some(bg) = element.background {
        commands.push(DrawCommand::Rect {
            bounds: node.bounds.clone(),
            background: bg,
            corner_radius: element.corner_radius,
            border: element.border.clone(),
        });
    } else if element.border.is_some() {
        // Border without background
        commands.push(DrawCommand::Rect {
            bounds: node.bounds.clone(),
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
                x: node.bounds.x,
                y: node.bounds.y,
            },
            font_size: element.font_size,
            color: element.color.unwrap_or(crate::color::WHITE),
        });
    }

    // Image
    if let ElementKind::Image { ref source } = element.kind {
        commands.push(DrawCommand::Image {
            source: source.clone(),
            bounds: node.bounds.clone(),
        });
    }

    // Recurse into children
    for (child_layout, child_element) in node.children.iter().zip(element.children.iter()) {
        emit_commands(child_layout, child_element, commands);
    }

    // Pop in reverse order
    for _ in 0..pop_count {
        if element.opacity < 1.0 {
            commands.push(DrawCommand::PopLayer);
        } else {
            commands.push(DrawCommand::PopClip);
        }
    }
}
