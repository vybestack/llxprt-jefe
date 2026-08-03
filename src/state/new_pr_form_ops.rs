//! New PR composer reducer operations (issue #183).
//!
//! Mirrors `new_issue_form_ops`: pure state transitions with no I/O. Every
//! reason a pull request will not be opened is decided here, so the boundary
//! layer never has to guess and never sends a request it knows will fail.

use super::types::{NewPrFormFocus, NewPrFormState, PrCreateMutationPending};
use super::util::{delete_char_at, delete_char_before, insert_char_at};
use super::{AppState, InlineState, PrFocus, PrLifecycleEvent};
use crate::domain::RepositoryId;

impl AppState {
    /// Apply a New PR composer event (returns handled).
    pub(super) fn apply_new_pr_form_event(&mut self, event: &PrLifecycleEvent) -> bool {
        match event {
            PrLifecycleEvent::OpenNewForm => {
                self.open_new_pr_form();
                true
            }
            PrLifecycleEvent::NewFormCancel => {
                self.prs_state.new_pr_form = None;
                true
            }
            PrLifecycleEvent::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            } => {
                self.apply_pr_branches_loaded(
                    scope_repo_id,
                    *request_id,
                    branches,
                    default_branch.as_deref(),
                );
                true
            }
            PrLifecycleEvent::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            } => {
                self.apply_pr_branches_failed(scope_repo_id, *request_id, error);
                true
            }
            PrLifecycleEvent::NewFormSubmit => {
                self.submit_new_pr_form();
                true
            }
            PrLifecycleEvent::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            } => {
                self.apply_pr_created(scope_repo_id, *mutation_id, *pr_number);
                true
            }
            PrLifecycleEvent::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            } => {
                self.apply_pr_create_failed(scope_repo_id, *mutation_id, error);
                true
            }
            other => self.apply_new_pr_form_edit(other),
        }
    }

    /// Navigation and text editing inside an open composer.
    fn apply_new_pr_form_edit(&mut self, event: &PrLifecycleEvent) -> bool {
        let Some(form) = self.prs_state.new_pr_form.as_mut() else {
            return false;
        };
        match event {
            PrLifecycleEvent::NewFormFocusNext => form.focus = form.focus.next(),
            PrLifecycleEvent::NewFormFocusPrevious => form.focus = form.focus.previous(),
            PrLifecycleEvent::NewFormBranchUp => move_branch_selection(form, false),
            PrLifecycleEvent::NewFormBranchDown => move_branch_selection(form, true),
            PrLifecycleEvent::NewFormChar(character) => insert_form_char(form, *character),
            PrLifecycleEvent::NewFormNewline => {
                if form.focus == NewPrFormFocus::Body {
                    insert_form_char(form, '\n');
                }
            }
            PrLifecycleEvent::NewFormBackspace => edit_focused_text(form, TextEdit::Backspace),
            PrLifecycleEvent::NewFormDelete => edit_focused_text(form, TextEdit::Delete),
            PrLifecycleEvent::NewFormCursorLeft => edit_focused_text(form, TextEdit::CursorLeft),
            PrLifecycleEvent::NewFormCursorRight => edit_focused_text(form, TextEdit::CursorRight),
            PrLifecycleEvent::NewFormCursorHome => edit_focused_text(form, TextEdit::CursorHome),
            PrLifecycleEvent::NewFormCursorEnd => edit_focused_text(form, TextEdit::CursorEnd),
            _ => return false,
        }
        true
    }

    /// Open the composer from the pull-request list.
    ///
    /// A composer opened with no repository selected could never resolve: the
    /// branch load would answer for a scope that matches nothing, and the
    /// composer would sit on "loading branches" forever. It is refused instead.
    fn open_new_pr_form(&mut self) {
        if self.prs_state.pr_focus != PrFocus::PrList || self.new_pr_form_blocked() {
            return;
        }
        if self.selected_repository_index.is_none() {
            self.prs_state.error =
                Some("Select a repository before opening a pull request.".to_string());
            return;
        }
        self.prs_state.next_property_request_id =
            self.prs_state.next_property_request_id.saturating_add(1);
        self.prs_state.new_pr_form = Some(NewPrFormState {
            branches_loading: true,
            load_request_id: self.prs_state.next_property_request_id,
            ..NewPrFormState::default()
        });
    }

    /// Whether an overlay or an in-flight mutation owns the screen already.
    fn new_pr_form_blocked(&self) -> bool {
        self.prs_state.inline_state != InlineState::None
            || self.prs_state.agent_chooser.is_some()
            || self.prs_state.merge_chooser.is_some()
            || self.prs_state.property_editor.is_some()
            || self.prs_state.delete_confirm.is_some()
            || self.prs_state.new_pr_form.is_some()
            || self.prs_state.mutation_pending.is_some()
            || self.prs_state.merge_mutation_pending.is_some()
            || self.prs_state.property_mutation_pending.is_some()
            || self.prs_state.delete_mutation_pending.is_some()
            || self.prs_state.create_mutation_pending.is_some()
    }

    /// Fill an open composer with the repository's branches.
    fn apply_pr_branches_loaded(
        &mut self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
        branches: &[String],
        default_branch: Option<&str>,
    ) {
        if !self.pr_scope_matches(scope_repo_id) {
            return;
        }
        let Some(form) = self.prs_state.new_pr_form.as_mut() else {
            return;
        };
        if form.load_request_id != request_id {
            return;
        }
        form.branches = branches.to_vec();
        form.branches_loading = false;
        form.error = None;
        let base_index = default_branch
            .and_then(|name| branches.iter().position(|branch| branch == name))
            .unwrap_or(0);
        form.base_index = base_index;
        // Opening a branch against itself is never what was meant, so the head
        // starts on the first branch that is not the base. A repository with a
        // single branch has no other choice, and the submit refuses it.
        form.head_index = (0..branches.len())
            .find(|index| *index != base_index)
            .unwrap_or(base_index);
    }

    /// Report a branch-load failure inside the composer.
    fn apply_pr_branches_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        request_id: u64,
        error: &str,
    ) {
        if !self.pr_scope_matches(scope_repo_id) {
            return;
        }
        let Some(form) = self.prs_state.new_pr_form.as_mut() else {
            return;
        };
        if form.load_request_id != request_id {
            return;
        }
        form.branches_loading = false;
        form.error = Some(format!("Could not list branches: {error}"));
    }

    /// Validate the draft and record the pending create.
    fn submit_new_pr_form(&mut self) {
        if self.prs_state.create_mutation_pending.is_some() {
            return;
        }
        let scope_repo_id = self
            .selected_repository_index
            .and_then(|index| self.repositories.get(index))
            .map(|repo| repo.id.clone());
        let Some(form) = self.prs_state.new_pr_form.as_mut() else {
            return;
        };
        if let Err(reason) = new_pr_submit_refusal(form) {
            form.error = Some(reason);
            return;
        }
        let Some(scope_repo_id) = scope_repo_id else {
            form.error = Some("Select a repository before opening a pull request.".to_string());
            return;
        };
        form.error = None;
        self.prs_state.next_mutation_id = self.prs_state.next_mutation_id.saturating_add(1);
        self.prs_state.create_mutation_pending = Some(PrCreateMutationPending {
            scope_repo_id,
            mutation_id: self.prs_state.next_mutation_id,
        });
    }

    /// Close the composer once GitHub confirms the pull request exists.
    fn apply_pr_created(&mut self, scope_repo_id: &RepositoryId, mutation_id: u64, pr_number: u64) {
        if !self.pr_create_pending_matches(scope_repo_id, mutation_id) {
            return;
        }
        self.prs_state.create_mutation_pending = None;
        self.prs_state.new_pr_form = None;
        self.prs_state.error = None;
        self.prs_state.post_mutation_refresh.request();
        self.prs_state.draft_notice = Some(format!("Opened PR #{pr_number}"));
    }

    /// Keep the draft and explain why GitHub refused it.
    fn apply_pr_create_failed(
        &mut self,
        scope_repo_id: &RepositoryId,
        mutation_id: u64,
        error: &str,
    ) {
        if !self.pr_create_pending_matches(scope_repo_id, mutation_id) {
            return;
        }
        self.prs_state.create_mutation_pending = None;
        if let Some(form) = self.prs_state.new_pr_form.as_mut() {
            form.error = Some(format!("Could not open the pull request: {error}"));
        } else {
            self.prs_state.error = Some(format!("Could not open the pull request: {error}"));
        }
    }

    /// Whether a create result belongs to the creation currently in flight.
    fn pr_create_pending_matches(&self, scope_repo_id: &RepositoryId, mutation_id: u64) -> bool {
        self.prs_state
            .create_mutation_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.mutation_id == mutation_id && pending.scope_repo_id == *scope_repo_id
            })
    }

    /// Whether an event's scope is the repository currently selected.
    fn pr_scope_matches(&self, scope_repo_id: &RepositoryId) -> bool {
        self.selected_repository_index
            .and_then(|index| self.repositories.get(index))
            .is_some_and(|repo| repo.id == *scope_repo_id)
    }
}

