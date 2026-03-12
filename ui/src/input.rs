//! Input event types for the UI framework.
//!
//! Defines pointer, keyboard, focus, and text input events, plus cursor styles
//! that elements can request. Includes event propagation control types.

// ---------------------------------------------------------------------------
// Event propagation control
// ---------------------------------------------------------------------------

/// Result returned by event handlers to control propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// Event was not handled; continue propagation.
    Continue,
    /// Event was handled; stop propagation (equivalent to stopPropagation).
    Stop,
    /// Event was handled AND default action should be prevented.
    StopAndPreventDefault,
    /// Continue propagation but prevent the default action.
    PreventDefault,
}

/// Phase of event dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

// ---------------------------------------------------------------------------
// Raw input events (from Wayland, translated in wayland.rs)
// ---------------------------------------------------------------------------

/// An input event from the windowing system, normalized for the UI framework.
#[derive(Debug, Clone)]
pub enum InputEvent {
    PointerMove { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, button: MouseButton, pressed: bool, time: u32 },
    PointerScroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        /// Scroll source (wheel, finger, continuous, wheel tilt).
        source: Option<ScrollSource>,
        /// Discrete scroll steps (non-zero for wheel clicks).
        discrete_x: i32,
        discrete_y: i32,
        /// True when the scroll source signals end of continuous scrolling.
        stop: bool,
        /// Timestamp in milliseconds.
        time: u32,
    },
    PointerLeave,
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    TextInput { text: String },

    /// IME composition started (preedit received while not composing).
    CompositionStart,
    /// IME preedit text updated.
    CompositionUpdate {
        text: String,
        cursor_begin: Option<usize>,
        cursor_end: Option<usize>,
    },
    /// IME composition committed — `text` is the final string.
    CompositionEnd { text: String },

    // Touch events (from wl_touch)
    /// New touch point contacts surface.
    TouchDown { id: i32, x: f32, y: f32, time: u32 },
    /// Touch point moved.
    TouchMotion { id: i32, x: f32, y: f32, time: u32 },
    /// Touch point lifted.
    TouchUp { id: i32, time: u32 },
    /// System cancelled all active touches.
    TouchCancel,
}

// ---------------------------------------------------------------------------
// Mouse / Pointer types
// ---------------------------------------------------------------------------

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    None,
    Left,      // button 272 / BTN_LEFT
    Right,     // button 273 / BTN_RIGHT
    Middle,    // button 274 / BTN_MIDDLE
    Back,      // button 275 / BTN_SIDE
    Forward,   // button 276 / BTN_EXTRA
}

/// Pointer type: mouse, pen, or touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
}

/// Rich pointer event dispatched through the element tree.
#[derive(Debug, Clone)]
pub struct PointerEvent {
    /// Unique pointer ID (for multi-touch support).
    pub pointer_id: u32,
    /// Pointer type: mouse, pen, touch.
    pub pointer_type: PointerType,
    /// Position in surface-local coordinates.
    pub x: f32,
    pub y: f32,
    /// Which button changed state (for PointerDown/PointerUp).
    pub button: MouseButton,
    /// Bitmask of currently pressed buttons.
    pub buttons: u32,
    /// Timestamp in milliseconds from Wayland.
    pub time: u32,
    /// Whether this is the primary pointer of its type.
    pub is_primary: bool,
    /// Modifier keys held during this event.
    pub modifiers: Modifiers,
}

impl PointerEvent {
    pub fn from_move(x: f32, y: f32, buttons: u32, modifiers: Modifiers) -> Self {
        Self {
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            x,
            y,
            button: MouseButton::None,
            buttons,
            time: 0,
            is_primary: true,
            modifiers,
        }
    }

    pub fn from_button(
        x: f32,
        y: f32,
        button: MouseButton,
        buttons: u32,
        time: u32,
        modifiers: Modifiers,
    ) -> Self {
        Self {
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            x,
            y,
            button,
            buttons,
            time,
            is_primary: true,
            modifiers,
        }
    }
}

// ---------------------------------------------------------------------------
// Click event (synthesized)
// ---------------------------------------------------------------------------

/// Synthesized click event.
#[derive(Debug, Clone)]
pub struct ClickEvent {
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// Which button was clicked.
    pub button: MouseButton,
    /// Click count (1 = single, 2 = double, 3 = triple).
    pub count: u32,
    /// Modifier keys.
    pub modifiers: Modifiers,
    /// Pointer type that generated this click.
    pub pointer_type: PointerType,
}

// ---------------------------------------------------------------------------
// Focus event
// ---------------------------------------------------------------------------

/// Focus change event.
#[derive(Debug, Clone)]
pub struct FocusEvent {
    /// The element that is the "other side" of the focus change.
    /// For Focus/FocusIn: the element that lost focus (if any).
    /// For Blur/FocusOut: the element that gained focus (if any).
    pub related_target: Option<usize>,
}

