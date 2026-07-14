//! Issue #265: Send-to-Agent reducer tests for repository-scoped agent
//! eligibility and the `No agents available` notice.
//!
//! Extracted from `issues_tests_detail_flow.rs` to keep that file under the
//! source-file-size hard limit.

use crate::domain::{
    Agent, AgentChooserEntry, AgentChooserGitMetadata, AgentId, AgentKind, IssueDetail, IssueState,
    Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{
    AgentChooserState, ComposerTarget, DetailSubfocus, EditorTarget, InlineState,
};

fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode)
}

fn issue_detail() -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        node_id: String::new(),
        title: "T".to_string(),
        state: IssueState::Open,
        author_login: "a".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        issue_type_name: None,
        body: String::new(),
        external_url: String::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    }
}

/// Borrow the open agent chooser from issue state, panicking with a clear
/// diagnostic when the chooser is unexpectedly absent.
fn issues_chooser(state: &AppState) -> &AgentChooserState {
    let Some(chooser) = state.issues_state.agent_chooser.as_ref() else {
        panic!(
            "expected agent_chooser to be open, but it was None; \
             draft_notice: {:?}",
            state.issues_state.draft_notice
        );
    };
    chooser
}

/// Issue #265: an agent that belongs to a DIFFERENT repository than the
/// selected one must NOT appear in the chooser. The app_input layer filters
/// by repository, so passing an empty entries list must surface the
/// `No agents available` notice and leave the chooser closed.
#[test]
fn test_send_to_agent_agent_in_different_repository() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut agent = Agent::new(
        AgentId("agent-other".to_string()),
        RepositoryId("repo-2".to_string()),
        "Other Repo Agent".to_string(),
        std::path::PathBuf::from("/tmp/a-other"),
    );
    agent.agent_kind = AgentKind::Llxprt;
    state.agents.push(agent);

    let state = state.apply(AppEvent::OpenAgentChooser { metadata: vec![] });

    assert!(
        state.issues_state.agent_chooser.is_none(),
        "agent from another repo must not open the chooser"
    );
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("No agents available"),
        "no eligible agents for the selected repo must set the notice"
    );
}

/// Issue #265: when an eligible agent exists AND there is a stale notice,
/// the reducer must clear the notice and open the chooser.
#[test]
fn test_send_to_agent_eligible_clears_stale_notice_and_opens_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut agent = Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "My Agent".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.agent_kind = AgentKind::Llxprt;
    state.agents.push(agent);
    state.issues_state.draft_notice = Some("No agents available".to_string());

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "agent-1".to_string(),
    ))];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });

    assert!(
        state.issues_state.agent_chooser.is_some(),
        "eligible agent must open the chooser"
    );
    assert!(
        state.issues_state.draft_notice.is_none(),
        "eligible agent must clear the stale notice"
    );
}

/// Issue #265: the no-eligible-agent path must clear any stale chooser that
/// was left open from a prior eligible state (defensive cleanup).
#[test]
fn test_send_to_agent_no_eligible_clears_stale_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        agents: vec![AgentChooserEntry::simple("stale", "Stale")],
    });

    let state = state.apply(AppEvent::OpenAgentChooser { metadata: vec![] });

    assert!(
        state.issues_state.agent_chooser.is_none(),
        "stale chooser must be cleared when no eligible agents exist"
    );
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("No agents available"),
        "no eligible agents must set the notice"
    );
}

// ─── Issue #265 remediation: clear stale notice on unrelated transitions ────
//
// A stale `No agents available` notice must not linger when the user
// transitions away from the send-to-agent context. The reducer must clear
// `draft_notice` on RefocusIssueList and when opening a new inline
// composer/editor — without touching the real `error` field.

