use crate::element::{Element, ElementKind};
use crate::style::{Alignment, Direction, Justify};
use skia_safe::textlayout::FontCollection;
use taffy::prelude::*;

#[derive(Debug, Clone)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct LayoutNode {
    pub bounds: Rect,
    pub element_index: usize,
    pub children: Vec<LayoutNode>,
}

#[derive(Debug)]
pub struct ElementRef {
    /// Debug description of the element kind
    pub kind_debug: String,
}

#[derive(Clone)]
struct TextMeasure {
    content: String,
    font_size: f32,
    font_collection: FontCollection,
    font_family: Option<Vec<String>>,
    font_weight: Option<i32>,
    font_italic: bool,
    line_height: Option<f32>,
    letter_spacing: f32,
    word_spacing: f32,
    max_lines: Option<usize>,
    text_overflow_ellipsis: bool,
}

pub fn compute_layout(
    root: &Element,
    available_width: f32,
    available_height: f32,
    font_collection: &FontCollection,
) -> (LayoutNode, Vec<ElementRef>) {
    let mut tree: TaffyTree<TextMeasure> = TaffyTree::new();
    let mut elements: Vec<ElementRef> = Vec::new();

    let root_id = build_taffy_node(&mut tree, root, &mut elements, font_collection);

    tree.compute_layout_with_measure(
        root_id,
        Size {
            width: AvailableSpace::Definite(available_width),
            height: AvailableSpace::Definite(available_height),
        },
        |known_dimensions, available_space, _node_id, node_context, _style| {
            if let Some(ctx) = node_context {
                let available_width = known_dimensions.width.unwrap_or_else(|| {
                    match available_space.width {
                        AvailableSpace::Definite(w) => w,
                        AvailableSpace::MinContent => 0.0,
                        AvailableSpace::MaxContent => f32::MAX,
                    }
                });

                let mut fc = ctx.font_collection.clone();
                let (measured_w, measured_h) = crate::renderer::measure_text_paragraph(
                    &ctx.content,
                    ctx.font_size,
                    &mut fc,
                    ctx.font_family.as_deref(),
                    ctx.font_weight,
                    ctx.font_italic,
                    ctx.line_height,
                    ctx.letter_spacing,
                    ctx.word_spacing,
                    available_width,
                    ctx.max_lines,
                    ctx.text_overflow_ellipsis,
                );

                let width = known_dimensions.width.unwrap_or(measured_w);
                let height = known_dimensions.height.unwrap_or(measured_h);
                Size { width, height }
            } else {
                Size::ZERO
            }
        },
    )
    .unwrap();

    let layout_tree = extract_layout(&tree, root_id, 0.0, 0.0, 0);

    (layout_tree.0, elements)
}

fn build_taffy_node(
    tree: &mut TaffyTree<TextMeasure>,
    element: &Element,
    elements: &mut Vec<ElementRef>,
    font_collection: &FontCollection,
) -> NodeId {
    elements.push(ElementRef {
        kind_debug: format!("{:?}", std::mem::discriminant(&element.kind)),
    });

    // Build children first
    let child_ids: Vec<NodeId> = element
        .children
        .iter()
        .map(|child| build_taffy_node(tree, child, elements, font_collection))
        .collect();

    let style = element_to_taffy_style(element);

    let make_ctx = |content: String, el: &Element| -> TextMeasure {
        TextMeasure {
            content,
            font_size: el.font_size,
            font_collection: font_collection.clone(),
            font_family: el.font_family.clone(),
            font_weight: el.font_weight,
            font_italic: el.font_italic,
            line_height: el.line_height,
            letter_spacing: el.letter_spacing,
            word_spacing: el.word_spacing,
            max_lines: el.max_lines,
            text_overflow_ellipsis: el.text_overflow_ellipsis,
        }
    };

    match &element.kind {
        ElementKind::Text { content } => {
            tree.new_leaf_with_context(style, make_ctx(content.clone(), element))
                .unwrap()
        }
        ElementKind::RichText { spans } => {
            let full_text: String = spans.iter().map(|s| s.content.as_str()).collect();
            tree.new_leaf_with_context(style, make_ctx(full_text, element))
                .unwrap()
        }
        ElementKind::TextInput { value, placeholder } => {
            let display_text = if value.is_empty() { placeholder.clone() } else { value.clone() };
            tree.new_leaf_with_context(style, make_ctx(display_text, element))
                .unwrap()
        }
        _ => {
            tree.new_with_children(style, &child_ids).unwrap()
        }
    }
}