// ---------------------------------------------------------------------------
// Keyboard types
// ---------------------------------------------------------------------------

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    /// Returns true if the platform "command" modifier is held.
    /// On Linux/Wayland this is Ctrl.
    pub fn command(&self) -> bool {
        self.ctrl
    }
}

/// Represents a keyboard key. Wraps a u32 keysym for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key(pub u32);

/// Logical key value (layout-dependent).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyValue {
    /// Printable character.
    Character(String),
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Insert,
    /// Function keys.
    F(u8),
    /// Modifier keys.
    Control,
    Shift,
    Alt,
    Super,
    CapsLock,
    NumLock,
    /// Media keys.
    MediaPlay,
    MediaPause,
    MediaStop,
    MediaNext,
    MediaPrev,
    AudioVolumeUp,
    AudioVolumeDown,
    AudioVolumeMute,
    /// Unknown/unrecognized key with raw keysym.
    Unknown(u32),
}

/// Physical key code (layout-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u32);

/// Rich keyboard event dispatched through the element tree.
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    /// Logical key value based on current keyboard layout.
    pub key: KeyValue,
    /// Physical key code, layout-independent.
    pub code: KeyCode,
    /// Whether this event is from key auto-repeat.
    pub repeat: bool,
    /// Modifier key state.
    pub modifiers: Modifiers,
    /// Whether this event fires during IME composition.
    pub is_composing: bool,
    /// Timestamp in milliseconds.
    pub time: u32,
}

// ---------------------------------------------------------------------------
// Text input events (BeforeInput / Input)
// ---------------------------------------------------------------------------

/// The type of input operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    InsertText,
    InsertLineBreak,
    InsertFromPaste,
    InsertFromDrop,
    /// Backspace
    DeleteContentBackward,
    /// Delete key
    DeleteContentForward,
    /// Ctrl+Backspace
    DeleteWordBackward,
    /// Ctrl+Delete
    DeleteWordForward,
    /// Cmd+Backspace (on Linux, mapped to Ctrl+Shift+Backspace or similar)
    DeleteSoftLineBackward,
    /// Cmd+Delete
    DeleteSoftLineForward,
    DeleteByCut,
    HistoryUndo,
    HistoryRedo,
    FormatBold,
    FormatItalic,
    FormatUnderline,
}

/// Fired before text content is modified. Cancelable.
#[derive(Debug, Clone)]
pub struct BeforeInputEvent {
    /// The text being inserted (empty/None for deletions).
    pub data: Option<String>,
    /// The type of input operation.
    pub input_type: InputType,
    /// Whether this event fires during IME composition.
    pub is_composing: bool,
}

/// Fired after text content is modified. Not cancelable.
#[derive(Debug, Clone)]
pub struct TextInputEvent {
    /// The text that was inserted (empty/None for deletions).
    pub data: Option<String>,
    /// The type of input operation.
    pub input_type: InputType,
    /// Whether this event fires during IME composition.
    pub is_composing: bool,
}

// ---------------------------------------------------------------------------
// Composition events (IME)
// ---------------------------------------------------------------------------

/// Fired during IME composition lifecycle.
#[derive(Debug, Clone)]
pub struct CompositionEvent {
    /// The current composition text.
    /// Empty for CompositionStart, committed text for CompositionEnd.
    pub data: String,
    /// Cursor position within the preedit string (byte offset).
    pub cursor_begin: Option<usize>,
    pub cursor_end: Option<usize>,
}

// ---------------------------------------------------------------------------
// Drag and Drop
// ---------------------------------------------------------------------------

/// Allowed/current drop effect for drag-and-drop operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropEffect {
    /// No drop allowed / drop rejected.
    None,
    /// Copy the dragged data.
    Copy,
    /// Move the dragged data.
    Move,
    /// Create a link/reference to the dragged data.
    Link,
}

/// Data payload carried during a drag-and-drop operation.
#[derive(Debug, Clone, Default)]
pub struct DragData {
    /// MIME type -> data pairs.
    entries: Vec<(String, Vec<u8>)>,
}

