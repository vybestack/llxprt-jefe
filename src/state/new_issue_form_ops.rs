//! New Issue inline form reducer operations (issue #407).
//!
//! Pure state transitions for `IssuesState::new_issue_form`. No I/O — the
//! `app_input` layer reads the form state on submit and drives the
//! create-then-apply-properties pipeline. Sticky milestone/project defaults
//! are restored from `RepoPreferences` on open and remembered back on a
//! successful submit (via `remember_new_issue_preferences`).

use super::AppState;
use super::types::NewIssueFormState;
use crate::domain::RepositoryId;
use crate::state::events::AppEvent;
use crate::state::util::{delete_char_at, delete_char_before, insert_char_at};
use std::collections::HashSet;

impl AppState {
    /// Open the inline New Issue form (issue #407 A1). Restores sticky
    /// milestone/project from per-repo preferences and sets the inline
    /// composer state. When no repository is selected the form still opens
    /// with blank defaults (properties are applied on submit).
    pub(super) fn open_new_issue_form(&mut self) {
        let (milestone, project_ids) = self
            .selected_repository_id()
            .map(|rid| {
                let prefs = self.user_preferences.for_repo(rid);
                (
                    prefs.last_new_issue_milestone.clone(),
                    prefs.last_new_issue_project_ids.clone(),
                )
            })
            .unwrap_or_default();
        let state = NewIssueFormState {
            milestone,
            project_ids,
            // Land on Title so the first keystroke types into the title
            // (issue #454): the Template picker has no inline text, so the
            // pre-#407 "type immediately" contract regressed when the form
            // opened on the Template focus.
            focus: super::NewIssueFormFocus::Title,
            ..NewIssueFormState::default()
        };
        // The form renders inside the detail pane and captures all keyboard
        // input while open, so the detail pane must read as focused (issue #454).
        self.issues_state.issue_focus = super::IssueFocus::IssueDetail;
        self.issues_state.inline_state = super::InlineState::Composer {
            target: super::ComposerTarget::NewIssue,
            text: String::new(),
            cursor: 0,
        };
        self.issues_state.new_issue_form = Some(state);
    }

    /// Close the inline New Issue form and discard the draft (issue #407 A11).
    pub(super) fn close_new_issue_form(&mut self) {
        if self.issues_state.new_issue_form.is_some() {
            self.issues_state.new_issue_form = None;
            self.issues_state.inline_state = super::InlineState::None;
            // Return focus to the issue list now that the detail-pane form is
            // gone (issue #454): open_new_issue_form moved focus to the detail
            // pane, so close must move it back.
            self.issues_state.issue_focus = super::IssueFocus::IssueList;
        }
    }

    /// Apply a New Issue form event. Returns `true` if handled.
    pub(super) fn apply_new_issue_form_event(&mut self, event: &AppEvent) -> bool {
        if self.issues_state.new_issue_form.is_none() {
            return false;
        }
        match event {
            AppEvent::NewIssueCancel => {
                self.close_new_issue_form();
                true
            }
            AppEvent::NewIssueTemplateNext => self.new_issue_template_next(),
            AppEvent::NewIssueTypeNext => self.new_issue_type_next(),
            AppEvent::NewIssueSubmit => self.new_issue_submit(),
            AppEvent::NewIssueCreated {
                scope_repo_id,
                mutation_id,
                issue,
            } => {
                self.apply_new_issue_created(scope_repo_id, *mutation_id, (**issue).clone());
                true
            }
            AppEvent::NewIssueCreateFailed {
                scope_repo_id,
                mutation_id,
                issue_number,
                error,
            } => {
                self.apply_new_issue_create_failed(
                    scope_repo_id,
                    *mutation_id,
                    *issue_number,
                    error.clone(),
                );
                true
            }
            AppEvent::NewIssueOptionsLoaded {
                labels,
                milestones,
                types,
                assignees,
            } => self.new_issue_options_loaded(
                labels.clone(),
                milestones.clone(),
                types.clone(),
                assignees.clone(),
            ),
            AppEvent::NewIssueOptionsFailed { error } => {
                self.new_issue_options_failed(error.clone())
            }
            title_event => self.apply_new_issue_title_event(title_event),
        }
    }

