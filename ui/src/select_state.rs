/// State management for the Select/Dropdown element.
///
/// Tracks open/closed state, highlighted option for keyboard navigation,
/// type-ahead search filtering, and scroll position within the dropdown.

/// A single option in a select dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    /// The value returned when this option is selected.
    pub value: String,
    /// Display text shown in the dropdown.
    pub label: String,
    /// Whether this option is disabled (not selectable).
    pub disabled: bool,
}

impl SelectOption {
    pub fn new(value: &str, label: &str) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            disabled: false,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// A group of options with a header label.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectGroup {
    pub label: String,
    pub options: Vec<SelectOption>,
}

impl SelectGroup {
    pub fn new(label: &str, options: Vec<SelectOption>) -> Self {
        Self {
            label: label.to_string(),
            options,
        }
    }
}

/// State for a Select/Dropdown element.
#[derive(Debug, Clone)]
pub struct SelectState {
    /// Whether the dropdown is open.
    pub open: bool,
    /// Index of the currently highlighted option (for keyboard navigation).
    pub highlighted: Option<usize>,
    /// Type-ahead search buffer.
    pub search_text: String,
    /// Scroll offset within the dropdown (in pixels).
    pub scroll_offset: f32,
}

impl SelectState {
    pub fn new() -> Self {
        Self {
            open: false,
            highlighted: None,
            search_text: String::new(),
            scroll_offset: 0.0,
        }
    }

    /// Toggle the dropdown open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.search_text.clear();
            self.scroll_offset = 0.0;
        }
    }

    /// Open the dropdown.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Close the dropdown and reset search.
    pub fn close(&mut self) {
        self.open = false;
        self.search_text.clear();
        self.scroll_offset = 0.0;
    }

    /// Move highlight to the next non-disabled option.
    pub fn highlight_next(&mut self, options: &[SelectOption]) {
        let count = options.len();
        if count == 0 {
            return;
        }
        let start = self.highlighted.map(|i| i + 1).unwrap_or(0);
        for offset in 0..count {
            let idx = (start + offset) % count;
            if !options[idx].disabled {
                self.highlighted = Some(idx);
                return;
            }
        }
    }

    /// Move highlight to the previous non-disabled option.
    pub fn highlight_prev(&mut self, options: &[SelectOption]) {
        let count = options.len();
        if count == 0 {
            return;
        }
        let start = self
            .highlighted
            .map(|i| if i == 0 { count - 1 } else { i - 1 })
            .unwrap_or(count - 1);
        for offset in 0..count {
            let idx = (start + count - offset) % count;
            if !options[idx].disabled {
                self.highlighted = Some(idx);
                return;
            }
        }
    }

    /// Move highlight to the first non-disabled option.
    pub fn highlight_first(&mut self, options: &[SelectOption]) {
        for (i, opt) in options.iter().enumerate() {
            if !opt.disabled {
                self.highlighted = Some(i);
                return;
            }
        }
    }

    /// Move highlight to the last non-disabled option.
    pub fn highlight_last(&mut self, options: &[SelectOption]) {
        for (i, opt) in options.iter().enumerate().rev() {
            if !opt.disabled {
                self.highlighted = Some(i);
                return;
            }
        }
    }

    /// Type-ahead: append a character and highlight the first matching option.
    pub fn type_ahead(&mut self, ch: char, options: &[SelectOption]) {
        self.search_text.push(ch);
        self.highlight_first_match(options);
    }

    /// Clear the type-ahead search buffer.
    pub fn clear_search(&mut self) {
        self.search_text.clear();
    }

    /// Highlight the first option whose label starts with the search text (case-insensitive).
    fn highlight_first_match(&mut self, options: &[SelectOption]) {
        let query = self.search_text.to_lowercase();
        for (i, opt) in options.iter().enumerate() {
            if !opt.disabled && opt.label.to_lowercase().starts_with(&query) {
                self.highlighted = Some(i);
                return;
            }
        }
    }

    /// Filter options by search text, returning indices of matching options.
    pub fn filtered_indices(&self, options: &[SelectOption]) -> Vec<usize> {
        if self.search_text.is_empty() {
            return (0..options.len()).collect();
        }
        let query = self.search_text.to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(_, opt)| opt.label.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }

    /// Ensure the highlighted option is visible within the scroll viewport.
    pub fn ensure_visible(&mut self, highlighted_idx: usize, item_height: f32, viewport_height: f32) {
        let item_top = highlighted_idx as f32 * item_height;
        let item_bottom = item_top + item_height;

        if item_top < self.scroll_offset {
            self.scroll_offset = item_top;
        } else if item_bottom > self.scroll_offset + viewport_height {
            self.scroll_offset = item_bottom - viewport_height;
        }
    }
}

