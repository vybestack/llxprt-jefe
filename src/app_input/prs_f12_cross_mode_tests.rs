//! F12 mode-aware behavior + cross-mode `i` navigation in PR mode (issue #164).
//!
//! Extracted from `prs_key_tests.rs` to keep file sizes under the project's
//! source-file-size hard limit. These tests exercise the pure
//! `resolve_prs_key_event` resolver — they assert which `AppEvent` (if any)
//! a given key produces for a given PR-mode state.

use super::*;

// ═══════════════════════════════════════════════════════════════════════
// F12 mode-aware behavior + cross-mode `i` (issue #164)
// ═══════════════════════════════════════════════════════════════════════

/// F12 in PrDetail focus returns to the PR list (issue #164).
#[test]
fn f12_in_pr_detail_returns_to_list() {
    let state = prs_state_with_focus(PrFocus::PrDetail);
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::RefocusPrList)),
        "F12 in PrDetail must yield RefocusPrList, got {event:?}"
    );
}

/// F12 at the PR list with the terminal unfocused is a no-op (issue #164).
#[test]
fn f12_in_pr_list_is_noop() {
    let mut state = prs_base_state();
    state.terminal_focused = false;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 at PrList (terminal unfocused) must be None, got {event:?}"
    );
}

/// F12 while the terminal is focused defocuses it (issue #164).
#[test]
fn f12_while_terminal_focused_defocuses() {
    let mut state = prs_base_state();
    state.terminal_focused = true;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus, got {event:?}"
    );
}

/// F12 does not fire when the inline composer is open (overlay owns the key).
#[test]
fn f12_does_not_fire_when_inline_composer_open() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 must be suppressed by inline composer, got {event:?}"
    );
}

/// `i` from PR mode enters Issues mode (issue #164 cross-mode navigation).
#[test]
fn i_from_prs_enters_issues_mode() {
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::EnterIssuesMode)),
        "'i' from PRs must yield EnterIssuesMode, got {event:?}"
    );
}

/// `p` from PrDetail still refocuses the PR list (regression, issue #164).
#[test]
fn p_from_prs_still_refocuses_list() {
    let state = prs_state_with_focus(PrFocus::PrDetail);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::RefocusPrList)),
        "'p' in PrDetail must yield RefocusPrList, got {event:?}"
    );
}

// ─── Overlay precedence for cross-mode keys (issue #164 review Finding 4) ──

/// F12 while the terminal is focused AND in PrDetail defocuses the terminal
/// first (one-layer-at-a-time). The detail view stays — only the terminal
/// defocus wins.
#[test]
fn f12_while_terminal_focused_and_in_detail_defocuses_terminal_first() {
    let mut state = prs_state_with_focus(PrFocus::PrDetail);
    state.terminal_focused = true;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus even in PrDetail, got {event:?}"
    );
}

/// `i` while the search input is focused types into the query — it must NOT
/// switch to Issues mode (overlay owns the key before the global tier).
#[test]
fn i_in_search_input_does_not_switch_modes() {
    let state = prs_state_with_search_focused();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::PrSetSearchQuery { .. })),
        "'i' with search focused must yield PrSetSearchQuery, got {event:?}"
    );
    assert!(
        !matches!(event, Some(AppEvent::EnterIssuesMode)),
        "'i' with search focused must NOT yield EnterIssuesMode"
    );
}

/// `i` while the inline composer is active types the character into the
/// composer — it must NOT switch to Issues mode.
#[test]
fn i_in_inline_composer_types_char() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::PrInlineChar('i'))),
        "'i' with inline composer must yield PrInlineChar('i'), got {event:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Cross-mode `g`/`G`: jump to Actions mode pre-filtered to current PR (#205)
// ═══════════════════════════════════════════════════════════════════════

use jefe::domain::{PrCheckStatus, PrState, PullRequest, PullRequestDetail};

/// Build a PR-mode state with a loaded PR detail (no list items).
fn prs_state_with_pr_detail(number: u64, head_sha: &str) -> AppState {
    let mut state = prs_state_with_focus(PrFocus::PrDetail);
    state.prs_state.pr_detail = Some(PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: head_sha.to_string(),
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
        comments: jefe::domain::PaginatedList::default(),
        mergeable: None,
        merge_state_status: None,
    });
    state
}

