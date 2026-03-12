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
pub mod text_input_state;
pub mod wayland;

pub use color::*;
pub use element::*;
pub use style::*;
pub use display_list::Point;
pub use app::{SurfaceConfig, View, WaylandContext};
pub use reactive::{Reactive, RenderContext};
pub use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
pub use handle::Handle;
pub use input::CursorStyle;
pub use animation::{Animation, Easing, From, To};

/// Run a view as a Wayland application.
pub fn run<V: View>(view: V, config: SurfaceConfig) {
    wayland::run_wayland(view, config);
}
