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

use crate::state::{AppEvent, AppState, ComposerTarget, InlineState, NewIssueFormFocus};

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
                //
                // Issue #454: the rendered draft lives on the focused form
                // field (title/body), not inline_state.text. Write the
                // rewritten text into the form so the renderer shows it.
                let stale = !matches!(
                    self.issues_state.inline_state,
                    InlineState::Composer {
                        target: ComposerTarget::NewIssue,
                        ..
                    }
                );
                if stale {
                    return true;
                }
                self.apply_rewrite_succeeded(replaced);
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

    /// Apply a successful rewrite to the focused form field (issue #454).
    ///
    /// The rendered draft lives on `new_issue_form`'s focused field (title
    /// or body), so the rewritten text must land there — not on the legacy
    /// `inline_state.text`. Picker fields fall back to the title so the
    /// rewrite is always visible.
    fn apply_rewrite_succeeded(&mut self, replaced: String) {
        let Some(form) = self.issues_state.new_issue_form.as_mut() else {
            return;
        };
        if form.focus == NewIssueFormFocus::Body {
            form.body_text = replaced;
            form.body_cursor = form.body_text.chars().count();
        } else {
            form.title_text = replaced;
            form.title_cursor = form.title_text.chars().count();
        }
        self.issues_state.draft_notice = Some("Issue draft rewritten by agent".to_owned());
    }
}

#[cfg(test)]
#[path = "issues_rewrite_tests.rs"]
mod tests;
