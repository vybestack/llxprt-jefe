use super::*;
use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};
use jefe::state::{AppEvent, AppState, PrFocus, PullRequestsState, ScreenId};

fn selected_pr_list_state() -> AppState {
    let mut state = AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::PullRequests),
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrList,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    };
    state
        .prs_state
        .list
        .replace_items(vec![jefe::domain::PullRequest {
            number: 621,
            title: "Pull request 621".to_owned(),
            state: jefe::domain::PrState::Open,
            author_login: "author".to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            head_ref: "issue621".to_owned(),
            head_sha: "621".to_owned(),
            base_ref: "main".to_owned(),
            is_draft: false,
            review_decision: None,
            checks_status: jefe::domain::PrCheckStatus::Success,
            mergeable: Some(true),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        }]);
    state.prs_state.list.set_selected_index(Some(0));
    state
}

fn state_with_list_send_override(chords: &[&str]) -> AppState {
    let mut state = selected_pr_list_state();
    let dir = tempfile::tempdir();
    let Ok(dir) = dir else {
        panic!("PR list-send config directory must be created: {dir:?}");
    };
    let values = chords
        .iter()
        .map(|chord| format!("\"{chord}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let settings = format!(
        "settings_schema = 2\n[keymap.\"prs.list\"]\n\"prs.list-send-agent\" = [{values}]\n"
    );
    if let Err(error) = std::fs::write(dir.path().join("settings.toml"), settings) {
        panic!("PR list-send settings must be written: {error}");
    }
    let startup = jefe::startup::build_persistence(Some(dir.path()));
    let Ok(startup) = startup else {
        panic!("PR list-send override must compose: {startup:?}");
    };
    state.action_registry_snapshot = Some(startup.keymap_snapshot);
    state
}

#[test]
fn ctrl_s_opens_agent_chooser_from_selected_pr_list_row() {
    let state = selected_pr_list_state();
    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('s'));
    key.modifiers = KeyModifiers::CONTROL;

    let event = resolve_prs_key_event(&state, &key);

    assert!(
        matches!(event, Some(AppEvent::PrOpenAgentChooser { .. })),
        "Ctrl+S must dispatch PrOpenAgentChooser from PR list, got {event:?}"
    );
}

#[test]
fn pr_list_send_uses_effective_remap_and_unbind() {
    let f8 = KeyEvent::new(KeyEventKind::Press, KeyCode::F(8));
    let mut ctrl_s = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('s'));
    ctrl_s.modifiers = KeyModifiers::CONTROL;

    let remapped = state_with_list_send_override(&["F8"]);
    assert!(matches!(
        resolve_prs_key_event(&remapped, &f8),
        Some(AppEvent::PrOpenAgentChooser { .. })
    ));
    assert!(resolve_prs_key_event(&remapped, &ctrl_s).is_none());

    let unbound = state_with_list_send_override(&[]);
    assert!(resolve_prs_key_event(&unbound, &f8).is_none());
    assert!(resolve_prs_key_event(&unbound, &ctrl_s).is_none());
}
