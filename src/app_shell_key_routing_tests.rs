//! Focused one-resolution production-route tests for issue #383 S3.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jefe::domain::action_registry::{HandlerKey, Resolution};
use jefe::domain::effects::{
    Effect, EffectCompletion, EffectResponse, ProviderEffect, ProviderResponse,
};
use jefe::messages::{AppMessage, RepositoryAgentMessage, SystemMessage};
use jefe::state::screen_overlays::ConfirmationRequest;
use jefe::state::transition::TransitionExt;
use jefe::state::{
    AppState, ComposerTarget, ErrorsFocus, InlineState, IssueFocus, IssuePropertyEditorState,
    IssuePropertyKind, IssuesState, NewIssueFormState, PaneFocus, PrFocus, PullRequestsState,
    ScreenId,
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

fn confirmation_field(
    kind: jefe::domain::plugin::field::FieldKind,
    choices: Vec<jefe::domain::plugin::field::Scalar>,
    min: Option<jefe::domain::plugin::field::Scalar>,
) -> jefe::domain::plugin::field::Field {
    use jefe::domain::plugin::field::{Field, FieldDraft, RestartScope};

    let Ok(id) = jefe::domain::Id::parse("note") else {
        panic!("static field ID must be valid");
    };
    let Ok(field) = Field::parse(FieldDraft {
        id,
        label: "Note".to_owned(),
        description: None,
        kind,
        required: true,
        default: None,
        min,
        max: None,
        choices,
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    }) else {
        panic!("static field must be valid");
    };
    field
}

fn confirmation_string_field(
    min: Option<jefe::domain::plugin::field::Scalar>,
) -> jefe::domain::plugin::field::Field {
    confirmation_field(
        jefe::domain::plugin::field::FieldKind::String,
        Vec::new(),
        min,
    )
}

#[test]
fn focused_provider_confirmation_field_consumes_enter_before_registry_fallback() {
    assert!(matches!(
        super::provider_surface_key_route(&key(KeyCode::Enter), true, true),
        super::ProviderSurfaceKeyRoute::Consume
    ));
    assert!(matches!(
        super::provider_surface_key_route(&key(KeyCode::Enter), true, false),
        super::ProviderSurfaceKeyRoute::Dispatch(
            crate::app_input::ProviderSurfaceControl::ActivateConfirmation
        )
    ));
}

#[test]
fn focused_provider_confirmation_field_routes_typed_keyboard_edits() {
    use jefe::domain::TypedValue;

    let field = confirmation_string_field(None);
    assert!(matches!(
        super::provider_confirmation_edit_route(
            &field,
            None,
            &key(KeyCode::Char('x'))
        ),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::String(value))
            if value == "x"
    ));
    assert!(matches!(
        super::provider_confirmation_edit_route(&field, None, &key(KeyCode::Backspace)),
        super::ProviderConfirmationEditRoute::Consume
    ));
}

#[test]
fn constraint_incomplete_confirmation_edit_routes_to_instance_draft() {
    use jefe::domain::TypedValue;
    use jefe::domain::plugin::field::Scalar;

    let field = confirmation_string_field(Some(Scalar::Integer(5)));

    assert!(matches!(
        super::provider_confirmation_edit_route(&field, None, &key(KeyCode::Char('x'))),
        super::ProviderConfirmationEditRoute::Draft(TypedValue::String(value)) if value == "x"
    ));
}

