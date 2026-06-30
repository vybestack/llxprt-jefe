//! Filter key routing for issues mode.
//! @requirement REQ-ISS-008

use iocraft::prelude::*;
use std::collections::BTreeSet;

use jefe::state::{AppEvent, AppState};

/// Filter field names indexed by `filter_field_index`.
/// 0=state (cycle-only), 1..4 are text fields.
const FILTER_FIELD_NAMES: [&str; 5] = ["state", "author", "assignee", "labels", "query_text"];

/// Resolve a key event while filter controls are open.
/// @requirement REQ-ISS-008
pub(super) fn resolve_filter_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let field_idx = state.issues_state.filter_ui.field_index;

    match key_event.code {
        KeyCode::Enter => Some(AppEvent::ApplyFilter),
        KeyCode::Esc => Some(AppEvent::CloseFilterControls),
        KeyCode::Tab => Some(AppEvent::FilterNavigateNext),
        KeyCode::BackTab => Some(AppEvent::FilterNavigatePrev),
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::ExitIssuesMode)
        }
        KeyCode::Delete => Some(AppEvent::ClearFilter),
        // Field-specific input
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_idx == 0 => {
            // State field: cycle through open/closed/all
            Some(AppEvent::CycleFilterState)
        }
        KeyCode::Right if is_choice_field(field_idx) => {
            choice_cycle_event(state, field_idx, ChoiceDirection::Next)
        }
        KeyCode::Left if is_choice_field(field_idx) => {
            choice_cycle_event(state, field_idx, ChoiceDirection::Previous)
        }
        KeyCode::Char(c) if field_idx > 0 => {
            let &field_name = FILTER_FIELD_NAMES.get(field_idx)?;
            let mut value = current_filter_field_value(state, field_name);
            value.push(c);
            Some(AppEvent::UpdateDraftFilter {
                field: field_name.to_string(),
                value,
            })
        }
        KeyCode::Backspace if field_idx > 0 => {
            let &field_name = FILTER_FIELD_NAMES.get(field_idx)?;
            let mut value = current_filter_field_value(state, field_name);
            value.pop();
            Some(AppEvent::UpdateDraftFilter {
                field: field_name.to_string(),
                value,
            })
        }
        _ => None, // consumed, no leak
    }
}

/// Build an update event that cycles author/assignee/label fields through
/// choices already visible in the loaded issue rows.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 120-127
#[derive(Clone, Copy)]
enum ChoiceDirection {
    Next,
    Previous,
}

fn is_choice_field(field_idx: usize) -> bool {
    matches!(field_idx, 1..=3)
}

fn choice_cycle_event(
    state: &AppState,
    field_idx: usize,
    direction: ChoiceDirection,
) -> Option<AppEvent> {
    let field_name = *FILTER_FIELD_NAMES.get(field_idx)?;
    let choices = issue_filter_choices(state, field_name);
    if choices.is_empty() {
        return None;
    }
    let current = current_filter_field_value(state, field_name);
    let next = if field_idx == 3 {
        cycle_label_choice(&choices, &current, direction)?
    } else {
        adjacent_choice(&choices, &current, direction)?
    };
    Some(AppEvent::UpdateDraftFilter {
        field: field_name.to_string(),
        value: next,
    })
}

/// Collect unique author/assignee/label choices from currently loaded issue metadata.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 120-127
fn issue_filter_choices(state: &AppState, field_name: &str) -> Vec<String> {
    let mut choices = BTreeSet::new();
    for issue in &state.issues_state.issues {
        match field_name {
            "author" => {
                choices.insert(issue.author_login.clone());
            }
            "assignee" => choices.extend(issue.assignees.iter().cloned()),
            "labels" => choices.extend(issue.labels.iter().cloned()),
            _ => {}
        }
    }
    choices.into_iter().collect()
}

