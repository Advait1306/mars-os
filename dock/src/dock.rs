//! Dock view using the ui framework's rendering pipeline and animation system.
//! Tracks open windows via KDE's plasma window management protocol on a
//! separate Wayland event queue, then renders icons declaratively.

use ui::app::{View, WaylandContext};
use ui::color::rgba;
use ui::element::*;
use ui::reactive::{Reactive, RenderContext};
use ui::style::*;
use ui::{From, To};

use wayland_client::EventQueue;
use wayland_protocols_plasma::plasma_window_management::client::org_kde_plasma_window_management;

use crate::icons;
use crate::windows::PlasmaState;

const ICON_SIZE: f32 = 44.0;
const ICON_PADDING: f32 = 8.0;
const DOCK_PADDING: f32 = 12.0;
const DOCK_HEIGHT: u32 = 64;
const DOCK_RADIUS: f32 = 18.0;

/// Calculate the dock width based on number of apps
fn dock_width(app_count: usize) -> u32 {
    if app_count == 0 {
        return 200; // minimum width for empty state
    }
    let icons_width = app_count as u32 * ICON_SIZE as u32
        + (app_count.saturating_sub(1)) as u32 * ICON_PADDING as u32;
    icons_width + DOCK_PADDING as u32 * 2
}

pub struct DockView {
    plasma: PlasmaState,
    event_queue: Option<EventQueue<PlasmaState>>,
    #[allow(dead_code)]
    plasma_wm: Option<org_kde_plasma_window_management::OrgKdePlasmaWindowManagement>,
    dirty: Reactive<bool>,
}

impl DockView {
    pub fn new() -> Self {
        Self {
            plasma: PlasmaState::new(),
            event_queue: None,
            plasma_wm: None,
            dirty: Reactive::new(false),
        }
    }
}

impl View for DockView {
    fn setup(&mut self, wl: &WaylandContext) {
        let event_queue = wl.connection.new_event_queue::<PlasmaState>();
        let qh = event_queue.handle();

        let plasma_wm: org_kde_plasma_window_management::OrgKdePlasmaWindowManagement = wl
            .globals
            .bind(&qh, 1..=16, ())
            .expect("org_kde_plasma_window_management not available");

        self.plasma_wm = Some(plasma_wm);
        self.event_queue = Some(event_queue);
    }

    fn tick(&mut self) {
        if let Some(eq) = &mut self.event_queue {
            let _ = eq.dispatch_pending(&mut self.plasma);
        }
        if self.plasma.tracker.changed {
            self.plasma.tracker.changed = false;
            self.dirty.set(true);
        }
    }

    fn render(&self, cx: &mut RenderContext) -> Element {
        let _ = self.dirty.get(cx);
        let apps = self.plasma.tracker.get_dock_apps();

        let target_width = dock_width(apps.len());
        cx.set_surface_size(target_width, DOCK_HEIGHT);
        let (sw, sh) = cx.surface_size();

        row()
            .size(sw as f32, sh as f32)
            .justify(Justify::Center)
            .align_items(Alignment::Center)
            .child(
                row()
                    .key("dock-bg")
                    .animate_layout()
                    .height(sh as f32)
                    .padding_xy(DOCK_PADDING, 0.0)
                    .gap(ICON_PADDING)
                    .background(rgba(30, 30, 30, 190))
                    .rounded(DOCK_RADIUS)
                    .border(rgba(255, 255, 255, 30), 1.0)
                    .clip()
                    .align_items(Alignment::Center)
                    .children(apps.iter().map(|app| {
                        let icon_path = icons::find_icon_path(&app.icon_name);
                        let icon_el = match icon_path {
                            Some(path) => image_file(&path).size(ICON_SIZE, ICON_SIZE),
                            None => container()
                                .size(ICON_SIZE, ICON_SIZE)
                                .rounded(ICON_SIZE / 2.0)
                                .background(rgba(80, 80, 80, 200)),
                        };

                        let dot = container()
                            .size(5.0, 5.0)
                            .rounded(2.5)
                            .background(rgba(255, 255, 255, 220))
                            .opacity(if app.is_active { 1.0 } else { 0.0 });

                        column()
                            .align_items(Alignment::Center)
                            .gap(2.0)
                            .child(icon_el)
                            .child(dot)
                            .key(&app.app_id)
                            .initial(From::new().offset_y(DOCK_HEIGHT as f32).delay_ms(150))
                            .exit(To::new().offset_y(20.0))
                            .animate_layout()
                    })),
            )
    }
}
