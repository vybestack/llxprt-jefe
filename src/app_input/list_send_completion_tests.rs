use super::issues_send::issue_send_info_from_state;
use super::prs_orchestration::pr_send_info_from_state;
use jefe::domain::{
    Agent, AgentChooserGitMetadata, AgentId, Issue, IssueDetail, IssueFilter, IssueState, PrCheck,
    PrCheckStatus, PrFilter, PrReview, PrReviewState, PrState, PullRequest, PullRequestDetail,
    Repository, RepositoryId,
};
use jefe::state::AppEvent;
use jefe::state::transition::TransitionExt;
use jefe::state::{
    AppState, IssueFocus, IssueListSendPending, PrFocus, PrListSendPending, ScreenId,
};
use std::path::PathBuf;

fn repository() -> Repository {
    let mut repository = Repository::new(
        RepositoryId("repo-1".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo".to_owned(),
        "repo".to_owned(),
        PathBuf::from("/tmp/repo"),
    );
    repository.issue_base_prompt = "Repository instructions".to_owned();
    repository
}

fn eligible_state() -> AppState {
    let repository_id = RepositoryId("repo-1".to_owned());
    let mut state = crate::test_app_state();
    state.repositories.push(repository());
    state.selected_repository_index = Some(0);
    state.available_agent_type_ids = vec![jefe::domain::shipped_agent_type(3)];
    state.agents.push(Agent::new(
        AgentId("agent-1".to_owned()),
        repository_id,
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Agent".to_owned(),
        PathBuf::from("/tmp/agent-1"),
    ));
    state
}

fn chooser_metadata() -> Vec<AgentChooserGitMetadata> {
    vec![AgentChooserGitMetadata::for_agent(AgentId(
        "agent-1".to_owned(),
    ))]
}

fn issue(number: u64) -> Issue {
    Issue {
        number,
        node_id: format!("I_{number}"),
        title: format!("Issue {number}"),
        state: IssueState::Open,
        author_login: "author".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        priority: None,
        state_reason: None,
        linked_pr_numbers: Vec::new(),
    }
}

fn pull_request(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR {number}"),
        state: PrState::Open,
        author_login: "author".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        head_ref: "feature".to_owned(),
        head_sha: "sha".to_owned(),
        base_ref: "main".to_owned(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        mergeable: None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

fn issue_detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "upstream/project".to_owned(),
        number,
        node_id: format!("I_{number}"),
        title: format!("Full issue {number}"),
        state: IssueState::Open,
        author_login: "author".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: vec!["full-label".to_owned()],
        assignees: vec!["full-assignee".to_owned()],
        milestone: Some("full-milestone".to_owned()),
        body: "FULL ISSUE BODY".to_owned(),
        external_url: format!("https://github.com/upstream/project/issues/{number}"),
        comments: jefe::domain::PaginatedList::default(),
        issue_type_name: Some("Task".to_owned()),
        state_reason: None,
    }
}

fn pull_request_detail(number: u64) -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "upstream/project".to_owned(),
        number,
        title: format!("Full PR {number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "author".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        head_ref: "full-feature".to_owned(),
        head_sha: "full-sha".to_owned(),
        base_ref: "main".to_owned(),
        labels: vec!["full-label".to_owned()],
        assignees: vec!["full-assignee".to_owned()],
        milestone: Some("full-milestone".to_owned()),
        body: "FULL PR BODY".to_owned(),
        external_url: format!("https://github.com/upstream/project/pull/{number}"),
        review_decision: Some(PrReviewState::Approved),
        checks_status: PrCheckStatus::Success,
        reviews: vec![PrReview {
            review_id: Some("PRR_full".to_owned()),
            author_login: "reviewer".to_owned(),
            state: PrReviewState::Approved,
            submitted_at: String::new(),
            body: Some("FULL REVIEW BODY".to_owned()),
            review_threads: Vec::new(),
        }],
        checks: vec![PrCheck {
            name: "full-check".to_owned(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_owned(),
            url: None,
        }],
        comments: jefe::domain::PaginatedList::default(),
        mergeable: Some(true),
        merge_state_status: Some("CLEAN".to_owned()),
    }
}

fn issue_list_state_after_detail_completion() -> AppState {
    let mut state = eligible_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state
        .issues_state
        .list
        .replace_items(vec![issue(620), issue(621)]);
    state.issues_state.list.set_selected_index(Some(1));
    state.mark_issue_detail_loading_with_request_id(RepositoryId("repo-1".to_owned()), 621, 7);
    state.issues_state.list_send_pending = Some(IssueListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        issue_number: 621,
        request_id: 7,
        metadata: chooser_metadata(),
        ready: false,
    });
    state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            issue_number: 621,
            request_id: 7,
            detail: Box::new(issue_detail(621)),
        })
        .committed_pure()
}

