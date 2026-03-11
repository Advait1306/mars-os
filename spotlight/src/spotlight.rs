use std::process::{Command, Stdio};
use std::time::Duration;

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface},
        Connection, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
    shell::WaylandSurface,
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use tiny_skia::Pixmap;

use crate::apps::{self, AppEntry};
use crate::render;

// XKB keysym raw values
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_UP: u32 = 0xff52;
const KEY_DOWN: u32 = 0xff54;
const KEY_TAB: u32 = 0xff09;

pub struct Spotlight {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    #[allow(dead_code)]
    layer_shell: LayerShell,

    layer_surface: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,

    keyboard: Option<wl_keyboard::WlKeyboard>,

    query: String,
    selected: usize,
    scroll_offset: usize,

    all_apps: Vec<AppEntry>,
    app_icons: Vec<Option<Pixmap>>,
    filtered: Vec<usize>,

    font: fontdue::Font,

    should_exit: bool,
}

impl Spotlight {
    pub fn run() {
        let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
        let (globals, event_queue) =
            registry_queue_init::<Spotlight>(&conn).expect("Failed to init registry");
        let qh = event_queue.handle();

        let compositor_state =
            CompositorState::bind(&globals, &qh).expect("Compositor not available");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
        let shm = Shm::bind(&globals, &qh).expect("SHM not available");

        // Discover apps and load icons
        let all_apps = apps::discover_apps();
        log::info!("Discovered {} apps", all_apps.len());
        let app_icons: Vec<Option<Pixmap>> = all_apps
            .iter()
            .map(|app| {
                app.icon_name
                    .as_ref()
                    .and_then(|name| render::load_icon(name))
            })
            .collect();

        let filtered: Vec<usize> = (0..all_apps.len()).collect();
        let height = render::calc_height(filtered.len());
        let width = render::WINDOW_WIDTH;

        // Create surface
        let surface = compositor_state.create_surface(&qh);
        let layer_surface = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("spotlight"),
            None,
        );

        layer_surface.set_anchor(Anchor::TOP);
        layer_surface.set_size(width, height);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_margin(200, 0, 0, 0);
        layer_surface.wl_surface().commit();

        let max_height = render::calc_height(render::MAX_VISIBLE);
        let pool = SlotPool::new(width as usize * max_height as usize * 4, &shm)
            .expect("Failed to create SHM pool");

        let font = render::load_font();

        let mut spotlight = Spotlight {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            compositor_state,
            shm,
            layer_shell,
            layer_surface,
            pool,
            width,
            height,
            configured: false,
            needs_redraw: true,
            keyboard: None,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            all_apps,
            app_icons,
            filtered,
            font,
            should_exit: false,
        };

        let mut event_loop: EventLoop<Spotlight> =
            EventLoop::try_new().expect("Failed to create event loop");
        let loop_handle = event_loop.handle();

        WaylandSource::new(conn, event_queue)
            .insert(loop_handle)
            .expect("Failed to insert Wayland source");

        loop {
            if spotlight.should_exit {
                break;
            }

            spotlight.draw();

            event_loop
                .dispatch(Duration::from_millis(16), &mut spotlight)
                .expect("Event loop dispatch failed");
        }
    }

    fn update_filter(&mut self) {
        self.filtered = apps::fuzzy_search(&self.all_apps, &self.query);

        // Clamp selection
        if !self.filtered.is_empty() {
            self.selected = self.selected.min(self.filtered.len() - 1);
        } else {
            self.selected = 0;
        }
        self.scroll_offset = 0;

        // Resize surface
        let new_height = render::calc_height(self.filtered.len());
        if new_height != self.height {
            self.height = new_height;
            self.layer_surface.set_size(self.width, self.height);
            self.layer_surface.wl_surface().commit();
        }

        self.needs_redraw = true;
    }

    fn launch_selected(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let idx = self.filtered[self.selected];
        let exec = &self.all_apps[idx].exec;
        log::info!("Launching: {}", exec);

        let _ = Command::new("sh")
            .arg("-c")
            .arg(exec)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        self.should_exit = true;
    }

    fn ensure_selection_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + render::MAX_VISIBLE {
            self.scroll_offset = self.selected - render::MAX_VISIBLE + 1;
        }
    }

    fn draw(&mut self) {
        if !self.configured || !self.needs_redraw {
            return;
        }

        let width = self.width;
        let height = self.height;
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        if self.pool.len() < buf_size {
            self.pool.resize(buf_size).expect("Failed to resize pool");
        }

        let (buffer, canvas) = self
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("Failed to create buffer");

        // Build visible results
        let visible_end = (self.scroll_offset + render::MAX_VISIBLE).min(self.filtered.len());
        let visible_indices = &self.filtered[self.scroll_offset..visible_end];
        let items: Vec<render::ResultItem> = visible_indices
            .iter()
            .map(|&i| render::ResultItem {
                name: &self.all_apps[i].name,
                icon: self.app_icons[i].as_ref(),
            })
            .collect();

        let selected_visual = self.selected.saturating_sub(self.scroll_offset);

        let pixmap = render::render_spotlight(
            &self.font,
            &self.query,
            &items,
            selected_visual,
            width,
            height,
        );

        // Convert RGBA (tiny-skia) to ARGB8888 (Wayland)
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

        let surface = self.layer_surface.wl_surface();
        surface.attach(Some(buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        self.needs_redraw = false;
    }
}

// --- SCTK handler implementations ---

impl CompositorHandler for Spotlight {
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
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
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

impl OutputHandler for Spotlight {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Spotlight {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.should_exit = true;
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
    }
}

impl SeatHandler for Spotlight {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to get keyboard");
            self.keyboard = Some(keyboard);
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Spotlight {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.should_exit = true;
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        match event.keysym.raw() {
            KEY_ESCAPE => {
                self.should_exit = true;
            }
            KEY_RETURN => {
                self.launch_selected();
            }
            KEY_BACKSPACE => {
                self.query.pop();
                self.selected = 0;
                self.update_filter();
            }
            KEY_UP => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.ensure_selection_visible();
                    self.needs_redraw = true;
                }
            }
            KEY_DOWN | KEY_TAB => {
                if !self.filtered.is_empty() && self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                    self.ensure_selection_visible();
                    self.needs_redraw = true;
                }
            }
            _ => {
                if let Some(ref text) = event.utf8 {
                    if !text.is_empty() && !text.chars().any(|c| c.is_control()) {
                        self.query.push_str(text);
                        self.selected = 0;
                        self.update_filter();
                    }
                }
            }
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: u32,
    ) {
    }
}

impl ShmHandler for Spotlight {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Spotlight {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Spotlight);
delegate_output!(Spotlight);
delegate_layer!(Spotlight);
delegate_registry!(Spotlight);
delegate_seat!(Spotlight);
delegate_shm!(Spotlight);
delegate_keyboard!(Spotlight);
