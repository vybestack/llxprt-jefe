//! Focused one-resolution production-route tests for issue #383 S3.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jefe::domain::action_registry::{HandlerKey, Resolution};
use jefe::domain::effects::{
    Effect, EffectCompletion, EffectResponse, ProviderEffect, ProviderResponse,
};
use jefe::messages::{AppMessage, RepositoryAgentMessage, SystemMessage};
use jefe::state::transition::TransitionExt;
use jefe::state::{
    AppState, ComposerTarget, ConfirmFocus, ErrorsFocus, InlineState, IssueFocus,
    IssuePropertyEditorState, IssuePropertyKind, IssuesState, ModalState, NewIssueFormState,
    PaneFocus, PrFocus, PullRequestsState, ScreenId,
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

fn with_submit_override(mut state: AppState, context: &str, action: &str) -> AppState {
    let dir = tempfile::tempdir();
    let Ok(dir) = dir else {
        panic!("composer route config directory must be created: {dir:?}");
    };
    let settings =
        format!("settings_schema = 2\n[keymap.\"{context}\"]\n\"{action}\" = [\"F8\"]\n");
    let result = std::fs::write(dir.path().join("settings.toml"), settings);
    if let Err(error) = result {
        panic!("composer route settings must be written: {error}");
    }
    let startup = jefe::startup::build_persistence(Some(dir.path()));
    let Ok(startup) = startup else {
        panic!("composer route fixture must compose: {startup:?}");
    };
    state.action_registry_snapshot = Some(startup.keymap_snapshot);
    state
}

fn assert_replaced_submit_route(state: AppState, expected: HandlerKey) {
    assert_handler(&state, &key(KeyCode::F(8)), expected);
    let compiled = modified(KeyCode::Enter, KeyModifiers::ALT);
    assert!(matches!(
        resolve_compiled_registry_key(&state, &compiled).resolution,
        Resolution::Unbound
    ));
}

fn with_projected_availability(mut state: AppState) -> AppState {
    let dir = tempfile::tempdir();
    let Ok(dir) = dir else {
        panic!("availability config directory must be created: {dir:?}");
    };
    let startup = jefe::startup::build_persistence(Some(dir.path()));
    let Ok(startup) = startup else {
        panic!("availability fixture must compose: {startup:?}");
    };
    state.action_registry_snapshot = Some(startup.keymap_snapshot);
    let transition = state.apply_message(AppMessage::RepositoryAgent(
        RepositoryAgentMessage::ProjectActionAvailability,
    ));
    let Ok(transition) = transition else {
        panic!("availability request must commit: {transition:?}");
    };
    let Some(issued) = transition.effects.first() else {
        panic!("availability request must stage one effect");
    };
    assert_eq!(transition.effects.len(), 1);
    let Effect::Provider(ProviderEffect::ProjectActionAvailability { entries }) = &issued.effect
    else {
        panic!("availability must use the closed provider variant");
    };
    let completion = EffectCompletion {
        correlation: issued.correlation.clone(),
        result: Ok(EffectResponse::Provider(
            ProviderResponse::ActionAvailability {
                entries: entries.clone(),
            },
        )),
    };
    transition
        .next_state
        .apply_message(AppMessage::EffectCompletion(Box::new(completion)))
        .committed_pure()
}

fn assert_list_send_unavailable(mut state: AppState, reason: &str) {
    let pending_issue_detail = state.issues_state.detail_pending.clone();
    let pending_pr_detail = state.prs_state.detail_pending.clone();
    let issue_chooser = state.issues_state.agent_chooser.clone();
    let pr_chooser = state.prs_state.agent_chooser.clone();
    let pending_effects = state.pending_effects.clone();
    let resolved =
        resolve_compiled_registry_key(&state, &modified(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let Resolution::Unavailable {
        reason: actual_reason,
        ..
    } = resolved.resolution
    else {
        panic!("list send must resolve unavailable: {resolved:?}");
    };
    assert_eq!(actual_reason, reason);

    super::record_unavailable(&mut state, None, actual_reason);

    assert_eq!(state.warning_message.as_deref(), Some(reason));
    assert_eq!(state.issues_state.detail_pending, pending_issue_detail);
    assert_eq!(state.prs_state.detail_pending, pending_pr_detail);
    assert_eq!(state.issues_state.agent_chooser, issue_chooser);
    assert_eq!(state.prs_state.agent_chooser, pr_chooser);
    assert_eq!(state.pending_effects, pending_effects);
}

#[test]
fn escape_consumes_an_active_warning_before_other_routing() {
    let mut state = AppState {
        modal: ModalState::ConfirmDeleteRepository {
            id: jefe::domain::RepositoryId("repo".to_owned()),
            confirm_focus: ConfirmFocus::Confirm,
        },
        ..AppState::default()
    };
    state.issues_state.draft_notice = Some("No agents available".to_owned());
    state.prs_state.draft_notice = Some("No agents available".to_owned());

    assert!(crate::app_shell::should_dismiss_warning(
        &state,
        &key(KeyCode::Esc)
    ));
    assert!(!crate::app_shell::should_dismiss_warning(
        &state,
        &key(KeyCode::Enter)
    ));
    assert!(!crate::app_shell::should_dismiss_warning(
        &AppState::default(),
        &key(KeyCode::Esc)
    ));

    let provider_action = jefe::domain::action_registry::ActionId::parse("vendor.deploy.ship")
        .unwrap_or_else(|error| panic!("provider action id: {error}"));
    let provider_state = AppState {
        warning_message: Some("provider unavailable".to_owned()),
        provider_surface_action: Some(provider_action),
        ..AppState::default()
    };
    assert!(!crate::app_shell::should_dismiss_warning(
        &provider_state,
        &key(KeyCode::Esc)
    ));

    let after = state
        .apply_message(AppMessage::System(SystemMessage::ClearWarning))
        .committed_pure();
    assert!(after.issues_state.draft_notice.is_none());
    assert!(after.prs_state.draft_notice.is_none());
    assert!(matches!(
        after.modal,
        ModalState::ConfirmDeleteRepository { .. }
    ));
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

    super::record_unavailable(
        &mut state,
        None,
        "No pull request loaded to merge".to_owned(),
    );

    assert_eq!(
        state.warning_message.as_deref(),
        Some("No pull request loaded to merge")
    );
    assert_eq!(state.pending_effects, pending_before);
    assert_eq!(state.repositories.len(), repository_count);
    assert_eq!(state.agents.len(), agent_count);
}

#[test]
fn issue_list_send_without_selection_resolves_unavailable_without_side_effects() {
    let state = with_projected_availability(AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::Issues),
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueList,
            ..IssuesState::default()
        },
        ..AppState::default()
    });

    assert_list_send_unavailable(state, "No issue selected");
}

