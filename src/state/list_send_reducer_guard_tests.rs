use crate::domain::{
    Agent, AgentChooserGitMetadata, AgentId, Issue, IssueState, PrState, PullRequest, Repository,
    RepositoryId,
};
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::{
    AppState, ComposerTarget, InlineState, IssueCloseReasonChooserState, IssueDeleteConfirmState,
    IssueFocus, IssuePropertyEditorState, IssuePropertyKind, ModalState, NewIssueFormState,
    PrFocus, PrMergeChooserState, PrPropertyEditorState, PrPropertyKind,
};

fn eligible_state() -> AppState {
    let repository_id = RepositoryId("repo-1".to_owned());
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        repository_id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repository".to_owned(),
        "repo-1".to_owned(),
        std::path::PathBuf::from("/tmp/repo-1"),
    ));
    state.selected_repository_index = Some(0);
    state.available_agent_type_ids = vec![crate::domain::shipped_agent_type(3)];
    state.agents.push(Agent::new(
        AgentId("agent-1".to_owned()),
        repository_id,
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Agent".to_owned(),
        std::path::PathBuf::from("/tmp/agent-1"),
    ));
    state
}

fn metadata() -> Vec<AgentChooserGitMetadata> {
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
        author_login: String::new(),
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
        author_login: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        head_ref: String::new(),
        head_sha: String::new(),
        base_ref: String::new(),
        is_draft: false,
        review_decision: None,
        checks_status: crate::domain::PrCheckStatus::None,
        mergeable: None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

fn issue_list_state() -> AppState {
    let mut state = eligible_state();
    state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Issues);
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.list.replace_items(vec![issue(621)]);
    state.issues_state.list.set_selected_index(Some(0));
    state
}

fn pr_list_state() -> AppState {
    let mut state = eligible_state();
    state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::PullRequests);
    state.prs_state.active = true;
    state.prs_state.pr_focus = PrFocus::PrList;
    state.prs_state.list.replace_items(vec![pull_request(621)]);
    state.prs_state.list.set_selected_index(Some(0));
    state
}

fn inline_composer() -> InlineState {
    InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    }
}

fn issue_property_editor() -> IssuePropertyEditorState {
    IssuePropertyEditorState {
        kind: IssuePropertyKind::Labels,
        options: Vec::new(),
        selected_index: 0,
        title_text: String::new(),
        title_cursor: 0,
        error: None,
        baseline: Vec::new(),
        loading_failed: false,
        options_loading: false,
        load_request_id: 0,
    }
}

fn pr_property_editor() -> PrPropertyEditorState {
    PrPropertyEditorState {
        kind: PrPropertyKind::Labels,
        options: Vec::new(),
        selected_index: 0,
        title_text: String::new(),
        title_cursor: 0,
        error: None,
        baseline: Vec::new(),
        loading_failed: false,
        options_loading: false,
        load_request_id: 0,
    }
}

#[test]
fn issue_list_send_begin_atomically_stages_exact_detail_request() {
    let state = issue_list_state()
        .apply(AppEvent::BeginIssueListSendDetail(metadata()))
        .committed_pure();
    let pending = state
        .issues_state
        .list_send_pending
        .as_ref()
        .unwrap_or_else(|| panic!("issue list-send continuation must be staged"));
    assert_eq!(pending.scope_repo_id, RepositoryId("repo-1".to_owned()));
    assert_eq!(pending.issue_number, 621);
    assert!(!pending.ready);
    assert!(state.issue_detail_request_is_current(
        &pending.scope_repo_id,
        pending.issue_number,
        pending.request_id,
    ));
}

#[test]
fn pr_list_send_begin_atomically_stages_exact_detail_request() {
    let state = pr_list_state()
        .apply(AppEvent::BeginPrListSendDetail(metadata()))
        .committed_pure();
    let pending = state
        .prs_state
        .list_send_pending
        .as_ref()
        .unwrap_or_else(|| panic!("PR list-send continuation must be staged"));
    assert_eq!(pending.scope_repo_id, RepositoryId("repo-1".to_owned()));
    assert_eq!(pending.pr_number, 621);
    assert!(!pending.ready);
    assert!(state.pr_detail_request_is_current(
        &pending.scope_repo_id,
        pending.pr_number,
        pending.request_id,
    ));
}

#[test]
fn list_send_begin_rejects_invalid_context_without_detail_side_effects() {
    let mut issue = issue_list_state()
        .apply(AppEvent::BeginIssueListSendDetail(metadata()))
        .committed_pure();
    issue.modal = ModalState::Help;
    let issue = issue
        .apply(AppEvent::BeginIssueListSendDetail(metadata()))
        .committed_pure();
    assert!(issue.issues_state.list_send_pending.is_none());
    assert!(issue.issues_state.detail_pending.is_none());
    assert!(!issue.issues_state.loading.detail);

    let mut pr = pr_list_state()
        .apply(AppEvent::BeginPrListSendDetail(metadata()))
        .committed_pure();
    pr.terminal_focused = true;
    let pr = pr
        .apply(AppEvent::BeginPrListSendDetail(metadata()))
        .committed_pure();
    assert!(pr.prs_state.list_send_pending.is_none());
    assert!(pr.prs_state.detail_pending.is_none());
    assert!(!pr.prs_state.loading.detail);
}

