//! Filter key routing for issues mode.
//! @requirement REQ-ISS-008

use iocraft::prelude::*;
use std::collections::BTreeSet;

use jefe::domain::{FILTER_CHOICE_ANY, FILTER_CHOICE_NONE};
use jefe::state::{AppEvent, AppState, ISSUE_FILTER_FIELD_COUNT};

/// Filter field names indexed by `filter_field_index`.
/// 0=state (cycle-only), 1..7 are text/choice fields.
const FILTER_FIELD_NAMES: [&str; ISSUE_FILTER_FIELD_COUNT] = [
    "state",
    "author",
    "assignee",
    "labels",
    "issue_type",
    "milestone",
    "module",
    "query_text",
];

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
        KeyCode::Char('l') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppEvent::ClearDraftFilter)
        }
        KeyCode::Delete => active_field_clear_event(field_idx),
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

/// Build an update event that cycles choice fields through values already
/// visible in the loaded issue rows.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-008
/// @pseudocode component-003 lines 120-127
#[derive(Clone, Copy)]
enum ChoiceDirection {
    Next,
    Previous,
}

fn is_choice_field(field_idx: usize) -> bool {
    matches!(field_idx, 1..=6)
}

fn active_field_clear_event(field_idx: usize) -> Option<AppEvent> {
    let field_name = *FILTER_FIELD_NAMES.get(field_idx)?;
    Some(AppEvent::UpdateDraftFilter {
        field: field_name.to_string(),
        value: String::new(),
    })
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
    let normalized_current = normalized_choice_value(field_name, &current);
    let next = if field_idx == 3 {
        cycle_label_choice(&choices, &normalized_current, direction)?
    } else {
        adjacent_choice(&choices, &normalized_current, direction)?
    };
    Some(AppEvent::UpdateDraftFilter {
        field: field_name.to_string(),
        value: next,
    })
}

/// Collect unique filter choices from currently loaded issue metadata.
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
            "issue_type" => insert_non_empty(&mut choices, &issue.issue_type),
            "milestone" => insert_non_empty(&mut choices, &issue.milestone),
            "module" => insert_non_empty(&mut choices, &issue.module),
            _ => {}
        }
    }
    ordered_filter_choices(choices, field_name)
}

fn insert_non_empty(choices: &mut BTreeSet<String>, value: &str) {
    if !value.is_empty() {
        choices.insert(value.to_string());
    }
}

fn ordered_filter_choices(choices: BTreeSet<String>, field_name: &str) -> Vec<String> {
    let mut ordered: Vec<String> = choices.into_iter().collect();
    if matches!(field_name, "assignee" | "milestone") {
        ordered.push(FILTER_CHOICE_NONE.to_string());
        ordered.push(String::new());
    }
    ordered
}
fn normalized_choice_value(field_name: &str, current: &str) -> String {
    if matches!(
        field_name,
        "author" | "assignee" | "issue_type" | "milestone" | "module"
    ) && current.trim().eq_ignore_ascii_case(FILTER_CHOICE_ANY)
    {
        String::new()
    } else if matches!(field_name, "assignee" | "milestone")
        && current.trim().eq_ignore_ascii_case(FILTER_CHOICE_NONE)
    {
        FILTER_CHOICE_NONE.to_string()
    } else {
        current.to_string()
    }
}

fn adjacent_choice(
    choices: &[String],
    current: &str,
    direction: ChoiceDirection,
) -> Option<String> {
    if current.is_empty() {
        return match direction {
            ChoiceDirection::Next => choices.first().cloned(),
            ChoiceDirection::Previous => previous_from_empty_choice(choices),
        };
    }
    let idx = choices.iter().position(|choice| choice == current)?;
    let next_idx = match direction {
        ChoiceDirection::Next => (idx + 1) % choices.len(),
        ChoiceDirection::Previous => (idx + choices.len() - 1) % choices.len(),
    };

    Some(choices[next_idx].clone())
}

