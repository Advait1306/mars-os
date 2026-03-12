use crate::spring::{SpringConfig, SpringState};

/// Scroll phase
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollPhase {
    Idle,
    Tracking,    // finger down, 1:1 scroll
    Momentum,    // finger lifted, decelerating
    Snapping,    // rubber band snap back
}

/// Internal scroll state for a scroll container
#[derive(Debug, Clone)]
pub struct ScrollState {
    pub offset: f32,
    pub velocity: f32,
    pub phase: ScrollPhase,
    pub max_offset: f32,
    pub content_size: f32,
    pub viewport_size: f32,
    /// Spring for overscroll snap-back and wheel scrolling
    snap_spring: SpringState,
    snap_config: SpringConfig,
    /// Indicator fade (1.0 = visible, 0.0 = hidden)
    pub indicator_opacity: f32,
    /// Time since last scroll activity (ms)
    pub idle_time_ms: f32,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            velocity: 0.0,
            phase: ScrollPhase::Idle,
            max_offset: 0.0,
            content_size: 0.0,
            viewport_size: 0.0,
            snap_spring: SpringState::new(0.0),
            snap_config: SpringConfig { stiffness: 680.0, damping: 52.0 },
            indicator_opacity: 0.0,
            idle_time_ms: 0.0,
        }
    }

    pub fn update_sizes(&mut self, content_size: f32, viewport_size: f32) {
        self.content_size = content_size;
        self.viewport_size = viewport_size;
        self.max_offset = (content_size - viewport_size).max(0.0);
        // Clamp offset
        if self.offset > self.max_offset {
            self.offset = self.max_offset;
        }
    }

    /// Handle scroll delta from trackpad/wheel
    pub fn on_scroll(&mut self, delta: f32) {
        self.phase = ScrollPhase::Tracking;
        self.indicator_opacity = 1.0;
        self.idle_time_ms = 0.0;

        // Rubber band at edges
        if self.offset < 0.0 || self.offset > self.max_offset {
            self.offset -= delta * 0.3; // damped
        } else {
            self.offset -= delta;
            self.velocity = -delta / 0.016; // approximate velocity from delta
        }
    }

    /// Handle scroll end (finger lifted)
    pub fn on_scroll_end(&mut self) {
        if self.offset < 0.0 || self.offset > self.max_offset {
            self.phase = ScrollPhase::Snapping;
            let target = self.offset.clamp(0.0, self.max_offset);
            self.snap_spring = SpringState::new(self.offset);
            self.snap_spring.set_target(target);
        } else if self.velocity.abs() > 10.0 {
            self.phase = ScrollPhase::Momentum;
        } else {
            self.phase = ScrollPhase::Idle;
        }
    }

    /// Step physics for one frame
    pub fn step(&mut self, dt: f32) -> bool {
        match self.phase {
            ScrollPhase::Idle => {
                // Fade indicator
                if self.indicator_opacity > 0.0 {
                    self.idle_time_ms += dt * 1000.0;
                    if self.idle_time_ms > 800.0 {
                        self.indicator_opacity = (self.indicator_opacity - dt * 3.0).max(0.0);
                    }
                    return self.indicator_opacity > 0.0;
                }
                false
            }
            ScrollPhase::Tracking => false,
            ScrollPhase::Momentum => {
                self.velocity *= 0.97_f32.powf(dt * 60.0);
                self.offset += self.velocity * dt;

                // Hit edge -> snap
                if self.offset < 0.0 || self.offset > self.max_offset {
                    self.phase = ScrollPhase::Snapping;
                    let target = self.offset.clamp(0.0, self.max_offset);
                    self.snap_spring = SpringState::new(self.offset);
                    self.snap_spring.set_target(target);
                    return true;
                }

                if self.velocity.abs() < 1.0 {
                    self.velocity = 0.0;
                    self.phase = ScrollPhase::Idle;
                    return false;
                }
                true
            }
            ScrollPhase::Snapping => {
                self.snap_spring.step(dt, &self.snap_config);
                self.offset = self.snap_spring.value;
                if self.snap_spring.is_settled() {
                    self.snap_spring.settle();
                    self.offset = self.snap_spring.value;
                    self.phase = ScrollPhase::Idle;
                    return false;
                }
                true
            }
        }
    }

    /// Programmatic scroll to offset (animated)
    pub fn scroll_to(&mut self, target: f32) {
        let target = target.clamp(0.0, self.max_offset);
        self.phase = ScrollPhase::Snapping;
        self.snap_spring = SpringState::new(self.offset);
        self.snap_spring.set_target(target);
        self.indicator_opacity = 1.0;
        self.idle_time_ms = 0.0;
    }

    /// Programmatic scroll to offset (immediate)
    pub fn scroll_to_immediate(&mut self, target: f32) {
        self.offset = target.clamp(0.0, self.max_offset);
        self.phase = ScrollPhase::Idle;
        self.velocity = 0.0;
    }
}
