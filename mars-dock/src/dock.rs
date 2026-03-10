//! Wayland layer-shell dock surface using smithay-client-toolkit
//! with KDE plasma window management protocol for real-time window tracking.

use std::time::Duration;

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
    shm::{
        slot::SlotPool,
        Shm, ShmHandler,
    },
};
use tiny_skia::Pixmap;
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window,
    org_kde_plasma_window_management,
};

use crate::render;
use crate::windows::{self, WindowTracker};

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
    apps: Vec<windows::DockApp>,
    icon_pixmaps: Vec<Option<Pixmap>>,
    needs_redraw: bool,
    exit: bool,

    // Window tracking
    window_tracker: WindowTracker,
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

        let width = render::dock_width(0);
        let height = render::dock_height();

        // Create surface
        let surface = compositor_state.create_surface(&qh);

        let layer_surface = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Top,
            Some("mars-dock"),
            None,
        );

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
            apps: Vec::new(),
            icon_pixmaps: Vec::new(),
            needs_redraw: true,
            exit: false,
            window_tracker: WindowTracker::new(),
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

            // Check if window tracker has changes and update dock
            if dock.window_tracker.changed {
                dock.window_tracker.changed = false;
                let new_apps = dock.window_tracker.get_dock_apps();
                log::debug!("Dock: {} apps", new_apps.len());
                let new_icons: Vec<Option<Pixmap>> = new_apps
                    .iter()
                    .map(|app| render::load_icon(&app.icon_name))
                    .collect();

                let new_width = render::dock_width(new_apps.len());
                if new_width != dock.width {
                    dock.width = new_width;
                    if let Some(ls) = &dock.layer_surface {
                        ls.set_size(dock.width, dock.height);
                        ls.wl_surface().commit();
                    }
                }
                dock.apps = new_apps;
                dock.icon_pixmaps = new_icons;
                dock.needs_redraw = true;
                dock.draw();
            }

            event_loop
                .dispatch(Duration::from_millis(16), &mut dock)
                .expect("Event loop dispatch failed");
        }
    }

    fn draw(&mut self) {
        if !self.configured || !self.needs_redraw {
            return;
        }

        let pool = match self.pool.as_mut() {
            Some(p) => p,
            None => return,
        };

        let width = self.width;
        let height = self.height;
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        if pool.len() < buf_size {
            pool.resize(buf_size).expect("Failed to resize pool");
        }

        let (buffer, canvas) = pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("Failed to create buffer");

        let pixmap = render::render_dock(&self.apps, width, height, &self.icon_pixmaps);

        // Convert RGBA (tiny-skia) to ARGB8888 (Wayland, BGRA in little-endian)
        let src = pixmap.data();
        for i in 0..(width * height) as usize {
            let si = i * 4;
            let r = src[si];
            let g = src[si + 1];
            let b = src[si + 2];
            let a = src[si + 3];
            canvas[si] = b;
            canvas[si + 1] = g;
            canvas[si + 2] = r;
            canvas[si + 3] = a;
        }

        if let Some(ls) = &self.layer_surface {
            let surface = ls.wl_surface();
            surface.attach(Some(buffer.wl_buffer()), 0, 0);
            surface.damage_buffer(0, 0, width as i32, height as i32);
            surface.commit();
        }

        self.needs_redraw = false;
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
        // Delegate to WindowTracker
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
        <WindowTracker as Dispatch<
            org_kde_plasma_window::OrgKdePlasmaWindow,
            u32,
            Dock,
        >>::event(state, proxy, event, data, conn, qh);
    }
}

// --- Smithay handler implementations ---

impl CompositorHandler for Dock {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {
        self.needs_redraw = true;
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        if self.needs_redraw { self.draw(); }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Dock {
    fn output_state(&mut self) -> &mut OutputState { &mut self.output_state }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Dock {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        self.width = if configure.new_size.0 > 0 { configure.new_size.0 } else { self.width };
        self.height = if configure.new_size.1 > 0 { configure.new_size.1 } else { self.height };
        self.configured = true;
        self.needs_redraw = true;
        self.draw();
    }
}

impl SeatHandler for Dock {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seat_state }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: smithay_client_toolkit::seat::Capability) {}
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: smithay_client_toolkit::seat::Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl ShmHandler for Dock {
    fn shm_state(&mut self) -> &mut Shm { &mut self.shm }
}

impl ProvidesRegistryState for Dock {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Dock);
delegate_output!(Dock);
delegate_layer!(Dock);
delegate_registry!(Dock);
delegate_seat!(Dock);
delegate_shm!(Dock);