impl DragData {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Set data for a MIME type.
    pub fn set(&mut self, mime_type: &str, data: impl Into<Vec<u8>>) {
        // Replace existing entry or add new one
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| k == mime_type) {
            entry.1 = data.into();
        } else {
            self.entries.push((mime_type.to_string(), data.into()));
        }
    }

    /// Get data for a MIME type.
    pub fn get(&self, mime_type: &str) -> Option<&[u8]> {
        self.entries.iter().find(|(k, _)| k == mime_type).map(|(_, v)| v.as_slice())
    }

    /// Get all available MIME types.
    pub fn types(&self) -> Vec<&str> {
        self.entries.iter().map(|(k, _)| k.as_str()).collect()
    }

    /// Convenience: set plain text data.
    pub fn set_text(&mut self, text: &str) {
        self.set("text/plain", text.as_bytes().to_vec());
    }

    /// Convenience: get plain text data.
    pub fn get_text(&self) -> Option<&str> {
        self.get("text/plain").and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Returns true if no data has been set.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Drag-and-drop event dispatched through the element tree.
#[derive(Debug, Clone)]
pub struct DragEvent {
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// The data being dragged.
    pub data: DragData,
    /// Allowed drop effects (set by drag source in DragStart).
    pub effect_allowed: DropEffect,
    /// Current drop effect (set by drop target during DragOver).
    pub drop_effect: DropEffect,
}

// ---------------------------------------------------------------------------
// Clipboard events
// ---------------------------------------------------------------------------

/// Data payload for clipboard operations (copy, cut, paste).
#[derive(Debug, Clone, Default)]
pub struct ClipboardData {
    /// MIME type -> data pairs.
    entries: std::collections::HashMap<String, Vec<u8>>,
}

impl ClipboardData {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Set data for a MIME type.
    pub fn set(&mut self, mime_type: &str, data: Vec<u8>) {
        self.entries.insert(mime_type.to_string(), data);
    }

    /// Get data for a MIME type.
    pub fn get(&self, mime_type: &str) -> Option<&[u8]> {
        self.entries.get(mime_type).map(|v| v.as_slice())
    }

    /// Get all available MIME types.
    pub fn types(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Convenience: set plain text data.
    pub fn set_text(&mut self, text: &str) {
        self.set("text/plain", text.as_bytes().to_vec());
    }

    /// Convenience: get plain text data.
    pub fn get_text(&self) -> Option<&str> {
        self.get("text/plain").and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Returns true if no data has been set.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Clipboard event for copy, cut, and paste operations.
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    /// The clipboard data.
    /// For Copy/Cut: handler populates this with content to copy.
    /// For Paste: contains the clipboard contents to paste.
    pub clipboard_data: ClipboardData,
}

// ---------------------------------------------------------------------------
// Touch events
// ---------------------------------------------------------------------------

/// Native touch event for multi-touch support.
/// The primary touch (first finger) is also coerced into pointer events,
/// so most elements work with touch without special handling.
#[derive(Debug, Clone)]
pub struct TouchEvent {
    /// Unique ID for this touch point (from Wayland touch.down).
    pub touch_id: i32,
    /// Position in surface coordinates.
    pub x: f32,
    pub y: f32,
    /// Contact area major axis (if available from Wayland shape event).
    pub width: Option<f32>,
    /// Contact area minor axis (if available from Wayland shape event).
    pub height: Option<f32>,
    /// Contact angle in degrees (if available from Wayland orientation event).
    pub orientation: Option<f32>,
    /// Timestamp in milliseconds.
    pub time: u32,
}

// ---------------------------------------------------------------------------
// Scroll event
// ---------------------------------------------------------------------------

/// Scroll source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSource {
    /// Mouse wheel with discrete steps.
    Wheel,
    /// Touchpad / trackpad with continuous motion.
    Finger,
    /// Continuous scroll from a knob or dial.
    Continuous,
    /// Tilt of a scroll wheel.
    WheelTilt,
}

/// Rich wheel/scroll event.
#[derive(Debug, Clone)]
pub struct WheelEvent {
    /// Pointer position.
    pub x: f32,
    pub y: f32,
    /// Scroll deltas in pixels.
    pub delta_x: f32,
    pub delta_y: f32,
    /// Source of the scroll event.
    pub source: ScrollSource,
    /// Whether this is from a discrete wheel (has notched steps).
    pub is_discrete: bool,
    /// Modifier keys.
    pub modifiers: Modifiers,
    /// Timestamp.
    pub time: u32,
}

/// Scroll-end event, fired when continuous scrolling stops (e.g., finger lifted from touchpad).
#[derive(Debug, Clone)]
pub struct ScrollEndEvent {
    /// Pointer position.
    pub x: f32,
    pub y: f32,
    /// Modifier keys.
    pub modifiers: Modifiers,
}

// ---------------------------------------------------------------------------
// Button bitmask helpers
// ---------------------------------------------------------------------------

pub const BUTTON_LEFT: u32 = 0x01;
pub const BUTTON_RIGHT: u32 = 0x02;
pub const BUTTON_MIDDLE: u32 = 0x04;

impl MouseButton {
    /// Convert a button to its bitmask.
    pub fn to_bitmask(self) -> u32 {
        match self {
            MouseButton::Left => BUTTON_LEFT,
            MouseButton::Right => BUTTON_RIGHT,
            MouseButton::Middle => BUTTON_MIDDLE,
            MouseButton::Back => 0x08,
            MouseButton::Forward => 0x10,
            MouseButton::None => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Cursor style
// ---------------------------------------------------------------------------

/// Controls whether an element participates in hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    /// Normal: this element and children participate in hit testing.
    Auto,
    /// Skip this element and children in hit testing.
    None,
    /// Skip this element but still test children.
    PassThrough,
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
