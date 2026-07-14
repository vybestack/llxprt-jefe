//! Pull Requests Mode detail-pane tests — detail loaded, staleness discard,
//! scroll-detail bounded by rendered length.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-009
//! @requirement REQ-PR-NFR-002

use crate::domain::{
    PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{PrDetailSubfocus, ScreenMode};

/// Helper: PR-mode state with one repository selected at index 0.
pub(super) fn prs_mode_state(repo_id: &str) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state
}

/// Helper: minimal PR list-row.
fn make_test_pr(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// Helper: minimal PR detail with the given number.
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
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
    }
}

/// PrDetailLoaded must set detail_subfocus=Body, clear loading.detail, and
/// populate pr_detail.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_sets_subfocus_body_and_clears_loading() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.list.replace_items(vec![make_test_pr(1)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.detail_subfocus = PrDetailSubfocus::Review(0);
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_string()), 1, 1);

    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 1,
        detail: Box::new(make_test_pr_detail(1)),
    });

    assert!(!new_state.prs_state.loading.detail);
    assert_eq!(new_state.prs_state.detail_subfocus, PrDetailSubfocus::Body);
    assert_eq!(new_state.prs_state.detail_scroll_offset, 0);
    let Some(loaded) = new_state.prs_state.pr_detail.as_ref() else {
        panic!("pr_detail should be Some");
    };
    assert_eq!(loaded.number, 1);
    assert_eq!(
        loaded.comments.identity(),
        Some(&crate::domain::CommentDetailIdentity {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            number: 1,
        })
    );
}

/// PrDetailLoaded with a stale pr_number (does not match the selected PR)
/// must be discarded — the existing detail is preserved. The request_id half
/// of the staleness contract is covered by the sibling
/// `test_detail_loaded_discards_mismatched_request_id`.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_stale_pr_number_or_request_id() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(1)); // selected PR is #2
    state.prs_state.loading.detail = true;
    let current = make_test_pr_detail(2);
    state.prs_state.pr_detail = Some(current);

    // Stale: arrives for PR #1 while PR #2 is selected.
    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        request_id: 0,
        detail: Box::new(make_test_pr_detail(1)),
    });

    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("pr_detail should remain"));
    assert_eq!(loaded.number, 2, "stale detail for PR #1 must be discarded");
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding stale"
    );
}

/// PrDetailLoaded carrying a request_id that does NOT match the pending
/// detail_pending request_id must be discarded, even when the scope and
/// pr_number match. This exercises the request_id half of the NFR-002
/// staleness contract for the detail load.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_mismatched_request_id() {
    let mut state = prs_mode_state("repo-1");
    state
        .prs_state
        .list
        .replace_items(vec![make_test_pr(1), make_test_pr(2)]);
    state.prs_state.list.set_selected_index(Some(1)); // selected PR is #2
    state.prs_state.loading.detail = true;
    let current = make_test_pr_detail(2);
    state.prs_state.pr_detail = Some(current);
    // Seed a detail load pending under request_id = R1 (=100).
    state.prs_state.detail_pending = Some(crate::state::types::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 2,
        request_id: 100,
    });

    // Dispatch PrDetailLoaded with a DIFFERENT request_id = R2 (=200),
    // matching scope and pr_number.
    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 2,
        request_id: 200,
        detail: Box::new(make_test_pr_detail(2)),
    });

    // The stale-request-id detail must be DISCARDED: existing detail preserved.
    let loaded = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("pr_detail should remain"));
    assert_eq!(
        loaded.number, 2,
        "existing detail must remain after mismatched request_id"
    );
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding mismatched request_id"
    );
    assert!(
        new_state.prs_state.detail_pending.is_some(),
        "detail_pending must remain after discarding mismatched request_id"
    );
}

/// PrDetailLoaded with a stale scope_repo_id must be discarded.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 230-235
#[test]
fn test_detail_loaded_discards_stale_scope() {
    let mut state = prs_mode_state("repo-1");
    state.prs_state.list.replace_items(vec![make_test_pr(1)]);
    state.prs_state.list.set_selected_index(Some(0));
    state.prs_state.loading.detail = true;

    let new_state = state.apply(AppEvent::PrDetailLoaded {
        scope_repo_id: RepositoryId("repo-WRONG".to_string()),
        pr_number: 1,
        request_id: 0,
        detail: Box::new(make_test_pr_detail(1)),
    });

    assert!(
        new_state.prs_state.pr_detail.is_none(),
        "stale-scope detail must be discarded"
    );
    assert!(
        new_state.prs_state.loading.detail,
        "loading.detail must remain true after discarding stale scope"
    );
}

