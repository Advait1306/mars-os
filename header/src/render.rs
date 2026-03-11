use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

// ---- Header bar constants ----
pub const BAR_HEIGHT: u32 = 20;
const FONT_SIZE: f32 = 11.0;
const ICON_SIZE: f32 = 12.0;
const RIGHT_PAD: f32 = 12.0;
const ITEM_GAP: f32 = 16.0;
const ICON_TEXT_GAP: f32 = 4.0;

// ---- Popup constants ----
pub const POPUP_WIDTH: u32 = 240;
pub const POPUP_HEIGHT: u32 = 48;
const POPUP_RADIUS: f32 = 8.0;
const POPUP_PAD: f32 = 14.0;
const POPUP_ICON_SIZE: f32 = 16.0;
const SLIDER_TRACK_H: f32 = 4.0;
const SLIDER_KNOB_R: f32 = 6.0;
const POPUP_TEXT_GAP: f32 = 10.0;

// ---- Colors ----
const TEXT_COLOR: [u8; 4] = [255, 255, 255, 200];
const DIM_COLOR: [u8; 4] = [255, 255, 255, 100];

fn bg_color() -> Color {
    Color::from_rgba8(20, 20, 22, 230)
}

// ---- Hit zones ----

/// X-coordinate ranges for interactive zones on the header bar.
pub struct HitZones {
    pub volume: (f32, f32),
    pub brightness: Option<(f32, f32)>,
}

// ---- Popup hit testing ----

fn slider_left() -> f32 {
    POPUP_PAD + POPUP_ICON_SIZE + POPUP_TEXT_GAP
}

fn slider_right() -> f32 {
    POPUP_WIDTH as f32 - POPUP_PAD - 48.0
}

pub fn slider_value_at(x: f32) -> f32 {
    let l = slider_left();
    let r = slider_right();
    ((x - l) / (r - l)).clamp(0.0, 1.0)
}

pub fn is_on_slider(x: f32, y: f32) -> bool {
    let cy = POPUP_HEIGHT as f32 / 2.0;
    x >= slider_left() - SLIDER_KNOB_R
        && x <= slider_right() + SLIDER_KNOB_R
        && y >= cy - SLIDER_KNOB_R * 2.0
        && y <= cy + SLIDER_KNOB_R * 2.0
}

pub fn is_on_mute_icon(x: f32, y: f32) -> bool {
    let cy = POPUP_HEIGHT as f32 / 2.0;
    x >= POPUP_PAD
        && x <= POPUP_PAD + POPUP_ICON_SIZE
        && y >= cy - POPUP_ICON_SIZE / 2.0
        && y <= cy + POPUP_ICON_SIZE / 2.0
}

// ---- Header bar rendering ----

pub fn render_header(
    font: &fontdue::Font,
    width: u32,
    time: &str,
    volume: f32,
    muted: bool,
    brightness: Option<f32>,
) -> (Pixmap, HitZones) {
    let mut pixmap = Pixmap::new(width, BAR_HEIGHT).unwrap();

    // Background
    if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, BAR_HEIGHT as f32) {
        let mut paint = Paint::default();
        paint.set_color(bg_color());
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    let baseline = 15.0;
    let mut x = width as f32 - RIGHT_PAD;

    // ---- Time (rightmost) ----
    let time_w = text_width(font, time, FONT_SIZE);
    x -= time_w;
    draw_text(&mut pixmap, font, time, FONT_SIZE, x, baseline, TEXT_COLOR);
    x -= ITEM_GAP;

    // ---- Volume ----
    let vol_text = if muted {
        "Muted".to_string()
    } else {
        format!("{}%", (volume * 100.0) as i32)
    };
    let vol_text_w = text_width(font, &vol_text, FONT_SIZE);
    let vol_right = x;
    x -= vol_text_w;
    let color = if muted { DIM_COLOR } else { TEXT_COLOR };
    draw_text(&mut pixmap, font, &vol_text, FONT_SIZE, x, baseline, color);
    x -= ICON_TEXT_GAP;
    x -= ICON_SIZE;
    draw_speaker_icon(
        &mut pixmap,
        x,
        (BAR_HEIGHT as f32 - ICON_SIZE) / 2.0,
        ICON_SIZE,
        muted,
    );
    let vol_left = x;
    x -= ITEM_GAP;

    // ---- Brightness (if available) ----
    let brightness_zone = if let Some(brt) = brightness {
        let brt_text = format!("{}%", (brt * 100.0) as i32);
        let brt_text_w = text_width(font, &brt_text, FONT_SIZE);
        let brt_right = x;
        x -= brt_text_w;
        draw_text(
            &mut pixmap,
            font,
            &brt_text,
            FONT_SIZE,
            x,
            baseline,
            TEXT_COLOR,
        );
        x -= ICON_TEXT_GAP;
        x -= ICON_SIZE;
        draw_sun_icon(
            &mut pixmap,
            x,
            (BAR_HEIGHT as f32 - ICON_SIZE) / 2.0,
            ICON_SIZE,
        );
        let brt_left = x;
        Some((brt_left, brt_right))
    } else {
        None
    };

    let zones = HitZones {
        volume: (vol_left, vol_right),
        brightness: brightness_zone,
    };

    (pixmap, zones)
}

