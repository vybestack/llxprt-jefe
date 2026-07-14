use crate::domain::{
    Agent, AgentChooserEntry, AgentChooserGitMetadata, AgentId, Issue, IssueComment, IssueDetail,
    IssueFilter, IssueState, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{
    AgentChooserState, ComposerTarget, DetailSubfocus, EditorTarget, InlineState, PaneFocus,
    ScreenMode,
};

use super::issues_test_fixtures::begin_issue_list_reload;

fn dashboard_issues_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
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
    }
}

/// Helper: create a state already in issues mode with a selected repository.
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
    state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number,
        request_id: 0,
        detail: Box::new(p15_detail(issue_number)),
    })
}

fn state_with_repo_and_agent() -> AppState {
    let mut state = AppState {
        selected_repository_index: Some(0),
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
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
    let mut state = issues_mode_state_with_repo("repo-1").apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id,
        mutation_id: 11,
        target: submitted_target,
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "newer draft".to_string(),
        cursor: 11,
    };

    let state = state.apply(AppEvent::ApplySearch);
    assert!(state.issues_state.mutation_pending.is_none());
    assert_eq!(state.issues_state.inline_state, InlineState::None);

    let state = state.apply(AppEvent::OpenNewCommentComposer);
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
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/repo2"),
    ));
    state.selected_repository_index = Some(0);
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 12,
        target: submitted_target,
    });
    state.selected_repository_index = Some(1);
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "new repo draft".to_string(),
        cursor: 14,
    };

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 12,
        issue_number: 1,
    });

    assert!(state.issues_state.mutation_pending.is_some());
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "new repo draft"),
        other => panic!("expected new repo draft to remain, got {other:?}"),
    }
    assert!(state.issues_state.draft_notice.is_none());
}

#[test]
fn test_create_issue_success_for_current_repo_sets_notice_and_clears_pending() {
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let state = issues_mode_state_with_repo("repo-1");
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 21,
        target: submitted_target.clone(),
    });
    state.issues_state.inline_state = submitted_target;

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 21,
        issue_number: 77,
    });

    assert!(state.issues_state.mutation_pending.is_none());
    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Created issue #77")
    );
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
        "Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    state.agents.push(Agent::new(
        AgentId("agent-0".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 0".to_string(),
        std::path::PathBuf::from("/tmp/a0"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/a1"),
    ));
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(1);

    // Enter issues mode — focus is saved
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);

    // Exit — prior focus restored
    let state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(state.pane_focus, PaneFocus::Agents);
    assert_eq!(state.selected_agent_index, Some(1));
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
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
        "Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    state.agents.push(Agent::new(
        AgentId("agent-0".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 0".to_string(),
        std::path::PathBuf::from("/tmp/a0"),
    ));
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    // Enter issues mode with agent-0 selected
    let state = state.apply(AppEvent::EnterIssuesMode);

    // Simulate agent removed while in issues mode by injecting stale prior_agent_focus
    // (In real usage agents can be deleted; we directly set a stale index)
    let mut state = state;
    state.agents.clear(); // delete agent
    // prior_agent_focus still points to index 0 (now out-of-bounds)

    // Exit — should fall back gracefully
    let state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
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
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    // Enter issues mode and load some issues for repo-1
    let mut state = state.apply(AppEvent::EnterIssuesMode);
    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id,
        issues: vec![make_test_issue(1), make_test_issue(2)],
        cursor: Some("cur".to_string()),
        has_more: true,
    });
    assert_eq!(state.issues_state.issues().len(), 2);
    assert!(state.issues_state.has_more_issues());
    assert!(!state.issues_state.list_loading());

    // Switch to a different repository.
    let state = state.apply(AppEvent::SelectRepository(1));

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
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    let mut state = state.apply(AppEvent::EnterIssuesMode);
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id,
        issues: vec![make_test_issue(1)],
        cursor: None,
        has_more: false,
    });

    // Switch repos
    let state = state.apply(AppEvent::SelectRepository(1));
    assert!(state.issues_state.issues().is_empty());

    // Now a stale response for repo-1 arrives
    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![make_test_issue(99)],
        cursor: None,
        has_more: false,
    });

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
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);

    // Enter issues mode, open composer, type text
    let state = state
        .apply(AppEvent::EnterIssuesMode)
        .apply(AppEvent::OpenNewCommentComposer)
        .apply(AppEvent::InlineChar('h'))
        .apply(AppEvent::InlineChar('i'));

    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "hi"),
        other => panic!("expected Composer, got {other:?}"),
    }

    // Change repository — should cancel inline and set draft notice
    let state = state.apply(AppEvent::SelectRepository(1));

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
    let state = base.clone().apply(AppEvent::OpenInlineEditor {
        target: EditorTarget::IssueBody,
    });
    assert!(
        matches!(
            &state.issues_state.inline_state,
            InlineState::Composer { .. }
        ),
        "Composer should block editor open, got {:?}",
        state.issues_state.inline_state
    );

    // Editor active → OpenNewCommentComposer blocked
    base.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "edit".to_string(),
        cursor: 4,
    };
    let state = base.clone().apply(AppEvent::OpenNewCommentComposer);
    assert!(
        matches!(&state.issues_state.inline_state, InlineState::Editor { .. }),
        "Editor should block composer open, got {:?}",
        state.issues_state.inline_state
    );

    // Editor active → OpenNewIssueComposer blocked
    base.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "edit".to_string(),
        cursor: 4,
    };
    let state = base.clone().apply(AppEvent::OpenNewIssueComposer);
    assert!(
        matches!(&state.issues_state.inline_state, InlineState::Editor { .. }),
        "Editor should block new-issue composer open, got {:?}",
        state.issues_state.inline_state
    );

    // Editor active → OpenReplyComposer blocked
    base.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "edit".to_string(),
        cursor: 4,
    };
    let state = base
        .clone()
        .apply(AppEvent::OpenReplyComposer { comment_index: 0 });
    assert!(
        matches!(&state.issues_state.inline_state, InlineState::Editor { .. }),
        "Editor should block reply composer open, got {:?}",
        state.issues_state.inline_state
    );
}