/// ScrollDetailDown must be bounded by the rendered content length — it must
/// never exceed the maximum scroll offset derived from the real rendered length.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_scroll_detail_down_bounded_by_rendered_length() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    // Short body so the rendered length is small relative to the viewport.
    detail.body = "line one\nline two".to_string();
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = 20; // viewport larger than content
    state.prs_state.detail_scroll_offset = 0;

    // Scrolling down repeatedly must never exceed the max offset (saturating
    // at content_height - viewport_rows, which is 0 when content fits).
    let mut new_state = state.apply(AppEvent::PrScrollDetailDown);
    new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    new_state = new_state.apply(AppEvent::PrScrollDetailDown);

    assert_eq!(
        new_state.prs_state.detail_scroll_offset, 0,
        "scroll must be bounded by rendered length (content fits viewport → offset stays 0)"
    );
}

/// ScrollDetailDown on content taller than the viewport advances the offset
/// but clamps at the max (rendered_length - viewport_rows).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn test_scroll_detail_down_advances_then_clamps() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.body = (0..50)
        .map(|i| format!("body line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_viewport_rows = 10;
    state.prs_state.detail_scroll_offset = 0;

    // Scroll down many times — must clamp, not exceed the max.
    let mut new_state = state;
    for _ in 0..200 {
        new_state = new_state.apply(AppEvent::PrScrollDetailDown);
    }

    // The offset must be bounded (non-negative, and not absurdly large).
    // With 50 lines of body + header rows, the max offset is bounded.
    assert!(
        new_state.prs_state.detail_scroll_offset <= 100,
        "scroll offset must be bounded by rendered length, got {}",
        new_state.prs_state.detail_scroll_offset
    );
    assert!(
        new_state.prs_state.detail_scroll_offset > 0,
        "scroll offset should have advanced past 0 with long content"
    );
}

/// Tab to an offscreen review thread must scroll the detail so the thread is
/// visible (#151).
#[test]
fn test_pr_subfocus_next_scrolls_to_offscreen_thread() {
    use crate::domain::{IssueComment, PrReview, PrReviewState, PrReviewThread};

    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.body = "PR body".to_string();
    // Build many review threads so thread #5 is below a small viewport.
    let thread: PrReviewThread = PrReviewThread {
        thread_id: "t1".to_string(),
        path: Some("src/main.rs".to_string()),
        line: Some(10),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2024-01-01".to_string(),
            edited_at: None,
            body: "thread body".to_string(),
        }],
    };
    detail.reviews = vec![PrReview {
        review_id: None,
        author_login: "reviewer".to_string(),
        state: PrReviewState::Approved,
        submitted_at: "2024-01-01".to_string(),
        body: Some("Review with a body".to_string()),
        review_threads: vec![thread; 8],
    }];
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    state.prs_state.detail_viewport_rows = 4; // small viewport
    state.prs_state.detail_scroll_offset = 0;

    // Advance subfocus forward through Reviews and ReviewThreads to thread #5.
    // Body -> Review(0)
    state = state.apply(AppEvent::PrDetailSubfocusNext);
    // Review(0) -> ReviewThread(0)
    state = state.apply(AppEvent::PrDetailSubfocusNext);
    // Advance through threads 0..=5
    for _ in 0..5 {
        state = state.apply(AppEvent::PrDetailSubfocusNext);
    }
    assert_eq!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(5),
        "should have advanced to ReviewThread(5)"
    );
    assert_pr_thread_visible(&state, 5);
}

/// Assert the focused review thread is within the current viewport.
fn assert_pr_thread_visible(state: &AppState, thread_idx: usize) {
    let offset = state.prs_state.detail_scroll_offset;
    let viewport = state.prs_state.detail_viewport_rows;
    assert!(
        offset > 0,
        "scroll offset should have advanced to reveal thread #{thread_idx}, got {offset}"
    );
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    let range = crate::pr_detail_content::pr_subfocus_line_range(
        detail,
        PrDetailSubfocus::ReviewThread(thread_idx),
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    )
    .unwrap_or_else(|| panic!("expected range for thread {thread_idx}"));
    assert!(
        range.0 >= offset && range.0 < offset + viewport,
        "thread #{thread_idx} first line {} must be within viewport [{}, {})",
        range.0,
        offset,
        offset + viewport
    );
}