// ---- Popup rendering ----

pub fn render_popup(
    font: &fontdue::Font,
    width: u32,
    height: u32,
    volume: f32,
    muted: bool,
) -> Pixmap {
    let mut pixmap = Pixmap::new(width, height).unwrap();

    // Background
    draw_rounded_rect(
        &mut pixmap,
        0.0,
        0.0,
        width as f32,
        height as f32,
        POPUP_RADIUS,
        bg_color(),
    );
    // Border
    draw_rounded_rect_stroke(
        &mut pixmap,
        0.5,
        0.5,
        width as f32 - 1.0,
        height as f32 - 1.0,
        POPUP_RADIUS,
        Color::from_rgba8(255, 255, 255, 20),
    );

    let cy = height as f32 / 2.0;

    // Speaker icon (mute toggle)
    draw_speaker_icon(
        &mut pixmap,
        POPUP_PAD,
        cy - POPUP_ICON_SIZE / 2.0,
        POPUP_ICON_SIZE,
        muted,
    );

    // Slider track
    let sl = slider_left();
    let sr = slider_right();
    let track_y = cy - SLIDER_TRACK_H / 2.0;

    // Track background
    draw_rounded_rect(
        &mut pixmap,
        sl,
        track_y,
        sr - sl,
        SLIDER_TRACK_H,
        2.0,
        Color::from_rgba8(255, 255, 255, 30),
    );

    // Track fill
    let fill_w = (sr - sl) * volume;
    if fill_w > 1.0 {
        draw_rounded_rect(
            &mut pixmap,
            sl,
            track_y,
            fill_w,
            SLIDER_TRACK_H,
            2.0,
            Color::from_rgba8(255, 255, 255, 180),
        );
    }

    // Knob
    let knob_x = sl + (sr - sl) * volume;
    draw_circle(
        &mut pixmap,
        knob_x,
        cy,
        SLIDER_KNOB_R,
        Color::from_rgba8(255, 255, 255, 230),
    );

    // Volume percentage
    let vol_text = format!("{}%", (volume * 100.0) as i32);
    let text_x = sr + POPUP_TEXT_GAP;
    let baseline = cy + FONT_SIZE / 3.0;
    draw_text(
        &mut pixmap,
        font,
        &vol_text,
        FONT_SIZE,
        text_x,
        baseline,
        TEXT_COLOR,
    );

    pixmap
}

// ---- Icon drawing ----

