//! Errors-mode reducer operations (issue #292).
//!
//! The error log is purely local — no remote data, no async loads. This module
//! handles mode enter/exit, list navigation, detail scrolling, focus cycling, and
//! clearing the log.

use super::{AppState, ErrorsFocus, ScreenId};
use crate::messages::{ErrorsMessage, NavDir, ScrollDir};

impl AppState {
    /// Enter errors mode with an instance-local copy of the captured error log.
    fn enter_errors_mode(&mut self) -> bool {
        let captured_errors = self.errors_state.clone();
        let source_repository_index = self.selected_repository_index;
        let _ = self.show_screen(ScreenId::Errors);
        self.errors_state = captured_errors;
        self.selected_repository_index = source_repository_index;
        self.errors_state.active = true;
        self.errors_state.focus = ErrorsFocus::ErrorList;
        // Ensure selection is valid (newest error after any recent push).
        if self.errors_state.errors.is_empty() {
            self.errors_state.selected_index = None;
        } else {
            self.errors_state.selected_index = Some(0);
        }
        self.errors_state.detail_scroll_offset = 0;
        true
    }

    /// Exit errors mode after clearing transient state on the disposed instance.
    fn exit_errors_mode(&mut self) {
        self.errors_state.active = false;
        let _ = self.leave_screen();
    }

    fn refocus_error_list(&mut self) -> bool {
        self.errors_state.focus = ErrorsFocus::ErrorList;
        true
    }

    fn handle_error_navigation(&mut self, dir: NavDir) -> bool {
        if matches!(self.errors_state.focus, ErrorsFocus::RepoList) {
            if matches!(dir, NavDir::Up | NavDir::Down) {
                self.move_repo_selection(dir);
            }
            // Home/End/Page/Next/Prev are no-ops for the repo sidebar today.
            return true;
        }
        let count = self.errors_state.errors.len();
        if count == 0 {
            return true;
        }
        let current = self.errors_state.selected_index.unwrap_or(0);
        let new_index = match dir {
            NavDir::Up => current.saturating_sub(1),
            NavDir::Down => (current + 1).min(count - 1),
            NavDir::Home => 0,
            NavDir::End => count - 1,
            NavDir::PageUp(_) | NavDir::PageDown(_) | NavDir::Next | NavDir::Prev => current,
        };
        self.errors_state.selected_index = Some(new_index);
        self.errors_state.detail_scroll_offset = 0;
        true
    }

    fn handle_error_enter(&mut self) -> bool {
        if matches!(self.errors_state.focus, ErrorsFocus::ErrorList)
            && self.errors_state.selected_error().is_some()
        {
            self.errors_state.focus = ErrorsFocus::ErrorDetail;
        }
        true
    }

    fn cycle_error_focus(&mut self) -> bool {
        self.errors_state.focus = match self.errors_state.focus {
            ErrorsFocus::RepoList => ErrorsFocus::ErrorList,
            ErrorsFocus::ErrorList => ErrorsFocus::ErrorDetail,
            ErrorsFocus::ErrorDetail => ErrorsFocus::RepoList,
        };
        true
    }

    fn cycle_error_focus_reverse(&mut self) -> bool {
        self.errors_state.focus = match self.errors_state.focus {
            ErrorsFocus::RepoList => ErrorsFocus::ErrorDetail,
            ErrorsFocus::ErrorList => ErrorsFocus::RepoList,
            ErrorsFocus::ErrorDetail => ErrorsFocus::ErrorList,
        };
        true
    }

    fn handle_error_scroll(&mut self, dir: ScrollDir) -> bool {
        let detail_lines = self.errors_detail_line_count();
        let max = detail_lines.saturating_sub(self.errors_state.detail_viewport_rows);
        let current = self.errors_state.detail_scroll_offset.min(max);
        self.errors_state.detail_scroll_offset = match dir {
            ScrollDir::Up => current.saturating_sub(1),
            ScrollDir::Down => current.saturating_add(1).min(max),
            ScrollDir::PageUp => current.saturating_sub(super::VIEWPORT_PAGE_JUMP),
            ScrollDir::PageDown => current.saturating_add(super::VIEWPORT_PAGE_JUMP).min(max),
        };
        true
    }

    fn clear_all_errors(&mut self) -> bool {
        self.errors_state.errors.clear();
        self.errors_state.selected_index = None;
        self.errors_state.detail_scroll_offset = 0;
        true
    }

    /// Number of wrapped detail lines for the selected error (approximation:
    /// one line per detail line in the stored text; the renderer wraps further
    /// but the scroll offset only needs to stay within a reasonable bound).
    fn errors_detail_line_count(&self) -> usize {
        self.errors_state.selected_error().map_or(0, |e| {
            // Header lines (title, source, timestamp) + detail body lines.
            let header = 4;
            let body = e.detail.lines().count().max(1);
            header + body
        })
    }

