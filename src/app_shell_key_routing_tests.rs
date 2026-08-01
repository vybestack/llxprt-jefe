//! Focused one-resolution production-route tests for issue #383 S3.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jefe::domain::action_registry::{HandlerKey, Resolution};
use jefe::state::{
    AppState, ConfirmFocus, ErrorsFocus, InlineState, IssueFocus, IssuePropertyEditorState,
    IssuePropertyKind, IssuesState, ModalState, PaneFocus, ScreenId,
};

use super::resolve_compiled_registry_key;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = key(code);
    event.modifiers = modifiers;
    event
}

fn assert_handler(state: &AppState, event: &KeyEvent, expected: HandlerKey) {
    let resolved = resolve_compiled_registry_key(state, event);
    assert!(
        matches!(resolved.resolution, Resolution::Dispatch { handler, .. } if handler == expected),
        "unexpected resolution: {:?}",
        resolved.resolution
    );
}

#[test]
fn unavailable_dispatch_records_exact_notice_and_stages_no_effect() {
    let mut state = AppState {
        warning_message: Some("prior".to_owned()),
        ..AppState::default()
    };
    let pending_before = state.pending_effects.clone();
    let repository_count = state.repositories.len();
    let agent_count = state.agents.len();

    super::record_unavailable(&mut state, "No pull request loaded to merge".to_owned());

    assert_eq!(
        state.warning_message.as_deref(),
        Some("No pull request loaded to merge")
    );
    assert_eq!(state.pending_effects, pending_before);
    assert_eq!(state.repositories.len(), repository_count);
    assert_eq!(state.agents.len(), agent_count);
}

#[test]
fn dashboard_and_split_use_registry_handlers() {
    assert_handler(
        &AppState::default(),
        &key(KeyCode::Down),
        HandlerKey::NavigateDown,
    );
    let split = AppState {
        screen: ScreenId::Repositories,
        ..AppState::default()
    };
    assert_handler(
        &split,
        &key(KeyCode::PageDown),
        HandlerKey::NavigatePageDown,
    );
    assert_handler(
        &split,
        &modified(KeyCode::Char('r'), KeyModifiers::CONTROL),
        HandlerKey::RestartSelectedAgent,
    );
}

#[test]
fn errors_reverse_cycle_and_detail_scroll_use_registry_handlers() {
    let mut state = AppState {
        screen: ScreenId::Errors,
        ..AppState::default()
    };
    assert_handler(&state, &key(KeyCode::Left), HandlerKey::ErrorsCyclePane);
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
    assert_handler(&state, &key(KeyCode::Char('j')), HandlerKey::ErrorsDown);
}

#[test]
fn terminal_and_actions_pre_mode_use_registry_handlers() {
    let terminal = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };
    assert_handler(
        &terminal,
        &key(KeyCode::End),
        HandlerKey::TerminalScrollTail,
    );
    let actions = AppState {
        screen: ScreenId::Actions,
        ..AppState::default()
    };
    assert_handler(
        &actions,
        &key(KeyCode::F(12)),
        HandlerKey::ToggleTerminalFocus,
    );
}
#[test]
fn dashboard_overlays_resolve_only_the_legacy_pre_mode_f12_binding() {
    let mut search = AppState::default();
    search.dashboard_search.input_focused = true;
    assert_handler(
        &search,
        &key(KeyCode::F(12)),
        HandlerKey::ToggleTerminalFocus,
    );
    assert!(matches!(
        resolve_compiled_registry_key(&search, &key(KeyCode::F(8))).resolution,
        Resolution::Unbound
    ));

    let modal = AppState {
        modal: ModalState::ConfirmDeleteRepository {
            id: jefe::domain::RepositoryId("repo".to_owned()),
            confirm_focus: ConfirmFocus::Confirm,
        },
        ..AppState::default()
    };
    for screen in [
        ScreenId::Dashboard,
        ScreenId::Repositories,
        ScreenId::Actions,
    ] {
        let state = AppState {
            screen,
            ..modal.clone()
        };
        assert_handler(
            &state,
            &key(KeyCode::F(12)),
            HandlerKey::ToggleTerminalFocus,
        );
        assert!(matches!(
            resolve_compiled_registry_key(&state, &key(KeyCode::F(8))).resolution,
            Resolution::Unbound
        ));
    }
    for screen in [ScreenId::Issues, ScreenId::PullRequests] {
        let state = AppState {
            screen,
            ..modal.clone()
        };
        assert!(matches!(
            resolve_compiled_registry_key(&state, &key(KeyCode::F(12))).resolution,
            Resolution::Unbound
        ));
    }
}

#[test]
fn full_s4_special_contexts_resolve_controls_and_leave_raw_text_unbound() {
    let mut state = AppState {
        screen: ScreenId::Issues,
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueDetail,
            property_editor: Some(IssuePropertyEditorState {
                kind: IssuePropertyKind::Title,
                options: Vec::new(),
                selected_index: 0,
                title_text: String::new(),
                title_cursor: 0,
                error: None,
                baseline: Vec::new(),
                loading_failed: false,
                options_loading: false,
                load_request_id: 0,
            }),
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    assert_handler(&state, &key(KeyCode::Esc), HandlerKey::IssuesChooserCancel);
    let text = resolve_compiled_registry_key(&state, &key(KeyCode::Char('x')));
    assert!(matches!(text.resolution, Resolution::Unbound));

    state.issues_state.property_editor = None;
    state.issues_state.inline_state = InlineState::Composer {
        target: jefe::state::ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    assert_handler(
        &state,
        &modified(KeyCode::Enter, KeyModifiers::ALT),
        HandlerKey::IssuesSubmitInline,
    );
    let newline = resolve_compiled_registry_key(&state, &key(KeyCode::Enter));
    assert!(matches!(newline.resolution, Resolution::Unbound));
}

#[test]
fn full_s4_root_has_no_legacy_action_fallback() {
    let shell = include_str!("app_shell.rs");
    for legacy in [
        concat!("dispatch_mode_specific_key", "("),
        concat!("handle_normal_key_event", "("),
        concat!("handle_mode_help_key", "("),
        concat!("handle_mode_search_key", "("),
    ] {
        assert!(
            !shell.contains(legacy),
            "root shell still reaches legacy key route {legacy}"
        );
    }

    for source in [
        include_str!("app_input/actions.rs"),
        include_str!("app_input/issues.rs"),
        include_str!("app_input/prs.rs"),
        include_str!("app_input/filter_controls.rs"),
        include_str!("app_input/modal_handlers.rs"),
    ] {
        assert!(
            !source.contains(concat!("handle_actions_mode_key", "("))
                && !source.contains(concat!("handle_issues_mode_key", "("))
                && !source.contains(concat!("handle_prs_mode_key", "("))
                && !source.contains(concat!("resolve_filter_control_key", "("))
                && !source.contains(concat!("handle_mode_form_key", "(")),
            "migrated S4 source still exposes a hardcoded action-control route"
        );
    }
}
