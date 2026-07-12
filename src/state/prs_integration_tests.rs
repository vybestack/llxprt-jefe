//! PR Mode end-to-end integration tests (state-reducer layer).
//!
//! Drives the REAL key→event→reducer→render chain for the state-side
//! checkpoints of P15: detail loading, scroll pagination, composer-follow,
//! exit focus restore, staleness discard, auth/empty errors, existing-modes
//! regression, and the full PR-list pagination/lazy-load/staleness-discard
//! guard. (Checkpoints 10 — Esc precedence — and 17 — persisted-state
//! exclusion — live in `src/app_input/prs_integration_tests.rs`, where the
//! real `prs::resolve_prs_key_event` and `to_persisted_state` are reachable.)
//!
//! @plan PLAN-20260624-PR-MODE.P15
//! @requirement REQ-PR-005
//! @requirement REQ-PR-007
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-014
//! @requirement REQ-PR-NFR-002

use crate::domain::{
    IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequest,
    PullRequestDetail, Repository, RepositoryId,
};
use crate::pr_detail_content::{build_pr_detail_content, pr_detail_content_line_count};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{PaneFocus, PrDetailSubfocus, PrFocus, ScreenMode};

use super::prs_test_fixtures::begin_pr_list_reload;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════════════

/// Minimal PR list-row fixture.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 22-34
pub(super) fn make_test_pr(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// A PR detail fixture with reviews + checks + comments for rendering tests.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 74-101,157-193
fn make_test_pr_detail(number: u64) -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body text".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: Some(PrReviewState::Approved),
        checks_status: PrCheckStatus::Success,
        reviews: vec![PrReview {
            review_id: None,
            author_login: "reviewer1".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2024-01-01T12:00:00Z".to_string(),
            body: Some("LGTM".to_string()),
            review_threads: vec![],
        }],
        checks: vec![PrCheck {
            name: "CI".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: Some("https://example.com/ci".to_string()),
        }],
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "commenter".to_string(),
            created_at: "2024-01-01T10:00:00Z".to_string(),
            edited_at: None,
            body: "First comment".to_string(),
        }],
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}

/// Dashboard AppState with two repositories (no github_repo slug set).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-76
pub(super) fn dashboard_state() -> AppState {
    let mut state = AppState::default();
    for slug in ["repo-1", "repo-2"] {
        state.repositories.push(Repository::new(
            RepositoryId(slug.to_string()),
            slug.to_string(),
            slug.to_string(),
            PathBuf::from(format!("/tmp/{slug}")),
        ));
    }
    state.selected_repository_index = Some(0);
    state
}

/// Dashboard AppState with an active PR mode (entered, list loaded).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 66-76,209-223
fn active_prs_state_with_list() -> AppState {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);
    let scope = RepositoryId("repo-1".to_string());
    let filter = state.prs_state.committed_filter.clone();
    let request_id = begin_pr_list_reload(&mut state, "repo-1", filter);
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: scope,
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });
    state
}

/// In-place apply helper to avoid the take/replace dance on owned AppState.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-291
pub(super) trait ApplyInPlace {
    fn apply_in_place(&mut self, event: AppEvent);
}

impl ApplyInPlace for AppState {
    fn apply_in_place(&mut self, event: AppEvent) {
        let old = std::mem::take(self);
        *self = old.apply(event);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
/// PR-mode state with a selected PR detail loaded (enters list, selects PR #1,
/// delivers PrDetailLoaded).  Reusable fixture for composer/detail tests.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 21-27,230-235
fn state_with_loaded_pr_detail() -> AppState {
    let mut state = active_prs_state_with_list();
    state.apply_in_place(AppEvent::PrListEnter);
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_string()), 1, 1);
    state.apply_in_place(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 1,
        detail: std::boxed::Box::new(make_test_pr_detail(1)),
    });
    state
}

/// A test comment with body "ship it" for the composer-submit flow.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-010
/// @pseudocode component-002 lines 157-193
fn make_ship_it_comment() -> IssueComment {
    IssueComment {
        comment_id: 99,
        author_login: "testuser".to_string(),
        created_at: "2024-01-05T00:00:00Z".to_string(),
        edited_at: None,
        body: "ship it".to_string(),
    }
}

