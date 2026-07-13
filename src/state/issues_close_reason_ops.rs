//! Issue close-reason chooser state operations (issue #188).
//!
//! Owns the close-reason chooser overlay transitions: open, navigate, select
//! (arms confirmation or enters duplicate-search), duplicate-search
//! char/backspace/navigate, confirm (dispatches close with reason), and
//! cancel. All transitions are deterministic and side-effect-free.
//!
//! The actual gh I/O is performed by the dispatch layer
//! (`app_input::issues_lifecycle::handle_issue_close_with_reason`) which
//! reads the `close_mutation_pending` record set by `CloseReasonConfirm`.

use super::{
    AppEvent, AppState, IssueCloseReasonChooserState, IssueDuplicateSearchState,
    IssueLifecycleMutationPending, ReadOnlyHintKind,
};
use crate::domain::{CLOSE_REASONS, CloseReason, IssueState};

impl AppState {
    /// Apply a close-reason chooser event (returns handled).
    pub(super) fn apply_issue_close_reason_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::OpenCloseReasonChooser => {
                self.open_close_reason_chooser();
                true
            }
            AppEvent::CloseReasonNavigateUp => {
                self.navigate_close_reason(false);
                true
            }
            AppEvent::CloseReasonNavigateDown => {
                self.navigate_close_reason(true);
                true
            }
            AppEvent::CloseReasonSelect => {
                self.select_close_reason();
                true
            }
            AppEvent::CloseReasonDuplicateSearchChar(c) => {
                self.duplicate_search_char(*c);
                true
            }
            AppEvent::CloseReasonDuplicateSearchBackspace => {
                self.duplicate_search_backspace();
                true
            }
            AppEvent::CloseReasonDuplicateSearchNavigateUp => {
                self.duplicate_search_navigate(false);
                true
            }
            AppEvent::CloseReasonDuplicateSearchNavigateDown => {
                self.duplicate_search_navigate(true);
                true
            }
            AppEvent::CloseReasonConfirm => {
                self.confirm_close_reason();
                true
            }
            AppEvent::CloseReasonCancel => {
                self.issues_state.close_reason_chooser = None;
                true
            }
            _ => false,
        }
    }

    /// Open the close-reason chooser overlay (precondition: issue focused,
    /// open, no other overlay/mutation active).
    fn open_close_reason_chooser(&mut self) {
        if self.lifecycle_overlay_active() {
            return;
        }
        let Some(state) = self.focused_issue_state() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        if state == IssueState::Closed {
            self.show_issue_notice(ReadOnlyHintKind::IssueAlreadyClosed);
            return;
        }
        let Some(issue_number) = self.focused_issue_number() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        self.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
            issue_number,
            selected_index: 0,
            duplicate_search: None,
            awaiting_confirmation: false,
        });
    }

    /// Navigate the reason selection up or down (bounded).
    fn navigate_close_reason(&mut self, down: bool) {
        let Some(chooser) = &mut self.issues_state.close_reason_chooser else {
            return;
        };
        if chooser.duplicate_search.is_some() {
            return;
        }
        let max = CLOSE_REASONS.len().saturating_sub(1);
        chooser.selected_index = if down {
            (chooser.selected_index + 1).min(max)
        } else {
            chooser.selected_index.saturating_sub(1)
        };
    }

    /// Select the highlighted reason: arms confirmation or enters
    /// duplicate-search sub-state for `Duplicate`.
    fn select_close_reason(&mut self) {
        let (reason, issue_number) = {
            let Some(chooser) = &self.issues_state.close_reason_chooser else {
                return;
            };
            if chooser.duplicate_search.is_some() || chooser.awaiting_confirmation {
                return;
            }
            let r = CLOSE_REASONS
                .get(chooser.selected_index)
                .copied()
                .unwrap_or(CloseReason::Completed);
            (r, chooser.issue_number)
        };
        if reason == CloseReason::Duplicate {
            let candidates = self.duplicate_candidates_for(issue_number);
            if let Some(c) = &mut self.issues_state.close_reason_chooser {
                if c.issue_number != issue_number {
                    return;
                }
                c.duplicate_search = Some(IssueDuplicateSearchState {
                    query: String::new(),
                    candidates,
                    selected_index: 0,
                });
            }
        } else if let Some(c) = &mut self.issues_state.close_reason_chooser {
            c.awaiting_confirmation = true;
        }
    }

    /// Build the duplicate candidate list from loaded open issues, excluding
    /// the issue being closed.
    fn duplicate_candidates_for(&self, exclude_number: u64) -> Vec<(u64, String)> {
        self.issues_state
            .issues()
            .iter()
            .filter(|issue| issue.state == IssueState::Open && issue.number != exclude_number)
            .map(|issue| (issue.number, issue.title.clone()))
            .collect()
    }

    /// Type a character into the duplicate search query (digits only).
    fn duplicate_search_char(&mut self, c: char) {
        if !c.is_ascii_digit() {
            return;
        }
        let Some(chooser) = &mut self.issues_state.close_reason_chooser else {
            return;
        };
        let Some(search) = chooser.duplicate_search.as_mut() else {
            return;
        };
        search.query.push(c);
        search.selected_index = 0;
    }

    /// Backspace in the duplicate search query.
    fn duplicate_search_backspace(&mut self) {
        let Some(chooser) = &mut self.issues_state.close_reason_chooser else {
            return;
        };
        let Some(search) = chooser.duplicate_search.as_mut() else {
            return;
        };
        search.query.pop();
        search.selected_index = 0;
    }

    /// Navigate the duplicate search candidate selection up or down.
    fn duplicate_search_navigate(&mut self, down: bool) {
        let Some(chooser) = &mut self.issues_state.close_reason_chooser else {
            return;
        };
        let Some(search) = chooser.duplicate_search.as_mut() else {
            return;
        };
        let filtered = filter_duplicate_candidates(&search.candidates, &search.query);
        if filtered.is_empty() {
            search.selected_index = 0;
            return;
        }
        let max = filtered.len().saturating_sub(1);
        search.selected_index = if down {
            (search.selected_index + 1).min(max)
        } else {
            search.selected_index.saturating_sub(1)
        };
    }

    /// Confirm the close-reason selection: sets `close_mutation_pending` with
    /// the reason (+ duplicate_of for Duplicate), then clears the chooser.
    ///
    /// Validates scope and (for Duplicate) a resolved duplicate target BEFORE
    /// consuming the chooser, so a recoverable error preserves the user's
    /// selection rather than silently dismissing the overlay.
    fn confirm_close_reason(&mut self) {
        // Validate scope first so we never destroy the chooser on a
        // recoverable early-exit (mirrors confirm_issue_delete).
        let Some(scope) = self.selected_issue_scope_repo_id() else {
            self.show_issue_notice(ReadOnlyHintKind::NoIssueFocused);
            return;
        };
        let Some(chooser) = self.issues_state.close_reason_chooser.clone() else {
            return;
        };
        let reason = CLOSE_REASONS
            .get(chooser.selected_index)
            .copied()
            .unwrap_or(CloseReason::Completed);

        let duplicate_of = if reason == CloseReason::Duplicate {
            chooser
                .duplicate_search
                .as_ref()
                .and_then(|s| {
                    if s.query.is_empty() {
                        None
                    } else {
                        s.query.parse::<u64>().ok()
                    }
                })
                .or_else(|| {
                    chooser.duplicate_search.as_ref().and_then(|s| {
                        let filtered = filter_duplicate_candidates(&s.candidates, &s.query);
                        filtered.get(s.selected_index).map(|(n, _)| *n)
                    })
                })
        } else {
            None
        };

        // A Duplicate close requires a resolved target. Without one, restore
        // the chooser so the user can pick a duplicate rather than dispatching
        // a semantically incomplete mutation.
        if reason == CloseReason::Duplicate && duplicate_of.is_none() {
            self.show_issue_notice(ReadOnlyHintKind::NoDuplicateTarget);
            return;
        }

        let issue_number = chooser.issue_number;
        let node_id = if reason == CloseReason::Duplicate {
            self.focused_issue_node_id(issue_number)
        } else {
            None
        };

        // All preconditions validated — commit by consuming the chooser.
        self.issues_state.close_reason_chooser = None;
        let mutation_id = self.next_issue_mutation_id();
        self.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
            scope_repo_id: scope,
            mutation_id,
            issue_number,
            node_id,
            close_reason: Some(reason),
            duplicate_of,
        });
    }
}

