//! Spring physics for smooth animations.
//!
//! Ported from dock/src/animation.rs and enhanced with configurable parameters.
//! Simulates: F = -stiffness * (x - target) - damping * velocity
//! Uses symplectic Euler integration for stability.

/// Configuration for spring physics behavior.
#[derive(Debug, Clone)]
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
}

impl Default for SpringConfig {
    fn default() -> Self {
        // Critical damping: damping = 2 * sqrt(stiffness)
        // stiffness=680 -> critical damping ~ 52.2
        Self {
            stiffness: 680.0,
            damping: 52.0,
        }
    }
}

/// Per-value spring simulation state.
#[derive(Debug, Clone)]
pub struct SpringState {
    pub value: f32,
    pub target: f32,
    pub velocity: f32,
}

impl SpringState {
    /// Create a spring at rest at the given value.
    pub fn new(initial: f32) -> Self {
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
        }
    }

    /// Change the target value (the spring will animate toward it).
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Advance the spring simulation by `dt` seconds.
    pub fn step(&mut self, dt: f32, config: &SpringConfig) {
        let force =
            -config.stiffness * (self.value - self.target) - config.damping * self.velocity;
        self.velocity += force * dt;
        self.value += self.velocity * dt;
    }

    /// Returns true when the spring has effectively reached its target.
    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < 0.5 && self.velocity.abs() < 0.5
    }

    /// Snap the spring to its target with zero velocity.
    pub fn settle(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }
}
