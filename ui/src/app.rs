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

/// Configuration for a popup surface (dropdown, picker, tooltip, etc.).
///
/// Popups are additional layer-shell surfaces managed by the framework,
/// each with their own render pipeline. They are identified by a string key
/// and opened/closed via `RenderContext::open_popup()` / `close_popup()`.
pub struct PopupConfig {
    /// Anchor edges on the screen (e.g., `Anchor::TOP | Anchor::RIGHT`).
    pub anchor: Anchor,
    /// Popup dimensions in pixels (width, height).
    pub size: (u32, u32),
    /// Margin from anchor edges (top, right, bottom, left).
    pub margin: (i32, i32, i32, i32),
    /// Keyboard interactivity mode.
    /// `Exclusive` grabs keyboard focus; `OnDemand` gets focus on click; `None` never gets focus.
    pub keyboard: KeyboardInteractivity,
    /// Called when this popup's surface loses keyboard focus (e.g., user clicked outside).
    /// Typically used to close the popup by setting a view flag to false.
    pub on_focus_lost: Option<Box<dyn Fn()>>,
}

impl PopupConfig {
    /// Create a basic popup config with the given anchor and size.
    pub fn new(anchor: Anchor, size: (u32, u32)) -> Self {
        Self {
            anchor,
            size,
            margin: (0, 0, 0, 0),
            keyboard: KeyboardInteractivity::Exclusive,
            on_focus_lost: None,
        }
    }

    /// Set the margin from anchor edges (top, right, bottom, left).
    pub fn margin(mut self, top: i32, right: i32, bottom: i32, left: i32) -> Self {
        self.margin = (top, right, bottom, left);
        self
    }

    /// Set the keyboard interactivity mode.
    pub fn keyboard(mut self, k: KeyboardInteractivity) -> Self {
        self.keyboard = k;
        self
    }

    /// Set the focus-lost callback.
    pub fn on_focus_lost(mut self, f: impl Fn() + 'static) -> Self {
        self.on_focus_lost = Some(Box::new(f));
        self
    }
}

impl std::fmt::Debug for PopupConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopupConfig")
            .field("anchor", &self.anchor)
            .field("size", &self.size)
            .field("margin", &self.margin)
            .field("keyboard", &self.keyboard)
            .field("on_focus_lost", &self.on_focus_lost.is_some())
            .finish()
    }
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
