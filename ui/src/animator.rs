//! Animation state machine — manages all active animations for keyed elements.
//!
//! The `Animator` tracks per-element, per-property animation state (spring or timed),
//! handles enter/exit transitions, and provides animated overrides that the display
//! list builder applies when rendering.

use std::collections::HashMap;

use crate::animation::{Animation, Easing, From, To};
use crate::element::Element;
use crate::layout::{LayoutNode, Rect};
use crate::spring::{SpringConfig, SpringState};

// ---------------------------------------------------------------------------
// Per-property animation state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum PropAnim {
    Spring {
        state: SpringState,
        config: SpringConfig,
    },
    Timed {
        from: f32,
        to: f32,
        elapsed_ms: f32,
        duration_ms: f32,
        easing: Easing,
    },
}

impl PropAnim {
    fn new(from_val: f32, to_val: f32, animation: &Animation) -> Self {
        match animation {
            Animation::Spring(config) => {
                let mut state = SpringState::new(from_val);
                state.set_target(to_val);
                PropAnim::Spring {
                    state,
                    config: config.clone(),
                }
            }
            Animation::Timed {
                duration_ms,
                easing,
            } => PropAnim::Timed {
                from: from_val,
                to: to_val,
                elapsed_ms: 0.0,
                duration_ms: *duration_ms as f32,
                easing: *easing,
            },
        }
    }

    fn step(&mut self, dt: f32) {
        match self {
            PropAnim::Spring { state, config } => state.step(dt, config),
            PropAnim::Timed { elapsed_ms, .. } => *elapsed_ms += dt * 1000.0,
        }
    }

    fn value(&self) -> f32 {
        match self {
            PropAnim::Spring { state, .. } => state.value,
            PropAnim::Timed {
                from,
                to,
                elapsed_ms,
                duration_ms,
                easing,
            } => {
                let t = (elapsed_ms / duration_ms).clamp(0.0, 1.0);
                let eased = easing.apply(t);
                from + (to - from) * eased
            }
        }
    }

    fn is_settled(&self) -> bool {
        match self {
            PropAnim::Spring { state, .. } => state.is_settled(),
            PropAnim::Timed {
                elapsed_ms,
                duration_ms,
                ..
            } => *elapsed_ms >= *duration_ms,
        }
    }

    #[allow(dead_code)]
    fn settle(&mut self) {
        match self {
            PropAnim::Spring { state, .. } => state.settle(),
            PropAnim::Timed {
                elapsed_ms,
                duration_ms,
                ..
            } => {
                *elapsed_ms = *duration_ms;
            }
        }
    }

