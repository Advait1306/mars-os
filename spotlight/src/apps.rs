use std::fs;
use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub icon_name: Option<String>,
    pub desktop_id: String,
}

pub fn discover_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let dirs = [
        "/usr/share/applications",
        "/usr/local/share/applications",
    ];

    for dir in &dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "desktop") {
                    if let Some(app) = parse_desktop_file(&path) {
                        apps.push(app);
                    }
                }
            }
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.desktop_id == b.desktop_id);
    apps
}

fn parse_desktop_file(path: &Path) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut no_display = false;
    let mut is_app = false;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if line.starts_with('[') {
            in_desktop_entry = false;
            continue;
        }
        if !in_desktop_entry {
            continue;
        }

        if let Some(val) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("Exec=") {
            let cleaned = val
                .split_whitespace()
                .filter(|s| !s.starts_with('%'))
                .collect::<Vec<_>>()
                .join(" ");
            exec = Some(cleaned);
        } else if let Some(val) = line.strip_prefix("Icon=") {
            icon = Some(val.to_string());
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Type=Application" {
            is_app = true;
        }
    }

    if no_display || !is_app {
        return None;
    }

    let desktop_id = path.file_stem()?.to_string_lossy().to_string();

    Some(AppEntry {
        name: name?,
        exec: exec?,
        icon_name: icon,
        desktop_id,
    })
}

pub fn fuzzy_search(apps: &[AppEntry], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..apps.len()).collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut results: Vec<(usize, i64)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, app)| {
            matcher
                .fuzzy_match(&app.name, query)
                .map(|score| (i, score))
        })
        .collect();

    results.sort_by(|a, b| b.1.cmp(&a.1));
    results.into_iter().map(|(i, _)| i).collect()
}