#[test]
fn provider_confirmation_routes_boolean_path_and_enum_keyboard_edits() {
    use jefe::domain::TypedValue;
    use jefe::domain::plugin::field::{FieldKind, Scalar};

    let boolean = confirmation_field(FieldKind::Boolean, Vec::new(), None);
    assert!(matches!(
        super::provider_confirmation_edit_route(
            &boolean,
            Some(&TypedValue::Bool(false)),
            &key(KeyCode::Char(' ')),
        ),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::Bool(true))
    ));

    let path = confirmation_field(FieldKind::Path, Vec::new(), None);
    assert!(matches!(
        super::provider_confirmation_edit_route(&path, None, &key(KeyCode::Char('/'))),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::String(value))
            if value == "/"
    ));

    let enumeration = confirmation_field(
        FieldKind::Enum,
        vec![
            Scalar::Text("red".to_owned()),
            Scalar::Text("blue".to_owned()),
        ],
        None,
    );
    assert!(matches!(
        super::provider_confirmation_edit_route(
            &enumeration,
            Some(&TypedValue::String("red".to_owned())),
            &key(KeyCode::Left),
        ),
        super::ProviderConfirmationEditRoute::Consume
    ));
}

#[test]
fn provider_confirmation_routes_numeric_list_and_secret_keyboard_edits() {
    use jefe::domain::TypedValue;
    use jefe::domain::plugin::field::FieldKind;

    let integer = confirmation_field(FieldKind::Integer, Vec::new(), None);
    let sign = super::provider_confirmation_edit_route(&integer, None, &key(KeyCode::Char('-')));
    let super::ProviderConfirmationEditRoute::Draft(sign) = sign else {
        panic!("an integer sign must remain an instance-local draft");
    };
    assert!(matches!(
        super::provider_confirmation_edit_route(&integer, Some(&sign), &key(KeyCode::Char('4'))),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::Integer(-4))
    ));

    let number = confirmation_field(FieldKind::FiniteNumber, Vec::new(), None);
    let decimal = super::provider_confirmation_edit_route(
        &number,
        Some(&TypedValue::Integer(1)),
        &key(KeyCode::Char('.')),
    );
    let super::ProviderConfirmationEditRoute::Draft(decimal) = decimal else {
        panic!("a trailing decimal point must remain an instance-local draft");
    };
    assert!(matches!(
        super::provider_confirmation_edit_route(&number, Some(&decimal), &key(KeyCode::Char('2'))),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::Decimal(value))
            if value.as_str() == "1.2"
    ));

    let list = confirmation_field(FieldKind::StringList, Vec::new(), None);
    assert!(matches!(
        super::provider_confirmation_edit_route(&list, None, &key(KeyCode::Char('a'))),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::List(values))
            if values == vec![TypedValue::String("a".to_owned())]
    ));

    let secret = confirmation_field(FieldKind::SecretReference, Vec::new(), None);
    assert!(matches!(
        super::provider_confirmation_edit_route(&secret, None, &key(KeyCode::Char('A'))),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::SecretRef(value))
            if value.env.env() == "A"
    ));
}

#[test]
fn provider_confirmation_accepts_shift_characters_and_consumes_navigation_keys() {
    use jefe::domain::TypedValue;

    // SHIFT-only characters (uppercase/shifted symbols) reach the field editor;
    // unmodified navigation keys are consumed so they cannot fall through to the
    // global key stack beneath the blocking confirmation.
    let uppercase = confirmation_string_field(None);
    assert!(matches!(
        super::provider_confirmation_edit_route(
            &uppercase,
            None,
            &modified(KeyCode::Char('A'), KeyModifiers::SHIFT),
        ),
        super::ProviderConfirmationEditRoute::Dispatch(TypedValue::String(value))
            if value == "A"
    ));
    assert!(matches!(
        super::provider_confirmation_edit_route(
            &confirmation_string_field(None),
            None,
            &key(KeyCode::Delete),
        ),
        super::ProviderConfirmationEditRoute::Consume
    ));
}
fn assert_handler(state: &AppState, event: &KeyEvent, expected: HandlerKey) {
    let resolved = resolve_compiled_registry_key(state, event);
    assert!(
        matches!(resolved.resolution, Resolution::Dispatch { handler, .. } if handler == expected),
        "unexpected resolution: {:?}",
        resolved.resolution
    );
}

