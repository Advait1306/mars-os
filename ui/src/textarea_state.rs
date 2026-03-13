use crate::text_input_state::UndoEntry;

/// Internal state for a textarea element (multiline text editing).
#[derive(Debug, Clone)]
pub struct TextareaState {
    // Cursor position as (line, column) in character units
    pub cursor_line: usize,
    pub cursor_column: usize,

    // "Sticky" column for up/down movement through lines of different lengths
    pub desired_column: Option<usize>,

    // Scroll offsets in pixels
    pub scroll_offset_y: f32,
    pub scroll_offset_x: f32,

    // Selection anchor as (line, column) -- None if no selection
    pub selection_anchor: Option<(usize, usize)>,

    // Cursor blink
    pub cursor_visible: bool,
    pub blink_timer_ms: f32,

    // Undo/redo
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub undo_group_timer_ms: f32,

    // Interaction tracking
    pub click_count: u32,
    pub last_click_time_ms: f32,
    pub mouse_selecting: bool,
}

impl TextareaState {
    pub fn new() -> Self {
        Self {
            cursor_line: 0,
            cursor_column: 0,
            desired_column: None,
            scroll_offset_y: 0.0,
            scroll_offset_x: 0.0,
            selection_anchor: None,
            cursor_visible: true,
            blink_timer_ms: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_group_timer_ms: 0.0,
            click_count: 0,
            last_click_time_ms: 0.0,
            mouse_selecting: false,
        }
    }

    /// Step timers. Returns true if redraw needed.
    pub fn step(&mut self, dt: f32) -> bool {
        self.blink_timer_ms += dt * 1000.0;
        let mut needs_redraw = false;
        if self.blink_timer_ms >= 530.0 {
            self.blink_timer_ms = 0.0;
            self.cursor_visible = !self.cursor_visible;
            needs_redraw = true;
        }
        if self.undo_group_timer_ms > 0.0 {
            self.undo_group_timer_ms -= dt * 1000.0;
            if self.undo_group_timer_ms <= 0.0 {
                self.undo_group_timer_ms = 0.0;
            }
        }
        needs_redraw
    }

