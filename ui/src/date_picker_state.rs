/// State management for the DatePicker element.
///
/// Tracks calendar navigation (current month/year view), selected date,
/// time values, and open/closed state. All date math is pure Rust with
/// no external dependencies — popup surface creation is handled separately.

/// A calendar date (year, month 1-12, day 1-31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u32, // 1-12
    pub day: u32,   // 1-31
}

impl CalendarDate {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// Format as ISO date string "YYYY-MM-DD".
    pub fn to_iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Parse from "YYYY-MM-DD" format.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let year = parts[0].parse::<i32>().ok()?;
        let month = parts[1].parse::<u32>().ok()?;
        let day = parts[2].parse::<u32>().ok()?;
        if month < 1 || month > 12 {
            return None;
        }
        let max_day = days_in_month(year, month);
        if day < 1 || day > max_day {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Day of the week: 0 = Monday, 6 = Sunday (ISO weekday).
    pub fn weekday(&self) -> u32 {
        // Tomohiko Sakamoto's algorithm
        let t = [0i32, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let mut y = self.year;
        if self.month < 3 {
            y -= 1;
        }
        let dow = (y + y / 4 - y / 100 + y / 400 + t[(self.month - 1) as usize] + self.day as i32) % 7;
        // Sakamoto gives 0=Sunday; convert to 0=Monday
        ((dow + 6) % 7) as u32
    }
}

impl PartialOrd for CalendarDate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CalendarDate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.year, self.month, self.day).cmp(&(other.year, other.month, other.day))
    }
}

/// A time value (24-hour format).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeValue {
    pub hour: u32,   // 0-23
    pub minute: u32, // 0-59
}

impl TimeValue {
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            hour: hour.min(23),
            minute: minute.min(59),
        }
    }

    /// Format as "HH:MM".
    pub fn to_string(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// Parse from "HH:MM" format.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let hour = parts[0].parse::<u32>().ok()?;
        let minute = parts[1].parse::<u32>().ok()?;
        if hour > 23 || minute > 59 {
            return None;
        }
        Some(Self { hour, minute })
    }

    /// Increment hour, wrapping at 24.
    pub fn next_hour(&mut self) {
        self.hour = (self.hour + 1) % 24;
    }

    /// Decrement hour, wrapping at 24.
    pub fn prev_hour(&mut self) {
        self.hour = if self.hour == 0 { 23 } else { self.hour - 1 };
    }

    /// Increment minute by a step, wrapping at 60.
    pub fn next_minute(&mut self, step: u32) {
        let step = step.max(1);
        self.minute = (self.minute + step) % 60;
    }

    /// Decrement minute by a step, wrapping at 60.
    pub fn prev_minute(&mut self, step: u32) {
        let step = step.max(1);
        self.minute = if self.minute < step {
            60 - (step - self.minute)
        } else {
            self.minute - step
        };
    }
}

/// Which part of the date picker popup is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePickerFocus {
    Calendar,
    HourInput,
    MinuteInput,
}

/// State for a DatePicker element.
#[derive(Debug, Clone)]
pub struct DatePickerState {
    /// Whether the picker popup is open.
    pub open: bool,
    /// The currently selected date (None if no date selected yet).
    pub selected_date: Option<CalendarDate>,
    /// The currently selected time.
    pub time: TimeValue,
    /// The month/year currently being viewed in the calendar grid.
    pub view_year: i32,
    pub view_month: u32, // 1-12
    /// Optional minimum selectable date.
    pub min_date: Option<CalendarDate>,
    /// Optional maximum selectable date.
    pub max_date: Option<CalendarDate>,
    /// Which sub-control is focused.
    pub focus: DatePickerFocus,
    /// Minute increment step (default 1, common values: 1, 5, 15, 30).
    pub minute_step: u32,
}

impl DatePickerState {
    /// Create a new state. If a date is given, start viewing that month.
    pub fn new(selected: Option<CalendarDate>) -> Self {
        let (vy, vm) = selected
            .map(|d| (d.year, d.month))
            .unwrap_or((2026, 1));
        Self {
            open: false,
            selected_date: selected,
            time: TimeValue::new(0, 0),
            view_year: vy,
            view_month: vm,
            min_date: None,
            max_date: None,
            focus: DatePickerFocus::Calendar,
            minute_step: 1,
        }
    }

    /// Create from an ISO date string ("YYYY-MM-DD").
    pub fn from_date_str(s: &str) -> Self {
        Self::new(CalendarDate::parse(s))
    }

