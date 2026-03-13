use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::Connection;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

/// Configuration for the surface to create.
pub enum SurfaceConfig {
    LayerShell {
        namespace: String,
        layer: Layer,
        anchor: Anchor,
        size: (u32, u32),
        exclusive_zone: i32,
        keyboard: KeyboardInteractivity,
        margin: (i32, i32, i32, i32),
    },
    Toplevel {
        title: String,
        app_id: String,
    },
}

/// Exposes the Wayland connection and globals for binding custom protocols.
pub struct WaylandContext {
    pub connection: Connection,
    pub globals: GlobalList,
}

/// The View trait -- implementors describe UI as an element tree.
///
/// The `RenderContext` parameter enables dependency tracking (reading `Reactive<T>` values)
/// and provides `cx.handle::<Self>()` for queueing mutations from closures.
pub trait View: 'static {
    /// Called once after Wayland connection is established, before the event loop.
    /// Use to bind custom Wayland protocols on the shared connection.
    fn setup(&mut self, _wl: &WaylandContext) {}

    /// Called each frame before render(). Use for polling custom event queues,
    /// updating internal state, etc.
    fn tick(&mut self) {}

    /// Build the element tree for this frame.
    fn render(&self, cx: &mut crate::reactive::RenderContext) -> crate::element::Element;

    /// Override the default theme. Called once at initialization.
    fn theme(&self) -> crate::theme::Theme {
        crate::theme::Theme::default()
    }
}
