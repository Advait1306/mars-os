use std::io::BufRead;
use std::process::{Command, Stdio};
use std::time::Duration;

use calloop::channel;
use calloop::timer::{TimeoutAction, Timer};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
        Connection, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
    shell::WaylandSurface,
    shm::{slot::SlotPool, Shm, ShmHandler},
};

use crate::controls::{self, SystemState};
use crate::render::{self, HitZones};

struct Popup {
    surface: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,
    dragging: bool,
    /// Local volume override during drag for instant visual feedback.
    drag_volume: Option<f32>,
}

struct MenuPopup {
    surface: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,
    hover_item: Option<usize>,
}

pub struct Header {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    layer_shell: LayerShell,

    layer_surface: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,

    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer_x: f32,

    font: fontdue::Font,
    state: SystemState,
    zones: HitZones,

    popup: Option<Popup>,
    menu_popup: Option<MenuPopup>,
    exit: bool,
}

impl Header {
    pub fn run() {
        let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
        let (globals, event_queue) =
            registry_queue_init::<Header>(&conn).expect("Failed to init registry");
        let qh = event_queue.handle();

        let compositor_state =
            CompositorState::bind(&globals, &qh).expect("Compositor not available");
        let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
        let shm = Shm::bind(&globals, &qh).expect("SHM not available");

        let width = 1920;
        let height = render::BAR_HEIGHT;

        let surface = compositor_state.create_surface(&qh);
        let layer_surface = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Top,
            Some("header"),
            None,
        );

        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_size(0, height);
        layer_surface.set_exclusive_zone(height as i32);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.wl_surface().commit();

        let pool = SlotPool::new(width as usize * height as usize * 4, &shm)
            .expect("Failed to create SHM pool");

        let font = render::load_font();
        let state = SystemState::poll();
        let zones = HitZones {
            mars_icon: (0.0, 0.0),
            volume: (0.0, 0.0),
            brightness: None,
        };

        let mut header = Header {
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
            pointer: None,
            keyboard: None,
            pointer_x: 0.0,
            font,
            state,
            zones,
            popup: None,
            menu_popup: None,
            exit: false,
        };

        let mut event_loop: EventLoop<Header> =
            EventLoop::try_new().expect("Failed to create event loop");
        let loop_handle = event_loop.handle();

        WaylandSource::new(conn, event_queue)
            .insert(loop_handle.clone())
            .expect("Failed to insert Wayland source");

        // Timer for time display (every second) and brightness (every 5s)
        let timer = Timer::from_duration(Duration::from_secs(1));
        loop_handle
            .insert_source(timer, |_, _, header: &mut Header| {
                let new_time = controls::poll_time();
                if new_time != header.state.time_str {
                    header.state.time_str = new_time;
                    header.needs_redraw = true;
                }
                // Poll brightness less frequently (sysfs read, cheap but no event source)
                header.state.brightness = controls::poll_brightness();
                TimeoutAction::ToDuration(Duration::from_secs(1))
            })
            .expect("Failed to insert timer");

