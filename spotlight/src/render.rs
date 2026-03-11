use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const WINDOW_WIDTH: u32 = 600;
pub const SEARCH_HEIGHT: u32 = 56;
pub const RESULT_HEIGHT: u32 = 44;
pub const MAX_VISIBLE: usize = 8;
pub const CORNER_RADIUS: f32 = 12.0;
pub const INNER_RADIUS: f32 = 8.0;
pub const SIDE_PAD: f32 = 12.0;
pub const ICON_SIZE: u32 = 28;
pub const BOTTOM_PAD: u32 = 8;
pub const SEPARATOR_PAD: u32 = 4;

fn bg_color() -> Color {
    Color::from_rgba8(20, 20, 22, 230)
}
fn border_color() -> Color {
    Color::from_rgba8(255, 255, 255, 20)
}
fn search_box_color() -> Color {
    Color::from_rgba8(255, 255, 255, 8)
}
fn separator_color() -> Color {
    Color::from_rgba8(255, 255, 255, 15)
}
fn selection_color() -> Color {
    Color::from_rgba8(255, 255, 255, 12)
}

const QUERY_TEXT_COLOR: [u8; 4] = [255, 255, 255, 230];
const PLACEHOLDER_COLOR: [u8; 4] = [255, 255, 255, 60];
const RESULT_TEXT_COLOR: [u8; 4] = [255, 255, 255, 200];
const CURSOR_COLOR: [u8; 4] = [255, 255, 255, 180];

const QUERY_FONT_SIZE: f32 = 20.0;
const RESULT_FONT_SIZE: f32 = 15.0;

pub fn calc_height(result_count: usize) -> u32 {
    let visible = result_count.min(MAX_VISIBLE);
    if visible == 0 {
        SEARCH_HEIGHT + BOTTOM_PAD
    } else {
        SEARCH_HEIGHT + SEPARATOR_PAD + visible as u32 * RESULT_HEIGHT + BOTTOM_PAD
    }
}

pub struct ResultItem<'a> {
    pub name: &'a str,
    pub icon: Option<&'a Pixmap>,
}

