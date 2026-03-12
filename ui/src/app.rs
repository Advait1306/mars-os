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

/// The View trait -- implementors describe UI as an element tree.
///
/// The `RenderContext` parameter enables dependency tracking (reading `Reactive<T>` values)
/// and provides `cx.handle::<Self>()` for queueing mutations from closures.
pub trait View: 'static {
    fn render(&self, cx: &mut crate::reactive::RenderContext) -> crate::element::Element;
}