    /// Create from an ISO datetime string ("YYYY-MM-DD HH:MM").
    pub fn from_datetime_str(s: &str) -> Self {
        let parts: Vec<&str> = s.splitn(2, ' ').collect();
        let date = parts.first().and_then(|d| CalendarDate::parse(d));
        let time = parts.get(1).and_then(|t| TimeValue::parse(t)).unwrap_or(TimeValue::new(0, 0));
        let (vy, vm) = date.map(|d| (d.year, d.month)).unwrap_or((2026, 1));
        Self {
            open: false,
            selected_date: date,
            time,
            view_year: vy,
            view_month: vm,
            min_date: None,
            max_date: None,
            focus: DatePickerFocus::Calendar,
            minute_step: 1,
        }
    }

    /// Toggle the popup.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Open the popup. If a date is selected, view that month.
    pub fn open(&mut self) {
        self.open = true;
        if let Some(d) = self.selected_date {
            self.view_year = d.year;
            self.view_month = d.month;
        }
    }

    /// Close the popup.
    pub fn close(&mut self) {
        self.open = false;
    }

    // --- Calendar navigation ---

    /// Move to the next month.
    pub fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_month = 1;
            self.view_year += 1;
        } else {
            self.view_month += 1;
        }
    }

    /// Move to the previous month.
    pub fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_month = 12;
            self.view_year -= 1;
        } else {
            self.view_month -= 1;
        }
    }

    /// Move to the next year.
    pub fn next_year(&mut self) {
        self.view_year += 1;
    }

    /// Move to the previous year.
    pub fn prev_year(&mut self) {
        self.view_year -= 1;
    }

    /// Jump to today's date view (caller provides today's date).
    pub fn go_to_today(&mut self, today: CalendarDate) {
        self.view_year = today.year;
        self.view_month = today.month;
    }

    // --- Date selection ---

    /// Select a day in the current view month. Returns false if the date is out of range.
    pub fn select_day(&mut self, day: u32) -> bool {
        let max = days_in_month(self.view_year, self.view_month);
        if day < 1 || day > max {
            return false;
        }
        let date = CalendarDate::new(self.view_year, self.view_month, day);
        if !self.is_date_in_range(date) {
            return false;
        }
        self.selected_date = Some(date);
        true
    }

    /// Select a full date. Returns false if out of range.
    pub fn select_date(&mut self, date: CalendarDate) -> bool {
        if !self.is_date_in_range(date) {
            return false;
        }
        self.selected_date = Some(date);
        self.view_year = date.year;
        self.view_month = date.month;
        true
    }

    /// Clear the selected date.
    pub fn clear(&mut self) {
        self.selected_date = None;
    }

    /// Check if a date is within min/max range.
    pub fn is_date_in_range(&self, date: CalendarDate) -> bool {
        if let Some(min) = self.min_date {
            if date < min {
                return false;
            }
        }
        if let Some(max) = self.max_date {
            if date > max {
                return false;
            }
        }
        true
    }

    // --- Calendar grid ---

    /// Get the grid of days for the current view month.
    /// Returns a vec of weeks, each week is 7 slots (Mon-Sun).
    /// Slots outside the month are `None`.
    pub fn calendar_grid(&self) -> Vec<[Option<u32>; 7]> {
        let total_days = days_in_month(self.view_year, self.view_month);
        let first = CalendarDate::new(self.view_year, self.view_month, 1);
        let start_weekday = first.weekday(); // 0=Mon

        let mut weeks: Vec<[Option<u32>; 7]> = Vec::new();
        let mut current_day = 1u32;
        let mut week = [None; 7];

        // Fill first week
        for col in start_weekday..7 {
            if current_day <= total_days {
                week[col as usize] = Some(current_day);
                current_day += 1;
            }
        }
        weeks.push(week);

        // Fill remaining weeks
        while current_day <= total_days {
            week = [None; 7];
            for col in 0..7 {
                if current_day <= total_days {
                    week[col] = Some(current_day);
                    current_day += 1;
                }
            }
            weeks.push(week);
        }

        weeks
    }

    // --- Time ---

    /// Set the time directly.
    pub fn set_time(&mut self, hour: u32, minute: u32) {
        self.time = TimeValue::new(hour, minute);
    }

    // --- Formatted output ---

    /// Get the formatted date string for the closed-state display.
    pub fn formatted_date(&self) -> Option<String> {
        self.selected_date.map(|d| d.to_iso())
    }

    /// Get the formatted time string.
    pub fn formatted_time(&self) -> String {
        self.time.to_string()
    }

    /// Get the formatted datetime string "YYYY-MM-DD HH:MM".
    pub fn formatted_datetime(&self) -> Option<String> {
        self.selected_date.map(|d| {
            format!("{} {}", d.to_iso(), self.time.to_string())
        })
    }

    /// Get the current view month name.
    pub fn view_month_name(&self) -> &'static str {
        month_name(self.view_month)
    }
}

