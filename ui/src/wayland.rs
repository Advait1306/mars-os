//! Wayland bootstrap and event loop for the UI framework.
//!
//! Connects to a Wayland compositor, creates a layer-shell surface (or toplevel),
//! and runs the render pipeline: View::render() -> layout -> display_list -> SkiaRenderer -> SHM buffer.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, delegate_touch,
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface, wl_touch},
        Connection, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{
            KeyEvent, KeyboardHandler, Keysym,
            Modifiers as SctkModifiers,
        },
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        touch::{TouchHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{
        LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
    },
    shell::WaylandSurface,
    shm::{slot::SlotPool, Shm, ShmHandler},
};

use wayland_client::protocol::wl_registry;
use wayland_client::Dispatch;
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3, zwp_text_input_v3,
};

use crate::animator::{collect_keyed_elements, Animator};
use crate::app::{SurfaceConfig, View, WaylandContext};
use crate::display_list::build_display_list;
use crate::element::Element;
use crate::event_dispatch::EventState;
use crate::handle::MutationQueue;
use crate::input::{InputEvent, MouseButton};
use crate::layout::{compute_layout, LayoutNode};
use crate::reactive::{self, RenderContext};
use crate::renderer::SkiaRenderer;

/// Type-erased view operations, constructed once in `run_wayland` with closures
/// that capture the concrete view type `V`.
struct ViewState {
    /// Render the view, producing an element tree.
    render_fn: Box<dyn Fn(&mut RenderContext) -> Element>,
    /// Apply all pending mutations from the Handle queue to the view.
    apply_fn: Box<dyn Fn()>,
    /// Check if there are pending mutations in the Handle queue.
    has_mutations_fn: Box<dyn Fn() -> bool>,
    /// Call View::tick() each frame before render.
    tick_fn: Box<dyn Fn()>,
    /// The mutation queue as `Rc<dyn Any>`, passed to RenderContext so it can
    /// hand out typed `Handle<V>` via downcast.
    mutations_rc: Rc<dyn Any>,
}

struct WaylandState {
    // SCTK state
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    #[allow(dead_code)]
    layer_shell: LayerShell,

    // Surface
    layer_surface: Option<LayerSurface>,
    pool: Option<SlotPool>,
    width: u32,
    height: u32,
    #[allow(dead_code)]
    scale_factor: i32,
    configured: bool,
    needs_redraw: bool,
    exit: bool,

    // Framework
    view_state: ViewState,
    renderer: SkiaRenderer,

    // Animation
    animator: Animator,
    last_tick: Instant,

    // Input / event dispatch
    pending_events: Vec<InputEvent>,
    event_state: EventState,
    last_layout: Option<LayoutNode>,
    last_element_tree: Option<Element>,

    // Keyboard modifier state (updated by SCTK)
    current_modifiers: crate::input::Modifiers,

    // Text input (IME) via zwp_text_input_v3
    text_input_manager: Option<zwp_text_input_manager_v3::ZwpTextInputManagerV3>,
    text_input: Option<zwp_text_input_v3::ZwpTextInputV3>,
    /// Accumulated preedit from the pending text_input.done cycle.
    pending_preedit: Option<(String, Option<i32>, Option<i32>)>,
    /// Accumulated commit string from the pending text_input.done cycle.
    pending_commit: Option<String>,
    /// Whether the text input is currently enabled (active for the focused element).
    text_input_enabled: bool,
}

impl WaylandState {
    /// Enable or disable the zwp_text_input_v3 based on whether a text input element is focused.
    fn sync_text_input_focus(&mut self) {
        let should_enable = if let Some(ref elements) = self.last_element_tree {
            self.event_state.focused_element_is_text_input(elements)
        } else {
            false
        };

        if should_enable && !self.text_input_enabled {
            if let Some(ref ti) = self.text_input {
                ti.enable();
                ti.commit();
            }
            self.text_input_enabled = true;
        } else if !should_enable && self.text_input_enabled {
            if let Some(ref ti) = self.text_input {
                ti.disable();
                ti.commit();
            }
            self.text_input_enabled = false;
        }
    }