        // Event-driven volume monitoring via pactl subscribe
        let (vol_tx, vol_rx) = channel::channel::<()>();
        std::thread::spawn(move || {
            loop {
                let mut child = match Command::new("pactl")
                    .args(["subscribe"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(_) => {
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };
                if let Some(stdout) = child.stdout.take() {
                    let reader = std::io::BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        if line.contains("'change'")
                            && line.contains("sink")
                            && !line.contains("sink-input")
                        {
                            if vol_tx.send(()).is_err() {
                                return;
                            }
                        }
                    }
                }
                let _ = child.wait();
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        loop_handle
            .insert_source(vol_rx, |event, _, header: &mut Header| {
                if let channel::Event::Msg(()) = event {
                    let (vol, muted) = controls::poll_volume();
                    header.state.volume = vol;
                    header.state.muted = muted;
                    header.needs_redraw = true;
                    if let Some(p) = header.popup.as_mut() {
                        p.needs_redraw = true;
                    }
                }
            })
            .expect("Failed to insert volume monitor");

        loop {
            if header.exit {
                break;
            }

            header.draw();
            header.draw_popup();
            header.draw_menu();

            event_loop
                .dispatch(Duration::from_millis(16), &mut header)
                .expect("Event loop dispatch failed");
        }
    }

    fn open_popup(&mut self, qh: &QueueHandle<Self>) {
        if self.popup.is_some() {
            return;
        }
        self.menu_popup.take(); // close menu if open

        let surface = self.compositor_state.create_surface(qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("header-popup"),
            None,
        );

        let right_margin = (self.width as f32 - self.zones.volume.1) as i32;

        layer_surface.set_anchor(Anchor::TOP | Anchor::RIGHT);
        layer_surface.set_size(render::POPUP_WIDTH, render::POPUP_HEIGHT);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_margin(render::BAR_HEIGHT as i32 + 4, right_margin, 0, 0);
        layer_surface.wl_surface().commit();

        let pool = SlotPool::new(
            render::POPUP_WIDTH as usize * render::POPUP_HEIGHT as usize * 4,
            &self.shm,
        )
        .expect("Failed to create popup pool");

        self.popup = Some(Popup {
            surface: layer_surface,
            pool,
            width: render::POPUP_WIDTH,
            height: render::POPUP_HEIGHT,
            configured: false,
            needs_redraw: true,
            dragging: false,
            drag_volume: None,
        });
    }

    fn close_popup(&mut self) {
        self.popup.take();
    }

    fn toggle_popup(&mut self, qh: &QueueHandle<Self>) {
        if self.popup.is_some() {
            self.close_popup();
        } else {
            self.open_popup(qh);
        }
    }

    fn refresh_state(&mut self) {
        self.state = SystemState::poll();
        self.needs_redraw = true;
        if let Some(p) = self.popup.as_mut() {
            p.needs_redraw = true;
        }
    }

    fn open_menu(&mut self, qh: &QueueHandle<Self>) {
        if self.menu_popup.is_some() {
            return;
        }
        self.popup.take(); // close volume popup if open

        let surface = self.compositor_state.create_surface(qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("header-menu"),
            None,
        );

        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer_surface.set_size(render::MENU_POPUP_WIDTH, render::MENU_POPUP_HEIGHT);
        layer_surface.set_exclusive_zone(0);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_margin(render::BAR_HEIGHT as i32 + 4, 0, 0, 4);
        layer_surface.wl_surface().commit();

        let pool = SlotPool::new(
            render::MENU_POPUP_WIDTH as usize * render::MENU_POPUP_HEIGHT as usize * 4,
            &self.shm,
        )
        .expect("Failed to create menu pool");

        self.menu_popup = Some(MenuPopup {
            surface: layer_surface,
            pool,
            width: render::MENU_POPUP_WIDTH,
            height: render::MENU_POPUP_HEIGHT,
            configured: false,
            needs_redraw: true,
            hover_item: None,
        });
    }

    fn close_menu(&mut self) {
        self.menu_popup.take();
    }

    fn toggle_menu(&mut self, qh: &QueueHandle<Self>) {
        if self.menu_popup.is_some() {
            self.close_menu();
        } else {
            self.open_menu(qh);
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

        let (pixmap, zones) = render::render_header(
            &self.font,
            width,
            &self.state.time_str,
            self.state.volume,
            self.state.muted,
            self.state.brightness,
        );
        self.zones = zones;

        rgba_to_argb(pixmap.data(), canvas, (width * height) as usize);

        let surface = self.layer_surface.wl_surface();
        surface.attach(Some(buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        self.needs_redraw = false;
    }

    fn draw_popup(&mut self) {
        let (width, height) = match &self.popup {
            Some(p) if p.configured && p.needs_redraw => (p.width, p.height),
            _ => return,
        };

        // Use drag_volume for instant feedback during slider drag
        let effective_volume = self
            .popup
            .as_ref()
            .and_then(|p| p.drag_volume)
            .unwrap_or(self.state.volume);

        let pixmap = render::render_popup(
            &self.font,
            width,
            height,
            effective_volume,
            self.state.muted,
        );

        // Now borrow popup mutably for buffer operations
        let popup = self.popup.as_mut().unwrap();
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        if popup.pool.len() < buf_size {
            popup.pool.resize(buf_size).expect("resize popup pool");
        }

        let (buffer, canvas) = popup
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create popup buffer");

        rgba_to_argb(pixmap.data(), canvas, (width * height) as usize);

        let surface = popup.surface.wl_surface();
        surface.attach(Some(buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        popup.needs_redraw = false;
    }

    fn draw_menu(&mut self) {
        let (width, height, hover_item) = match &self.menu_popup {
            Some(p) if p.configured && p.needs_redraw => (p.width, p.height, p.hover_item),
            _ => return,
        };

        let pixmap = render::render_menu_popup(&self.font, width, height, hover_item);

        let menu = self.menu_popup.as_mut().unwrap();
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        if menu.pool.len() < buf_size {
            menu.pool.resize(buf_size).expect("resize menu pool");
        }

        let (buffer, canvas) = menu
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create menu buffer");

        rgba_to_argb(pixmap.data(), canvas, (width * height) as usize);

        let surface = menu.surface.wl_surface();
        surface.attach(Some(buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        menu.needs_redraw = false;
    }
}

fn rgba_to_argb(src: &[u8], dst: &mut [u8], pixel_count: usize) {
    for i in 0..pixel_count {
        let si = i * 4;
        dst[si] = src[si + 2];     // B
        dst[si + 1] = src[si + 1]; // G
        dst[si + 2] = src[si];     // R
        dst[si + 3] = src[si + 3]; // A
    }
}

// ---- Pointer handling ----

impl PointerHandler for Header {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let on_header = event.surface == *self.layer_surface.wl_surface();
            let on_popup = self
                .popup
                .as_ref()
                .map_or(false, |p| event.surface == *p.surface.wl_surface());
            let on_menu = self
                .menu_popup
                .as_ref()
                .map_or(false, |p| event.surface == *p.surface.wl_surface());

            if on_header {
                match event.kind {
                    PointerEventKind::Motion { .. } => {
                        self.pointer_x = event.position.0 as f32;
                    }
                    PointerEventKind::Press {
                        button: 0x110, // BTN_LEFT
                        ..
                    } => {
                        let x = self.pointer_x;
                        if x >= self.zones.mars_icon.0 && x <= self.zones.mars_icon.1 {
                            self.popup.take();
                            self.toggle_menu(qh);
                        } else if x >= self.zones.volume.0 && x <= self.zones.volume.1 {
                            self.menu_popup.take();
                            self.toggle_popup(qh);
                        } else {
                            self.close_popup();
                            self.close_menu();
                        }
                    }
                    PointerEventKind::Axis {
                        horizontal: _,
                        vertical,
                        ..
                    } => {
                        let scroll = if vertical.discrete != 0 {
                            -vertical.discrete as f32 * 0.05
                        } else if vertical.absolute.abs() > 0.0 {
                            -(vertical.absolute as f32) * 0.002
                        } else {
                            0.0
                        };

                        if scroll.abs() < 0.001 {
                            continue;
                        }

                        let x = self.pointer_x;

                        if let Some((left, right)) = self.zones.brightness {
                            if x >= left && x <= right {
                                controls::adjust_brightness(scroll);
                                self.refresh_state();
                                continue;
                            }
                        }

                        controls::adjust_volume(scroll);
                        self.refresh_state();
                    }
                    _ => {}
                }
            } else if on_popup {
                let x = event.position.0 as f32;
                let y = event.position.1 as f32;

                match event.kind {
                    PointerEventKind::Motion { .. } => {
                        let dragging = self.popup.as_ref().map_or(false, |p| p.dragging);
                        if dragging {
                            let vol = render::slider_value_at(x);
                            // Update local volume for instant visual feedback
                            if let Some(p) = self.popup.as_mut() {
                                p.drag_volume = Some(vol);
                                p.needs_redraw = true;
                            }
                            // Fire-and-forget volume change (non-blocking)
                            controls::set_volume(vol);
                        }
                    }
                    PointerEventKind::Press {
                        button: 0x110,
                        ..
                    } => {
                        if render::is_on_mute_icon(x, y) {
                            controls::toggle_mute();
                            self.refresh_state();
                        } else if render::is_on_slider(x, y) {
                            let vol = render::slider_value_at(x);
                            if let Some(p) = self.popup.as_mut() {
                                p.dragging = true;
                                p.drag_volume = Some(vol);
                                p.needs_redraw = true;
                            }
                            controls::set_volume(vol);
                        }
                    }
                    PointerEventKind::Release {
                        button: 0x110,
                        ..
                    } => {
                        if let Some(p) = self.popup.as_mut() {
                            p.dragging = false;
                            p.drag_volume = None;
                        }
                        // Sync actual state now that drag is done
                        self.refresh_state();
                    }
                    PointerEventKind::Axis {
                        vertical,
                        ..
                    } => {
                        let scroll = if vertical.discrete != 0 {
                            -vertical.discrete as f32 * 0.05
                        } else if vertical.absolute.abs() > 0.0 {
                            -(vertical.absolute as f32) * 0.002
                        } else {
                            0.0
                        };
                        if scroll.abs() > 0.001 {
                            controls::adjust_volume(scroll);
                            self.refresh_state();
                        }
                    }
                    _ => {}
                }
            } else if on_menu {
                match event.kind {
                    PointerEventKind::Motion { .. } => {
                        let item = render::menu_item_at(event.position.1 as f32);
                        if let Some(mp) = self.menu_popup.as_mut() {
                            if mp.hover_item != item {
                                mp.hover_item = item;
                                mp.needs_redraw = true;
                            }
                        }
                    }
                    PointerEventKind::Press {
                        button: 0x110,
                        ..
                    } => {
                        if let Some(item) = render::menu_item_at(event.position.1 as f32) {
                            match item {
                                0 => controls::logout(),
                                1 => controls::restart(),
                                2 => controls::shutdown(),
                                _ => {}
                            }
                            self.close_menu();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---- SCTK handler implementations ----

impl CompositorHandler for Header {
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

impl OutputHandler for Header {
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

impl LayerShellHandler for Header {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(menu) = &self.menu_popup {
            if layer.wl_surface() == menu.surface.wl_surface() {
                self.menu_popup = None;
                return;
            }
        }
        if let Some(popup) = &self.popup {
            if layer.wl_surface() == popup.surface.wl_surface() {
                self.popup = None;
                return;
            }
        }
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        // Check menu popup
        if let Some(menu) = &mut self.menu_popup {
            if layer.wl_surface() == menu.surface.wl_surface() {
                if configure.new_size.0 > 0 {
                    menu.width = configure.new_size.0;
                }
                if configure.new_size.1 > 0 {
                    menu.height = configure.new_size.1;
                }
                menu.configured = true;
                menu.needs_redraw = true;
                return;
            }
        }

        // Check volume popup
        if let Some(popup) = &mut self.popup {
            if layer.wl_surface() == popup.surface.wl_surface() {
                if configure.new_size.0 > 0 {
                    popup.width = configure.new_size.0;
                }
                if configure.new_size.1 > 0 {
                    popup.height = configure.new_size.1;
                }
                popup.configured = true;
                popup.needs_redraw = true;
                return;
            }
        }

        // Header surface
        if configure.new_size.0 > 0 {
            self.width = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.height = configure.new_size.1;
        }
        self.configured = true;
        self.needs_redraw = true;
    }
}

impl SeatHandler for Header {
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
        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to get pointer");
            self.pointer = Some(pointer);
        }
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

impl ShmHandler for Header {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Header {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl KeyboardHandler for Header {
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
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(popup) = &self.popup {
            if surface == popup.surface.wl_surface() {
                self.close_popup();
            }
        }
        if let Some(menu) = &self.menu_popup {
            if surface == menu.surface.wl_surface() {
                self.close_menu();
            }
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.close_popup();
            self.close_menu();
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

delegate_compositor!(Header);
delegate_output!(Header);
delegate_layer!(Header);
delegate_registry!(Header);
delegate_seat!(Header);
delegate_shm!(Header);
delegate_pointer!(Header);
delegate_keyboard!(Header);
