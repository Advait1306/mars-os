//! Wayland layer-shell dock surface using smithay-client-toolkit
//! with KDE plasma window management protocol for real-time window tracking.
//!
//! Rendering uses the ui framework's pipeline (Element -> Layout -> DisplayList -> SkiaRenderer)
//! for the dock background, while icons are rendered directly via Skia since their positions
//! are controlled by per-icon spring animations.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_output, wl_seat, wl_shm, wl_surface},
        Connection, Dispatch, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{SeatHandler, SeatState},
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
    shell::WaylandSurface,
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window, org_kde_plasma_window_management,
};

use ui::color::rgba;
use ui::display_list::build_display_list;
use ui::element::*;
use ui::layout::compute_layout;
use ui::renderer::SkiaRenderer;
use ui::spring::{SpringConfig, SpringState};
use ui::style::*;

use crate::icons;
use crate::windows::{self, WindowTracker};

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

/// An icon slot in the dock, tracking its animation state
struct AnimSlot {
    app_id: String,
    app: windows::DockApp,
    icon: Option<skia_safe::Image>,
    /// Spring controlling vertical offset: 0 = resting position, DOCK_HEIGHT = fully hidden below
    y_spring: SpringState,
    /// Spring controlling horizontal offset from the surface center (left edge of icon)
    x_spring: SpringState,
    /// True while waiting for resize phase to complete before sliding in
    entering: bool,
    /// True while sliding down before removal
    leaving: bool,
}

pub struct Dock {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    #[allow(dead_code)]
    layer_shell: LayerShell,

    layer_surface: Option<LayerSurface>,
    pool: Option<SlotPool>,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,
    exit: bool,

    // Window tracking
    window_tracker: WindowTracker,

    // Animation state
    anim_slots: Vec<AnimSlot>,
    width_spring: SpringState,
    spring_config: SpringConfig,
    last_tick: Instant,
    animating: bool,
    /// True while resize phase is in progress and entering icons are waiting
    pending_entries: bool,

    // UI framework rendering
    renderer: SkiaRenderer,
}

impl AsMut<WindowTracker> for Dock {
    fn as_mut(&mut self) -> &mut WindowTracker {
        &mut self.window_tracker
    }
}

impl Dock {
    pub fn run() {
        let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
        let (globals, event_queue) =
            registry_queue_init::<Dock>(&conn).expect("Failed to init registry");
        let qh = event_queue.handle();

        let compositor_state =
            CompositorState::bind(&globals, &qh).expect("Compositor not available");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
        let shm = Shm::bind(&globals, &qh).expect("SHM not available");

        // Bind plasma window management
        let _plasma_wm: org_kde_plasma_window_management::OrgKdePlasmaWindowManagement = globals
            .bind(&qh, 1..=16, ())
            .expect("org_kde_plasma_window_management not available");

        let width = dock_width(0);
        let height = DOCK_HEIGHT;

        // Create surface
        let surface = compositor_state.create_surface(&qh);

        let layer_surface =
            layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("dock"), None);

        layer_surface.set_anchor(Anchor::BOTTOM);
        layer_surface.set_size(width, height);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_margin(0, 0, 8, 0);
        layer_surface.wl_surface().commit();

        let pool = SlotPool::new(width as usize * height as usize * 4, &shm)
            .expect("Failed to create SHM pool");

        let mut dock = Dock {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            compositor_state,
            shm,
            layer_shell,
            layer_surface: Some(layer_surface),
            pool: Some(pool),
            width,
            height,
            configured: false,
            needs_redraw: true,
            exit: false,
            window_tracker: WindowTracker::new(),
            anim_slots: Vec::new(),
            width_spring: SpringState::new(width as f32),
            spring_config: SpringConfig::default(),
            last_tick: Instant::now(),
            animating: false,
            pending_entries: false,
            renderer: SkiaRenderer::new(),
        };