fn assert_issue_list_position(state: &AppState) {
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);
    assert_eq!(state.issues_state.list.selected_index(), Some(1));
}

fn assert_issue_send_payload(state: &AppState) {
    let send_info = issue_send_info_from_state(state);
    let Some(send_info) = send_info else {
        panic!("issue list chooser must resolve complete send info");
    };
    assert_eq!(send_info.payload.repository, "upstream/project");
    assert_eq!(send_info.payload.issue_number, 621);
    assert_eq!(send_info.payload.issue_title, "Full issue 621");
    assert_eq!(send_info.payload.issue_body, "FULL ISSUE BODY");
    assert_eq!(send_info.payload.issue_labels, ["full-label"]);
    assert_eq!(send_info.payload.issue_assignees, ["full-assignee"]);
    assert_eq!(
        send_info.payload.issue_base_prompt,
        "Repository instructions"
    );
}
fn issue_list_state_with_pending_detail() -> AppState {
    let mut state = eligible_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state
        .issues_state
        .list
        .replace_items(vec![issue(620), issue(621)]);
    state.issues_state.list.set_selected_index(Some(1));
    state.mark_issue_detail_loading_with_request_id(RepositoryId("repo-1".to_owned()), 621, 7);
    state.issues_state.list_send_pending = Some(IssueListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        issue_number: 621,
        request_id: 7,
        metadata: chooser_metadata(),
        ready: false,
    });
    state
}

fn issue_detail_loaded_event() -> AppEvent {
    AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        issue_number: 621,
        request_id: 7,
        detail: Box::new(issue_detail(621)),
    }
}

fn issue_list_send_ready_event() -> AppEvent {
    AppEvent::IssueListSendDetailReady {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        issue_number: 621,
        request_id: 7,
    }
}

fn pr_list_state_with_pending_detail() -> AppState {
    let mut state = eligible_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::PullRequests);
    state.prs_state.active = true;
    state.prs_state.pr_focus = PrFocus::PrList;
    state
        .prs_state
        .list
        .replace_items(vec![pull_request(620), pull_request(621)]);
    state.prs_state.list.set_selected_index(Some(1));
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_owned()), 621, 7);
    state.prs_state.list_send_pending = Some(PrListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        pr_number: 621,
        request_id: 7,
        metadata: chooser_metadata(),
        ready: false,
    });
    state
}

fn pr_detail_loaded_event() -> AppEvent {
    AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        pr_number: 621,
        request_id: 7,
        detail: Box::new(pull_request_detail(621)),
    }
}

fn pr_list_send_ready_event() -> AppEvent {
    AppEvent::PrListSendDetailReady {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        pr_number: 621,
        request_id: 7,
    }
}