#[test]
fn list_send_cancel_clears_the_exact_paired_detail_request() {
    let issue = issue_list_state()
        .apply(AppEvent::BeginIssueListSendDetail(metadata()))
        .committed_pure()
        .apply(AppEvent::CancelIssueListSendDetail)
        .committed_pure();
    assert!(issue.issues_state.list_send_pending.is_none());
    assert!(issue.issues_state.detail_pending.is_none());
    assert!(!issue.issues_state.loading.detail);

    let pr = pr_list_state()
        .apply(AppEvent::BeginPrListSendDetail(metadata()))
        .committed_pure()
        .apply(AppEvent::CancelPrListSendDetail)
        .committed_pure();
    assert!(pr.prs_state.list_send_pending.is_none());
    assert!(pr.prs_state.detail_pending.is_none());
    assert!(!pr.prs_state.loading.detail);
}

#[test]
fn auth_required_clears_exact_requests_without_domain_error() {
    let issue = issue_list_state()
        .apply(AppEvent::BeginIssueListSendDetail(metadata()))
        .committed_pure();
    let issue_pending = issue
        .issues_state
        .list_send_pending
        .as_ref()
        .unwrap_or_else(|| panic!("issue list-send continuation must be staged"));
    let issue_event = AppEvent::IssueDetailAuthRequired(
        issue_pending.scope_repo_id.clone(),
        issue_pending.issue_number,
        issue_pending.request_id,
    );
    let issue = issue.apply(issue_event).committed_pure();
    assert!(issue.issues_state.list_send_pending.is_none());
    assert!(issue.issues_state.detail_pending.is_none());
    assert!(issue.issues_state.error.is_none());

    let pr = pr_list_state()
        .apply(AppEvent::BeginPrListSendDetail(metadata()))
        .committed_pure();
    let pr_pending = pr
        .prs_state
        .list_send_pending
        .as_ref()
        .unwrap_or_else(|| panic!("PR list-send continuation must be staged"));
    let pr_event = AppEvent::PrDetailAuthRequired(
        pr_pending.scope_repo_id.clone(),
        pr_pending.pr_number,
        pr_pending.request_id,
    );
    let pr = pr.apply(pr_event).committed_pure();
    assert!(pr.prs_state.list_send_pending.is_none());
    assert!(pr.prs_state.detail_pending.is_none());
    assert!(pr.prs_state.error.is_none());
}
fn assert_issue_chooser_rejected(state: AppState) {
    let state = state
        .apply(AppEvent::OpenAgentChooser {
            metadata: metadata(),
        })
        .committed_pure();
    assert!(state.issues_state.agent_chooser.is_none());
    assert!(state.issues_state.draft_notice.is_none());
}

fn assert_pr_chooser_rejected(state: AppState) {
    let state = state
        .apply(AppEvent::PrOpenAgentChooser {
            metadata: metadata(),
        })
        .committed_pure();
    assert!(state.prs_state.agent_chooser.is_none());
    assert!(state.prs_state.draft_notice.is_none());
}

#[test]
fn issue_chooser_open_is_ignored_under_every_competing_interaction() {
    let mut base = eligible_state();
    base.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Issues);
    base.issues_state.active = true;
    base.issues_state.issue_focus = IssueFocus::IssueList;
    let guards: &[fn(&mut AppState)] = &[
        |state| state.modal = ModalState::Help,
        |state| state.issues_state.issue_focus = IssueFocus::RepoList,
        |state| state.issues_state.inline_state = inline_composer(),
        |state| state.issues_state.property_editor = Some(issue_property_editor()),
        |state| {
            state.issues_state.close_reason_chooser = Some(IssueCloseReasonChooserState {
                issue_number: 621,
                selected_index: 0,
                duplicate_search: None,
                awaiting_confirmation: false,
            });
        },
        |state| {
            state.issues_state.delete_confirm = Some(IssueDeleteConfirmState {
                issue_number: 621,
                awaiting_confirmation: false,
            });
        },
        |state| state.issues_state.new_issue_form = Some(NewIssueFormState::default()),
        |state| state.issues_state.search_input_focused = true,
        |state| state.issues_state.filter_ui.controls_open = true,
    ];
    for guard in guards {
        let mut state = base.clone();
        guard(&mut state);
        assert_issue_chooser_rejected(state);
    }
}

#[test]
fn pr_chooser_open_is_ignored_under_every_competing_interaction() {
    let mut base = eligible_state();
    base.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::PullRequests);
    base.prs_state.active = true;
    base.prs_state.pr_focus = PrFocus::PrList;
    let guards: &[fn(&mut AppState)] = &[
        |state| state.modal = ModalState::Help,
        |state| state.prs_state.pr_focus = PrFocus::RepoList,
        |state| state.prs_state.inline_state = inline_composer(),
        |state| {
            state.prs_state.merge_chooser = Some(PrMergeChooserState {
                selected_index: 0,
                allowed_methods: None,
                awaiting_confirmation: false,
            });
        },
        |state| state.prs_state.property_editor = Some(pr_property_editor()),
        |state| state.prs_state.search_input_focused = true,
        |state| state.prs_state.filter_ui.controls_open = true,
    ];
    for guard in guards {
        let mut state = base.clone();
        guard(&mut state);
        assert_pr_chooser_rejected(state);
    }
}