fn adjacent_choice(
    choices: &[String],
    current: &str,
    direction: ChoiceDirection,
) -> Option<String> {
    if current.is_empty() {
        return choices.first().cloned();
    }
    let idx = choices.iter().position(|choice| choice == current)?;
    let next_idx = match direction {
        ChoiceDirection::Next => (idx + 1) % choices.len(),
        ChoiceDirection::Previous => (idx + choices.len() - 1) % choices.len(),
    };

    Some(choices[next_idx].clone())
}

fn cycle_label_choice(
    choices: &[String],
    current: &str,
    direction: ChoiceDirection,
) -> Option<String> {
    let Some((prefix, active)) = split_label_filter_tail(current) else {
        return adjacent_choice(choices, current.trim(), direction);
    };
    if active.is_empty() {
        let next = first_unused_label_choice(choices, &prefix)?;
        return Some(format!("{prefix}{next}"));
    }
    let next = adjacent_choice(choices, active, direction)?;
    Some(format!("{prefix}{next}"))
}

fn first_unused_label_choice(choices: &[String], prefix: &str) -> Option<String> {
    choices
        .iter()
        .find(|choice| !label_prefix_contains(prefix, choice))
        .cloned()
}

fn label_prefix_contains(prefix: &str, label: &str) -> bool {
    prefix
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| part == label)
}

fn split_label_filter_tail(current: &str) -> Option<(String, &str)> {
    let comma_idx = current.rfind(',')?;
    let (_, tail) = current.split_at(comma_idx + 1);
    let trimmed_tail = tail.trim_start();
    let whitespace_len = tail.len().saturating_sub(trimmed_tail.len());
    let visible_prefix_len = comma_idx + 1 + whitespace_len;
    current
        .get(..visible_prefix_len)
        .map(|prefix| (prefix.to_string(), trimmed_tail.trim_end()))
}

