use crate::color::Color;
use crate::input::CursorStyle;
use crate::style::{Alignment, Border, Direction, Justify};

pub enum ElementKind {
    Container,
    Text { content: String },
    Image { source: ImageSource },
    Spacer,
    Divider { thickness: f32 },
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
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub fill_width: bool,
    pub fill_height: bool,
    pub padding: [f32; 4], // top, right, bottom, left
    pub gap: f32,
    pub align_items: Alignment,
    pub justify: Justify,
    pub flex_grow: f32,

    // Visual props
    pub background: Option<Color>,
    pub corner_radius: f32,
    pub border: Option<Border>,
    pub opacity: f32,
    pub clip: bool,

    // Text-specific
    pub font_size: f32,
    pub color: Option<Color>,

    // Event handlers
    pub on_click: Option<Box<dyn Fn()>>,
    pub on_hover: Option<Box<dyn Fn(bool)>>,
    pub on_drag: Option<Box<dyn Fn(f32, f32)>>,
    pub on_scroll: Option<Box<dyn Fn(f32, f32)>>,
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
            align_items: Alignment::Start,
            justify: Justify::Start,
            flex_grow: 0.0,
            background: None,
            corner_radius: 0.0,
            border: None,
            opacity: 1.0,
            clip: false,
            font_size: 14.0,
            color: None,
            on_click: None,
            on_hover: None,
            on_drag: None,
            on_scroll: None,
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

// Chainable style methods
impl Element {
    // Layout
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = Some(h);
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
    pub fn align_items(mut self, a: Alignment) -> Self {
        self.align_items = a;
        self
    }
    pub fn justify(mut self, j: Justify) -> Self {
        self.justify = j;
        self
    }

    // Visual
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

    // Children
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    // Identity
    pub fn key(mut self, k: &str) -> Self {
        self.key = Some(k.to_string());
        self
    }

    // Text/Image-specific
    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = s;
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    // Event handlers
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
    pub fn cursor(mut self, style: CursorStyle) -> Self {
        self.cursor = Some(style);
        self
    }
}
