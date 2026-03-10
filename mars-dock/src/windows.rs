//! Window tracking via KDE's org_kde_plasma_window_management Wayland protocol.
//! Event-driven: compositor notifies us of window open/close/focus changes.

use std::collections::BTreeMap;

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window,
    org_kde_plasma_window_management,
};

/// Grouped application entry for the dock
#[derive(Debug, Clone)]
pub struct DockApp {
    pub app_id: String,
    pub name: String,
    pub icon_name: String,
    pub is_active: bool,
    pub window_count: u32,
}

/// Tracks individual window state from the protocol
#[derive(Debug, Clone)]
struct WindowInfo {
    app_id: String,
    title: String,
    icon_name: String,
    is_active: bool,
    skip_taskbar: bool,
    /// get_window sends a spurious Unmapped during init; track it
    seen_first_unmap: bool,
}

impl Default for WindowInfo {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            title: String::new(),
            icon_name: String::new(),
            is_active: false,
            skip_taskbar: false,
            seen_first_unmap: false,
        }
    }
}

/// State for all tracked windows
pub struct WindowTracker {
    /// Map from window ID to window info, ordered by insertion (BTreeMap for stable order)
    windows: BTreeMap<u32, WindowInfo>,
    /// Counter for stable ordering
    next_order: u32,
    /// Map from window ID to insertion order
    order: BTreeMap<u32, u32>,
    /// Whether the window list has changed since last query
    pub changed: bool,
}

impl WindowTracker {
    pub fn new() -> Self {
        Self {
            windows: BTreeMap::new(),
            next_order: 0,
            order: BTreeMap::new(),
            changed: true,
        }
    }

    /// Get the grouped list of dock apps, sorted by first-seen order
    pub fn get_dock_apps(&self) -> Vec<DockApp> {
        // Group by app_id, preserving order of first appearance
        let mut app_map: BTreeMap<String, DockApp> = BTreeMap::new();
        let mut app_order: BTreeMap<String, u32> = BTreeMap::new();

        // Sort windows by their insertion order
        let mut ordered_windows: Vec<_> = self.windows.iter().collect();
        ordered_windows.sort_by_key(|(id, _)| self.order.get(id).unwrap_or(&u32::MAX));

        for (_id, info) in &ordered_windows {
            if info.skip_taskbar || info.app_id.is_empty() {
                continue;
            }

            let entry = app_map.entry(info.app_id.clone()).or_insert_with(|| {
                app_order.insert(info.app_id.clone(), self.next_order);
                DockApp {
                    app_id: info.app_id.clone(),
                    name: info.title.clone(),
                    icon_name: if info.icon_name.is_empty() {
                        resolve_icon_name(&info.app_id)
                    } else {
                        info.icon_name.clone()
                    },
                    is_active: false,
                    window_count: 0,
                }
            });
            entry.window_count += 1;
            if info.is_active {
                entry.is_active = true;
            }
        }

        // Sort by first appearance order
        let mut apps: Vec<_> = app_map.into_values().collect();
        apps.sort_by_key(|a| app_order.get(&a.app_id).copied().unwrap_or(u32::MAX));
        apps
    }

    fn has_window(&self, id: u32) -> bool {
        self.windows.contains_key(&id)
    }

    /// Returns true if this window already saw its first Unmapped (init phase done).
    /// Sets the flag on first call, so second call returns true.
    fn mark_first_unmap(&mut self, id: u32) -> bool {
        if let Some(w) = self.windows.get_mut(&id) {
            if w.seen_first_unmap {
                return true; // already seen — this is a real unmap
            }
            w.seen_first_unmap = true;
        }
        false
    }

    fn add_window(&mut self, id: u32) {
        if !self.windows.contains_key(&id) {
            self.windows.insert(id, WindowInfo::default());
            self.order.insert(id, self.next_order);
            self.next_order += 1;
            self.changed = true;
        }
    }

    fn remove_window(&mut self, id: u32) {
        self.windows.remove(&id);
        self.order.remove(&id);
        self.changed = true;
    }

    fn update_window<F: FnOnce(&mut WindowInfo)>(&mut self, id: u32, f: F) {
        if let Some(info) = self.windows.get_mut(&id) {
            f(info);
            self.changed = true;
        }
    }
}

