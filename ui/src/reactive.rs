//! Reactive state primitives for the UI framework.
//!
//! `Reactive<T>` is a value wrapper that triggers re-renders when mutated.
//! `RenderContext` is passed to `View::render()` and enables dependency tracking
//! and access to `Handle<V>` for queueing mutations from closures.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::View;
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

/// Render context passed to `View::render()` for dependency tracking and handle access.
pub struct RenderContext {
    mutations_any: Rc<dyn Any>,
}

impl RenderContext {
    pub fn new(mutations_any: Rc<dyn Any>) -> Self {
        Self { mutations_any }
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
}