    /// Process any pending input events against the cached layout/element trees.
    fn process_input_events(&mut self) {
        if self.pending_events.is_empty() {
            return;
        }
        let events = std::mem::take(&mut self.pending_events);
        if let (Some(layout), Some(elements)) = (&self.last_layout, &self.last_element_tree) {
            for event in &events {
                if self.event_state.dispatch(event, layout, elements) {
                    self.needs_redraw = true;
                }
            }
        }
    }

    fn draw(&mut self) {
        if !self.configured || !self.needs_redraw {
            return;
        }

        // Apply any pending mutations from Handle::update() calls
        (self.view_state.apply_fn)();

        // Clear dirty reactive state (mutations may have called Reactive::set)
        reactive::take_dirty();

        // Render through the framework pipeline
        let mut cx = RenderContext::new(
            self.view_state.mutations_rc.clone(),
            self.width,
            self.height,
        );
        let element_tree = (self.view_state.render_fn)(&mut cx);

        // Handle surface resize request from the view
        if let Some((new_w, new_h)) = cx.take_requested_size() {
            if new_w != self.width || new_h != self.height {
                self.width = new_w;
                self.height = new_h;
                if let Some(ls) = &self.layer_surface {
                    ls.set_size(new_w, new_h);
                    ls.wl_surface().commit();
                }
            }
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

        let (buffer, canvas_data) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("Failed to create buffer");
        let (layout_tree, _elements) = compute_layout(&element_tree, width as f32, height as f32, &self.renderer.font_collection());

        // Diff keyed elements for enter/exit/layout animations
        let (keyed_infos, keyed_bounds) =
            collect_keyed_elements(&element_tree, &layout_tree);

        // Detect exits: keys that were in prev_bounds but are no longer present.
        // We need to start exit animations before diff_and_update clears them.
        if let Some(ref prev_tree) = self.last_element_tree {
            let prev_layout = self.last_layout.as_ref();
            if let Some(prev_layout) = prev_layout {
                let (prev_infos, prev_bounds) =
                    collect_keyed_elements(prev_tree, prev_layout);
                for (key, info) in &prev_infos {
                    if !keyed_infos.contains_key(key) {
                        if let Some(ref exit) = info.exit {
                            if let Some(bounds) = prev_bounds.get(key) {
                                self.animator.start_exit(key, exit, bounds);
                            }
                        }
                    }
                }
            }
        }

        self.animator.diff_and_update(&keyed_infos, &keyed_bounds);

        let commands = build_display_list(&layout_tree, &element_tree, Some(&self.animator));

        // Create a Skia raster surface and render
        // Skia N32 premul on little-endian = BGRA = Wayland ARGB8888
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((width as i32, height as i32))
                .expect("Failed to create Skia surface");

        {
            let skia_canvas = surface.canvas();
            skia_canvas.clear(skia_safe::Color::TRANSPARENT);
            self.renderer.execute(skia_canvas, &commands);
        }

        // Read pixels from Skia surface into the SHM buffer
        let image_info = surface.image_info();
        let row_bytes = width as usize * 4;
        surface.read_pixels(
            &image_info,
            canvas_data,
            row_bytes,
            (0, 0),
        );

        if let Some(ls) = &self.layer_surface {
            let wl_surface = ls.wl_surface();
            wl_surface.attach(Some(buffer.wl_buffer()), 0, 0);
            wl_surface.damage_buffer(0, 0, width as i32, height as i32);
            wl_surface.commit();
        }

        // Cache layout and element tree for input event dispatch
        self.last_layout = Some(layout_tree);
        self.last_element_tree = Some(element_tree);

        self.needs_redraw = false;
    }
}

// --- Smithay handler implementations ---

impl CompositorHandler for WaylandState {
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

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for WaylandState {
    fn closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
    ) {
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

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
    ) {
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.seat_state.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Touch {
            self.seat_state.get_touch(qh, &seat).ok();
        }
        if capability == Capability::Keyboard {
            self.seat_state.get_keyboard(qh, &seat, None).ok();
            // Create text input instance for IME support
            if self.text_input.is_none() {
                if let Some(ref manager) = self.text_input_manager {
                    let ti = manager.get_text_input(&seat, qh, ());
                    self.text_input = Some(ti);
                }
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }

    fn remove_seat(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
    ) {
    }
}

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let input = match event.kind {
                PointerEventKind::Enter { .. } => Some(InputEvent::PointerMove {
                    x: event.position.0 as f32,
                    y: event.position.1 as f32,
                }),
                PointerEventKind::Leave { .. } => Some(InputEvent::PointerLeave),
                PointerEventKind::Motion { .. } => Some(InputEvent::PointerMove {
                    x: event.position.0 as f32,
                    y: event.position.1 as f32,
                }),
                PointerEventKind::Press { button, time, .. } => {
                    let mb = match button {
                        272 => MouseButton::Left,    // BTN_LEFT
                        273 => MouseButton::Right,   // BTN_RIGHT
                        274 => MouseButton::Middle,  // BTN_MIDDLE
                        275 => MouseButton::Back,    // BTN_SIDE
                        276 => MouseButton::Forward, // BTN_EXTRA
                        _ => MouseButton::Left,
                    };
                    Some(InputEvent::PointerButton {
                        x: event.position.0 as f32,
                        y: event.position.1 as f32,
                        button: mb,
                        pressed: true,
                        time,
                    })
                }
                PointerEventKind::Release { button, time, .. } => {
                    let mb = match button {
                        272 => MouseButton::Left,
                        273 => MouseButton::Right,
                        274 => MouseButton::Middle,
                        275 => MouseButton::Back,
                        276 => MouseButton::Forward,
                        _ => MouseButton::Left,
                    };
                    Some(InputEvent::PointerButton {
                        x: event.position.0 as f32,
                        y: event.position.1 as f32,
                        button: mb,
                        pressed: false,
                        time,
                    })
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    source,
                    time,
                } => {
                    use smithay_client_toolkit::reexports::client::protocol::wl_pointer::AxisSource;
                    let scroll_source = source.map(|s| match s {
                        AxisSource::Wheel => crate::input::ScrollSource::Wheel,
                        AxisSource::Finger => crate::input::ScrollSource::Finger,
                        AxisSource::Continuous => crate::input::ScrollSource::Continuous,
                        AxisSource::WheelTilt => crate::input::ScrollSource::WheelTilt,
                        _ => crate::input::ScrollSource::Wheel,
                    });
                    Some(InputEvent::PointerScroll {
                        x: event.position.0 as f32,
                        y: event.position.1 as f32,
                        delta_x: horizontal.absolute as f32,
                        delta_y: vertical.absolute as f32,
                        source: scroll_source,
                        discrete_x: horizontal.discrete,
                        discrete_y: vertical.discrete,
                        stop: horizontal.stop || vertical.stop,
                        time,
                    })
                }
            };

            if let Some(input_event) = input {
                self.pending_events.push(input_event);
            }
        }
    }
}

