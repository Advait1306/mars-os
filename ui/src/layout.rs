use crate::element::{Element, ElementKind};
use crate::style::{
    AlignContent, Alignment, Dim, Direction, DisplayMode, FlexWrap, GridAutoFlow, GridPlacement,
    Justify, Overflow, PositionType, TrackMax, TrackMin, TrackSize,
};
use skia_safe::textlayout::FontCollection;
use taffy::prelude::*;
use taffy::{
    GridTemplateComponent, MinMax, RepetitionCount,
    MinTrackSizingFunction, MaxTrackSizingFunction,
};

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
            let full_text: String = spans.iter().filter_map(|s| match s {
                crate::element::RichSpan::Text(ts) => Some(ts.content.as_str()),
                _ => None,
            }).collect();
            tree.new_leaf_with_context(style, make_ctx(full_text, element))
                .unwrap()
        }
        ElementKind::TextInput { value, placeholder } => {
            let display_text = if value.is_empty() { placeholder.clone() } else { value.clone() };
            tree.new_leaf_with_context(style, make_ctx(display_text, element))
                .unwrap()
        }
        ElementKind::Textarea { value, placeholder } => {
            let display_text = if value.is_empty() { placeholder.clone() } else { value.clone() };
            tree.new_leaf_with_context(style, make_ctx(display_text, element))
                .unwrap()
        }
        ElementKind::Select { options, selected, placeholder } => {
            let display_text = selected
                .and_then(|i| options.get(i))
                .map(|o| o.label.clone())
                .unwrap_or_else(|| placeholder.clone());
            tree.new_leaf_with_context(style, make_ctx(display_text, element))
                .unwrap()
        }
        _ => {
            tree.new_with_children(style, &child_ids).unwrap()
        }
    }
}

// === Dimension conversion helpers ===

fn dim_to_taffy(d: &Dim) -> Dimension {
    match d {
        Dim::Auto => Dimension::auto(),
        Dim::Px(v) => Dimension::length(*v),
        Dim::Pct(v) => Dimension::percent(*v),
    }
}

#[allow(dead_code)]
fn dim_to_length_percentage_auto(d: &Dim) -> LengthPercentageAuto {
    match d {
        Dim::Auto => LengthPercentageAuto::auto(),
        Dim::Px(v) => LengthPercentageAuto::length(*v),
        Dim::Pct(v) => LengthPercentageAuto::percent(*v),
    }
}

fn map_alignment(a: &Alignment) -> AlignItems {
    match a {
        Alignment::Start => AlignItems::FlexStart,
        Alignment::Center => AlignItems::Center,
        Alignment::End => AlignItems::FlexEnd,
        Alignment::Stretch => AlignItems::Stretch,
        Alignment::Baseline => AlignItems::Baseline,
    }
}

fn map_align_self(a: &Alignment) -> AlignSelf {
    match a {
        Alignment::Start => AlignSelf::FlexStart,
        Alignment::Center => AlignSelf::Center,
        Alignment::End => AlignSelf::FlexEnd,
        Alignment::Stretch => AlignSelf::Stretch,
        Alignment::Baseline => AlignSelf::Baseline,
    }
}

fn map_justify(j: &Justify) -> JustifyContent {
    match j {
        Justify::Start => JustifyContent::FlexStart,
        Justify::Center => JustifyContent::Center,
        Justify::End => JustifyContent::FlexEnd,
        Justify::SpaceBetween => JustifyContent::SpaceBetween,
        Justify::SpaceAround => JustifyContent::SpaceAround,
        Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
    }
}

fn map_align_content(a: &AlignContent) -> taffy::AlignContent {
    match a {
        AlignContent::Start => taffy::AlignContent::FlexStart,
        AlignContent::Center => taffy::AlignContent::Center,
        AlignContent::End => taffy::AlignContent::FlexEnd,
        AlignContent::Stretch => taffy::AlignContent::Stretch,
        AlignContent::SpaceBetween => taffy::AlignContent::SpaceBetween,
        AlignContent::SpaceAround => taffy::AlignContent::SpaceAround,
        AlignContent::SpaceEvenly => taffy::AlignContent::SpaceEvenly,
    }
}

fn map_overflow(o: &Overflow) -> taffy::Overflow {
    match o {
        Overflow::Visible => taffy::Overflow::Visible,
        Overflow::Hidden => taffy::Overflow::Hidden,
        Overflow::Clip => taffy::Overflow::Clip,
        Overflow::Scroll => taffy::Overflow::Scroll,
    }
}

fn map_grid_placement(gp: &GridPlacement) -> taffy::GridPlacement {
    match gp {
        GridPlacement::Auto => taffy::GridPlacement::AUTO,
        GridPlacement::Line(n) => taffy::GridPlacement::from_line_index(*n),
        GridPlacement::Span(n) => taffy::GridPlacement::from_span(*n),
    }
}