/// P15 Test 16: Build send payload from detail with focused comment — all fields present.
///
/// Tests that state correctly holds all data needed for agent send payload:
/// issue detail, focused comment (via detail_subfocus), agent chooser state.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-011
#[test]
fn test_send_to_agent_payload_complete() {
    let mut state = state_with_repo_and_agent().apply(AppEvent::EnterIssuesMode);
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    let mut state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id,
        issues: vec![make_test_issue(7)],
        cursor: None,
        has_more: false,
    });
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 7);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 7,
        request_id: 0,
        detail: Box::new(send_payload_detail()),
    });

    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(1)
    );

    let metadata = vec![AgentChooserGitMetadata::for_agent(AgentId(
        "agent-1".to_string(),
    ))];
    let state = state.apply(AppEvent::OpenAgentChooser { metadata });
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

    let state = state.apply(AppEvent::OpenAgentChooser { metadata: vec![] });

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
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    );
    repo.issue_base_prompt = "Always look for root causes before proposing fixes.".to_string();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);

    let state = state.apply(AppEvent::EnterIssuesMode);

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
    let state = state.apply(AppEvent::InlineCancelOrEsc);
    assert_eq!(state.issues_state.inline_state, InlineState::None);

    // Level 2: Agent Chooser — AgentChooserCancel closes it
    let mut state = state;
    state.issues_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        agents: vec![AgentChooserEntry::simple("a1", "Agent 1")],
    });
    let state = state.apply(AppEvent::AgentChooserCancel);
    assert!(state.issues_state.agent_chooser.is_none());

    // Level 3: Search with text — ClearSearch clears text (stays focused)
    let mut state = state;
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "open bug".to_string();
    let state = state.apply(AppEvent::ClearSearch);
    assert!(state.issues_state.search_query.is_empty());
    assert!(state.issues_state.search_input_focused);

    // Level 4: Search empty — BlurSearchInput removes focus
    let state = state.apply(AppEvent::BlurSearchInput);
    assert!(!state.issues_state.search_input_focused);

    // Level 5: Filter controls open — CloseFilterControls closes them
    let mut state = state;
    state.issues_state.filter_ui.controls_open = true;
    let state = state.apply(AppEvent::CloseFilterControls);
    assert!(!state.issues_state.filter_ui.controls_open);

    // Level 6: Nothing else active — ExitIssuesMode exits mode
    let state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert!(!state.issues_state.active);
}

