//! PR-mode property-editor state operations (issue #175).
//!
//! Mirrors `issues_property_ops::apply_issue_property_event` and
//! `prs_merge_ops::apply_pr_merge_event`. Owns the property-editor overlay
//! transitions (open/navigate/toggle/confirm/cancel/title-edit) and the
//! edit-result lifecycle (Succeeded/Failed/OptionsLoaded/OptionsFailed).

use super::{
    AppEvent, AppState, InlineState, PrFocus, PrPropertyEditorState, PrPropertyKind,
    PropertyMutationPending, PropertyOption,
};

impl AppState {
    /// Apply a PR property-editor event (returns handled).
    pub(super) fn apply_pr_property_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenPropertyEditor { .. }
            | AppEvent::PrPropertyEditorNavigateUp
            | AppEvent::PrPropertyEditorNavigateDown
            | AppEvent::PrPropertyEditorToggle
            | AppEvent::PrPropertyEditorConfirm
            | AppEvent::PrPropertyEditorCancel
            | AppEvent::PrPropertyEditorTitleChar(_)
            | AppEvent::PrPropertyEditorTitleBackspace
            | AppEvent::PrPropertyEditorTitleDelete
            | AppEvent::PrPropertyEditorTitleCursorLeft
            | AppEvent::PrPropertyEditorTitleCursorRight => self.apply_pr_property_editor_ui(event),
            AppEvent::PrPropertyEditorOptionsLoaded { .. }
            | AppEvent::PrPropertyEditorOptionsFailed { .. }
            | AppEvent::PrPropertyEditSucceeded { .. }
            | AppEvent::PrPropertyEditFailed { .. } => self.apply_pr_property_lifecycle(event),
            AppEvent::PrPropertyEditorValidationError { .. } => {
                self.apply_pr_property_validation_error(event)
            }
            _ => false,
        }
    }

    /// Editor UI events: open, navigate, toggle, cancel, and title editing.
    fn apply_pr_property_editor_ui(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenPropertyEditor { kind } => {
                self.open_pr_property_editor(*kind);
                true
            }
            AppEvent::PrPropertyEditorNavigateUp => {
                self.navigate_pr_property_editor(false);
                true
            }
            AppEvent::PrPropertyEditorNavigateDown => {
                self.navigate_pr_property_editor(true);
                true
            }
            AppEvent::PrPropertyEditorToggle => {
                self.toggle_pr_property_editor();
                true
            }
            AppEvent::PrPropertyEditorConfirm => true,
            AppEvent::PrPropertyEditorCancel => {
                self.cancel_pr_property_editor();
                true
            }
            AppEvent::PrPropertyEditorTitleChar(c) => {
                self.pr_property_title_char(*c);
                true
            }
            AppEvent::PrPropertyEditorTitleBackspace => {
                self.pr_property_title_backspace();
                true
            }
            AppEvent::PrPropertyEditorTitleDelete => {
                self.pr_property_title_delete();
                true
            }
            AppEvent::PrPropertyEditorTitleCursorLeft => {
                self.pr_property_title_cursor_left();
                true
            }
            AppEvent::PrPropertyEditorTitleCursorRight => {
                self.pr_property_title_cursor_right();
                true
            }
            _ => false,
        }
    }

    /// Lifecycle events: options loaded/failed, edit succeeded/failed.
    fn apply_pr_property_lifecycle(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrPropertyEditorOptionsLoaded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                options,
            } => self.apply_pr_property_options_loaded(
                scope_repo_id,
                *pr_number,
                *kind,
                *request_id,
                options,
            ),
            AppEvent::PrPropertyEditorOptionsFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => self.apply_pr_property_options_failed(
                scope_repo_id,
                *pr_number,
                *kind,
                *request_id,
                error,
            ),
            AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
            } => self.apply_pr_property_succeeded(scope_repo_id, *pr_number, *kind, *request_id),
            AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                kind,
                request_id,
                error,
            } => {
                self.apply_pr_property_failed(scope_repo_id, *pr_number, *kind, *request_id, error)
            }
            _ => false,
        }
    }

    fn open_pr_property_editor(&mut self, kind: PrPropertyKind) {
        if self.prs_state.pr_focus != PrFocus::PrDetail
            || self.prs_state.inline_state != InlineState::None
            || self.prs_state.agent_chooser.is_some()
            || self.prs_state.merge_chooser.is_some()
            || self.prs_state.property_editor.is_some()
            || self.prs_state.mutation_pending.is_some()
            || self.prs_state.property_mutation_pending.is_some()
        {
            return;
        }
        if self.prs_state.pr_detail.is_none() {
            return;
        }
        let (title_text, options, baseline) = self.pr_property_initial_state(kind);
        // Title editor opens with the caret at the start (issue #175 H1).
        let title_cursor = 0;
        let load_request_id = self.prs_state.next_property_request_id;
        self.prs_state.next_property_request_id += 1;
        let needs_load = needs_pr_background_options(kind);
        // F2: initialize cursor to the currently-selected option for
        // single-select kinds (State, Milestone).
        let selected_index = initial_pr_selected_index(kind, &options);
        self.prs_state.property_editor = Some(PrPropertyEditorState {
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

    fn pr_property_initial_state(
        &self,
        kind: PrPropertyKind,
    ) -> (String, Vec<PropertyOption>, Vec<String>) {
        let Some(detail) = &self.prs_state.pr_detail else {
            return (String::new(), Vec::new(), Vec::new());
        };
        match kind {
            PrPropertyKind::Labels => {
                let opts = detail
                    .labels
                    .iter()
                    .map(|l| PropertyOption {
                        label: l.clone(),
                        selected: true,
                        id: None,
                    })
                    .collect();
                (String::new(), opts, detail.labels.clone())
            }
            PrPropertyKind::Assignees => {
                let opts = detail
                    .assignees
                    .iter()
                    .map(|a| PropertyOption {
                        label: a.clone(),
                        selected: true,
                        id: None,
                    })
                    .collect();
                (String::new(), opts, detail.assignees.clone())
            }
            PrPropertyKind::Milestone => {
                let opts = detail
                    .milestone
                    .iter()
                    .map(|m| PropertyOption {
                        label: m.clone(),
                        selected: true,
                        id: None,
                    })
                    .collect();
                let baseline = detail.milestone.clone().into_iter().collect();
                (String::new(), opts, baseline)
            }
            PrPropertyKind::Title => (detail.title.clone(), Vec::new(), Vec::new()),
            PrPropertyKind::State => {
                let is_open = detail.state == crate::domain::PrState::Open;
                let opts = vec![
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
                ];
                (String::new(), opts, Vec::new())
            }
        }
    }

    fn navigate_pr_property_editor(&mut self, forward: bool) {
        let Some(editor) = &mut self.prs_state.property_editor else {
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

    fn toggle_pr_property_editor(&mut self) {
        let Some(editor) = &mut self.prs_state.property_editor else {
            return;
        };
        match editor.kind {
            PrPropertyKind::Labels | PrPropertyKind::Assignees => {
                if let Some(opt) = editor.options.get_mut(editor.selected_index) {
                    opt.selected = !opt.selected;
                }
            }
            PrPropertyKind::Milestone | PrPropertyKind::State => {
                for (i, opt) in editor.options.iter_mut().enumerate() {
                    opt.selected = i == editor.selected_index;
                }
                if editor.options.is_empty() {
                    editor.options.push(PropertyOption {
                        label: "(clear)".to_string(),
                        selected: true,
                        id: None,
                    });
                }
            }
            PrPropertyKind::Title => {}
        }
    }

    fn cancel_pr_property_editor(&mut self) {
        self.prs_state.property_editor = None;
        // H4/M11: do NOT clear property_mutation_pending here. The in-flight
        // mutation may still fail after the user closes the editor; leaving
        // the pending token lets the late failure be correlated and surfaced
        // as a scoped warning. A subsequent confirm is allowed to overwrite a
        // stale pending (editor is closed).
    }

    // ── Title editing (H1) ──────────────────────────────────────────────

    fn pr_property_title_char(&mut self, c: char) {
        if let Some(editor) = &mut self.prs_state.property_editor
            && editor.kind == PrPropertyKind::Title
        {
            editor.title_text.insert(editor.title_cursor, c);
            editor.title_cursor += c.len_utf8();
        }
    }

    fn pr_property_title_backspace(&mut self) {
        if let Some(editor) = &mut self.prs_state.property_editor
            && editor.kind == PrPropertyKind::Title
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

    fn pr_property_title_delete(&mut self) {
        if let Some(editor) = &mut self.prs_state.property_editor
            && editor.kind == PrPropertyKind::Title
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

    fn pr_property_title_cursor_left(&mut self) {
        if let Some(editor) = &mut self.prs_state.property_editor
            && editor.kind == PrPropertyKind::Title
            && editor.title_cursor > 0
        {
            let prev = editor.title_text[..editor.title_cursor]
                .chars()
                .last()
                .map_or(0, char::len_utf8);
            editor.title_cursor -= prev;
        }
    }

    fn pr_property_title_cursor_right(&mut self) {
        if let Some(editor) = &mut self.prs_state.property_editor
            && editor.kind == PrPropertyKind::Title
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

    fn pr_property_scope_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return false;
        }
        self.prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.number == pr_number)
    }

    fn apply_pr_property_options_loaded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        kind: PrPropertyKind,
        request_id: u64,
        options: &[(Option<String>, String, bool)],
    ) -> bool {
        if !self.pr_property_scope_matches(scope_repo_id, pr_number) {
            return true;
        }
        let Some(editor) = &mut self.prs_state.property_editor else {
            return true;
        };
        if editor.kind != kind || editor.load_request_id != request_id {
            return true;
        }
        editor.loading_failed = false;
        match kind {
            PrPropertyKind::Labels | PrPropertyKind::Assignees => {
                Self::apply_pr_property_multi_select_options(editor, options);
            }
            PrPropertyKind::Milestone => {
                Self::apply_pr_property_single_select_options(editor, options);
            }
            PrPropertyKind::Title | PrPropertyKind::State => {}
        }
        true
    }

    /// Multi-select options (labels, assignees): preserve baseline selections.
    fn apply_pr_property_multi_select_options(
        editor: &mut PrPropertyEditorState,
        options: &[(Option<String>, String, bool)],
    ) {
        let current_selected: Vec<String> = editor.baseline.clone();
        // M6: preserve user toggles from the current editor state.
        let prior_selections: Vec<String> = editor
            .options
            .iter()
            .filter(|o| o.selected)
            .map(|o| o.label.clone())
            .collect();
        editor.options = options
            .iter()
            .map(|(id, label, _)| {
                let from_baseline = current_selected
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(label));
                let from_toggle = prior_selections
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(label));
                PropertyOption {
                    label: label.clone(),
                    selected: from_toggle || from_baseline,
                    id: id.clone(),
                }
            })
            .collect();
        // M10: preserve currently-applied values not in first page.
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
        // M6: preserve toggles that are not in the fetched option set.
        for toggle_label in &prior_selections {
            if !editor
                .options
                .iter()
                .any(|o| o.label.eq_ignore_ascii_case(toggle_label))
            {
                editor.options.push(PropertyOption {
                    label: toggle_label.clone(),
                    selected: true,
                    id: None,
                });
            }
        }
        if editor.selected_index >= editor.options.len() {
            editor.selected_index = 0;
        }
        editor.options_loading = false;
    }

    /// Single-select options (milestone): preserve current selection,
    /// add "(clear)" option.
    fn apply_pr_property_single_select_options(
        editor: &mut PrPropertyEditorState,
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
            label: "(clear)".to_string(),
            selected: current.is_none(),
            id: None,
        });
        editor.options = new_opts;
        // F2: initialize cursor to the currently-selected option, not 0.
        editor.selected_index = editor.options.iter().position(|o| o.selected).unwrap_or(0);
        editor.options_loading = false;
    }

    fn apply_pr_property_options_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        kind: PrPropertyKind,
        request_id: u64,
        error: &str,
    ) -> bool {
        if !self.pr_property_scope_matches(scope_repo_id, pr_number) {
            return true;
        }
        let Some(editor) = &mut self.prs_state.property_editor else {
            return true;
        };
        if editor.kind != kind || editor.load_request_id != request_id {
            return true;
        }
        editor.loading_failed = true;
        editor.options_loading = false;
        editor.error = Some(error.to_string());
        true
    }

    fn apply_pr_property_succeeded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        kind: PrPropertyKind,
        request_id: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        let pending_matches = self
            .prs_state
            .property_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.request_id == request_id
                    && p.number == pr_number
                    && p.scope_repo_id == *scope_repo_id
            });
        if !pending_matches {
            return true;
        }
        self.prs_state.property_mutation_pending = None;
        if self
            .prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.number == pr_number)
        {
            self.prs_state.property_editor = None;
        }
        let _ = kind;
        true
    }

    fn apply_pr_property_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        kind: PrPropertyKind,
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
            .prs_state
            .property_mutation_pending
            .as_ref()
            .is_some_and(|p| {
                p.request_id == request_id
                    && p.number == pr_number
                    && p.scope_repo_id == *scope_repo_id
            });
        if !pending_matches {
            return true;
        }
        self.prs_state.property_mutation_pending = None;
        if self
            .prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.number == pr_number)
            && self
                .prs_state
                .property_editor
                .as_ref()
                .is_some_and(|e| e.kind == kind)
        {
            if let Some(editor) = &mut self.prs_state.property_editor {
                editor.error = Some(error.to_string());
            }
        } else {
            let kind_str = pr_property_kind_label(kind);
            self.prs_state.draft_notice = Some(format!(
                "Failed to edit {kind_str} on PR #{pr_number}: {error}"
            ));
        }
        true
    }

    /// Apply a synchronous validation error (empty title, missing repo) by
    /// setting the open PR editor's error directly, WITHOUT mutation
    /// correlation (issue #175 F5).
    fn apply_pr_property_validation_error(&mut self, event: &AppEvent) -> bool {
        let AppEvent::PrPropertyEditorValidationError { kind, error } = event else {
            return false;
        };
        if self
            .prs_state
            .property_editor
            .as_ref()
            .is_some_and(|e| e.kind == *kind)
        {
            if let Some(editor) = &mut self.prs_state.property_editor {
                editor.error = Some(error.clone());
            }
        }
        true
    }

    /// Allocate a property mutation request ID and mark it as pending (H4).
    pub fn mark_pr_property_mutation_pending(
        &mut self,
        scope_repo_id: crate::domain::RepositoryId,
        pr_number: u64,
    ) -> Option<u64> {
        if self.prs_state.property_mutation_pending.is_some() {
            return None;
        }
        let request_id = self.prs_state.next_property_request_id;
        self.prs_state.next_property_request_id += 1;
        self.prs_state.property_mutation_pending = Some(PropertyMutationPending {
            scope_repo_id,
            request_id,
            number: pr_number,
        });
        Some(request_id)
    }
}

/// Whether a PR property kind requires a background fetch of options.
fn needs_pr_background_options(kind: PrPropertyKind) -> bool {
    matches!(
        kind,
        PrPropertyKind::Labels | PrPropertyKind::Assignees | PrPropertyKind::Milestone
    )
}

/// F2: Initialize `selected_index` to the position of the currently-selected
/// option for single-select kinds (State, Milestone).
fn initial_pr_selected_index(kind: PrPropertyKind, options: &[PropertyOption]) -> usize {
    match kind {
        PrPropertyKind::Milestone | PrPropertyKind::State => {
            options.iter().position(|o| o.selected).unwrap_or(0)
        }
        _ => 0,
    }
}

fn pr_property_kind_label(kind: PrPropertyKind) -> &'static str {
    match kind {
        PrPropertyKind::Labels => "labels",
        PrPropertyKind::Assignees => "assignees",
        PrPropertyKind::Milestone => "milestone",
        PrPropertyKind::Title => "title",
        PrPropertyKind::State => "state",
    }
}

#[cfg(test)]
#[path = "prs_property_ops_tests.rs"]
mod tests;