#[test]
fn list_send_completion_stays_invalid_after_help_opens_and_closes() {
    let issue = issue_list_state_with_pending_detail()
        .apply(AppEvent::OpenHelp)
        .committed_pure()
        .apply(AppEvent::CloseModal)
        .committed_pure()
        .apply(issue_detail_loaded_event())
        .committed_pure()
        .apply(issue_list_send_ready_event())
        .committed_pure();
    assert!(issue.issues_state.list_send_pending.is_none());
    assert!(issue.issues_state.agent_chooser.is_none());

    let pr = pr_list_state_with_pending_detail()
        .apply(AppEvent::OpenHelp)
        .committed_pure()
        .apply(AppEvent::CloseModal)
        .committed_pure()
        .apply(pr_detail_loaded_event())
        .committed_pure()
        .apply(pr_list_send_ready_event())
        .committed_pure();
    assert!(pr.prs_state.list_send_pending.is_none());
    assert!(pr.prs_state.agent_chooser.is_none());
}

#[test]
fn list_send_completion_stays_invalid_after_selection_moves_away_and_back() {
    let issue = issue_list_state_with_pending_detail()
        .apply(AppEvent::IssuesNavigateUp)
        .committed_pure()
        .apply(AppEvent::IssuesNavigateDown)
        .committed_pure()
        .apply(issue_detail_loaded_event())
        .committed_pure()
        .apply(issue_list_send_ready_event())
        .committed_pure();
    assert_eq!(issue.issues_state.list.selected_index(), Some(1));
    assert!(issue.issues_state.agent_chooser.is_none());

    let pr = pr_list_state_with_pending_detail()
        .apply(AppEvent::PrNavigateUp)
        .committed_pure()
        .apply(AppEvent::PrNavigateDown)
        .committed_pure()
        .apply(pr_detail_loaded_event())
        .committed_pure()
        .apply(pr_list_send_ready_event())
        .committed_pure();
    assert_eq!(pr.prs_state.list.selected_index(), Some(1));
    assert!(pr.prs_state.agent_chooser.is_none());
}

