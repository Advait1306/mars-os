#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

pub const TRANSPARENT: Color = rgba(0, 0, 0, 0);
pub const WHITE: Color = rgba(255, 255, 255, 255);
pub const BLACK: Color = rgba(0, 0, 0, 255);
