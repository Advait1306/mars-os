use crate::animation::{Animation, From, To};
use crate::color::Color;
use crate::input::CursorStyle;
use crate::style::{
    AlignContent, Alignment, Border, Dim, Direction, DisplayMode, FlexWrap, GridAutoFlow,
    GridPlacement, Justify, Overflow, PositionType, TextAlign, TextDecorationStyle, TrackSize,
};

pub enum ElementKind {
    Container,
    Text { content: String },
    RichText { spans: Vec<TextSpan> },
    Image { source: ImageSource },
    Spacer,
    Divider { thickness: f32 },
    TextInput { value: String, placeholder: String },
}

#[derive(Debug, Clone)]
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
    pub background: Option<Color>,
    pub text_decoration_color: Option<Color>,
    pub text_decoration_style: Option<TextDecorationStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
    Both,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    Svg(String),
    File(String),
}

pub struct Element {
    pub kind: ElementKind,
    pub key: Option<String>,
    pub direction: Direction,
    pub children: Vec<Element>,

    // Layout props
    pub width: Option<Dim>,
    pub height: Option<Dim>,
    pub fill_width: bool,
    pub fill_height: bool,
    pub padding: [f32; 4], // top, right, bottom, left
    pub gap: f32,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub align_items: Alignment,
    pub justify: Justify,
    pub flex_grow: f32,

    // Phase 1: Box model
    pub margin: [f32; 4],       // top, right, bottom, left
    pub margin_auto: [bool; 4], // per-side auto margin
    pub min_width: Option<Dim>,
    pub min_height: Option<Dim>,
    pub max_width: Option<Dim>,
    pub max_height: Option<Dim>,
    pub aspect_ratio: Option<f32>,
    pub display: DisplayMode,

    // Phase 2: Flexbox
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dim>,
    pub flex_wrap: FlexWrap,
    pub align_self: Option<Alignment>,
    pub align_content: Option<AlignContent>,

    // Phase 3: Positioning
    pub position: PositionType,
    pub inset: [Option<f32>; 4], // top, right, bottom, left
    pub z_index: Option<i32>,

    // Phase 4: Overflow
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Phase 5: Grid
    pub grid_template_columns: Vec<TrackSize>,
    pub grid_template_rows: Vec<TrackSize>,
    pub grid_column: Option<(GridPlacement, GridPlacement)>,
    pub grid_row: Option<(GridPlacement, GridPlacement)>,
    pub grid_auto_flow: GridAutoFlow,
    pub grid_auto_columns: Vec<TrackSize>,
    pub grid_auto_rows: Vec<TrackSize>,
    pub justify_items: Option<Alignment>,
    pub justify_self: Option<Alignment>,

    // Visual props
    pub background: Option<Color>,
    pub corner_radius: f32,
    pub border: Option<Border>,
    pub opacity: f32,
    pub clip: bool,

    // Text-specific
    pub font_size: f32,
    pub color: Option<Color>,
    pub font_family: Option<Vec<String>>,
    pub font_weight: Option<i32>,
    pub font_italic: bool,
    pub line_height: Option<f32>,
    pub letter_spacing: f32,
    pub text_align: Option<TextAlign>,
    pub max_lines: Option<usize>,
    pub text_overflow_ellipsis: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub text_decoration_style: Option<TextDecorationStyle>,
    pub text_decoration_color: Option<Color>,
    pub word_spacing: f32,
    pub text_shadow: Vec<(Color, (f32, f32), f64)>,

    // Text input cursor state (set by the view when the input is focused)
    pub cursor_offset: Option<usize>,
    pub selection_range: Option<(usize, usize)>,
    pub scroll_offset: f32,

    // Animation
    pub animate: Option<Animation>,
    pub animate_layout: bool,
    pub layout_animation: Option<Animation>,
    pub initial: Option<From>,
    pub exit: Option<To>,

    // Scroll
    pub scroll_direction: Option<ScrollDirection>,

    // Event handlers
    pub on_click: Option<Box<dyn Fn()>>,
    pub on_hover: Option<Box<dyn Fn(bool)>>,
    pub on_drag: Option<Box<dyn Fn(f32, f32)>>,
    pub on_scroll: Option<Box<dyn Fn(f32, f32)>>,
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn()>>,
    pub cursor: Option<CursorStyle>,
}