    /// Handle all Errors events.
    pub(super) fn apply_errors_message(&mut self, message: ErrorsMessage) -> bool {
        match message {
            ErrorsMessage::EnterMode => self.enter_errors_mode(),
            ErrorsMessage::ExitMode => {
                self.exit_errors_mode();
                true
            }
            ErrorsMessage::RefocusList => self.refocus_error_list(),
            ErrorsMessage::Navigate(dir) => self.handle_error_navigation(dir),
            ErrorsMessage::Enter => self.handle_error_enter(),
            ErrorsMessage::CycleFocus => self.cycle_error_focus(),
            ErrorsMessage::CycleFocusReverse => self.cycle_error_focus_reverse(),
            ErrorsMessage::ScrollDetail(dir) => self.handle_error_scroll(dir),
            ErrorsMessage::CaptureSilent {
                title,
                detail,
                source,
                timestamp,
            } => {
                self.errors_state
                    .push_silent(title, detail, source, timestamp);
                true
            }
            ErrorsMessage::ClearAll => self.clear_all_errors(),
        }
    }
}

/// Unix epoch seconds used to stamp a captured error.
fn now_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default()
}

/// Record a contained background-worker panic on the errors screen (issue #437).
///
/// Worker panics are recoverable but must never reach the terminal, because
/// the default panic hook writes over the running interface. Recording them
/// here keeps the copyable report — including its source location — available
/// even for routes that intentionally fail silently, and never steals the
/// current screen or selection.
///
/// This is deliberately not deduplicated: each panic is a distinct occurrence,
/// unlike the sticky error slots that `capture_runtime_errors` samples.
pub fn capture_worker_panic(state: &mut AppState, detail: &str) {
    let snap_to_newest = !state.errors_state.active;
    state.errors_state.push(
        "Background task panicked".to_string(),
        detail.to_string(),
        crate::domain::ErrorSource::Other,
        now_timestamp(),
        snap_to_newest,
    );
}

/// Record a startup session-reclaim report in the errors ring (issue #435).
///
/// Reclaim reports enumerate every unmatched session, so they routinely run to
/// hundreds of characters and used to be crammed into the single-line status-bar
/// warning slot, where they were unreadable and buried every other warning. They
/// are recorded silently: the operator gets a count as their cue and the full,
/// copyable list on the Errors screen, without the report stealing a selection
/// they were already working with.
pub fn capture_reclaim_report(state: &mut AppState, report: &str) {
    state.errors_state.push_silent(
        "Session reclaim report".to_string(),
        report.to_string(),
        crate::domain::ErrorSource::Other,
        now_timestamp(),
    );
}

/// Capture runtime errors into the errors ring buffer (issue #292).
///
/// Called from `finalize_message` after every reducer step. Inspects all known
/// error slots (global `error_message`, per-mode `issues_state.error`,
/// `prs_state.error`, `actions_state.error`) and pushes a new entry into
/// `errors_state` when the text changes. Deduplication is per-slot.
///
/// The timestamp is only allocated when at least one slot has changed, to
/// avoid per-message heap allocation on the hot path.
pub fn capture_runtime_errors(state: &mut AppState) {
    // Quick-change check: see if any slot differs from the last captured value.
    let global_changed =
        state.error_message.as_deref() != state.errors_state.last_captured_global_snapshot();
    let issues_changed =
        state.issues_state.error.as_deref() != state.errors_state.last_captured_issues_snapshot();
    let prs_changed =
        state.prs_state.error.as_deref() != state.errors_state.last_captured_prs_snapshot();
    let actions_changed =
        state.actions_state.error.as_deref() != state.errors_state.last_captured_actions_snapshot();

    if !global_changed && !issues_changed && !prs_changed && !actions_changed {
        return;
    }

    let timestamp = now_timestamp();

    if let Some(ref msg) = state.error_message {
        // `error_message` is a catch-all slot written from many subsystems
        // (agent lifecycle, availability, forms, auth, persistence, etc.),
        // so it cannot be attributed to a single source.
        state
            .errors_state
            .capture_global(msg, crate::domain::ErrorSource::Other, &timestamp);
    } else {
        state.errors_state.reset_global_tracker();
    }
    if let Some(ref msg) = state.issues_state.error {
        state.errors_state.capture_issues(msg, &timestamp);
    } else {
        state.errors_state.reset_issues_tracker();
    }
    if let Some(ref msg) = state.prs_state.error {
        state.errors_state.capture_prs(msg, &timestamp);
    } else {
        state.errors_state.reset_prs_tracker();
    }
    if let Some(ref msg) = state.actions_state.error {
        state.errors_state.capture_actions(msg, &timestamp);
    } else {
        state.errors_state.reset_actions_tracker();
    }
}
#[cfg(test)]
#[path = "errors_ops_issue403_tests.rs"]
mod issue403_tests;
