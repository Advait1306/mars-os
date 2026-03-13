use crate::color::Color;

/// Centralized theme for all UI elements.
///
/// Form elements and widgets read colors, sizes, and styling from the theme
/// instead of using hardcoded values. Views can override the theme by
/// implementing `View::theme()`.
#[derive(Debug, Clone)]
pub struct Theme {
    // Backgrounds
    pub input_bg: Color,
    pub input_bg_hover: Color,
    pub input_bg_disabled: Color,

    // Borders
    pub input_border: Color,
    pub input_border_hover: Color,
    pub input_border_focus: Color,
    pub input_border_error: Color,

    // Primary action color (buttons, selections, focus rings)
    pub primary: Color,
    pub primary_hover: Color,
    pub primary_active: Color,

    // Danger
    pub danger: Color,
    pub danger_hover: Color,

    // Text
    pub text: Color,
    pub text_secondary: Color,
    pub text_placeholder: Color,
    pub text_disabled: Color,
    pub text_error: Color,

    // Selection
    pub selection_bg: Color,

    // Focus ring
    pub focus_ring: Color,
    pub focus_ring_width: f32,

    // Slider
    pub slider_track: Color,
    pub slider_track_fill: Color,
    pub slider_thumb: Color,

    // Switch
    pub switch_track_off: Color,
    pub switch_track_on: Color,
    pub switch_thumb: Color,

    // Checkbox/Radio
    pub check_border: Color,
    pub check_fill: Color,
    pub check_mark: Color,

    // Progress
    pub progress_track: Color,
    pub progress_fill: Color,

    // Popup/dropdown
    pub popup_bg: Color,
    pub popup_border: Color,
    pub popup_shadow: Color,
    pub option_hover_bg: Color,

    // Fonts
    pub font_size: f32,
    pub font_size_small: f32,
    pub font_size_large: f32,

    // Sizing
    pub input_height: f32,
    pub input_corner_radius: f32,
    pub input_padding_x: f32,
    pub input_padding_y: f32,
    pub checkbox_size: f32,
    pub radio_size: f32,
    pub switch_width: f32,
    pub switch_height: f32,
    pub slider_track_height: f32,
    pub slider_thumb_radius: f32,
}

impl Theme {
    /// Dark theme (default).
    pub fn dark() -> Self {
        Self {
            // Backgrounds
            input_bg: Color::new(30, 30, 34, 255),
            input_bg_hover: Color::new(40, 40, 40, 255),
            input_bg_disabled: Color::new(20, 20, 20, 255),

            // Borders
            input_border: Color::new(80, 80, 80, 255),
            input_border_hover: Color::new(100, 100, 100, 255),
            input_border_focus: Color::new(66, 133, 244, 255),
            input_border_error: Color::new(220, 53, 69, 255),

            // Primary
            primary: Color::new(66, 133, 244, 255),
            primary_hover: Color::new(85, 148, 255, 255),
            primary_active: Color::new(50, 115, 220, 255),

            // Danger
            danger: Color::new(220, 53, 69, 255),
            danger_hover: Color::new(235, 70, 85, 255),

            // Text
            text: Color::new(255, 255, 255, 255),
            text_secondary: Color::new(180, 180, 180, 255),
            text_placeholder: Color::new(160, 160, 160, 255),
            text_disabled: Color::new(120, 120, 120, 255),
            text_error: Color::new(220, 53, 69, 255),

            // Selection
            selection_bg: Color::new(66, 133, 244, 80),

            // Focus ring
            focus_ring: Color::new(66, 133, 244, 200),
            focus_ring_width: 2.0,

            // Slider
            slider_track: Color::new(80, 80, 80, 255),
            slider_track_fill: Color::new(66, 133, 244, 255),
            slider_thumb: Color::new(255, 255, 255, 255),

            // Switch
            switch_track_off: Color::new(80, 80, 80, 255),
            switch_track_on: Color::new(66, 133, 244, 255),
            switch_thumb: Color::new(255, 255, 255, 255),

            // Checkbox/Radio
            check_border: Color::new(120, 120, 120, 255),
            check_fill: Color::new(66, 133, 244, 255),
            check_mark: Color::new(255, 255, 255, 255),

            // Progress
            progress_track: Color::new(80, 80, 80, 255),
            progress_fill: Color::new(66, 133, 244, 255),

            // Popup/dropdown
            popup_bg: Color::new(38, 38, 42, 255),
            popup_border: Color::new(60, 60, 60, 255),
            popup_shadow: Color::new(0, 0, 0, 80),
            option_hover_bg: Color::new(55, 55, 60, 255),

            // Fonts
            font_size: 14.0,
            font_size_small: 12.0,
            font_size_large: 16.0,

            // Sizing
            input_height: 36.0,
            input_corner_radius: 6.0,
            input_padding_x: 12.0,
            input_padding_y: 8.0,
            checkbox_size: 18.0,
            radio_size: 18.0,
            switch_width: 44.0,
            switch_height: 24.0,
            slider_track_height: 4.0,
            slider_thumb_radius: 8.0,
        }
    }

