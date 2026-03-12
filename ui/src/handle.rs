//! Handle and MutationQueue for queueing view mutations from closures.
//!
//! `Handle<V>` is a clonable handle that lets event handlers and callbacks
//! queue mutations to the view state. Mutations are applied before the next render.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// A shared mutation queue for a view of type `V`.
///
/// Mutations are boxed `FnOnce(&mut V)` closures that will be applied
/// to the view before the next render pass.
pub struct MutationQueue<V> {
    queue: RefCell<VecDeque<Box<dyn FnOnce(&mut V)>>>,
}

impl<V> MutationQueue<V> {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            queue: RefCell::new(VecDeque::new()),
        })
    }

    pub fn push(&self, f: impl FnOnce(&mut V) + 'static) {
        self.queue.borrow_mut().push_back(Box::new(f));
    }

    pub fn drain(&self) -> Vec<Box<dyn FnOnce(&mut V)>> {
        self.queue.borrow_mut().drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.borrow().is_empty()
    }
}

/// A clonable handle for mutating view state from closures.
///
/// Obtained via `cx.handle::<MyView>()` inside `View::render()`.
/// Mutations queued via `update()` are applied before the next render.
pub struct Handle<V> {
    mutations: Rc<MutationQueue<V>>,
}

impl<V> Handle<V> {
    pub fn new(mutations: Rc<MutationQueue<V>>) -> Self {
        Self { mutations }
    }

    /// Queue a mutation to the view state. Will be applied before the next render.
    pub fn update(&self, f: impl FnOnce(&mut V) + 'static) {
        self.mutations.push(f);
    }
}

impl<V> Clone for Handle<V> {
    fn clone(&self) -> Self {
        Self {
            mutations: Rc::clone(&self.mutations),
        }
    }
}
