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
        // stiffness=680 → critical damping ≈ 52.2
        // 4x stiffer than original (170) for ~2x faster settle time
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
            stiffness: 680.0,
            damping: 52.0,
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

    /// Returns true when the spring is close enough to trigger the next phase.
    /// Looser than is_settled() to avoid visible pauses between animation phases.
    pub fn is_near_target(&self, threshold: f32) -> bool {
        (self.value - self.target).abs() < threshold
    }

    /// Snap the spring to its target with zero velocity.
    pub fn settle(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }
}