impl KeyboardHandler for WaylandState {
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
        // Surface gained keyboard focus
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Surface lost keyboard focus
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let modifiers = self.current_modifiers;
        let key = crate::input::Key(event.keysym.raw());

        // Queue KeyDown event
        self.pending_events.push(InputEvent::KeyDown {
            key,
            modifiers,
        });

        // If the key produces text and we're not in an IME composition, queue a TextInput event.
        // During IME composition, text comes through the zwp_text_input_v3 protocol instead.
        if !self.event_state.is_composing {
            if let Some(ref text) = event.utf8 {
                if !text.is_empty() && !text.chars().any(|c| c.is_control()) {
                    self.pending_events.push(InputEvent::TextInput {
                        text: text.clone(),
                    });
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
        event: KeyEvent,
    ) {
        let modifiers = self.current_modifiers;
        let key = crate::input::Key(event.keysym.raw());

        self.pending_events.push(InputEvent::KeyUp {
            key,
            modifiers,
        });
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: SctkModifiers,
        _: u32,
    ) {
        self.current_modifiers = crate::input::Modifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            super_: modifiers.logo,
            caps_lock: modifiers.caps_lock,
            num_lock: modifiers.num_lock,
        };
    }
}

impl TouchHandler for WaylandState {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        _surface: wl_surface::WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        self.pending_events.push(InputEvent::TouchDown {
            id,
            x: position.0 as f32,
            y: position.1 as f32,
            time,
        });
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        id: i32,
    ) {
        self.pending_events.push(InputEvent::TouchUp { id, time });
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        self.pending_events.push(InputEvent::TouchMotion {
            id,
            x: position.0 as f32,
            y: position.1 as f32,
            time,
        });
    }