        let mut event_loop: EventLoop<Dock> =
            EventLoop::try_new().expect("Failed to create event loop");
        let loop_handle = event_loop.handle();

        WaylandSource::new(conn, event_queue)
            .insert(loop_handle)
            .expect("Failed to insert Wayland source");

        loop {
            if dock.exit {
                break;
            }

            // Check if window tracker has changes — diff and start animations
            if dock.window_tracker.changed {
                dock.window_tracker.changed = false;
                dock.update_apps();
            }

            // Tick animations
            let now = Instant::now();
            let dt = now.duration_since(dock.last_tick).as_secs_f32().min(0.05);
            dock.last_tick = now;

            if dock.animating {
                dock.tick_animations(dt);
                dock.needs_redraw = true;
            }

            dock.draw();

            event_loop
                .dispatch(Duration::from_millis(16), &mut dock)
                .expect("Event loop dispatch failed");
        }
    }

    /// Diff new apps against current animation slots and start phased animations.
    ///
    /// ENTER: dock expands + icons reposition (phase 1), then new icon slides up (phase 2).
    /// EXIT:  icon slides down (phase 1), then dock contracts + icons reposition (phase 2).
    fn update_apps(&mut self) {
        let new_apps = self.window_tracker.get_dock_apps();
        log::debug!("Dock: {} apps", new_apps.len());

        let new_icons: Vec<Option<skia_safe::Image>> = new_apps
            .iter()
            .map(|app| icons::load_icon(&app.icon_name))
            .collect();

        let new_ids: HashSet<String> = new_apps.iter().map(|a| a.app_id.clone()).collect();
        let old_ids: HashSet<String> = self
            .anim_slots
            .iter()
            .filter(|s| !s.leaving)
            .map(|s| s.app_id.clone())
            .collect();

        // Mark removed apps as leaving — they slide down, dock stays same size.
        // Target 20px: only ~14px needed to be clipped out by the background mask,
        // so 20px is plenty. Much faster than targeting full DOCK_HEIGHT (64px).
        for slot in &mut self.anim_slots {
            if !slot.leaving && !new_ids.contains(&slot.app_id) {
                slot.leaving = true;
                slot.y_spring.set_target(20.0);
            }
        }

        // Graduate any currently entering icons — let them start sliding up now
        // rather than holding them hidden while the next resize happens.
        for slot in &mut self.anim_slots {
            if slot.entering {
                slot.entering = false;
                slot.y_spring.set_target(0.0);
            }
        }

        // Update existing apps' state (active, name, etc.)
        for (app, icon) in new_apps.iter().zip(new_icons.iter()) {
            if let Some(slot) = self
                .anim_slots
                .iter_mut()
                .find(|s| s.app_id == app.app_id && !s.leaving)
            {
                slot.app = app.clone();
                if icon.is_some() {
                    slot.icon = icon.clone();
                }
            }
        }

        // Add new apps
        let had_visible = !old_ids.is_empty();
        let mut has_new = false;
        for (app, icon) in new_apps.into_iter().zip(new_icons.into_iter()) {
            if !old_ids.contains(&app.app_id) {
                has_new = true;
                // Always start hidden below dock and slide up.
                // If had_visible, entering=true delays slide-up until resize settles.
                // If first icon, entering=false so y animates immediately with width.
                let mut y_spring = SpringState::new(DOCK_HEIGHT as f32);
                if had_visible {
                    y_spring.set_target(DOCK_HEIGHT as f32);
                } else {
                    y_spring.set_target(0.0);
                }
                let x_spring = SpringState::new(0.0);
                self.anim_slots.push(AnimSlot {
                    app_id: app.app_id.clone(),
                    app,
                    icon,
                    y_spring,
                    x_spring,
                    entering: had_visible,
                    leaving: false,
                });
            }
        }

        // Compute x targets for all slots
        if had_visible {
            // Animate existing icons to new positions; snap entering icons' x
            self.recompute_x_targets(false);
            for slot in &mut self.anim_slots {
                if slot.entering {
                    slot.x_spring.settle();
                }
            }
        } else {
            // First appearance — snap everything
            self.recompute_x_targets(true);
        }

        // Width = dock_width for ALL current slots (leaving ones still hold space)
        let target_width = dock_width(self.anim_slots.len()) as f32;
        self.width_spring.set_target(target_width);

        // Jump surface to target width immediately. The dock is BOTTOM-anchored
        // and compositor-centered, so surface_width changes don't shift icons in
        // screen coordinates — the compositor re-centers the surface.
        let needed = (self.width_spring.value.ceil() as u32).max(target_width as u32);
        if needed > self.width {
            self.width = needed;
            if let Some(ls) = &self.layer_surface {
                ls.set_size(self.width, self.height);
                ls.wl_surface().commit();
            }
        }

        self.pending_entries = has_new && had_visible;
        self.animating = true;
        self.needs_redraw = true;
    }

    /// Compute x-offset targets for all slots.
    /// Offsets are relative to the surface center (left edge of each icon).
    /// If `snap` is true, springs are settled immediately (first appearance).
    fn recompute_x_targets(&mut self, snap: bool) {
        let n = self.anim_slots.len();
        let total_w = n as f32 * ICON_SIZE + n.saturating_sub(1) as f32 * ICON_PADDING;
        let stride = ICON_SIZE + ICON_PADDING;

        for (idx, slot) in self.anim_slots.iter_mut().enumerate() {
            let target = -total_w / 2.0 + idx as f32 * stride;
            slot.x_spring.set_target(target);
            if snap {
                slot.x_spring.settle();
            }
        }
    }

    /// Step all springs forward with sequenced animations:
    ///
    /// ENTER: width + x springs first, then entering icons' y springs.
    /// EXIT:  leaving icons' y springs first, then width + x springs contract.
    fn tick_animations(&mut self, dt: f32) {
        self.width_spring.step(dt, &self.spring_config);

        for slot in &mut self.anim_slots {
            slot.x_spring.step(dt, &self.spring_config);
            slot.y_spring.step(dt, &self.spring_config);
        }

        // EXIT phase transition: when leaving icons are nearly off-screen,
        // remove them and start the dock contraction + reposition.
        // Use a loose threshold (4px) so the next phase starts while the icon
        // is still barely visible, eliminating the perceptible gap.
        let had_leaving = self.anim_slots.iter().any(|s| s.leaving);
        self.anim_slots.retain(|s| {
            !(s.leaving && (s.y_spring.value - s.y_spring.target).abs() < 4.0)
        });
        if had_leaving && !self.anim_slots.iter().any(|s| s.leaving) {
            // All leaving icons gone — contract dock and reposition
            self.recompute_x_targets(false);
            let target_width = dock_width(self.anim_slots.len()) as f32;
            self.width_spring.set_target(target_width);
        }

        // ENTER phase transition: when resize (width + x) is nearly done,
        // start entering icons' slide-up animation.
        // Use a loose threshold (3px) to overlap phases slightly.
        if self.pending_entries {
            let width_near =
                (self.width_spring.value - self.width_spring.target).abs() < 3.0;
            let x_near = self
                .anim_slots
                .iter()
                .all(|s| (s.x_spring.value - s.x_spring.target).abs() < 3.0);
            if width_near && x_near {
                for slot in &mut self.anim_slots {
                    if slot.entering {
                        slot.y_spring.set_target(0.0);
                        slot.entering = false;
                    }
                }
                self.pending_entries = false;
            }
        }

        // Settle done springs
        for slot in &mut self.anim_slots {
            if slot.y_spring.is_settled() {
                slot.y_spring.settle();
            }
            if slot.x_spring.is_settled() {
                slot.x_spring.settle();
            }
        }

        // Check if all animation is complete
        let all_settled = self.width_spring.is_settled()
            && self
                .anim_slots
                .iter()
                .all(|s| s.y_spring.is_settled() && s.x_spring.is_settled());

        if all_settled {
            self.width_spring.settle();
            self.animating = false;

            // Now safe to shrink surface to final width
            let final_width = self.width_spring.value as u32;
            if final_width != self.width {
                self.width = final_width;
                if let Some(ls) = &self.layer_surface {
                    ls.set_size(self.width, self.height);
                    ls.wl_surface().commit();
                }
            }
        } else {
            // During animation, ensure surface fits the target
            let needed = (self.width_spring.value.ceil() as u32)
                .max(self.width_spring.target as u32);
            if needed > self.width {
                self.width = needed;
                if let Some(ls) = &self.layer_surface {
                    ls.set_size(self.width, self.height);
                    ls.wl_surface().commit();
                }
            }
        }
    }

    /// Build the element tree for the dock background.
    /// Icons are rendered separately via direct Skia calls since their positions
    /// are controlled by per-icon spring animations (absolute positioning).
    fn build_ui(&self) -> Element {
        let bg_width = self.width_spring.value;
        let surface_width = self.width as f32;
        let surface_height = self.height as f32;

        if bg_width < 2.0 && self.anim_slots.is_empty() {
            return container().size(surface_width, surface_height);
        }

        // Center the background panel within the surface.
        // Use a row with spacers to center the background container horizontally.
        row()
            .size(surface_width, surface_height)
            .justify(Justify::Center)
            .align_items(Alignment::Start)
            .child(
                container()
                    .width(bg_width)
                    .height(surface_height)
                    .background(rgba(30, 30, 30, 190))
                    .rounded(DOCK_RADIUS)
                    .border(rgba(255, 255, 255, 30), 1.0),
            )
    }

    fn draw(&mut self) {
        if !self.configured || !self.needs_redraw {
            return;
        }

        if self.pool.is_none() {
            return;
        }

        let width = self.width;
        let height = self.height;

        // Render to Skia surface first (no pool borrow needed)
        let mut surface = skia_safe::surfaces::raster_n32_premul((width as i32, height as i32))
            .expect("Failed to create Skia surface");

        {
            let skia_canvas = surface.canvas();
            skia_canvas.clear(skia_safe::Color::TRANSPARENT);

            // Render the background using ui framework pipeline
            let element_tree = self.build_ui();
            let (layout_tree, _) = compute_layout(&element_tree, width as f32, height as f32);
            let commands = build_display_list(&layout_tree, &element_tree, None);
            self.renderer.execute(skia_canvas, &commands);

            // Render icons directly using Skia (positioned by springs)
            self.render_icons(skia_canvas, width, height);
        }

        // Now borrow pool to create buffer and copy pixels
        let pool = self.pool.as_mut().unwrap();
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        if pool.len() < buf_size {
            pool.resize(buf_size).expect("Failed to resize pool");
        }

        let (buffer, canvas_data) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("Failed to create buffer");

        // Copy pixels from Skia surface to SHM buffer.
        // Skia's native N32 premul format on little-endian is BGRA, which matches
        // Wayland's Argb8888 (BGRA in little-endian byte order). No conversion needed.
        let image_info = surface.image_info();
        let row_bytes = width as usize * 4;
        surface.read_pixels(&image_info, canvas_data, row_bytes, (0, 0));

        if let Some(ls) = &self.layer_surface {
            let wl_surface = ls.wl_surface();
            wl_surface.attach(Some(buffer.wl_buffer()), 0, 0);
            wl_surface.damage_buffer(0, 0, width as i32, height as i32);
            wl_surface.commit();
        }

        self.needs_redraw = false;
    }

    /// Render icons directly onto the Skia canvas.
    /// Each icon's position is controlled by its own x/y spring animations.
    /// Icons are clipped to the dock background shape for smooth enter/exit reveals.
    fn render_icons(&self, canvas: &skia_safe::Canvas, surface_width: u32, surface_height: u32) {
        let bg_width = self.width_spring.value;

        if self.anim_slots.is_empty() {
            return;
        }

        // Create clip from background shape so icons are revealed/hidden at edges
        let bg_x = (surface_width as f32 - bg_width) / 2.0;
        let bg_rrect = skia_safe::RRect::new_rect_xy(
            skia_safe::Rect::from_xywh(bg_x, 0.0, bg_width, surface_height as f32),
            DOCK_RADIUS,
            DOCK_RADIUS,
        );
        canvas.save();
        canvas.clip_rrect(bg_rrect, skia_safe::ClipOp::Intersect, true);

        let center_x = surface_width as f32 / 2.0;

        for slot in &self.anim_slots {
            let x = center_x + slot.x_spring.value;
            let base_y = (surface_height as f32 - ICON_SIZE) / 2.0 - 2.0;
            let y = base_y + slot.y_spring.value;

            // Draw icon
            if let Some(icon) = &slot.icon {
                let dst = skia_safe::Rect::from_xywh(x, y, ICON_SIZE, ICON_SIZE);
                canvas.draw_image_rect(icon, None, dst, &skia_safe::Paint::default());
            } else {
                // Placeholder circle
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(skia_safe::Color::from_argb(200, 80, 80, 80));
                let cx = x + ICON_SIZE / 2.0;
                let cy = y + ICON_SIZE / 2.0;
                let r = ICON_SIZE / 2.0 - 2.0;
                canvas.draw_circle((cx, cy), r, &paint);
            }

            // Active indicator dot
            if slot.app.is_active && !slot.entering && !slot.leaving {
                let dot_x = x + ICON_SIZE / 2.0;
                let dot_y = surface_height as f32 - 6.0;
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(skia_safe::Color::from_argb(220, 255, 255, 255));
                canvas.draw_circle((dot_x, dot_y), 2.5, &paint);
            }
        }

        canvas.restore(); // pop clip
    }
}