fn previous_from_empty_choice(choices: &[String]) -> Option<String> {
    choices
        .iter()
        .rev()
        .find(|choice| !choice.is_empty())
        .cloned()
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
        "issue_type" => state.issues_state.draft_filter.issue_type.clone(),
        "milestone" => state.issues_state.draft_filter.milestone.clone(),
        "module" => state.issues_state.draft_filter.module.clone(),
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
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
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
    fn test_filter_delete_clears_active_state_field() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &key(KeyCode::Delete));
        assert!(
            matches!(evt, Some(AppEvent::UpdateDraftFilter { field, value }) if field == "state" && value.is_empty())
        );
    }

    #[test]
    fn test_filter_ctrl_c_unwinds_instead_of_clearing() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &ctrl(KeyCode::Char('c')));
        assert!(matches!(evt, Some(AppEvent::ExitIssuesMode)));
    }

    #[test]
    fn test_filter_ctrl_l_clears_entire_filter_form() {
        let state = filter_state();
        let evt = resolve_filter_key_event(&state, &ctrl(KeyCode::Char('l')));
        assert!(matches!(evt, Some(AppEvent::ClearDraftFilter)));
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

    #[test]
    fn test_filter_delete_clears_only_active_author_field() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 1;
        state.issues_state.draft_filter.author = "alice".to_string();
        state.issues_state.draft_filter.assignee = "bob".to_string();

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Delete));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "author");
                assert!(value.is_empty());
            }
            _ => panic!("expected active-field author clear"),
        }
    }

    #[test]
    fn test_filter_right_cycles_assignee_to_none_choice() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2;
        state.issues_state.draft_filter.assignee = "zara".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "none");
            }
            _ => panic!("expected assignee none choice"),
        }
    }

    #[test]
    fn test_filter_right_cycles_type_milestone_module_choices() {
        let mut state = filter_state();
        state.issues_state.issues = vec![
            issue_with_extended(1, "alice", "ui", "bug", "v1", "app"),
            issue_with_extended(2, "bob", "runtime", "feature", "v2", "cli"),
        ];

        assert_choice_update(&mut state, 4, "issue_type", "bug");
        assert_choice_update(&mut state, 5, "milestone", "v1");
        assert_choice_update(&mut state, 6, "module", "app");
    }

    #[test]
    fn test_filter_left_from_assignee_any_cycles_to_none() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2;
        state.issues_state.issues = vec![issue(1, "zara", "bug")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Left));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "none");
            }
            _ => panic!("expected assignee none choice from previous on any"),
        }
    }

    #[test]
    fn test_filter_right_cycles_milestone_to_none_choice() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 5;
        state.issues_state.draft_filter.milestone = "v1".to_string();
        state.issues_state.issues = vec![issue_with_extended(1, "alice", "ui", "bug", "v1", "app")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "milestone");
                assert_eq!(value, "none");
            }
            _ => panic!("expected milestone none choice"),
        }
    }

    #[test]
    fn test_filter_right_cycles_typed_assignee_any_as_empty_choice() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 2;
        state.issues_state.draft_filter.assignee = "ANY".to_string();
        state.issues_state.issues = vec![issue(1, "zara", "bug")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "assignee");
                assert_eq!(value, "zara");
            }
            _ => panic!("expected assignee choice after typed any"),
        }
    }

    #[test]
    fn test_filter_left_cycles_typed_milestone_any_to_none() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 5;
        state.issues_state.draft_filter.milestone = "any".to_string();
        state.issues_state.issues = vec![issue_with_extended(1, "alice", "ui", "bug", "v1", "app")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Left));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "milestone");
                assert_eq!(value, "none");
            }
            _ => panic!("expected milestone none choice after typed any"),
        }
    }

    #[test]
    fn test_filter_right_cycles_typed_type_any_as_empty_choice() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 4;
        state.issues_state.draft_filter.issue_type = "ANY".to_string();
        state.issues_state.issues = vec![issue_with_extended(1, "alice", "ui", "bug", "v1", "app")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "issue_type");
                assert_eq!(value, "bug");
            }
            _ => panic!("expected type choice after typed any"),
        }
    }

    #[test]
    fn test_filter_right_cycles_typed_module_any_as_empty_choice() {
        let mut state = filter_state();
        state.issues_state.filter_ui.field_index = 6;
        state.issues_state.draft_filter.module = "any".to_string();
        state.issues_state.issues = vec![issue_with_extended(1, "alice", "ui", "bug", "v1", "app")];

        let evt = resolve_filter_key_event(&state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, "module");
                assert_eq!(value, "app");
            }
            _ => panic!("expected module choice after typed any"),
        }
    }
    fn issue_with_extended(
        number: u64,
        author: &str,
        label: &str,
        issue_type: &str,
        milestone: &str,
        module: &str,
    ) -> Issue {
        Issue {
            issue_type: issue_type.to_string(),
            milestone: milestone.to_string(),
            module: module.to_string(),
            ..issue_with_author(number, author, author, label)
        }
    }

    fn assert_choice_update(
        state: &mut AppState,
        field_index: usize,
        field_name: &str,
        expected_value: &str,
    ) {
        state.issues_state.filter_ui.field_index = field_index;
        let evt = resolve_filter_key_event(state, &key(KeyCode::Right));
        match evt {
            Some(AppEvent::UpdateDraftFilter { field, value }) => {
                assert_eq!(field, field_name);
                assert_eq!(value, expected_value);
            }
            _ => panic!("expected choice update for {field_name}"),
        }
    }
}
