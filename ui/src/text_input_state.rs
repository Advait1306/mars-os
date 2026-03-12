/// Internal state for a text input element
#[derive(Debug, Clone)]
pub struct TextInputState {
    pub cursor_position: usize,
    pub selection: Option<(usize, usize)>,
    pub cursor_visible: bool,
    pub blink_timer_ms: f32,
    pub scroll_offset: f32,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            cursor_position: 0,
            selection: None,
            cursor_visible: true,
            blink_timer_ms: 0.0,
            scroll_offset: 0.0,
        }
    }

    pub fn step(&mut self, dt: f32) -> bool {
        self.blink_timer_ms += dt * 1000.0;
        if self.blink_timer_ms >= 530.0 {
            self.blink_timer_ms = 0.0;
            self.cursor_visible = !self.cursor_visible;
            return true; // needs redraw
        }
        false
    }

    pub fn reset_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer_ms = 0.0;
    }

    /// Insert text at cursor, return new value
    pub fn insert(&mut self, current: &str, text: &str) -> String {
        let pos = self.cursor_position.min(current.chars().count());
        let byte_pos = char_to_byte_pos(current, pos);
        let mut result = String::with_capacity(current.len() + text.len());
        result.push_str(&current[..byte_pos]);
        result.push_str(text);
        result.push_str(&current[byte_pos..]);
        self.cursor_position = pos + text.chars().count();
        self.reset_blink();
        result
    }

    /// Delete character before cursor, return new value
    pub fn backspace(&mut self, current: &str) -> String {
        if self.cursor_position == 0 || current.is_empty() {
            return current.to_string();
        }
        // Find the byte boundary before cursor
        let byte_pos = char_to_byte_pos(current, self.cursor_position);
        let prev_boundary = prev_char_boundary(current, byte_pos);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..prev_boundary]);
        result.push_str(&current[byte_pos..]);
        self.cursor_position = byte_to_char_pos(current, prev_boundary);
        self.reset_blink();
        result
    }

    /// Delete character after cursor, return new value
    pub fn delete(&mut self, current: &str) -> String {
        let byte_pos = char_to_byte_pos(current, self.cursor_position);
        if byte_pos >= current.len() {
            return current.to_string();
        }
        let next_boundary = next_char_boundary(current, byte_pos);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..byte_pos]);
        result.push_str(&current[next_boundary..]);
        self.reset_blink();
        result
    }

    pub fn move_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
        self.reset_blink();
    }

    pub fn move_right(&mut self, text_len: usize) {
        if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
        self.reset_blink();
    }

    pub fn move_to_start(&mut self) {
        self.cursor_position = 0;
        self.reset_blink();
    }

    pub fn move_to_end(&mut self, text_len: usize) {
        self.cursor_position = text_len;
        self.reset_blink();
    }
}

fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn byte_to_char_pos(s: &str, byte_pos: usize) -> usize {
    s[..byte_pos].chars().count()
}

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 { return 0; }
    let mut i = pos - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() { return s.len(); }
    let mut i = pos + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}
