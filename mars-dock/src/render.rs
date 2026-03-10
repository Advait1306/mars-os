//! Software rendering for the dock using tiny-skia

use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Transform};

use crate::windows::DockApp;

pub const ICON_SIZE: u32 = 44;
pub const ICON_PADDING: u32 = 8;
pub const DOCK_PADDING: u32 = 12;
pub const DOCK_HEIGHT: u32 = 64;
const DOCK_RADIUS: f32 = 18.0;
const DOT_RADIUS: f32 = 2.5;

fn bg_color() -> Color {
    Color::from_rgba8(30, 30, 30, 190)
}
fn border_color() -> Color {
    Color::from_rgba8(255, 255, 255, 30)
}
fn dot_color() -> Color {
    Color::from_rgba8(255, 255, 255, 220)
}

/// Calculate the dock width based on number of apps
pub fn dock_width(app_count: usize) -> u32 {
    if app_count == 0 {
        return 200; // minimum width for empty state
    }
    let icons_width =
        app_count as u32 * ICON_SIZE + (app_count.saturating_sub(1)) as u32 * ICON_PADDING;
    icons_width + DOCK_PADDING * 2
}

pub fn dock_height() -> u32 {
    DOCK_HEIGHT
}

/// Per-icon render state passed from the animation system
pub struct RenderSlot<'a> {
    pub app: &'a DockApp,
    pub icon: Option<&'a Pixmap>,
    pub y_offset: f32, // pixels below normal position (0 = resting)
    pub show_dot: bool,
}

/// Render the dock with animation support.
///
/// `bg_width` is the animated background width (may differ from surface_width during animation).
/// The background is centered within the surface; icons are centered within the background.
/// A clip mask from the background shape clips icons for smooth enter/exit reveals.
pub fn render_dock(
    slots: &[RenderSlot],
    bg_width: f32,
    surface_width: u32,
    surface_height: u32,
) -> Pixmap {
    let mut pixmap = Pixmap::new(surface_width, surface_height).unwrap();

    if bg_width < 2.0 && slots.is_empty() {
        return pixmap;
    }

    // Center the background in the surface
    let bg_x = (surface_width as f32 - bg_width) / 2.0;
    let bg_h = surface_height as f32;

    // Draw background rounded rectangle
    draw_rounded_rect(
        &mut pixmap,
        bg_x,
        0.0,
        bg_width,
        bg_h,
        DOCK_RADIUS,
        bg_color(),
        None,
    );
    draw_rounded_rect_stroke(
        &mut pixmap,
        bg_x + 0.5,
        0.5,
        bg_width - 1.0,
        bg_h - 1.0,
        DOCK_RADIUS,
        border_color(),
        None,
    );

    if slots.is_empty() {
        return pixmap;
    }

    // Create clip mask from the background shape so icons are revealed/hidden at the edges
    let mask = create_clip_mask(
        bg_x,
        0.0,
        bg_width,
        bg_h,
        DOCK_RADIUS,
        surface_width,
        surface_height,
    );

    // Compute icon x-positions centered within the background
    let slot_count = slots.len();
    let total_icons_width = slot_count as f32 * ICON_SIZE as f32
        + (slot_count.saturating_sub(1)) as f32 * ICON_PADDING as f32;
    let start_x = bg_x + (bg_width - total_icons_width) / 2.0;

    for (i, slot) in slots.iter().enumerate() {
        let x = start_x + i as f32 * (ICON_SIZE + ICON_PADDING) as f32;
        let base_y = (surface_height as f32 - ICON_SIZE as f32) / 2.0 - 2.0;
        let y = base_y + slot.y_offset;

        // Draw icon (clipped to background shape)
        if let Some(icon_pixmap) = slot.icon {
            let scaled = scale_pixmap(icon_pixmap, ICON_SIZE, ICON_SIZE);
            pixmap.draw_pixmap(
                x as i32,
                y as i32,
                scaled.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                mask.as_ref(),
            );
        } else {
            draw_placeholder_icon(&mut pixmap, x, y, ICON_SIZE as f32, mask.as_ref());
        }

        // Draw active indicator dot (also clipped)
        if slot.show_dot {
            let dot_x = x + ICON_SIZE as f32 / 2.0;
            let dot_y = surface_height as f32 - 6.0;
            draw_circle(
                &mut pixmap,
                dot_x,
                dot_y,
                DOT_RADIUS,
                dot_color(),
                mask.as_ref(),
            );
        }
    }

    pixmap
}

fn create_clip_mask(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    surface_w: u32,
    surface_h: u32,
) -> Option<Mask> {
    let mut mask = Mask::new(surface_w, surface_h)?;
    let mut pb = PathBuilder::new();
    rounded_rect_path(&mut pb, x, y, w, h, r);
    let path = pb.finish()?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    Some(mask)
}

fn draw_rounded_rect(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Color,
    mask: Option<&Mask>,
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
            mask,
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
    mask: Option<&Mask>,
) {
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
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
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

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color, mask: Option<&Mask>) {
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
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            mask,
        );
    }
}

fn draw_placeholder_icon(pixmap: &mut Pixmap, x: f32, y: f32, size: f32, mask: Option<&Mask>) {
    let cx = x + size / 2.0;
    let cy = y + size / 2.0;
    let r = size / 2.0 - 2.0;
    draw_circle(pixmap, cx, cy, r, Color::from_rgba8(80, 80, 80, 200), mask);
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
    let icon_path = find_icon_path(icon_name)?;
    load_icon_file(&icon_path)
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
                    let svg_path = format!("{}/{}/{}/{}/{}.svg", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&svg_path).exists() {
                        return Some(svg_path);
                    }
                    let png_path = format!("{}/{}/{}/{}/{}.png", base, theme, size, cat, icon_name);
                    if std::path::Path::new(&png_path).exists() {
                        return Some(png_path);
                    }
                }
            }
        }
    }

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
