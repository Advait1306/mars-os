pub mod color;
pub mod element;
pub mod style;
pub mod layout;
pub mod display_list;
pub mod renderer;
pub mod app;
pub mod reactive;
pub mod handle;
pub mod input;
pub mod hit_test;
pub mod event_dispatch;
pub mod spring;
pub mod animation;
pub mod animator;
pub mod scroll;
pub mod select_state;
pub mod text_input_state;
pub mod textarea_state;
pub mod svg_render;
pub mod theme;
pub mod wayland;

pub use color::*;
pub use element::*;
pub use style::*;
pub use display_list::Point;
pub use app::{SurfaceConfig, View, WaylandContext};
pub use reactive::{Reactive, RenderContext};
pub use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
pub use handle::Handle;
pub use input::{
    BeforeInputEvent, ClickEvent, ClipboardData, ClipboardEvent, CompositionEvent, CursorStyle,
    DragData, DragEvent, DropEffect, EventPhase, EventResult, FocusEvent, InputType, KeyCode,
    KeyValue, KeyboardEvent, Modifiers, MouseButton, PointerEvent, PointerEvents, PointerType,
    ScrollEndEvent, ScrollSource, TextInputEvent, TouchEvent, WheelEvent,
};
pub use animation::{Animation, Easing, From, To};
pub use select_state::{SelectOption, SelectGroup, SelectState};
pub use theme::Theme;

/// Run a view as a Wayland application.
pub fn run<V: View>(view: V, config: SurfaceConfig) {
    wayland::run_wayland(view, config);
}