/// Why the composer will not open a pull request, if it will not.
fn new_pr_submit_refusal(form: &NewPrFormState) -> Result<(), String> {
    if form.branches_loading {
        return Err("Branches are still loading.".to_string());
    }
    let (Some(head), Some(base)) = (form.head_branch(), form.base_branch()) else {
        return Err("No branches are available to open a pull request from.".to_string());
    };
    if form.title_text.trim().is_empty() {
        return Err("Title cannot be empty.".to_string());
    }
    if head == base {
        return Err(format!(
            "The head branch {head} is also the base branch; pick two different branches."
        ));
    }
    Ok(())
}

/// Move the focused branch selection, clamped at the ends of the list.
///
/// Clamping rather than wrapping: a repository can have hundreds of branches,
/// and a list that jumps from the last back to the first hides where it ended.
fn move_branch_selection(form: &mut NewPrFormState, forward: bool) {
    let last = match form.branches.len() {
        0 => return,
        len => len - 1,
    };
    let index = match form.focus {
        NewPrFormFocus::Head => &mut form.head_index,
        NewPrFormFocus::Base => &mut form.base_index,
        NewPrFormFocus::Title | NewPrFormFocus::Body => return,
    };
    *index = if forward {
        (*index + 1).min(last)
    } else {
        index.saturating_sub(1)
    };
}

