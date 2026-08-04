//! Issue #633: Send-to-Agent must not be refused while the startup agent
//! availability probe is still in flight (or after it failed).
//!
//! `available_agent_type_ids` is populated only once the async startup probe
//! reports `InstalledCompatible`. On Windows the `llxprt` npm shim takes
//! seconds to answer, so for the first part of every session the list is
//! empty — and it stays empty for the whole session when the probe times out.
//! Gating the agent chooser on that list makes `Shift+S` silently refuse.
//!
//! This mirrors the launch-admission precedent from issues #587/#553/#575:
//! a startup verdict cannot outlive the startup that produced it.

use crate::agent_candidate::CandidateResolution;
use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::agent_definition::{AgentDefinition, Availability, ProbeErrorCode};
use crate::domain::{Agent, AgentId, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;

/// The shipped definition backing [`crate::domain::shipped_agent_type(3)`],
/// which every fixture in this module uses as the agent type under test.
fn definition_under_test() -> AgentDefinition {
    let type_id = crate::domain::shipped_agent_type(3);
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == type_id)
        .unwrap_or_else(|| panic!("shipped_agent_type(3) must have a shipped definition"))
}

/// A repository with one non-running agent of the type under test, in issues
/// mode, with `available_agent_type_ids` deliberately EMPTY (the probe has not
/// reported a compatible verdict yet).
fn state_with_unprobed_agent() -> AppState {
    let mut state = AppState::default();
    let mut repository = Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Test Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/test"),
    );
    repository.github_repo = "owner/repo".to_string();
    state.repositories.push(repository);
    state.selected_repository_index = Some(0);

    let mut agent = Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "My Agent".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.type_id = crate::domain::shipped_agent_type(3);
    state.agents.push(agent);

    state.apply(AppEvent::EnterIssuesMode).committed_pure()
}

/// An observation for a definition whose process probe has been dispatched but
/// has not answered yet (`pending_generation` is set).
fn pending_observation() -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::pending(
        &definition_under_test(),
        true,
        1,
        CandidateResolution::NotFound(Vec::new()),
    )
}

/// An observation whose probe failed — e.g. the Windows npm shim exceeded the
/// probe timeout. The executable was still resolved; only the interrogation
/// failed.
fn probe_error_observation() -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::new(
        &definition_under_test(),
        true,
        Availability::ProbeError {
            code: ProbeErrorCode::Agte201,
            reason: "probe timed out".to_string(),
            generation: 1,
        },
    )
}

/// An observation that definitively established the executable is absent.
fn not_found_observation() -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::not_found(&definition_under_test(), true, 1)
}

/// An observation from a probe that answered successfully — the verdict
/// production also records by adding the type to `available_agent_type_ids`.
fn installed_compatible_observation() -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::new(
        &definition_under_test(),
        true,
        Availability::InstalledCompatible {
            identity: "0.10.0".to_string(),
            capabilities: vec!["prompt-interactive".to_string()],
            generation: 1,
        },
    )
}

// ── Selector: chooser eligibility must survive an unfinished probe ──────────

#[test]
fn chooser_includes_agent_while_startup_probe_is_pending() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![pending_observation()];

    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());

    assert_eq!(
        infos.len(),
        1,
        "an agent whose type probe is still in flight must remain eligible; \
         a pending verdict is not a negative verdict"
    );
}

#[test]
fn chooser_includes_agent_after_probe_error() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![probe_error_observation()];

    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());

    assert_eq!(
        infos.len(),
        1,
        "a failed probe must not permanently disable send-to-agent for the session"
    );
}

/// The widening must not cost the case it was built on top of: a probe that
/// answered `InstalledCompatible` still makes the agent eligible. Guards
/// against a future rewrite that folds success into the negative branch.
#[test]
fn chooser_includes_agent_after_a_successful_probe() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![installed_compatible_observation()];
    // Production records the positive verdict in both places.
    state.available_agent_type_ids = vec![crate::domain::shipped_agent_type(3)];

    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());

    assert_eq!(
        infos.len(),
        1,
        "a successfully probed agent type must stay eligible"
    );
    assert!(
        state.is_transient_available_for_repo(repo_id.as_ref()),
        "a successfully probed default type must still allow a transient agent"
    );
}

