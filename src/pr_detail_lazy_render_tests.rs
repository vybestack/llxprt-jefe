//! Lazy-render tests for review-thread conversations (issue #155 performance).
//!
//! Every review thread is collapsed when not focused, regardless of
//! resolved/outdated status, so a detail with hundreds of threads renders
//! compactly (headers only) and never calls markdown rendering for the
//! comment bodies of non-focused threads. Focused threads expand their full
//! conversation.
//!
//! Child module of `pr_detail_content_tests` so it shares the fixtures; split
//! out to keep the parent file under the source-size limit.

use super::*;
use crate::domain::{
    IssueComment, PrCheckStatus, PrReview, PrReviewState, PrReviewThread, PrState,
    PullRequestDetail,
};
use crate::state::InlineState;

/// Build an unresolved, current review thread with a single comment whose
/// body is `body`.
fn unresolved_thread(thread_id: &str, path: &str, line: u32, body: &str) -> PrReviewThread {
    PrReviewThread {
        thread_id: thread_id.to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some(path.to_string()),
        line: Some(line),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2024-01-01".to_string(),
            edited_at: None,
            body: body.to_string(),
        }],
    }
}

/// Build a detail with one review containing one unresolved, current thread.
fn detail_with_unresolved_thread() -> PullRequestDetail {
    let mut detail = sample_detail();
    detail.reviews[0].review_threads = vec![unresolved_thread(
        "T1",
        "src/main.rs",
        10,
        "This needs a fix.",
    )];
    detail
}

// ── Semantic change: unresolved/current threads collapse while not focused ──

/// An unresolved, current thread is COLLAPSED while not focused: the header
/// and "(select to expand)" hint are present, but the comment body is absent.
/// This is the core lazy-render semantic change (every thread collapses unless
/// focused, regardless of resolved/outdated status).
#[test]
fn unresolved_thread_collapsed_body_when_not_focused() {
    let detail = detail_with_unresolved_thread();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("src/main.rs:10"),
        "unresolved thread header must render"
    );
    assert!(
        content.text.contains("[UNRESOLVED]"),
        "unresolved tag must render"
    );
    assert!(
        !content.text.contains("This needs a fix."),
        "unresolved thread body must be HIDDEN while not focused (lazy render)"
    );
    assert!(
        content.text.contains("(select to expand)"),
        "collapsed unresolved thread must hint how to expand"
    );
    assert!(
        content.text.contains("1 comment"),
        "collapsed thread must show its comment count"
    );
}

/// Focusing that same unresolved thread reveals its body and action hints.
#[test]
fn focusing_unresolved_thread_reveals_body_and_actions() {
    let detail = detail_with_unresolved_thread();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("This needs a fix."),
        "focused unresolved thread must render its conversation body"
    );
    assert!(
        content.text.contains("reviewer"),
        "focused thread must render comment author"
    );
    assert!(
        content.text.contains("[ r reply ]"),
        "focused thread must show reply hint"
    );
}

// ── Performance-shape: hundreds of threads render compactly ──────────────

/// Build a detail with `count` unresolved, current threads under a single
/// review. Each thread carries a distinctive markdown body so we can assert
/// none of the non-focused bodies are rendered when Body is focused.
///
/// The body marker uses a zero-padded index (`{i:05}`) so that marker `0001`
/// is never a substring of marker `0010` or `0100` — preventing false-positive
/// `.contains()` matches in the assertion loops below.
fn detail_with_many_threads(count: usize) -> PullRequestDetail {
    let mut detail = sample_detail();
    let threads: Vec<PrReviewThread> = (0..count)
        .map(|i| {
            unresolved_thread(
                &format!("T{i}"),
                &format!("src/file_{i}.rs"),
                u32::try_from(i).unwrap_or(u32::MAX),
                // Distinctive markdown body that exercises the renderer path.
                &format!("# Heading {i:05}\n\n**distinctive body marker {i:05}**"),
            )
        })
        .collect();
    detail.reviews[0].review_threads = threads;
    detail
}

/// A large detail (hundreds of unresolved threads) with distinctive markdown
/// bodies renders COMPACTLY when Body is focused: NONE of the thread bodies
/// are rendered, only the headers. This is behavioral/performance-shape
/// coverage proving the renderer does not parse hundreds of hidden bodies on
/// the render path.
#[test]
fn large_detail_renders_compactly_with_no_thread_bodies_when_body_focused() {
    let count = 300;
    let detail = detail_with_many_threads(count);
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    // No thread body should be rendered.
    for i in 0..count {
        let marker = format!("distinctive body marker {i:05}");
        assert!(
            !content.text.contains(&marker),
            "thread {i} body must NOT render when Body is focused (lazy render), but found marker"
        );
    }
    // All thread headers must render (so the user sees they exist).
    for i in 0..count {
        let header = format!("src/file_{i}.rs");
        assert!(
            content.text.contains(&header),
            "thread {i} header must render even when collapsed"
        );
    }
}

