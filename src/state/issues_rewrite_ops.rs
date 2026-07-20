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
//!   draft, drop the cursor at the end, clear the pending flag, and surface a
//!   confirmation notice. Stale results (composer no longer a NewIssue draft)
//!   only clear the pending flag so the user is never surprised by a text
//!   change in an unrelated view.
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
                // Staleness guard: only apply the rewritten text and notice
                // when the user is still on the NewIssue composer. If they
                // navigated away (comment composer, closed, etc.) the result
                // is dropped — only the pending flag is cleared so the state
                // never gets stuck waiting. The pending flag is always cleared
                // so a future request is never permanently blocked.
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
                    self.issues_state.draft_notice =
                        Some("Issue draft rewritten by agent".to_owned());
                }
                true
            }
            AppEvent::IssueRewriteFailed { error } => {
                self.issues_state.rewrite_pending = false;
                // Scope the notice to the NewIssue composer too, so a failure
                // is not surfaced in an unrelated view. The draft is preserved
                // either way; the pending flag is always cleared.
                if matches!(
                    self.issues_state.inline_state,
                    InlineState::Composer {
                        target: ComposerTarget::NewIssue,
                        ..
                    }
                ) {
                    self.issues_state.draft_notice = Some(format!("Agent rewrite failed: {error}"));
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "issues_rewrite_tests.rs"]
mod tests;
