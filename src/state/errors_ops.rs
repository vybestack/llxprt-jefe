//! Errors-mode reducer operations (issue #292).
//!
//! The error log is purely local — no remote data, no async loads. This module
//! handles mode enter/exit (with prior-focus save/restore, mirroring
//! issues/prs/actions), list navigation, detail scrolling, focus cycling, and
//! clearing the log.

use super::{AppState, ErrorsFocus, PaneFocus, PriorAgentFocus, ScreenMode};
use crate::messages::{ErrorsMessage, NavDir, ScrollDir};

impl AppState {
    /// Enter errors mode, saving prior focus state.
    fn enter_errors_mode(&mut self) -> bool {
        self.errors_state.prior_agent_focus = Some(PriorAgentFocus {
            pane_focus: self.pane_focus,
            selected_repository_index: self.selected_repository_index,
            selected_agent_index: self.selected_agent_index,
        });
        self.screen_mode = ScreenMode::DashboardErrors;
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

    /// Exit errors mode, restoring prior focus state.
    fn exit_errors_mode(&mut self) {
        self.screen_mode = ScreenMode::Dashboard;
        self.errors_state.active = false;
        if let Some(prior) = self.errors_state.prior_agent_focus.take() {
            self.pane_focus = prior.pane_focus;
            if let Some(idx) = prior.selected_agent_index
                && idx < self.agents.len()
            {
                self.selected_agent_index = Some(idx);
            }
            if let Some(idx) = prior.selected_repository_index
                && idx < self.repositories.len()
            {
                self.selected_repository_index = Some(idx);
            }
        } else {
            self.pane_focus = PaneFocus::Agents;
        }
    }

    fn refocus_error_list(&mut self) -> bool {
        self.errors_state.focus = ErrorsFocus::ErrorList;
        true
    }

    fn handle_error_navigation(&mut self, dir: NavDir) -> bool {
        if matches!(self.errors_state.focus, ErrorsFocus::RepoList)
            && matches!(dir, NavDir::Up | NavDir::Down)
        {
            self.move_repo_selection(dir);
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
            ErrorsMessage::ClearAll => self.clear_all_errors(),
        }
    }
}

/// Capture runtime errors into the errors ring buffer (issue #292).
///
/// Called from `finalize_message` after every reducer step. Inspects all known
/// error slots (global `error_message`, per-mode `issues_state.error`,
/// `prs_state.error`, `actions_state.error`) and pushes a new entry into
/// `errors_state` when the text changes. Deduplication is per-slot.
pub(super) fn capture_runtime_errors(state: &mut AppState) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default();

    if let Some(ref msg) = state.error_message {
        state
            .errors_state
            .capture_global(msg, crate::domain::ErrorSource::Persistence, &timestamp);
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
