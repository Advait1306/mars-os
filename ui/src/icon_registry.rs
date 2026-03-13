//! Icon registry with inline SVG registration, icon packs, and freedesktop theme fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// An icon pack: a named collection of SVG icon data keyed by icon name.
#[derive(Debug, Clone)]
pub struct IconPack {
    pub name: String,
    pub icons: HashMap<String, String>,
}

impl IconPack {
    /// Create a new empty icon pack.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            icons: HashMap::new(),
        }
    }

    /// Add an icon to the pack.
    pub fn add(&mut self, name: &str, svg_data: &str) {
        self.icons.insert(name.to_string(), svg_data.to_string());
    }

    /// Load all SVG files from a directory into this pack.
    /// Each file's stem (e.g. "folder" from "folder.svg") becomes the icon name.
    pub fn load_directory(&mut self, dir: &str) -> std::io::Result<usize> {
        let mut count = 0;
        let path = Path::new(dir);
        if !path.is_dir() {
            return Ok(0);
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().and_then(|e| e.to_str()) == Some("svg") {
                if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(data) = std::fs::read_to_string(&file_path) {
                        self.icons.insert(stem.to_string(), data);
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }
}

/// Resolved icon: either inline SVG data or a file path.
#[derive(Debug, Clone)]
pub enum ResolvedIcon {
    /// Inline SVG string data.
    Svg(String),
    /// Path to an SVG or PNG file on disk.
    File(PathBuf),
}

/// Icon registry with multi-tier resolution: inline → packs → freedesktop themes.
#[derive(Debug, Clone)]
pub struct IconRegistry {
    /// Inline SVG data by name (highest priority).
    inline: HashMap<String, String>,
    /// Registered icon packs (searched in order).
    packs: Vec<IconPack>,
    /// Freedesktop theme search paths.
    theme_paths: Vec<String>,
    /// Active freedesktop icon theme name.
    theme: String,
}

impl Default for IconRegistry {
    fn default() -> Self {
        Self {
            inline: HashMap::new(),
            packs: Vec::new(),
            theme_paths: vec![
                "/usr/share/icons".to_string(),
                "/usr/local/share/icons".to_string(),
            ],
            theme: "breeze-dark".to_string(),
        }
    }
}

impl IconRegistry {
    /// Create a new icon registry with default freedesktop paths.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the active freedesktop icon theme.
    pub fn set_theme(&mut self, theme: &str) {
        self.theme = theme.to_string();
    }

    /// Register a single inline SVG icon.
    pub fn register(&mut self, name: &str, svg_data: &str) {
        self.inline.insert(name.to_string(), svg_data.to_string());
    }

    /// Register an icon pack.
    pub fn register_pack(&mut self, pack: IconPack) {
        self.packs.push(pack);
    }

    /// Register an icon pack from a directory of SVG files.
    pub fn register_pack_from_dir(&mut self, name: &str, dir: &str) -> std::io::Result<usize> {
        let mut pack = IconPack::new(name);
        let count = pack.load_directory(dir)?;
        self.packs.push(pack);
        Ok(count)
    }

    /// Resolve an icon name to SVG data or a file path.
    ///
    /// Resolution order:
    /// 1. Inline registered icons
    /// 2. Icon packs (in registration order)
    /// 3. Freedesktop icon theme directories
    /// 4. Pixmaps fallback
    pub fn resolve(&self, name: &str) -> Option<ResolvedIcon> {
        // 1. Check inline icons
        if let Some(svg) = self.inline.get(name) {
            return Some(ResolvedIcon::Svg(svg.clone()));
        }

        // 2. Check icon packs
        for pack in &self.packs {
            if let Some(svg) = pack.icons.get(name) {
                return Some(ResolvedIcon::Svg(svg.clone()));
            }
        }

        // 3. Freedesktop icon theme lookup
        if let Some(path) = self.find_freedesktop_icon(name) {
            return Some(ResolvedIcon::File(path));
        }

        None
    }

    /// Search freedesktop icon theme directories for an icon.
    fn find_freedesktop_icon(&self, name: &str) -> Option<PathBuf> {
        let sizes = ["scalable", "48x48", "64x64", "256x256", "128x128", "32x32", "24x24", "22x22", "16x16"];
        let categories = [
            "apps", "applications", "actions", "categories", "devices",
            "emblems", "mimetypes", "places", "status",
        ];
        let themes = [self.theme.as_str(), "hicolor"];

        for base in &self.theme_paths {
            for theme in &themes {
                for size in &sizes {
                    for cat in &categories {
                        let svg_path = PathBuf::from(format!("{}/{}/{}/{}/{}.svg", base, theme, size, cat, name));
                        if svg_path.exists() {
                            return Some(svg_path);
                        }
                        let png_path = PathBuf::from(format!("{}/{}/{}/{}/{}.png", base, theme, size, cat, name));
                        if png_path.exists() {
                            return Some(png_path);
                        }
                    }
                }
            }
        }

        // Pixmaps fallback
        for ext in &["svg", "png"] {
            let pixmap_path = PathBuf::from(format!("/usr/share/pixmaps/{}.{}", name, ext));
            if pixmap_path.exists() {
                return Some(pixmap_path);
            }
        }

        None
    }
}
