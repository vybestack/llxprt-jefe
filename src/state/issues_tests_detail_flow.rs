use crate::domain::{
    Agent, AgentChooserEntry, AgentChooserGitMetadata, AgentId, Issue, IssueComment, IssueDetail,
    IssueFilter, IssueState, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{
    AgentChooserState, ComposerTarget, DetailSubfocus, EditorTarget, InlineState, PaneFocus,
    ScreenId,
};

use super::issues_test_fixtures::begin_issue_list_reload;
use crate::state::transition::TransitionExt;

fn dashboard_issues_state() -> AppState {
    AppState {
        screen: ScreenId::Issues,
        ..AppState::default()
    }
}

/// Helper to create a test issue with the given number.
fn make_test_issue(number: u64) -> Issue {
    Issue {
        number,
        node_id: String::new(),
        title: format!("Test Issue #{number}"),
        state: IssueState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
        created_at: String::new(),
        priority: None,
    }
}

/// Helper: create a state already in issues mode with a selected repository.
fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode).committed_pure()
}

/// Helper: create a minimal IssueDetail with given number and empty comments.
fn p15_detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        node_id: String::new(),
        title: format!("Issue #{number}"),
        state: IssueState::Open,
        author_login: "user".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Issue body".to_string(),
        external_url: format!("https://github.com/owner/repo/issues/{number}"),
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        issue_type_name: None,
        state_reason: None,
    }
}

fn p15_comment(comment_id: u64, author_login: &str, created_at: &str, body: &str) -> IssueComment {
    IssueComment {
        comment_id,
        author_login: author_login.to_string(),
        created_at: created_at.to_string(),
        edited_at: None,
        body: body.to_string(),
    }
}

fn p15_state_with_loaded_detail(repo_id: &RepositoryId, issue_number: u64) -> AppState {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), issue_number);
    state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number,
            request_id: 0,
            detail: Box::new(p15_detail(issue_number)),
        })
        .committed_pure()
}

fn state_with_repo_and_agent() -> AppState {
    let mut state = AppState {
        selected_repository_index: Some(0),
        available_agent_type_ids: vec![crate::domain::shipped_agent_type(3)],
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "My Agent".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    state
}

fn send_payload_detail() -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 7,
        node_id: String::new(),
        title: "Fix crash".to_string(),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec!["bug".to_string()],
        assignees: vec![],
        milestone: None,
        body: "Crash on startup".to_string(),
        external_url: "https://github.com/owner/repo/issues/7".to_string(),
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 7,
            },
            vec![
                p15_comment(100, "dev", "2024-01-02T00:00:00Z", "Reproduced on main"),
                p15_comment(101, "tester", "2024-01-03T00:00:00Z", "Also seen in v2.1"),
            ],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        issue_type_name: None,
        state_reason: None,
    }
}

#[test]
fn test_scope_reset_clears_pending_mutation_and_allows_new_inline_draft() {
    let repo_id = RepositoryId("repo-1".to_string());
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "submitted".to_string(),
        cursor: 9,
    };
    let mut state = issues_mode_state_with_repo("repo-1")
        .apply(AppEvent::MutationSubmitted {
            scope_repo_id: repo_id,
            mutation_id: 11,
            target: submitted_target,
        })
        .committed_pure();
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "newer draft".to_string(),
        cursor: 11,
    };

    let state = state.apply(AppEvent::ApplySearch).committed_pure();
    assert!(state.issues_state.mutation_pending.is_none());
    assert_eq!(state.issues_state.inline_state, InlineState::None);

    let state = state
        .apply(AppEvent::OpenNewCommentComposer)
        .committed_pure();
    assert!(matches!(
        state.issues_state.inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            ..
        }
    ));
}

#[test]
fn test_stale_create_issue_success_after_repo_change_does_not_clear_current_draft() {
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/repo2"),
    ));
    state.selected_repository_index = Some(0);
    let mut state = state
        .apply(AppEvent::MutationSubmitted {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: 12,
            target: submitted_target,
        })
        .committed_pure();
    state.selected_repository_index = Some(1);
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "new repo draft".to_string(),
        cursor: 14,
    };

    let state = state
        .apply(AppEvent::IssueCreated {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: 12,
            issue: Box::new(make_test_issue(1)),
        })
        .committed_pure();

    assert!(state.issues_state.mutation_pending.is_some());
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "new repo draft"),
        other => panic!("expected new repo draft to remain, got {other:?}"),
    }
    assert!(state.issues_state.draft_notice.is_none());
}

