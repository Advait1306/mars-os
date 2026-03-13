use crate::animation::{Animation, From, To};
use crate::color::Color;
use crate::input::{
    BeforeInputEvent, ClickEvent, ClipboardEvent, CompositionEvent, CursorStyle, DragEvent,
    EventResult, FocusEvent, KeyboardEvent, PointerEvent, PointerEvents, ScrollEndEvent,
    TextInputEvent, TouchEvent, WheelEvent,
};
use crate::select_state::SelectOption;
use crate::style::{
    AlignContent, Alignment, Border, BorderSide, BorderStyle, BoxShadow, CornerRadii, Dim,
    Direction, DisplayMode, Filter, FlexWrap, FullBorder, Gradient, GridAutoFlow, GridPlacement,
    BlendMode, Justify, Outline, Overflow, PositionType, TextAlign, TextDecorationStyle, TrackSize,
    Transform,
};

pub enum ElementKind {
    Container,
    Text { content: String },
    RichText { spans: Vec<RichSpan> },
    Image { source: ImageSource },
    Spacer,
    Divider { thickness: f32 },
    TextInput { value: String, placeholder: String },
    Shape { data: ShapeData },
    Button {
        label: String,
        variant: ButtonVariant,
    },
    Checkbox {
        checked: bool,
        indeterminate: bool,
        label: Option<String>,
    },
    Radio {
        selected: bool,
        group: String,
        value: String,
        label: Option<String>,
    },
    Switch {
        on: bool,
        label: Option<String>,
    },
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
    },
    RangeSlider {
        low: f64,
        high: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
    },
    Progress {
        value: Option<f64>,
        variant: ProgressVariant,
    },
    Textarea {
        value: String,
        placeholder: String,
    },
    Select {
        options: Vec<SelectOption>,
        selected: Option<usize>,
        placeholder: String,
    },
}

/// Data for a vector shape element.
#[derive(Debug, Clone)]
pub struct ShapeData {
    /// SVG path `d` attribute string.
    pub path_data: String,
    /// Fill color (None = no fill).
    pub fill: Option<Color>,
    /// Stroke color and width (None = no stroke).
    pub stroke: Option<(Color, f32)>,
    /// ViewBox width/height for scaling the path to element bounds.
    pub viewbox: Option<(f32, f32)>,
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
    pub font_features: Vec<(String, i32)>,
    pub font_variations: Vec<(String, f32)>,
}

/// Alignment of an inline placeholder relative to surrounding text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaceholderAlignment {
    Baseline,
    AboveBaseline,
    BelowBaseline,
    Top,
    Bottom,
    Middle,
}

/// An inline placeholder in rich text (for icons, images, etc.).
#[derive(Debug, Clone)]
pub struct InlinePlaceholder {
    pub width: f32,
    pub height: f32,
    pub alignment: PlaceholderAlignment,
    pub image: Option<ImageSource>,
}