/// RefocusIssueList must clear a stale `draft_notice` so the notice does not
/// persist into the list view (issue #265).
#[test]
fn refocus_issue_list_clears_stale_draft_notice() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.draft_notice = Some("No agents available".to_string());

    let state = state.apply(AppEvent::RefocusIssueList);

    assert!(
        state.issues_state.draft_notice.is_none(),
        "RefocusIssueList must clear a stale draft_notice"
    );
    assert_eq!(
        state.issues_state.issue_focus,
        crate::state::types::IssueFocus::IssueList,
        "RefocusIssueList must still set the focus"
    );
}

/// RefocusIssueList must NOT clear a real `error` (only the non-blocking
/// `draft_notice` is transient).
#[test]
fn refocus_issue_list_preserves_real_error() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.error = Some("load failed".to_string());

    let state = state.apply(AppEvent::RefocusIssueList);

    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("load failed"),
        "RefocusIssueList must not erase a real error"
    );
}

/// Opening a new issue composer must clear a stale `draft_notice` so the
/// composer view does not show a stale no-agent banner (issue #265).
#[test]
fn open_new_issue_composer_clears_stale_draft_notice() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.draft_notice = Some("No agents available".to_string());
    // Seed a real error to prove the transition does not erase it.
    state.issues_state.error = Some("load failed".to_string());

    let state = state.apply(AppEvent::OpenNewIssueComposer);

    assert!(
        state.issues_state.draft_notice.is_none(),
        "OpenNewIssueComposer must clear a stale draft_notice"
    );
    assert!(
        matches!(
            state.issues_state.inline_state,
            crate::state::types::InlineState::Composer { .. }
        ),
        "OpenNewIssueComposer must still open the composer"
    );
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("load failed"),
        "OpenNewIssueComposer must preserve a real error"
    );
}

/// Opening a new comment composer must clear a stale `draft_notice`.
#[test]
fn open_new_comment_composer_clears_stale_draft_notice() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.issue_detail = Some(issue_detail());
    state.issues_state.draft_notice = Some("No agents available".to_string());
    // Seed a real error to prove the transition does not erase it.
    state.issues_state.error = Some("load failed".to_string());

    let state = state.apply(AppEvent::OpenNewCommentComposer);

    assert!(
        state.issues_state.draft_notice.is_none(),
        "OpenNewCommentComposer must clear a stale draft_notice"
    );
    assert!(
        matches!(
            state.issues_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "OpenNewCommentComposer must open the new-comment composer"
    );
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::NewComment,
        "OpenNewCommentComposer must focus the new-comment composer"
    );
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("load failed"),
        "OpenNewCommentComposer must preserve a real error"
    );
}

/// Opening the inline editor must clear a stale `draft_notice`.
#[test]
fn open_inline_editor_clears_stale_draft_notice() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.issues_state.issue_detail = Some(issue_detail());
    state.issues_state.draft_notice = Some("No agents available".to_string());
    // Seed a real error to prove the transition does not erase it.
    state.issues_state.error = Some("load failed".to_string());

    let state = state.apply(AppEvent::OpenInlineEditor {
        target: crate::state::types::EditorTarget::IssueBody,
    });

    assert!(
        state.issues_state.draft_notice.is_none(),
        "OpenInlineEditor must clear a stale draft_notice"
    );
    assert!(
        matches!(
            state.issues_state.inline_state,
            InlineState::Editor {
                target: EditorTarget::IssueBody,
                ..
            }
        ),
        "OpenInlineEditor must open the requested issue-body editor"
    );
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("load failed"),
        "OpenInlineEditor must preserve a real error"
    );
}

#[test]
fn blocked_inline_opens_preserve_draft_notice_and_active_editor() {
    let events = [
        AppEvent::OpenNewIssueComposer,
        AppEvent::OpenNewCommentComposer,
        AppEvent::OpenReplyComposer { comment_index: 0 },
        AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody,
        },
    ];

    for event in events {
        let mut state = issues_mode_state_with_repo("repo-1");
        state.issues_state.draft_notice = Some("No agents available".to_string());
        state.issues_state.inline_state = InlineState::Editor {
            target: EditorTarget::Comment { comment_index: 0 },
            text: "existing draft".to_string(),
            cursor: 14,
        };

        let state = state.apply(event);

        assert_eq!(
            state.issues_state.draft_notice.as_deref(),
            Some("No agents available"),
        );
        assert!(matches!(
            state.issues_state.inline_state,
            InlineState::Editor {
                target: EditorTarget::Comment { comment_index: 0 },
                ref text,
                cursor: 14,
            } if text == "existing draft"
        ));
    }
}