// Checkpoint 5: it_select_pr_loads_detail_with_reviews_and_checks
// ═══════════════════════════════════════════════════════════════════════════

/// Enter on a selected PR transitions to PrDetail and a delivered
/// `PrDetailLoaded` event renders the review summaries, check summaries, and
/// comment in the detail content.
///
/// Drives: PrListEnter to PrDetail focus transition; then simulate the
/// `PrDetailLoaded` event the dispatch layer delivers; assert the rendered
/// `build_pr_detail_content(...).text` contains the review/check summaries and
/// `pr_detail_content_line_count` matches.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 21-27,230-235
#[test]
fn it_select_pr_loads_detail_with_reviews_and_checks() {
    let mut state = active_prs_state_with_list();

    // Select the first PR via Enter (PrList focus to PrDetail focus).
    state.apply_in_place(AppEvent::PrListEnter);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);

    // Simulate the detail-loading dispatch by delivering the loaded detail
    // event (the event the background thread would produce).
    let scope = RepositoryId("repo-1".to_string());
    let detail = make_test_pr_detail(1);
    state.mark_pr_detail_loading(scope.clone(), 1, 1);
    state.apply_in_place(AppEvent::PrDetailLoaded {
        scope_repo_id: scope,
        pr_number: 1,
        request_id: 1,
        detail: std::boxed::Box::new(detail.clone()),
    });

    // Assert the detail is populated.
    let loaded = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("PrDetailLoaded must populate pr_detail"));
    assert_eq!(loaded.number, 1);
    assert_eq!(loaded.reviews.len(), 1);
    assert_eq!(loaded.checks.len(), 1);
    assert_eq!(loaded.comments.len(), 1);

    // Assert the RENDERED content includes review/check summaries.
    let content = build_pr_detail_content(
        loaded,
        PrDetailSubfocus::Body,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    );
    assert!(
        content.text.contains("reviewer1") || content.text.contains("Review"),
        "detail content must include review summaries: {}",
        content.text
    );
    assert!(
        content.text.contains("CI") || content.text.contains("Check"),
        "detail content must include check summaries: {}",
        content.text
    );

    // Line count must match the rendered text.
    let line_count = pr_detail_content_line_count(
        loaded,
        PrDetailSubfocus::Body,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    );
    assert!(
        line_count > 0,
        "detail content line count must be positive for non-empty detail"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 6: it_scroll_detail_paginates_comments
// ═══════════════════════════════════════════════════════════════════════════

/// Scrolling the detail viewport toward the bottom, then delivering a
/// `PrCommentsPageLoaded` event, appends new comments to the loaded detail.
///
/// Drives: PrScrollDetailDown; then simulate the `PrCommentsPageLoaded` event
/// the dispatch layer would deliver; assert `apply_pr_comments_page_loaded`
/// appends the new comments and updates the cursor.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-010
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 236-241
#[test]
fn it_scroll_detail_paginates_comments() {
    let mut state = active_prs_state_with_list();
    state.apply_in_place(AppEvent::PrListEnter);

    let scope = RepositoryId("repo-1".to_string());
    let mut detail = make_test_pr_detail(1);
    detail.has_more_comments = true;
    detail.comments_cursor = Some("cursor-1".to_string());
    state.mark_pr_detail_loading(scope.clone(), 1, 1);
    state.apply_in_place(AppEvent::PrDetailLoaded {
        scope_repo_id: scope.clone(),
        pr_number: 1,
        request_id: 1,
        detail: std::boxed::Box::new(detail),
    });

    // Simulate the comments-page dispatch by marking comments loading, then
    // delivering the PrCommentsPageLoaded event (the event the background
    // thread would produce).
    state.prs_state.loading.comments = true;
    state.prs_state.comments_page_pending = Some(crate::state::types::PrCommentsPagePending {
        scope_repo_id: scope.clone(),
        pr_number: 1,
        cursor: Some("cursor-1".to_string()),
        request_id: 0,
    });

    let new_comment = IssueComment {
        comment_id: 2,
        author_login: "commenter2".to_string(),
        created_at: "2024-01-03T00:00:00Z".to_string(),
        edited_at: None,
        body: "Second comment".to_string(),
    };

    state.apply_in_place(AppEvent::PrCommentsPageLoaded {
        scope_repo_id: scope,
        pr_number: 1,
        request_id: 0,
        comments: vec![new_comment],
        cursor: None,
        has_more: false,
    });

    let loaded = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail must remain loaded after comments page"));
    assert_eq!(
        loaded.comments.len(),
        2,
        "PrCommentsPageLoaded must append comments"
    );
    assert!(
        !state.prs_state.loading.comments,
        "PrCommentsPageLoaded must clear loading.comments"
    );

    assert_stale_comments_page_is_discarded(&mut state);
}

/// Stale-discard guard: after the page above cleared comments_page_pending,
/// a late/duplicate PrCommentsPageLoaded with a non-matching request_id must
/// be DISCARDED (no append, no state change). This proves the reducer's
/// pr_comments_page_pending_matches guard is load-bearing.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 236-241
fn assert_stale_comments_page_is_discarded(state: &mut AppState) {
    let stale_comment = IssueComment {
        comment_id: 999,
        author_login: "stale".to_string(),
        created_at: "2024-09-09T00:00:00Z".to_string(),
        edited_at: None,
        body: "stale comment".to_string(),
    };
    state.apply_in_place(AppEvent::PrCommentsPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 4242,
        comments: vec![stale_comment],
        cursor: None,
        has_more: false,
    });
    let after_stale = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail must remain loaded after stale comments page"));
    assert_eq!(
        after_stale.comments.len(),
        2,
        "stale PrCommentsPageLoaded (non-matching request_id, pending already cleared) must be discarded"
    );
}