#[test]
fn same_index_list_replacement_permanently_invalidates_list_send() {
    let repository_id = RepositoryId("repo-1".to_owned());
    let mut issue_state = issue_list_state_with_pending_detail();
    issue_state
        .issues_state
        .list
        .replace_items(vec![issue(621), issue(620)]);
    issue_state.issues_state.list.set_selected_index(Some(0));
    issue_state.mark_issue_list_silent_refresh_loading(
        repository_id.clone(),
        IssueFilter::default(),
        9,
    );
    let issue_state = issue_state
        .apply(AppEvent::IssueListSilentRefreshed {
            scope_repo_id: repository_id.clone(),
            filter: Box::new(IssueFilter::default()),
            request_id: 9,
            issues: vec![issue(622), issue(620)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    assert_eq!(issue_state.issues_state.list.selected_index(), Some(0));
    assert_eq!(issue_state.issues_state.issues()[0].number, 622);
    assert!(issue_state.issues_state.list_send_pending.is_none());
    let mut issue_state = issue_state;
    issue_state.mark_issue_list_silent_refresh_loading(
        repository_id.clone(),
        IssueFilter::default(),
        10,
    );
    let issue_state = issue_state
        .apply(AppEvent::IssueListSilentRefreshed {
            scope_repo_id: repository_id.clone(),
            filter: Box::new(IssueFilter::default()),
            request_id: 10,
            issues: vec![issue(621), issue(620)],
            cursor: None,
            has_more: false,
        })
        .committed_pure()
        .apply(issue_detail_loaded_event())
        .committed_pure()
        .apply(issue_list_send_ready_event())
        .committed_pure();
    assert_eq!(issue_state.issues_state.issues()[0].number, 621);
    assert!(issue_state.issues_state.agent_chooser.is_none());
}

#[test]
fn same_index_pr_list_replacement_permanently_invalidates_list_send() {
    let repository_id = RepositoryId("repo-1".to_owned());
    let mut pr_state = pr_list_state_with_pending_detail();
    pr_state
        .prs_state
        .list
        .replace_items(vec![pull_request(621), pull_request(620)]);
    pr_state.prs_state.list.set_selected_index(Some(0));
    pr_state.mark_pr_list_silent_refresh_loading(repository_id.clone(), PrFilter::default(), 9);
    let pr_state = pr_state
        .apply(AppEvent::PrListSilentRefreshed {
            scope_repo_id: repository_id.clone(),
            filter: Box::new(PrFilter::default()),
            request_id: 9,
            pull_requests: vec![pull_request(619), pull_request(620)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    assert_eq!(pr_state.prs_state.list.selected_index(), Some(0));
    assert_eq!(pr_state.prs_state.pull_requests()[0].number, 619);
    assert!(pr_state.prs_state.list_send_pending.is_none());
    let mut pr_state = pr_state;
    pr_state.mark_pr_list_silent_refresh_loading(repository_id.clone(), PrFilter::default(), 10);
    let pr_state = pr_state
        .apply(AppEvent::PrListSilentRefreshed {
            scope_repo_id: repository_id,
            filter: Box::new(PrFilter::default()),
            request_id: 10,
            pull_requests: vec![pull_request(620), pull_request(621)],
            cursor: None,
            has_more: false,
        })
        .committed_pure()
        .apply(pr_detail_loaded_event())
        .committed_pure()
        .apply(pr_list_send_ready_event())
        .committed_pure();
    assert_eq!(pr_state.prs_state.pull_requests()[1].number, 621);
    assert!(pr_state.prs_state.agent_chooser.is_none());
}

#[test]
fn issue_list_detail_completion_opens_chooser_with_full_payload_and_preserves_list() {
    let state = issue_list_state_after_detail_completion();
    assert_issue_list_position(&state);
    assert!(state.issues_state.detail_pending.is_none());
    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        panic!("issue detail completion must install full detail");
    };
    assert_eq!(detail.body, "FULL ISSUE BODY");

    let state = state
        .apply(AppEvent::IssueListSendDetailReady {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            issue_number: 621,
            request_id: 7,
        })
        .committed_pure();
    assert_issue_send_payload(&state);
    assert_issue_list_position(&state);
    assert!(state.issues_state.list_send_pending.is_none());

    let state = state.apply(AppEvent::AgentChooserCancel).committed_pure();
    assert!(state.issues_state.agent_chooser.is_none());
    assert_issue_list_position(&state);
    let state = state
        .apply(AppEvent::IssueListSendDetailReady {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            issue_number: 621,
            request_id: 7,
        })
        .committed_pure();
    assert!(state.issues_state.agent_chooser.is_none());
}

fn pr_list_state_after_detail_completion() -> AppState {
    let mut state = eligible_state();
    state.nav = jefe::state::navigation::NavState::rooted(ScreenId::PullRequests);
    state.prs_state.active = true;
    state.prs_state.pr_focus = PrFocus::PrList;
    state
        .prs_state
        .list
        .replace_items(vec![pull_request(620), pull_request(621)]);
    state.prs_state.list.set_selected_index(Some(1));
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_owned()), 621, 7);
    state.prs_state.list_send_pending = Some(PrListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        pr_number: 621,
        request_id: 7,
        metadata: chooser_metadata(),
        ready: false,
    });
    state
        .apply(AppEvent::PrDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            pr_number: 621,
            request_id: 7,
            detail: Box::new(pull_request_detail(621)),
        })
        .committed_pure()
}

fn assert_pr_list_position(state: &AppState) {
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);
    assert_eq!(state.prs_state.list.selected_index(), Some(1));
}

