//! Issues-mode property-editor state operations (issue #175).
//!
//! Mirrors `prs_merge_ops::apply_pr_merge_event`. Owns the property-editor
//! overlay transitions (open/navigate/toggle/confirm/cancel/title-edit) and
//! the edit-result lifecycle (Succeeded/Failed/OptionsLoaded/OptionsFailed).

use super::{
    AppEvent, AppState, IssueFocus, IssuePropertyEditorState, IssuePropertyKind,
    PROPERTY_CLEAR_LABEL, PropertyMutationPending, PropertyOption,
};

impl AppState {
    /// Apply an issue property-editor event (returns handled).
    pub(super) fn apply_issue_property_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::IssueOpenPropertyEditor { .. }
            | AppEvent::IssuePropertyEditorNavigateUp
            | AppEvent::IssuePropertyEditorNavigateDown
            | AppEvent::IssuePropertyEditorToggle
            | AppEvent::IssuePropertyEditorConfirm
            | AppEvent::IssuePropertyEditorCancel
            | AppEvent::IssuePropertyEditorTitleChar(_)
            | AppEvent::IssuePropertyEditorTitleBackspace
            | AppEvent::IssuePropertyEditorTitleDelete
            | AppEvent::IssuePropertyEditorTitleCursorLeft
            | AppEvent::IssuePropertyEditorTitleCursorRight => {
                self.apply_issue_property_editor_ui(event)
            }
            AppEvent::IssuePropertyEditorOptionsLoaded { .. }
            | AppEvent::IssuePropertyEditorOptionsFailed { .. }
            | AppEvent::IssuePropertyEditSucceeded { .. }
            | AppEvent::IssuePropertyEditFailed { .. } => {
                self.apply_issue_property_lifecycle(event)
            }
            AppEvent::IssuePropertyEditorValidationError { .. } => {
                self.apply_issue_property_validation_error(event)
            }
            AppEvent::IssuePostMutationRefreshStarted => {
                self.issues_state.post_mutation_refresh.started();
                true
            }
            _ => false,
        }
    }

    /// Editor UI events: open, navigate, toggle, cancel, and title editing.
    fn apply_issue_property_editor_ui(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::IssueOpenPropertyEditor { kind } => {
                self.open_issue_property_editor(*kind);
                true
            }
            AppEvent::IssuePropertyEditorNavigateUp => {
                self.navigate_issue_property_editor(false);
                true
            }
            AppEvent::IssuePropertyEditorNavigateDown => {
                self.navigate_issue_property_editor(true);
                true
            }
            AppEvent::IssuePropertyEditorToggle => {
                self.toggle_issue_property_editor();
                true
            }
            AppEvent::IssuePropertyEditorConfirm => true,
            AppEvent::IssuePropertyEditorCancel => {
                self.cancel_issue_property_editor();
                true
            }
            AppEvent::IssuePropertyEditorTitleChar(c) => {
                self.issue_property_title_char(*c);
                true
            }
            AppEvent::IssuePropertyEditorTitleBackspace => {
                self.issue_property_title_backspace();
                true
            }
            AppEvent::IssuePropertyEditorTitleDelete => {
                self.issue_property_title_delete();
                true
            }
            AppEvent::IssuePropertyEditorTitleCursorLeft => {
                self.issue_property_title_cursor_left();
                true
            }
            AppEvent::IssuePropertyEditorTitleCursorRight => {
                self.issue_property_title_cursor_right();
                true
            }
            _ => false,
        }
    }

    /// Lifecycle events: options loaded/failed, edit succeeded/failed.
    fn apply_issue_property_lifecycle(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::IssuePropertyEditorOptionsLoaded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                options,
            } => self.apply_issue_property_options_loaded(
                scope_repo_id,
                *issue_number,
                *kind,
                *request_id,
                options,
            ),
            AppEvent::IssuePropertyEditorOptionsFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => self.apply_issue_property_options_failed(
                scope_repo_id,
                *issue_number,
                *kind,
                *request_id,
                error,
            ),
            AppEvent::IssuePropertyEditSucceeded {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
            } => self.apply_issue_property_succeeded(
                scope_repo_id,
                *issue_number,
                *kind,
                *request_id,
            ),
            AppEvent::IssuePropertyEditFailed {
                scope_repo_id,
                issue_number,
                kind,
                request_id,
                error,
            } => self.apply_issue_property_failed(
                scope_repo_id,
                *issue_number,
                *kind,
                *request_id,
                error,
            ),
            _ => false,
        }
    }

    fn open_issue_property_editor(&mut self, kind: IssuePropertyKind) {
        if self.issues_state.issue_focus != IssueFocus::IssueDetail
            || self.issues_state.inline_state != super::InlineState::None
            || self.issues_state.agent_chooser.is_some()
            || self.issues_state.property_editor.is_some()
            || self.issues_state.close_reason_chooser.is_some()
            || self.issues_state.delete_confirm.is_some()
            || self.issues_state.mutation_pending.is_some()
            || self.issues_state.property_mutation_pending.is_some()
        {
            return;
        }
        if self.issues_state.issue_detail.is_none() {
            return;
        }
        let (title_text, options, baseline) = self.issue_property_initial_state(kind);
        // Title editor opens with the caret at the start (issue #175 H1).
        let title_cursor = 0;
        let load_request_id = self.issues_state.next_property_request_id;
        self.issues_state.next_property_request_id += 1;
        // F2: initialize cursor to the currently-selected option for
        // single-select kinds (State, Milestone, Type).
        let needs_load = needs_issue_background_options(kind);
        let selected_index = initial_selected_index(kind, &options);
        self.issues_state.property_editor = Some(IssuePropertyEditorState {
            kind,
            options,
            selected_index,
            title_text,
            title_cursor,
            error: None,
            baseline,
            loading_failed: false,
            options_loading: needs_load,
            load_request_id,
        });
    }

    fn issue_property_initial_state(
        &self,
        kind: IssuePropertyKind,
    ) -> (String, Vec<PropertyOption>, Vec<String>) {
        let Some(detail) = &self.issues_state.issue_detail else {
            return (String::new(), Vec::new(), Vec::new());
        };
        let selected_opts = |items: &[String]| -> Vec<PropertyOption> {
            items
                .iter()
                .map(|l| PropertyOption {
                    label: l.clone(),
                    selected: true,
                    id: None,
                })
                .collect()
        };
        let one_selected = |label: &str| {
            vec![PropertyOption {
                label: label.to_string(),
                selected: true,
                id: None,
            }]
        };
        match kind {
            IssuePropertyKind::Labels => {
                let opts = selected_opts(&detail.labels);
                (String::new(), opts, detail.labels.clone())
            }
            IssuePropertyKind::Assignees => {
                let opts = selected_opts(&detail.assignees);
                (String::new(), opts, detail.assignees.clone())
            }
            IssuePropertyKind::Milestone => {
                let ms: Vec<String> = detail.milestone.clone().into_iter().collect();
                let opts = selected_opts(&ms);
                (String::new(), opts, ms)
            }
            IssuePropertyKind::Title => (detail.title.clone(), Vec::new(), Vec::new()),
            IssuePropertyKind::Type => match &detail.issue_type_name {
                Some(t) => (String::new(), one_selected(t), Vec::new()),
                None => (String::new(), Vec::new(), Vec::new()),
            },
            IssuePropertyKind::State => {
                let is_open = detail.state == crate::domain::IssueState::Open;
                (
                    String::new(),
                    vec![
                        PropertyOption {
                            label: "Open".to_string(),
                            selected: is_open,
                            id: None,
                        },
                        PropertyOption {
                            label: "Closed".to_string(),
                            selected: !is_open,
                            id: None,
                        },
                    ],
                    Vec::new(),
                )
            }
        }
    }

    fn navigate_issue_property_editor(&mut self, forward: bool) {
        let Some(editor) = &mut self.issues_state.property_editor else {
            return;
        };
        if editor.options.is_empty() {
            return;
        }
        let len = editor.options.len();
        let current = editor.selected_index;
        editor.selected_index = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
    }

    fn toggle_issue_property_editor(&mut self) {
        let Some(editor) = &mut self.issues_state.property_editor else {
            return;
        };
        match editor.kind {
            IssuePropertyKind::Labels | IssuePropertyKind::Assignees => {
                if let Some(opt) = editor.options.get_mut(editor.selected_index) {
                    opt.selected = !opt.selected;
                }
            }
            IssuePropertyKind::Milestone | IssuePropertyKind::Type | IssuePropertyKind::State => {
                for (i, opt) in editor.options.iter_mut().enumerate() {
                    opt.selected = i == editor.selected_index;
                }
                if editor.options.is_empty() {
                    editor.options.push(PropertyOption {
                        label: PROPERTY_CLEAR_LABEL.to_string(),
                        selected: true,
                        id: None,
                    });
                }
            }
            IssuePropertyKind::Title => {}
        }
    }

    fn cancel_issue_property_editor(&mut self) {
        self.issues_state.property_editor = None;
        // H4/M11: do NOT clear property_mutation_pending here. The in-flight
        // mutation may still fail after the user closes the editor; leaving
        // the pending token lets the late failure be correlated and surfaced
        // as a scoped warning. A subsequent confirm is allowed to overwrite a
        // stale pending (editor is closed) — see mark_issue_property_mutation_pending.
    }

    // ── Title editing (H1) ──────────────────────────────────────────────

    fn issue_property_title_char(&mut self, c: char) {
        if let Some(editor) = &mut self.issues_state.property_editor
            && editor.kind == IssuePropertyKind::Title
        {
            editor.title_text.insert(editor.title_cursor, c);
            editor.title_cursor += c.len_utf8();
        }
    }

    fn issue_property_title_backspace(&mut self) {
        if let Some(editor) = &mut self.issues_state.property_editor
            && editor.kind == IssuePropertyKind::Title
            && editor.title_cursor > 0
        {
            let prev = editor.title_text[..editor.title_cursor]
                .chars()
                .last()
                .map_or(0, char::len_utf8);
            editor
                .title_text
                .drain((editor.title_cursor - prev)..editor.title_cursor);
            editor.title_cursor -= prev;
        }
    }

    fn issue_property_title_delete(&mut self) {
        if let Some(editor) = &mut self.issues_state.property_editor
            && editor.kind == IssuePropertyKind::Title
            && editor.title_cursor < editor.title_text.len()
        {
            let next = editor.title_text[editor.title_cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            editor
                .title_text
                .drain(editor.title_cursor..(editor.title_cursor + next));
        }
    }

    fn issue_property_title_cursor_left(&mut self) {
        if let Some(editor) = &mut self.issues_state.property_editor
            && editor.kind == IssuePropertyKind::Title
            && editor.title_cursor > 0
        {
            let prev = editor.title_text[..editor.title_cursor]
                .chars()
                .last()
                .map_or(0, char::len_utf8);
            editor.title_cursor -= prev;
        }
    }

    fn issue_property_title_cursor_right(&mut self) {
        if let Some(editor) = &mut self.issues_state.property_editor
            && editor.kind == IssuePropertyKind::Title
            && editor.title_cursor < editor.title_text.len()
        {
            let next = editor.title_text[editor.title_cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            editor.title_cursor += next;
        }
    }

    // ── Options loaded/failed (H5, M6) ──────────────────────────────────

    fn issue_property_scope_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return false;
        }
        self.issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|d| d.number == issue_number)
    }

    fn apply_issue_property_options_loaded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        kind: IssuePropertyKind,
        request_id: u64,
        options: &[(Option<String>, String, bool)],
    ) -> bool {
        if !self.issue_property_scope_matches(scope_repo_id, issue_number) {
            return true;
        }
        let Some(editor) = &mut self.issues_state.property_editor else {
            return true;
        };
        if editor.kind != kind || editor.load_request_id != request_id {
            return true;
        }
        editor.loading_failed = false;
        match kind {
            IssuePropertyKind::Labels | IssuePropertyKind::Assignees => {
                Self::apply_issue_property_multi_select_options(editor, options);
            }
            IssuePropertyKind::Milestone | IssuePropertyKind::Type => {
                Self::apply_issue_property_single_select_options(editor, options);
            }
            IssuePropertyKind::Title | IssuePropertyKind::State => {}
        }
        true
    }

    /// Multi-select options (labels, assignees): preserve baseline selections.
    fn apply_issue_property_multi_select_options(
        editor: &mut IssuePropertyEditorState,
        options: &[(Option<String>, String, bool)],
    ) {
        let current_selected: Vec<String> = editor.baseline.clone();
        // M6/M1: preserve the user's explicit toggle intent (both selections
        // and deselections) from the pre-load editor state. A baseline label
        // the user deselected must remain deselected after options load.
        let prior_state: Vec<(String, bool)> = editor
            .options
            .iter()
            .map(|o| (o.label.clone(), o.selected))
            .collect();
        editor.options = options
            .iter()
            .map(|(id, label, _)| {
                // If the user already toggled this option, honor that intent.
                let prior_toggled = prior_state
                    .iter()
                    .find(|(l, _)| l.eq_ignore_ascii_case(label))
                    .map(|(_, s)| *s);
                let from_baseline = current_selected
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(label));
                // Explicit user intent wins over baseline restore.
                let selected = prior_toggled.unwrap_or(from_baseline);
                PropertyOption {
                    label: label.clone(),
                    selected,
                    id: id.clone(),
                }
            })
            .collect();
        // M10: ensure currently-applied values are present even if
        // not in the first page of results.
        for baseline_label in &current_selected {
            if !editor
                .options
                .iter()
                .any(|o| o.label.eq_ignore_ascii_case(baseline_label))
            {
                editor.options.push(PropertyOption {
                    label: baseline_label.clone(),
                    selected: true,
                    id: None,
                });
            }
        }
        // M6: preserve toggles that are not in the fetched option set and not
        // already covered by the baseline-restore loop above.
        for (toggle_label, toggle_selected) in &prior_state {
            if !editor
                .options
                .iter()
                .any(|o| o.label.eq_ignore_ascii_case(toggle_label))
            {
                editor.options.push(PropertyOption {
                    label: toggle_label.clone(),
                    selected: *toggle_selected,
                    id: None,
                });
            }
        }
        if editor.selected_index >= editor.options.len() {
            editor.selected_index = 0;
        }
        editor.options_loading = false;
    }

    /// Single-select options (milestone, type): preserve current selection,
    /// add "(clear)" option.
    fn apply_issue_property_single_select_options(
        editor: &mut IssuePropertyEditorState,
        options: &[(Option<String>, String, bool)],
    ) {
        let current = editor
            .options
            .iter()
            .find(|o| o.selected)
            .map(|o| o.label.clone());
        let mut new_opts: Vec<PropertyOption> = options
            .iter()
            .map(|(id, label, _)| PropertyOption {
                label: label.clone(),
                selected: current
                    .as_ref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(label)),
                id: id.clone(),
            })
            .collect();
        // M10: ensure currently-applied milestone/type is present.
        if let Some(ref c) = current
            && !new_opts.iter().any(|o| o.label.eq_ignore_ascii_case(c))
        {
            new_opts.push(PropertyOption {
                label: c.clone(),
                selected: true,
                id: None,
            });
        }
        new_opts.push(PropertyOption {
            label: PROPERTY_CLEAR_LABEL.to_string(),
            selected: current.is_none(),
            id: None,
        });
        editor.options = new_opts;
        // F2: initialize cursor to the currently-selected option, not 0.
        editor.selected_index = editor.options.iter().position(|o| o.selected).unwrap_or(0);
        editor.options_loading = false;
    }

    fn apply_issue_property_options_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        kind: IssuePropertyKind,
        request_id: u64,
        error: &str,
    ) -> bool {
        if !self.issue_property_scope_matches(scope_repo_id, issue_number) {
            return true;
        }
        let Some(editor) = &mut self.issues_state.property_editor else {
            return true;
        };
        if editor.kind != kind || editor.load_request_id != request_id {
            return true;
        }
        // H5: do NOT replace options with empty — keep existing intact.
        editor.loading_failed = true;
        editor.options_loading = false;
        editor.error = Some(error.to_string());
        true
    }

    fn apply_issue_property_succeeded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        kind: IssuePropertyKind,
        request_id: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        // H4: only apply completion if request_id+kind+number match pending.
        let pending_matches = self
            .issues_state
            .property_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.request_id == request_id
                    && p.number == issue_number
                    && p.scope_repo_id == *scope_repo_id
            });
        if !pending_matches {
            return true;
        }
        // Clear pending and queue one refresh; orchestration starts it once
        // list and detail request channels are both idle.
        self.issues_state.property_mutation_pending = None;
        self.issues_state.post_mutation_refresh.request();
        if self
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|d| d.number == issue_number)
        {
            self.issues_state.property_editor = None;
        }
        // Suppress unused warning for kind parameter while keeping the
        // signature aligned with the event.
        let _ = kind;
        true
    }

    fn apply_issue_property_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        kind: IssuePropertyKind,
        request_id: u64,
        error: &str,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        let pending_matches = self
            .issues_state
            .property_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.request_id == request_id
                    && p.number == issue_number
                    && p.scope_repo_id == *scope_repo_id
            });
        if !pending_matches {
            return true;
        }
        // H4: clear pending, keep editor open with error so user can retry.
        self.issues_state.property_mutation_pending = None;
        if self
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|d| d.number == issue_number)
            && self
                .issues_state
                .property_editor
                .as_ref()
                .is_some_and(|e| e.kind == kind)
        {
            if let Some(editor) = &mut self.issues_state.property_editor {
                editor.error = Some(error.to_string());
            }
        } else {
            // M11: editor no longer active — surface as a scoped warning.
            let kind_str = issue_property_kind_label(kind);
            self.issues_state.draft_notice = Some(format!(
                "Failed to edit {kind_str} on issue #{issue_number}: {error}"
            ));
        }
        true
    }

    /// Apply a synchronous validation error (empty title, missing repo) by
    /// setting the open editor's error directly, WITHOUT mutation correlation
    /// (issue #175 F5). No scope/request_id check — this is a deterministic
    /// pre-flight validation applied to whatever editor is currently open.
    fn apply_issue_property_validation_error(&mut self, event: &AppEvent) -> bool {
        let AppEvent::IssuePropertyEditorValidationError { kind, error } = event else {
            return false;
        };
        if self
            .issues_state
            .property_editor
            .as_ref()
            .is_some_and(|e| e.kind == *kind)
        {
            if let Some(editor) = &mut self.issues_state.property_editor {
                editor.error = Some(error.clone());
            }
        }
        true
    }

    /// Allocate a property mutation request ID and mark it as pending (H4).
    /// Called by the dispatch layer on confirm.
    pub fn mark_issue_property_mutation_pending(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
    ) -> Option<u64> {
        if self.issues_state.property_mutation_pending.is_some() {
            return None;
        }
        let request_id = self.issues_state.next_property_request_id;
        self.issues_state.next_property_request_id += 1;
        self.issues_state.property_mutation_pending = Some(PropertyMutationPending {
            scope_repo_id,
            request_id,
            number: issue_number,
        });
        Some(request_id)
    }
    /// Whether a successful issue mutation's coalesced refresh can start now.
    #[must_use]
    pub fn issue_post_mutation_refresh_ready(&self) -> bool {
        self.issues_state.post_mutation_refresh.is_ready(
            self.issues_state.list_pending(),
            self.issues_state.detail_pending.is_some(),
        )
    }
}

/// Whether an issue property kind requires a background fetch of options.
fn needs_issue_background_options(kind: IssuePropertyKind) -> bool {
    matches!(
        kind,
        IssuePropertyKind::Labels
            | IssuePropertyKind::Assignees
            | IssuePropertyKind::Milestone
            | IssuePropertyKind::Type
    )
}

/// F2: Initialize `selected_index` to the position of the currently-selected
/// option for single-select kinds. For multi-select and title, returns 0.
fn initial_selected_index(kind: IssuePropertyKind, options: &[PropertyOption]) -> usize {
    match kind {
        IssuePropertyKind::Milestone | IssuePropertyKind::Type | IssuePropertyKind::State => {
            options.iter().position(|o| o.selected).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Human-readable label for a property kind (used in warning messages).
fn issue_property_kind_label(kind: IssuePropertyKind) -> &'static str {
    match kind {
        IssuePropertyKind::Labels => "labels",
        IssuePropertyKind::Assignees => "assignees",
        IssuePropertyKind::Milestone => "milestone",
        IssuePropertyKind::Title => "title",
        IssuePropertyKind::Type => "type",
        IssuePropertyKind::State => "state",
    }
}

#[cfg(test)]
#[path = "issues_property_ops_tests.rs"]
mod tests;