/// Focusing ONE thread in a large detail reveals EXACTLY that thread's body
/// and no others.
#[test]
fn focusing_one_thread_in_large_detail_reveals_only_that_body() {
    let count = 300;
    let detail = detail_with_many_threads(count);
    let focus_idx = 150;
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(focus_idx),
        &InlineState::None,
        false,
        false,
    );
    // The focused thread's body must render.
    let focused_marker = format!("distinctive body marker {focus_idx:05}");
    assert!(
        content.text.contains(&focused_marker),
        "focused thread {focus_idx} body must render"
    );
    // No other thread body should render.
    for i in 0..count {
        if i == focus_idx {
            continue;
        }
        let marker = format!("distinctive body marker {i:05}");
        assert!(
            !content.text.contains(&marker),
            "thread {i} body must NOT render when thread {focus_idx} is focused"
        );
    }
}

/// The rendered line count of a large detail must be bounded proportionally
/// to the number of thread headers, NOT to the number of comment bodies. With
/// all threads collapsed, each thread contributes ~2 lines (header + blank).
/// This is the performance-shape invariant: the output does not explode with
/// comment bodies.
#[test]
fn large_detail_line_count_bounded_proportionally_to_headers() {
    let small = detail_with_many_threads(10);
    let large = detail_with_many_threads(300);

    let small_count = pr_detail_content_line_count(
        &small,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let large_count = pr_detail_content_line_count(
        &large,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );

    // The large detail has 290 more threads. Each collapsed thread is ~2 lines
    // (header + trailing blank). So large_count should be roughly
    // small_count + 290 * ~2 = small_count + ~580. It must NOT be
    // small_count + 290 * (many body lines per thread).
    // Assert the marginal cost per thread is small (≤ 3 lines per added thread).
    let marginal_per_thread = large_count.saturating_sub(small_count) / (300 - 10);
    assert!(
        marginal_per_thread <= 3,
        "collapsed thread marginal line cost must be ≤ 3, got {marginal_per_thread} \
         (small_count={small_count}, large_count={large_count})"
    );
}

// ── Commentless threads do not advertise expansion ───────────────────────

/// A commentless thread should NOT show the "(select to expand)" hint when
/// collapsed (expanding reveals nothing).
#[test]
fn commentless_thread_does_not_advertise_expansion() {
    let mut detail = sample_detail();
    detail.reviews[0].review_threads = vec![PrReviewThread {
        thread_id: "T_empty".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/empty.rs".to_string()),
        line: Some(1),
        comments: vec![],
    }];
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("src/empty.rs:1"),
        "commentless thread header must render"
    );
    assert!(
        !content.text.contains("(select to expand)"),
        "commentless thread must NOT advertise expansion"
    );
    assert!(
        !content.text.contains("0 comments"),
        "commentless thread must NOT show '0 comments'"
    );
}

// ── Multiple comment counts in hint ───────────────────────────────────────

/// A collapsed thread with multiple comments shows the plural "N comments".
#[test]
fn collapsed_thread_with_multiple_comments_shows_plural_count() {
    let mut detail = sample_detail();
    detail.reviews[0].review_threads = vec![PrReviewThread {
        thread_id: "T_multi".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/multi.rs".to_string()),
        line: Some(5),
        comments: vec![
            IssueComment {
                comment_id: 1,
                author_login: "a".to_string(),
                created_at: "2024-01-01".to_string(),
                edited_at: None,
                body: "first".to_string(),
            },
            IssueComment {
                comment_id: 2,
                author_login: "b".to_string(),
                created_at: "2024-01-02".to_string(),
                edited_at: None,
                body: "second".to_string(),
            },
        ],
    }];
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("2 comments"),
        "collapsed multi-comment thread must show '2 comments'"
    );
    assert!(
        !content.text.contains("first"),
        "collapsed thread must hide comment bodies"
    );
    assert!(
        !content.text.contains("second"),
        "collapsed thread must hide comment bodies"
    );
}

// ── Bodyless review headers render as non-focusable group labels ──────────

/// Build a PR 233-shaped detail: a bodyless COMMENTED review with one child
/// thread carrying `inline_body`.
fn bodyless_review_detail() -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 233,
        title: "Bodyless review PR".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "pat".to_string(),
        created_at: "2026-06-20".to_string(),
        updated_at: "2026-06-24".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: "https://github.com/owner/repo/pull/233".to_string(),
        review_decision: Some(PrReviewState::Commented),
        checks_status: PrCheckStatus::None,
        reviews: vec![PrReview {
            review_id: Some("PRR_1".to_string()),
            author_login: "bot".to_string(),
            state: PrReviewState::Commented,
            submitted_at: "2026-06-23".to_string(),
            body: None,
            review_threads: vec![unresolved_thread(
                "T1",
                "src/a.rs",
                1,
                "inline comment body",
            )],
        }],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}

/// A bodyless review (COMMENTED, no review-level body) renders as a compact
/// group/status label with NO focus marker (it is not a keyboard focus stop).
/// Its child threads still render.
#[test]
fn bodyless_review_header_has_no_focus_marker() {
    let detail = bodyless_review_detail();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    let review_line = lines
        .iter()
        .find(|l| l.contains("COMMENTED") && l.contains("bot"))
        .unwrap_or_else(|| {
            panic!(
                "bodyless review header must render, got content:\n{}",
                content.text
            )
        });
    assert!(
        !review_line.starts_with("> "),
        "bodyless review header must NOT have a focus marker, got: {review_line}"
    );
    assert!(
        !content.text.contains("inline comment body"),
        "thread body must be hidden when not focused"
    );
}