#[allow(clippy::field_reassign_with_default)]
fn element_to_taffy_style(element: &Element) -> Style {
    let mut style = Style::default();

    // Direction
    style.display = Display::Flex;
    style.flex_direction = match element.direction {
        Direction::Row => FlexDirection::Row,
        Direction::Column => FlexDirection::Column,
    };

    // Size
    if let Some(w) = element.width {
        style.size.width = Dimension::Length(w);
    }
    if let Some(h) = element.height {
        style.size.height = Dimension::Length(h);
    }

    // Fill width/height
    if element.fill_width {
        style.size.width = Dimension::Percent(1.0);
        style.flex_grow = 1.0;
    }
    if element.fill_height {
        style.size.height = Dimension::Percent(1.0);
        style.flex_grow = 1.0;
    }

    // Flex grow (for spacer)
    if element.flex_grow > 0.0 {
        style.flex_grow = element.flex_grow;
    }

    // Padding
    style.padding = taffy::geometry::Rect {
        top: LengthPercentage::Length(element.padding[0]),
        right: LengthPercentage::Length(element.padding[1]),
        bottom: LengthPercentage::Length(element.padding[2]),
        left: LengthPercentage::Length(element.padding[3]),
    };

    // Gap
    if element.gap > 0.0 {
        style.gap = Size {
            width: LengthPercentage::Length(element.gap),
            height: LengthPercentage::Length(element.gap),
        };
    }

    // Alignment
    style.align_items = Some(match element.align_items {
        Alignment::Start => AlignItems::FlexStart,
        Alignment::Center => AlignItems::Center,
        Alignment::End => AlignItems::FlexEnd,
    });

    // Justify
    style.justify_content = Some(match element.justify {
        Justify::Start => JustifyContent::FlexStart,
        Justify::Center => JustifyContent::Center,
        Justify::End => JustifyContent::FlexEnd,
        Justify::SpaceBetween => JustifyContent::SpaceBetween,
    });

    // For divider elements, set a fixed size on the cross axis
    if let ElementKind::Divider { thickness } = &element.kind {
        match element.direction {
            Direction::Row => {
                style.size.height = Dimension::Length(*thickness);
                style.size.width = Dimension::Percent(1.0);
            }
            Direction::Column => {
                style.size.width = Dimension::Length(*thickness);
                style.size.height = Dimension::Percent(1.0);
            }
        }
    }

    style
}

fn extract_layout(
    tree: &TaffyTree<TextMeasure>,
    node_id: NodeId,
    parent_x: f32,
    parent_y: f32,
    element_index: usize,
) -> (LayoutNode, usize) {
    let layout = tree.layout(node_id).unwrap();

    let abs_x = parent_x + layout.location.x;
    let abs_y = parent_y + layout.location.y;

    let mut children_nodes = Vec::new();
    let mut current_index = element_index + 1;

    for &child_id in tree.children(node_id).unwrap().iter() {
        let (child_node, next_index) = extract_layout(tree, child_id, abs_x, abs_y, current_index);
        children_nodes.push(child_node);
        current_index = next_index;
    }

    let node = LayoutNode {
        bounds: Rect {
            x: abs_x,
            y: abs_y,
            width: layout.size.width,
            height: layout.size.height,
        },
        element_index,
        children: children_nodes,
    };

    (node, current_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::*;
    use crate::style::*;
    use skia_safe::FontMgr;

    fn test_fc() -> FontCollection {
        let mut fc = FontCollection::new();
        fc.set_default_font_manager(FontMgr::default(), None);
        fc
    }

    #[test]
    fn test_simple_container() {
        let tree = container().width(100.0).height(50.0);
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        assert_eq!(layout.bounds.width, 100.0);
        assert_eq!(layout.bounds.height, 50.0);
    }

    #[test]
    fn test_row_with_children() {
        let tree = row()
            .child(container().width(50.0).height(30.0))
            .child(container().width(70.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        // Row should be wide enough for both children
        assert_eq!(layout.children.len(), 2);
        assert_eq!(layout.children[0].bounds.width, 50.0);
        assert_eq!(layout.children[1].bounds.width, 70.0);
        // Second child should start after first
        assert!(layout.children[1].bounds.x >= 50.0);
    }

    #[test]
    fn test_padding() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .padding(10.0)
            .child(container().fill_width().fill_height());
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert_eq!(child.bounds.x, 10.0);
        assert_eq!(child.bounds.y, 10.0);
        assert!((child.bounds.width - 180.0).abs() < 1.0);
        assert!((child.bounds.height - 80.0).abs() < 1.0);
    }

    #[test]
    fn test_gap() {
        let tree = row()
            .gap(10.0)
            .child(container().width(50.0).height(30.0))
            .child(container().width(50.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        assert_eq!(layout.children[1].bounds.x, 60.0); // 50 + 10 gap
    }

    #[test]
    fn test_spacer() {
        let tree = row()
            .width(200.0)
            .child(container().width(50.0).height(30.0))
            .child(spacer())
            .child(container().width(50.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        // Last child should be at the end
        assert!((layout.children[2].bounds.x - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_text_element() {
        let tree = text("Hello").font_size(16.0);
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn test_nested_layout() {
        let tree = column()
            .padding(10.0)
            .child(
                row()
                    .gap(5.0)
                    .child(container().width(40.0).height(40.0))
                    .child(container().width(40.0).height(40.0)),
            )
            .child(container().width(100.0).height(20.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        assert_eq!(layout.children.len(), 2);
        // First child is a row with 2 children
        assert_eq!(layout.children[0].children.len(), 2);
    }

    #[test]
    fn test_alignment_center() {
        let tree = column()
            .width(200.0)
            .height(100.0)
            .align_items(Alignment::Center)
            .child(container().width(50.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        // Should be centered: (200 - 50) / 2 = 75
        assert!((child.bounds.x - 75.0).abs() < 1.0);
    }
}