/// P15 Test 10: Enter issues, exit — prior agent focus is restored.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-005
#[test]
fn test_exit_focus_restoration_valid() {
    let mut state = AppState::default();

    // Set up repo + 2 agents
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    state.agents.push(Agent::new(
        AgentId("agent-0".to_string()),
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Agent 0".to_string(),
        std::path::PathBuf::from("/tmp/a0"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(1);

    // Enter issues mode — focus is saved
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert_eq!(state.screen, ScreenId::Issues);

    // Exit — prior focus restored
    let state = state.apply(AppEvent::ExitIssuesMode).committed_pure();
    assert_eq!(state.pane_focus, PaneFocus::Agents);
    assert_eq!(state.selected_agent_index, Some(1));
    assert_eq!(state.screen, ScreenId::Dashboard);
}

/// P15 Test 11: Enter issues, agent removed while in issues mode, exit — fallback, no crash.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-005
#[test]
fn test_exit_focus_restoration_stale() {
    let mut state = AppState::default();

    // Set up repo + 1 agent
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    state.agents.push(Agent::new(
        AgentId("agent-0".to_string()),
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Agent 0".to_string(),
        std::path::PathBuf::from("/tmp/a0"),
    ));
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    // Enter issues mode with agent-0 selected
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();

    // Simulate agent removed while in issues mode by injecting stale prior_agent_focus
    // (In real usage agents can be deleted; we directly set a stale index)
    let mut state = state;
    state.agents.clear(); // delete agent
    // prior_agent_focus still points to index 0 (now out-of-bounds)

    // Exit — should fall back gracefully
    let state = state.apply(AppEvent::ExitIssuesMode).committed_pure();
    assert_eq!(state.screen, ScreenId::Dashboard);
    assert!(!state.issues_state.active);
    // No panic; agent_index is None or 0 (fallback)
    assert!(
        state.selected_agent_index.is_none() || state.selected_agent_index == Some(0),
        "expected None or Some(0), got {:?}",
        state.selected_agent_index
    );
}

/// P15 Test 12: SelectRepository in issues mode clears issues_state and resets list_loading.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-001
#[test]
fn test_scope_change_invalidation() {
    let mut state = AppState::default();

    // Set up two repositories
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    // Enter issues mode and load some issues for repo-1
    let mut state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: Box::new(filter),
            request_id,
            issues: vec![make_test_issue(1), make_test_issue(2)],
            cursor: Some("cur".to_string()),
            has_more: true,
        })
        .committed_pure();
    assert_eq!(state.issues_state.issues().len(), 2);
    assert!(state.issues_state.has_more_issues());
    assert!(!state.issues_state.list_loading());

    // Switch to a different repository.
    let state = state.apply(AppEvent::SelectRepository(1)).committed_pure();

    // The reducer clears stale issues; the dispatch layer (not exercised by
    // this pure-reducer test) begins the reload.
    assert!(state.issues_state.issues().is_empty());
    assert!(!state.issues_state.list_loading());
    assert!(!state.issues_state.has_more_issues());
    assert!(state.issues_state.selected_issue_index().is_none());
}

/// P15 Test 13: SelectRepository clears existing data when repo changes.
///
/// Tests that stale scope response from old repo is irrelevant after repo change.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
#[test]
fn test_stale_scope_response_suppressed() {
    let mut state = AppState::default();

    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    let mut state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    let state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: Box::new(IssueFilter::default()),
            request_id,
            issues: vec![make_test_issue(1)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();

    // Switch repos
    let state = state.apply(AppEvent::SelectRepository(1)).committed_pure();
    assert!(state.issues_state.issues().is_empty());

    // Now a stale response for repo-1 arrives
    let state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: Box::new(IssueFilter::default()),
            request_id: 0,
            issues: vec![make_test_issue(99)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();

    // Stale data is discarded — repo-1 data does not appear since current repo is repo-2
    assert!(state.issues_state.issues().is_empty());
}

/// P15 Test 14: Open composer with text, change repo — inline cancelled, draft_notice set.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
#[test]
fn test_draft_discard_on_scope_change() {
    let mut state = AppState::default();

    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    // Enter issues mode, open composer, type text
    let state = state
        .apply(AppEvent::EnterIssuesMode)
        .committed_pure()
        .apply(AppEvent::OpenNewCommentComposer)
        .committed_pure()
        .apply(AppEvent::InlineChar('h'))
        .committed_pure()
        .apply(AppEvent::InlineChar('i'))
        .committed_pure();

    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "hi"),
        other => panic!("expected Composer, got {other:?}"),
    }

    // Change repository — should cancel inline and set draft notice
    let state = state.apply(AppEvent::SelectRepository(1)).committed_pure();

    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert!(
        state.issues_state.draft_notice.is_some(),
        "expected draft_notice to be set"
    );
}

/// P15 Test 15: With composer active, attempt to open editor — exclusivity enforced.
/// With editor active, attempt to open composer — exclusivity enforced.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-010
#[test]
fn test_inline_exclusivity_all_combinations() {
    let mut base = dashboard_issues_state();

    // Composer active → OpenInlineEditor blocked
    base.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let state = base
        .clone()
        .apply(AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody,
        })
        .committed_pure();
    assert!(
        matches!(
            &state.issues_state.inline_state,
            InlineState::Composer { .. }
        ),
        "Composer should block editor open, got {:?}",
        state.issues_state.inline_state
    );

    // Editor active → each composer-open event stays blocked
    let composer_opens = [
        AppEvent::OpenNewCommentComposer,
        AppEvent::OpenNewIssueComposer,
        AppEvent::OpenReplyComposer { comment_index: 0 },
    ];
    for event in composer_opens {
        base.issues_state.inline_state = InlineState::Editor {
            target: EditorTarget::IssueBody,
            text: "edit".to_string(),
            cursor: 4,
        };
        let event_label = format!("{event:?}");
        let state = base.clone().apply(event).committed_pure();
        assert!(
            matches!(&state.issues_state.inline_state, InlineState::Editor { .. }),
            "Editor should block {event_label}, got {:?}",
            state.issues_state.inline_state
        );
    }
}

