//! Issue #265: Send-to-Agent reducer tests for repository-scoped agent
//! eligibility and the `No agents available` notice.
//!
//! Extracted from `issues_tests_detail_flow.rs` to keep that file under the
//! source-file-size hard limit.

use crate::domain::{Agent, AgentId, AgentKind, Repository, RepositoryId};
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

/// Issue #265: an agent that belongs to a DIFFERENT repository than the
/// selected one must NOT appear in the chooser. The reducer must surface the
/// `No agents available` notice and leave the chooser closed.
#[test]
fn test_send_to_agent_agent_in_different_repository() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.agents.push(Agent::new(
        AgentId("agent-other".to_string()),
        RepositoryId("repo-2".to_string()),
        "Other Repo Agent".to_string(),
        std::path::PathBuf::from("/tmp/a-other"),
    ));

    let state = state.apply(AppEvent::OpenAgentChooser);

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

    let state = state.apply(AppEvent::OpenAgentChooser);

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
        agents: vec![(AgentId("stale".to_string()), "Stale".to_string())],
    });

    let state = state.apply(AppEvent::OpenAgentChooser);

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
    // Need an issue detail to open a comment composer against.
    state.issues_state.issue_detail = Some(crate::domain::IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        node_id: String::new(),
        title: "T".to_string(),
        state: crate::domain::IssueState::Open,
        author_login: "a".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: String::new(),
        external_url: String::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    });
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
    state.issues_state.issue_detail = Some(crate::domain::IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        node_id: String::new(),
        title: "T".to_string(),
        state: crate::domain::IssueState::Open,
        author_login: "a".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: String::new(),
        external_url: String::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    });
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
