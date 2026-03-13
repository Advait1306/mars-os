use crate::color::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignContent {
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionType {
    Relative,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overflow {
    Visible,
    Hidden,
    Clip,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Dimension value for width/height/min/max/flex-basis
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dim {
    Auto,
    Px(f32),
    Pct(f32),
}

/// Grid track sizing
#[derive(Debug, Clone, PartialEq)]
pub enum TrackSize {
    Px(f32),
    Pct(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
    MinMax(TrackMin, TrackMax),
    Repeat(u16, Vec<TrackSize>),
    AutoFill(Vec<TrackSize>),
    AutoFit(Vec<TrackSize>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackMin {
    Px(f32),
    Pct(f32),
    Auto,
    MinContent,
    MaxContent,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackMax {
    Px(f32),
    Pct(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

/// Grid placement for a child element
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridPlacement {
    Auto,
    Line(i16),
    Span(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// Per-corner border radii (top-left, top-right, bottom-right, bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub const ZERO: Self = Self {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    /// Uniform radius on all corners.
    pub fn uniform(r: f32) -> Self {
        Self {
            top_left: r,
            top_right: r,
            bottom_right: r,
            bottom_left: r,
        }
    }

    /// Returns true if all corners are zero.
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }
}

impl Default for CornerRadii {
    fn default() -> Self {
        Self::ZERO
    }
}

/// A color stop in a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    /// Position in 0.0–1.0 range. None = auto-distribute evenly.
    pub position: Option<f32>,
}

/// Gradient background definition.
#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear {
        /// CSS angle in degrees: 0 = to top, 90 = to right, 180 = to bottom (default).
        angle_deg: f32,
        stops: Vec<ColorStop>,
    },
    Radial {
        /// Center as fraction of element bounds (0.0–1.0). None = center (0.5, 0.5).
        center: Option<(f32, f32)>,
        stops: Vec<ColorStop>,
    },
    Conic {
        /// Center as fraction of element bounds. None = center.
        center: Option<(f32, f32)>,
        /// Starting angle in degrees.
        from_angle_deg: f32,
        stops: Vec<ColorStop>,
    },
}

/// A single background layer (solid, gradient, or image).
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Solid(Color),
    Gradient(Gradient),
    Image {
        source: String,
    },
}

/// Controls where the background is painted relative to the box model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackgroundClip {
    /// Paint within the border edge (default).
    BorderBox,
    /// Paint within the padding edge.
    PaddingBox,
    /// Paint within the content edge.
    ContentBox,
}

impl Default for BackgroundClip {
    fn default() -> Self {
        BackgroundClip::BorderBox
    }
}

/// Controls the origin for background positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackgroundOrigin {
    /// Position relative to border box (default).
    BorderBox,
    /// Position relative to padding box.
    PaddingBox,
    /// Position relative to content box.
    ContentBox,
}

impl Default for BackgroundOrigin {
    fn default() -> Self {
        BackgroundOrigin::PaddingBox
    }
}

/// Border image with nine-slice rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderImage {
    /// Image source path or data URI.
    pub source: String,
    /// Inset from each edge for the nine-slice (top, right, bottom, left) in pixels.
    pub slice: [f32; 4],
    /// Width of the border image area (top, right, bottom, left). If None, uses slice values.
    pub width: Option<[f32; 4]>,
    /// Whether to fill the center of the nine-slice.
    pub fill: bool,
}

/// Box shadow definition (supports both outset and inset shadows).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: f32,
}

/// Per-side border definition.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub color: Color,
    pub style: BorderStyle,
}

/// Full per-side border specification.
#[derive(Debug, Clone, PartialEq)]
pub struct FullBorder {
    pub top: Option<BorderSide>,
    pub right: Option<BorderSide>,
    pub bottom: Option<BorderSide>,
    pub left: Option<BorderSide>,
}

/// Outline (does not affect layout, drawn outside the element).
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
    pub style: BorderStyle,
    pub offset: f32,
}

/// CSS filter functions.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32), // degrees
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow {
        x: f32,
        y: f32,
        blur: f32,
        color: Color,
    },
}

/// CSS transform functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    Translate(f32, f32),
    Rotate(f32),        // degrees
    Scale(f32, f32),
    Skew(f32, f32),     // degrees
    Matrix([f32; 6]),   // CSS matrix(a,b,c,d,e,f)
}

/// CSS mix-blend-mode values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