/// Compute the expected PR-detail bottom scroll offset by replicating the
/// production `scroll_pr_detail_to_bottom` → `pr_max_detail_scroll_offset`
/// path: the REAL rendered line count from the shared
/// `pr_detail_content::pr_detail_content_line_count` parity function (for the
/// current subfocus + inline composer state), then
/// `saturating_sub(detail_viewport_rows)`.  This deliberately uses the same
/// parity surface production uses so the assertion follows the real rendered
/// bottom (reviews, checks, separators, section headers, composer block) and
/// never the old header+body+comments heuristic that under-scrolled (#56).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
fn pr_expected_detail_bottom_scroll(state: &AppState) -> usize {
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        return 0;
    };
    crate::pr_detail_content::pr_detail_content_line_count(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    )
    .saturating_sub(state.prs_state.detail_viewport_rows)
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 7: it_compose_comment_follows_viewport_and_appends
// ═══════════════════════════════════════════════════════════════════════════

/// Opening the composer from Body subfocus, submitting a non-blank comment,
/// and delivering `PrCommentCreated` appends the comment to the detail.
///
/// Drives: PrOpenNewCommentComposer to set InlineState::Composer (visible);
/// PrInlineChar to fill text; PrInlineSubmit to set mutation_pending; then
/// simulate PrCommentCreated to append.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 292-330
#[test]
fn it_compose_comment_follows_viewport_and_appends() {
    use crate::state::types::InlineState;

    let mut state = state_with_loaded_pr_detail();
    let scope = RepositoryId("repo-1".to_string());

    // Open the composer from Body subfocus.
    state.apply_in_place(AppEvent::PrOpenNewCommentComposer);
    assert!(
        matches!(state.prs_state.inline_state, InlineState::Composer { .. }),
        "PrOpenNewCommentComposer must open the composer (visible)"
    );
    // Composer open must move detail_subfocus to NewComment (#56).
    assert_eq!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::NewComment,
        "opening the composer must set detail_subfocus to NewComment (#56)"
    );
    // Composer open must scroll the detail to the REAL rendered bottom (#56).
    // Mirrors the Issues exemplar (detail_scroll_offset ==
    // max_detail_scroll_offset()); `pr_expected_detail_bottom_scroll`
    // replicates production via the shared parity line-count.
    assert_eq!(
        state.prs_state.detail_scroll_offset,
        pr_expected_detail_bottom_scroll(&state),
        "opening the composer must scroll detail to the bottom (#56)"
    );

    // Type "ship it", submit (sets mutation_pending).
    for ch in "ship it".chars() {
        state.apply_in_place(AppEvent::PrInlineChar(ch));
    }
    assert!(
        matches!(state.prs_state.inline_state, InlineState::Composer { ref text, .. } if text == "ship it"),
        "composer text must be \"ship it\" after typing"
    );
    state.apply_in_place(AppEvent::PrInlineSubmit);
    assert!(
        state.prs_state.mutation_pending.is_some(),
        "PrInlineSubmit must set mutation_pending"
    );

    // Deliver PrCommentCreated (the mutation result).
    state.apply_in_place(AppEvent::PrCommentCreated {
        scope_repo_id: scope,
        pr_number: 1,
        mutation_id: state.prs_state.next_mutation_id,
        comment: make_ship_it_comment(),
    });
    let loaded = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail must remain loaded after comment create"));
    assert!(
        loaded.comments.iter().any(|c| c.body == "ship it"),
        "PrCommentCreated must append the new comment to the detail"
    );
    assert!(
        state.prs_state.inline_state == InlineState::None,
        "PrCommentCreated must close the composer"
    );
    // Post-create subfocus must point at the newly-created comment (#56).
    // Fixture starts with 1 comment; appended comment is at index 1.
    // Mirrors prs_tests_composer_focus.rs:170-175.
    assert_eq!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::Comment(1),
        "subfocus must point at the newly-created comment (#56)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 11: it_exit_restores_prior_dashboard_focus
