//! PR-Mode integration tests — lifecycle checkpoints (extracted from prs_integration_tests.rs).
//!
//! Checkpoint 10 (Esc precedence chain) + Checkpoint 17 (persistence exclusion).
//!
//! @plan PLAN-20260624-PR-MODE.P15
//! @requirement REQ-PR-004
//! @requirement REQ-PR-NFR-003
//! @pseudocode component-001 lines 66-76
//! @pseudocode component-003 lines 92-98

use iocraft::prelude::KeyCode;
use jefe::domain::{Agent, AgentId, RepositoryId};
use jefe::persistence::State as PersistedState;
use jefe::state::{AppEvent, AppState, PrFocus, ScreenMode};
use std::path::PathBuf;

use super::prs_integration_tests::{
    ApplyInPlace, active_prs_state, dashboard_prs_state, key, make_test_pr, make_test_pr_detail,
};
use super::{prs, to_persisted_state};

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 10: full Esc precedence chain (REQ-PR-004)
// ═════════════════════════════════════════════════════════════════════════

/// Base state for the Esc-precedence chain: PR mode active with a loaded PR
/// list + detail, so every overlay layer can be layered on top in isolation.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn esc_chain_base_state() -> AppState {
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(make_test_pr_detail(1));
    state
}

/// Resolve Esc through the REAL key router and assert the emitted event
/// matches `expected_match` (a closure returning bool from the event), then
/// apply the event through the reducer and return the resulting state.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn resolve_esc_and_apply<F: Fn(&AppEvent) -> bool>(state: &mut AppState, matches_expected: F) {
    let event = prs::resolve_prs_key_event(state, &key(KeyCode::Esc))
        .unwrap_or_else(|| panic!("Esc at this precedence level must emit an event"));
    assert!(
        matches_expected(&event),
        "Esc emitted unexpected event: {event:?}"
    );
    state.apply_in_place(event);
}

/// L1: an active inline composer — Esc (via the REAL key router) emits
/// `PrInlineCancelOrEsc`; the composer closes and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,99-108
fn esc_l1_inline_composer_cancels() {
    use jefe::state::InlineState;
    let mut state = esc_chain_base_state();
    state.apply_in_place(AppEvent::PrOpenNewCommentComposer);
    assert!(matches!(
        state.prs_state.inline_state,
        InlineState::Composer { .. }
    ));
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrInlineCancelOrEsc));
    assert_eq!(state.prs_state.inline_state, InlineState::None);
    assert!(
        state.prs_state.active,
        "mode must stay active after composer Esc"
    );
}

/// L2: an open agent chooser — Esc (via the REAL key router) emits
/// `PrAgentChooserCancel`; the chooser closes and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,120-126
fn esc_l2_agent_chooser_cancels() {
    use jefe::state::AgentChooserState;
    let mut state = esc_chain_base_state();
    state.prs_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        agents: vec![],
    });
    resolve_esc_and_apply(&mut state, |ev| {
        matches!(ev, AppEvent::PrAgentChooserCancel)
    });
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "chooser must close after PrAgentChooserCancel"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after chooser Esc"
    );
}

/// L3a: search focused with a NON-EMPTY query — Esc (via the REAL key router)
/// emits `PrClearSearch`; the query clears and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,127-133
fn esc_l3a_search_nonempty_clears() {
    let mut state = esc_chain_base_state();
    state.prs_state.search_input_focused = true;
    state.prs_state.search_query = "auth".to_string();
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrClearSearch));
    assert!(
        state.prs_state.search_query.is_empty(),
        "PrClearSearch must clear the query"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after search-clear Esc"
    );
}

/// L3b: search focused with an EMPTY query — Esc (via the REAL key router)
/// emits `PrBlurSearchInput`; the input blurs and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,127-133
fn esc_l3b_search_empty_blurs() {
    let mut state = esc_chain_base_state();
    state.prs_state.search_input_focused = true;
    state.prs_state.search_query = String::new();
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrBlurSearchInput));
    assert!(
        !state.prs_state.search_input_focused,
        "PrBlurSearchInput must blur the search input"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after search-blur Esc"
    );
}

