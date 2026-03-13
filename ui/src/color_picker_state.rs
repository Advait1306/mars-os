/// State management for the ColorPicker element.
///
/// Tracks the current HSV selection, alpha, hex input, and which sub-control
/// (SV gradient, hue slider, alpha slider) is being dragged. All color math
/// is pure Rust with no Wayland dependencies — popup surface creation is
/// handled separately.
use crate::color::{Color, Hsv};

/// Which part of the color picker is currently being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPickerDrag {
    /// Dragging on the saturation/value gradient square.
    SvGradient,
    /// Dragging on the hue slider bar.
    HueSlider,
    /// Dragging on the alpha slider bar.
    AlphaSlider,
}

/// State for a ColorPicker element.
#[derive(Debug, Clone)]
pub struct ColorPickerState {
    /// Whether the picker popup is open.
    pub open: bool,
    /// Current HSV values.
    pub hsv: Hsv,
    /// Current alpha (0.0-1.0).
    pub alpha: f32,
    /// Which control is being dragged, if any.
    pub dragging: Option<ColorPickerDrag>,
    /// Hex input text (user-editable, may be invalid mid-edit).
    pub hex_input: String,
    /// Whether the hex input field is focused.
    pub hex_focused: bool,
}

impl ColorPickerState {
    /// Create a new state from an initial color.
    pub fn new(color: Color) -> Self {
        let hsv = color.to_hsv();
        let alpha = color.a as f32 / 255.0;
        Self {
            open: false,
            hsv,
            alpha,
            dragging: None,
            hex_input: color.to_hex(),
            hex_focused: false,
        }
    }

    /// Get the currently selected color (RGBA).
    pub fn color(&self) -> Color {
        self.hsv.to_color_with_alpha(self.alpha)
    }

    /// Toggle the popup open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.dragging = None;
            self.hex_focused = false;
        }
    }

    /// Open the popup.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Close the popup and stop any drag.
    pub fn close(&mut self) {
        self.open = false;
        self.dragging = None;
        self.hex_focused = false;
    }

    /// Set the color from an external source (e.g. the controlled `value` prop changed).
    pub fn set_color(&mut self, color: Color) {
        self.hsv = color.to_hsv();
        self.alpha = color.a as f32 / 255.0;
        self.sync_hex();
    }

    // --- SV Gradient ---

    /// Update saturation and value from a position on the SV gradient square.
    /// `x` and `y` are normalized to 0.0-1.0 within the gradient bounds.
    /// x = saturation (left=0, right=1), y = value (top=1, bottom=0).
    pub fn set_sv(&mut self, x: f32, y: f32) {
        self.hsv.s = x.clamp(0.0, 1.0);
        self.hsv.v = (1.0 - y).clamp(0.0, 1.0);
        self.sync_hex();
    }

    /// Begin dragging on the SV gradient.
    pub fn start_sv_drag(&mut self, x: f32, y: f32) {
        self.dragging = Some(ColorPickerDrag::SvGradient);
        self.set_sv(x, y);
    }

    // --- Hue Slider ---

    /// Set the hue from a normalized position (0.0-1.0) on the hue slider.
    pub fn set_hue(&mut self, t: f32) {
        self.hsv.h = (t.clamp(0.0, 1.0) * 360.0) % 360.0;
        self.sync_hex();
    }

    /// Begin dragging on the hue slider.
    pub fn start_hue_drag(&mut self, t: f32) {
        self.dragging = Some(ColorPickerDrag::HueSlider);
        self.set_hue(t);
    }

    // --- Alpha Slider ---

    /// Set the alpha from a normalized position (0.0-1.0) on the alpha slider.
    pub fn set_alpha(&mut self, t: f32) {
        self.alpha = t.clamp(0.0, 1.0);
        self.sync_hex();
    }

    /// Begin dragging on the alpha slider.
    pub fn start_alpha_drag(&mut self, t: f32) {
        self.dragging = Some(ColorPickerDrag::AlphaSlider);
        self.set_alpha(t);
    }

    // --- Drag continuation ---

    /// Continue a drag at the given normalized position.
    /// For SV gradient, pass (x, y). For hue/alpha sliders, only `x` is used.
    pub fn drag_move(&mut self, x: f32, y: f32) {
        match self.dragging {
            Some(ColorPickerDrag::SvGradient) => self.set_sv(x, y),
            Some(ColorPickerDrag::HueSlider) => self.set_hue(x),
            Some(ColorPickerDrag::AlphaSlider) => self.set_alpha(x),
            None => {}
        }
    }

    /// End any active drag.
    pub fn end_drag(&mut self) {
        self.dragging = None;
    }

    // --- Hex Input ---

    /// Update the hex input text. If it's a valid hex color, apply it.
    pub fn set_hex_input(&mut self, text: String) {
        self.hex_input = text;
        if let Some(color) = parse_hex_color(&self.hex_input) {
            self.hsv = color.to_hsv();
            self.alpha = color.a as f32 / 255.0;
        }
    }

    /// Commit the hex input: if valid, apply; if invalid, revert to current color.
    pub fn commit_hex(&mut self) {
        if let Some(color) = parse_hex_color(&self.hex_input) {
            self.hsv = color.to_hsv();
            self.alpha = color.a as f32 / 255.0;
        }
        // Always sync hex back to the canonical form
        self.sync_hex();
    }

    /// Sync the hex input string to reflect the current color.
    fn sync_hex(&mut self) {
        if !self.hex_focused {
            self.hex_input = self.color().to_hex();
        }
    }

    /// Get the pure hue color (for rendering the SV gradient background).
    pub fn hue_color(&self) -> Color {
        self.hsv.hue_color()
    }

    /// Get the SV indicator position as (x, y) normalized 0.0-1.0.
    pub fn sv_position(&self) -> (f32, f32) {
        (self.hsv.s, 1.0 - self.hsv.v)
    }

    /// Get the hue slider position as normalized 0.0-1.0.
    pub fn hue_position(&self) -> f32 {
        self.hsv.h / 360.0
    }

    /// Get the alpha slider position as normalized 0.0-1.0.
    pub fn alpha_position(&self) -> f32 {
        self.alpha
    }
}