/// Insert a character into the focused text field.
fn insert_form_char(form: &mut NewPrFormState, character: char) {
    match form.focus {
        NewPrFormFocus::Title => {
            form.title_cursor = insert_char_at(&mut form.title_text, form.title_cursor, character);
        }
        NewPrFormFocus::Body => {
            form.body_cursor = insert_char_at(&mut form.body_text, form.body_cursor, character);
        }
        NewPrFormFocus::Head | NewPrFormFocus::Base => {}
    }
}

/// The text edits the composer supports beyond insertion.
#[derive(Clone, Copy)]
enum TextEdit {
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
}

/// Apply a text edit to whichever text field has focus.
fn edit_focused_text(form: &mut NewPrFormState, edit: TextEdit) {
    let (text, cursor) = match form.focus {
        NewPrFormFocus::Title => (&mut form.title_text, &mut form.title_cursor),
        NewPrFormFocus::Body => (&mut form.body_text, &mut form.body_cursor),
        NewPrFormFocus::Head | NewPrFormFocus::Base => return,
    };
    let length = text.chars().count();
    match edit {
        TextEdit::Backspace => *cursor = delete_char_before(text, *cursor),
        TextEdit::Delete => delete_char_at(text, *cursor),
        TextEdit::CursorLeft => *cursor = cursor.saturating_sub(1),
        TextEdit::CursorRight => *cursor = (*cursor + 1).min(length),
        TextEdit::CursorHome => *cursor = 0,
        TextEdit::CursorEnd => *cursor = length,
    }
}