// ─── Issue #230: Metadata rejection, confirm revalidation, send targets ─────
//
// The reducer rebuilds chooser entries from current AppState. Metadata for
// agents that are not currently eligible (running, cross-repo, unavailable
// kind, stale/removed) must be silently dropped. The chooser never trusts
// injected identity — only git display metadata is joined, and only for
// agents the deterministic selector deems eligible.

/// Metadata for a cross-repository agent must be dropped: the chooser only
/// shows agents in the currently selected repository.
#[test]
fn metadata_cross_repo_agent_dropped_from_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    // Agent in repo-1 (eligible).
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "My Agent".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    // Metadata includes a cross-repo agent_id that is NOT in state.
    let metadata = vec![
        AgentChooserGitMetadata::for_agent(AgentId("agent-1".to_string())),
        AgentChooserGitMetadata {
            agent_id: AgentId("injected-cross-repo".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
    ];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the eligible agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "cross-repo metadata must be dropped"
    );
    assert_eq!(chooser.agents[0].agent_id, AgentId("agent-1".to_string()));
}

/// Metadata for a running agent must be dropped: running agents are excluded
/// by the selector even if their metadata was captured before they started.
#[test]
fn metadata_running_agent_dropped_from_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut running_agent = Agent::new(
        AgentId("running-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Running Agent".to_string(),
        std::path::PathBuf::from("/tmp/running"),
    );
    running_agent.status = crate::domain::AgentStatus::Running;
    state.agents.push(running_agent);
    // An eligible idle agent so the chooser opens.
    state.agents.push(Agent::new(
        AgentId("idle-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Idle Agent".to_string(),
        std::path::PathBuf::from("/tmp/idle"),
    ));

    let metadata = vec![
        AgentChooserGitMetadata {
            agent_id: AgentId("running-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        AgentChooserGitMetadata::for_agent(AgentId("idle-agent".to_string())),
    ];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let chooser = state
        .issues_state
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
        AgentId("idle-agent".to_string())
    );
}

/// Metadata for an agent whose kind is not installed (unavailable) must be
/// dropped: the selector excludes agents whose runtime kind is not in the
/// installed snapshot (unless the repo is remote-enabled).
#[test]
fn metadata_unavailable_kind_agent_dropped_from_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    // Only LLxprt is installed.
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    // A Code Puppy agent (kind not installed) and an LLxprt agent.
    let mut puppy_agent = Agent::new(
        AgentId("puppy-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Puppy Agent".to_string(),
        std::path::PathBuf::from("/tmp/puppy"),
    );
    puppy_agent.agent_kind = AgentKind::CodePuppy;
    state.agents.push(puppy_agent);
    state.agents.push(Agent::new(
        AgentId("llxprt-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "LLxprt Agent".to_string(),
        std::path::PathBuf::from("/tmp/llxprt"),
    ));

    let metadata = vec![
        AgentChooserGitMetadata {
            agent_id: AgentId("puppy-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        AgentChooserGitMetadata::for_agent(AgentId("llxprt-agent".to_string())),
    ];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the LLxprt agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "unavailable-kind agent metadata must be dropped"
    );
    assert_eq!(
        chooser.agents[0].agent_id,
        AgentId("llxprt-agent".to_string())
    );
}

/// Metadata for an agent_id that no longer exists in state (stale/removed)
/// must be dropped: the reducer rebuilds identity from current state.
#[test]
fn metadata_stale_removed_agent_dropped_from_chooser() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("current-agent".to_string()),
        RepositoryId("repo-1".to_string()),
        "Current Agent".to_string(),
        std::path::PathBuf::from("/tmp/current"),
    ));

    // Metadata includes a removed agent_id.
    let metadata = vec![
        AgentChooserGitMetadata {
            agent_id: AgentId("removed-agent".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        AgentChooserGitMetadata::for_agent(AgentId("current-agent".to_string())),
    ];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open for the current agent"));
    assert_eq!(
        chooser.agents.len(),
        1,
        "stale/removed agent metadata must be dropped"
    );
    assert_eq!(
        chooser.agents[0].agent_id,
        AgentId("current-agent".to_string())
    );
}

/// Injected metadata that tries to override identity (e.g. a different name)
/// must be ignored: the reducer rebuilds name/kind/config from AppState, NOT
/// from metadata. Metadata only carries branch + dirty.
#[test]
fn metadata_cannot_override_identity() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut agent = Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Real Name".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = "real-profile".to_string();
    state.agents.push(agent);

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "agent-1".to_string(),
    ))];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser must open"));
    // Identity must come from state, not metadata.
    assert_eq!(chooser.agents[0].name, "Real Name");
    assert_eq!(chooser.agents[0].kind, AgentKind::Llxprt);
    assert_eq!(chooser.agents[0].runtime_config.value, "real-profile");
}

