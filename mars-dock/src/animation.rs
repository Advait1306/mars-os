//! Spring physics for smooth dock animations.

/// A critically-damped spring for smooth, non-bouncy motion.
///
/// Simulates: F = -stiffness * (x - target) - damping * velocity
/// Uses symplectic Euler integration for stability.
pub struct Spring {
    pub value: f32,
    pub target: f32,
    velocity: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    /// Create a spring at rest at the given value.
    pub fn new(initial: f32) -> Self {
        // Critical damping: damping = 2 * sqrt(stiffness)
        // stiffness=170 → critical damping ≈ 26.1
        // Using 26.0 for near-critically-damped (barely perceptible overshoot)
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
            stiffness: 170.0,
            damping: 26.0,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Advance the spring simulation by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        let force = -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
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