/// L4: open filter controls — Esc (via the REAL key router) emits
/// `PrCloseFilterControls`; the controls close and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,134-146
fn esc_l4_filter_controls_close() {
    let mut state = esc_chain_base_state();
    state.apply_in_place(AppEvent::PrOpenFilterControls);
    assert!(state.prs_state.filter_ui.controls_open);
    resolve_esc_and_apply(&mut state, |ev| {
        matches!(ev, AppEvent::PrCloseFilterControls)
    });
    assert!(
        !state.prs_state.filter_ui.controls_open,
        "filter controls must close after PrCloseFilterControls"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after filter Esc"
    );
}

/// L5: nothing open — Esc (via the REAL key router) from PrDetail focus emits
/// `RefocusPrList` (the detail pane is focused, so Esc returns to the list
/// rather than exiting the whole mode). A subsequent Esc from PrList focus
/// then emits `ExitPrsMode`; the mode becomes inactive and the screen returns
/// to the Dashboard. This mirrors issues-mode Esc semantics.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn esc_l5_nothing_open_exits() {
    let mut state = esc_chain_base_state();
    // PrDetail focus, nothing open => Esc refocuses to the list (not exit).
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::RefocusPrList));
    assert!(
        state.prs_state.active,
        "mode must stay active after Esc in PrDetail (refocus, not exit)"
    );
    // PrList focus, nothing open => Esc exits the mode.
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::ExitPrsMode));
    assert!(
        !state.prs_state.active,
        "mode must be inactive after final Esc"
    );
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

/// Checkpoint 10 (REQ-PR-004): Esc unwinds by the full 6-level precedence
/// chain — inline composer → agent chooser → search-clear → search-blur →
/// filter controls → refocus-list (from PrDetail) → exit mode. Each Esc peels
/// exactly one layer, leaving the mode active until the final (nothing-open,
/// PrList-focused) Esc exits.
///
/// Drives the REAL key router `prs::resolve_prs_key_event(&state,
/// &key(KeyCode::Esc))` at EACH precedence level (the genuine 8-level
/// resolver in `src/app_input/prs.rs`), asserting the EXACT emitted
/// `AppEvent` variant via `matches!`, THEN applying it through the reducer
/// (`AppState::apply`) and asserting the resulting state. Each level is
/// exercised in isolation from a fresh base state.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
#[test]
fn it_esc_precedence_unwinds_then_exits() {
    esc_l1_inline_composer_cancels();
    esc_l2_agent_chooser_cancels();
    esc_l3a_search_nonempty_clears();
    esc_l3b_search_empty_blurs();
    esc_l4_filter_controls_close();
    esc_l5_nothing_open_exits();
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 17: persisted state excludes prs_state (NFR-003)
// ═════════════════════════════════════════════════════════════════════════

/// Build an AppState with an ACTIVE, populated `prs_state` AND realistic
/// persisted fields (repositories, agents, selected indices,
/// hide_idle_repositories, last_selected_agent_by_repo), so the REAL
/// `to_persisted_state` mapper has non-trivial persisted data to copy while PR
/// data is simultaneously present (and must be excluded).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn state_with_active_prs_and_persisted_fields() -> AppState {
    let mut state = dashboard_prs_state();
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent One".to_string(),
        PathBuf::from("/tmp/repo-1"),
    ));
    state.selected_agent_index = Some(0);
    state.hide_idle_repositories = true;
    state.last_selected_agent_by_repo = vec![(
        RepositoryId("repo-1".to_string()),
        AgentId("agent-1".to_string()),
    )];

    // ACTIVE, populated prs_state (transient — must be excluded).
    state.apply_in_place(AppEvent::EnterPrsMode);
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(make_test_pr_detail(1));
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state
}

/// Assert the persisted DTO carries NO pull-request data (no prs_state /
/// pull_request / pr_detail / "pr_" keys), via a serde_json round-trip.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn assert_no_pr_data_in_persisted(persisted: &PersistedState) {
    let json = serde_json::to_value(persisted)
        .unwrap_or_else(|e| panic!("persisted should serialize: {e}"));
    let json_str =
        serde_json::to_string(&json).unwrap_or_else(|e| panic!("json should stringify: {e}"));
    let lower = json_str.to_lowercase();
    assert!(
        !lower.contains("prs_state")
            && !lower.contains("pull_request")
            && !lower.contains("pr_detail")
            && !lower.contains("\"pr_"),
        "persisted state must not carry any PR data, got: {json_str}"
    );
}