impl Default for ColorPickerState {
    fn default() -> Self {
        Self::new(Color::new(255, 0, 0, 255))
    }
}

/// Parse a hex color string into a Color.
/// Supports: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (with or without `#`).
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#').unwrap_or(s);
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()?;
            let g = u8::from_str_radix(&s[1..2], 16).ok()?;
            let b = u8::from_str_radix(&s[2..3], 16).ok()?;
            Some(Color::new(r * 17, g * 17, b * 17, 255))
        }
        4 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()?;
            let g = u8::from_str_radix(&s[1..2], 16).ok()?;
            let b = u8::from_str_radix(&s[2..3], 16).ok()?;
            let a = u8::from_str_radix(&s[3..4], 16).ok()?;
            Some(Color::new(r * 17, g * 17, b * 17, a * 17))
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color::new(r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some(Color::new(r, g, b, a))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    #[test]
    fn test_new_from_red() {
        let state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        assert!(!state.open);
        assert_eq!(state.dragging, None);
        assert!((state.hsv.h - 0.0).abs() < 1.0); // hue ~0 for red
        assert!((state.hsv.s - 1.0).abs() < 0.01);
        assert!((state.hsv.v - 1.0).abs() < 0.01);
        assert!((state.alpha - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_new_from_semi_transparent_blue() {
        let state = ColorPickerState::new(Color::new(0, 0, 255, 128));
        assert!((state.hsv.h - 240.0).abs() < 1.0);
        assert!((state.alpha - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_color_roundtrip() {
        let original = Color::new(100, 150, 200, 180);
        let state = ColorPickerState::new(original);
        let result = state.color();
        // Allow small rounding differences
        assert!((result.r as i32 - original.r as i32).abs() <= 1);
        assert!((result.g as i32 - original.g as i32).abs() <= 1);
        assert!((result.b as i32 - original.b as i32).abs() <= 1);
        assert!((result.a as i32 - original.a as i32).abs() <= 1);
    }

    #[test]
    fn test_toggle() {
        let mut state = ColorPickerState::default();
        assert!(!state.open);
        state.toggle();
        assert!(state.open);
        state.dragging = Some(ColorPickerDrag::HueSlider);
        state.toggle();
        assert!(!state.open);
        assert_eq!(state.dragging, None);
    }

    #[test]
    fn test_set_sv() {
        let mut state = ColorPickerState::default();
        state.set_sv(0.5, 0.3);
        assert!((state.hsv.s - 0.5).abs() < 0.001);
        assert!((state.hsv.v - 0.7).abs() < 0.001); // y=0.3 → v=0.7
    }

    #[test]
    fn test_set_sv_clamps() {
        let mut state = ColorPickerState::default();
        state.set_sv(-0.5, 1.5);
        assert_eq!(state.hsv.s, 0.0);
        assert_eq!(state.hsv.v, 0.0); // y=1.5 clamped to 1.0, v=0.0
        state.set_sv(2.0, -1.0);
        assert_eq!(state.hsv.s, 1.0);
        assert_eq!(state.hsv.v, 1.0); // y=-1.0 clamped to 0.0, v=1.0
    }

    #[test]
    fn test_set_hue() {
        let mut state = ColorPickerState::default();
        state.set_hue(0.5);
        assert!((state.hsv.h - 180.0).abs() < 0.1);
        state.set_hue(0.0);
        assert!((state.hsv.h - 0.0).abs() < 0.1);
        state.set_hue(1.0);
        // hue 360 wraps to 0
        assert!((state.hsv.h % 360.0).abs() < 0.1);
    }

    #[test]
    fn test_set_alpha() {
        let mut state = ColorPickerState::default();
        state.set_alpha(0.5);
        assert!((state.alpha - 0.5).abs() < 0.001);
        state.set_alpha(-1.0);
        assert_eq!(state.alpha, 0.0);
        state.set_alpha(2.0);
        assert_eq!(state.alpha, 1.0);
    }

    #[test]
    fn test_drag_sv() {
        let mut state = ColorPickerState::default();
        state.start_sv_drag(0.2, 0.8);
        assert_eq!(state.dragging, Some(ColorPickerDrag::SvGradient));
        assert!((state.hsv.s - 0.2).abs() < 0.001);

        state.drag_move(0.9, 0.1);
        assert!((state.hsv.s - 0.9).abs() < 0.001);
        assert!((state.hsv.v - 0.9).abs() < 0.001);

        state.end_drag();
        assert_eq!(state.dragging, None);
    }

    #[test]
    fn test_drag_hue() {
        let mut state = ColorPickerState::default();
        state.start_hue_drag(0.33);
        assert_eq!(state.dragging, Some(ColorPickerDrag::HueSlider));
        assert!((state.hsv.h - 118.8).abs() < 1.0);

        state.drag_move(0.66, 0.0); // y ignored for hue
        assert!((state.hsv.h - 237.6).abs() < 1.0);

        state.end_drag();
        assert_eq!(state.dragging, None);
    }

    #[test]
    fn test_drag_alpha() {
        let mut state = ColorPickerState::default();
        state.start_alpha_drag(0.75);
        assert_eq!(state.dragging, Some(ColorPickerDrag::AlphaSlider));
        assert!((state.alpha - 0.75).abs() < 0.001);

        state.drag_move(0.25, 0.0);
        assert!((state.alpha - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_drag_move_no_drag() {
        let mut state = ColorPickerState::default();
        let s_before = state.hsv.s;
        state.drag_move(0.5, 0.5); // no drag active, should be no-op
        assert_eq!(state.hsv.s, s_before);
    }

    #[test]
    fn test_sv_position() {
        let mut state = ColorPickerState::default();
        state.hsv.s = 0.3;
        state.hsv.v = 0.8;
        let (x, y) = state.sv_position();
        assert!((x - 0.3).abs() < 0.001);
        assert!((y - 0.2).abs() < 0.001); // y = 1.0 - v
    }

    #[test]
    fn test_hue_position() {
        let mut state = ColorPickerState::default();
        state.hsv.h = 120.0;
        assert!((state.hue_position() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_alpha_position() {
        let mut state = ColorPickerState::default();
        state.alpha = 0.6;
        assert!((state.alpha_position() - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_hue_color() {
        let mut state = ColorPickerState::default();
        state.hsv = Hsv::new(120.0, 0.3, 0.5); // green hue, low sat/val
        let hue_c = state.hue_color();
        // Pure green at full sat/val
        assert_eq!(hue_c.r, 0);
        assert_eq!(hue_c.g, 255);
        assert_eq!(hue_c.b, 0);
    }

    #[test]
    fn test_set_color_external() {
        let mut state = ColorPickerState::default();
        let new_color = Color::new(0, 128, 255, 200);
        state.set_color(new_color);
        let result = state.color();
        assert!((result.r as i32 - 0).abs() <= 1);
        assert!((result.g as i32 - 128).abs() <= 1);
        assert!((result.b as i32 - 255).abs() <= 1);
        assert!((result.a as i32 - 200).abs() <= 1);
    }

    // --- Hex parsing tests ---

    #[test]
    fn test_parse_hex_6_digit() {
        let c = parse_hex_color("#FF8000").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_parse_hex_8_digit() {
        let c = parse_hex_color("#FF800080").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 128);
    }

    #[test]
    fn test_parse_hex_3_digit() {
        let c = parse_hex_color("#F80").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 136);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_parse_hex_4_digit() {
        let c = parse_hex_color("#F80A").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 136);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 170);
    }

    #[test]
    fn test_parse_hex_no_hash() {
        let c = parse_hex_color("00FF00").unwrap();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_hex_lowercase() {
        let c = parse_hex_color("#ff8000").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert!(parse_hex_color("").is_none());
        assert!(parse_hex_color("#").is_none());
        assert!(parse_hex_color("#GG0000").is_none());
        assert!(parse_hex_color("#12345").is_none()); // 5 digits
        assert!(parse_hex_color("not a color").is_none());
    }

    #[test]
    fn test_hex_input_valid() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        state.set_hex_input("#00FF00".to_string());
        let c = state.color();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_hex_input_invalid_preserves_color() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        let before = state.color();
        state.set_hex_input("invalid".to_string());
        let after = state.color();
        assert_eq!(before.r, after.r);
        assert_eq!(before.g, after.g);
        assert_eq!(before.b, after.b);
    }

    #[test]
    fn test_commit_hex_valid() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        state.hex_input = "#0000FF".to_string();
        state.commit_hex();
        let c = state.color();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 255);
        assert_eq!(state.hex_input, "#0000FF");
    }

    #[test]
    fn test_commit_hex_invalid_reverts() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        state.hex_input = "garbage".to_string();
        state.commit_hex();
        // Color unchanged, hex reverts to canonical
        assert_eq!(state.color().r, 255);
        assert_eq!(state.hex_input, "#FF0000");
    }

    #[test]
    fn test_hex_sync_on_sv_change() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        assert_eq!(state.hex_input, "#FF0000");
        state.set_sv(0.0, 0.0); // white (s=0, v=1)
        assert_eq!(state.hex_input, "#FFFFFF");
    }

    #[test]
    fn test_hex_no_sync_when_focused() {
        let mut state = ColorPickerState::new(Color::new(255, 0, 0, 255));
        state.hex_focused = true;
        state.hex_input = "#00F".to_string(); // user is typing
        state.set_hue(0.5); // change hue, but hex should NOT update while focused
        assert_eq!(state.hex_input, "#00F"); // preserved
    }

    #[test]
    fn test_close_clears_drag_and_focus() {
        let mut state = ColorPickerState::default();
        state.open = true;
        state.dragging = Some(ColorPickerDrag::SvGradient);
        state.hex_focused = true;
        state.close();
        assert!(!state.open);
        assert_eq!(state.dragging, None);
        assert!(!state.hex_focused);
    }

    #[test]
    fn test_default_is_red() {
        let state = ColorPickerState::default();
        let c = state.color();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 255);
    }
}