    fn cancel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
    ) {
        self.pending_events.push(InputEvent::TouchCancel);
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _id: i32,
        _major: f64,
        _minor: f64,
    ) {
        // TODO: Store shape data for touch events
    }

    fn orientation(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _id: i32,
        _orientation: f64,
    ) {
        // TODO: Store orientation data for touch events
    }
}

impl ShmHandler for WaylandState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_layer!(WaylandState);
delegate_pointer!(WaylandState);
delegate_keyboard!(WaylandState);
delegate_touch!(WaylandState);
delegate_registry!(WaylandState);
delegate_seat!(WaylandState);
delegate_shm!(WaylandState);

// --- zwp_text_input_v3 handlers ---

impl Dispatch<zwp_text_input_manager_v3::ZwpTextInputManagerV3, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &zwp_text_input_manager_v3::ZwpTextInputManagerV3,
        _: zwp_text_input_manager_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The manager has no events.
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _text_input: &zwp_text_input_v3::ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                // Accumulate preedit — applied on Done
                state.pending_preedit = text.map(|t| (t, Some(cursor_begin), Some(cursor_end)));
            }

            zwp_text_input_v3::Event::CommitString { text } => {
                // Accumulate commit — applied on Done
                state.pending_commit = text;
            }

            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length: _,
                after_length: _,
            } => {
                // TODO: Implement delete-surrounding-text support.
                // This requires tracking the surrounding text and cursor position,
                // which needs cooperation from the text input element state.
            }

            zwp_text_input_v3::Event::Done { serial: _ } => {
                // The compositor signals that the current batch of updates is complete.
                // Process accumulated preedit and commit.

                let was_composing = state.event_state.is_composing;

                if let Some(commit) = state.pending_commit.take() {
                    // Commit string present — end any composition and emit text input
                    if was_composing {
                        state.pending_events.push(InputEvent::CompositionEnd {
                            text: commit.clone(),
                        });
                    } else {
                        // Direct commit without prior composition
                        state.pending_events.push(InputEvent::TextInput { text: commit });
                    }
                    // Clear preedit since we committed
                    state.pending_preedit = None;
                } else if let Some((preedit_text, cursor_begin, cursor_end)) =
                    state.pending_preedit.take()
                {
                    if !was_composing {
                        // Start new composition
                        state.pending_events.push(InputEvent::CompositionStart);
                    }
                    // Update composition with preedit
                    let cb = cursor_begin.and_then(|v| if v >= 0 { Some(v as usize) } else { None });
                    let ce = cursor_end.and_then(|v| if v >= 0 { Some(v as usize) } else { None });
                    state.pending_events.push(InputEvent::CompositionUpdate {
                        text: preedit_text,
                        cursor_begin: cb,
                        cursor_end: ce,
                    });
                } else if was_composing {
                    // No preedit and no commit but we were composing — composition ended with empty
                    state.pending_events.push(InputEvent::CompositionEnd {
                        text: String::new(),
                    });
                }
            }

            _ => {}
        }
    }
}

// --- Public entry point ---