impl Default for Element {
    fn default() -> Self {
        Self {
            kind: ElementKind::Container,
            key: None,
            direction: Direction::Column,
            children: Vec::new(),
            width: None,
            height: None,
            fill_width: false,
            fill_height: false,
            padding: [0.0; 4],
            gap: 0.0,
            row_gap: None,
            column_gap: None,
            align_items: Alignment::Start,
            justify: Justify::Start,
            flex_grow: 0.0,
            // Phase 1
            margin: [0.0; 4],
            margin_auto: [false; 4],
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            aspect_ratio: None,
            display: DisplayMode::Flex,
            // Phase 2
            flex_shrink: None,
            flex_basis: None,
            flex_wrap: FlexWrap::NoWrap,
            align_self: None,
            align_content: None,
            // Phase 3
            position: PositionType::Relative,
            inset: [None; 4],
            z_index: None,
            // Phase 4
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            // Phase 5
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_column: None,
            grid_row: None,
            grid_auto_flow: GridAutoFlow::Row,
            grid_auto_columns: Vec::new(),
            grid_auto_rows: Vec::new(),
            justify_items: None,
            justify_self: None,
            // Visual
            background: None,
            corner_radius: 0.0,
            border: None,
            opacity: 1.0,
            clip: false,
            font_size: 14.0,
            color: None,
            font_family: None,
            font_weight: None,
            font_italic: false,
            line_height: None,
            letter_spacing: 0.0,
            text_align: None,
            max_lines: None,
            text_overflow_ellipsis: false,
            underline: false,
            strikethrough: false,
            overline: false,
            text_decoration_style: None,
            text_decoration_color: None,
            word_spacing: 0.0,
            text_shadow: Vec::new(),
            cursor_offset: None,
            selection_range: None,
            scroll_offset: 0.0,
            animate: None,
            animate_layout: false,
            layout_animation: None,
            initial: None,
            exit: None,
            scroll_direction: None,
            on_click: None,
            on_hover: None,
            on_drag: None,
            on_scroll: None,
            on_change: None,
            on_submit: None,
            cursor: None,
        }
    }
}

// Builder functions
pub fn container() -> Element {
    Element {
        kind: ElementKind::Container,
        direction: Direction::Column,
        ..Default::default()
    }
}

pub fn row() -> Element {
    Element {
        kind: ElementKind::Container,
        direction: Direction::Row,
        ..Default::default()
    }
}

pub fn column() -> Element {
    Element {
        kind: ElementKind::Container,
        direction: Direction::Column,
        ..Default::default()
    }
}

pub fn stack() -> Element {
    Element {
        kind: ElementKind::Container,
        direction: Direction::Column,
        ..Default::default()
    }
}

pub fn grid() -> Element {
    Element {
        kind: ElementKind::Container,
        display: DisplayMode::Grid,
        ..Default::default()
    }
}

pub fn text(content: &str) -> Element {
    Element {
        kind: ElementKind::Text {
            content: content.to_string(),
        },
        ..Default::default()
    }
}

pub fn image(svg: &str) -> Element {
    Element {
        kind: ElementKind::Image {
            source: ImageSource::Svg(svg.to_string()),
        },
        ..Default::default()
    }
}

pub fn image_file(path: &str) -> Element {
    Element {
        kind: ElementKind::Image {
            source: ImageSource::File(path.to_string()),
        },
        ..Default::default()
    }
}

pub fn spacer() -> Element {
    Element {
        kind: ElementKind::Spacer,
        flex_grow: 1.0,
        ..Default::default()
    }
}

pub fn divider() -> Element {
    Element {
        kind: ElementKind::Divider { thickness: 1.0 },
        ..Default::default()
    }
}

pub fn scroll() -> Element {
    Element {
        kind: ElementKind::Container,
        scroll_direction: Some(ScrollDirection::Vertical),
        clip: true,
        ..Default::default()
    }
}

pub fn scroll_x() -> Element {
    Element {
        kind: ElementKind::Container,
        scroll_direction: Some(ScrollDirection::Horizontal),
        clip: true,
        ..Default::default()
    }
}

pub fn scroll_xy() -> Element {
    Element {
        kind: ElementKind::Container,
        scroll_direction: Some(ScrollDirection::Both),
        clip: true,
        ..Default::default()
    }
}