    fn apply_new_issue_title_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::NewIssueTitleChar(c) => self.new_issue_title_char(*c),
            AppEvent::NewIssueTitleBackspace => self.new_issue_title_backspace(),
            AppEvent::NewIssueTitleDelete => self.new_issue_title_delete(),
            AppEvent::NewIssueTitleCursorLeft => self.new_issue_title_cursor_left(),
            AppEvent::NewIssueTitleCursorRight => self.new_issue_title_cursor_right(),
            AppEvent::NewIssueTitleCursorHome => self.new_issue_title_cursor_home(),
            AppEvent::NewIssueTitleCursorEnd => self.new_issue_title_cursor_end(),
            body_event => self.apply_new_issue_body_event(body_event),
        }
    }

    fn apply_new_issue_body_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::NewIssueBodyChar(c) => self.new_issue_body_char(*c),
            AppEvent::NewIssueBodyNewline => self.new_issue_body_newline(),
            AppEvent::NewIssueBodyBackspace => self.new_issue_body_backspace(),
            AppEvent::NewIssueBodyDelete => self.new_issue_body_delete(),
            AppEvent::NewIssueBodyCursorLeft => self.new_issue_body_cursor_left(),
            AppEvent::NewIssueBodyCursorRight => self.new_issue_body_cursor_right(),
            AppEvent::NewIssueBodyCursorUp => self.new_issue_body_cursor_up(),
            AppEvent::NewIssueBodyCursorDown => self.new_issue_body_cursor_down(),
            AppEvent::NewIssueBodyCursorHome => self.new_issue_body_cursor_home(),
            AppEvent::NewIssueBodyCursorEnd => self.new_issue_body_cursor_end(),
            AppEvent::NewIssueFocusNext => self.new_issue_focus_next(),
            AppEvent::NewIssueFocusPrev => self.new_issue_focus_prev(),
            _ => false,
        }
    }

    fn with_form_mut<R>(&mut self, f: impl FnOnce(&mut NewIssueFormState) -> R) -> Option<R> {
        self.issues_state.new_issue_form.as_mut().map(f)
    }

    fn new_issue_template_next(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.template = d.template.next();
            d.body_text = d.template.body_scaffold().to_string();
            d.body_cursor = d.body_text.chars().count();
            // Clear the title for built-in templates; the user types it fresh.
            d.title_text.clear();
            d.title_cursor = 0;
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_type_next(&mut self) -> bool {
        self.with_form_mut(|d| {
            if d.available_types.is_empty() {
                d.type_name = None;
                d.type_id = None;
            } else {
                // Cycle: None → first → second → ... → None.
                let current_idx = d
                    .type_id
                    .as_deref()
                    .and_then(|id| d.available_types.iter().position(|t| t.id == id));
                let next_idx = match current_idx {
                    Some(idx) if idx + 1 < d.available_types.len() => Some(idx + 1),
                    Some(_) => None, // wrap back to None (clear)
                    None => Some(0),
                };
                if let Some(idx) = next_idx {
                    let t = &d.available_types[idx];
                    d.type_id = Some(t.id.clone());
                    d.type_name = Some(t.name.clone());
                } else {
                    d.type_id = None;
                    d.type_name = None;
                }
            }
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_title_char(&mut self, c: char) -> bool {
        self.with_form_mut(|d| {
            // Title is single-line: reject newlines.
            if c != '\n' && c != '\r' {
                d.title_cursor = insert_char_at(&mut d.title_text, d.title_cursor, c);
                d.error = None;
            }
        })
        .is_some()
    }

    fn new_issue_title_backspace(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.title_cursor = delete_char_before(&mut d.title_text, d.title_cursor);
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_title_delete(&mut self) -> bool {
        self.with_form_mut(|d| {
            delete_char_at(&mut d.title_text, d.title_cursor);
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_title_cursor_left(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.title_cursor = d.title_cursor.saturating_sub(1);
        })
        .is_some()
    }

    fn new_issue_title_cursor_right(&mut self) -> bool {
        self.with_form_mut(|d| {
            let max = d.title_text.chars().count();
            if d.title_cursor < max {
                d.title_cursor += 1;
            }
        })
        .is_some()
    }

    fn new_issue_title_cursor_home(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.title_cursor = 0;
        })
        .is_some()
    }

    fn new_issue_title_cursor_end(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.title_cursor = d.title_text.chars().count();
        })
        .is_some()
    }

    fn new_issue_body_char(&mut self, c: char) -> bool {
        self.with_form_mut(|d| {
            d.body_cursor = insert_char_at(&mut d.body_text, d.body_cursor, c);
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_body_newline(&mut self) -> bool {
        self.new_issue_body_char('\n')
    }

    fn new_issue_body_backspace(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.body_cursor = delete_char_before(&mut d.body_text, d.body_cursor);
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_body_delete(&mut self) -> bool {
        self.with_form_mut(|d| {
            delete_char_at(&mut d.body_text, d.body_cursor);
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_body_cursor_left(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.body_cursor = d.body_cursor.saturating_sub(1);
        })
        .is_some()
    }

    fn new_issue_body_cursor_right(&mut self) -> bool {
        self.with_form_mut(|d| {
            let max = d.body_text.chars().count();
            if d.body_cursor < max {
                d.body_cursor += 1;
            }
        })
        .is_some()
    }

    fn new_issue_body_cursor_up(&mut self) -> bool {
        self.with_form_mut(|d| {
            crate::state::util::inline_cursor_vertical(&d.body_text, &mut d.body_cursor, -1);
        })
        .is_some()
    }

    fn new_issue_body_cursor_down(&mut self) -> bool {
        self.with_form_mut(|d| {
            crate::state::util::inline_cursor_vertical(&d.body_text, &mut d.body_cursor, 1);
        })
        .is_some()
    }

    fn new_issue_body_cursor_home(&mut self) -> bool {
        self.with_form_mut(|d| {
            crate::state::util::inline_cursor_line_start(&d.body_text, &mut d.body_cursor);
        })
        .is_some()
    }

    fn new_issue_body_cursor_end(&mut self) -> bool {
        self.with_form_mut(|d| {
            crate::state::util::inline_cursor_line_end(&d.body_text, &mut d.body_cursor);
        })
        .is_some()
    }

    fn new_issue_focus_next(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.focus = d.focus.next();
        })
        .is_some()
    }

    fn new_issue_focus_prev(&mut self) -> bool {
        self.with_form_mut(|d| {
            d.focus = d.focus.prev();
        })
        .is_some()
    }

    /// Submit validation (issue #407 A10). The actual create pipeline is
    /// spawned by the `app_input` layer after the reducer signals readiness;
    /// here we only validate the title is non-empty and clear/block
    /// accordingly. Returns `true` (handled) when the form is open.
    fn new_issue_submit(&mut self) -> bool {
        let title_empty = self
            .issues_state
            .new_issue_form
            .as_ref()
            .is_some_and(|d| d.title_text.trim().is_empty());
        if title_empty {
            self.with_form_mut(|d| {
                d.error = Some("Issue title cannot be empty".to_string());
            });
        } else {
            // Clear any stale validation error so the footer does not show a
            // previous empty-title error once the title is valid.
            self.with_form_mut(|d| {
                d.error = None;
            });
        }
        // The app_input layer is responsible for spawning the create task;
        // the reducer only validates. Return true so the event is consumed.
        true
    }

    /// Apply a successful New Issue create (issue #407 A9): insert the issue
    /// into the local list, remember the sticky milestone/project, and close
    /// the form.
    fn apply_new_issue_created(
        &mut self,
        scope_repo_id: &RepositoryId,
        mutation_id: u64,
        issue: crate::domain::Issue,
    ) {
        // Staleness guard: only apply if the mutation is still pending and
        // the repo has not changed.
        let Some(pending) = self.issues_state.mutation_pending.as_ref() else {
            return;
        };
        if pending.id != mutation_id || self.selected_repository_id() != Some(scope_repo_id) {
            return;
        }
        let issue_number = issue.number;
        if super::issues_mutation_ops::created_issue_visible_in_committed_filter(
            &self.issues_state.committed_filter,
        ) {
            super::issues_mutation_ops::prepend_or_replace_created_issue(
                &mut self.issues_state.list,
                issue,
            );
            self.issues_state.list.set_selected_index(Some(0));
        }
        self.issues_state.issue_focus = super::IssueFocus::IssueList;
        self.issues_state.draft_notice = Some(format!("Created issue #{issue_number}"));
        self.issues_state.error = None;
        self.issues_state.mutation_pending = None;
        // Remember sticky milestone/project before closing the form.
        self.remember_new_issue_preferences();
        self.close_new_issue_form();
    }

    /// Apply a New Issue create failure (issue #407 A10): surface the error in
    /// the open form and clear the pending mutation so the user can retry.
    /// When `issue_number` is `Some`, the issue was created but a property
    /// failed — the error message includes the number so the user can finish
    /// the properties by hand.
    fn apply_new_issue_create_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        mutation_id: u64,
        issue_number: Option<u64>,
        error: String,
    ) {
        let Some(pending) = self.issues_state.mutation_pending.as_ref() else {
            return;
        };
        if pending.id != mutation_id || self.selected_repository_id() != Some(scope_repo_id) {
            return;
        }
        self.issues_state.mutation_pending = None;
        let display = match issue_number {
            Some(n) => format!("Issue #{n} was created but a property failed: {error}"),
            None => error,
        };
        self.with_form_mut(|d| {
            d.error = Some(display);
        });
    }

    fn new_issue_options_loaded(
        &mut self,
        labels: Vec<String>,
        milestones: Vec<String>,
        types: Vec<crate::state::IssueType>,
        assignees: Vec<String>,
    ) -> bool {
        self.with_form_mut(|d| {
            // If the options list changed, clear any selection that no longer
            // exists in the fresh list so the form never holds a stale
            // selection (type, labels, milestone, assignees). Build HashSets
            // for O(1) membership checks instead of O(n*m) nested scans.
            if d.available_types != types
                && let Some(id) = d.type_id.as_ref()
                && !types.iter().any(|t| t.id == *id)
            {
                d.type_id = None;
                d.type_name = None;
            }
            if d.available_labels != labels {
                let label_set: HashSet<&str> = labels.iter().map(String::as_str).collect();
                d.labels.retain(|l| label_set.contains(l.as_str()));
            }
            if d.available_milestones != milestones
                && let Some(m) = d.milestone.as_ref()
                && !milestones.iter().any(|a| a == m)
            {
                d.milestone = None;
            }
            if d.available_assignees != assignees {
                let assignee_set: HashSet<&str> = assignees.iter().map(String::as_str).collect();
                d.assignees.retain(|a| assignee_set.contains(a.as_str()));
            }
            d.available_labels = labels;
            d.available_milestones = milestones;
            d.available_types = types;
            d.available_assignees = assignees;
            d.options_loading = false;
            d.error = None;
        })
        .is_some()
    }

    fn new_issue_options_failed(&mut self, error: String) -> bool {
        self.with_form_mut(|d| {
            d.options_loading = false;
            d.error = Some(error);
        })
        .is_some()
    }
}

#[cfg(test)]
#[path = "new_issue_form_ops_tests.rs"]
mod tests;