pub fn run_wayland<V: View>(view: V, config: SurfaceConfig) {
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, event_queue) =
        registry_queue_init::<WaylandState>(&conn).expect("Failed to init registry");
    let qh = event_queue.handle();

    // Create WaylandContext for View::setup()
    let wl_context = WaylandContext {
        connection: conn.clone(),
        globals,
    };

    let compositor_state =
        CompositorState::bind(&wl_context.globals, &qh).expect("Compositor not available");
    let layer_shell =
        LayerShell::bind(&wl_context.globals, &qh).expect("Layer shell not available");
    let shm = Shm::bind(&wl_context.globals, &qh).expect("SHM not available");

    // Bind zwp_text_input_manager_v3 if available (not fatal if missing)
    let text_input_manager: Option<zwp_text_input_manager_v3::ZwpTextInputManagerV3> =
        wl_context.globals.bind(&qh, 1..=1, ()).ok();

    let (width, height, layer_surface) = match &config {
        SurfaceConfig::LayerShell {
            namespace,
            layer,
            anchor,
            size,
            exclusive_zone,
            keyboard,
            margin,
        } => {
            let surface = compositor_state.create_surface(&qh);
            let ls = layer_shell.create_layer_surface(
                &qh,
                surface,
                *layer,
                Some(namespace.as_str()),
                None,
            );
            ls.set_anchor(*anchor);
            ls.set_size(size.0, size.1);
            ls.set_exclusive_zone(*exclusive_zone);
            ls.set_keyboard_interactivity(*keyboard);
            ls.set_margin(margin.0, margin.1, margin.2, margin.3);
            ls.wl_surface().commit();

            (size.0, size.1, Some(ls))
        }
        SurfaceConfig::Toplevel { .. } => {
            // TODO: xdg_toplevel support in future phase
            panic!("Toplevel not yet supported");
        }
    };

    let pool = SlotPool::new(
        (width as usize * height as usize * 4).max(4096),
        &shm,
    )
    .expect("Failed to create SHM pool");

    // Create the mutation queue and type-erased view state.
    // The view is stored in Rc<RefCell<V>> so closures can borrow it.
    let view = Rc::new(RefCell::new(view));

    // Call View::setup() with Wayland context before the event loop
    view.borrow_mut().setup(&wl_context);

    let mutations: Rc<MutationQueue<V>> = MutationQueue::new();

    let view_for_render = Rc::clone(&view);
    let render_fn = Box::new(move |cx: &mut RenderContext| -> Element {
        view_for_render.borrow().render(cx)
    });

    let view_for_mutate = Rc::clone(&view);
    let mutations_for_apply = Rc::clone(&mutations);
    let apply_fn = Box::new(move || {
        let muts = mutations_for_apply.drain();
        if !muts.is_empty() {
            let mut v = view_for_mutate.borrow_mut();
            for f in muts {
                f(&mut *v);
            }
        }
    });

    let mutations_for_check = Rc::clone(&mutations);
    let has_mutations_fn = Box::new(move || -> bool {
        !mutations_for_check.is_empty()
    });

    let view_for_tick = Rc::clone(&view);
    let tick_fn = Box::new(move || {
        view_for_tick.borrow_mut().tick();
    });

    let mutations_rc: Rc<dyn Any> = mutations;

    let view_state = ViewState {
        render_fn,
        apply_fn,
        has_mutations_fn,
        tick_fn,
        mutations_rc,
    };

    let mut state = WaylandState {
        registry_state: RegistryState::new(&wl_context.globals),
        seat_state: SeatState::new(&wl_context.globals, &qh),
        output_state: OutputState::new(&wl_context.globals, &qh),
        compositor_state,
        shm,
        layer_shell,
        layer_surface,
        pool: Some(pool),
        width,
        height,
        scale_factor: 1,
        configured: false,
        needs_redraw: true,
        exit: false,
        view_state,
        renderer: SkiaRenderer::new(),
        animator: Animator::new(),
        last_tick: Instant::now(),
        pending_events: Vec::new(),
        event_state: EventState::new(),
        last_layout: None,
        last_element_tree: None,
        current_modifiers: crate::input::Modifiers::default(),
        text_input_manager,
        text_input: None,
        pending_preedit: None,
        pending_commit: None,
        text_input_enabled: false,
    };

    let mut event_loop: EventLoop<WaylandState> =
        EventLoop::try_new().expect("Failed to create event loop");
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn, event_queue)
        .insert(loop_handle)
        .expect("Failed to insert Wayland source");

    loop {
        if state.exit {
            break;
        }

        // Process pending input events against the cached layout/element trees
        state.process_input_events();

        // Sync text input protocol state with focus (enable/disable IME)
        state.sync_text_input_focus();

        // Call View::tick() for custom event queue polling
        (state.view_state.tick_fn)();

        // Step animations
        let now = Instant::now();
        let dt = now.duration_since(state.last_tick).as_secs_f32();
        state.last_tick = now;
        if state.animator.step(dt) {
            state.needs_redraw = true;
        }

        // Check for pending mutations or dirty reactive state and trigger redraw
        if (state.view_state.has_mutations_fn)() || reactive::is_dirty() {
            state.needs_redraw = true;
        }

        state.draw();
        event_loop
            .dispatch(Duration::from_millis(16), &mut state)
            .expect("Event loop dispatch failed");
    }
}
