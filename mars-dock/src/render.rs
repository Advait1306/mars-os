//! Software rendering for the dock using tiny-skia

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::windows::DockApp;

const ICON_SIZE: u32 = 44;
const ICON_PADDING: u32 = 8;
const DOCK_PADDING: u32 = 12;
const DOCK_HEIGHT: u32 = 64;
const DOCK_RADIUS: f32 = 18.0;
const DOT_RADIUS: f32 = 2.5;

fn bg_color() -> Color { Color::from_rgba8(30, 30, 30, 190) }
fn border_color() -> Color { Color::from_rgba8(255, 255, 255, 30) }
fn dot_color() -> Color { Color::from_rgba8(255, 255, 255, 220) }

/// Calculate the dock width based on number of apps
pub fn dock_width(app_count: usize) -> u32 {
    if app_count == 0 {
        return 200; // minimum width for empty state
    }
    let icons_width = app_count as u32 * ICON_SIZE + (app_count.saturating_sub(1)) as u32 * ICON_PADDING;
    icons_width + DOCK_PADDING * 2
}

pub fn dock_height() -> u32 {
    DOCK_HEIGHT
}

/// Render the dock to a pixel buffer
pub fn render_dock(apps: &[DockApp], width: u32, height: u32, icon_pixmaps: &[Option<Pixmap>]) -> Pixmap {
    let mut pixmap = Pixmap::new(width, height).unwrap();

    // Draw background rounded rectangle
    draw_rounded_rect(&mut pixmap, 0.0, 0.0, width as f32, height as f32, DOCK_RADIUS, bg_color());
    draw_rounded_rect_stroke(&mut pixmap, 0.5, 0.5, width as f32 - 1.0, height as f32 - 1.0, DOCK_RADIUS, border_color());

    if apps.is_empty() {
        // Empty state — just show the background
        return pixmap;
    }

    // Draw each app icon
    let total_icons_width = apps.len() as u32 * ICON_SIZE + (apps.len().saturating_sub(1)) as u32 * ICON_PADDING;
    let start_x = (width - total_icons_width) / 2;

    for (i, app) in apps.iter().enumerate() {
        let x = start_x + i as u32 * (ICON_SIZE + ICON_PADDING);
        let y = (height - ICON_SIZE) / 2 - 2; // slight offset up for dot

        // Draw icon
        if let Some(Some(icon_pixmap)) = icon_pixmaps.get(i) {
            // Scale and draw the icon pixmap
            let scaled = scale_pixmap(icon_pixmap, ICON_SIZE, ICON_SIZE);
            pixmap.draw_pixmap(
                x as i32,
                y as i32,
                scaled.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        } else {
            // Draw a placeholder circle
            draw_placeholder_icon(&mut pixmap, x as f32, y as f32, ICON_SIZE as f32);
        }

        // Draw active indicator dot
        if app.is_active {
            let dot_x = x as f32 + ICON_SIZE as f32 / 2.0;
            let dot_y = height as f32 - 6.0;
            draw_circle(&mut pixmap, dot_x, dot_y, DOT_RADIUS, dot_color());
        }
    }

    pixmap
}

fn draw_rounded_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let mut pb = PathBuilder::new();
    rounded_rect_path(&mut pb, x, y, w, h, r);
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_rounded_rect_stroke(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let mut pb = PathBuilder::new();
    rounded_rect_path(&mut pb, x, y, w, h, r);
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        let stroke = tiny_skia::Stroke {
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
    // Approximate circle with cubic beziers
    let k = 0.5522848; // magic number for cubic bezier circle approximation
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

fn draw_placeholder_icon(pixmap: &mut Pixmap, x: f32, y: f32, size: f32) {
    let cx = x + size / 2.0;
    let cy = y + size / 2.0;
    let r = size / 2.0 - 2.0;
    draw_circle(pixmap, cx, cy, r, Color::from_rgba8(80, 80, 80, 200));
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

/// Load an icon from the freedesktop icon theme
pub fn load_icon(icon_name: &str) -> Option<Pixmap> {
    // Try to find the icon file
    let icon_path = find_icon_path(icon_name)?;
    load_icon_file(&icon_path)
}

fn find_icon_path(icon_name: &str) -> Option<String> {
    // If it's already an absolute path
    if icon_name.starts_with('/') {
        if std::path::Path::new(icon_name).exists() {
            return Some(icon_name.to_string());
        }
    }

    // Search common icon theme directories
    let sizes = ["48x48", "64x64", "scalable", "256x256", "128x128", "32x32"];
    let categories = ["apps", "applications"];
    let themes = ["hicolor", "breeze-dark", "breeze"];
    let base_dirs = [
        "/usr/share/icons",
        "/usr/local/share/icons",
    ];

    for base in &base_dirs {
        for theme in &themes {
            for size in &sizes {
                for cat in &categories {
                    // Try SVG first
                    let svg_path = format!("{}/{}/{}/{}/{}.svg", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&svg_path).exists() {
                        return Some(svg_path);
                    }
                    // Then PNG
                    let png_path = format!("{}/{}/{}/{}/{}.png", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&png_path).exists() {
                        return Some(png_path);
                    }
                }
            }
        }
    }

    // Try pixmaps directory
    let pixmap_path = format!("/usr/share/pixmaps/{}.png", icon_name);
    if std::path::Path::new(&pixmap_path).exists() {
        return Some(pixmap_path);
    }
    let pixmap_svg_path = format!("/usr/share/pixmaps/{}.svg", icon_name);
    if std::path::Path::new(&pixmap_svg_path).exists() {
        return Some(pixmap_svg_path);
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
    // tiny-skia expects premultiplied alpha in RGBA order
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
