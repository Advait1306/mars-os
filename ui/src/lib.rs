pub mod color;
pub mod element;
pub mod style;
pub mod layout;
pub mod display_list;
pub mod renderer;
pub mod app;
pub mod wayland;

pub use color::*;
pub use element::*;
pub use style::*;
pub use display_list::Point;
pub use app::{SurfaceConfig, View};

/// Run a view as a Wayland application.
pub fn run(view: impl View, config: SurfaceConfig) {
    wayland::run_wayland(Box::new(view), config);
}
