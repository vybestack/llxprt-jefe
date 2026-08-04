use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
use jefe::domain::{Agent, AgentId, RepositoryId};
use jefe::state::{AppEvent, AppState, IssueFocus, IssuesState, ScreenId};
use std::path::PathBuf;

fn selected_issue_list_state() -> AppState {
    let mut state = AppState {
        screen: ScreenId::Issues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    state
        .issues_state
        .list
        .replace_items(vec![jefe::domain::Issue {
            number: 621,
            node_id: "I_621".to_owned(),
            title: "Issue 621".to_owned(),
            state: jefe::domain::IssueState::Open,
            author_login: "author".to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
            priority: None,
            state_reason: None,
            linked_pr_numbers: Vec::new(),
        }]);
    state.issues_state.list.set_selected_index(Some(0));
    state.agents.push(Agent::new(
        AgentId("agent-1".to_owned()),
        RepositoryId("repo-1".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Agent One".to_owned(),
        PathBuf::from("/tmp/agent"),
    ));
    state
}

fn state_with_list_send_override(chords: &[&str]) -> AppState {
    let mut state = selected_issue_list_state();
    let dir = tempfile::tempdir();
    let Ok(dir) = dir else {
        panic!("issue list-send config directory must be created: {dir:?}");
    };
    let values = chords
        .iter()
        .map(|chord| format!("\"{chord}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let settings = format!(
        "settings_schema = 2\n[keymap.\"issues.list\"]\n\"issues.list-send-agent\" = [{values}]\n"
    );
    if let Err(error) = std::fs::write(dir.path().join("settings.toml"), settings) {
        panic!("issue list-send settings must be written: {error}");
    }
    let startup = jefe::startup::build_persistence(Some(dir.path()));
    let Ok(startup) = startup else {
        panic!("issue list-send override must compose: {startup:?}");
    };
    state.action_registry_snapshot = Some(startup.keymap_snapshot);
    state
}

#[test]
fn ctrl_s_opens_agent_chooser_from_selected_issue_list_row() {
    let state = selected_issue_list_state();
    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('s'));
    key.modifiers = KeyModifiers::CONTROL;

    let event = resolve_issues_key_event(&state, &key);

    assert!(
        matches!(event, Some(AppEvent::OpenAgentChooser { .. })),
        "Ctrl+S must dispatch OpenAgentChooser from issue list, got {event:?}"
    );
}

#[test]
fn issue_list_send_uses_effective_remap_and_unbind() {
    let f8 = KeyEvent::new(KeyEventKind::Press, KeyCode::F(8));
    let mut ctrl_s = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('s'));
    ctrl_s.modifiers = KeyModifiers::CONTROL;

    let remapped = state_with_list_send_override(&["F8"]);
    assert!(matches!(
        resolve_issues_key_event(&remapped, &f8),
        Some(AppEvent::OpenAgentChooser { .. })
    ));
    assert!(resolve_issues_key_event(&remapped, &ctrl_s).is_none());

    let unbound = state_with_list_send_override(&[]);
    assert!(resolve_issues_key_event(&unbound, &f8).is_none());
    assert!(resolve_issues_key_event(&unbound, &ctrl_s).is_none());
}