impl Default for DatePickerState {
    fn default() -> Self {
        Self::new(None)
    }
}

// --- Calendar math ---

/// Whether a year is a leap year.
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in a given month (1-12) of a given year.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 => 31,
        2 => if is_leap_year(year) { 29 } else { 28 },
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 0,
    }
}

/// Month name from number (1-12).
pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- CalendarDate tests ---

    #[test]
    fn test_date_parse_valid() {
        let d = CalendarDate::parse("2026-03-15").unwrap();
        assert_eq!(d.year, 2026);
        assert_eq!(d.month, 3);
        assert_eq!(d.day, 15);
    }

    #[test]
    fn test_date_parse_invalid() {
        assert!(CalendarDate::parse("").is_none());
        assert!(CalendarDate::parse("2026-13-01").is_none()); // month 13
        assert!(CalendarDate::parse("2026-02-30").is_none()); // feb 30
        assert!(CalendarDate::parse("2026-00-01").is_none()); // month 0
        assert!(CalendarDate::parse("not-a-date").is_none());
    }

    #[test]
    fn test_date_to_iso() {
        let d = CalendarDate::new(2026, 3, 5);
        assert_eq!(d.to_iso(), "2026-03-05");
    }

    #[test]
    fn test_date_ordering() {
        let a = CalendarDate::new(2026, 1, 15);
        let b = CalendarDate::new(2026, 3, 1);
        let c = CalendarDate::new(2025, 12, 31);
        assert!(a < b);
        assert!(c < a);
        assert_eq!(a, CalendarDate::new(2026, 1, 15));
    }

    #[test]
    fn test_weekday() {
        // 2026-03-13 is a Friday = 4 (Mon=0)
        let d = CalendarDate::new(2026, 3, 13);
        assert_eq!(d.weekday(), 4);

        // 2026-03-09 is a Monday = 0
        let d = CalendarDate::new(2026, 3, 9);
        assert_eq!(d.weekday(), 0);

        // 2026-03-15 is a Sunday = 6
        let d = CalendarDate::new(2026, 3, 15);
        assert_eq!(d.weekday(), 6);
    }

    // --- TimeValue tests ---

    #[test]
    fn test_time_parse() {
        let t = TimeValue::parse("14:30").unwrap();
        assert_eq!(t.hour, 14);
        assert_eq!(t.minute, 30);
    }

    #[test]
    fn test_time_parse_invalid() {
        assert!(TimeValue::parse("").is_none());
        assert!(TimeValue::parse("25:00").is_none());
        assert!(TimeValue::parse("12:60").is_none());
        assert!(TimeValue::parse("abc").is_none());
    }

    #[test]
    fn test_time_to_string() {
        let t = TimeValue::new(9, 5);
        assert_eq!(t.to_string(), "09:05");
    }

    #[test]
    fn test_time_wrap() {
        let mut t = TimeValue::new(23, 55);
        t.next_hour();
        assert_eq!(t.hour, 0);
        t.prev_hour();
        assert_eq!(t.hour, 23);

        let mut t = TimeValue::new(0, 0);
        t.prev_minute(5);
        assert_eq!(t.minute, 55);
        t.next_minute(5);
        assert_eq!(t.minute, 0);
    }

    #[test]
    fn test_time_clamps() {
        let t = TimeValue::new(30, 90);
        assert_eq!(t.hour, 23);
        assert_eq!(t.minute, 59);
    }

    // --- Calendar math tests ---

    #[test]
    fn test_leap_year() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    // --- DatePickerState tests ---

    #[test]
    fn test_new_empty() {
        let state = DatePickerState::new(None);
        assert!(!state.open);
        assert!(state.selected_date.is_none());
        assert_eq!(state.view_year, 2026);
        assert_eq!(state.view_month, 1);
    }

    #[test]
    fn test_new_with_date() {
        let d = CalendarDate::new(2026, 6, 15);
        let state = DatePickerState::new(Some(d));
        assert_eq!(state.view_year, 2026);
        assert_eq!(state.view_month, 6);
        assert_eq!(state.selected_date, Some(d));
    }

    #[test]
    fn test_from_date_str() {
        let state = DatePickerState::from_date_str("2026-03-13");
        assert_eq!(state.selected_date, Some(CalendarDate::new(2026, 3, 13)));
        assert_eq!(state.view_month, 3);
    }

    #[test]
    fn test_from_datetime_str() {
        let state = DatePickerState::from_datetime_str("2026-03-13 14:30");
        assert_eq!(state.selected_date, Some(CalendarDate::new(2026, 3, 13)));
        assert_eq!(state.time.hour, 14);
        assert_eq!(state.time.minute, 30);
    }

    #[test]
    fn test_month_navigation() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2026;
        state.view_month = 1;

        state.prev_month();
        assert_eq!(state.view_year, 2025);
        assert_eq!(state.view_month, 12);

        state.next_month();
        assert_eq!(state.view_year, 2026);
        assert_eq!(state.view_month, 1);

        state.view_month = 12;
        state.next_month();
        assert_eq!(state.view_year, 2027);
        assert_eq!(state.view_month, 1);
    }

    #[test]
    fn test_year_navigation() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2026;
        state.next_year();
        assert_eq!(state.view_year, 2027);
        state.prev_year();
        assert_eq!(state.view_year, 2026);
    }

    #[test]
    fn test_select_day() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2026;
        state.view_month = 3;

        assert!(state.select_day(15));
        assert_eq!(state.selected_date, Some(CalendarDate::new(2026, 3, 15)));

        // Invalid day
        assert!(!state.select_day(32));
        assert!(!state.select_day(0));
    }

    #[test]
    fn test_select_day_respects_range() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2026;
        state.view_month = 3;
        state.min_date = Some(CalendarDate::new(2026, 3, 10));
        state.max_date = Some(CalendarDate::new(2026, 3, 20));

        assert!(!state.select_day(5));  // before min
        assert!(state.select_day(15));  // in range
        assert!(!state.select_day(25)); // after max
    }

    #[test]
    fn test_clear() {
        let mut state = DatePickerState::new(Some(CalendarDate::new(2026, 3, 13)));
        assert!(state.selected_date.is_some());
        state.clear();
        assert!(state.selected_date.is_none());
    }

    #[test]
    fn test_calendar_grid() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2026;
        state.view_month = 3; // March 2026 starts on Sunday

        let grid = state.calendar_grid();
        assert!(!grid.is_empty());

        // March 1, 2026 is a Sunday (weekday=6)
        let first_week = &grid[0];
        assert_eq!(first_week[6], Some(1)); // Sunday column
        for col in 0..6 {
            assert_eq!(first_week[col], None); // Mon-Sat empty
        }

        // Check last day (31st) exists somewhere
        let all_days: Vec<u32> = grid
            .iter()
            .flat_map(|w| w.iter().filter_map(|d| *d))
            .collect();
        assert_eq!(all_days.len(), 31);
        assert_eq!(*all_days.first().unwrap(), 1);
        assert_eq!(*all_days.last().unwrap(), 31);
    }

    #[test]
    fn test_calendar_grid_feb_leap() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2024;
        state.view_month = 2;

        let grid = state.calendar_grid();
        let all_days: Vec<u32> = grid
            .iter()
            .flat_map(|w| w.iter().filter_map(|d| *d))
            .collect();
        assert_eq!(all_days.len(), 29);
    }

    #[test]
    fn test_formatted_output() {
        let mut state = DatePickerState::new(Some(CalendarDate::new(2026, 3, 13)));
        state.time = TimeValue::new(14, 30);

        assert_eq!(state.formatted_date(), Some("2026-03-13".to_string()));
        assert_eq!(state.formatted_time(), "14:30");
        assert_eq!(
            state.formatted_datetime(),
            Some("2026-03-13 14:30".to_string())
        );
    }

    #[test]
    fn test_formatted_no_date() {
        let state = DatePickerState::new(None);
        assert_eq!(state.formatted_date(), None);
        assert_eq!(state.formatted_datetime(), None);
    }

    #[test]
    fn test_month_name() {
        assert_eq!(month_name(1), "January");
        assert_eq!(month_name(12), "December");
    }

    #[test]
    fn test_go_to_today() {
        let mut state = DatePickerState::new(None);
        state.view_year = 2020;
        state.view_month = 1;

        state.go_to_today(CalendarDate::new(2026, 3, 13));
        assert_eq!(state.view_year, 2026);
        assert_eq!(state.view_month, 3);
    }

    #[test]
    fn test_open_navigates_to_selected() {
        let mut state = DatePickerState::new(Some(CalendarDate::new(2026, 8, 20)));
        state.view_year = 2020;
        state.view_month = 1;
        state.open();
        assert_eq!(state.view_year, 2026);
        assert_eq!(state.view_month, 8);
    }
}