/// Tab backwards to an offscreen comment must scroll the detail so the comment
/// is visible (#151).
#[test]
fn test_pr_subfocus_prev_scrolls_to_offscreen_comment() {
    use crate::domain::IssueComment;

    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.body = "PR body".to_string();
    detail.comments.replace_items(
        (0u32..12)
            .map(|i| IssueComment {
                comment_id: u64::from(i),
                author_login: format!("user{i}"),
                created_at: "2024-01-01".to_string(),
                edited_at: None,
                body: format!("comment body {i}"),
            })
            .collect(),
    );
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_subfocus = PrDetailSubfocus::NewComment;
    state.prs_state.detail_viewport_rows = 4; // small viewport
    // Start scrolled near the bottom (NewComment is last section).
    state.prs_state.detail_scroll_offset = 100;

    // Prev from NewComment -> Comment(11) (last comment). With viewport=4, it
    // should scroll up to reveal comment 11.
    let state = state.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::Comment(11),
        "should have moved to Comment(11)"
    );
    let offset = state.prs_state.detail_scroll_offset;
    let viewport = state.prs_state.detail_viewport_rows;
    assert!(
        offset < 100,
        "scroll offset should have decreased to reveal comment 11, got {offset}"
    );
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    let range = crate::pr_detail_content::pr_subfocus_line_range(
        detail,
        PrDetailSubfocus::Comment(11),
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    )
    .unwrap_or_else(|| panic!("expected range for comment 11"));
    assert!(
        range.0 >= offset && range.0 < offset + viewport,
        "comment 11 first line {} must be within viewport [{}, {})",
        range.0,
        offset,
        offset + viewport
    );
}

/// Helper: build a minimal review thread with the given thread_id.
pub(super) fn make_thread(thread_id: &str) -> crate::domain::PrReviewThread {
    use crate::domain::{IssueComment, PrReviewThread};
    PrReviewThread {
        thread_id: thread_id.to_string(),
        path: Some("src/main.rs".to_string()),
        line: Some(10),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2024-01-01".to_string(),
            edited_at: None,
            body: "thread body".to_string(),
        }],
    }
}

/// Helper: build a minimal review with the given threads.
pub(super) fn make_review(
    author: &str,
    threads: Vec<crate::domain::PrReviewThread>,
) -> crate::domain::PrReview {
    use crate::domain::{PrReview, PrReviewState};
    PrReview {
        review_id: None,
        author_login: author.to_string(),
        state: PrReviewState::Commented,
        submitted_at: "2024-01-01".to_string(),
        body: None,
        review_threads: threads,
    }
}

/// Helper: build a minimal review with a non-empty body (so it IS a focus
/// stop) and the given threads.
fn make_review_with_body(
    author: &str,
    threads: Vec<crate::domain::PrReviewThread>,
) -> crate::domain::PrReview {
    let mut review = make_review(author, threads);
    review.body = Some(format!("Review body for {author}"));
    review
}

/// Subfocus-next must follow document order: Body → Review(0) → ReviewThread(0)
/// → ReviewThread(1) → Review(1) → ReviewThread(2) → NewComment → Body (wrap).
///
/// The rendered document (build_reviews_section) interleaves each review
/// header with its own threads, using a flat thread index that increments
/// across all reviews in that document order. The subfocus cycle MUST match
/// that same interleaving, not visit all review headers first then all
/// threads. Reviews here have non-empty bodies so they ARE focus stops.
#[test]
fn subfocus_next_follows_document_order_reviews_interleaved_with_threads() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    // 2 reviews: review 0 has 2 threads, review 1 has 1 thread.
    detail.reviews = vec![
        make_review_with_body("rev0", vec![make_thread("t0"), make_thread("t1")]),
        make_review_with_body("rev1", vec![make_thread("t2")]),
    ];
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    state.prs_state.detail_viewport_rows = 100; // large viewport, no scrolling interference

    let mut s = state;
    // Body -> Review(0)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Review(0));
    // Review(0) -> ReviewThread(0)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(0)
    );
    // ReviewThread(0) -> ReviewThread(1)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(1)
    );
    // ReviewThread(1) -> Review(1)  [interleaved! NOT skipped to a different thread]
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Review(1));
    // Review(1) -> ReviewThread(2)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(2)
    );
    // ReviewThread(2) -> NewComment (no checks or comments in this detail)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::NewComment);
    // NewComment -> Body (wrap)
    s = s.apply(AppEvent::PrDetailSubfocusNext);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Body);
}