pub fn rich_text() -> Element {
    Element {
        kind: ElementKind::RichText { spans: Vec::new() },
        ..Default::default()
    }
}

pub fn text_input(value: &str) -> Element {
    Element {
        kind: ElementKind::TextInput {
            value: value.to_string(),
            placeholder: String::new(),
        },
        cursor: Some(CursorStyle::Text),
        ..Default::default()
    }
}

// Chainable style methods
impl Element {
    // === Layout ===

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dim::Px(w));
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dim::Px(h));
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Dim::Px(w));
        self.height = Some(Dim::Px(h));
        self
    }
    pub fn width_pct(mut self, pct: f32) -> Self {
        self.width = Some(Dim::Pct(pct));
        self
    }
    pub fn height_pct(mut self, pct: f32) -> Self {
        self.height = Some(Dim::Pct(pct));
        self
    }
    pub fn width_auto(mut self) -> Self {
        self.width = Some(Dim::Auto);
        self
    }
    pub fn height_auto(mut self) -> Self {
        self.height = Some(Dim::Auto);
        self
    }
    pub fn fill_width(mut self) -> Self {
        self.fill_width = true;
        self
    }
    pub fn fill_height(mut self) -> Self {
        self.fill_height = true;
        self
    }
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = [p, p, p, p];
        self
    }
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = [y, x, y, x];
        self
    }
    pub fn padding_edges(mut self, t: f32, r: f32, b: f32, l: f32) -> Self {
        self.padding = [t, r, b, l];
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }
    pub fn row_gap(mut self, g: f32) -> Self {
        self.row_gap = Some(g);
        self
    }
    pub fn column_gap(mut self, g: f32) -> Self {
        self.column_gap = Some(g);
        self
    }
    pub fn align_items(mut self, a: Alignment) -> Self {
        self.align_items = a;
        self
    }
    pub fn justify(mut self, j: Justify) -> Self {
        self.justify = j;
        self
    }
    pub fn flex_grow(mut self, g: f32) -> Self {
        self.flex_grow = g;
        self
    }

    // === Phase 1: Box Model ===

    pub fn margin(mut self, m: f32) -> Self {
        self.margin = [m, m, m, m];
        self
    }
    pub fn margin_xy(mut self, x: f32, y: f32) -> Self {
        self.margin = [y, x, y, x];
        self
    }
    pub fn margin_edges(mut self, t: f32, r: f32, b: f32, l: f32) -> Self {
        self.margin = [t, r, b, l];
        self
    }
    pub fn margin_x_auto(mut self) -> Self {
        self.margin_auto[1] = true; // right
        self.margin_auto[3] = true; // left
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(Dim::Px(w));
        self
    }
    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = Some(Dim::Px(h));
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(Dim::Px(w));
        self
    }
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(Dim::Px(h));
        self
    }
    pub fn min_width_pct(mut self, pct: f32) -> Self {
        self.min_width = Some(Dim::Pct(pct));
        self
    }
    pub fn min_height_pct(mut self, pct: f32) -> Self {
        self.min_height = Some(Dim::Pct(pct));
        self
    }
    pub fn max_width_pct(mut self, pct: f32) -> Self {
        self.max_width = Some(Dim::Pct(pct));
        self
    }
    pub fn max_height_pct(mut self, pct: f32) -> Self {
        self.max_height = Some(Dim::Pct(pct));
        self
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }
    pub fn hidden(mut self) -> Self {
        self.display = DisplayMode::None;
        self
    }
    pub fn visible(mut self) -> Self {
        self.display = DisplayMode::Flex;
        self
    }

    // === Phase 2: Flexbox ===

    pub fn flex_shrink(mut self, s: f32) -> Self {
        self.flex_shrink = Some(s);
        self
    }
    pub fn flex_basis_px(mut self, px: f32) -> Self {
        self.flex_basis = Some(Dim::Px(px));
        self
    }
    pub fn flex_basis_pct(mut self, pct: f32) -> Self {
        self.flex_basis = Some(Dim::Pct(pct));
        self
    }
    pub fn wrap(mut self) -> Self {
        self.flex_wrap = FlexWrap::Wrap;
        self
    }
    pub fn wrap_reverse(mut self) -> Self {
        self.flex_wrap = FlexWrap::WrapReverse;
        self
    }
    pub fn no_wrap(mut self) -> Self {
        self.flex_wrap = FlexWrap::NoWrap;
        self
    }
    pub fn direction(mut self, d: Direction) -> Self {
        self.direction = d;
        self
    }
    pub fn align_self(mut self, a: Alignment) -> Self {
        self.align_self = Some(a);
        self
    }
    pub fn align_content(mut self, a: AlignContent) -> Self {
        self.align_content = Some(a);
        self
    }

    // === Phase 3: Positioning ===

    pub fn position_relative(mut self) -> Self {
        self.position = PositionType::Relative;
        self
    }
    pub fn position_absolute(mut self) -> Self {
        self.position = PositionType::Absolute;
        self
    }
    pub fn inset(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.inset = [Some(top), Some(right), Some(bottom), Some(left)];
        self
    }
    pub fn top(mut self, v: f32) -> Self {
        self.inset[0] = Some(v);
        self
    }
    pub fn right(mut self, v: f32) -> Self {
        self.inset[1] = Some(v);
        self
    }
    pub fn bottom(mut self, v: f32) -> Self {
        self.inset[2] = Some(v);
        self
    }
    pub fn left(mut self, v: f32) -> Self {
        self.inset[3] = Some(v);
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = Some(z);
        self
    }

    // === Phase 4: Overflow ===

    pub fn overflow(mut self, o: Overflow) -> Self {
        self.overflow_x = o;
        self.overflow_y = o;
        self
    }
    pub fn overflow_x(mut self, o: Overflow) -> Self {
        self.overflow_x = o;
        self
    }
    pub fn overflow_y(mut self, o: Overflow) -> Self {
        self.overflow_y = o;
        self
    }

    // === Phase 5: Grid ===

    pub fn grid_template_columns(mut self, tracks: Vec<TrackSize>) -> Self {
        self.grid_template_columns = tracks;
        self
    }
    pub fn grid_template_rows(mut self, tracks: Vec<TrackSize>) -> Self {
        self.grid_template_rows = tracks;
        self
    }
    pub fn grid_column(mut self, start: i16, end: i16) -> Self {
        self.grid_column = Some((GridPlacement::Line(start), GridPlacement::Line(end)));
        self
    }
    pub fn grid_column_span(mut self, start: i16, span: u16) -> Self {
        self.grid_column = Some((GridPlacement::Line(start), GridPlacement::Span(span)));
        self
    }
    pub fn grid_row(mut self, start: i16, end: i16) -> Self {
        self.grid_row = Some((GridPlacement::Line(start), GridPlacement::Line(end)));
        self
    }
    pub fn grid_row_span(mut self, start: i16, span: u16) -> Self {
        self.grid_row = Some((GridPlacement::Line(start), GridPlacement::Span(span)));
        self
    }
    pub fn grid_auto_flow(mut self, flow: GridAutoFlow) -> Self {
        self.grid_auto_flow = flow;
        self
    }
    pub fn grid_auto_columns(mut self, tracks: Vec<TrackSize>) -> Self {
        self.grid_auto_columns = tracks;
        self
    }
    pub fn grid_auto_rows(mut self, tracks: Vec<TrackSize>) -> Self {
        self.grid_auto_rows = tracks;
        self
    }
    pub fn justify_items(mut self, a: Alignment) -> Self {
        self.justify_items = Some(a);
        self
    }
    pub fn justify_self(mut self, a: Alignment) -> Self {
        self.justify_self = Some(a);
        self
    }

    // === Visual ===

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.corner_radius = r;
        self
    }
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(Border { color, width });
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn clip(mut self) -> Self {
        self.clip = true;
        self
    }

    // === Children ===

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    // === Identity ===

    pub fn key(mut self, k: &str) -> Self {
        self.key = Some(k.to_string());
        self
    }

    // === Text/Image-specific ===

    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = s;
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    pub fn font_family(mut self, family: &str) -> Self {
        self.font_family = Some(vec![family.to_string()]);
        self
    }
    pub fn font_families(mut self, families: &[&str]) -> Self {
        self.font_family = Some(families.iter().map(|s| s.to_string()).collect());
        self
    }
    pub fn font_weight(mut self, weight: i32) -> Self {
        self.font_weight = Some(weight);
        self
    }
    pub fn bold(mut self) -> Self {
        self.font_weight = Some(700);
        self
    }
    pub fn italic(mut self) -> Self {
        self.font_italic = true;
        self
    }
    pub fn line_height(mut self, lh: f32) -> Self {
        self.line_height = Some(lh);
        self
    }
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.text_align = Some(align);
        self
    }
    pub fn max_lines(mut self, n: usize) -> Self {
        self.max_lines = Some(n);
        self
    }
    pub fn ellipsis(mut self) -> Self {
        self.text_overflow_ellipsis = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub fn overline(mut self) -> Self {
        self.overline = true;
        self
    }
    pub fn text_decoration_style(mut self, style: TextDecorationStyle) -> Self {
        self.text_decoration_style = Some(style);
        self
    }
    pub fn text_decoration_color(mut self, color: Color) -> Self {
        self.text_decoration_color = Some(color);
        self
    }
    pub fn word_spacing(mut self, spacing: f32) -> Self {
        self.word_spacing = spacing;
        self
    }
    pub fn text_shadow(mut self, color: Color, offset: (f32, f32), blur: f64) -> Self {
        self.text_shadow.push((color, offset, blur));
        self
    }
    pub fn span(mut self, content: &str, style_fn: impl FnOnce(SpanBuilder) -> SpanBuilder) -> Self {
        if let ElementKind::RichText { ref mut spans } = self.kind {
            let builder = style_fn(SpanBuilder::new(content));
            spans.push(builder.build());
        }
        self
    }

    // === Event handlers ===

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    pub fn on_hover(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_hover = Some(Box::new(f));
        self
    }
    pub fn on_drag(mut self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.on_drag = Some(Box::new(f));
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }
    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }
    pub fn placeholder(mut self, text: &str) -> Self {
        match &mut self.kind {
            ElementKind::TextInput { placeholder, .. } => {
                *placeholder = text.to_string();
            }
            _ => {}
        }
        self
    }
    pub fn cursor(mut self, style: CursorStyle) -> Self {
        self.cursor = Some(style);
        self
    }

    // === Animation ===

    pub fn animate(mut self, animation: Animation) -> Self {
        self.animate = Some(animation);
        self
    }
    pub fn animate_layout(mut self) -> Self {
        self.animate_layout = true;
        self
    }
    pub fn animate_layout_with(mut self, animation: Animation) -> Self {
        self.animate_layout = true;
        self.layout_animation = Some(animation);
        self
    }
    pub fn initial(mut self, from: From) -> Self {
        self.initial = Some(from);
        self
    }
    pub fn exit(mut self, to: To) -> Self {
        self.exit = Some(to);
        self
    }
}

