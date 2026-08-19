//! Production Back commit regressions for reload and post-lock effect handoff.

use jefe::domain::effects::{Effect, ProviderEffect};
use jefe::domain::{Id, TypedMap};
use jefe::runtime::provider::protocol::BodyKind;
use jefe::state::provider_panels::DeclareInput;
use jefe::state::{AppEvent, AppState, IssueFocus, PrFocus};
use jefe::workbench::PanelId;

use super::commit_back;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("test identifier must be valid: {error:?}"))
}

fn apply(state: &mut AppState, event: AppEvent) {
    *state = state
        .clone()
        .apply(event)
        .unwrap_or_else(|error| panic!("test transition must commit: {error:?}"))
        .next_state;
}

#[test]
fn production_back_commit_requests_issue_reload_only_after_detail_refocus() {
    let mut state = crate::test_app_state();
    apply(&mut state, AppEvent::EnterIssuesMode);
    state.issues_state.issue_focus = IssueFocus::IssueDetail;

    let (effects, reload_issues, reload_prs) = commit_back(&mut state);

    assert!(effects.is_empty());
    assert!(reload_issues);
    assert!(!reload_prs);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);
}

#[test]
fn production_back_commit_requests_pr_reload_only_after_detail_refocus() {
    let mut state = crate::test_app_state();
    apply(&mut state, AppEvent::EnterPrsMode);
    state.prs_state.pr_focus = PrFocus::PrDetail;

    let (effects, reload_issues, reload_prs) = commit_back(&mut state);

    assert!(effects.is_empty());
    assert!(!reload_issues);
    assert!(reload_prs);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);
}

#[test]
fn production_back_commit_returns_provider_deactivation_for_post_lock_dispatch() {
    let mut state = crate::test_app_state();
    apply(&mut state, AppEvent::EnterIssuesMode);
    let current = state.nav.current().id.get();
    let owner = id("vendor.pkg");
    let panel_type = id("vendor.panel");
    let panel_id = PanelId::from_static("main");
    let activation = TypedMap::new();
    let declared = state
        .provider_panels
        .declare(DeclareInput {
            owner: &owner,
            panel_id: &panel_id,
            screen_instance_id: current,
            panel_type: &panel_type,
            activation: &activation,
            allowed_model_kinds: &[BodyKind::List],
            allowed_events: &[],
            action_authority: &[],
            process_generation: 1,
        })
        .unwrap_or_else(|error| panic!("provider panel declaration must succeed: {error:?}"));
    state
        .provider_panels
        .activate(declared.instance)
        .unwrap_or_else(|error| panic!("provider panel activation must succeed: {error:?}"));

    let (effects, reload_issues, reload_prs) = commit_back(&mut state);

    assert!(!reload_issues);
    assert!(!reload_prs);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        effects[0].effect,
        Effect::Provider(ProviderEffect::DeactivatePanel { .. })
    ));
}