    /// Light theme.
    pub fn light() -> Self {
        Self {
            // Backgrounds
            input_bg: Color::new(255, 255, 255, 255),
            input_bg_hover: Color::new(245, 245, 245, 255),
            input_bg_disabled: Color::new(240, 240, 240, 255),

            // Borders
            input_border: Color::new(200, 200, 200, 255),
            input_border_hover: Color::new(170, 170, 170, 255),
            input_border_focus: Color::new(66, 133, 244, 255),
            input_border_error: Color::new(220, 53, 69, 255),

            // Primary
            primary: Color::new(66, 133, 244, 255),
            primary_hover: Color::new(85, 148, 255, 255),
            primary_active: Color::new(50, 115, 220, 255),

            // Danger
            danger: Color::new(220, 53, 69, 255),
            danger_hover: Color::new(235, 70, 85, 255),

            // Text
            text: Color::new(0, 0, 0, 255),
            text_secondary: Color::new(100, 100, 100, 255),
            text_placeholder: Color::new(150, 150, 150, 255),
            text_disabled: Color::new(180, 180, 180, 255),
            text_error: Color::new(220, 53, 69, 255),

            // Selection
            selection_bg: Color::new(66, 133, 244, 60),

            // Focus ring
            focus_ring: Color::new(66, 133, 244, 200),
            focus_ring_width: 2.0,

            // Slider
            slider_track: Color::new(220, 220, 220, 255),
            slider_track_fill: Color::new(66, 133, 244, 255),
            slider_thumb: Color::new(255, 255, 255, 255),

            // Switch
            switch_track_off: Color::new(200, 200, 200, 255),
            switch_track_on: Color::new(66, 133, 244, 255),
            switch_thumb: Color::new(255, 255, 255, 255),

            // Checkbox/Radio
            check_border: Color::new(180, 180, 180, 255),
            check_fill: Color::new(66, 133, 244, 255),
            check_mark: Color::new(255, 255, 255, 255),

            // Progress
            progress_track: Color::new(220, 220, 220, 255),
            progress_fill: Color::new(66, 133, 244, 255),

            // Popup/dropdown
            popup_bg: Color::new(255, 255, 255, 255),
            popup_border: Color::new(220, 220, 220, 255),
            popup_shadow: Color::new(0, 0, 0, 40),
            option_hover_bg: Color::new(240, 240, 245, 255),

            // Fonts
            font_size: 14.0,
            font_size_small: 12.0,
            font_size_large: 16.0,

            // Sizing
            input_height: 36.0,
            input_corner_radius: 6.0,
            input_padding_x: 12.0,
            input_padding_y: 8.0,
            checkbox_size: 18.0,
            radio_size: 18.0,
            switch_width: 44.0,
            switch_height: 24.0,
            slider_track_height: 4.0,
            slider_thumb_radius: 8.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
