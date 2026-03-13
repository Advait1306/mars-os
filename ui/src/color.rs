#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse hex color: "#RGB", "#RGBA", "#RRGGBB", "#RRGGBBAA"
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Self { r, g, b, a: 255 })
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
                Some(Self { r, g, b, a })
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self { r, g, b, a: 255 })
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self { r, g, b, a })
            }
            _ => None,
        }
    }

    /// Create from HSL values (h: 0-360, s: 0.0-1.0, l: 0.0-1.0).
    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        Self::from_hsla(h, s, l, 1.0)
    }

    /// Create from HSLA values (h: 0-360, s: 0.0-1.0, l: 0.0-1.0, a: 0.0-1.0).
    pub fn from_hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = (h % 360.0) / 60.0;
        let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r1, g1, b1) = if h_prime < 1.0 {
            (c, x, 0.0)
        } else if h_prime < 2.0 {
            (x, c, 0.0)
        } else if h_prime < 3.0 {
            (0.0, c, x)
        } else if h_prime < 4.0 {
            (0.0, x, c)
        } else if h_prime < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        Self {
            r: ((r1 + m) * 255.0) as u8,
            g: ((g1 + m) * 255.0) as u8,
            b: ((b1 + m) * 255.0) as u8,
            a: (a * 255.0) as u8,
        }
    }

    /// Return a copy with the given alpha (0.0–1.0).
    pub fn with_alpha(self, a: f32) -> Self {
        Self {
            a: (a.clamp(0.0, 1.0) * 255.0) as u8,
            ..self
        }
    }

    /// Return a lighter variant (amount 0.0–1.0, e.g. 0.3 = 30% lighter).
    pub fn lighter(self, amount: f32) -> Self {
        Self {
            r: (self.r as f32 + (255.0 - self.r as f32) * amount) as u8,
            g: (self.g as f32 + (255.0 - self.g as f32) * amount) as u8,
            b: (self.b as f32 + (255.0 - self.b as f32) * amount) as u8,
            a: self.a,
        }
    }

    /// Return a darker variant (amount 0.0–1.0, e.g. 0.3 = 30% darker).
    pub fn darker(self, amount: f32) -> Self {
        let factor = 1.0 - amount;
        Self {
            r: (self.r as f32 * factor) as u8,
            g: (self.g as f32 * factor) as u8,
            b: (self.b as f32 * factor) as u8,
            a: self.a,
        }
    }
}

    /// Format as hex string: "#RRGGBB" or "#RRGGBBAA" if alpha != 255.
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Convert to HSV color space (h: 0-360, s: 0-1, v: 0-1).
    pub fn to_hsv(&self) -> Hsv {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta < f32::EPSILON {
            0.0
        } else if (max - r).abs() < f32::EPSILON {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < f32::EPSILON {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let s = if max < f32::EPSILON { 0.0 } else { delta / max };
        let v = max;

        Hsv {
            h: if h < 0.0 { h + 360.0 } else { h },
            s,
            v,
        }
    }

    /// Create from HSV values (h: 0-360, s: 0.0-1.0, v: 0.0-1.0).
    pub fn from_hsv(hsv: &Hsv) -> Self {
        Self::from_hsva(hsv.h, hsv.s, hsv.v, 1.0)
    }

    /// Create from HSVA values (h: 0-360, s: 0.0-1.0, v: 0.0-1.0, a: 0.0-1.0).
    pub fn from_hsva(h: f32, s: f32, v: f32, a: f32) -> Self {
        let c = v * s;
        let h_prime = (h % 360.0) / 60.0;
        let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
        let m = v - c;

        let (r1, g1, b1) = if h_prime < 1.0 {
            (c, x, 0.0)
        } else if h_prime < 2.0 {
            (x, c, 0.0)
        } else if h_prime < 3.0 {
            (0.0, c, x)
        } else if h_prime < 4.0 {
            (0.0, x, c)
        } else if h_prime < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Self {
            r: ((r1 + m) * 255.0).round() as u8,
            g: ((g1 + m) * 255.0).round() as u8,
            b: ((b1 + m) * 255.0).round() as u8,
            a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }

    /// Convert to HSL color space (h: 0-360, s: 0-1, l: 0-1).
    pub fn to_hsl(&self) -> (f32, f32, f32) {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let l = (max + min) / 2.0;

        if delta < f32::EPSILON {
            return (0.0, 0.0, l);
        }

        let s = if l < 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let h = if (max - r).abs() < f32::EPSILON {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < f32::EPSILON {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let h = if h < 0.0 { h + 360.0 } else { h };
        (h, s, l)
    }

    /// Linearly interpolate between two colors (t: 0.0 = self, 1.0 = other).
    pub fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        Self {
            r: (self.r as f32 * inv + other.r as f32 * t).round() as u8,
            g: (self.g as f32 * inv + other.g as f32 * t).round() as u8,
            b: (self.b as f32 * inv + other.b as f32 * t).round() as u8,
            a: (self.a as f32 * inv + other.a as f32 * t).round() as u8,
        }
    }

    /// Compute perceived luminance (0.0-1.0) using sRGB coefficients.
    pub fn luminance(&self) -> f32 {
        0.2126 * (self.r as f32 / 255.0)
            + 0.7152 * (self.g as f32 / 255.0)
            + 0.0722 * (self.b as f32 / 255.0)
    }

    /// Returns true if the color is perceptually dark (luminance < 0.5).
    pub fn is_dark(&self) -> bool {
        self.luminance() < 0.5
    }

    /// Return a contrasting text color (white for dark backgrounds, black for light).
    pub fn contrast_text(&self) -> Self {
        if self.is_dark() { WHITE } else { BLACK }
    }
}

/// HSV (Hue, Saturation, Value) color representation.
/// Used internally by the color picker for intuitive color selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    /// Hue in degrees (0-360).
    pub h: f32,
    /// Saturation (0.0-1.0).
    pub s: f32,
    /// Value/brightness (0.0-1.0).
    pub v: f32,
}

impl Hsv {
    pub fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }

    /// Convert to RGB Color with full opacity.
    pub fn to_color(&self) -> Color {
        Color::from_hsv(self)
    }

    /// Convert to RGB Color with the given alpha (0.0-1.0).
    pub fn to_color_with_alpha(&self, a: f32) -> Color {
        Color::from_hsva(self.h, self.s, self.v, a)
    }

    /// Get the pure hue color (full saturation and value).
    pub fn hue_color(&self) -> Color {
        Color::from_hsva(self.h, 1.0, 1.0, 1.0)
    }
}

pub const TRANSPARENT: Color = rgba(0, 0, 0, 0);
pub const WHITE: Color = rgba(255, 255, 255, 255);
pub const BLACK: Color = rgba(0, 0, 0, 255);