/// Navigating to a nonzero chooser index selects the correct AgentId. The
/// send target is the entry at the navigated index, proving the chooser
/// supports multi-agent selection.
#[test]
fn nonzero_chooser_index_selects_correct_agent_id() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("agent-alpha".to_string()),
        RepositoryId("repo-1".to_string()),
        "Alpha".to_string(),
        std::path::PathBuf::from("/tmp/alpha"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-beta".to_string()),
        RepositoryId("repo-1".to_string()),
        "Beta".to_string(),
        std::path::PathBuf::from("/tmp/beta"),
    ));

    let metadata = vec![
        AgentChooserGitMetadata::for_agent(AgentId("agent-alpha".to_string())),
        AgentChooserGitMetadata::for_agent(AgentId("agent-beta".to_string())),
    ];
    let mut state = state.apply(AppEvent::OpenAgentChooser { metadata });
    assert_eq!(issues_chooser(&state).selected_index, 0);

    // Navigate down to index 1.
    state = state.apply(AppEvent::AgentChooserNavigateDown);
    let chooser = issues_chooser(&state);
    assert_eq!(chooser.selected_index, 1);
    assert_eq!(
        chooser.agents[1].agent_id,
        AgentId("agent-beta".to_string())
    );
    assert_eq!(chooser.agents[1].name, "Beta");
}

/// NavigateDown must not advance past the last entry.
#[test]
fn navigate_down_clamps_at_last_entry() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    state.agents.push(Agent::new(
        AgentId("a2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        std::path::PathBuf::from("/tmp/a2"),
    ));

    let metadata = vec![
        AgentChooserGitMetadata::for_agent(AgentId("a1".to_string())),
        AgentChooserGitMetadata::for_agent(AgentId("a2".to_string())),
    ];
    let mut state = state.apply(AppEvent::OpenAgentChooser { metadata });
    state = state.apply(AppEvent::AgentChooserNavigateDown);
    assert_eq!(issues_chooser(&state).selected_index, 1);
    // Try to advance past the end — should stay at 1.
    state = state.apply(AppEvent::AgentChooserNavigateDown);
    assert_eq!(issues_chooser(&state).selected_index, 1);
}

/// Confirm closes the chooser. After confirm, the selected entry's AgentId
/// is the one at the navigated index (the reducer closes state; the dispatch
/// layer reads the selected AgentId before closing).
#[test]
fn confirm_closes_chooser_after_nonzero_navigation() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    state.agents.push(Agent::new(
        AgentId("a2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        std::path::PathBuf::from("/tmp/a2"),
    ));

    let metadata = vec![
        AgentChooserGitMetadata::for_agent(AgentId("a1".to_string())),
        AgentChooserGitMetadata::for_agent(AgentId("a2".to_string())),
    ];
    let mut state = state.apply(AppEvent::OpenAgentChooser { metadata });
    state = state.apply(AppEvent::AgentChooserNavigateDown);
    // Before confirm, index is 1.
    assert_eq!(issues_chooser(&state).selected_index, 1);
    // Confirm closes the chooser.
    state = state.apply(AppEvent::AgentChooserConfirm);
    assert!(state.issues_state.agent_chooser.is_none());
}

