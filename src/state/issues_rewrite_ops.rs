//! Reducer cases for the agent-driven new-issue draft rewrite (issue #214).
//!
//! The orchestration lives in the app_input layer: it reads the current
//! NewIssue composer draft, builds the rewrite instruction, runs the
//! configured default agent non-interactively, and applies the result back via
//! `IssueRewriteSucceeded` / `IssueRewriteFailed`. These reducer cases only own
//! the deterministic state transitions:
//!
//! - `RequestIssueRewrite`: flip `rewrite_pending` to true while a rewrite runs.
//! - `IssueRewriteSucceeded`: replace the composer text with the rewritten
//!   draft, drop the cursor at the end, clear the pending flag and any prior
//!   draft notice.
//! - `IssueRewriteFailed`: clear the pending flag and surface the error as a
//!   non-fatal draft notice so the original draft is preserved.

use crate::state::{AppEvent, AppState, ComposerTarget, InlineState};

impl AppState {
    pub(super) fn apply_issue_rewrite_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::RequestIssueRewrite => {
                // Only valid for the new-issue composer. A no-op (return true
                // so the event is consumed) when no NewIssue composer is
                // active or a mutation is already in flight.
                if self.issues_state.rewrite_pending {
                    return true;
                }
                let eligible = matches!(
                    self.issues_state.inline_state,
                    InlineState::Composer {
                        target: ComposerTarget::NewIssue,
                        ..
                    }
                ) && self.issues_state.mutation_pending.is_none();
                if eligible {
                    self.issues_state.rewrite_pending = true;
                    self.issues_state.draft_notice = Some("Rewriting issue draft…".to_owned());
                }
                true
            }
            AppEvent::IssueRewriteSucceeded { text: replaced } => {
                self.issues_state.rewrite_pending = false;
                if let InlineState::Composer {
                    target: ComposerTarget::NewIssue,
                    ref mut text,
                    ref mut cursor,
                } = self.issues_state.inline_state
                {
                    *text = replaced;
                    // Drop the caret at the end of the rewritten text. The
                    // cursor is a byte offset (see `insert_inline_char`).
                    *cursor = text.len();
                }
                self.issues_state.draft_notice = Some("Issue draft rewritten by agent".to_owned());
                true
            }
            AppEvent::IssueRewriteFailed { error } => {
                self.issues_state.rewrite_pending = false;
                // Preserve the existing draft; surface the failure as a
                // non-fatal notice so the user can retry or edit manually.
                self.issues_state.draft_notice = Some(format!("Agent rewrite failed: {error}"));
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "issues_rewrite_tests.rs"]
mod tests;