/// P15 Test 16: Build send payload from detail with focused comment — all fields present.
///
/// Tests that state correctly holds all data needed for agent send payload:
/// issue detail, focused comment (via detail_subfocus), agent chooser state.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-011
fn state_with_send_payload_detail() -> AppState {
    let mut state = state_with_repo_and_agent()
        .apply(AppEvent::EnterIssuesMode)
        .committed_pure();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    let mut state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: Box::new(IssueFilter::default()),
            request_id,
            issues: vec![make_test_issue(7)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 7);
    state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            issue_number: 7,
            request_id: 0,
            detail: Box::new(send_payload_detail()),
        })
        .committed_pure()
}

#[test]
fn test_send_to_agent_payload_complete() {
    let state = state_with_send_payload_detail();

    let state = state
        .apply(AppEvent::IssueDetailSubfocusNext)
        .committed_pure()
        .apply(AppEvent::IssueDetailSubfocusNext)
        .committed_pure();
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(1)
    );

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "agent-1".to_string(),
    ))];
    let state = state
        .apply(AppEvent::OpenAgentChooser { metadata })
        .committed_pure();
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .unwrap_or_else(|| panic!("chooser should be open"));
    assert_eq!(chooser.agents.len(), 1);
    assert_eq!(chooser.agents[0].name, "My Agent");

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail should be set"));
    assert_eq!(detail.number, 7);
    assert_eq!(detail.title, "Fix crash");
    assert_eq!(detail.body, "Crash on startup");
    let focused_comment = match state.issues_state.detail_subfocus {
        DetailSubfocus::Comment(idx) => detail.comments.get(idx),
        _ => None,
    };
    assert_eq!(
        focused_comment
            .unwrap_or_else(|| panic!("expected value"))
            .comment_id,
        101
    );
}

/// P15 Test 17: OpenAgentChooser with no agents — chooser not opened, notice set.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-011
#[test]
fn test_send_to_agent_no_agents() {
    let state = issues_mode_state_with_repo("repo-1");
    assert!(state.agents.is_empty());

    let state = state
        .apply(AppEvent::OpenAgentChooser { metadata: vec![] })
        .committed_pure();

    assert!(state.issues_state.agent_chooser.is_none());
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("No agents available"),
        "no eligible agents must set the No agents available notice"
    );
}

/// P15 Test 18: Build payload with issue_base_prompt — field present in repository.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-012
#[test]
fn test_issue_base_prompt_in_payload() {
    let mut state = AppState::default();

    // Repository with issue_base_prompt set
    let mut repo = Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    );
    repo.issue_base_prompt = "Always look for root causes before proposing fixes.".to_string();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);

    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();

    // Verify the field is accessible from selected repository
    let repo = state
        .selected_repository()
        .unwrap_or_else(|| panic!("repo should be selected"));
    assert_eq!(
        repo.issue_base_prompt,
        "Always look for root causes before proposing fixes."
    );
}

