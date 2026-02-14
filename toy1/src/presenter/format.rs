//! Formatting utilities for presenting data in the TUI.

use crate::data::models::{AgentStatus, TodoStatus};

/// Format elapsed seconds as HH:MM:SS.
#[must_use]
pub fn format_elapsed(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Returns the status icon character for an agent status.
#[must_use]
pub const fn status_icon(status: &AgentStatus) -> char {
    match status {
        AgentStatus::Running => 'o',
        AgentStatus::Completed => '+',
        AgentStatus::Errored => 'x',
        AgentStatus::Waiting => '*',
        AgentStatus::Paused => '#',
        AgentStatus::Queued => '-',
        AgentStatus::Dead => '!',
    }
}

/// Returns the status label for an agent status.
#[must_use]
pub const fn status_label(status: &AgentStatus) -> &str {
    match status {
        AgentStatus::Running => "Running",
        AgentStatus::Completed => "Completed",
        AgentStatus::Errored => "Errored",
        AgentStatus::Waiting => "Waiting",
        AgentStatus::Paused => "Paused",
        AgentStatus::Queued => "Queued",
        AgentStatus::Dead => "Dead",
    }
}

/// Returns the todo icon for a todo status.
#[must_use]
pub const fn todo_icon(status: &TodoStatus) -> char {
    match status {
        TodoStatus::Completed => '+',
        TodoStatus::InProgress => '>',
        TodoStatus::Pending => '-',
    }
}

/// Truncate a string to a maximum length, appending "..." if truncated.
#[must_use]
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_owned()
    } else if max_len > 3 {
        let mut result = s[..max_len - 3].to_owned();
        result.push_str("...");
        result
    } else {
        "...".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_elapsed_zero() {
        assert_eq!(format_elapsed(0), "00:00:00");
    }

    #[test]
    fn format_elapsed_minutes() {
        assert_eq!(format_elapsed(42 * 60), "00:42:00");
    }

    #[test]
    fn format_elapsed_hours() {
        assert_eq!(format_elapsed(2 * 3600 + 15 * 60 + 30), "02:15:30");
    }

    #[test]
    fn status_icons() {
        assert_eq!(status_icon(&AgentStatus::Running), 'o');
        assert_eq!(status_icon(&AgentStatus::Completed), '+');
        assert_eq!(status_icon(&AgentStatus::Errored), 'x');
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world!", 8), "hello...");
    }

    #[test]
    fn todo_icons() {
        assert_eq!(todo_icon(&TodoStatus::Completed), '+');
        assert_eq!(todo_icon(&TodoStatus::InProgress), '>');
        assert_eq!(todo_icon(&TodoStatus::Pending), '-');
    }
}