/// Filter duplicate candidates by number-prefix of the query.
///
/// Pure projection: returns references into the input slice. Unit-testable
/// without iocraft.
#[must_use]
pub fn filter_duplicate_candidates<'a>(
    issues: &'a [(u64, String)],
    query: &str,
) -> Vec<&'a (u64, String)> {
    if query.is_empty() {
        return issues.iter().collect();
    }
    issues
        .iter()
        .filter(|(number, _)| number.to_string().starts_with(query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_empty_query_returns_all() {
        let candidates = vec![(1u64, "First".to_string()), (42u64, "Second".to_string())];
        let filtered = filter_duplicate_candidates(&candidates, "");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_by_number_prefix() {
        let candidates = vec![
            (1u64, "First".to_string()),
            (10u64, "Second".to_string()),
            (100u64, "Third".to_string()),
            (42u64, "Fourth".to_string()),
        ];
        let filtered = filter_duplicate_candidates(&candidates, "1");
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].0, 1);
        assert_eq!(filtered[1].0, 10);
        assert_eq!(filtered[2].0, 100);
    }

    #[test]
    fn filter_no_match_returns_empty() {
        let candidates = vec![(1u64, "First".to_string()), (42u64, "Second".to_string())];
        let filtered = filter_duplicate_candidates(&candidates, "999");
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_exact_match_returns_one() {
        let candidates = vec![(1u64, "First".to_string()), (42u64, "Second".to_string())];
        let filtered = filter_duplicate_candidates(&candidates, "42");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, 42);
    }
}
