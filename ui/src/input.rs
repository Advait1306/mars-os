//! Input event types for the UI framework.
//!
//! Defines pointer, keyboard, and text input events, plus cursor styles
//! that elements can request.

/// An input event from the windowing system, normalized for the UI framework.
#[derive(Debug, Clone)]
pub enum InputEvent {
    PointerMove { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, button: MouseButton, pressed: bool },
    PointerScroll { x: f32, y: f32, delta_x: f32, delta_y: f32 },
    PointerLeave,
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    TextInput { text: String },
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}

/// Cursor style that an element can request when hovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    NotAllowed,
}

/// Represents a keyboard key. Wraps a u32 keysym for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key(pub u32);