    fn retarget(&mut self, new_target: f32) {
        match self {
            PropAnim::Spring { state, .. } => state.set_target(new_target),
            PropAnim::Timed {
                from,
                to,
                elapsed_ms,
                duration_ms,
                easing,
            } => {
                // Compute the current value before resetting
                let t = (*elapsed_ms / *duration_ms).clamp(0.0, 1.0);
                let eased = easing.apply(t);
                let current = *from + (*to - *from) * eased;
                // Start from current value, animate to new target
                *from = current;
                *to = new_target;
                *elapsed_ms = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-element animation state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct ElementAnim {
    opacity: Option<PropAnim>,
    offset_x: Option<PropAnim>,
    offset_y: Option<PropAnim>,
    scale: Option<PropAnim>,
    // Layout position/size animation
    layout_x: Option<PropAnim>,
    layout_y: Option<PropAnim>,
    layout_w: Option<PropAnim>,
    layout_h: Option<PropAnim>,
    /// True when this element has been removed from the tree and is playing
    /// its exit animation.
    exiting: bool,
}

impl ElementAnim {
    fn is_settled(&self) -> bool {
        self.opacity.as_ref().map_or(true, |a| a.is_settled())
            && self.offset_x.as_ref().map_or(true, |a| a.is_settled())
            && self.offset_y.as_ref().map_or(true, |a| a.is_settled())
            && self.scale.as_ref().map_or(true, |a| a.is_settled())
            && self.layout_x.as_ref().map_or(true, |a| a.is_settled())
            && self.layout_y.as_ref().map_or(true, |a| a.is_settled())
            && self.layout_w.as_ref().map_or(true, |a| a.is_settled())
            && self.layout_h.as_ref().map_or(true, |a| a.is_settled())
    }

    fn step(&mut self, dt: f32) {
        if let Some(a) = &mut self.opacity {
            a.step(dt);
        }
        if let Some(a) = &mut self.offset_x {
            a.step(dt);
        }
        if let Some(a) = &mut self.offset_y {
            a.step(dt);
        }
        if let Some(a) = &mut self.scale {
            a.step(dt);
        }
        if let Some(a) = &mut self.layout_x {
            a.step(dt);
        }
        if let Some(a) = &mut self.layout_y {
            a.step(dt);
        }
        if let Some(a) = &mut self.layout_w {
            a.step(dt);
        }
        if let Some(a) = &mut self.layout_h {
            a.step(dt);
        }
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Info about a keyed element needed for animation decisions.
pub struct ElementInfo {
    pub initial: Option<From>,
    pub exit: Option<To>,
    pub animate_layout: bool,
    pub layout_animation: Option<Animation>,
    pub target_opacity: f32,
    pub has_animation: bool,
    pub animation: Option<Animation>,
}

/// Animated overrides to apply when rendering a keyed element.
#[derive(Debug, Clone, Default)]
pub struct AnimOverrides {
    pub opacity: Option<f32>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub scale: Option<f32>,
    pub layout_x: Option<f32>,
    pub layout_y: Option<f32>,
    pub layout_w: Option<f32>,
    pub layout_h: Option<f32>,
}

// ---------------------------------------------------------------------------
// Animator
// ---------------------------------------------------------------------------

/// Manages all active animations across the element tree.
pub struct Animator {
    elements: HashMap<String, ElementAnim>,
    /// Previous frame's layout bounds per key (for detecting layout changes).
    prev_bounds: HashMap<String, Rect>,
}

impl Animator {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            prev_bounds: HashMap::new(),
        }
    }

    /// Step all animations by `dt` seconds. Returns `true` if any are still in flight.
    pub fn step(&mut self, dt: f32) -> bool {
        // Remove fully settled exit animations
        self.elements
            .retain(|_, anim| !(anim.exiting && anim.is_settled()));

        let mut any_active = false;
        for anim in self.elements.values_mut() {
            if !anim.is_settled() {
                anim.step(dt);
                any_active = true;
            }
        }

        any_active
    }

    /// Called after a new render to detect element enters, exits, and layout changes.
    pub fn diff_and_update(
        &mut self,
        current_keys: &HashMap<String, ElementInfo>,
        new_bounds: &HashMap<String, Rect>,
    ) {
        // Detect enters and layout changes
        for (key, info) in current_keys {
            let new_b = new_bounds.get(key);
            let old_b = self.prev_bounds.get(key);

            let anim = self.elements.entry(key.clone()).or_default();

            // Enter animation: element was not present in the previous frame
            if old_b.is_none() {
                if let Some(initial) = &info.initial {
                    let animation = initial
                        .animation
                        .as_ref()
                        .cloned()
                        .unwrap_or_default();

                    if let Some(v) = initial.opacity {
                        anim.opacity =
                            Some(PropAnim::new(v, info.target_opacity, &animation));
                    }
                    if let Some(v) = initial.offset_x {
                        anim.offset_x = Some(PropAnim::new(v, 0.0, &animation));
                    }
                    if let Some(v) = initial.offset_y {
                        anim.offset_y = Some(PropAnim::new(v, 0.0, &animation));
                    }
                    if let Some(v) = initial.scale {
                        anim.scale = Some(PropAnim::new(v, 1.0, &animation));
                    }
                }
            }

            // Layout animation: element moved or resized
            if info.animate_layout {
                if let (Some(old), Some(new)) = (old_b, new_b) {
                    let animation = info
                        .layout_animation
                        .as_ref()
                        .cloned()
                        .unwrap_or_default();

                    if (old.x - new.x).abs() > 0.5 {
                        if let Some(a) = &mut anim.layout_x {
                            a.retarget(new.x);
                        } else {
                            anim.layout_x = Some(PropAnim::new(old.x, new.x, &animation));
                        }
                    }
                    if (old.y - new.y).abs() > 0.5 {
                        if let Some(a) = &mut anim.layout_y {
                            a.retarget(new.y);
                        } else {
                            anim.layout_y = Some(PropAnim::new(old.y, new.y, &animation));
                        }
                    }
                    if (old.width - new.width).abs() > 0.5 {
                        if let Some(a) = &mut anim.layout_w {
                            a.retarget(new.width);
                        } else {
                            anim.layout_w =
                                Some(PropAnim::new(old.width, new.width, &animation));
                        }
                    }
                    if (old.height - new.height).abs() > 0.5 {
                        if let Some(a) = &mut anim.layout_h {
                            a.retarget(new.height);
                        } else {
                            anim.layout_h =
                                Some(PropAnim::new(old.height, new.height, &animation));
                        }
                    }
                }
            }
        }

        // Update prev_bounds for next frame's diff
        self.prev_bounds = new_bounds.clone();
    }

    /// Start an exit animation for a keyed element that has been removed.
    pub fn start_exit(&mut self, key: &str, exit: &To, _current_bounds: &Rect) {
        let animation = exit.animation.as_ref().cloned().unwrap_or_default();
        let anim = self.elements.entry(key.to_string()).or_default();
        anim.exiting = true;

        if let Some(target_opacity) = exit.opacity {
            let current = anim.opacity.as_ref().map(|a| a.value()).unwrap_or(1.0);
            anim.opacity = Some(PropAnim::new(current, target_opacity, &animation));
        }
        if let Some(v) = exit.offset_x {
            let current = anim.offset_x.as_ref().map(|a| a.value()).unwrap_or(0.0);
            anim.offset_x = Some(PropAnim::new(current, v, &animation));
        }
        if let Some(v) = exit.offset_y {
            let current = anim.offset_y.as_ref().map(|a| a.value()).unwrap_or(0.0);
            anim.offset_y = Some(PropAnim::new(current, v, &animation));
        }
        if let Some(v) = exit.scale {
            let current = anim.scale.as_ref().map(|a| a.value()).unwrap_or(1.0);
            anim.scale = Some(PropAnim::new(current, v, &animation));
        }
    }

    /// Get animated overrides for a keyed element.
    pub fn get_overrides(&self, key: &str) -> AnimOverrides {
        let anim = match self.elements.get(key) {
            Some(a) => a,
            None => return AnimOverrides::default(),
        };

        AnimOverrides {
            opacity: anim.opacity.as_ref().map(|a| a.value()),
            offset_x: anim
                .offset_x
                .as_ref()
                .map(|a| a.value())
                .unwrap_or(0.0),
            offset_y: anim
                .offset_y
                .as_ref()
                .map(|a| a.value())
                .unwrap_or(0.0),
            scale: anim.scale.as_ref().map(|a| a.value()),
            layout_x: anim.layout_x.as_ref().map(|a| a.value()),
            layout_y: anim.layout_y.as_ref().map(|a| a.value()),
            layout_w: anim.layout_w.as_ref().map(|a| a.value()),
            layout_h: anim.layout_h.as_ref().map(|a| a.value()),
        }
    }

    /// Check if any animations are currently active.
    pub fn is_animating(&self) -> bool {
        self.elements.values().any(|a| !a.is_settled())
    }

    /// Get all exiting element keys and their current overrides.
    pub fn exiting_elements(&self) -> Vec<(String, AnimOverrides)> {
        self.elements
            .iter()
            .filter(|(_, a)| a.exiting)
            .map(|(k, _)| (k.clone(), self.get_overrides(k)))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helper: collect keyed element info from the element + layout trees
// ---------------------------------------------------------------------------

/// Walk the element and layout trees, collecting animation-relevant info for
/// every keyed element.
pub fn collect_keyed_elements(
    element: &Element,
    layout: &LayoutNode,
) -> (HashMap<String, ElementInfo>, HashMap<String, Rect>) {
    let mut infos = HashMap::new();
    let mut bounds = HashMap::new();
    collect_recursive(element, layout, &mut infos, &mut bounds);
    (infos, bounds)
}

fn collect_recursive(
    element: &Element,
    layout: &LayoutNode,
    infos: &mut HashMap<String, ElementInfo>,
    bounds: &mut HashMap<String, Rect>,
) {
    if let Some(ref key) = element.key {
        infos.insert(
            key.clone(),
            ElementInfo {
                initial: element.initial.clone(),
                exit: element.exit.clone(),
                animate_layout: element.animate_layout,
                layout_animation: element.layout_animation.clone(),
                target_opacity: element.opacity,
                has_animation: element.animate.is_some(),
                animation: element.animate.clone(),
            },
        );
        bounds.insert(key.clone(), layout.bounds.clone());
    }

    for (child_elem, child_layout) in element.children.iter().zip(layout.children.iter()) {
        collect_recursive(child_elem, child_layout, infos, bounds);
    }
}