/// Metadata with branch and dirty status is joined into the chooser entry
/// for eligible agents. Proves the git display info propagates.
#[test]
fn metadata_branch_and_dirty_joined_for_eligible_agent() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));

    let metadata = vec![AgentChooserGitMetadata {
        agent_id: AgentId("a1".to_string()),
        branch: Some("feature".to_string()),
        dirty: crate::domain::DirtyStatus::dirty(),
    }];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let entry = &issues_chooser(&state).agents[0];
    assert_eq!(entry.branch.as_deref(), Some("feature"));
    assert!(entry.dirty.is_dirty());
}

/// When metadata has no matching AgentId for an eligible agent, the entry
/// gets unknown dirty status and no branch (default display).
#[test]
fn no_matching_metadata_gives_unknown_dirty_and_no_branch() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    state.agents.push(Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));

    // Pass empty metadata — the eligible agent should still appear with
    // unknown dirty and no branch.
    let state = state.apply(AppEvent::OpenAgentChooser { metadata: vec![] });
    let entry = &issues_chooser(&state).agents[0];
    assert!(entry.branch.is_none());
    assert_eq!(entry.dirty, crate::domain::DirtyStatus::unknown());
}

/// Event/message round-trip: OpenAgentChooser metadata survives the
/// AppEvent→IssuesMessage→AppEvent conversion.
#[test]
fn open_agent_chooser_metadata_survives_message_round_trip() {
    use crate::messages::AppMessage;
    let metadata = vec![
        AgentChooserGitMetadata {
            agent_id: AgentId("a1".to_string()),
            branch: Some("main".to_string()),
            dirty: crate::domain::DirtyStatus::dirty(),
        },
        AgentChooserGitMetadata::for_agent(AgentId("a2".to_string())),
    ];
    let event = AppEvent::OpenAgentChooser {
        metadata: metadata.clone(),
    };
    let message: AppMessage = event.into();
    let round_trip: AppEvent = message.into();
    match round_trip {
        AppEvent::OpenAgentChooser { metadata: rt_md } => {
            assert_eq!(rt_md, metadata);
        }
        _ => panic!("round-trip must produce OpenAgentChooser"),
    }
}

/// Empty agent value with nonempty repository defaults: the chooser shows
/// the agent's own (empty) config, NOT the repository default. Proves the
/// selector reports the raw agent field without fallback.
#[test]
fn empty_agent_value_not_replaced_by_repo_default() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut agent = Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = String::new(); // empty agent profile
    state.agents.push(agent);
    // Set a nonempty repo default profile.
    if let Some(repo) = state.repositories.get_mut(0) {
        repo.default_profile = "repo-default-profile".to_string();
    }

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "a1".to_string(),
    ))];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let entry = &issues_chooser(&state).agents[0];
    // The agent's own empty value is preserved, NOT the repo default.
    assert!(
        entry.runtime_config.value.is_empty(),
        "empty agent profile must not be replaced by repo default"
    );
}

/// Agent config preserved exactly: a nonempty agent profile is used even
/// when the repo has a different default.
#[test]
fn agent_config_preserved_exactly_not_repo_fallback() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    let mut agent = Agent::new(
        AgentId("a1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    );
    agent.profile = "agent-profile".to_string();
    state.agents.push(agent);
    if let Some(repo) = state.repositories.get_mut(0) {
        repo.default_profile = "repo-default".to_string();
    }

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "a1".to_string(),
    ))];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
    let entry = &issues_chooser(&state).agents[0];
    assert_eq!(entry.runtime_config.value, "agent-profile");
}