fn draw_speaker_icon(pixmap: &mut Pixmap, x: f32, y: f32, size: f32, muted: bool) {
    let alpha = if muted { 100u8 } else { 200u8 };
    let color = Color::from_rgba8(255, 255, 255, alpha);
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;

    let cx = x + size / 2.0;
    let cy = y + size / 2.0;
    let s = size * 0.4;

    // Speaker body
    let mut pb = PathBuilder::new();
    pb.move_to(cx - s * 0.8, cy - s * 0.35);
    pb.line_to(cx - s * 0.3, cy - s * 0.35);
    pb.line_to(cx + s * 0.2, cy - s * 0.8);
    pb.line_to(cx + s * 0.2, cy + s * 0.8);
    pb.line_to(cx - s * 0.3, cy + s * 0.35);
    pb.line_to(cx - s * 0.8, cy + s * 0.35);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    if !muted {
        let stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        let wave_cx = cx + s * 0.2;
        for i in 1..=2 {
            let r = s * 0.35 * i as f32;
            let mut pb = PathBuilder::new();
            let steps = 10;
            let angle = std::f32::consts::FRAC_PI_4;
            for j in 0..=steps {
                let t = -angle + 2.0 * angle * j as f32 / steps as f32;
                let px = wave_cx + r * t.cos();
                let py = cy + r * t.sin();
                if j == 0 {
                    pb.move_to(px, py);
                } else {
                    pb.line_to(px, py);
                }
            }
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &stroke,
                    Transform::identity(),
                    None,
                );
            }
        }
    } else {
        let stroke = Stroke {
            width: 1.2,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.move_to(cx + s * 0.3, cy - s * 0.5);
        pb.line_to(cx + s * 0.9, cy + s * 0.5);
        pb.move_to(cx + s * 0.9, cy - s * 0.5);
        pb.line_to(cx + s * 0.3, cy + s * 0.5);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
}

fn draw_sun_icon(pixmap: &mut Pixmap, x: f32, y: f32, size: f32) {
    let color = Color::from_rgba8(255, 255, 255, 200);
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;

    let cx = x + size / 2.0;
    let cy = y + size / 2.0;
    let body_r = size * 0.18;
    let ray_inner = size * 0.28;
    let ray_outer = size * 0.44;

    draw_circle(pixmap, cx, cy, body_r, color);

    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    for i in 0..8 {
        let angle = std::f32::consts::PI * 2.0 * i as f32 / 8.0;
        let mut pb = PathBuilder::new();
        pb.move_to(cx + ray_inner * angle.cos(), cy + ray_inner * angle.sin());
        pb.line_to(cx + ray_outer * angle.cos(), cy + ray_outer * angle.sin());
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

// ---- Geometry helpers ----

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    let k = 0.5522848 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_rounded_rect(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
) {
    let mut pb = PathBuilder::new();
    rounded_rect_path(&mut pb, x, y, w, h, r);
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_rounded_rect_stroke(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
) {
    let mut pb = PathBuilder::new();
    rounded_rect_path(&mut pb, x, y, w, h, r);
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        let stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn rounded_rect_path(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w / 2.0).min(h / 2.0);
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
}

// ---- Text rendering ----

pub fn load_font() -> fontdue::Font {
    let paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    for path in &paths {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = fontdue::Font::from_bytes(data, fontdue::FontSettings::default()) {
                return font;
            }
        }
    }
    panic!("No system font found");
}

pub fn text_width(font: &fontdue::Font, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| font.metrics(c, size).advance_width)
        .sum()
}

fn draw_text(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    text: &str,
    size: f32,
    x: f32,
    baseline_y: f32,
    color: [u8; 4],
) {
    let pm_width = pixmap.width() as i32;
    let pm_height = pixmap.height() as i32;
    let stride = pm_width as usize;
    let color_r = color[0] as f32;
    let color_g = color[1] as f32;
    let color_b = color[2] as f32;
    let color_a = color[3] as f32 / 255.0;

    let mut pen_x = x;

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);

        let glyph_x = pen_x as i32 + metrics.xmin;
        let glyph_y = baseline_y as i32 - metrics.height as i32 - metrics.ymin;

        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let px = glyph_x + col as i32;
                let py = glyph_y + row as i32;

                if px < 0 || py < 0 || px >= pm_width || py >= pm_height {
                    continue;
                }

                let coverage = bitmap[row * metrics.width + col] as f32 / 255.0;
                if coverage < 0.004 {
                    continue;
                }

                let alpha = color_a * coverage;
                let inv_alpha = 1.0 - alpha;
                let idx = (py as usize * stride + px as usize) * 4;
                let data = pixmap.data_mut();

                data[idx] =
                    (color_r * alpha + data[idx] as f32 * inv_alpha).min(255.0) as u8;
                data[idx + 1] =
                    (color_g * alpha + data[idx + 1] as f32 * inv_alpha).min(255.0) as u8;
                data[idx + 2] =
                    (color_b * alpha + data[idx + 2] as f32 * inv_alpha).min(255.0) as u8;
                data[idx + 3] =
                    (alpha * 255.0 + data[idx + 3] as f32 * inv_alpha).min(255.0) as u8;
            }
        }

        pen_x += metrics.advance_width;
    }
}