impl Default for SelectState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_options() -> Vec<SelectOption> {
        vec![
            SelectOption::new("apple", "Apple"),
            SelectOption::new("banana", "Banana"),
            SelectOption::new("cherry", "Cherry"),
            SelectOption::new("date", "Date"),
            SelectOption::new("elderberry", "Elderberry"),
        ]
    }

    fn options_with_disabled() -> Vec<SelectOption> {
        vec![
            SelectOption::new("a", "Alpha"),
            SelectOption::new("b", "Beta").disabled(),
            SelectOption::new("c", "Charlie"),
            SelectOption::new("d", "Delta").disabled(),
            SelectOption::new("e", "Echo"),
        ]
    }

    #[test]
    fn test_new_state() {
        let state = SelectState::new();
        assert!(!state.open);
        assert_eq!(state.highlighted, None);
        assert!(state.search_text.is_empty());
    }

    #[test]
    fn test_toggle() {
        let mut state = SelectState::new();
        state.toggle();
        assert!(state.open);
        state.search_text = "test".to_string();
        state.toggle();
        assert!(!state.open);
        assert!(state.search_text.is_empty());
    }

    #[test]
    fn test_highlight_next() {
        let opts = sample_options();
        let mut state = SelectState::new();

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(0));

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(1));

        state.highlighted = Some(4);
        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(0)); // wraps around
    }

    #[test]
    fn test_highlight_prev() {
        let opts = sample_options();
        let mut state = SelectState::new();

        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, Some(4)); // starts from last

        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, Some(3));

        state.highlighted = Some(0);
        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, Some(4)); // wraps around
    }

    #[test]
    fn test_highlight_skips_disabled() {
        let opts = options_with_disabled();
        let mut state = SelectState::new();

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(0)); // Alpha

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(2)); // Charlie (skips Beta)

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, Some(4)); // Echo (skips Delta)
    }

    #[test]
    fn test_highlight_prev_skips_disabled() {
        let opts = options_with_disabled();
        let mut state = SelectState::new();
        state.highlighted = Some(4); // Echo

        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, Some(2)); // Charlie (skips Delta)

        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, Some(0)); // Alpha (skips Beta)
    }

    #[test]
    fn test_highlight_first_last() {
        let opts = options_with_disabled();
        let mut state = SelectState::new();

        state.highlight_first(&opts);
        assert_eq!(state.highlighted, Some(0));

        state.highlight_last(&opts);
        assert_eq!(state.highlighted, Some(4));
    }

    #[test]
    fn test_type_ahead() {
        let opts = sample_options();
        let mut state = SelectState::new();

        state.type_ahead('c', &opts);
        assert_eq!(state.highlighted, Some(2)); // Cherry

        state.clear_search();
        state.type_ahead('b', &opts);
        assert_eq!(state.highlighted, Some(1)); // Banana
    }

    #[test]
    fn test_type_ahead_multi_char() {
        let opts = vec![
            SelectOption::new("do", "Dog"),
            SelectOption::new("du", "Duck"),
            SelectOption::new("da", "Dart"),
        ];
        let mut state = SelectState::new();

        state.type_ahead('d', &opts);
        assert_eq!(state.highlighted, Some(0)); // Dog (first 'd')

        state.type_ahead('u', &opts);
        assert_eq!(state.highlighted, Some(1)); // Duck (matches "du")
    }

    #[test]
    fn test_filtered_indices() {
        let opts = sample_options();
        let mut state = SelectState::new();

        // No filter -> all options
        assert_eq!(state.filtered_indices(&opts), vec![0, 1, 2, 3, 4]);

        // Filter by "e" -> Apple, Cherry, Date, Elderberry (contain 'e')
        state.search_text = "e".to_string();
        assert_eq!(state.filtered_indices(&opts), vec![0, 2, 3, 4]);

        // Filter by "ber" -> Elderberry
        state.search_text = "ber".to_string();
        assert_eq!(state.filtered_indices(&opts), vec![4]);
    }

    #[test]
    fn test_ensure_visible() {
        let mut state = SelectState::new();
        let item_height = 32.0;
        let viewport = 128.0; // 4 items visible

        // Item 0 is visible at scroll 0
        state.ensure_visible(0, item_height, viewport);
        assert_eq!(state.scroll_offset, 0.0);

        // Item 5 is below viewport, scroll down
        state.ensure_visible(5, item_height, viewport);
        assert_eq!(state.scroll_offset, 5.0 * 32.0 + 32.0 - 128.0); // 64.0

        // Item 1 is above current scroll (64), scroll up
        state.ensure_visible(1, item_height, viewport);
        assert_eq!(state.scroll_offset, 32.0);
    }

    #[test]
    fn test_close_resets_state() {
        let mut state = SelectState::new();
        state.open = true;
        state.search_text = "test".to_string();
        state.scroll_offset = 100.0;

        state.close();
        assert!(!state.open);
        assert!(state.search_text.is_empty());
        assert_eq!(state.scroll_offset, 0.0);
    }

    #[test]
    fn test_empty_options() {
        let opts: Vec<SelectOption> = vec![];
        let mut state = SelectState::new();

        state.highlight_next(&opts);
        assert_eq!(state.highlighted, None);

        state.highlight_prev(&opts);
        assert_eq!(state.highlighted, None);

        state.highlight_first(&opts);
        assert_eq!(state.highlighted, None);

        state.highlight_last(&opts);
        assert_eq!(state.highlighted, None);
    }

    #[test]
    fn test_all_disabled() {
        let opts = vec![
            SelectOption::new("a", "Alpha").disabled(),
            SelectOption::new("b", "Beta").disabled(),
        ];
        let mut state = SelectState::new();

        // Should not highlight anything since all are disabled
        state.highlight_next(&opts);
        assert_eq!(state.highlighted, None);
    }
}
