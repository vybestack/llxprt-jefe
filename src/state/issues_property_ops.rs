//! Issues-mode property-editor state operations (issue #175).
//!
//! Mirrors `prs_merge_ops::apply_pr_merge_event`. Owns the property-editor
//! overlay transitions (open/navigate/toggle/confirm/cancel) and the
//! edit-result lifecycle (Succeeded/Failed/OptionsLoaded).

use super::{
    AppEvent, AppState, IssueFocus, IssuePropertyEditorState, IssuePropertyKind, PropertyOption,
};

impl AppState {
    /// Apply an issue property-editor event (returns handled).
    pub(super) fn apply_issue_property_event(&mut self, event: &AppEvent) -> bool {
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
                self.issues_state.property_editor = None;
                true
            }
            AppEvent::IssuePropertyEditorOptionsLoaded { options } => {
                self.apply_issue_property_options_loaded(options);
                true
            }
            AppEvent::IssuePropertyEditSucceeded {
                scope_repo_id,
                issue_number,
            } => self.apply_issue_property_succeeded(scope_repo_id, *issue_number),
            AppEvent::IssuePropertyEditFailed {
                scope_repo_id,
                issue_number,
                error,
            } => self.apply_issue_property_failed(scope_repo_id, *issue_number, error),
            _ => false,
        }
    }

    fn open_issue_property_editor(&mut self, kind: IssuePropertyKind) {
        if self.issues_state.issue_focus != IssueFocus::IssueDetail
            || self.issues_state.inline_state != super::InlineState::None
            || self.issues_state.agent_chooser.is_some()
            || self.issues_state.property_editor.is_some()
            || self.issues_state.mutation_pending.is_some()
        {
            return;
        }
        if self.issues_state.issue_detail.is_none() {
            return;
        }
        let (title_text, options) = self.issue_property_initial_state(kind);
        let title_cursor = title_text.len();
        self.issues_state.property_editor = Some(IssuePropertyEditorState {
            kind,
            options,
            selected_index: 0,
            title_text,
            title_cursor,
            error: None,
        });
    }

    fn issue_property_initial_state(
        &self,
        kind: IssuePropertyKind,
    ) -> (String, Vec<PropertyOption>) {
        let Some(detail) = &self.issues_state.issue_detail else {
            return (String::new(), Vec::new());
        };
        let selected = |items: &[String]| {
            items
                .iter()
                .map(|l| PropertyOption {
                    label: l.clone(),
                    selected: true,
                })
                .collect::<Vec<_>>()
        };
        let one_selected = |label: &str| {
            vec![PropertyOption {
                label: label.to_string(),
                selected: true,
            }]
        };
        match kind {
            IssuePropertyKind::Labels => (String::new(), selected(&detail.labels)),
            IssuePropertyKind::Assignees => (String::new(), selected(&detail.assignees)),
            IssuePropertyKind::Milestone => (
                String::new(),
                selected(&detail.milestone.clone().into_iter().collect::<Vec<_>>()),
            ),
            IssuePropertyKind::Title => (detail.title.clone(), Vec::new()),
            IssuePropertyKind::Type => match &detail.issue_type_name {
                Some(t) => (String::new(), one_selected(t)),
                None => (String::new(), Vec::new()),
            },
            IssuePropertyKind::State => {
                let is_open = detail.state == crate::domain::IssueState::Open;
                (
                    String::new(),
                    vec![
                        PropertyOption {
                            label: "Open".to_string(),
                            selected: is_open,
                        },
                        PropertyOption {
                            label: "Closed".to_string(),
                            selected: !is_open,
                        },
                    ],
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
                        label: "(clear)".to_string(),
                        selected: true,
                    });
                }
            }
            IssuePropertyKind::Title => {}
        }
    }

    fn apply_issue_property_options_loaded(&mut self, options: &[(String, bool)]) {
        let Some(editor) = &mut self.issues_state.property_editor else {
            return;
        };
        let kind = editor.kind;
        match kind {
            IssuePropertyKind::Labels | IssuePropertyKind::Assignees => {
                let current_selected: Vec<String> = editor
                    .options
                    .iter()
                    .filter(|o| o.selected)
                    .map(|o| o.label.clone())
                    .collect();
                editor.options = options
                    .iter()
                    .map(|(label, _)| {
                        let selected = current_selected
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(label));
                        PropertyOption {
                            label: label.clone(),
                            selected,
                        }
                    })
                    .collect();
                if editor.selected_index >= editor.options.len() {
                    editor.selected_index = 0;
                }
            }
            IssuePropertyKind::Milestone | IssuePropertyKind::Type => {
                let current = editor
                    .options
                    .iter()
                    .find(|o| o.selected)
                    .map(|o| o.label.clone());
                let mut new_opts: Vec<PropertyOption> = options
                    .iter()
                    .map(|(label, _)| PropertyOption {
                        label: label.clone(),
                        selected: current
                            .as_ref()
                            .is_some_and(|c| c.eq_ignore_ascii_case(label)),
                    })
                    .collect();
                new_opts.push(PropertyOption {
                    label: "(clear)".to_string(),
                    selected: current.is_none(),
                });
                editor.options = new_opts;
                editor.selected_index = 0;
            }
            IssuePropertyKind::Title | IssuePropertyKind::State => {}
        }
    }

    fn apply_issue_property_succeeded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        if self
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|d| d.number == issue_number)
        {
            self.issues_state.property_editor = None;
        }
        true
    }

    fn apply_issue_property_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: u64,
        error: &str,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        if self
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|d| d.number == issue_number)
        {
            if let Some(editor) = &mut self.issues_state.property_editor {
                editor.error = Some(error.to_string());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{IssueDetail, IssueState, RepositoryId};
    use crate::state::IssuesState;

    fn make_state_with_detail() -> AppState {
        let detail = IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 42,
            title: "Test Issue".to_string(),
            state: IssueState::Open,
            author_login: "alice".to_string(),
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-02".to_string(),
            labels: vec!["bug".to_string()],
            assignees: vec!["alice".to_string()],
            milestone: Some("v1.0".to_string()),
            issue_type_name: None,
            body: "body".to_string(),
            external_url: "url".to_string(),
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
        };
        AppState {
            issues_state: IssuesState {
                active: true,
                issue_focus: IssueFocus::IssueDetail,
                issue_detail: Some(detail),
                ..IssuesState::default()
            },
            ..AppState::default()
        }
    }

    fn require_issue_editor(state: &AppState) -> &IssuePropertyEditorState {
        state
            .issues_state
            .property_editor
            .as_ref()
            .unwrap_or_else(|| panic!("expected property editor to be open"))
    }

    #[test]
    fn open_property_editor_labels() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        let editor = require_issue_editor(&state);
        assert_eq!(editor.kind, IssuePropertyKind::Labels);
        assert_eq!(editor.options.len(), 1);
        assert!(editor.options[0].selected);
    }

    #[test]
    fn open_property_editor_title_prepopulates() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Title,
        });
        let editor = require_issue_editor(&state);
        assert_eq!(editor.title_text, "Test Issue");
    }

    #[test]
    fn navigate_wraps() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditorNavigateUp);
        let editor = require_issue_editor(&state);
        assert_eq!(editor.selected_index, 0);
    }

    #[test]
    fn toggle_labels_flips_selected() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditorToggle);
        let editor = require_issue_editor(&state);
        assert!(!editor.options[0].selected);
    }

    #[test]
    fn cancel_closes_editor() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditorCancel);
        assert!(state.issues_state.property_editor.is_none());
    }

    #[test]
    fn succeeded_clears_editor() {
        let mut state = make_state_with_detail();
        state.repositories.push(crate::domain::Repository::new(
            RepositoryId("r1".to_string()),
            "repo".to_string(),
            "owner/repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        state.selected_repository_index = Some(0);
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditSucceeded {
            scope_repo_id: RepositoryId("r1".to_string()),
            issue_number: 42,
        });
        assert!(state.issues_state.property_editor.is_none());
    }

    #[test]
    fn failed_sets_error_keeps_editor_open() {
        let mut state = make_state_with_detail();
        state.repositories.push(crate::domain::Repository::new(
            RepositoryId("r1".to_string()),
            "repo".to_string(),
            "owner/repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        state.selected_repository_index = Some(0);
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditFailed {
            scope_repo_id: RepositoryId("r1".to_string()),
            issue_number: 42,
            error: "boom".to_string(),
        });
        let editor = require_issue_editor(&state);
        assert_eq!(editor.error.as_deref(), Some("boom"));
    }

    #[test]
    fn options_loaded_preserves_selection() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::IssueOpenPropertyEditor {
            kind: IssuePropertyKind::Labels,
        });
        state = state.apply(AppEvent::IssuePropertyEditorOptionsLoaded {
            options: vec![
                ("bug".to_string(), false),
                ("enhancement".to_string(), false),
            ],
        });
        let editor = require_issue_editor(&state);
        assert_eq!(editor.options.len(), 2);
        assert!(editor.options[0].selected);
        assert!(!editor.options[1].selected);
    }
}