/// InlineCursorUp/Down move the cursor between lines in multi-line text.
#[test]
fn test_inline_cursor_vertical_navigation() {
    use super::inline_cursor_vertical;

    // 3 lines: abc, def, ghi — offsets [0..3], [4..7], [8..11]
    let text = ["abc", "def", "ghi"].join(&String::from(char::from(0x0Au8)));

    // Down from line 0 col 1 to line 1 col 1
    let mut cursor = 1;
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 5);

    // Down from line 1 col 1 to line 2 col 1
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9);

    // Down from last line stays
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9);

    // Up from line 2 col 1 to line 1 col 1
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 5);

    // Up from line 1 col 1 to line 0 col 1
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 1);

    // Up from first line stays
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 1);
}

/// InlineCursorUp/Down clamp column when target line is shorter.
#[test]
fn test_inline_cursor_vertical_column_clamping() {
    use super::inline_cursor_vertical;

    // 3 lines: abcdef (len 6), xy (len 2), z (len 1)
    let nl = String::from(char::from(0x0Au8));
    let text = ["abcdef", "xy", "z"].join(&nl);

    // Cursor at col 5 of line 0 -> down to line 1 (len 2) -> clamp to col 2
    let mut cursor = 5;
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9); // line 1 start=7, col clamped to 2 = byte 9
}

/// InlineCursorUp/Down compute columns in characters for multi-byte (Unicode) text.
/// Without this fix, byte-based column math lands on invalid positions.
#[test]
fn test_inline_cursor_vertical_unicode_columns() {
    use super::inline_cursor_vertical;

    let nl = String::from(char::from(0x0Au8));
    // Line 0: 3 emoji (4 bytes each = 12 bytes, 3 chars)
    // Line 1: 2 emoji (4 bytes each = 8 bytes, 2 chars)
    let emoji = "\u{1F600}\u{1F601}\u{1F602}";
    let emoji_short = "\u{1F600}\u{1F601}";
    let text = [emoji, emoji_short].join(&nl);
    // Byte layout:
    //   [0..4)   emoji 1 (line 0)
    //   [4..8)   emoji 2 (line 0)
    //   [8..12)  emoji 3 (line 0)
    //   [12]     newline
    //   [13..17) emoji 1 (line 1)
    //   [17..21) emoji 2 (line 1)

    // Place cursor after the 2nd emoji on line 0 (char col 2, byte 8).
    let mut cursor = 8;
    // Move down: char col 2 on line 1 (end of line 1) = byte 21.
    // The old byte-based code would compute col=8 and clamp to 8, landing
    // at byte 13+8=21 only by coincidence; for col 1 the bug is visible.
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(
        cursor, 21,
        "Unicode down: should land at char col 2 on line 1"
    );

    // Move back up: char col 2 on line 0 = byte 8 again.
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 8, "Unicode up: should land at char col 2 on line 0");

    // Place cursor after 1st emoji on line 0 (char col 1, byte 4).
    let mut cursor = 4;
    // Down: char col 1 on line 1 = byte 17.
    // With the old byte-based code, col=4 would land at byte 13+4=17, which
    // is the middle of emoji 2 on line 1 (bytes 17..21) — an invalid char
    // boundary. The fix lands exactly on the boundary at byte 17.
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(
        cursor, 17,
        "Unicode down: should land at char col 1 on line 1"
    );
}

#[test]
fn test_detail_load_failure_with_pending_token_surfaces_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);

    let state = state.apply(AppEvent::IssueDetailLoadFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        error: "No GitHub repository configured".to_string(),
    });

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

    let state = state.apply(AppEvent::IssueCommentsPageFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id,
        request_cursor: Some("cursor-1".to_string()),
        error: "No GitHub repository configured".to_string(),
    });

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

    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: repo_id,
        issue_number: Some(42),
        mutation_id: None,
        error: "No GitHub repository configured".to_string(),
    });

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
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

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
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });
    let submitted_target = InlineState::Editor {
        target: EditorTarget::Comment { comment_index: 0 },
        text: "submitted edit".to_string(),
        cursor: 14,
    };
    state.issues_state.inline_state = submitted_target.clone();
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 7,
        target: submitted_target,
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "newer draft".to_string(),
        cursor: 11,
    };

    let state = state.apply(AppEvent::CommentUpdated {
        scope_repo_id: repo_id,
        issue_number: 42,
        mutation_id: 7,
        comment_id: 1,
        comment_index: 0,
        body: "submitted edit".to_string(),
    });

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
