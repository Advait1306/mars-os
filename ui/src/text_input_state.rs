/// Undo/redo entry for text input
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub value: String,
    pub cursor_position: usize,
    pub selection_anchor: Option<usize>,
}

/// Internal state for a text input element
#[derive(Debug, Clone)]
pub struct TextInputState {
    // Cursor
    pub cursor_position: usize,
    pub cursor_visible: bool,
    pub blink_timer_ms: f32,
    pub scroll_offset: f32,

    // Selection
    pub selection_anchor: Option<usize>,

    // IME composition
    pub composing: bool,
    pub compose_text: String,
    pub compose_cursor: usize,

    // Undo/redo
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub undo_group_timer_ms: f32,

    // Interaction tracking
    pub click_count: u32,
    pub last_click_time_ms: f32,
    pub last_click_position: usize,
    pub mouse_selecting: bool,

    // Password reveal (for password inputs)
    pub password_reveal_char: Option<usize>,
    pub password_reveal_timer_ms: f32,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            cursor_position: 0,
            cursor_visible: true,
            blink_timer_ms: 0.0,
            scroll_offset: 0.0,
            selection_anchor: None,
            composing: false,
            compose_text: String::new(),
            compose_cursor: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_group_timer_ms: 0.0,
            click_count: 0,
            last_click_time_ms: 0.0,
            last_click_position: 0,
            mouse_selecting: false,
            password_reveal_char: None,
            password_reveal_timer_ms: 0.0,
        }
    }

    /// Backward-compatible: return selection as Option<(usize, usize)>
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor_position);
            let end = anchor.max(self.cursor_position);
            (start, end)
        })
    }

    /// Get the selected range (start, end) where start <= end
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            (anchor.min(self.cursor_position), anchor.max(self.cursor_position))
        })
    }

    /// Returns true if there is an active selection
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.map_or(false, |a| a != self.cursor_position)
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Select all text
    pub fn select_all(&mut self, text_len: usize) {
        self.selection_anchor = Some(0);
        self.cursor_position = text_len;
        self.reset_blink();
    }

    pub fn step(&mut self, dt: f32) -> bool {
        self.blink_timer_ms += dt * 1000.0;
        let mut needs_redraw = false;
        if self.blink_timer_ms >= 530.0 {
            self.blink_timer_ms = 0.0;
            self.cursor_visible = !self.cursor_visible;
            needs_redraw = true;
        }
        // Undo group timer
        if self.undo_group_timer_ms > 0.0 {
            self.undo_group_timer_ms -= dt * 1000.0;
            if self.undo_group_timer_ms <= 0.0 {
                self.undo_group_timer_ms = 0.0;
            }
        }
        // Password reveal timer
        if let Some(_) = self.password_reveal_char {
            self.password_reveal_timer_ms -= dt * 1000.0;
            if self.password_reveal_timer_ms <= 0.0 {
                self.password_reveal_char = None;
                self.password_reveal_timer_ms = 0.0;
                needs_redraw = true;
            }
        }
        needs_redraw
    }

    pub fn reset_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer_ms = 0.0;
    }

    /// Push current state to undo stack
    fn push_undo(&mut self, current: &str) {
        // If undo_group_timer is active and last entry has same cursor, group edits
        if self.undo_group_timer_ms > 0.0 {
            // Don't push a new undo entry; just update timer
        } else {
            self.undo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: self.cursor_position,
                selection_anchor: self.selection_anchor,
            });
            // Cap undo stack size
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
        }
        self.undo_group_timer_ms = 500.0;
        // Any new edit clears the redo stack
        self.redo_stack.clear();
    }

    /// Delete the currently selected text and return the new value
    pub fn delete_selection(&mut self, current: &str) -> String {
        if let Some((start, end)) = self.selection_range() {
            let byte_start = char_to_byte_pos(current, start);
            let byte_end = char_to_byte_pos(current, end);
            let mut result = String::with_capacity(current.len());
            result.push_str(&current[..byte_start]);
            result.push_str(&current[byte_end..]);
            self.cursor_position = start;
            self.selection_anchor = None;
            result
        } else {
            current.to_string()
        }
    }

    /// Get selected text
    pub fn selected_text<'a>(&self, current: &'a str) -> &'a str {
        if let Some((start, end)) = self.selection_range() {
            let byte_start = char_to_byte_pos(current, start);
            let byte_end = char_to_byte_pos(current, end);
            &current[byte_start..byte_end]
        } else {
            ""
        }
    }

    /// Insert text at cursor (deleting selection first if any), return new value
    pub fn insert(&mut self, current: &str, text: &str) -> String {
        self.push_undo(current);
        // Delete selection first if any
        let base = if self.has_selection() {
            self.delete_selection(current)
        } else {
            current.to_string()
        };
        let pos = self.cursor_position.min(base.chars().count());
        let byte_pos = char_to_byte_pos(&base, pos);
        let mut result = String::with_capacity(base.len() + text.len());
        result.push_str(&base[..byte_pos]);
        result.push_str(text);
        result.push_str(&base[byte_pos..]);
        self.cursor_position = pos + text.chars().count();
        self.selection_anchor = None;
        self.reset_blink();
        result
    }

    /// Delete character before cursor, return new value
    pub fn backspace(&mut self, current: &str) -> String {
        self.push_undo(current);
        // If selection, delete selection
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
        if self.cursor_position == 0 || current.is_empty() {
            return current.to_string();
        }
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
        self.push_undo(current);
        // If selection, delete selection
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
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

    /// Delete the word before cursor (Ctrl+Backspace), return new value
    pub fn delete_word_back(&mut self, current: &str) -> String {
        self.push_undo(current);
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
        let target = prev_word_boundary(current, self.cursor_position);
        let byte_start = char_to_byte_pos(current, target);
        let byte_end = char_to_byte_pos(current, self.cursor_position);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..byte_start]);
        result.push_str(&current[byte_end..]);
        self.cursor_position = target;
        self.reset_blink();
        result
    }

    /// Delete the word after cursor (Ctrl+Delete), return new value
    pub fn delete_word_forward(&mut self, current: &str) -> String {
        self.push_undo(current);
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
        let text_len = current.chars().count();
        let target = next_word_boundary(current, self.cursor_position, text_len);
        let byte_start = char_to_byte_pos(current, self.cursor_position);
        let byte_end = char_to_byte_pos(current, target);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..byte_start]);
        result.push_str(&current[byte_end..]);
        self.reset_blink();
        result
    }

    /// Undo last edit, return new value (or None if nothing to undo)
    pub fn undo(&mut self, current: &str) -> Option<String> {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: self.cursor_position,
                selection_anchor: self.selection_anchor,
            });
            self.cursor_position = entry.cursor_position;
            self.selection_anchor = entry.selection_anchor;
            self.reset_blink();
            Some(entry.value)
        } else {
            None
        }
    }

    /// Redo last undone edit, return new value (or None if nothing to redo)
    pub fn redo(&mut self, current: &str) -> Option<String> {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: self.cursor_position,
                selection_anchor: self.selection_anchor,
            });
            self.cursor_position = entry.cursor_position;
            self.selection_anchor = entry.selection_anchor;
            self.reset_blink();
            Some(entry.value)
        } else {
            None
        }
    }

    // === Cursor movement ===

    pub fn move_left(&mut self) {
        if self.has_selection() {
            // Collapse to left edge
            if let Some((start, _)) = self.selection_range() {
                self.cursor_position = start;
            }
            self.clear_selection();
        } else if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
        self.reset_blink();
    }

    pub fn move_right(&mut self, text_len: usize) {
        if self.has_selection() {
            // Collapse to right edge
            if let Some((_, end)) = self.selection_range() {
                self.cursor_position = end;
            }
            self.clear_selection();
        } else if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
        self.reset_blink();
    }

    pub fn move_word_left(&mut self, text: &str) {
        if self.has_selection() {
            if let Some((start, _)) = self.selection_range() {
                self.cursor_position = start;
            }
            self.clear_selection();
        }
        self.cursor_position = prev_word_boundary(text, self.cursor_position);
        self.reset_blink();
    }

    pub fn move_word_right(&mut self, text: &str) {
        let text_len = text.chars().count();
        if self.has_selection() {
            if let Some((_, end)) = self.selection_range() {
                self.cursor_position = end;
            }
            self.clear_selection();
        }
        self.cursor_position = next_word_boundary(text, self.cursor_position, text_len);
        self.reset_blink();
    }

    pub fn move_to_start(&mut self) {
        if self.has_selection() {
            self.clear_selection();
        }
        self.cursor_position = 0;
        self.reset_blink();
    }

    pub fn move_to_end(&mut self, text_len: usize) {
        if self.has_selection() {
            self.clear_selection();
        }
        self.cursor_position = text_len;
        self.reset_blink();
    }

    // === Selection movement ===

    pub fn select_left(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
        self.reset_blink();
    }

    pub fn select_right(&mut self, text_len: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
        self.reset_blink();
    }

    pub fn select_word_left(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        self.cursor_position = prev_word_boundary(text, self.cursor_position);
        self.reset_blink();
    }

    pub fn select_word_right(&mut self, text: &str) {
        let text_len = text.chars().count();
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        self.cursor_position = next_word_boundary(text, self.cursor_position, text_len);
        self.reset_blink();
    }

    pub fn select_to_start(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        self.cursor_position = 0;
        self.reset_blink();
    }

    pub fn select_to_end(&mut self, text_len: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_position);
        }
        self.cursor_position = text_len;
        self.reset_blink();
    }

    /// Select the word at the given character position (for double-click)
    pub fn select_word_at(&mut self, text: &str, char_pos: usize) {
        let text_len = text.chars().count();
        let start = prev_word_boundary(text, char_pos);
        let end = next_word_boundary(text, char_pos, text_len);
        self.selection_anchor = Some(start);
        self.cursor_position = end;
        self.reset_blink();
    }
}

// === Helper functions ===

pub fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
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

/// Find the previous word boundary (character position) from the given char position.
/// A word boundary is a transition between alphanumeric and non-alphanumeric chars.
fn prev_word_boundary(text: &str, char_pos: usize) -> usize {
    if char_pos == 0 {
        return 0;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut pos = char_pos;
    // Skip whitespace/non-alnum going backward
    while pos > 0 && !chars[pos - 1].is_alphanumeric() {
        pos -= 1;
    }
    // Skip alnum going backward
    while pos > 0 && chars[pos - 1].is_alphanumeric() {
        pos -= 1;
    }
    pos
}

/// Find the next word boundary (character position) from the given char position.
fn next_word_boundary(text: &str, char_pos: usize, text_len: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = char_pos;
    // Skip alnum going forward
    while pos < text_len && chars[pos].is_alphanumeric() {
        pos += 1;
    }
    // Skip whitespace/non-alnum going forward
    while pos < text_len && !chars[pos].is_alphanumeric() {
        pos += 1;
    }
    pos
}