fn with_submit_override(
    context: &str,
    action: &str,
    customize: impl FnOnce(AppState) -> AppState,
) -> AppState {
    let settings =
        format!("settings_schema = 2\n[keymap.\"{context}\"]\n\"{action}\" = [\"F8\"]\n");
    customize(crate::test_app_state_from_settings(settings.as_bytes()))
}

fn assert_replaced_submit_route(state: AppState, expected: HandlerKey) {
    assert_handler(&state, &key(KeyCode::F(8)), expected);
    let compiled = modified(KeyCode::Enter, KeyModifiers::ALT);
    assert!(matches!(
        resolve_compiled_registry_key(&state, &compiled).resolution,
        Resolution::Unbound
    ));
}

fn with_projected_availability(state: AppState) -> AppState {
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
fn escape_defers_warning_dismissal_to_active_back_layers() {
    let mut state = crate::test_app_state();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
            id: jefe::domain::RepositoryId("repo".to_owned()),
        })
    );
    state.issues_state.draft_notice = Some("No agents available".to_owned());
    state.prs_state.draft_notice = Some("No agents available".to_owned());

    assert!(!crate::app_shell::should_dismiss_warning(
        &state,
        &key(KeyCode::Esc)
    ));
    assert!(crate::app_shell::should_clear_warning_during_back(
        &state,
        &key(KeyCode::Esc)
    ));
    let mut warning_only = crate::test_app_state();
    warning_only.warning_message = Some("runtime unavailable".to_owned());
    assert!(crate::app_shell::should_dismiss_warning(
        &warning_only,
        &key(KeyCode::Esc)
    ));
    assert!(!crate::app_shell::should_clear_warning_during_back(
        &warning_only,
        &key(KeyCode::Esc)
    ));
    let mut routed = crate::test_app_state();
    routed.enter_screen(ScreenId::Issues);
    routed.warning_message = Some("runtime unavailable".to_owned());
    assert!(!crate::app_shell::should_dismiss_warning(
        &routed,
        &key(KeyCode::Esc)
    ));
    assert!(crate::app_shell::should_clear_warning_during_back(
        &routed,
        &key(KeyCode::Esc)
    ));
    assert!(!crate::app_shell::should_dismiss_warning(
        &state,
        &key(KeyCode::Enter)
    ));
    assert!(!crate::app_shell::should_dismiss_warning(
        &crate::test_app_state(),
        &key(KeyCode::Esc)
    ));

    let after = state
        .apply_message(AppMessage::System(SystemMessage::ClearWarning))
        .committed_pure();
    assert!(after.issues_state.draft_notice.is_none());
    assert!(after.prs_state.draft_notice.is_none());
    assert!(matches!(
        after.nav.current().overlays().generic_confirmation(),
        Some(ConfirmationRequest::DeleteRepository { .. })
    ));
}

#[test]
fn unavailable_dispatch_records_exact_notice_and_stages_no_effect() {
    let mut state = crate::test_app_state();
    state.warning_message = Some("prior".to_owned());
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
    let mut state = crate::test_app_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state = IssuesState {
        active: true,
        issue_focus: IssueFocus::IssueList,
        ..IssuesState::default()
    };
    let state = with_projected_availability(state);

    assert_list_send_unavailable(state, "No issue selected");
}

#[test]
fn pr_list_send_without_selection_resolves_unavailable_without_side_effects() {
    let mut state = crate::test_app_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::PullRequests);
    state.prs_state = PullRequestsState {
        active: true,
        pr_focus: PrFocus::PrList,
        ..PullRequestsState::default()
    };
    let state = with_projected_availability(state);

    assert_list_send_unavailable(state, "No pull request selected");
}

#[test]
fn dashboard_and_split_use_registry_handlers() {
    assert_handler(
        &crate::test_app_state(),
        &key(KeyCode::Down),
        HandlerKey::NavigateDown,
    );
    let mut split = crate::test_app_state();
    split.restore_navigation_root(jefe::workbench::REPOSITORIES_IDENTITY);
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
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Errors);
    assert_handler(&state, &key(KeyCode::Left), HandlerKey::ErrorsCyclePane);
    state.errors_state.focus = ErrorsFocus::ErrorDetail;
    assert_handler(&state, &key(KeyCode::Char('j')), HandlerKey::ErrorsDown);
}