/// Subfocus-prev must be the exact reverse of next's document order. Reviews
/// here have non-empty bodies so they ARE focus stops.
#[test]
fn subfocus_prev_follows_reverse_document_order_reviews_interleaved_with_threads() {
    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(1);
    detail.reviews = vec![
        make_review_with_body("rev0", vec![make_thread("t0"), make_thread("t1")]),
        make_review_with_body("rev1", vec![make_thread("t2")]),
    ];
    state.prs_state.pr_detail = Some(detail);
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    state.prs_state.detail_viewport_rows = 100;

    let mut s = state;
    // Body -> NewComment (prev wraps to last item)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::NewComment);
    // NewComment -> ReviewThread(2)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(2)
    );
    // ReviewThread(2) -> Review(1)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Review(1));
    // Review(1) -> ReviewThread(1)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(1)
    );
    // ReviewThread(1) -> ReviewThread(0)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(
        s.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(0)
    );
    // ReviewThread(0) -> Review(0)
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Review(0));
    // Review(0) -> Body
    s = s.apply(AppEvent::PrDetailSubfocusPrev);
    assert_eq!(s.prs_state.detail_subfocus, PrDetailSubfocus::Body);
}

/// Build a detail fixture exercising every section kind (body, reviews with
/// bodies and threads, checks, comments) for the navigation⇄renderer parity
/// test. Reviews have non-empty bodies so they ARE focus stops.
fn make_full_section_detail() -> PullRequestDetail {
    use crate::domain::{IssueComment, PrCheck, PrCheckStatus};
    let mut detail = make_test_pr_detail(1);
    detail.reviews = vec![
        make_review_with_body("rev0", vec![make_thread("t0"), make_thread("t1")]),
        make_review_with_body("rev1", vec![make_thread("t2")]),
    ];
    detail.checks = vec![
        PrCheck {
            name: "build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
        PrCheck {
            name: "test".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "success".to_string(),
            url: None,
        },
    ];
    detail.comments.replace_items(vec![
        IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2024-01-01".to_string(),
            edited_at: None,
            body: "first comment".to_string(),
        },
        IssueComment {
            comment_id: 2,
            author_login: "bob".to_string(),
            created_at: "2024-01-02".to_string(),
            edited_at: None,
            body: "second comment".to_string(),
        },
    ]);
    detail
}

/// Navigation order must match the rendered document top-to-bottom: for every
/// item in `pr_detail_subfocus_order`, the start line reported by
/// `pr_subfocus_line_range` (a projection over the SAME text the renderer
/// paints) must be strictly increasing. This pins the navigation⇄renderer
/// parity so a future change to either cannot silently break tab order.
///
/// NewComment is included: its section label renders after the comments
/// section, so it participates in the same strictly-increasing sequence.
#[test]
fn subfocus_order_matches_rendered_document_order() {
    use crate::state::InlineState;

    let detail = make_full_section_detail();
    let order = super::prs_nav_ops::pr_detail_subfocus_order(&detail);
    // Sanity: the count is derived from the fixture itself (body + per-review
    // headers + all review threads + checks + comments + NewComment) so a
    // fixture change can't silently desync a hardcoded number from reality.
    let review_threads: usize = detail.reviews.iter().map(|r| r.review_threads.len()).sum();
    let expected =
        1 + detail.reviews.len() + review_threads + detail.checks.len() + detail.comments.len() + 1;
    assert_eq!(order.len(), expected, "full order: {order:?}");

    let mut last_start: Option<usize> = None;
    for item in order {
        let Some((start, _end)) = crate::pr_detail_content::pr_subfocus_line_range(
            &detail,
            item,
            &InlineState::None,
            false,
            false,
        ) else {
            panic!("subfocus {item:?} from the order list must resolve to a line range");
        };
        if let Some(prev) = last_start {
            assert!(
                start > prev,
                "navigation order must follow the rendered document: \
                 {item:?} starts at line {start}, not after previous start {prev}"
            );
        }
        last_start = Some(start);
    }
}
