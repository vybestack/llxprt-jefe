use super::*;
use crossterm::event::{MouseButton, MouseEventKind};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState};

const EVENTS: [MouseEventKind; 12] = [
    MouseEventKind::Down(MouseButton::Left),
    MouseEventKind::Drag(MouseButton::Left),
    MouseEventKind::Up(MouseButton::Left),
    MouseEventKind::Down(MouseButton::Right),
    MouseEventKind::Drag(MouseButton::Right),
    MouseEventKind::Up(MouseButton::Right),
    MouseEventKind::Down(MouseButton::Middle),
    MouseEventKind::Drag(MouseButton::Middle),
    MouseEventKind::Up(MouseButton::Middle),
    MouseEventKind::ScrollUp,
    MouseEventKind::ScrollDown,
    MouseEventKind::Moved,
];

fn assert_overlay_consumes_without_panel_mutation(mut state: AppState) {
    let repository_selection = state.selected_repository_index;
    let agent_selection = state.selected_agent_index;
    let provider_panels = state.provider_panels().clone();
    let pending_effect_count = state.pending_effects.len();
    let pending_confirmation_count = state.provider_requests.pending_confirmation_count();

    for event in EVENTS {
        assert!(consume_blocking_overlay_mouse(&mut state, event, 120, 40));
        assert_eq!(state.selected_repository_index, repository_selection);
        assert_eq!(state.selected_agent_index, agent_selection);
        assert_eq!(state.provider_panels(), &provider_panels);
        assert_eq!(state.pending_effects.len(), pending_effect_count);
        assert_eq!(
            state.provider_requests.pending_confirmation_count(),
            pending_confirmation_count
        );
    }
}

fn provider_confirmation_state() -> AppState {
    use jefe::domain::Id;
    use jefe::domain::plugin::action::{ActionConfirmation, ActionOutcome};
    use jefe::messages::{AppMessage, ProviderMessage};
    use jefe::runtime::provider::protocol::Outcome;
    use jefe::state::provider_requests::{ActionPolicy, InvokeInput};

    let mut state = crate::test_app_state();
    let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
    let action =
        Id::parse("provider.confirm").unwrap_or_else(|error| panic!("provider action: {error}"));
    let screen = Id::parse(state.screen().as_str())
        .unwrap_or_else(|error| panic!("screen identity: {error}"));
    let instance = Id::parse(&state.nav.current().id.to_string())
        .unwrap_or_else(|error| panic!("instance identity: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let empty = jefe::domain::TypedMap::new();
    let key = state
        .provider_requests
        .invoke(InvokeInput {
            owner: &owner,
            action_id: &action,
            context_screen: &screen,
            context_instance: &instance,
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|error| panic!("invoke: {error}"))
        .key;
    state
        .apply_message(AppMessage::Provider(Box::new(ProviderMessage::Outcome {
            key,
            outcome: Outcome::RequestHostConfirmation {
                confirmation_id: Id::parse("confirm.mouse")
                    .unwrap_or_else(|error| panic!("confirmation: {error}")),
                title: "Confirm provider action".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: false,
                continuation_schema: Vec::new(),
            },
            now_epoch: 1,
        })))
        .unwrap_or_else(|error| panic!("provider confirmation: {error}"))
        .next_state
}

#[test]
fn help_owns_click_wheel_drag_and_release_before_hidden_panels() {
    let state = crate::test_app_state()
        .apply(AppEvent::OpenHelp)
        .committed_pure();

    assert_overlay_consumes_without_panel_mutation(state);
}

#[test]
fn generic_confirmation_owns_click_wheel_drag_and_release_before_hidden_panels() {
    let state = crate::test_app_state()
        .apply(AppEvent::OpenDeleteRepository(jefe::domain::RepositoryId(
            "repo".to_owned(),
        )))
        .committed_pure();

    assert_overlay_consumes_without_panel_mutation(state);
}

#[test]
fn provider_confirmation_owns_mouse_without_staging_hidden_panel_effects() {
    assert_overlay_consumes_without_panel_mutation(provider_confirmation_state());
}

#[test]
fn a_screen_without_a_blocking_overlay_does_not_consume_panel_mouse_input() {
    let mut state = crate::test_app_state();

    for event in EVENTS {
        assert!(!consume_blocking_overlay_mouse(&mut state, event, 120, 40));
    }
}
