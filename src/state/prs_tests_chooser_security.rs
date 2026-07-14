//! Issue #230: PR send-to-agent chooser security and remediation tests.
//!
//! Mirrors the issue-mode chooser tests: the PR reducer rebuilds chooser
//! entries from current `AppState`, so injected/cross-repo/running/
//! unavailable/stale metadata must be silently dropped. The chooser never
//! trusts injected identity — only git display metadata is joined, and only
//! for agents the deterministic selector deems eligible.

use super::prs_test_fixtures::prs_state_with_detail;
use crate::domain::RepositoryId;
use crate::state::events::AppEvent;

/// PR chooser: cross-repo metadata is dropped.
///
/// A genuine cross-repo agent exists in state under `repo-2` and has
/// matching metadata, so the only reason it is dropped is the repository
/// filter — not absence from state (which is covered by the stale/removed
/// test below).
#[test]
fn pr_metadata_cross_repo_agent_dropped_from_chooser() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    // Eligible agent in the selected repository (repo-1).
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "My Agent".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    // A real agent that exists in state but belongs to a different repository.
    let mut other_repo_agent = crate::domain::Agent::new(
        crate::domain::AgentId("agent-other-repo".to_string()),
        RepositoryId("repo-2".to_string()),
        "Other Repo Agent".to_string(),
        std::path::PathBuf::from("/tmp/a-other"),
    );
    other_repo_agent.agent_kind = crate::domain::AgentKind::Llxprt;
    state.agents.push(other_repo_agent);
    let metadata = vec![
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "agent-1".to_string(),
        )),
        crate::domain::AgentChooserGitMetadata {
            agent_id: crate::domain::AgentId("agent-other-repo".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
    ];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the eligible agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "cross-repo metadata must be dropped"
    );
    assert_eq!(
        chooser.agents[0].agent_id,
        crate::domain::AgentId("agent-1".to_string()),
    );
}

/// PR chooser: running agent metadata is dropped.
#[test]
fn pr_metadata_running_agent_dropped_from_chooser() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let mut running = crate::domain::Agent::new(
        crate::domain::AgentId("running-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Running Agent".to_string(),
        std::path::PathBuf::from("/tmp/running"),
    );
    running.status = crate::domain::AgentStatus::Running;
    state.agents.push(running);
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("idle-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Idle Agent".to_string(),
        std::path::PathBuf::from("/tmp/idle"),
    ));

    let metadata = vec![
        crate::domain::AgentChooserGitMetadata {
            agent_id: crate::domain::AgentId("running-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "idle-agent".to_string(),
        )),
    ];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the idle agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "running agent metadata must be dropped"
    );
    assert_eq!(
        chooser.agents[0].agent_id,
        crate::domain::AgentId("idle-agent".to_string())
    );
}

/// PR chooser: unavailable-kind agent metadata is dropped.
#[test]
fn pr_metadata_unavailable_kind_agent_dropped_from_chooser() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let mut puppy = crate::domain::Agent::new(
        crate::domain::AgentId("puppy-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Puppy Agent".to_string(),
        std::path::PathBuf::from("/tmp/puppy"),
    );
    puppy.agent_kind = crate::domain::AgentKind::CodePuppy;
    state.agents.push(puppy);
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("llxprt-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "LLxprt Agent".to_string(),
        std::path::PathBuf::from("/tmp/llxprt"),
    ));

    let metadata = vec![
        crate::domain::AgentChooserGitMetadata {
            agent_id: crate::domain::AgentId("puppy-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "llxprt-agent".to_string(),
        )),
    ];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the LLxprt agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "unavailable-kind agent metadata must be dropped"
    );
}

/// PR chooser: stale/removed agent metadata is dropped.
#[test]
fn pr_metadata_stale_removed_agent_dropped_from_chooser() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("current-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Current Agent".to_string(),
        std::path::PathBuf::from("/tmp/current"),
    ));

    let metadata = vec![
        crate::domain::AgentChooserGitMetadata {
            agent_id: crate::domain::AgentId("removed-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "current-agent".to_string(),
        )),
    ];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the current agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "stale/removed agent metadata must be dropped"
    );
}

/// PR event/message round-trip: `PrOpenAgentChooser` metadata survives the
/// `AppEvent`→`AppMessage`→`AppEvent` conversion.
#[test]
fn pr_open_agent_chooser_metadata_survives_message_round_trip() {
    use crate::messages::AppMessage;
    let metadata = vec![
        crate::domain::AgentChooserGitMetadata {
            agent_id: crate::domain::AgentId("a1".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId("a2".to_string())),
    ];
    let event = AppEvent::PrOpenAgentChooser {
        metadata: metadata.clone(),
    };
    let message: AppMessage = event.into();
    let round_trip: AppEvent = message.into();
    match round_trip {
        AppEvent::PrOpenAgentChooser { metadata: rt_md } => {
            assert_eq!(rt_md, metadata);
        }
        _ => panic!("round-trip must produce PrOpenAgentChooser"),
    }
}

/// PR chooser: metadata with branch and dirty is joined for eligible agents.
#[test]
fn pr_metadata_branch_and_dirty_joined_for_eligible_agent() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));

    let metadata = vec![crate::domain::AgentChooserGitMetadata {
        agent_id: crate::domain::AgentId("a1".to_string()),
        branch: Some("feature".to_string()),
        dirty: crate::domain::DirtyStatus::dirty(),
    }];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let entry = &state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open"))
        .agents[0];
    assert_eq!(entry.branch.as_deref(), Some("feature"));
    assert!(entry.dirty.is_dirty());
}