fn assert_pr_send_payload(state: &AppState) {
    let send_info = pr_send_info_from_state(state);
    let Some(send_info) = send_info else {
        panic!("PR list chooser must resolve complete send info");
    };
    assert_eq!(send_info.payload.repository, "upstream/project");
    assert_eq!(send_info.payload.pr_number, 621);
    assert_eq!(send_info.payload.pr_title, "Full PR 621");
    assert_eq!(send_info.payload.pr_body, "FULL PR BODY");
    assert_eq!(send_info.payload.head_ref, "full-feature");
    assert_eq!(send_info.payload.review_summary, ["reviewer: approved"]);
    assert_eq!(send_info.payload.check_summary, ["full-check: success"]);
    assert_eq!(send_info.payload.pr_base_prompt, "Repository instructions");
}

#[test]
fn pr_list_detail_completion_opens_chooser_with_full_payload_and_preserves_list() {
    let state = pr_list_state_after_detail_completion();
    assert_pr_list_position(&state);
    assert!(state.prs_state.detail_pending.is_none());
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        panic!("PR detail completion must install full detail");
    };
    assert_eq!(detail.body, "FULL PR BODY");

    let state = state
        .apply(AppEvent::PrListSendDetailReady {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            pr_number: 621,
            request_id: 7,
        })
        .committed_pure();
    assert_pr_send_payload(&state);
    assert_pr_list_position(&state);
    assert!(state.prs_state.list_send_pending.is_none());

    let state = state.apply(AppEvent::PrAgentChooserCancel).committed_pure();
    assert!(state.prs_state.agent_chooser.is_none());
    assert_pr_list_position(&state);
    let state = state
        .apply(AppEvent::PrListSendDetailReady {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            pr_number: 621,
            request_id: 7,
        })
        .committed_pure();
    assert!(state.prs_state.agent_chooser.is_none());
}

#[test]
fn exact_failure_clears_list_send_and_late_ready_cannot_open_chooser() {
    let issue = issue_list_state_with_pending_detail()
        .apply(AppEvent::IssueDetailLoadFailed {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            issue_number: 621,
            request_id: 7,
            error: "failed".to_owned(),
        })
        .committed_pure()
        .apply(issue_list_send_ready_event())
        .committed_pure();
    assert!(issue.issues_state.list_send_pending.is_none());
    assert!(issue.issues_state.agent_chooser.is_none());

    let pr = pr_list_state_with_pending_detail()
        .apply(AppEvent::PrDetailLoadFailed {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            pr_number: 621,
            request_id: 7,
            error: "failed".to_owned(),
        })
        .committed_pure()
        .apply(pr_list_send_ready_event())
        .committed_pure();
    assert!(pr.prs_state.list_send_pending.is_none());
    assert!(pr.prs_state.agent_chooser.is_none());
}

#[test]
fn stale_ready_event_cannot_consume_a_newer_list_send_continuation() {
    let mut issue = issue_list_state_with_pending_detail();
    issue.mark_issue_detail_loading_with_request_id(RepositoryId("repo-1".to_owned()), 621, 8);
    issue.issues_state.list_send_pending = Some(IssueListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        issue_number: 621,
        request_id: 8,
        metadata: chooser_metadata(),
        ready: true,
    });
    let issue = issue.apply(issue_list_send_ready_event()).committed_pure();
    assert_eq!(
        issue
            .issues_state
            .list_send_pending
            .as_ref()
            .map(|pending| pending.request_id),
        Some(8)
    );
    assert!(issue.issues_state.agent_chooser.is_none());

    let mut pr = pr_list_state_with_pending_detail();
    pr.mark_pr_detail_loading(RepositoryId("repo-1".to_owned()), 621, 8);
    pr.prs_state.list_send_pending = Some(PrListSendPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        pr_number: 621,
        request_id: 8,
        metadata: chooser_metadata(),
        ready: true,
    });
    let pr = pr.apply(pr_list_send_ready_event()).committed_pure();
    assert_eq!(
        pr.prs_state
            .list_send_pending
            .as_ref()
            .map(|pending| pending.request_id),
        Some(8)
    );
    assert!(pr.prs_state.agent_chooser.is_none());
}