fn map_track_min(tm: &TrackMin) -> MinTrackSizingFunction {
    match tm {
        TrackMin::Px(v) => MinTrackSizingFunction::from(LengthPercentage::length(*v)),
        TrackMin::Pct(v) => MinTrackSizingFunction::from(LengthPercentage::percent(*v)),
        TrackMin::Auto => MinTrackSizingFunction::auto(),
        TrackMin::MinContent => MinTrackSizingFunction::min_content(),
        TrackMin::MaxContent => MinTrackSizingFunction::max_content(),
    }
}

fn map_track_max(tm: &TrackMax) -> MaxTrackSizingFunction {
    match tm {
        TrackMax::Px(v) => MaxTrackSizingFunction::from(LengthPercentage::length(*v)),
        TrackMax::Pct(v) => MaxTrackSizingFunction::from(LengthPercentage::percent(*v)),
        TrackMax::Fr(v) => MaxTrackSizingFunction::fr(*v),
        TrackMax::Auto => MaxTrackSizingFunction::auto(),
        TrackMax::MinContent => MaxTrackSizingFunction::min_content(),
        TrackMax::MaxContent => MaxTrackSizingFunction::max_content(),
    }
}

fn make_single_track(min: MinTrackSizingFunction, max: MaxTrackSizingFunction) -> GridTemplateComponent<String> {
    GridTemplateComponent::Single(MinMax { min, max })
}

fn map_track_size(ts: &TrackSize) -> GridTemplateComponent<String> {
    match ts {
        TrackSize::Px(v) => {
            let lp = LengthPercentage::length(*v);
            make_single_track(lp.into(), lp.into())
        }
        TrackSize::Pct(v) => {
            let lp = LengthPercentage::percent(*v);
            make_single_track(lp.into(), lp.into())
        }
        TrackSize::Fr(v) => {
            make_single_track(MinTrackSizingFunction::auto(), MaxTrackSizingFunction::fr(*v))
        }
        TrackSize::Auto => {
            make_single_track(MinTrackSizingFunction::auto(), MaxTrackSizingFunction::auto())
        }
        TrackSize::MinContent => {
            make_single_track(MinTrackSizingFunction::min_content(), MaxTrackSizingFunction::min_content())
        }
        TrackSize::MaxContent => {
            make_single_track(MinTrackSizingFunction::max_content(), MaxTrackSizingFunction::max_content())
        }
        TrackSize::MinMax(min, max) => {
            make_single_track(map_track_min(min), map_track_max(max))
        }
        TrackSize::Repeat(count, tracks) => {
            let inner: Vec<_> = tracks.iter().map(|t| map_track_to_non_repeated(t)).collect();
            taffy::style_helpers::repeat(RepetitionCount::Count(*count), inner)
        }
        TrackSize::AutoFill(tracks) => {
            let inner: Vec<_> = tracks.iter().map(|t| map_track_to_non_repeated(t)).collect();
            taffy::style_helpers::repeat(RepetitionCount::AutoFill, inner)
        }
        TrackSize::AutoFit(tracks) => {
            let inner: Vec<_> = tracks.iter().map(|t| map_track_to_non_repeated(t)).collect();
            taffy::style_helpers::repeat(RepetitionCount::AutoFit, inner)
        }
    }
}

fn map_track_to_non_repeated(ts: &TrackSize) -> MinMax<MinTrackSizingFunction, MaxTrackSizingFunction> {
    match ts {
        TrackSize::Px(v) => {
            let lp = LengthPercentage::length(*v);
            MinMax { min: lp.into(), max: lp.into() }
        }
        TrackSize::Pct(v) => {
            let lp = LengthPercentage::percent(*v);
            MinMax { min: lp.into(), max: lp.into() }
        }
        TrackSize::Fr(v) => MinMax { min: MinTrackSizingFunction::auto(), max: MaxTrackSizingFunction::fr(*v) },
        TrackSize::Auto => MinMax { min: MinTrackSizingFunction::auto(), max: MaxTrackSizingFunction::auto() },
        TrackSize::MinContent => MinMax { min: MinTrackSizingFunction::min_content(), max: MaxTrackSizingFunction::min_content() },
        TrackSize::MaxContent => MinMax { min: MinTrackSizingFunction::max_content(), max: MaxTrackSizingFunction::max_content() },
        TrackSize::MinMax(min, max) => MinMax { min: map_track_min(min), max: map_track_max(max) },
        _ => MinMax { min: MinTrackSizingFunction::auto(), max: MaxTrackSizingFunction::auto() },
    }
}

// === Main style conversion ===