/// PR chooser: nonzero index navigation selects the correct `AgentId`.
#[test]
fn pr_nonzero_chooser_index_selects_correct_agent_id() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-alpha".to_string()),
        RepositoryId("repo-1".to_string()),
        "Alpha".to_string(),
        std::path::PathBuf::from("/tmp/alpha"),
    ));
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-beta".to_string()),
        RepositoryId("repo-1".to_string()),
        "Beta".to_string(),
        std::path::PathBuf::from("/tmp/beta"),
    ));

    let metadata = vec![
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "agent-alpha".to_string(),
        )),
        crate::domain::AgentChooserGitMetadata::for_agent(crate::domain::AgentId(
            "agent-beta".to_string(),
        )),
    ];
    let mut state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    assert_eq!(
        state
            .prs_state
            .agent_chooser
            .as_ref()
            .unwrap_or_else(|| panic!("chooser must be open"))
            .selected_index,
        0
    );
    state = state.apply(AppEvent::PrAgentChooserNavigateDown);
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must remain open after navigation"));
    assert_eq!(chooser.selected_index, 1);
    assert_eq!(
        chooser.agents[1].agent_id,
        crate::domain::AgentId("agent-beta".to_string())
    );
}

/// PR chooser: injected metadata cannot override identity. The reducer
/// rebuilds name/kind/config from `AppState`, NOT from metadata. Metadata
/// only carries branch + dirty.
#[test]
fn pr_metadata_cannot_override_identity() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let mut agent = crate::domain::Agent::new(
        crate::domain::AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Real Name".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = "real-profile".to_string();
    state.agents.push(agent);

    let metadata = vec![crate::domain::AgentChooserGitMetadata::for_agent(
        crate::domain::AgentId("agent-1".to_string()),
    )];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let chooser = state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open"));
    assert_eq!(chooser.agents[0].name, "Real Name");
    assert_eq!(chooser.agents[0].kind, crate::domain::AgentKind::Llxprt);
    assert_eq!(chooser.agents[0].runtime_config.value, "real-profile");
}

/// PR chooser: when metadata has no matching `AgentId` for an eligible agent,
/// the entry gets unknown dirty status and no branch (default display).
#[test]
fn pr_no_metadata_gives_unknown_dirty_no_branch() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));

    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata: vec![] });
    let entry = &state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open with default metadata"))
        .agents[0];
    assert!(entry.branch.is_none());
    assert_eq!(entry.dirty, crate::domain::DirtyStatus::unknown());
}

/// PR chooser: empty agent value with nonempty repository defaults — the
/// chooser shows the agent's own (empty) config, NOT the repository default.
#[test]
fn pr_empty_agent_value_kept_not_repo_default() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let mut agent = crate::domain::Agent::new(
        crate::domain::AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = String::new();
    state.agents.push(agent);
    if let Some(repo) = state.repositories.get_mut(0) {
        repo.default_profile = "repo-default-profile".to_string();
    }

    let metadata = vec![crate::domain::AgentChooserGitMetadata::for_agent(
        crate::domain::AgentId("a1".to_string()),
    )];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let entry = &state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open"))
        .agents[0];
    assert!(
        entry.runtime_config.value.is_empty(),
        "empty agent profile must not be replaced by repo default"
    );
}

/// PR chooser: agent config preserved exactly — a nonempty agent profile is
/// used even when the repo has a different default.
#[test]
fn pr_agent_config_preserved_not_repo_fallback() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let mut agent = crate::domain::Agent::new(
        crate::domain::AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = "agent-profile".to_string();
    state.agents.push(agent);
    if let Some(repo) = state.repositories.get_mut(0) {
        repo.default_profile = "repo-default".to_string();
    }

    let metadata = vec![crate::domain::AgentChooserGitMetadata::for_agent(
        crate::domain::AgentId("a1".to_string()),
    )];
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    let entry = &state
        .prs_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open"))
        .agents[0];
    assert_eq!(entry.runtime_config.value, "agent-profile");
}

/// PR chooser stale state: when `PrOpenAgentChooser` recomputes no eligible
/// entries (e.g. the sole agent became running between open calls), the
/// chooser must be cleared before setting the notice — matching issues
/// behavior. A stale chooser left open would reference agents that are no
/// longer eligible.
#[test]
fn pr_open_chooser_clears_stale_chooser_when_no_eligible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.installed_agent_kinds = vec![crate::domain::AgentKind::Llxprt];
    let agent = crate::domain::Agent::new(
        crate::domain::AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    state.agents.push(agent);

    // Open the chooser: one eligible agent → chooser is open.
    let metadata = vec![crate::domain::AgentChooserGitMetadata::for_agent(
        crate::domain::AgentId("a1".to_string()),
    )];
    let mut state = state.apply(AppEvent::PrOpenAgentChooser { metadata });
    assert!(
        state.prs_state.agent_chooser.is_some(),
        "chooser must be open initially"
    );

    // Now the agent becomes running → no longer eligible.
    state.agents[0].status = crate::domain::AgentStatus::Running;

    // Reopen: no eligible agents → chooser must be cleared and notice set.
    let state = state.apply(AppEvent::PrOpenAgentChooser { metadata: vec![] });
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "stale chooser must be cleared when no agents are eligible"
    );
    assert_eq!(
        state.prs_state.draft_notice.as_deref(),
        Some("No agents available"),
        "notice must be set when chooser has no eligible entries"
    );
}