#[test]
fn pr_list_send_without_selection_resolves_unavailable_without_side_effects() {
    let state = with_projected_availability(AppState {
        nav: jefe::state::navigation::NavState::rooted(ScreenId::PullRequests),
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrList,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    });

    assert_list_send_unavailable(state, "No pull request selected");
}

#[test]
fn dashboard_and_split_use_registry_handlers() {
    assert_handler(
        &AppState::default(),
        &key(KeyCode::Down),
        HandlerKey::NavigateDown,
    );
    let split = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Repositories),
        ..AppState::default()
    };
    assert_handler(
        &split,
        &key(KeyCode::PageDown),
        // On the workbench a page is a page of agent cards (issue #626).
        HandlerKey::WorkbenchNextPage,
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
        nav: crate::state::navigation::NavState::rooted(ScreenId::Errors),
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
        nav: crate::state::navigation::NavState::rooted(ScreenId::Actions),
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
            nav: jefe::state::navigation::NavState::rooted(screen),
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
            nav: jefe::state::navigation::NavState::rooted(screen),
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
        nav: crate::state::navigation::NavState::rooted(ScreenId::Issues),
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
fn new_issue_submit_override_uses_state_derived_production_context() {
    let state = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Issues),
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueDetail,
            inline_state: InlineState::Composer {
                target: ComposerTarget::NewIssue,
                text: String::new(),
                cursor: 0,
            },
            new_issue_form: Some(NewIssueFormState::default()),
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    let state = with_submit_override(state, "issues.new-form", "issues.new-submit");

    assert_replaced_submit_route(state, HandlerKey::IssuesSubmitInline);
}

#[test]
fn issue_inline_submit_override_uses_state_derived_production_context() {
    let state = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Issues),
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueDetail,
            inline_state: InlineState::Composer {
                target: ComposerTarget::NewComment,
                text: String::new(),
                cursor: 0,
            },
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    let state = with_submit_override(state, "issues.inline", "issues.inline-submit");

    assert_replaced_submit_route(state, HandlerKey::IssuesSubmitInline);
}

#[test]
fn pr_inline_submit_override_uses_state_derived_production_context() {
    let state = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::PullRequests),
        prs_state: PullRequestsState {
            active: true,
            inline_state: InlineState::Composer {
                target: ComposerTarget::NewComment,
                text: String::new(),
                cursor: 0,
            },
            ..PullRequestsState::default()
        },
        ..AppState::default()
    };
    let state = with_submit_override(state, "prs.inline", "prs.inline-submit");

    assert_replaced_submit_route(state, HandlerKey::PullRequestsSubmitInline);
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