    pub fn reset_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer_ms = 0.0;
    }

    // === Line utilities ===

    /// Split text into lines by '\n'.
    pub fn lines(text: &str) -> Vec<&str> {
        if text.is_empty() {
            return vec![""];
        }
        let lines: Vec<&str> = text.split('\n').collect();
        lines
    }

    /// Get the byte offset in `text` for the given (line, column) position.
    pub fn pos_to_byte_offset(text: &str, line: usize, column: usize) -> usize {
        let mut byte_offset = 0;
        for (i, l) in text.split('\n').enumerate() {
            if i == line {
                // Clamp column to line length
                let col = column.min(l.chars().count());
                byte_offset += l.char_indices()
                    .nth(col)
                    .map(|(idx, _)| idx)
                    .unwrap_or(l.len());
                return byte_offset;
            }
            byte_offset += l.len() + 1; // +1 for '\n'
        }
        text.len()
    }

    /// Convert a byte offset to (line, column).
    pub fn byte_offset_to_pos(text: &str, byte_offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut line_start = 0;
        for (i, ch) in text.char_indices() {
            if i >= byte_offset {
                let col = text[line_start..byte_offset].chars().count();
                return (line, col);
            }
            if ch == '\n' {
                line += 1;
                line_start = i + 1;
            }
        }
        // At the end of text
        let col = text[line_start..].chars().count();
        (line, col)
    }

    /// Convert (line, column) to a flat character position.
    pub fn pos_to_char_offset(text: &str, line: usize, column: usize) -> usize {
        let mut char_offset = 0;
        for (i, l) in text.split('\n').enumerate() {
            if i == line {
                return char_offset + column.min(l.chars().count());
            }
            char_offset += l.chars().count() + 1; // +1 for '\n'
        }
        text.chars().count()
    }

    /// Get the char count of the given line.
    pub fn line_char_count(text: &str, line: usize) -> usize {
        text.split('\n').nth(line).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// Total number of lines.
    pub fn line_count(text: &str) -> usize {
        if text.is_empty() {
            1
        } else {
            text.split('\n').count()
        }
    }

    // === Selection ===

    pub fn has_selection(&self) -> bool {
        self.selection_anchor.map_or(false, |a| a != (self.cursor_line, self.cursor_column))
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Get ordered selection range: ((start_line, start_col), (end_line, end_col)).
    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        self.selection_anchor.map(|anchor| {
            let cursor = (self.cursor_line, self.cursor_column);
            if anchor <= cursor {
                (anchor, cursor)
            } else {
                (cursor, anchor)
            }
        })
    }

    pub fn select_all(&mut self, text: &str) {
        self.selection_anchor = Some((0, 0));
        let lines = Self::lines(text);
        self.cursor_line = lines.len().saturating_sub(1);
        self.cursor_column = lines.last().map(|l| l.chars().count()).unwrap_or(0);
        self.reset_blink();
    }

    /// Get selected text.
    pub fn selected_text<'a>(&self, text: &'a str) -> String {
        if let Some((start, end)) = self.selection_range() {
            let start_byte = Self::pos_to_byte_offset(text, start.0, start.1);
            let end_byte = Self::pos_to_byte_offset(text, end.0, end.1);
            text[start_byte..end_byte].to_string()
        } else {
            String::new()
        }
    }

    // === Undo ===

    fn push_undo(&mut self, current: &str) {
        if self.undo_group_timer_ms > 0.0 {
            // Group with previous edit
        } else {
            self.undo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: Self::pos_to_char_offset(current, self.cursor_line, self.cursor_column),
                selection_anchor: self.selection_anchor.map(|(l, c)| {
                    Self::pos_to_char_offset(current, l, c)
                }),
            });
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
        }
        self.undo_group_timer_ms = 500.0;
        self.redo_stack.clear();
    }

    // === Editing ===

    /// Delete selected text, return new value.
    pub fn delete_selection(&mut self, current: &str) -> String {
        if let Some((start, end)) = self.selection_range() {
            let start_byte = Self::pos_to_byte_offset(current, start.0, start.1);
            let end_byte = Self::pos_to_byte_offset(current, end.0, end.1);
            let mut result = String::with_capacity(current.len());
            result.push_str(&current[..start_byte]);
            result.push_str(&current[end_byte..]);
            self.cursor_line = start.0;
            self.cursor_column = start.1;
            self.selection_anchor = None;
            result
        } else {
            current.to_string()
        }
    }

    /// Insert text at cursor, return new value.
    pub fn insert(&mut self, current: &str, text: &str) -> String {
        self.push_undo(current);
        let base = if self.has_selection() {
            self.delete_selection(current)
        } else {
            current.to_string()
        };
        let byte_pos = Self::pos_to_byte_offset(&base, self.cursor_line, self.cursor_column);
        let mut result = String::with_capacity(base.len() + text.len());
        result.push_str(&base[..byte_pos]);
        result.push_str(text);
        result.push_str(&base[byte_pos..]);

        // Update cursor position after insert
        let inserted_lines: Vec<&str> = text.split('\n').collect();
        if inserted_lines.len() == 1 {
            self.cursor_column += text.chars().count();
        } else {
            self.cursor_line += inserted_lines.len() - 1;
            self.cursor_column = inserted_lines.last().unwrap().chars().count();
        }
        self.selection_anchor = None;
        self.desired_column = None;
        self.reset_blink();
        result
    }

    /// Insert a newline at cursor.
    pub fn insert_newline(&mut self, current: &str) -> String {
        self.insert(current, "\n")
    }

    /// Insert a tab (spaces) at cursor.
    pub fn insert_tab(&mut self, current: &str, tab_size: u32) -> String {
        let spaces: String = std::iter::repeat(' ').take(tab_size as usize).collect();
        self.insert(current, &spaces)
    }

    /// Backspace: delete char before cursor or selection.
    pub fn backspace(&mut self, current: &str) -> String {
        self.push_undo(current);
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
        if self.cursor_line == 0 && self.cursor_column == 0 {
            return current.to_string();
        }
        let byte_pos = Self::pos_to_byte_offset(current, self.cursor_line, self.cursor_column);
        // Find previous char boundary
        let prev = Self::prev_char_byte(current, byte_pos);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..prev]);
        result.push_str(&current[byte_pos..]);
        // Update cursor
        let (new_line, new_col) = Self::byte_offset_to_pos(&result, prev);
        self.cursor_line = new_line;
        self.cursor_column = new_col;
        self.desired_column = None;
        self.reset_blink();
        result
    }

    /// Delete: delete char after cursor or selection.
    pub fn delete(&mut self, current: &str) -> String {
        self.push_undo(current);
        if self.has_selection() {
            let result = self.delete_selection(current);
            self.reset_blink();
            return result;
        }
        let byte_pos = Self::pos_to_byte_offset(current, self.cursor_line, self.cursor_column);
        if byte_pos >= current.len() {
            return current.to_string();
        }
        let next = Self::next_char_byte(current, byte_pos);
        let mut result = String::with_capacity(current.len());
        result.push_str(&current[..byte_pos]);
        result.push_str(&current[next..]);
        self.desired_column = None;
        self.reset_blink();
        result
    }

    /// Undo: return new value or None.
    pub fn undo(&mut self, current: &str) -> Option<String> {
        if let Some(entry) = self.undo_stack.pop() {
            let char_offset = Self::pos_to_char_offset(current, self.cursor_line, self.cursor_column);
            let sel = self.selection_anchor.map(|(l, c)| Self::pos_to_char_offset(current, l, c));
            self.redo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: char_offset,
                selection_anchor: sel,
            });
            let (line, col) = Self::char_offset_to_pos(&entry.value, entry.cursor_position);
            self.cursor_line = line;
            self.cursor_column = col;
            self.selection_anchor = entry.selection_anchor.map(|a| {
                Self::char_offset_to_pos(&entry.value, a)
            });
            self.desired_column = None;
            self.reset_blink();
            Some(entry.value)
        } else {
            None
        }
    }

    /// Redo: return new value or None.
    pub fn redo(&mut self, current: &str) -> Option<String> {
        if let Some(entry) = self.redo_stack.pop() {
            let char_offset = Self::pos_to_char_offset(current, self.cursor_line, self.cursor_column);
            let sel = self.selection_anchor.map(|(l, c)| Self::pos_to_char_offset(current, l, c));
            self.undo_stack.push(UndoEntry {
                value: current.to_string(),
                cursor_position: char_offset,
                selection_anchor: sel,
            });
            let (line, col) = Self::char_offset_to_pos(&entry.value, entry.cursor_position);
            self.cursor_line = line;
            self.cursor_column = col;
            self.selection_anchor = entry.selection_anchor.map(|a| {
                Self::char_offset_to_pos(&entry.value, a)
            });
            self.desired_column = None;
            self.reset_blink();
            Some(entry.value)
        } else {
            None
        }
    }

    // === Cursor movement ===

    pub fn move_left(&mut self, text: &str) {
        if self.has_selection() {
            if let Some((start, _)) = self.selection_range() {
                self.cursor_line = start.0;
                self.cursor_column = start.1;
            }
            self.clear_selection();
        } else if self.cursor_column > 0 {
            self.cursor_column -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_column = Self::line_char_count(text, self.cursor_line);
        }
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn move_right(&mut self, text: &str) {
        if self.has_selection() {
            if let Some((_, end)) = self.selection_range() {
                self.cursor_line = end.0;
                self.cursor_column = end.1;
            }
            self.clear_selection();
        } else {
            let line_len = Self::line_char_count(text, self.cursor_line);
            if self.cursor_column < line_len {
                self.cursor_column += 1;
            } else if self.cursor_line < Self::line_count(text) - 1 {
                self.cursor_line += 1;
                self.cursor_column = 0;
            }
        }
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn move_up(&mut self, text: &str) {
        if self.has_selection() {
            if let Some((start, _)) = self.selection_range() {
                self.cursor_line = start.0;
                self.cursor_column = start.1;
            }
            self.clear_selection();
        }
        if self.cursor_line > 0 {
            let col = self.desired_column.unwrap_or(self.cursor_column);
            self.desired_column = Some(col);
            self.cursor_line -= 1;
            let line_len = Self::line_char_count(text, self.cursor_line);
            self.cursor_column = col.min(line_len);
        }
        self.reset_blink();
    }

    pub fn move_down(&mut self, text: &str) {
        if self.has_selection() {
            if let Some((_, end)) = self.selection_range() {
                self.cursor_line = end.0;
                self.cursor_column = end.1;
            }
            self.clear_selection();
        }
        let total_lines = Self::line_count(text);
        if self.cursor_line < total_lines - 1 {
            let col = self.desired_column.unwrap_or(self.cursor_column);
            self.desired_column = Some(col);
            self.cursor_line += 1;
            let line_len = Self::line_char_count(text, self.cursor_line);
            self.cursor_column = col.min(line_len);
        }
        self.reset_blink();
    }

    pub fn move_to_line_start(&mut self) {
        if self.has_selection() {
            self.clear_selection();
        }
        self.cursor_column = 0;
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn move_to_line_end(&mut self, text: &str) {
        if self.has_selection() {
            self.clear_selection();
        }
        self.cursor_column = Self::line_char_count(text, self.cursor_line);
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn move_to_text_start(&mut self) {
        if self.has_selection() {
            self.clear_selection();
        }
        self.cursor_line = 0;
        self.cursor_column = 0;
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn move_to_text_end(&mut self, text: &str) {
        if self.has_selection() {
            self.clear_selection();
        }
        let lines = Self::lines(text);
        self.cursor_line = lines.len().saturating_sub(1);
        self.cursor_column = lines.last().map(|l| l.chars().count()).unwrap_or(0);
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn page_up(&mut self, text: &str, visible_lines: usize) {
        if self.has_selection() {
            self.clear_selection();
        }
        let col = self.desired_column.unwrap_or(self.cursor_column);
        self.desired_column = Some(col);
        self.cursor_line = self.cursor_line.saturating_sub(visible_lines);
        let line_len = Self::line_char_count(text, self.cursor_line);
        self.cursor_column = col.min(line_len);
        self.reset_blink();
    }

    pub fn page_down(&mut self, text: &str, visible_lines: usize) {
        if self.has_selection() {
            self.clear_selection();
        }
        let col = self.desired_column.unwrap_or(self.cursor_column);
        self.desired_column = Some(col);
        let total = Self::line_count(text);
        self.cursor_line = (self.cursor_line + visible_lines).min(total - 1);
        let line_len = Self::line_char_count(text, self.cursor_line);
        self.cursor_column = col.min(line_len);
        self.reset_blink();
    }

    // === Selection movement ===

    pub fn select_left(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        if self.cursor_column > 0 {
            self.cursor_column -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_column = Self::line_char_count(text, self.cursor_line);
        }
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_right(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        let line_len = Self::line_char_count(text, self.cursor_line);
        if self.cursor_column < line_len {
            self.cursor_column += 1;
        } else if self.cursor_line < Self::line_count(text) - 1 {
            self.cursor_line += 1;
            self.cursor_column = 0;
        }
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_up(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        if self.cursor_line > 0 {
            let col = self.desired_column.unwrap_or(self.cursor_column);
            self.desired_column = Some(col);
            self.cursor_line -= 1;
            let line_len = Self::line_char_count(text, self.cursor_line);
            self.cursor_column = col.min(line_len);
        }
        self.reset_blink();
    }

    pub fn select_down(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        let total = Self::line_count(text);
        if self.cursor_line < total - 1 {
            let col = self.desired_column.unwrap_or(self.cursor_column);
            self.desired_column = Some(col);
            self.cursor_line += 1;
            let line_len = Self::line_char_count(text, self.cursor_line);
            self.cursor_column = col.min(line_len);
        }
        self.reset_blink();
    }

    pub fn select_to_line_start(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        self.cursor_column = 0;
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_to_line_end(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        self.cursor_column = Self::line_char_count(text, self.cursor_line);
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_to_text_start(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        self.cursor_line = 0;
        self.cursor_column = 0;
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_to_text_end(&mut self, text: &str) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        let lines = Self::lines(text);
        self.cursor_line = lines.len().saturating_sub(1);
        self.cursor_column = lines.last().map(|l| l.chars().count()).unwrap_or(0);
        self.desired_column = None;
        self.reset_blink();
    }

    pub fn select_page_up(&mut self, text: &str, visible_lines: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        let col = self.desired_column.unwrap_or(self.cursor_column);
        self.desired_column = Some(col);
        self.cursor_line = self.cursor_line.saturating_sub(visible_lines);
        let line_len = Self::line_char_count(text, self.cursor_line);
        self.cursor_column = col.min(line_len);
        self.reset_blink();
    }

    pub fn select_page_down(&mut self, text: &str, visible_lines: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_line, self.cursor_column));
        }
        let col = self.desired_column.unwrap_or(self.cursor_column);
        self.desired_column = Some(col);
        let total = Self::line_count(text);
        self.cursor_line = (self.cursor_line + visible_lines).min(total - 1);
        let line_len = Self::line_char_count(text, self.cursor_line);
        self.cursor_column = col.min(line_len);
        self.reset_blink();
    }

    // === Scroll management ===

    /// Ensure the cursor is visible by adjusting scroll offsets.
    /// `line_height` is in pixels, `viewport_height`/`viewport_width` in pixels.
    pub fn ensure_cursor_visible(
        &mut self,
        line_height: f32,
        viewport_height: f32,
        viewport_width: f32,
        cursor_x: f32,
    ) {
        let cursor_y = self.cursor_line as f32 * line_height;

        // Vertical scroll
        if cursor_y < self.scroll_offset_y {
            self.scroll_offset_y = cursor_y;
        } else if cursor_y + line_height > self.scroll_offset_y + viewport_height {
            self.scroll_offset_y = cursor_y + line_height - viewport_height;
        }

        // Horizontal scroll
        let margin = 10.0;
        if cursor_x < self.scroll_offset_x + margin {
            self.scroll_offset_x = (cursor_x - margin).max(0.0);
        } else if cursor_x > self.scroll_offset_x + viewport_width - margin {
            self.scroll_offset_x = cursor_x - viewport_width + margin;
        }
    }

    // === Helper: char offset <-> (line, col) ===

    fn char_offset_to_pos(text: &str, char_offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for (i, ch) in text.chars().enumerate() {
            if i == char_offset {
                return (line, col);
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn prev_char_byte(s: &str, pos: usize) -> usize {
        if pos == 0 { return 0; }
        let mut i = pos - 1;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_char_byte(s: &str, pos: usize) -> usize {
        if pos >= s.len() { return s.len(); }
        let mut i = pos + 1;
        while i < s.len() && !s.is_char_boundary(i) {
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_empty() {
        assert_eq!(TextareaState::lines(""), vec![""]);
    }

    #[test]
    fn test_lines_multiline() {
        assert_eq!(TextareaState::lines("hello\nworld"), vec!["hello", "world"]);
    }

    #[test]
    fn test_lines_trailing_newline() {
        assert_eq!(TextareaState::lines("hello\n"), vec!["hello", ""]);
    }

    #[test]
    fn test_pos_to_byte_offset() {
        let text = "hello\nworld";
        assert_eq!(TextareaState::pos_to_byte_offset(text, 0, 0), 0);
        assert_eq!(TextareaState::pos_to_byte_offset(text, 0, 5), 5);
        assert_eq!(TextareaState::pos_to_byte_offset(text, 1, 0), 6);
        assert_eq!(TextareaState::pos_to_byte_offset(text, 1, 5), 11);
    }

    #[test]
    fn test_byte_offset_to_pos() {
        let text = "hello\nworld";
        assert_eq!(TextareaState::byte_offset_to_pos(text, 0), (0, 0));
        assert_eq!(TextareaState::byte_offset_to_pos(text, 5), (0, 5));
        assert_eq!(TextareaState::byte_offset_to_pos(text, 6), (1, 0));
        assert_eq!(TextareaState::byte_offset_to_pos(text, 11), (1, 5));
    }

    #[test]
    fn test_insert_text() {
        let mut state = TextareaState::new();
        let result = state.insert("hello\nworld", "X");
        assert_eq!(result, "Xhello\nworld");
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_column, 1);
    }

    #[test]
    fn test_insert_newline() {
        let mut state = TextareaState::new();
        state.cursor_line = 0;
        state.cursor_column = 5;
        let result = state.insert_newline("hello world");
        assert_eq!(result, "hello\n world");
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_column, 0);
    }

    #[test]
    fn test_backspace_at_line_start() {
        let mut state = TextareaState::new();
        state.cursor_line = 1;
        state.cursor_column = 0;
        let result = state.backspace("hello\nworld");
        assert_eq!(result, "helloworld");
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_column, 5);
    }

    #[test]
    fn test_backspace_middle_of_line() {
        let mut state = TextareaState::new();
        state.cursor_line = 0;
        state.cursor_column = 3;
        let result = state.backspace("hello");
        assert_eq!(result, "helo");
        assert_eq!(state.cursor_column, 2);
    }

    #[test]
    fn test_move_up_down() {
        let mut state = TextareaState::new();
        let text = "hello\nworld\nfoo";

        state.cursor_line = 0;
        state.cursor_column = 3;
        state.move_down(text);
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_column, 3);

        state.move_down(text);
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.cursor_column, 3); // "foo" has 3 chars, column clamps to 3

        state.move_up(text);
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_column, 3);
    }

    #[test]
    fn test_desired_column_preserved() {
        let mut state = TextareaState::new();
        let text = "hello world\nhi\nfoo bar baz";

        state.cursor_line = 0;
        state.cursor_column = 8;
        state.move_down(text);
        // Line 1 "hi" only has 2 chars, so column clamps to 2
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_column, 2);
        // But desired_column should be 8
        assert_eq!(state.desired_column, Some(8));

        state.move_down(text);
        // Line 2 has 11 chars, desired column 8 is available
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.cursor_column, 8);
    }

    #[test]
    fn test_selection() {
        let mut state = TextareaState::new();
        let text = "hello\nworld";

        state.cursor_line = 0;
        state.cursor_column = 2;
        state.select_right(text);
        state.select_right(text);
        state.select_right(text);

        assert!(state.has_selection());
        assert_eq!(state.selection_anchor, Some((0, 2)));
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_column, 5);
        assert_eq!(state.selected_text(text), "llo");
    }

    #[test]
    fn test_selection_across_lines() {
        let mut state = TextareaState::new();
        let text = "hello\nworld";

        state.cursor_line = 0;
        state.cursor_column = 3;
        state.select_down(text);

        assert!(state.has_selection());
        assert_eq!(state.selected_text(text), "lo\nwor");
    }

    #[test]
    fn test_delete_selection() {
        let mut state = TextareaState::new();
        let text = "hello\nworld";

        state.cursor_line = 0;
        state.cursor_column = 3;
        state.selection_anchor = Some((1, 2));

        let result = state.delete_selection(text);
        assert_eq!(result, "helrld");
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_column, 3);
    }

    #[test]
    fn test_select_all() {
        let mut state = TextareaState::new();
        let text = "hello\nworld\nfoo";
        state.select_all(text);
        assert_eq!(state.selection_anchor, Some((0, 0)));
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.cursor_column, 3);
        assert_eq!(state.selected_text(text), text);
    }

    #[test]
    fn test_line_count() {
        assert_eq!(TextareaState::line_count(""), 1);
        assert_eq!(TextareaState::line_count("hello"), 1);
        assert_eq!(TextareaState::line_count("hello\nworld"), 2);
        assert_eq!(TextareaState::line_count("a\nb\nc"), 3);
    }

    #[test]
    fn test_undo_redo() {
        let mut state = TextareaState::new();
        let text = "hello";
        let text2 = state.insert(text, "X");
        assert_eq!(text2, "Xhello");

        let undone = state.undo(&text2).unwrap();
        assert_eq!(undone, "hello");

        let redone = state.redo(&undone).unwrap();
        assert_eq!(redone, "Xhello");
    }

    #[test]
    fn test_unicode() {
        let text = "héllo\nwörld";
        assert_eq!(TextareaState::pos_to_byte_offset(text, 0, 2), 3); // 'é' is 2 bytes
        assert_eq!(TextareaState::line_char_count(text, 0), 5);
        assert_eq!(TextareaState::line_char_count(text, 1), 5);
    }
}