#[allow(clippy::field_reassign_with_default)]
fn element_to_taffy_style(element: &Element) -> Style {
    let mut style = Style::default();

    // Display mode
    style.display = match element.display {
        DisplayMode::Flex => Display::Flex,
        DisplayMode::Grid => Display::Grid,
        DisplayMode::None => Display::None,
    };

    // Box sizing: default to border-box (modern CSS best practice)
    style.box_sizing = BoxSizing::BorderBox;

    // Direction
    style.flex_direction = match element.direction {
        Direction::Row => FlexDirection::Row,
        Direction::RowReverse => FlexDirection::RowReverse,
        Direction::Column => FlexDirection::Column,
        Direction::ColumnReverse => FlexDirection::ColumnReverse,
    };

    // Flex wrap
    style.flex_wrap = match element.flex_wrap {
        FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        FlexWrap::Wrap => taffy::FlexWrap::Wrap,
        FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    };

    // Size
    if let Some(w) = &element.width {
        style.size.width = dim_to_taffy(w);
    }
    if let Some(h) = &element.height {
        style.size.height = dim_to_taffy(h);
    }

    // Fill width/height (backward compat)
    if element.fill_width {
        style.size.width = Dimension::percent(1.0);
        style.flex_grow = 1.0;
    }
    if element.fill_height {
        style.size.height = Dimension::percent(1.0);
        style.flex_grow = 1.0;
    }

    // Min/max size
    if let Some(d) = &element.min_width {
        style.min_size.width = dim_to_taffy(d);
    }
    if let Some(d) = &element.min_height {
        style.min_size.height = dim_to_taffy(d);
    }
    if let Some(d) = &element.max_width {
        style.max_size.width = dim_to_taffy(d);
    }
    if let Some(d) = &element.max_height {
        style.max_size.height = dim_to_taffy(d);
    }

    // Aspect ratio
    style.aspect_ratio = element.aspect_ratio;

    // Flex grow (for spacer and explicit)
    if element.flex_grow > 0.0 {
        style.flex_grow = element.flex_grow;
    }

    // Flex shrink
    if let Some(s) = element.flex_shrink {
        style.flex_shrink = s;
    }

    // Flex basis
    if let Some(b) = &element.flex_basis {
        style.flex_basis = dim_to_taffy(b);
    }

    // Padding
    style.padding = taffy::geometry::Rect {
        top: LengthPercentage::length(element.padding[0]),
        right: LengthPercentage::length(element.padding[1]),
        bottom: LengthPercentage::length(element.padding[2]),
        left: LengthPercentage::length(element.padding[3]),
    };

    // Margin
    let margin_val = |i: usize| -> LengthPercentageAuto {
        if element.margin_auto[i] {
            LengthPercentageAuto::auto()
        } else if element.margin[i] != 0.0 {
            LengthPercentageAuto::length(element.margin[i])
        } else {
            LengthPercentageAuto::length(0.0)
        }
    };
    style.margin = taffy::geometry::Rect {
        top: margin_val(0),
        right: margin_val(1),
        bottom: margin_val(2),
        left: margin_val(3),
    };

    // Border (layout contribution)
    if let Some(border) = &element.border {
        let bw = LengthPercentage::length(border.width);
        style.border = taffy::geometry::Rect {
            top: bw,
            right: bw,
            bottom: bw,
            left: bw,
        };
    }

    // Gap
    let row_gap = element.row_gap.unwrap_or(element.gap);
    let col_gap = element.column_gap.unwrap_or(element.gap);
    if row_gap > 0.0 || col_gap > 0.0 {
        style.gap = Size {
            width: LengthPercentage::length(col_gap),
            height: LengthPercentage::length(row_gap),
        };
    }

    // Alignment
    style.align_items = Some(map_alignment(&element.align_items));

    // Align self
    if let Some(a) = &element.align_self {
        style.align_self = Some(map_align_self(a));
    }

    // Align content
    if let Some(a) = &element.align_content {
        style.align_content = Some(map_align_content(a));
    }

    // Justify content
    style.justify_content = Some(map_justify(&element.justify));

    // Justify items (grid)
    if let Some(a) = &element.justify_items {
        style.justify_items = Some(map_alignment(a));
    }

    // Justify self (grid)
    if let Some(a) = &element.justify_self {
        style.justify_self = Some(map_align_self(a));
    }

    // Position
    style.position = match element.position {
        PositionType::Relative => Position::Relative,
        PositionType::Absolute => Position::Absolute,
    };

    // Inset
    let inset_val = |v: Option<f32>| -> LengthPercentageAuto {
        match v {
            Some(px) => LengthPercentageAuto::length(px),
            None => LengthPercentageAuto::auto(),
        }
    };
    style.inset = taffy::geometry::Rect {
        top: inset_val(element.inset[0]),
        right: inset_val(element.inset[1]),
        bottom: inset_val(element.inset[2]),
        left: inset_val(element.inset[3]),
    };

    // Overflow
    style.overflow = taffy::geometry::Point {
        x: map_overflow(&element.overflow_x),
        y: map_overflow(&element.overflow_y),
    };

    // Grid template
    if !element.grid_template_columns.is_empty() {
        style.grid_template_columns = element.grid_template_columns.iter().map(map_track_size).collect();
    }
    if !element.grid_template_rows.is_empty() {
        style.grid_template_rows = element.grid_template_rows.iter().map(map_track_size).collect();
    }

    // Grid auto sizing
    if !element.grid_auto_columns.is_empty() {
        style.grid_auto_columns = element.grid_auto_columns.iter().map(map_track_to_non_repeated).collect();
    }
    if !element.grid_auto_rows.is_empty() {
        style.grid_auto_rows = element.grid_auto_rows.iter().map(map_track_to_non_repeated).collect();
    }

    // Grid auto flow
    style.grid_auto_flow = match element.grid_auto_flow {
        GridAutoFlow::Row => taffy::GridAutoFlow::Row,
        GridAutoFlow::Column => taffy::GridAutoFlow::Column,
        GridAutoFlow::RowDense => taffy::GridAutoFlow::RowDense,
        GridAutoFlow::ColumnDense => taffy::GridAutoFlow::ColumnDense,
    };

    // Grid placement
    if let Some((start, end)) = &element.grid_column {
        style.grid_column = taffy::geometry::Line {
            start: map_grid_placement(start),
            end: map_grid_placement(end),
        };
    }
    if let Some((start, end)) = &element.grid_row {
        style.grid_row = taffy::geometry::Line {
            start: map_grid_placement(start),
            end: map_grid_placement(end),
        };
    }

    // For divider elements, set a fixed size on the cross axis
    if let ElementKind::Divider { thickness } = &element.kind {
        match element.direction {
            Direction::Row | Direction::RowReverse => {
                style.size.height = Dimension::length(*thickness);
                style.size.width = Dimension::percent(1.0);
            }
            Direction::Column | Direction::ColumnReverse => {
                style.size.width = Dimension::length(*thickness);
                style.size.height = Dimension::percent(1.0);
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
        assert_eq!(layout.children.len(), 2);
        assert_eq!(layout.children[0].bounds.width, 50.0);
        assert_eq!(layout.children[1].bounds.width, 70.0);
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
        assert_eq!(layout.children[1].bounds.x, 60.0);
    }

    #[test]
    fn test_spacer() {
        let tree = row()
            .width(200.0)
            .child(container().width(50.0).height(30.0))
            .child(spacer())
            .child(container().width(50.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
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
        assert!((child.bounds.x - 75.0).abs() < 1.0);
    }

    #[test]
    fn test_margin() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .child(container().width(50.0).height(30.0).margin(10.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert!((child.bounds.x - 10.0).abs() < 1.0);
        assert!((child.bounds.y - 10.0).abs() < 1.0);
    }

    #[test]
    fn test_min_max_size() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .child(container().fill_width().height(30.0).max_width(80.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert!(child.bounds.width <= 80.0);
    }

    #[test]
    fn test_display_none() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .child(container().width(50.0).height(30.0).hidden());
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert_eq!(child.bounds.width, 0.0);
        assert_eq!(child.bounds.height, 0.0);
    }

    #[test]
    fn test_flex_wrap() {
        let tree = row()
            .width(100.0)
            .wrap()
            .child(container().width(60.0).height(30.0))
            .child(container().width(60.0).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        // Second child should wrap to a new line
        assert!(layout.children[1].bounds.y >= 30.0);
    }

    #[test]
    fn test_absolute_position() {
        let tree = container()
            .width(200.0)
            .height(200.0)
            .child(
                container()
                    .width(50.0)
                    .height(50.0)
                    .position_absolute()
                    .top(10.0)
                    .left(10.0),
            );
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert!((child.bounds.x - 10.0).abs() < 1.0);
        assert!((child.bounds.y - 10.0).abs() < 1.0);
    }

    #[test]
    fn test_percentage_width() {
        let tree = container()
            .width(200.0)
            .height(100.0)
            .child(container().width_pct(0.5).height(30.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert!((child.bounds.width - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_aspect_ratio() {
        let tree = container()
            .width(200.0)
            .height(200.0)
            .child(container().width(100.0).aspect_ratio(2.0));
        let (layout, _) = compute_layout(&tree, 800.0, 600.0, &test_fc());
        let child = &layout.children[0];
        assert!((child.bounds.height - 50.0).abs() < 1.0);
    }
}
