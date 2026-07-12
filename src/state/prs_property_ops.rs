//! PR-mode property-editor state operations (issue #175).
//!
//! Mirrors `issues_property_ops::apply_issue_property_event` and
//! `prs_merge_ops::apply_pr_merge_event`. Owns the property-editor overlay
//! transitions (open/navigate/toggle/confirm/cancel) and the edit-result
//! lifecycle (Succeeded/Failed/OptionsLoaded).

use super::{
    AppEvent, AppState, InlineState, PrFocus, PrPropertyEditorState, PrPropertyKind, PropertyOption,
};

impl AppState {
    /// Apply a PR property-editor event (returns handled).
    pub(super) fn apply_pr_property_event(&mut self, event: &AppEvent) -> bool {
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
                self.prs_state.property_editor = None;
                true
            }
            AppEvent::PrPropertyEditorOptionsLoaded { options } => {
                self.apply_pr_property_options_loaded(options);
                true
            }
            AppEvent::PrPropertyEditSucceeded {
                scope_repo_id,
                pr_number,
            } => self.apply_pr_property_succeeded(scope_repo_id, *pr_number),
            AppEvent::PrPropertyEditFailed {
                scope_repo_id,
                pr_number,
                error,
            } => self.apply_pr_property_failed(scope_repo_id, *pr_number, error),
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
        {
            return;
        }
        if self.prs_state.pr_detail.is_none() {
            return;
        }
        let (title_text, options) = self.pr_property_initial_state(kind);
        let title_cursor = title_text.len();
        self.prs_state.property_editor = Some(PrPropertyEditorState {
            kind,
            options,
            selected_index: 0,
            title_text,
            title_cursor,
            error: None,
        });
    }

    fn pr_property_initial_state(&self, kind: PrPropertyKind) -> (String, Vec<PropertyOption>) {
        let Some(detail) = &self.prs_state.pr_detail else {
            return (String::new(), Vec::new());
        };
        match kind {
            PrPropertyKind::Labels => {
                let opts = detail
                    .labels
                    .iter()
                    .map(|l| PropertyOption {
                        label: l.clone(),
                        selected: true,
                    })
                    .collect();
                (String::new(), opts)
            }
            PrPropertyKind::Assignees => {
                let opts = detail
                    .assignees
                    .iter()
                    .map(|a| PropertyOption {
                        label: a.clone(),
                        selected: true,
                    })
                    .collect();
                (String::new(), opts)
            }
            PrPropertyKind::Milestone => {
                let opts = detail
                    .milestone
                    .iter()
                    .map(|m| PropertyOption {
                        label: m.clone(),
                        selected: true,
                    })
                    .collect();
                (String::new(), opts)
            }
            PrPropertyKind::Title => (detail.title.clone(), Vec::new()),
            PrPropertyKind::State => {
                let is_open = detail.state == crate::domain::PrState::Open;
                let opts = vec![
                    PropertyOption {
                        label: "Open".to_string(),
                        selected: is_open,
                    },
                    PropertyOption {
                        label: "Closed".to_string(),
                        selected: !is_open,
                    },
                ];
                (String::new(), opts)
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
                    });
                }
            }
            PrPropertyKind::Title => {}
        }
    }

    fn apply_pr_property_options_loaded(&mut self, options: &[(String, bool)]) {
        let Some(editor) = &mut self.prs_state.property_editor else {
            return;
        };
        let kind = editor.kind;
        match kind {
            PrPropertyKind::Labels | PrPropertyKind::Assignees => {
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
            PrPropertyKind::Milestone => {
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
            PrPropertyKind::Title | PrPropertyKind::State => {}
        }
    }

    fn apply_pr_property_succeeded(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
    ) -> bool {
        let scope_matches = self
            .selected_repository_index
            .and_then(|idx| self.repositories.get(idx))
            .is_some_and(|repo| &repo.id == scope_repo_id);
        if !scope_matches {
            return true;
        }
        if self
            .prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.number == pr_number)
        {
            self.prs_state.property_editor = None;
        }
        true
    }

    fn apply_pr_property_failed(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
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
            .prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.number == pr_number)
        {
            if let Some(editor) = &mut self.prs_state.property_editor {
                editor.error = Some(error.to_string());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PrState, PullRequestDetail, RepositoryId};
    use crate::state::PullRequestsState;

    fn make_state_with_detail() -> AppState {
        let detail = PullRequestDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 42,
            title: "Test PR".to_string(),
            state: PrState::Open,
            is_draft: false,
            author_login: "alice".to_string(),
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-02".to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            labels: vec!["bug".to_string()],
            assignees: vec!["alice".to_string()],
            milestone: Some("v1.0".to_string()),
            body: "body".to_string(),
            external_url: "url".to_string(),
            review_decision: None,
            checks_status: crate::domain::PrCheckStatus::None,
            reviews: Vec::new(),
            checks: Vec::new(),
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
            mergeable: Some(true),
            merge_state_status: None,
        };
        AppState {
            prs_state: PullRequestsState {
                active: true,
                pr_focus: PrFocus::PrDetail,
                pr_detail: Some(detail),
                ..PullRequestsState::default()
            },
            ..AppState::default()
        }
    }

    fn require_pr_editor(state: &AppState) -> &PrPropertyEditorState {
        state
            .prs_state
            .property_editor
            .as_ref()
            .unwrap_or_else(|| panic!("expected property editor to be open"))
    }

    #[test]
    fn open_property_editor_labels() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        let editor = require_pr_editor(&state);
        assert_eq!(editor.kind, PrPropertyKind::Labels);
        assert_eq!(editor.options.len(), 1);
        assert!(editor.options[0].selected);
    }

    #[test]
    fn open_property_editor_title_prepopulates() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Title,
        });
        let editor = require_pr_editor(&state);
        assert_eq!(editor.title_text, "Test PR");
    }

    #[test]
    fn navigate_wraps() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        state = state.apply(AppEvent::PrPropertyEditorNavigateUp);
        let editor = require_pr_editor(&state);
        assert_eq!(editor.selected_index, 0);
    }

    #[test]
    fn toggle_labels_flips_selected() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        state = state.apply(AppEvent::PrPropertyEditorToggle);
        let editor = require_pr_editor(&state);
        assert!(!editor.options[0].selected);
    }

    #[test]
    fn cancel_closes_editor() {
        let mut state = make_state_with_detail();
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        state = state.apply(AppEvent::PrPropertyEditorCancel);
        assert!(state.prs_state.property_editor.is_none());
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
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        state = state.apply(AppEvent::PrPropertyEditSucceeded {
            scope_repo_id: RepositoryId("r1".to_string()),
            pr_number: 42,
        });
        assert!(state.prs_state.property_editor.is_none());
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
        state = state.apply(AppEvent::PrOpenPropertyEditor {
            kind: PrPropertyKind::Labels,
        });
        state = state.apply(AppEvent::PrPropertyEditFailed {
            scope_repo_id: RepositoryId("r1".to_string()),
            pr_number: 42,
            error: "boom".to_string(),
        });
        let editor = require_pr_editor(&state);
        assert_eq!(editor.error.as_deref(), Some("boom"));
    }
}