/// Assert the persisted DTO carries the same persisted-field values as the
/// source AppState (proving the REAL mapper copies them faithfully).
/// Repository/Agent do not derive `PartialEq`, so they are compared by
/// length + key fields (id/name).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn assert_persisted_fields_match_source(persisted: &PersistedState, state: &AppState) {
    assert_eq!(
        persisted.schema_version,
        jefe::persistence::STATE_SCHEMA_VERSION
    );
    assert_eq!(
        persisted.repositories.len(),
        state.repositories.len(),
        "repositories count must match source"
    );
    for (i, (pr, sr)) in persisted
        .repositories
        .iter()
        .zip(state.repositories.iter())
        .enumerate()
    {
        assert_eq!(pr.id, sr.id, "repository[{i}] id must match source");
        assert_eq!(pr.name, sr.name, "repository[{i}] name must match source");
    }
    assert_eq!(
        persisted.agents.len(),
        state.agents.len(),
        "agents count must match source"
    );
    for (i, (pa, sa)) in persisted.agents.iter().zip(state.agents.iter()).enumerate() {
        assert_eq!(pa.id, sa.id, "agent[{i}] id must match source");
    }
    assert_eq!(
        persisted.selected_repository_index,
        state.selected_repository_index
    );
    assert_eq!(persisted.selected_agent_index, state.selected_agent_index);
    assert_eq!(
        persisted.hide_idle_repositories,
        state.hide_idle_repositories
    );
    assert_eq!(
        persisted.last_selected_agent_by_repo,
        state.last_selected_agent_by_repo
    );
}

/// Checkpoint 17 (NFR-003): the REAL production mapper
/// `to_persisted_state(&state)` (`src/app_input/mod.rs`, the same fn the
/// sibling unit test `app_input_tests::test_to_persisted_state_excludes_prs_state`
/// exercises) must carry ONLY the persisted fields (schema_version,
/// repositories, agents, selected_repository_index, selected_agent_index,
/// hide_idle_repositories, last_selected_agent_by_repo) and NEVER any
/// `prs_state`/pull-request data (which is transient). This integration-level
/// variant is BROADER than the existing unit test: it drives the REAL mapper
/// against an AppState with BOTH an active, populated `prs_state` (active=true,
/// pull_requests non-empty, selected_pr_index Some, pr_detail Some, non-default
/// pr_focus) AND realistic persisted fields, then asserts (a) the persisted
/// fields equal the source AppState's values, (b) a serde_json round-trip of
/// the PersistedState carries NO PR data, and (c) a fresh `AppState::default()`
/// has `prs_state.active == false` and default `prs_state` (PR state is
/// transient and never rehydrated from the persisted form).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
#[test]
fn it_persisted_state_excludes_prs_state() {
    let state = state_with_active_prs_and_persisted_fields();
    // Precondition: PR mode is active and populated (meaningful test).
    assert!(state.prs_state.active);
    assert!(!state.prs_state.pull_requests.is_empty());

    // Drive the REAL production mapper (same path as app_input_tests.rs:284).
    let persisted = to_persisted_state(&state);

    // The REAL mapper copied the persisted fields faithfully (equal to source).
    assert_persisted_fields_match_source(&persisted, &state);

    // Structurally (via serde_json round-trip of the DTO) NO PR data is present.
    assert_no_pr_data_in_persisted(&persisted);

    // Round-trip: serialize → deserialize, and confirm the DTO still has no PR
    // fields, and a fresh AppState (the hydration baseline) has inactive /
    // default prs_state — proving PR state is transient.
    let json = serde_json::to_string(&persisted)
        .unwrap_or_else(|e| panic!("persisted should serialize: {e}"));
    let reloaded: PersistedState =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("persisted should deserialize: {e}"));
    assert_no_pr_data_in_persisted(&reloaded);

    let fresh = AppState::default();
    assert!(
        !fresh.prs_state.active,
        "fresh AppState prs_state must be inactive"
    );
    assert!(
        fresh.prs_state.pull_requests.is_empty()
            && fresh.prs_state.pr_detail.is_none()
            && fresh.prs_state.selected_pr_index.is_none(),
        "fresh AppState prs_state must be at defaults (transient, never rehydrated)"
    );
}