/// Read the current value of a draft filter text field.
/// For labels, reads the raw editing string to preserve trailing commas.
fn current_filter_field_value(state: &AppState, field_name: &str) -> String {
    match field_name {
        "author" => state.issues_state.draft_filter.author.clone(),
        "assignee" => state.issues_state.draft_filter.assignee.clone(),
        "labels" => state.issues_state.filter_ui.draft_labels_text.clone(),
        "query_text" => state.issues_state.draft_filter.query_text.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
    use jefe::domain::{Issue, IssueState};
    use jefe::state::{AppState, IssueFilterUiState, ScreenMode};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        let mut evt = KeyEvent::new(KeyEventKind::Press, code);
        evt.modifiers = KeyModifiers::CONTROL;
        evt
    }

    fn filter_state() -> AppState {
        AppState {
            screen_mode: ScreenMode::DashboardIssues,
            issues_state: jefe::state::IssuesState {
                active: true,
                filter_ui: IssueFilterUiState {
                    controls_open: true,
                    field_index: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn issue(number: u64, assignees: &str, labels: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue {number}"),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2026-06-30".to_string(),
            assignee_summary: assignees.to_string(),
            labels_summary: labels.to_string(),
            assignees: summary_vec(assignees),
            labels: summary_vec(labels),
            comment_count: 0,
            body: String::new(),
        }
    }

    fn issue_with_author(number: u64, author: &str, assignees: &str, labels: &str) -> Issue {
        Issue {
            author_login: author.to_string(),
            ..issue(number, assignees, labels)
        }
    }

    fn summary_vec(summary: &str) -> Vec<String> {
        summary
            .split(", ")
            .filter(|value| !value.is_empty())
            .map(String::from)
            .collect()
    }

    #[test]
    fn test_filter_enter_applies() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Enter));
        assert!(matches!(evt, Some(AppEvent::ApplyFilter)));
    }

    #[test]
    fn test_filter_esc_closes() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Esc));
        assert!(matches!(evt, Some(AppEvent::CloseFilterControls)));
    }

    #[test]
    fn test_filter_tab_navigates_next() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Tab));
        assert!(matches!(evt, Some(AppEvent::FilterNavigateNext)));
    }

    #[test]
    fn test_filter_backtab_navigates_prev() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::BackTab));
        assert!(matches!(evt, Some(AppEvent::FilterNavigatePrev)));
    }

    #[test]
    fn test_filter_delete_clears() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Delete));
        assert!(matches!(evt, Some(AppEvent::ClearFilter)));
    }

    #[test]
    fn test_filter_ctrl_c_unwinds_instead_of_clearing() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &ctrl(KeyCode::Char('c')));
        assert!(matches!(evt, Some(AppEvent::ExitIssuesMode)));
    }

    #[test]
    fn test_filter_space_on_state_field_cycles() {
        let state = filter_state();
        assert_eq!(state.issues_state.filter_ui.field_index, 0);
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char(' ')));
        assert!(matches!(evt, Some(AppEvent::CycleFilterState)));
    }

    #[test]
    fn test_filter_left_on_state_field_cycles() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Left));
        assert!(matches!(evt, Some(AppEvent::CycleFilterState)));
    }

    #[test]
    fn test_filter_char_on_text_field_appends() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 1; // author
        state.issues_state.draft_filter.author = "al".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char('i')));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "author");
                assert_eq!(value, "ali");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }

    #[test]
    fn test_filter_backspace_on_text_field_pops() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2; // assignee
        state.issues_state.draft_filter.assignee = "bob".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Backspace));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "bo");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }

    #[test]
    fn test_filter_char_on_state_field_not_text_input() {
        let state = filter_state();
        assert_eq!(state.issues_state.filter_ui.field_index, 0);
        // Typing a regular letter on the state field (idx 0) should be consumed
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char('x')));
        assert!(
            evt.is_none(),
            "non-special keys on state field are consumed"
        );
    }

    #[test]
    fn test_filter_labels_field_text_input() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3; // labels
        state.issues_state.filter_ui.draft_labels_text = "bug".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char(',')));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "bug,");
            }
            _ => panic!("expected UpdateDraftFilter"),
        }
    }

    #[test]
    fn test_filter_right_cycles_author_choices_from_loaded_issues() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 1;
        state.issues_state.issues = vec![
            issue_with_author(1, "zara", "zara", "bug"),
            issue_with_author(2, "alice", "alice", "ui"),
        ];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "author");
                assert_eq!(value, "alice");
            }
            _ => panic!("expected choice-backed author update"),
        }
    }

    #[test]
    fn test_filter_right_cycles_assignee_choices_from_loaded_issues() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2;
        state.issues_state.issues = vec![issue(1, "zara", "bug"), issue(2, "alice", "ui")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "alice");
            }
            _ => panic!("expected choice-backed assignee update"),
        }
    }

    #[test]
    fn test_filter_right_cycles_label_choices_from_loaded_issues() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "bug".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug, ui"), issue(2, "alice", "docs")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "docs");
            }
            _ => panic!("expected choice-backed labels update"),
        }
    }

    #[test]
    fn test_filter_space_still_types_in_label_text_field() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "good".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "good first issue")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Char(' ')));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "good ");
            }
            _ => panic!("expected literal space text input"),
        }
    }

    #[test]
    fn test_filter_left_cycles_label_choices_backward() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "docs".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug, ui"), issue(2, "alice", "docs")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Left));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "bug");
            }
            _ => panic!("expected backward labels update"),
        }
    }

    #[test]
    fn test_filter_right_cycles_only_last_label_token() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "bug, docs".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug, ui"), issue(2, "alice", "docs")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "bug, ui");
            }
            _ => panic!("expected last-token labels update"),
        }
    }

    #[test]
    fn test_filter_right_preserves_unknown_typed_assignee() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2;
        state.issues_state.draft_filter.assignee = "custom-user".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        assert!(evt.is_none());
    }

    #[test]
    fn test_filter_right_preserves_unknown_last_label_token() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "bug, custom".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug, ui")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        assert!(evt.is_none());
    }

    #[test]
    fn test_filter_right_fills_trailing_comma_with_unused_label() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 3;
        state.issues_state.filter_ui.draft_labels_text = "bug,".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug, ui")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "labels");
                assert_eq!(value, "bug,ui");
            }
            _ => panic!("expected unused label choice after trailing comma"),
        }
    }
}
