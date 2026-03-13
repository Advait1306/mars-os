//! Reactive state primitives for the UI framework.
//!
//! `Reactive<T>` is a value wrapper that triggers re-renders when mutated.
//! `RenderContext` is passed to `View::render()` and enables dependency tracking
//! and access to `Handle<V>` for queueing mutations from closures.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::app::{PopupConfig, View};
use crate::element::Element;
use crate::handle::{Handle, MutationQueue};

/// Unique identifier for a reactive field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReactiveId(u64);

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_reactive_id() -> ReactiveId {
    ReactiveId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// A reactive value that triggers re-renders when changed.
///
/// During `View::render()`, read via `get(cx)` to register a dependency.
/// Outside render (e.g. event handlers), read via `get_untracked()`.
/// Write via `set()` to mark the value dirty for re-render.
pub struct Reactive<T> {
    value: T,
    id: ReactiveId,
}

impl<T> Reactive<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            id: next_reactive_id(),
        }
    }

    /// Read the value and register a dependency (for use during render).
    pub fn get(&self, cx: &RenderContext) -> &T {
        cx.track(self.id);
        &self.value
    }

    /// Read the value without tracking (for use outside render, e.g. in event handlers).
    pub fn get_untracked(&self) -> &T {
        &self.value
    }

    /// Set the value and mark as dirty for re-render.
    pub fn set(&mut self, value: T) {
        self.value = value;
        DIRTY_SET.with(|d| d.borrow_mut().insert(self.id));
    }

    /// Get the reactive ID.
    pub fn id(&self) -> ReactiveId {
        self.id
    }
}

// Thread-local dirty set -- tracks which reactive fields have been mutated.
thread_local! {
    static DIRTY_SET: RefCell<HashSet<ReactiveId>> = RefCell::new(HashSet::new());
}

/// Check if any reactive fields are dirty and clear the dirty set.
/// Returns `true` if there were dirty fields.
pub fn take_dirty() -> bool {
    DIRTY_SET.with(|d| {
        let mut set = d.borrow_mut();
        let was_dirty = !set.is_empty();
        set.clear();
        was_dirty
    })
}

/// Check if any reactive fields are dirty without clearing.
pub fn is_dirty() -> bool {
    DIRTY_SET.with(|d| !d.borrow().is_empty())
}

/// A pending popup request collected during `render()`.
#[derive(Debug)]
pub enum PopupRequest {
    Open { key: String, config: PopupConfig },
    Close { key: String },
}

/// Render context passed to `View::render()` for dependency tracking and handle access.
pub struct RenderContext {
    mutations_any: Rc<dyn Any>,
    surface_width: u32,
    surface_height: u32,
    requested_size: Option<(u32, u32)>,
    /// Popup open/close requests accumulated during this render.
    popup_requests: Vec<PopupRequest>,
    /// Element trees provided for open popups.
    popup_elements: HashMap<String, Element>,
    /// Snapshot of currently open popup keys (set before render() is called).
    open_popups: HashSet<String>,
}

impl RenderContext {
    pub fn new(mutations_any: Rc<dyn Any>, surface_width: u32, surface_height: u32) -> Self {
        Self {
            mutations_any,
            surface_width,
            surface_height,
            requested_size: None,
            popup_requests: Vec::new(),
            popup_elements: HashMap::new(),
            open_popups: HashSet::new(),
        }
    }

    /// Current surface dimensions.
    pub fn surface_size(&self) -> (u32, u32) {
        (self.surface_width, self.surface_height)
    }

    /// Request the surface be resized. Takes effect next frame.
    pub fn set_surface_size(&mut self, width: u32, height: u32) {
        self.surface_width = width;
        self.surface_height = height;
        self.requested_size = Some((width, height));
    }

    /// Take the requested surface size, if any was set during render.
    pub fn take_requested_size(&mut self) -> Option<(u32, u32)> {
        self.requested_size.take()
    }

    /// Record that the current render depends on this reactive field.
    ///
    /// For Phase 4, we use a simple "any dirty = full re-render" strategy.
    /// Fine-grained dependency tracking (which view depends on which reactive)
    /// can be added later as an optimization.
    pub fn track(&self, _id: ReactiveId) {
        // no-op for now
    }

    /// Get a clonable handle for queueing mutations to the view.
    ///
    /// The type parameter `V` must match the concrete `View` type passed to `ui::run()`.
    pub fn handle<V: View + 'static>(&self) -> Handle<V> {
        let mutations = self
            .mutations_any
            .clone()
            .downcast::<MutationQueue<V>>()
            .expect("Handle type mismatch -- ensure V matches the view type passed to ui::run()");
        Handle::new(mutations)
    }

    // --- Popup API ---

    /// Open a popup surface. If already open with this key, the config is updated.
    /// The popup is not visible until `render_popup()` provides its element tree.
    pub fn open_popup(&mut self, key: &str, config: PopupConfig) {
        self.popup_requests.push(PopupRequest::Open {
            key: key.to_string(),
            config,
        });
    }

    /// Close a popup surface by key. No-op if not open.
    pub fn close_popup(&mut self, key: &str) {
        self.popup_requests.push(PopupRequest::Close {
            key: key.to_string(),
        });
    }

    /// Returns true if a popup with this key is currently open.
    pub fn is_popup_open(&self, key: &str) -> bool {
        self.open_popups.contains(key)
    }

    /// Provide the element tree for an open popup. Called during `render()`.
    /// If the popup isn't open (or hasn't been opened this frame), this stores the
    /// element for when the popup surface is ready.
    pub fn render_popup(&mut self, key: &str, element: Element) {
        self.popup_elements.insert(key.to_string(), element);
    }

    /// Set the snapshot of currently open popup keys.
    /// Called by the framework before `render()`.
    pub fn set_open_popups(&mut self, keys: HashSet<String>) {
        self.open_popups = keys;
    }

    /// Take all popup requests accumulated during this render.
    /// Called by the framework after `render()`.
    pub fn take_popup_requests(&mut self) -> Vec<PopupRequest> {
        std::mem::take(&mut self.popup_requests)
    }

    /// Take all popup element trees accumulated during this render.
    /// Called by the framework after `render()`.
    pub fn take_popup_elements(&mut self) -> HashMap<String, Element> {
        std::mem::take(&mut self.popup_elements)
    }
}