#[test]
fn terminal_and_actions_pre_mode_use_registry_handlers() {
    let mut terminal = crate::test_app_state();
    terminal.pane_focus = PaneFocus::Terminal;
    terminal.terminal_focused = true;
    assert_handler(
        &terminal,
        &key(KeyCode::End),
        HandlerKey::TerminalScrollTail,
    );
    let mut actions = crate::test_app_state();
    actions.nav = crate::state::navigation::NavState::rooted(ScreenId::Actions);
    assert_handler(
        &actions,
        &key(KeyCode::F(12)),
        HandlerKey::ToggleTerminalFocus,
    );
}
#[test]
fn dashboard_overlays_resolve_only_the_legacy_pre_mode_f12_binding() {
    let search = crate::test_app_state()
        .apply(jefe::state::AppEvent::OpenSearch)
        .committed_pure();
    assert_handler(
        &search,
        &key(KeyCode::F(12)),
        HandlerKey::ToggleTerminalFocus,
    );
    assert!(matches!(
        resolve_compiled_registry_key(&search, &key(KeyCode::F(8))).resolution,
        Resolution::Unbound
    ));

    for screen in [
        jefe::workbench::DASHBOARD_IDENTITY,
        jefe::workbench::REPOSITORIES_IDENTITY,
        ScreenId::Actions.into(),
    ] {
        let mut state = crate::test_app_state();
        state.restore_navigation_root(screen);
        assert!(
            state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
                id: jefe::domain::RepositoryId("repo".to_owned()),
            })
        );
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
        let mut state = crate::test_app_state();
        state.restore_navigation_root(screen);
        assert!(
            state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
                id: jefe::domain::RepositoryId("repo".to_owned()),
            })
        );
        assert!(matches!(
            resolve_compiled_registry_key(&state, &key(KeyCode::F(12))).resolution,
            Resolution::Unbound
        ));
    }
}

#[test]
fn full_s4_special_contexts_resolve_controls_and_leave_raw_text_unbound() {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state = IssuesState {
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
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state = IssuesState {
        active: true,
        issue_focus: IssueFocus::IssueDetail,
        inline_state: InlineState::Composer {
            target: ComposerTarget::NewIssue,
            text: String::new(),
            cursor: 0,
        },
        new_issue_form: Some(NewIssueFormState::default()),
        ..IssuesState::default()
    };
    let state = with_submit_override("issues.new-form", "issues.new-submit", |mut base| {
        base.nav = state.nav;
        base.issues_state = state.issues_state;
        base
    });

    assert_replaced_submit_route(state, HandlerKey::IssuesSubmitInline);
}

#[test]
fn issue_inline_submit_override_uses_state_derived_production_context() {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state = IssuesState {
        active: true,
        issue_focus: IssueFocus::IssueDetail,
        inline_state: InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: String::new(),
            cursor: 0,
        },
        ..IssuesState::default()
    };
    let state = with_submit_override("issues.inline", "issues.inline-submit", |mut base| {
        base.nav = state.nav;
        base.issues_state = state.issues_state;
        base
    });

    assert_replaced_submit_route(state, HandlerKey::IssuesSubmitInline);
}

#[test]
fn pr_inline_submit_override_uses_state_derived_production_context() {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::PullRequests);
    state.prs_state = PullRequestsState {
        active: true,
        inline_state: InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: String::new(),
            cursor: 0,
        },
        ..PullRequestsState::default()
    };
    let state = with_submit_override("prs.inline", "prs.inline-submit", |mut base| {
        base.nav = state.nav;
        base.prs_state = state.prs_state;
        base
    });

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