/// `g` from PR detail enters Actions mode with the PR's number and head SHA.
#[test]
fn g_from_pr_detail_enters_actions_with_pr_filter() {
    let state = prs_state_with_pr_detail(42, "sha123");
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('g')));
    assert!(
        matches!(
            event,
            Some(AppEvent::EnterActionsModeWithPrFilter { pr_number: 42, ref head_sha }) if head_sha == "sha123"
        ),
        "'g' from PR detail must yield EnterActionsModeWithPrFilter with PR 42 and sha123, got {event:?}"
    );
}

/// `G` (uppercase) also triggers the cross-mode action.
#[test]
fn uppercase_g_from_pr_detail_enters_actions_with_pr_filter() {
    let state = prs_state_with_pr_detail(7, "sha7");
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('G')));
    assert!(
        matches!(
            event,
            Some(AppEvent::EnterActionsModeWithPrFilter { pr_number: 7, .. })
        ),
        "'G' from PR detail must yield EnterActionsModeWithPrFilter, got {event:?}"
    );
}

/// `g` from the PR list (no detail loaded) uses the selected PR's SHA.
#[test]
fn g_from_pr_list_uses_selected_pr() {
    let mut state = prs_base_state();
    state.prs_state.list.replace_items(vec![PullRequest {
        number: 99,
        title: "PR #99".to_string(),
        state: PrState::Open,
        author_login: "user".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "listsha".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }]);
    state.prs_state.list.set_selected_index(Some(0));

    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('g')));
    assert!(
        matches!(
            event,
            Some(AppEvent::EnterActionsModeWithPrFilter { pr_number: 99, ref head_sha }) if head_sha == "listsha"
        ),
        "'g' from PR list must yield EnterActionsModeWithPrFilter with the selected PR's SHA, got {event:?}"
    );
}

/// `g` with no PR selected and no detail enters Actions mode without a filter.
#[test]
fn g_with_no_pr_enters_actions_without_filter() {
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('g')));
    assert!(
        matches!(event, Some(AppEvent::EnterActionsMode)),
        "'g' with no PR selected must yield plain EnterActionsMode, got {event:?}"
    );
}

/// `g` does not fire when the inline composer is open (overlay owns the key).
#[test]
fn g_does_not_fire_when_inline_composer_open() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('g')));
    assert!(
        matches!(event, Some(AppEvent::PrInlineChar('g'))),
        "'g' with inline composer must yield PrInlineChar('g'), got {event:?}"
    );
}

/// Build a `PullRequest` with the given number and head SHA (all other
/// fields are defaulted for test purposes).
fn make_list_pr(number: u64, head_sha: &str) -> PullRequest {
    PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "user".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: format!("feature{number}"),
        head_sha: head_sha.to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// `g` uses the selected list PR when `pr_detail` is stale (CodeRabbit
/// finding): list navigation keeps the previous detail around, so the
/// selection may point to a different PR than `pr_detail`.
#[test]
fn g_prefers_selected_pr_over_stale_detail() {
    let mut state = prs_base_state();
    state.prs_state.pr_detail = Some(PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        title: "PR #1".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature1".to_string(),
        head_sha: "detail_sha_1".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: String::new(),
        external_url: "https://github.com/owner/repo/pull/1".to_string(),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: jefe::domain::PaginatedList::default(),
        mergeable: None,
        merge_state_status: None,
    });
    state.prs_state.list.replace_items(vec![
        make_list_pr(1, "list_sha_1"),
        make_list_pr(2, "list_sha_2"),
    ]);
    state.prs_state.list.set_selected_index(Some(1));

    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('g')));
    assert!(
        matches!(
            event,
            Some(AppEvent::EnterActionsModeWithPrFilter { pr_number: 2, ref head_sha }) if head_sha == "list_sha_2"
        ),
        "'g' must use the selected list PR (#2) when pr_detail is stale (#1), got {event:?}"
    );
}
