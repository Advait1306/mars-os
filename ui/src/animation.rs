//! Animation configuration types: spring presets, timed animations with easing,
//! and enter/exit animation descriptors.

use crate::spring::SpringConfig;

/// An animation can be either a physics-based spring or a timed curve.
#[derive(Debug, Clone)]
pub enum Animation {
    Spring(SpringConfig),
    Timed { duration_ms: u32, easing: Easing },
}

impl Animation {
    /// Default critically-damped spring (same as dock).
    pub fn default_spring() -> Self {
        Animation::Spring(SpringConfig {
            stiffness: 680.0,
            damping: 52.0,
        })
    }

    /// Fast, responsive spring with minimal overshoot.
    pub fn snappy() -> Self {
        Animation::Spring(SpringConfig {
            stiffness: 1200.0,
            damping: 70.0,
        })
    }

    /// Slow, gentle spring for large transitions.
    pub fn smooth() -> Self {
        Animation::Spring(SpringConfig {
            stiffness: 300.0,
            damping: 35.0,
        })
    }

    /// Lively spring with visible bounce.
    pub fn bouncy() -> Self {
        Animation::Spring(SpringConfig {
            stiffness: 600.0,
            damping: 25.0,
        })
    }

    /// Linear timed animation over the given duration.
    pub fn linear(duration_ms: u32) -> Self {
        Animation::Timed {
            duration_ms,
            easing: Easing::Linear,
        }
    }

    /// Ease-out timed animation (decelerating).
    pub fn ease_out(duration_ms: u32) -> Self {
        Animation::Timed {
            duration_ms,
            easing: Easing::EaseOut,
        }
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::default_spring()
    }
}

/// Easing functions for timed animations.
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Map a linear progress `t` in [0, 1] to an eased value.
    pub fn apply(&self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::CubicBezier(_, _, _, _) => t, // simplified placeholder
        }
    }
}

/// Describes the initial state for an enter animation.
///
/// When a keyed element first appears, its animated properties start at these
/// values and transition toward their normal state.
#[derive(Debug, Clone, Default)]
pub struct From {
    pub opacity: Option<f32>,
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub scale: Option<f32>,
    pub animation: Option<Animation>,
}

impl From {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn opacity(mut self, v: f32) -> Self {
        self.opacity = Some(v);
        self
    }
    pub fn offset_x(mut self, v: f32) -> Self {
        self.offset_x = Some(v);
        self
    }
    pub fn offset_y(mut self, v: f32) -> Self {
        self.offset_y = Some(v);
        self
    }
    pub fn scale(mut self, v: f32) -> Self {
        self.scale = Some(v);
        self
    }
    pub fn animation(mut self, a: Animation) -> Self {
        self.animation = Some(a);
        self
    }
}

/// Describes the final state for an exit animation.
///
/// When a keyed element is removed from the tree, its animated properties
/// transition toward these values before being cleaned up.
#[derive(Debug, Clone, Default)]
pub struct To {
    pub opacity: Option<f32>,
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub scale: Option<f32>,
    pub animation: Option<Animation>,
}

impl To {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn opacity(mut self, v: f32) -> Self {
        self.opacity = Some(v);
        self
    }
    pub fn offset_x(mut self, v: f32) -> Self {
        self.offset_x = Some(v);
        self
    }
    pub fn offset_y(mut self, v: f32) -> Self {
        self.offset_y = Some(v);
        self
    }
    pub fn scale(mut self, v: f32) -> Self {
        self.scale = Some(v);
        self
    }
    pub fn animation(mut self, a: Animation) -> Self {
        self.animation = Some(a);
        self
    }
}
