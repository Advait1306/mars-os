//! Icon loading for dock apps from freedesktop icon themes.
//!
//! Loads app icons as skia_safe::Image, supporting both SVG and raster formats.

pub const ICON_SIZE: u32 = 44;

/// Load an icon by name from the freedesktop icon theme, returning a Skia image.
pub fn load_icon(icon_name: &str) -> Option<skia_safe::Image> {
    let icon_path = find_icon_path(icon_name)?;
    load_icon_file(&icon_path)
}

/// Find the filesystem path for a given icon name.
/// Exposed so the dock can also pass paths to the ui framework's image_file().
#[allow(dead_code)]
pub fn find_icon_path_for_app(icon_name: &str) -> Option<String> {
    find_icon_path(icon_name)
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

fn load_icon_file(path: &str) -> Option<skia_safe::Image> {
    if path.ends_with(".svg") {
        load_svg(path)
    } else {
        load_raster(path)
    }
}

fn load_svg(path: &str) -> Option<skia_safe::Image> {
    let data = std::fs::read(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).ok()?;
    let size = tree.size();
    let sx = ICON_SIZE as f32 / size.width();
    let sy = ICON_SIZE as f32 / size.height();
    let scale = sx.min(sy);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Convert tiny-skia pixmap to skia-safe Image
    let (w, h) = (pixmap.width(), pixmap.height());
    let info = skia_safe::ImageInfo::new(
        (w as i32, h as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(pixmap.data()),
        w as usize * 4,
    )
}

fn load_raster(path: &str) -> Option<skia_safe::Image> {
    let data = std::fs::read(path).ok()?;
    let sk_data = skia_safe::Data::new_copy(&data);
    skia_safe::images::deferred_from_encoded_data(sk_data, None)
}
