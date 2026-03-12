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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: f32,
}