#[test]
fn chooser_excludes_agent_when_probe_proved_not_found() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![not_found_observation()];

    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());

    assert!(
        infos.is_empty(),
        "a definitive NotFound verdict must still exclude the agent"
    );
}

#[test]
fn chooser_excludes_agent_with_no_observation_at_all() {
    let state = state_with_unprobed_agent();

    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());

    assert!(
        infos.is_empty(),
        "without any observation there is no evidence the type exists"
    );
}

// ── Selector: transient availability must survive an unfinished probe ───────

#[test]
fn transient_available_while_startup_probe_is_pending() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![pending_observation()];

    let repo_id = state.selected_repository_id().cloned();
    assert!(
        state.is_transient_available_for_repo(repo_id.as_ref()),
        "a pending probe must not hide the transient-agent chooser row"
    );
}

#[test]
fn transient_unavailable_when_probe_proved_not_found() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![not_found_observation()];

    let repo_id = state.selected_repository_id().cloned();
    assert!(
        !state.is_transient_available_for_repo(repo_id.as_ref()),
        "a definitive NotFound verdict must still hide the transient row"
    );
}

// ── Reducer: the chooser actually opens ────────────────────────────────────

#[test]
fn open_agent_chooser_opens_while_startup_probe_is_pending() {
    let mut state = state_with_unprobed_agent();
    state.agent_type_availability = vec![pending_observation()];

    let state = state
        .apply(AppEvent::OpenAgentChooser { metadata: vec![] })
        .committed_pure();

    assert!(
        state.issues_state.agent_chooser.is_some(),
        "Shift+S during the startup probe window must open the chooser, \
         got notice {:?}",
        state.issues_state.draft_notice
    );
}

// ── Visibility: a refusal must reach the screen banner, not just the status bar ──

#[test]
fn unavailable_action_sets_the_issues_banner_notice() {
    let state = state_with_unprobed_agent();
    let mut state = state;
    state.record_unavailable_action("No agents available".to_string());

    assert_eq!(
        state.warning_message.as_deref(),
        Some("No agents available"),
        "the status-bar warning must still be set"
    );
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("No agents available"),
        "on the Issues screen the refusal must also surface in the banner so \
         the key press is not silently swallowed"
    );
}

#[test]
fn unavailable_action_sets_the_pull_requests_banner_notice() {
    let mut state = state_with_unprobed_agent();
    state.nav =
        crate::state::navigation::NavState::rooted(crate::workbench::ScreenId::PullRequests);

    state.record_unavailable_action("No agents available".to_string());

    assert_eq!(
        state.prs_state.draft_notice.as_deref(),
        Some("No agents available"),
        "the PR screen owns its own notice band and must show the refusal there"
    );
    assert_eq!(
        state.issues_state.draft_notice, None,
        "a refusal on the PR screen must not leak into the Issues banner"
    );
}

#[test]
fn unavailable_action_on_other_screens_only_warns() {
    // Screens without a notice band still surface the refusal in the status
    // bar, and must not write a notice into a screen state they are not
    // showing — a notice written there would appear later, out of context.
    for screen in [
        crate::workbench::ScreenId::Dashboard,
        crate::workbench::ScreenId::Repositories,
        crate::workbench::ScreenId::Actions,
        crate::workbench::ScreenId::Errors,
        crate::workbench::ScreenId::Terminals,
    ] {
        let mut state = state_with_unprobed_agent();
        state.nav = crate::state::navigation::NavState::rooted(screen);

        state.record_unavailable_action("Nothing to do here".to_string());

        assert_eq!(
            state.warning_message.as_deref(),
            Some("Nothing to do here"),
            "{screen:?} must still surface the refusal in the status bar"
        );
        assert_eq!(
            state.issues_state.draft_notice, None,
            "{screen:?} must not write an Issues notice"
        );
        assert_eq!(
            state.prs_state.draft_notice, None,
            "{screen:?} must not write a PR notice"
        );
    }
}