pub struct SpanBuilder {
    span: TextSpan,
}

impl SpanBuilder {
    fn new(content: &str) -> Self {
        Self {
            span: TextSpan {
                content: content.to_string(),
                color: None,
                font_size: None,
                font_weight: None,
                italic: false,
                underline: false,
                strikethrough: false,
                font_family: None,
                letter_spacing: None,
                background: None,
                text_decoration_color: None,
                text_decoration_style: None,
            },
        }
    }
    fn build(self) -> TextSpan {
        self.span
    }
    pub fn color(mut self, c: Color) -> Self {
        self.span.color = Some(c);
        self
    }
    pub fn font_size(mut self, s: f32) -> Self {
        self.span.font_size = Some(s);
        self
    }
    pub fn font_weight(mut self, w: i32) -> Self {
        self.span.font_weight = Some(w);
        self
    }
    pub fn bold(mut self) -> Self {
        self.span.font_weight = Some(700);
        self
    }
    pub fn italic(mut self) -> Self {
        self.span.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.span.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.span.strikethrough = true;
        self
    }
    pub fn font_family(mut self, family: &str) -> Self {
        self.span.font_family = Some(vec![family.to_string()]);
        self
    }
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.span.letter_spacing = Some(spacing);
        self
    }
    pub fn background(mut self, c: Color) -> Self {
        self.span.background = Some(c);
        self
    }
    pub fn text_decoration_color(mut self, c: Color) -> Self {
        self.span.text_decoration_color = Some(c);
        self
    }
    pub fn text_decoration_style(mut self, style: TextDecorationStyle) -> Self {
        self.span.text_decoration_style = Some(style);
        self
    }
}