pub fn render_spotlight(
    font: &fontdue::Font,
    query: &str,
    results: &[ResultItem],
    selected: usize,
    width: u32,
    height: u32,
) -> Pixmap {
    let mut pixmap = Pixmap::new(width, height).unwrap();

    // Background
    draw_rounded_rect(&mut pixmap, 0.0, 0.0, width as f32, height as f32, CORNER_RADIUS, bg_color());
    draw_rounded_rect_stroke(
        &mut pixmap,
        0.5,
        0.5,
        width as f32 - 1.0,
        height as f32 - 1.0,
        CORNER_RADIUS,
        border_color(),
    );

    // Search box inner background
    draw_rounded_rect(
        &mut pixmap,
        SIDE_PAD,
        10.0,
        width as f32 - SIDE_PAD * 2.0,
        36.0,
        INNER_RADIUS,
        search_box_color(),
    );

    // Query text or placeholder
    let text_x = SIDE_PAD + 12.0;
    let text_baseline = 34.0;
    if query.is_empty() {
        draw_text(
            &mut pixmap,
            font,
            "Search apps...",
            QUERY_FONT_SIZE,
            text_x,
            text_baseline,
            PLACEHOLDER_COLOR,
        );
    } else {
        draw_text(
            &mut pixmap,
            font,
            query,
            QUERY_FONT_SIZE,
            text_x,
            text_baseline,
            QUERY_TEXT_COLOR,
        );
    }

    // Cursor
    let cursor_x = text_x + text_width(font, query, QUERY_FONT_SIZE);
    draw_rect(
        &mut pixmap,
        cursor_x,
        16.0,
        2.0,
        24.0,
        CURSOR_COLOR,
    );

    // Results
    if !results.is_empty() {
        // Separator
        let sep_y = SEARCH_HEIGHT as f32;
        draw_rect(
            &mut pixmap,
            SIDE_PAD + 4.0,
            sep_y,
            width as f32 - (SIDE_PAD + 4.0) * 2.0,
            1.0,
            [255, 255, 255, 15],
        );

        let results_start = SEARCH_HEIGHT as f32 + SEPARATOR_PAD as f32;

        for (i, item) in results.iter().enumerate() {
            let y = results_start + i as f32 * RESULT_HEIGHT as f32;

            // Selection highlight
            if i == selected {
                draw_rounded_rect(
                    &mut pixmap,
                    8.0,
                    y + 2.0,
                    width as f32 - 16.0,
                    RESULT_HEIGHT as f32 - 4.0,
                    INNER_RADIUS,
                    selection_color(),
                );
            }

            // Icon
            let icon_x = 20.0;
            let icon_y = y + (RESULT_HEIGHT as f32 - ICON_SIZE as f32) / 2.0;
            if let Some(icon_pm) = item.icon {
                let scaled = scale_pixmap(icon_pm, ICON_SIZE, ICON_SIZE);
                pixmap.draw_pixmap(
                    icon_x as i32,
                    icon_y as i32,
                    scaled.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            } else {
                // Placeholder circle
                draw_circle(
                    &mut pixmap,
                    icon_x + ICON_SIZE as f32 / 2.0,
                    icon_y + ICON_SIZE as f32 / 2.0,
                    ICON_SIZE as f32 / 2.0 - 2.0,
                    Color::from_rgba8(60, 60, 60, 180),
                );
            }

            // App name
            let name_x = icon_x + ICON_SIZE as f32 + 12.0;
            let name_baseline = y + RESULT_HEIGHT as f32 / 2.0 + RESULT_FONT_SIZE / 3.0;
            draw_text(
                &mut pixmap,
                font,
                item.name,
                RESULT_FONT_SIZE,
                name_x,
                name_baseline,
                RESULT_TEXT_COLOR,
            );
        }
    }

    pixmap
}

// --- Text rendering ---

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
    panic!("No system font found. Install fonts-dejavu-core or similar.");
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

fn draw_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
    let color = Color::from_rgba8(color[0], color[1], color[2], color[3]);
    let mut paint = Paint::default();
    paint.set_color(color);
    let rect = tiny_skia::Rect::from_xywh(x, y, w, h);
    if let Some(rect) = rect {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

// --- Geometry helpers ---

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
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
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

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    let mut pb = PathBuilder::new();
    let k = 0.5522848;
    let kr = k * r;
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
    pb.cubic_to(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
    pb.cubic_to(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
    pb.cubic_to(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn scale_pixmap(src: &Pixmap, target_w: u32, target_h: u32) -> Pixmap {
    let mut dst = Pixmap::new(target_w, target_h).unwrap();
    let sx = target_w as f32 / src.width() as f32;
    let sy = target_h as f32 / src.height() as f32;
    dst.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::from_scale(sx, sy),
        None,
    );
    dst
}

// --- Icon loading ---

pub fn load_icon(icon_name: &str) -> Option<Pixmap> {
    let path = find_icon_path(icon_name)?;
    load_icon_file(&path)
}

fn find_icon_path(icon_name: &str) -> Option<String> {
    if icon_name.starts_with('/') {
        if std::path::Path::new(icon_name).exists() {
            return Some(icon_name.to_string());
        }
    }

    let sizes = ["48x48", "64x64", "scalable", "256x256", "128x128", "32x32"];
    let categories = ["apps", "applications"];
    let themes = ["hicolor", "breeze-dark", "breeze"];
    let base_dirs = ["/usr/share/icons", "/usr/local/share/icons"];

    for base in &base_dirs {
        for theme in &themes {
            for size in &sizes {
                for cat in &categories {
                    let svg = format!("{}/{}/{}/{}/{}.svg", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&svg).exists() {
                        return Some(svg);
                    }
                    let png = format!("{}/{}/{}/{}/{}.png", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&png).exists() {
                        return Some(png);
                    }
                }
            }
        }
    }

    let pixmap_png = format!("/usr/share/pixmaps/{}.png", icon_name);
    if std::path::Path::new(&pixmap_png).exists() {
        return Some(pixmap_png);
    }
    let pixmap_svg = format!("/usr/share/pixmaps/{}.svg", icon_name);
    if std::path::Path::new(&pixmap_svg).exists() {
        return Some(pixmap_svg);
    }

    None
}

fn load_icon_file(path: &str) -> Option<Pixmap> {
    if path.ends_with(".svg") {
        load_svg(path)
    } else {
        load_png(path)
    }
}

fn load_svg(path: &str) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).ok()?;
    let size = tree.size();
    let sx = ICON_SIZE as f32 / size.width();
    let sy = ICON_SIZE as f32 / size.height();
    let scale = sx.min(sy);
    let mut pixmap = Pixmap::new(ICON_SIZE, ICON_SIZE)?;
    let transform = Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

fn load_png(path: &str) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&data).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut pm = Pixmap::new(w, h)?;
    for (i, pixel) in rgba.pixels().enumerate() {
        let [r, g, b, a] = pixel.0;
        let a_f = a as f32 / 255.0;
        pm.data_mut()[i * 4] = (r as f32 * a_f) as u8;
        pm.data_mut()[i * 4 + 1] = (g as f32 * a_f) as u8;
        pm.data_mut()[i * 4 + 2] = (b as f32 * a_f) as u8;
        pm.data_mut()[i * 4 + 3] = a;
    }
    Some(pm)
}
