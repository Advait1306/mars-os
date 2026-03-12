//! Icon path resolution for dock apps from freedesktop icon themes.

/// Find the filesystem path for a given icon name.
pub fn find_icon_path(icon_name: &str) -> Option<String> {
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