/// A rich text span: either styled text or an inline placeholder.
#[derive(Debug, Clone)]
pub enum RichSpan {
    Text(TextSpan),
    Placeholder(InlinePlaceholder),
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
    /// Vector SVG: resolution-independent rendering via Skia paths instead of rasterization.
    VectorSvg(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressVariant {
    Bar,
    Circular,
}

/// Text wrapping mode for textarea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextWrap {
    /// Visual wrapping only, no newlines inserted.
    Soft,
    /// No wrapping, horizontal scroll.
    Off,
}

/// Resize behavior for textarea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextareaResize {
    None,
    Vertical,
    Horizontal,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextInputVariant {
    Text,
    Password,
    Email,
    Url,
    Search,
    Number,
    Tel,
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
    pub gradient: Option<Gradient>,
    pub corner_radii: CornerRadii,
    pub border: Option<Border>,
    pub border_style: BorderStyle,
    pub full_border: Option<FullBorder>,
    pub outline: Option<Outline>,
    pub box_shadows: Vec<BoxShadow>,
    pub visible: bool,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub filters: Vec<Filter>,
    pub backdrop_filters: Vec<Filter>,
    pub transforms: Vec<Transform>,
    /// Transform origin as fraction of element size (0.0–1.0). Default: center (0.5, 0.5).
    pub transform_origin: (f32, f32),
    pub clip: bool,
    pub tint: Option<Color>,
    pub image_fit: ImageFit,

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
    pub font_features: Vec<(String, i32)>,
    pub font_variations: Vec<(String, f32)>,
    pub text_direction: Option<TextDirection>,
    pub locale: Option<String>,

    // Text input cursor state (set by the view when the input is focused)
    pub cursor_offset: Option<usize>,
    pub selection_range: Option<(usize, usize)>,
    pub scroll_offset: f32,

    // IME preedit/composition state
    pub preedit_text: Option<String>,
    pub preedit_cursor: Option<usize>,

    // Animation
    pub animate: Option<Animation>,
    pub animate_layout: bool,
    pub layout_animation: Option<Animation>,
    pub initial: Option<From>,
    pub exit: Option<To>,

    // Scroll
    pub scroll_direction: Option<ScrollDirection>,

    // Event handlers -- bubble phase
    pub on_pointer_down: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub on_pointer_up: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub on_pointer_move: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub on_pointer_enter: Option<Box<dyn Fn(&PointerEvent)>>,
    pub on_pointer_leave: Option<Box<dyn Fn(&PointerEvent)>>,

    // Event handlers -- capture phase
    pub on_pointer_down_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub on_pointer_up_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,
    pub on_pointer_move_capture: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,

    // Synthesized pointer events
    pub on_click: Option<Box<dyn Fn(&ClickEvent) -> EventResult>>,
    pub on_double_click: Option<Box<dyn Fn(&ClickEvent) -> EventResult>>,
    pub on_context_menu: Option<Box<dyn Fn(&PointerEvent) -> EventResult>>,

    // Keyboard events
    pub on_key_down: Option<Box<dyn Fn(&KeyboardEvent) -> EventResult>>,
    pub on_key_up: Option<Box<dyn Fn(&KeyboardEvent) -> EventResult>>,

    // Focus events
    pub on_focus: Option<Box<dyn Fn(&FocusEvent)>>,
    pub on_blur: Option<Box<dyn Fn(&FocusEvent)>>,
    pub on_focus_in: Option<Box<dyn Fn(&FocusEvent) -> EventResult>>,
    pub on_focus_out: Option<Box<dyn Fn(&FocusEvent) -> EventResult>>,

    // Scroll events
    pub on_wheel: Option<Box<dyn Fn(&WheelEvent) -> EventResult>>,
    pub on_scroll_end: Option<Box<dyn Fn(&ScrollEndEvent)>>,

    // Text input events
    pub on_before_input: Option<Box<dyn Fn(&BeforeInputEvent) -> EventResult>>,
    pub on_input: Option<Box<dyn Fn(&TextInputEvent)>>,

    // Composition (IME) events
    pub on_composition_start: Option<Box<dyn Fn(&CompositionEvent) -> EventResult>>,
    pub on_composition_update: Option<Box<dyn Fn(&CompositionEvent)>>,
    pub on_composition_end: Option<Box<dyn Fn(&CompositionEvent)>>,

    // Clipboard events
    pub on_copy: Option<Box<dyn Fn(&mut ClipboardEvent) -> EventResult>>,
    pub on_cut: Option<Box<dyn Fn(&mut ClipboardEvent) -> EventResult>>,
    pub on_paste: Option<Box<dyn Fn(&ClipboardEvent) -> EventResult>>,

    // Drag and drop events
    pub on_drag_start: Option<Box<dyn Fn(&mut DragEvent) -> EventResult>>,
    pub on_drag_over: Option<Box<dyn Fn(&mut DragEvent) -> EventResult>>,
    pub on_drop: Option<Box<dyn Fn(&DragEvent) -> EventResult>>,
    pub on_drag_enter: Option<Box<dyn Fn(&DragEvent)>>,
    pub on_drag_leave: Option<Box<dyn Fn(&DragEvent)>>,

    // Touch events (native multi-touch, in addition to touch-to-pointer coercion)
    pub on_touch_start: Option<Box<dyn Fn(&TouchEvent) -> EventResult>>,
    pub on_touch_move: Option<Box<dyn Fn(&TouchEvent) -> EventResult>>,
    pub on_touch_end: Option<Box<dyn Fn(&TouchEvent) -> EventResult>>,
    pub on_touch_cancel: Option<Box<dyn Fn(&TouchEvent)>>,

    // Legacy handlers (kept for compatibility during migration)
    pub on_hover: Option<Box<dyn Fn(bool)>>,
    pub on_drag: Option<Box<dyn Fn(f32, f32)>>,
    pub on_scroll: Option<Box<dyn Fn(f32, f32)>>,
    pub on_change: Option<Box<dyn Fn(String)>>,
    pub on_submit: Option<Box<dyn Fn()>>,
    pub cursor: Option<CursorStyle>,

    // Focus properties
    pub focusable: Option<bool>,
    pub tab_index: Option<i32>,
    pub focus_trap: bool,

    // Form element properties
    pub disabled: bool,
    pub read_only: bool,
    pub error: Option<String>,
    pub label: Option<String>,
    pub indeterminate: bool,
    pub loading: bool,
    pub show_value: bool,
    pub progress_color: Option<Color>,
    pub track_color: Option<Color>,

    // Select/Dropdown properties
    pub select_open: bool,
    pub select_highlighted: Option<usize>,
    pub select_searchable: bool,
    pub select_search_text: String,
    pub select_max_visible: usize,
    pub select_scroll_offset: f32,

    // Textarea properties
    pub text_wrap: TextWrap,
    pub textarea_resize: TextareaResize,
    pub show_line_numbers: bool,
    pub tab_size: u32,
    pub auto_resize: bool,
    pub scroll_offset_y: f32,
    pub scroll_offset_x: f32,

    // Hit testing
    pub pointer_events: PointerEvents,
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
            gradient: None,
            corner_radii: CornerRadii::ZERO,
            border: None,
            border_style: BorderStyle::Solid,
            full_border: None,
            outline: None,
            box_shadows: Vec::new(),
            visible: true,
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            filters: Vec::new(),
            backdrop_filters: Vec::new(),
            transforms: Vec::new(),
            transform_origin: (0.5, 0.5),
            clip: false,
            tint: None,
            image_fit: ImageFit::Contain,
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
            font_features: Vec::new(),
            font_variations: Vec::new(),
            text_direction: None,
            locale: None,
            cursor_offset: None,
            selection_range: None,
            scroll_offset: 0.0,
            preedit_text: None,
            preedit_cursor: None,
            animate: None,
            animate_layout: false,
            layout_animation: None,
            initial: None,
            exit: None,
            scroll_direction: None,
            // Event handlers -- bubble
            on_pointer_down: None,
            on_pointer_up: None,
            on_pointer_move: None,
            on_pointer_enter: None,
            on_pointer_leave: None,
            // Event handlers -- capture
            on_pointer_down_capture: None,
            on_pointer_up_capture: None,
            on_pointer_move_capture: None,
            // Synthesized
            on_click: None,
            on_double_click: None,
            on_context_menu: None,
            // Keyboard
            on_key_down: None,
            on_key_up: None,
            // Focus
            on_focus: None,
            on_blur: None,
            on_focus_in: None,
            on_focus_out: None,
            // Scroll
            on_wheel: None,
            on_scroll_end: None,
            // Text input
            on_before_input: None,
            on_input: None,
            // Composition
            on_composition_start: None,
            on_composition_update: None,
            on_composition_end: None,
            // Clipboard
            on_copy: None,
            on_cut: None,
            on_paste: None,
            // Drag and drop
            on_drag_start: None,
            on_drag_over: None,
            on_drop: None,
            on_drag_enter: None,
            on_drag_leave: None,
            // Touch
            on_touch_start: None,
            on_touch_move: None,
            on_touch_end: None,
            on_touch_cancel: None,
            // Legacy
            on_hover: None,
            on_drag: None,
            on_scroll: None,
            on_change: None,
            on_submit: None,
            cursor: None,
            // Focus properties
            focusable: None,
            tab_index: None,
            focus_trap: false,
            // Form element properties
            disabled: false,
            read_only: false,
            error: None,
            label: None,
            indeterminate: false,
            loading: false,
            show_value: false,
            progress_color: None,
            track_color: None,
            // Select/Dropdown
            select_open: false,
            select_highlighted: None,
            select_searchable: false,
            select_search_text: String::new(),
            select_max_visible: 8,
            select_scroll_offset: 0.0,
            // Textarea
            text_wrap: TextWrap::Soft,
            textarea_resize: TextareaResize::None,
            show_line_numbers: false,
            tab_size: 4,
            auto_resize: false,
            scroll_offset_y: 0.0,
            scroll_offset_x: 0.0,
            // Hit testing
            pointer_events: PointerEvents::Auto,
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

/// Create an image element using vector SVG rendering (resolution-independent).
/// The SVG is rendered via native Skia path calls instead of rasterization.
pub fn vector_svg(svg: &str) -> Element {
    Element {
        kind: ElementKind::Image {
            source: ImageSource::VectorSvg(svg.to_string()),
        },
        ..Default::default()
    }
}

/// Create an image element from an SVG file using vector rendering.
pub fn vector_svg_file(path: &str) -> Element {
    Element {
        kind: ElementKind::Image {
            source: ImageSource::VectorSvg(format!("file:{}", path)),
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

/// Create a vector shape element from an SVG path `d` attribute string.
pub fn shape(path_d: &str) -> Element {
    Element {
        kind: ElementKind::Shape {
            data: ShapeData {
                path_data: path_d.to_string(),
                fill: Some(crate::color::WHITE),
                stroke: None,
                viewbox: None,
            },
        },
        ..Default::default()
    }
}

/// Create a shape with an explicit viewbox for scaling.
pub fn shape_with_viewbox(path_d: &str, vb_w: f32, vb_h: f32) -> Element {
    Element {
        kind: ElementKind::Shape {
            data: ShapeData {
                path_data: path_d.to_string(),
                fill: Some(crate::color::WHITE),
                stroke: None,
                viewbox: Some((vb_w, vb_h)),
            },
        },
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

pub fn password_input(value: &str) -> Element {
    Element {
        kind: ElementKind::TextInput {
            value: value.to_string(),
            placeholder: String::new(),
        },
        cursor: Some(CursorStyle::Text),
        ..Default::default()
    }
}

pub fn button(label: &str) -> Element {
    Element {
        kind: ElementKind::Button {
            label: label.to_string(),
            variant: ButtonVariant::Primary,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

pub fn checkbox(checked: bool) -> Element {
    Element {
        kind: ElementKind::Checkbox {
            checked,
            indeterminate: false,
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

pub fn radio(selected: bool, group: &str, value: &str) -> Element {
    Element {
        kind: ElementKind::Radio {
            selected,
            group: group.to_string(),
            value: value.to_string(),
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

pub fn switch(on: bool) -> Element {
    Element {
        kind: ElementKind::Switch {
            on,
            label: None,
        },
        cursor: Some(CursorStyle::Pointer),
        ..Default::default()
    }
}

pub fn slider(value: f64, min: f64, max: f64) -> Element {
    Element {
        kind: ElementKind::Slider {
            value,
            min,
            max,
            step: None,
        },
        cursor: Some(CursorStyle::Pointer),
        height: Some(Dim::Px(24.0)),
        ..Default::default()
    }
}

pub fn range_slider(low: f64, high: f64, min: f64, max: f64) -> Element {
    Element {
        kind: ElementKind::RangeSlider {
            low,
            high,
            min,
            max,
            step: None,
        },
        cursor: Some(CursorStyle::Pointer),
        height: Some(Dim::Px(24.0)),
        ..Default::default()
    }
}

pub fn progress(value: f64) -> Element {
    Element {
        kind: ElementKind::Progress {
            value: Some(value.clamp(0.0, 1.0)),
            variant: ProgressVariant::Bar,
        },
        height: Some(Dim::Px(6.0)),
        ..Default::default()
    }
}

pub fn progress_indeterminate() -> Element {
    Element {
        kind: ElementKind::Progress {
            value: None,
            variant: ProgressVariant::Bar,
        },
        height: Some(Dim::Px(6.0)),
        ..Default::default()
    }
}

pub fn spinner() -> Element {
    Element {
        kind: ElementKind::Progress {
            value: None,
            variant: ProgressVariant::Circular,
        },
        width: Some(Dim::Px(24.0)),
        height: Some(Dim::Px(24.0)),
        ..Default::default()
    }
}

pub fn select(options: Vec<SelectOption>, selected: Option<usize>) -> Element {
    Element {
        kind: ElementKind::Select {
            options,
            selected,
            placeholder: String::new(),
        },
        cursor: Some(CursorStyle::Pointer),
        height: Some(Dim::Px(36.0)),
        focusable: Some(true),
        tab_index: Some(0),
        ..Default::default()
    }
}

pub fn textarea(value: &str) -> Element {
    Element {
        kind: ElementKind::Textarea {
            value: value.to_string(),
            placeholder: String::new(),
        },
        cursor: Some(CursorStyle::Text),
        width: Some(Dim::Px(300.0)),
        height: Some(Dim::Px(120.0)),
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
    pub fn background_gradient(mut self, gradient: Gradient) -> Self {
        self.gradient = Some(gradient);
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.corner_radii = CornerRadii::uniform(r);
        self
    }
    pub fn corner_radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_radii = CornerRadii {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
        };
        self
    }
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(Border { color, width });
        self
    }
    pub fn border_style(mut self, style: BorderStyle) -> Self {
        self.border_style = style;
        self
    }
    pub fn border_top(mut self, color: Color, width: f32) -> Self {
        let side = BorderSide { width, color, style: BorderStyle::Solid };
        self.full_border.get_or_insert(FullBorder { top: None, right: None, bottom: None, left: None }).top = Some(side);
        self
    }
    pub fn border_right(mut self, color: Color, width: f32) -> Self {
        let side = BorderSide { width, color, style: BorderStyle::Solid };
        self.full_border.get_or_insert(FullBorder { top: None, right: None, bottom: None, left: None }).right = Some(side);
        self
    }
    pub fn border_bottom(mut self, color: Color, width: f32) -> Self {
        let side = BorderSide { width, color, style: BorderStyle::Solid };
        self.full_border.get_or_insert(FullBorder { top: None, right: None, bottom: None, left: None }).bottom = Some(side);
        self
    }
    pub fn border_left(mut self, color: Color, width: f32) -> Self {
        let side = BorderSide { width, color, style: BorderStyle::Solid };
        self.full_border.get_or_insert(FullBorder { top: None, right: None, bottom: None, left: None }).left = Some(side);
        self
    }
    pub fn outline(mut self, color: Color, width: f32) -> Self {
        self.outline = Some(Outline { color, width, style: BorderStyle::Solid, offset: 0.0 });
        self
    }
    pub fn outline_offset(mut self, offset: f32) -> Self {
        if let Some(ref mut o) = self.outline {
            o.offset = offset;
        }
        self
    }
    pub fn box_shadow(mut self, color: Color, offset_x: f32, offset_y: f32, blur: f32, spread: f32) -> Self {
        self.box_shadows.push(BoxShadow {
            color,
            offset_x,
            offset_y,
            blur,
            spread,
            inset: false,
        });
        self
    }
    pub fn inset_shadow(mut self, color: Color, offset_x: f32, offset_y: f32, blur: f32, spread: f32) -> Self {
        self.box_shadows.push(BoxShadow {
            color,
            offset_x,
            offset_y,
            blur,
            spread,
            inset: true,
        });
        self
    }
    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
    pub fn blend_mode(mut self, mode: BlendMode) -> Self {
        self.blend_mode = mode;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn filter(mut self, f: Filter) -> Self {
        self.filters.push(f);
        self
    }
    pub fn filter_blur(mut self, radius: f32) -> Self {
        self.filters.push(Filter::Blur(radius));
        self
    }
    pub fn filter_brightness(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Brightness(amount));
        self
    }
    pub fn filter_contrast(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Contrast(amount));
        self
    }
    pub fn filter_grayscale(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Grayscale(amount));
        self
    }
    pub fn filter_saturate(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Saturate(amount));
        self
    }
    pub fn filter_sepia(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Sepia(amount));
        self
    }
    pub fn filter_hue_rotate(mut self, degrees: f32) -> Self {
        self.filters.push(Filter::HueRotate(degrees));
        self
    }
    pub fn filter_invert(mut self, amount: f32) -> Self {
        self.filters.push(Filter::Invert(amount));
        self
    }
    pub fn filter_drop_shadow(mut self, x: f32, y: f32, blur: f32, color: Color) -> Self {
        self.filters.push(Filter::DropShadow { x, y, blur, color });
        self
    }
    pub fn backdrop_filter(mut self, f: Filter) -> Self {
        self.backdrop_filters.push(f);
        self
    }
    pub fn backdrop_blur(mut self, radius: f32) -> Self {
        self.backdrop_filters.push(Filter::Blur(radius));
        self
    }
    pub fn rotate(mut self, degrees: f32) -> Self {
        self.transforms.push(Transform::Rotate(degrees));
        self
    }
    pub fn scale(mut self, s: f32) -> Self {
        self.transforms.push(Transform::Scale(s, s));
        self
    }
    pub fn scale_xy(mut self, sx: f32, sy: f32) -> Self {
        self.transforms.push(Transform::Scale(sx, sy));
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.transforms.push(Transform::Translate(x, y));
        self
    }
    pub fn skew(mut self, x_deg: f32, y_deg: f32) -> Self {
        self.transforms.push(Transform::Skew(x_deg, y_deg));
        self
    }
    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        self.transform_origin = (x, y);
        self
    }
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }
    pub fn image_fit(mut self, fit: ImageFit) -> Self {
        self.image_fit = fit;
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
    /// Add an OpenType font feature (e.g., "tnum" for tabular numbers, "liga" for ligatures).
    pub fn font_feature(mut self, tag: &str, value: i32) -> Self {
        self.font_features.push((tag.to_string(), value));
        self
    }
    /// Add a variable font axis value (e.g., "wght" for weight, "wdth" for width).
    pub fn font_variation(mut self, axis: &str, value: f32) -> Self {
        self.font_variations.push((axis.to_string(), value));
        self
    }
    /// Set text direction for BiDi support (LTR or RTL).
    pub fn text_direction(mut self, dir: TextDirection) -> Self {
        self.text_direction = Some(dir);
        self
    }
    /// Set RTL text direction.
    pub fn rtl(mut self) -> Self {
        self.text_direction = Some(TextDirection::Rtl);
        self
    }
    /// Set locale for proper line breaking (e.g., "ja" for Japanese, "ar" for Arabic).
    pub fn locale(mut self, locale: &str) -> Self {
        self.locale = Some(locale.to_string());
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
            spans.push(RichSpan::Text(builder.build()));
        }
        self
    }
    /// Add an inline placeholder (icon/image) within rich text flow.
    pub fn inline_placeholder(mut self, width: f32, height: f32, alignment: PlaceholderAlignment, image: Option<ImageSource>) -> Self {
        if let ElementKind::RichText { ref mut spans } = self.kind {
            spans.push(RichSpan::Placeholder(InlinePlaceholder {
                width,
                height,
                alignment,
                image,
            }));
        }
        self
    }
    /// Add an inline image within rich text flow, aligned to the middle of the text.
    pub fn inline_image(self, source: ImageSource, width: f32, height: f32) -> Self {
        self.inline_placeholder(width, height, PlaceholderAlignment::Middle, Some(source))
    }

    // === Event handlers -- pointer (bubble phase) ===

    pub fn on_pointer_down(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_down = Some(Box::new(f));
        self
    }
    pub fn on_pointer_up(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_up = Some(Box::new(f));
        self
    }
    pub fn on_pointer_move(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_move = Some(Box::new(f));
        self
    }
    pub fn on_pointer_enter(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_pointer_enter = Some(Box::new(f));
        self
    }
    pub fn on_pointer_leave(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_pointer_leave = Some(Box::new(f));
        self
    }

    // === Event handlers -- pointer (capture phase) ===

    pub fn on_pointer_down_capture(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_down_capture = Some(Box::new(f));
        self
    }
    pub fn on_pointer_up_capture(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_up_capture = Some(Box::new(f));
        self
    }
    pub fn on_pointer_move_capture(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_pointer_move_capture = Some(Box::new(f));
        self
    }

    // === Synthesized pointer events ===

    pub fn on_click(mut self, f: impl Fn(&ClickEvent) -> EventResult + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    pub fn on_double_click(mut self, f: impl Fn(&ClickEvent) -> EventResult + 'static) -> Self {
        self.on_double_click = Some(Box::new(f));
        self
    }
    pub fn on_context_menu(mut self, f: impl Fn(&PointerEvent) -> EventResult + 'static) -> Self {
        self.on_context_menu = Some(Box::new(f));
        self
    }

    // === Keyboard events ===

    pub fn on_key_down(mut self, f: impl Fn(&KeyboardEvent) -> EventResult + 'static) -> Self {
        self.on_key_down = Some(Box::new(f));
        self
    }
    pub fn on_key_up(mut self, f: impl Fn(&KeyboardEvent) -> EventResult + 'static) -> Self {
        self.on_key_up = Some(Box::new(f));
        self
    }

    // === Focus events ===

    pub fn on_focus(mut self, f: impl Fn(&FocusEvent) + 'static) -> Self {
        self.on_focus = Some(Box::new(f));
        self
    }
    pub fn on_blur(mut self, f: impl Fn(&FocusEvent) + 'static) -> Self {
        self.on_blur = Some(Box::new(f));
        self
    }
    pub fn on_focus_in(mut self, f: impl Fn(&FocusEvent) -> EventResult + 'static) -> Self {
        self.on_focus_in = Some(Box::new(f));
        self
    }
    pub fn on_focus_out(mut self, f: impl Fn(&FocusEvent) -> EventResult + 'static) -> Self {
        self.on_focus_out = Some(Box::new(f));
        self
    }

    // === Scroll events ===

    pub fn on_wheel(mut self, f: impl Fn(&WheelEvent) -> EventResult + 'static) -> Self {
        self.on_wheel = Some(Box::new(f));
        self
    }
    pub fn on_scroll_end(mut self, f: impl Fn(&ScrollEndEvent) + 'static) -> Self {
        self.on_scroll_end = Some(Box::new(f));
        self
    }

    // === Text input events ===

    pub fn on_before_input(mut self, f: impl Fn(&BeforeInputEvent) -> EventResult + 'static) -> Self {
        self.on_before_input = Some(Box::new(f));
        self
    }
    pub fn on_input(mut self, f: impl Fn(&TextInputEvent) + 'static) -> Self {
        self.on_input = Some(Box::new(f));
        self
    }

    // === Composition (IME) events ===

    pub fn on_composition_start(mut self, f: impl Fn(&CompositionEvent) -> EventResult + 'static) -> Self {
        self.on_composition_start = Some(Box::new(f));
        self
    }
    pub fn on_composition_update(mut self, f: impl Fn(&CompositionEvent) + 'static) -> Self {
        self.on_composition_update = Some(Box::new(f));
        self
    }
    pub fn on_composition_end(mut self, f: impl Fn(&CompositionEvent) + 'static) -> Self {
        self.on_composition_end = Some(Box::new(f));
        self
    }

    // === Drag and drop events ===

    pub fn on_drag_start(mut self, f: impl Fn(&mut DragEvent) -> EventResult + 'static) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }
    pub fn on_drag_over(mut self, f: impl Fn(&mut DragEvent) -> EventResult + 'static) -> Self {
        self.on_drag_over = Some(Box::new(f));
        self
    }
    pub fn on_drop(mut self, f: impl Fn(&DragEvent) -> EventResult + 'static) -> Self {
        self.on_drop = Some(Box::new(f));
        self
    }
    pub fn on_drag_enter(mut self, f: impl Fn(&DragEvent) + 'static) -> Self {
        self.on_drag_enter = Some(Box::new(f));
        self
    }
    pub fn on_drag_leave(mut self, f: impl Fn(&DragEvent) + 'static) -> Self {
        self.on_drag_leave = Some(Box::new(f));
        self
    }

    // === Clipboard events ===

    pub fn on_copy(mut self, f: impl Fn(&mut ClipboardEvent) -> EventResult + 'static) -> Self {
        self.on_copy = Some(Box::new(f));
        self
    }
    pub fn on_cut(mut self, f: impl Fn(&mut ClipboardEvent) -> EventResult + 'static) -> Self {
        self.on_cut = Some(Box::new(f));
        self
    }
    pub fn on_paste(mut self, f: impl Fn(&ClipboardEvent) -> EventResult + 'static) -> Self {
        self.on_paste = Some(Box::new(f));
        self
    }

    // === Touch events ===

    pub fn on_touch_start(mut self, f: impl Fn(&TouchEvent) -> EventResult + 'static) -> Self {
        self.on_touch_start = Some(Box::new(f));
        self
    }
    pub fn on_touch_move(mut self, f: impl Fn(&TouchEvent) -> EventResult + 'static) -> Self {
        self.on_touch_move = Some(Box::new(f));
        self
    }
    pub fn on_touch_end(mut self, f: impl Fn(&TouchEvent) -> EventResult + 'static) -> Self {
        self.on_touch_end = Some(Box::new(f));
        self
    }
    pub fn on_touch_cancel(mut self, f: impl Fn(&TouchEvent) + 'static) -> Self {
        self.on_touch_cancel = Some(Box::new(f));
        self
    }

    // === Legacy event handlers (kept for migration) ===

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
            ElementKind::TextInput { placeholder, .. }
            | ElementKind::Textarea { placeholder, .. }
            | ElementKind::Select { placeholder, .. } => {
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

    /// Set preedit (IME composition) text to display inline at the cursor.
    pub fn preedit(mut self, text: &str, cursor: Option<usize>) -> Self {
        if text.is_empty() {
            self.preedit_text = None;
            self.preedit_cursor = None;
        } else {
            self.preedit_text = Some(text.to_string());
            self.preedit_cursor = cursor;
        }
        self
    }

    // === Focus properties ===

    /// Make this element focusable.
    /// tab_index controls tab order:
    ///   None = not tabbable (but still focusable by click/programmatic)
    ///   Some(0) = tabbable in document order
    ///   Some(n > 0) = tabbable with explicit order (lower numbers first)
    pub fn focusable(mut self, tab_index: Option<i32>) -> Self {
        self.focusable = Some(true);
        self.tab_index = tab_index;
        self
    }

    /// Make this element not focusable.
    pub fn not_focusable(mut self) -> Self {
        self.focusable = Some(false);
        self.tab_index = None;
        self
    }

    /// Trap focus within this element's subtree.
    /// Tab/Shift+Tab will cycle only among focusable descendants.
    pub fn focus_trap(mut self) -> Self {
        self.focus_trap = true;
        self
    }

    // === Form element properties ===

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn read_only(mut self, r: bool) -> Self {
        self.read_only = r;
        self
    }
    pub fn error(mut self, e: Option<&str>) -> Self {
        self.error = e.map(|s| s.to_string());
        self
    }
    pub fn label(mut self, text: &str) -> Self {
        match &mut self.kind {
            ElementKind::Checkbox { label, .. }
            | ElementKind::Radio { label, .. }
            | ElementKind::Switch { label, .. } => {
                *label = Some(text.to_string());
            }
            _ => {
                self.label = Some(text.to_string());
            }
        }
        self
    }
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        if let ElementKind::Button { variant, .. } = &mut self.kind {
            *variant = v;
        }
        self
    }
    pub fn indeterminate(mut self, i: bool) -> Self {
        if let ElementKind::Checkbox { indeterminate, .. } = &mut self.kind {
            *indeterminate = i;
        } else {
            self.indeterminate = i;
        }
        self
    }
    pub fn step(mut self, s: f64) -> Self {
        match &mut self.kind {
            ElementKind::Slider { step, .. } | ElementKind::RangeSlider { step, .. } => {
                *step = Some(s);
            }
            _ => {}
        }
        self
    }
    pub fn show_value(mut self, on: bool) -> Self {
        self.show_value = on;
        self
    }
    pub fn progress_color(mut self, c: Color) -> Self {
        self.progress_color = Some(c);
        self
    }
    pub fn track_color(mut self, c: Color) -> Self {
        self.track_color = Some(c);
        self
    }
    pub fn loading(mut self, on: bool) -> Self {
        self.loading = on;
        self
    }

    // === Select/Dropdown properties ===

    pub fn select_open(mut self, open: bool) -> Self {
        self.select_open = open;
        self
    }
    pub fn select_highlighted(mut self, idx: Option<usize>) -> Self {
        self.select_highlighted = idx;
        self
    }
    pub fn searchable(mut self, on: bool) -> Self {
        self.select_searchable = on;
        self
    }
    pub fn search_text(mut self, text: &str) -> Self {
        self.select_search_text = text.to_string();
        self
    }
    pub fn max_visible(mut self, n: usize) -> Self {
        self.select_max_visible = n;
        self
    }
    pub fn select_scroll_offset(mut self, offset: f32) -> Self {
        self.select_scroll_offset = offset;
        self
    }

    // === Textarea properties ===

    pub fn text_wrap(mut self, w: TextWrap) -> Self {
        self.text_wrap = w;
        self
    }
    pub fn textarea_resize(mut self, r: TextareaResize) -> Self {
        self.textarea_resize = r;
        self
    }
    pub fn line_numbers(mut self, on: bool) -> Self {
        self.show_line_numbers = on;
        self
    }
    pub fn tab_size(mut self, n: u32) -> Self {
        self.tab_size = n;
        self
    }
    pub fn auto_resize(mut self, on: bool) -> Self {
        self.auto_resize = on;
        self
    }
    /// Set textarea rows (sets height based on line height).
    pub fn rows(mut self, r: u32) -> Self {
        let line_h = self.line_height.unwrap_or(1.4) * self.font_size;
        let padding_v = self.padding[0] + self.padding[2];
        self.height = Some(Dim::Px(r as f32 * line_h + padding_v + 16.0)); // 16 = default padding if none set
        self
    }

    /// This element and its children are invisible to pointer events.
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events = PointerEvents::None;
        self
    }

    /// Skip this element in hit testing but still test children.
    pub fn pointer_events_pass_through(mut self) -> Self {
        self.pointer_events = PointerEvents::PassThrough;
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
                font_features: Vec::new(),
                font_variations: Vec::new(),
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
    pub fn font_feature(mut self, tag: &str, value: i32) -> Self {
        self.span.font_features.push((tag.to_string(), value));
        self
    }
    pub fn font_variation(mut self, axis: &str, value: f32) -> Self {
        self.span.font_variations.push((axis.to_string(), value));
        self
    }
}