// --- Plasma window management dispatch ---

impl Dispatch<org_kde_plasma_window_management::OrgKdePlasmaWindowManagement, ()> for Dock {
    fn event(
        state: &mut Self,
        proxy: &org_kde_plasma_window_management::OrgKdePlasmaWindowManagement,
        event: org_kde_plasma_window_management::Event,
        data: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        <WindowTracker as Dispatch<
            org_kde_plasma_window_management::OrgKdePlasmaWindowManagement,
            (),
            Dock,
        >>::event(state, proxy, event, data, conn, qh);
    }
}

impl Dispatch<org_kde_plasma_window::OrgKdePlasmaWindow, u32> for Dock {
    fn event(
        state: &mut Self,
        proxy: &org_kde_plasma_window::OrgKdePlasmaWindow,
        event: org_kde_plasma_window::Event,
        data: &u32,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        <WindowTracker as Dispatch<org_kde_plasma_window::OrgKdePlasmaWindow, u32, Dock>>::event(
            state, proxy, event, data, conn, qh,
        );
    }
}

// --- Smithay handler implementations ---

impl CompositorHandler for Dock {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
        self.needs_redraw = true;
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        if self.needs_redraw {
            self.draw();
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Dock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Dock {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.width = if configure.new_size.0 > 0 {
            configure.new_size.0
        } else {
            self.width
        };
        self.height = if configure.new_size.1 > 0 {
            configure.new_size.1
        } else {
            self.height
        };
        self.configured = true;
        self.needs_redraw = true;
        self.draw();
    }
}

impl SeatHandler for Dock {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl ShmHandler for Dock {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Dock {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Dock);
delegate_output!(Dock);
delegate_layer!(Dock);
delegate_registry!(Dock);
delegate_seat!(Dock);
delegate_shm!(Dock);