/// P15 Test 19: Set up state with inline active + search focused + filter open;
/// apply Esc events in sequence; verify each level closes correctly.
///
/// The 6-level Esc chain (from innermost to outermost):
///   1. Inline editor/composer → InlineCancelOrEsc
///   2. Agent chooser → AgentChooserCancel
///   3. Search non-empty → ClearSearch
///   4. Search empty → BlurSearchInput
///   5. Filter controls → CloseFilterControls
///   6. Mode exit → ExitIssuesMode
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-004
#[test]
fn test_esc_chain_all_six_levels_integrated() {
    // Level 1: Inline Composer — InlineCancelOrEsc closes it
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let state = state.apply(AppEvent::InlineCancelOrEsc).committed_pure();
    assert_eq!(state.issues_state.inline_state, InlineState::None);

    // Level 2: Agent Chooser — AgentChooserCancel closes it
    let mut state = state;
    state.issues_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        transient_available: false,
        agents: vec![AgentChooserEntry::simple("a1", "Agent 1")],
    });
    let state = state.apply(AppEvent::AgentChooserCancel).committed_pure();
    assert!(state.issues_state.agent_chooser.is_none());

    // Level 3: Search with text — ClearSearch clears text (stays focused)
    let mut state = state;
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "open bug".to_string();
    let state = state.apply(AppEvent::ClearSearch).committed_pure();
    assert!(state.issues_state.search_query.is_empty());
    assert!(state.issues_state.search_input_focused);

    // Level 4: Search empty — BlurSearchInput removes focus
    let state = state.apply(AppEvent::BlurSearchInput).committed_pure();
    assert!(!state.issues_state.search_input_focused);

    // Level 5: Filter controls open — CloseFilterControls closes them
    let mut state = state;
    state.issues_state.filter_ui.controls_open = true;
    let state = state.apply(AppEvent::CloseFilterControls).committed_pure();
    assert!(!state.issues_state.filter_ui.controls_open);

    // Level 6: Nothing else active — ExitIssuesMode exits mode
    let state = state.apply(AppEvent::ExitIssuesMode).committed_pure();
    assert_eq!(state.screen, ScreenId::Dashboard);
    assert!(!state.issues_state.active);
}

#[test]
fn test_detail_load_failure_with_pending_token_surfaces_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);

    let state = state
        .apply(AppEvent::IssueDetailLoadFailed {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id: 0,
            error: "No GitHub repository configured".to_string(),
        })
        .committed_pure();

    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("No GitHub repository configured")
    );
}

#[test]
fn test_comment_page_failure_with_pending_token_surfaces_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = p15_state_with_loaded_detail(&repo_id, 42);
    let Some(request_id) =
        state.begin_issue_comment_page_for_test(repo_id.clone(), 42, Some("cursor-1".to_string()))
    else {
        panic!("comment page should start");
    };

    let state = state
        .apply(AppEvent::IssueCommentsPageFailed {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id,
            request_cursor: Some("cursor-1".to_string()),
            error: "No GitHub repository configured".to_string(),
        })
        .committed_pure();

    assert!(!state.issues_state.loading.comments);
    assert!(
        !state
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|detail| detail.comments.has_pending_request())
    );
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("No GitHub repository configured")
    );
}

#[test]
fn test_untokened_mutation_failure_for_current_detail_surfaces_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = p15_state_with_loaded_detail(&repo_id, 42);

    let state = state
        .apply(AppEvent::MutationFailed {
            scope_repo_id: repo_id,
            issue_number: Some(42),
            mutation_id: None,
            error: "No GitHub repository configured".to_string(),
        })
        .committed_pure();

    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("No GitHub repository configured")
    );
}

#[test]
fn test_detail_scroll_limit_uses_stored_viewport_rows() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.body = (0..30)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id: 0,
            detail: Box::new(detail),
        })
        .committed_pure();

    state.issues_state.detail_viewport_rows = 5;
    let compact_max = state.issues_state.max_detail_scroll_offset();
    state.issues_state.detail_viewport_rows = 20;
    let roomy_max = state.issues_state.max_detail_scroll_offset();

    assert!(compact_max > roomy_max);
    assert_eq!(
        compact_max,
        state.issues_state.max_detail_scroll_offset_for_viewport(5)
    );
    assert_eq!(
        roomy_max,
        state.issues_state.max_detail_scroll_offset_for_viewport(20)
    );
}

#[test]
fn test_matching_mutation_response_does_not_clear_newer_inline_draft() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.comments.replace_items(vec![p15_comment(
        1,
        "alice",
        "2024-01-01T00:00:00Z",
        "original",
    )]);
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            request_id: 0,
            detail: Box::new(detail),
        })
        .committed_pure();
    let submitted_target = InlineState::Editor {
        target: EditorTarget::Comment { comment_index: 0 },
        text: "submitted edit".to_string(),
        cursor: 14,
    };
    state.issues_state.inline_state = submitted_target.clone();
    let mut state = state
        .apply(AppEvent::MutationSubmitted {
            scope_repo_id: repo_id.clone(),
            mutation_id: 7,
            target: submitted_target,
        })
        .committed_pure();
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "newer draft".to_string(),
        cursor: 11,
    };

    let state = state
        .apply(AppEvent::CommentUpdated {
            scope_repo_id: repo_id,
            issue_number: 42,
            mutation_id: 7,
            comment_id: 1,
            comment_index: 0,
            body: "submitted edit".to_string(),
        })
        .committed_pure();

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert_eq!(detail.comments[0].body, "submitted edit");
    assert!(state.issues_state.mutation_pending.is_none());
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "newer draft"),
        other => panic!("expected newer composer draft to remain, got {other:?}"),
    }
}
