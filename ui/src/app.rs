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
    },
    Toplevel {
        title: String,
        app_id: String,
    },
}

/// The View trait — implementors describe UI as an element tree.
/// For Phase 3, this is a simple trait without RenderContext (that comes in Phase 4).
pub trait View: 'static {
    fn render(&self) -> crate::element::Element;
}