// State flags from the plasma-window-management protocol enum
const STATE_IS_ACTIVE: u32 = 0x1;       // is_active
const STATE_SKIP_TASKBAR: u32 = 0x4000; // skip_taskbar

/// Dispatch handler for org_kde_plasma_window_management
impl<D> Dispatch<org_kde_plasma_window_management::OrgKdePlasmaWindowManagement, (), D> for WindowTracker
where
    D: Dispatch<org_kde_plasma_window_management::OrgKdePlasmaWindowManagement, ()>
        + Dispatch<org_kde_plasma_window::OrgKdePlasmaWindow, u32>
        + AsMut<WindowTracker>
        + 'static,
{
    fn event(
        state: &mut D,
        proxy: &org_kde_plasma_window_management::OrgKdePlasmaWindowManagement,
        event: org_kde_plasma_window_management::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<D>,
    ) {
        match event {
            org_kde_plasma_window_management::Event::WindowWithUuid { id, uuid: _ } => {
                // Prefer WindowWithUuid (newer) — only call get_window once per window
                if !state.as_mut().has_window(id) {
                    log::debug!("New window: {}", id);
                    state.as_mut().add_window(id);
                    if proxy.version() >= 16 {
                        let _ = proxy.get_window(id, qh, id);
                    }
                }
            }
            org_kde_plasma_window_management::Event::Window { id } => {
                // Fallback for older protocol versions without WindowWithUuid
                if !state.as_mut().has_window(id) {
                    log::debug!("New window: {}", id);
                    state.as_mut().add_window(id);
                    if proxy.version() >= 16 {
                        let _ = proxy.get_window(id, qh, id);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Dispatch handler for individual plasma windows
impl<D> Dispatch<org_kde_plasma_window::OrgKdePlasmaWindow, u32, D> for WindowTracker
where
    D: Dispatch<org_kde_plasma_window::OrgKdePlasmaWindow, u32> + AsMut<WindowTracker>,
{
    fn event(
        state: &mut D,
        _proxy: &org_kde_plasma_window::OrgKdePlasmaWindow,
        event: org_kde_plasma_window::Event,
        id: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        let window_id = *id;
        let tracker = state.as_mut();

        match event {
            org_kde_plasma_window::Event::TitleChanged { title } => {
                tracker.update_window(window_id, |w| w.title = title);
            }
            org_kde_plasma_window::Event::AppIdChanged { app_id } => {
                log::debug!("Window {} app_id: {}", window_id, app_id);
                tracker.update_window(window_id, |w| w.app_id = app_id);
            }
            org_kde_plasma_window::Event::ThemedIconNameChanged { name } => {
                tracker.update_window(window_id, |w| w.icon_name = name);
            }
            org_kde_plasma_window::Event::StateChanged { flags } => {
                tracker.update_window(window_id, |w| {
                    w.is_active = flags & STATE_IS_ACTIVE != 0;
                    w.skip_taskbar = flags & STATE_SKIP_TASKBAR != 0;
                });
            }
            org_kde_plasma_window::Event::Unmapped => {
                // get_window sends a spurious Unmapped during init.
                // First Unmapped = init (ignore), subsequent = real close.
                if tracker.mark_first_unmap(window_id) {
                    // Was already seen — this is a real close
                    tracker.remove_window(window_id);
                }
            }
            _ => {}
        }
    }
}

/// Resolve the icon name for an application from .desktop files
fn resolve_icon_name(app_id: &str) -> String {
    let desktop_paths = [
        "/usr/share/applications",
        "/usr/local/share/applications",
    ];

    for base in &desktop_paths {
        for name in &[
            format!("{}.desktop", app_id),
            format!("org.kde.{}.desktop", app_id),
        ] {
            let path = format!("{}/{}", base, name);
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Some(icon) = extract_icon_from_desktop(&contents) {
                    return icon;
                }
            }
        }
    }

    app_id.to_string()
}

fn extract_icon_from_desktop(contents: &str) -> Option<String> {
    for line in contents.lines() {
        if let Some(icon) = line.strip_prefix("Icon=") {
            return Some(icon.trim().to_string());
        }
    }
    None
}