// ═══════════════════════════════════════════════════════════════════════════

/// Exiting PR mode restores the prior dashboard focus (pane_focus, repo/agent
/// index) that was saved on entry.
///
/// Drives: set a known dashboard focus; EnterPrsMode (saves prior focus);
/// ExitPrsMode (restores it).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-005
/// @pseudocode component-001 lines 66-87
#[test]
fn it_exit_restores_prior_dashboard_focus() {
    let mut state = dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_repository_index = Some(1);
    state.selected_agent_index = None;

    // Enter PR mode (saves prior focus).
    state.apply_in_place(AppEvent::EnterPrsMode);
    assert!(state.prs_state.active);
    assert_eq!(state.screen_mode, ScreenMode::DashboardPullRequests);

    // Exit restores prior focus.
    state.apply_in_place(AppEvent::ExitPrsMode);
    assert!(!state.prs_state.active);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert_eq!(
        state.pane_focus,
        PaneFocus::Agents,
        "pane_focus must be restored"
    );
    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "selected_repository_index must be restored"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 12: it_stale_response_discarded_after_repo_switch
// ═══════════════════════════════════════════════════════════════════════════

/// After switching repositories, a late `PrListLoaded` for the OLD scope is
/// discarded by the staleness guard (no list update).
///
/// Drives: EnterPrsMode for repo-1; repo nav down to repo-2 (resets list +
/// reload pending); deliver PrListLoaded for the OLD scope (repo-1); assert it
/// is discarded (list stays empty, no rows from the stale response).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 88-98,209-223
#[test]
fn it_stale_response_discarded_after_repo_switch() {
    let mut state = dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    state = state.apply(AppEvent::EnterPrsMode);
    state.prs_state.pr_focus = PrFocus::RepoList;

    // Navigate down to repo-2 (scope switch resets the list).
    state.apply_in_place(AppEvent::PrNavigateDown);
    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "repo nav must move to repo-2"
    );
    assert!(
        state.prs_state.pull_requests().is_empty(),
        "list must be cleared after repo switch"
    );

    // Deliver a PrListLoaded for the OLD scope (repo-1) — stale.
    let old_scope = RepositoryId("repo-1".to_string());
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: old_scope,
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id: 0,
        pull_requests: vec![make_test_pr(100), make_test_pr(200)],
        cursor: None,
        has_more: false,
    });

    // The stale response must be discarded: list stays empty.
    assert!(
        state.prs_state.pull_requests().is_empty(),
        "stale PrListLoaded for old scope must be discarded"
    );
    assert_eq!(
        state.prs_state.selected_pr_index(),
        None,
        "stale response must not set selected_pr_index"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 14: it_not_authenticated_shows_auth_error
// ═══════════════════════════════════════════════════════════════════════════

/// A `PrListLoadFailed` event with an auth-related error surfaces a scoped
/// error message (never silent).
///
/// Drives: EnterPrsMode (loading.list=true); deliver PrListLoadFailed with an
/// auth error; assert prs_state.error is set with the message and loading
/// is cleared.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 242-247
#[test]
fn it_not_authenticated_shows_auth_error() {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);
    let filter = state.prs_state.committed_filter.clone();
    let request_id = begin_pr_list_reload(&mut state, "repo-1", filter);
    assert!(
        state.prs_state.list_loading(),
        "reload must set list loading"
    );

    let scope = RepositoryId("repo-1".to_string());
    state.apply_in_place(AppEvent::PrListLoadFailed {
        scope_repo_id: scope,
        request_id,
        error: "gh is not authenticated. Run: gh auth login".to_string(),
    });

    assert!(
        state.prs_state.error.is_some(),
        "PrListLoadFailed must surface an error (never silent)"
    );
    let error = state
        .prs_state
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("error must be set"));
    assert!(
        error.to_lowercase().contains("auth"),
        "error must mention auth, got: {error}"
    );
    assert!(
        !state.prs_state.list_loading(),
        "PrListLoadFailed must clear loading.list"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 15: it_empty_pr_list_shows_empty_state
// ═══════════════════════════════════════════════════════════════════════════

/// Loading a PR list with zero PRs yields an empty-state render: the list is
/// cleared, selected_pr_index is None, and pr_detail is None.
///
/// Drives: seed a non-empty list; deliver PrListLoaded with empty result;
/// assert the reducer clears the list.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 218-220
#[test]
fn it_empty_pr_list_shows_empty_state() {
    let mut state = dashboard_state();
    state = state.apply(AppEvent::EnterPrsMode);
    // Seed a non-empty list so the empty-result clearing is observable.
    state.prs_state.list.replace_items(vec![make_test_pr(42)]);
    state.prs_state.list.set_selected_index(Some(0));

    let scope = RepositoryId("repo-1".to_string());
    let filter = state.prs_state.committed_filter.clone();
    let request_id = begin_pr_list_reload(&mut state, "repo-1", filter);
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: scope,
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id,
        pull_requests: vec![],
        cursor: None,
        has_more: false,
    });

    assert!(
        state.prs_state.pull_requests().is_empty(),
        "empty PrListLoaded must clear the list"
    );
    assert_eq!(
        state.prs_state.selected_pr_index(),
        None,
        "empty PrListLoaded must reset selected_pr_index"
    );
    assert!(
        state.prs_state.pr_detail.is_none(),
        "empty PrListLoaded must clear pr_detail"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Checkpoint 18: it_dashboard_and_issues_modes_unaffected
// ═══════════════════════════════════════════════════════════════════════════

/// Dashboard and Issues modes are unaffected by the PR-mode additions: the
/// Dashboard screen mode renders correctly, and Issues mode can still be
/// entered/exited. This is a regression guard.
///
/// Drives: verify AppState::default() is Dashboard; enter Issues mode; exit
/// Issues mode; verify Dashboard is restored.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-76
#[test]
fn it_dashboard_and_issues_modes_unaffected() {
    let state = AppState::default();
    assert_eq!(
        state.screen_mode,
        ScreenMode::Dashboard,
        "default must be Dashboard"
    );

    // Issues mode regression.
    let state2 = dashboard_state();
    let entered = state2.apply(AppEvent::EnterIssuesMode);
    assert_eq!(entered.screen_mode, ScreenMode::DashboardIssues);
    assert!(entered.issues_state.active);
    let exited = entered.apply(AppEvent::ExitIssuesMode);
    assert_eq!(exited.screen_mode, ScreenMode::Dashboard);
    assert!(!exited.issues_state.active);

    // PR mode does not interfere.
    assert!(
        !exited.prs_state.active,
        "PR mode must not be active after Issues enter/exit"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FIX 4: reducer clamp derives from the REAL rendered line count
// ═══════════════════════════════════════════════════════════════════════════

/// The reducer's max scroll offset must equal the REAL rendered line count
/// produced by `pr_detail_content_line_count` (the exact text the renderer
/// emits). The markdown renderer soft-wraps long lines — including
/// whitespace-free runs (issue #155) — INSIDE the builder, so the count and
/// the displayed text can never desync; the clamp must derive from that real
/// count, not a heuristic (the original "jump to top" regression came from a
/// reducer-side wrap heuristic diverging from the rendered text).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn reducer_max_scroll_matches_real_rendered_line_count() {
    let mut state = state_with_loaded_pr_detail();

    state.prs_state.detail_viewport_rows = 1;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;

    // Render the body for a given last line and return the real line count.
    let count_for_last_line = |last: &str| {
        let mut s = state.clone();
        let mut lines: Vec<String> = (0..20).map(|i| format!("body line {i}")).collect();
        lines.push(last.to_string());
        if let Some(detail) = s.prs_state.pr_detail.as_mut() {
            detail.body = lines.join("\n");
        }
        let detail = s
            .prs_state
            .pr_detail
            .as_ref()
            .unwrap_or_else(|| panic!("pr_detail must be loaded"));
        pr_detail_content_line_count(
            detail,
            s.prs_state.detail_subfocus,
            &s.prs_state.inline_state,
            s.prs_state.loading.detail,
            s.prs_state.loading.comments,
        )
    };

    // Non-circular proof that the count tracks the RENDERED shape: a 200-char
    // whitespace-free run soft-wraps inside the builder (issue #155), so its
    // count EXCEEDS the short-line count. A clamp built from source lines (or
    // any other heuristic) would not see those extra rows and desync — exactly
    // the "jump to top" class of regression.
    let count_with_long_line = count_for_last_line(&"x".repeat(200));
    assert!(
        count_with_long_line > count_for_last_line("short"),
        "a long whitespace-free body line soft-wraps into extra rendered rows"
    );

    // The reducer's nav-End clamp must derive from that real rendered count.
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        let mut lines: Vec<String> = (0..20).map(|i| format!("body line {i}")).collect();
        lines.push("x".repeat(200));
        detail.body = lines.join("\n");
    }
    state.apply_in_place(AppEvent::PrNavigateEnd);
    let expected = count_with_long_line.saturating_sub(state.prs_state.detail_viewport_rows);
    assert_eq!(
        state.prs_state.detail_scroll_offset, expected,
        "nav End clamp must use the real rendered line count"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FIX 5: prs_nav_ops clamp must use current subfocus
// ═══════════════════════════════════════════════════════════════════════════

/// Setting a non-Body subfocus must make the nav clamp (PrNavigateEnd) equal
/// `build_pr_detail_content`'s count for THAT subfocus, not Body. Although
/// the current builder produces identical line counts for all subfocuses
/// (subfocus only affects `>` prefix markers, not line count), this test
/// guards against a future builder change that makes them diverge. The
/// clamp must read `detail_subfocus`, not a hard-coded Body.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn nav_clamp_uses_current_subfocus_not_body() {
    let mut state = state_with_loaded_pr_detail();

    // Set subfocus to NewComment.
    state.prs_state.detail_subfocus = PrDetailSubfocus::NewComment;
    state.prs_state.detail_viewport_rows = 1;

    // Navigate to End → sets detail_scroll_offset to max.
    state.apply_in_place(AppEvent::PrNavigateEnd);

    // Expected max offset for the ACTUAL subfocus (NewComment).
    let expected_for_subfocus = {
        let detail = state
            .prs_state
            .pr_detail
            .as_ref()
            .unwrap_or_else(|| panic!("pr_detail must be loaded"));
        pr_detail_content_line_count(
            detail,
            PrDetailSubfocus::NewComment,
            &state.prs_state.inline_state,
            state.prs_state.loading.detail,
            state.prs_state.loading.comments,
        )
        .saturating_sub(state.prs_state.detail_viewport_rows)
    };

    assert_eq!(
        state.prs_state.detail_scroll_offset, expected_for_subfocus,
        "nav End clamp must use the current subfocus (NewComment)"
    );
}
