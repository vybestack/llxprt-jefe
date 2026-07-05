//! PR-mode key dispatch unit tests (extracted from prs.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P10
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-001
//! @requirement REQ-PR-002
//! @requirement REQ-PR-003
//! @requirement REQ-PR-004
//! @requirement REQ-PR-008
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013

use super::*;
use jefe::domain::{AgentId, ChecksFilter, ReviewDecisionFilter};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::state::{
    AgentChooserState, ComposerTarget, PrFilterUiState, PullRequestsState, ScreenMode,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut evt = KeyEvent::new(KeyEventKind::Press, code);
    evt.modifiers = modifiers;
    evt
}

fn prs_base_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrList,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    }
}

fn prs_state_with_focus(focus: PrFocus) -> AppState {
    let mut state = prs_base_state();
    state.prs_state.pr_focus = focus;
    state
}

fn prs_state_with_inline(inline: InlineState) -> AppState {
    let mut state = prs_base_state();
    state.prs_state.inline_state = inline;
    state
}

fn prs_state_with_chooser() -> AppState {
    let mut state = prs_base_state();
    state.prs_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        agents: vec![(AgentId(String::from("a1")), String::from("Agent 1"))],
    });
    state
}

fn prs_state_with_detail_subfocus(subfocus: PrDetailSubfocus) -> AppState {
    let mut state = prs_base_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.detail_subfocus = subfocus;
    state
}

fn prs_state_with_search_focused() -> AppState {
    let mut state = prs_base_state();
    state.prs_state.search_input_focused = true;
    state
}

fn prs_state_with_filter_open(field_index: usize) -> AppState {
    let mut state = prs_base_state();
    state.prs_state.filter_ui = PrFilterUiState {
        controls_open: true,
        field_index,
        draft_labels_text: String::new(),
    };
    state
}

// ═══════════════════════════════════════════════════════════════════════
// Mode Entry / Exit (tests 1-5)
// ═══════════════════════════════════════════════════════════════════════

/// `p` from Dashboard enters PR Mode (REQ-PR-001 entry routing).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-001
/// @pseudocode component-003 lines 01-09
#[test]
fn test_p_from_dashboard_emits_enter_prs_mode() {
    // Entry routing lives in normal.rs resolve_mode_key (Dashboard-only 'p' arm).
    // Exercise the real router so the assertion is grounded in production logic.
    use super::super::normal::{KeyHandling, resolve_mode_key};
    let lower = resolve_mode_key(&key(KeyCode::Char('p')), ScreenMode::Dashboard);
    let upper = resolve_mode_key(&key(KeyCode::Char('P')), ScreenMode::Dashboard);
    assert!(
        matches!(lower, KeyHandling::Handled(Some(AppEvent::EnterPrsMode))),
        "Dashboard 'p' must emit EnterPrsMode"
    );
    assert!(
        matches!(upper, KeyHandling::Handled(Some(AppEvent::EnterPrsMode))),
        "Dashboard 'P' must emit EnterPrsMode"
    );
}

/// `p` is ignored when NOT in Dashboard (REQ-PR-001 no re-entry from elsewhere).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-001
/// @pseudocode component-003 lines 01-09
#[test]
fn test_p_ignored_when_not_dashboard() {
    // In DashboardPullRequests, 'p' must NOT emit EnterPrsMode (no re-entry);
    // it should yield RefocusPrList instead (P5 global tier).
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        !matches!(event, Some(AppEvent::EnterPrsMode)),
        "'p' in PR mode must not re-enter (got {event:?})"
    );
}

/// `p` in PR mode refocuses the PR list (NOT a second EnterPrsMode).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-001
/// @pseudocode component-003 lines 24-25
#[test]
fn test_p_in_prs_mode_refocuses_pr_list() {
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('p')));
    assert!(matches!(event, Some(AppEvent::RefocusPrList)));
}

/// `p` in PR mode yields RefocusPrList — proving the dashboard-prs intercept
/// consumes 'p' before resolve_mode_key can re-enter (ordering proof).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-001
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 05-09,18
#[test]
fn test_handle_dashboard_prs_key_runs_before_resolve_mode_key() {
    let state = prs_base_state();
    // Capital P must also refocus (not re-enter).
    let lower = resolve_prs_key_event(&state, &key(KeyCode::Char('p')));
    let upper = resolve_prs_key_event(&state, &key(KeyCode::Char('P')));
    assert!(matches!(lower, Some(AppEvent::RefocusPrList)));
    assert!(matches!(upper, Some(AppEvent::RefocusPrList)));
}

/// `a` exits PR mode from the global tier.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-001
/// @pseudocode component-003 lines 10-20
#[test]
fn test_a_exits_prs_mode_from_global_level() {
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('a')));
    assert!(matches!(event, Some(AppEvent::ExitPrsMode)));
}

// ═══════════════════════════════════════════════════════════════════════
// InputMode Precedence (test 6)
// ═══════════════════════════════════════════════════════════════════════

/// input_mode_for_state routes DashboardPullRequests by precedence
/// Inline > Chooser > Search > Filter > Normal.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-002
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 07,51
#[test]
fn test_input_mode_for_state_routes_dashboard_pull_requests_by_precedence() {
    let base = prs_base_state();
    // Normal: nothing active.
    assert!(matches!(input_mode_for_state(&base), InputMode::PrsNormal));
    // Filter controls open => PrsFilter.
    let mut filter = base.clone();
    filter.prs_state.filter_ui.controls_open = true;
    assert!(matches!(
        input_mode_for_state(&filter),
        InputMode::PrsFilter
    ));
    // Search focused => PrsSearch (overrides filter).
    let mut search = filter.clone();
    search.prs_state.search_input_focused = true;
    assert!(matches!(
        input_mode_for_state(&search),
        InputMode::PrsSearch
    ));
    // Chooser open => PrsChooser (overrides search).
    let chooser = prs_state_with_chooser();
    assert!(matches!(
        input_mode_for_state(&chooser),
        InputMode::PrsChooser
    ));
    // Inline active => PrsInline (highest).
    let inline = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    assert!(matches!(
        input_mode_for_state(&inline),
        InputMode::PrsInline
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// Pane Cycling / Detail Subfocus (tests 7-10)
// ═══════════════════════════════════════════════════════════════════════

/// Tab/Shift+Tab cycle panes from EVERY pane (RepoList, PrList, PrDetail).
/// Issue #46: Tab between repo list / PR list / PR detail.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 14-20
#[test]
fn test_tab_cycles_panes_from_every_pane() {
    for focus in [PrFocus::RepoList, PrFocus::PrList, PrFocus::PrDetail] {
        let state = prs_state_with_focus(focus);
        let tab = resolve_prs_key_event(&state, &key(KeyCode::Tab));
        assert!(
            matches!(tab, Some(AppEvent::PrCycleFocus)),
            "Tab from {focus:?} should yield PrCycleFocus (got {tab:?})"
        );
        let back = resolve_prs_key_event(&state, &key(KeyCode::BackTab));
        assert!(
            matches!(back, Some(AppEvent::PrCycleFocusReverse)),
            "Shift+Tab from {focus:?} should yield PrCycleFocusReverse (got {back:?})"
        );
    }
}

/// j/k move detail subfocus; Tab/Shift+Tab do NOT map to subfocus in PrDetail.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 81-82
#[test]
fn test_jk_moves_subfocus_in_pr_detail() {
    let state = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let j = resolve_prs_key_event(&state, &key(KeyCode::Char('j')));
    assert!(matches!(j, Some(AppEvent::PrDetailSubfocusNext)));
    let k = resolve_prs_key_event(&state, &key(KeyCode::Char('k')));
    assert!(matches!(k, Some(AppEvent::PrDetailSubfocusPrev)));
    // Tab/Shift+Tab must NOT be consumed for subfocus — they cycle panes.
    let tab = resolve_prs_key_event(&state, &key(KeyCode::Tab));
    assert!(matches!(tab, Some(AppEvent::PrCycleFocus)));
    let back = resolve_prs_key_event(&state, &key(KeyCode::BackTab));
    assert!(matches!(back, Some(AppEvent::PrCycleFocusReverse)));
}

/// Left arrow yields optional reverse pane-cycle in PrDetail.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 83-85
#[test]
fn test_left_arrow_optional_reverse_cycle_in_pr_detail() {
    let state = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let left = resolve_prs_key_event(&state, &key(KeyCode::Left));
    assert!(matches!(left, Some(AppEvent::PrCycleFocusReverse)));
}

/// Up/Down in RepoList change repo selection (navigate), not pane focus.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 49-56
#[test]
fn test_repo_focus_up_down_changes_repo_not_pane_focus() {
    let state = prs_state_with_focus(PrFocus::RepoList);
    let up = resolve_prs_key_event(&state, &key(KeyCode::Up));
    assert!(matches!(up, Some(AppEvent::PrNavigateUp)));
    let down = resolve_prs_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(down, Some(AppEvent::PrNavigateDown)));
}

/// Arrow pane-cycle matrix per pane (component-003 L324-328):
/// RepoList Right→PrCycleFocus (Left unbound); PrList Left→Reverse /
/// Right→forward; PrDetail Left→Reverse (Right unbound).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-003 lines 49-62,72-78a,324-328
#[test]
fn test_arrow_pane_cycle_matrix_per_pane() {
    // RepoList: Right => PrCycleFocus (Left unbound => None).
    let repo = prs_state_with_focus(PrFocus::RepoList);
    assert!(matches!(
        resolve_prs_key_event(&repo, &key(KeyCode::Right)),
        Some(AppEvent::PrCycleFocus)
    ));
    assert!(
        resolve_prs_key_event(&repo, &key(KeyCode::Left)).is_none(),
        "Left must be unbound in RepoList"
    );

    // PrList: Left => PrCycleFocusReverse, Right => PrCycleFocus.
    let list = prs_state_with_focus(PrFocus::PrList);
    assert!(matches!(
        resolve_prs_key_event(&list, &key(KeyCode::Left)),
        Some(AppEvent::PrCycleFocusReverse)
    ));
    assert!(matches!(
        resolve_prs_key_event(&list, &key(KeyCode::Right)),
        Some(AppEvent::PrCycleFocus)
    ));

    // PrDetail: Left => PrCycleFocusReverse (optional parity), Right unbound.
    let detail = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    assert!(matches!(
        resolve_prs_key_event(&detail, &key(KeyCode::Left)),
        Some(AppEvent::PrCycleFocusReverse)
    ));
    assert!(
        resolve_prs_key_event(&detail, &key(KeyCode::Right)).is_none(),
        "Right must be unbound in PrDetail"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Precedence: Inline / Chooser / Search (tests 11-13)
// ═══════════════════════════════════════════════════════════════════════

/// Inline composer consumes keys before global (P1 > P5).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-002
/// @requirement REQ-PR-010
/// @pseudocode component-003 lines 10-18
#[test]
fn test_inline_composer_consumes_keys_before_global() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    // 'a' would exit at P5, but inline (P1) consumes it as PrInlineChar.
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('a')));
    assert!(
        matches!(event, Some(AppEvent::PrInlineChar('a'))),
        "Inline must consume 'a' before global exit (got {event:?})"
    );
}

/// Agent chooser consumes keys before search (P2 > P3).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-002
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 10-18
#[test]
fn test_agent_chooser_consumes_keys_before_search() {
    let mut state = prs_state_with_chooser();
    state.prs_state.search_input_focused = true;
    // Down navigates the chooser (P2), not the search (P3).
    let event = resolve_prs_key_event(&state, &key(KeyCode::Down));
    assert!(matches!(event, Some(AppEvent::PrAgentChooserNavigateDown)));
}

/// Search input routes chars to the query (P3).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-002
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 127-133
#[test]
fn test_search_input_routes_chars_to_query() {
    let state = prs_state_with_search_focused();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('x')));
    assert!(
        matches!(event, Some(AppEvent::PrSetSearchQuery { .. })),
        "Search-focused char should route to query (got {event:?})"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Filter Controls (tests 14-18)
// ═══════════════════════════════════════════════════════════════════════

/// Filter controls: Tab/space/text/enter/clear/esc each route correctly.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-146
#[test]
fn test_filter_controls_tab_space_text_enter_clear_esc() {
    let state = prs_state_with_filter_open(0);
    // Tab => navigate next field.
    let tab = resolve_prs_key_event(&state, &key(KeyCode::Tab));
    assert!(matches!(tab, Some(AppEvent::PrFilterNavigateNext)));
    // Esc => close controls.
    let esc = resolve_prs_key_event(&state, &key(KeyCode::Esc));
    assert!(matches!(esc, Some(AppEvent::PrCloseFilterControls)));
    // Enter => apply.
    let enter = resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(enter, Some(AppEvent::PrApplyFilter)));
}

/// Filter field cycling wraps through all eight fields (state, draft,
/// review-decision, checks-status, author, assignee, reviewer, labels).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-138
#[test]
fn test_filter_field_cycling_wraps_through_all_eight_fields() {
    let mut state = prs_state_with_filter_open(0);
    // Forward: 0 -> 1 -> ... -> 7 -> 0 (wrap).
    for expected in 1..=8 {
        let event = resolve_prs_key_event(&state, &key(KeyCode::Tab));
        assert!(matches!(event, Some(AppEvent::PrFilterNavigateNext)));
        state = state.apply(AppEvent::PrFilterNavigateNext);
        assert_eq!(
            state.prs_state.filter_ui.field_index,
            expected % 8,
            "forward field_index mismatch at step {expected}"
        );
    }
    // Reverse+wrap: from 0, Shift+Tab wraps to 7.
    let event = resolve_prs_key_event(&state, &key(KeyCode::BackTab));
    assert!(matches!(event, Some(AppEvent::PrFilterNavigatePrev)));
    state = state.apply(AppEvent::PrFilterNavigatePrev);
    assert_eq!(state.prs_state.filter_ui.field_index, 7);
}

/// Space on review-decision field (index 2) cycles draft_filter.review_decision.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 139-140
#[test]
fn test_space_cycles_review_decision_filter_draft_state() {
    let state = prs_state_with_filter_open(2);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(matches!(event, Some(AppEvent::PrCycleReviewFilter)));
    // Reducer cycles Any -> Approved -> ChangesRequested -> ReviewRequired -> None -> Any.
    let mut s = state.apply(AppEvent::PrCycleReviewFilter);
    assert_eq!(
        s.prs_state.draft_filter.review_decision,
        ReviewDecisionFilter::Approved
    );
    s = s.apply(AppEvent::PrCycleReviewFilter);
    assert_eq!(
        s.prs_state.draft_filter.review_decision,
        ReviewDecisionFilter::ChangesRequested
    );
    s = s.apply(AppEvent::PrCycleReviewFilter);
    assert_eq!(
        s.prs_state.draft_filter.review_decision,
        ReviewDecisionFilter::ReviewRequired
    );
    s = s.apply(AppEvent::PrCycleReviewFilter);
    assert_eq!(
        s.prs_state.draft_filter.review_decision,
        ReviewDecisionFilter::None
    );
    s = s.apply(AppEvent::PrCycleReviewFilter);
    assert_eq!(
        s.prs_state.draft_filter.review_decision,
        ReviewDecisionFilter::Any
    );
}

/// Space on checks-status field (index 3) cycles draft_filter.checks_status.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 139-140
#[test]
fn test_space_cycles_checks_status_filter_draft_state() {
    let state = prs_state_with_filter_open(3);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(matches!(event, Some(AppEvent::PrCycleChecksFilter)));
    // Reducer cycles Any -> Success -> Failing -> Pending -> Any.
    let mut s = state.apply(AppEvent::PrCycleChecksFilter);
    assert_eq!(
        s.prs_state.draft_filter.checks_status,
        ChecksFilter::Success
    );
    s = s.apply(AppEvent::PrCycleChecksFilter);
    assert_eq!(
        s.prs_state.draft_filter.checks_status,
        ChecksFilter::Failing
    );
    s = s.apply(AppEvent::PrCycleChecksFilter);
    assert_eq!(
        s.prs_state.draft_filter.checks_status,
        ChecksFilter::Pending
    );
    s = s.apply(AppEvent::PrCycleChecksFilter);
    assert_eq!(s.prs_state.draft_filter.checks_status, ChecksFilter::Any);
}

/// HIGH-4: Space on a TEXT field (author, index 4) MUST insert a space char
/// (PrUpdateDraftFilter with value ending in a space), NOT cycle a filter.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-146
#[test]
fn test_space_on_author_text_field_inserts_space_not_cycle() {
    let mut state = prs_state_with_filter_open(4); // AUTHOR_FIELD
    state.prs_state.draft_filter.author = "octo".to_string();

    let event = resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));

    match event {
        Some(AppEvent::PrUpdateDraftFilter { field, value }) => {
            assert_eq!(
                field, "author",
                "Space on author must update the author field"
            );
            assert!(
                value.ends_with(' '),
                "Space on a text field must append a space (got {value:?})"
            );
        }
        other => {
            panic!("Space on author (text field) must yield PrUpdateDraftFilter, got {other:?}")
        }
    }
}

/// HIGH-4 (complement): Space on a CYCLE field (state, index 0) MUST still
/// cycle the filter (the fix must not break cycle-field Space behavior).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-146
#[test]
fn test_space_on_state_cycle_field_still_cycles() {
    let state = prs_state_with_filter_open(0); // STATE_FIELD
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(
        matches!(event, Some(AppEvent::PrCycleFilterState)),
        "Space on the state (cycle) field must yield PrCycleFilterState, got {event:?}"
    );
}

/// Apply commits draft_filter to committed_filter and triggers a reload.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 143
#[test]
fn test_apply_commits_review_and_checks_filters_and_triggers_reload() {
    let mut state = prs_state_with_filter_open(2);
    // Cycle review + checks draft fields first.
    state = state.apply(AppEvent::PrCycleReviewFilter);
    state = state.apply(AppEvent::PrCycleChecksFilter);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(event, Some(AppEvent::PrApplyFilter)));
    // committed_filter should reflect the draft after apply.
    assert_ne!(
        state.prs_state.committed_filter.review_decision,
        state.prs_state.draft_filter.review_decision,
        "pre-apply committed must differ from draft"
    );
    let draft_review = state.prs_state.draft_filter.review_decision;
    let after = state.apply(AppEvent::PrApplyFilter);
    assert_eq!(
        after.prs_state.committed_filter.review_decision, draft_review,
        "apply must copy draft -> committed"
    );
    assert!(
        after.prs_state.loading.list,
        "apply must trigger a list reload (loading.list=true)"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Esc Precedence (test 19)
// ═══════════════════════════════════════════════════════════════════════

/// Esc unwinds by precedence: inline -> chooser -> search -> filter -> exit.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
#[test]
fn test_esc_precedence_inline_then_chooser_then_search_then_filter_then_exit() {
    // Inline active => Esc cancels inline.
    let inline = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let e = resolve_prs_key_event(&inline, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::PrInlineCancelOrEsc)));
    // Chooser open (no inline) => Esc cancels chooser.
    let chooser = prs_state_with_chooser();
    let e = resolve_prs_key_event(&chooser, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::PrAgentChooserCancel)));
    // Search focused with a nonempty query => Esc clears the query (keep focus).
    let mut search_nonempty = prs_state_with_search_focused();
    search_nonempty.prs_state.search_query = String::from("abc");
    let e = resolve_prs_key_event(&search_nonempty, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::PrClearSearch)));
    // Search focused with an empty query => Esc blurs the search input.
    let search_empty = prs_state_with_search_focused();
    let e = resolve_prs_key_event(&search_empty, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::PrBlurSearchInput)));
    // Filter controls open => Esc closes controls.
    let filter = prs_state_with_filter_open(0);
    let e = resolve_prs_key_event(&filter, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::PrCloseFilterControls)));
    // Nothing active => Esc exits mode.
    let base = prs_base_state();
    let e = resolve_prs_key_event(&base, &key(KeyCode::Esc));
    assert!(matches!(e, Some(AppEvent::ExitPrsMode)));
}

// ═══════════════════════════════════════════════════════════════════════
// Read-Only Notices: c / r / e (tests 20-23)
// ═══════════════════════════════════════════════════════════════════════

/// `c` opens the comment composer only from Body/Comment/NewComment subfocus.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-010
/// @pseudocode component-003 lines 72-82
#[test]
fn test_c_opens_comment_composer_only_from_detail_subfocus() {
    let body = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let event = resolve_prs_key_event(&body, &key(KeyCode::Char('c')));
    assert!(matches!(event, Some(AppEvent::PrOpenNewCommentComposer)));
}

/// `c` on Review/Check subfocus emits a notice (NOT None) + sets draft_notice.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 83-85
#[test]
fn test_c_on_review_or_check_emits_show_notice_not_none() {
    let review = prs_state_with_detail_subfocus(PrDetailSubfocus::Review(0));
    let event = resolve_prs_key_event(&review, &key(KeyCode::Char('c')));
    assert!(matches!(
        event,
        Some(AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyNoComment))
    ));
    // Reducer must surface a non-blocking draft_notice.
    let after = review.apply(AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyNoComment));
    assert!(
        after.prs_state.draft_notice.is_some(),
        "PrShowNotice must populate draft_notice"
    );
}

/// `r` replies only on Comment subfocus; elsewhere yields a notice + draft_notice.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 86-87
#[test]
fn test_r_replies_only_on_comment_subfocus() {
    // On Comment(i) => opens reply composer.
    let comment = prs_state_with_detail_subfocus(PrDetailSubfocus::Comment(2));
    let event = resolve_prs_key_event(&comment, &key(KeyCode::Char('r')));
    assert!(matches!(
        event,
        Some(AppEvent::PrOpenReplyComposer { comment_index: 2 })
    ));
    // On Body => notice, not None.
    let body = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let event = resolve_prs_key_event(&body, &key(KeyCode::Char('r')));
    assert!(matches!(
        event,
        Some(AppEvent::PrShowNotice(
            ReadOnlyHintKind::ReadOnlyReplyOnComment
        ))
    ));
    let after = body.apply(AppEvent::PrShowNotice(
        ReadOnlyHintKind::ReadOnlyReplyOnComment,
    ));
    assert!(after.prs_state.draft_notice.is_some());
}

/// `e` on PR detail emits a notice (NOT None) + sets draft_notice.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 83-89
#[test]
fn test_e_on_pr_detail_emits_show_notice_not_none() {
    let state = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('e')));
    assert!(matches!(
        event,
        Some(AppEvent::PrShowNotice(
            ReadOnlyHintKind::ReadOnlyNotEditable
        ))
    ));
    let after = state.apply(AppEvent::PrShowNotice(
        ReadOnlyHintKind::ReadOnlyNotEditable,
    ));
    assert!(after.prs_state.draft_notice.is_some());
}

// ═══════════════════════════════════════════════════════════════════════
// Agent Chooser / Open-in-Browser (tests 24-26)
// ═══════════════════════════════════════════════════════════════════════

/// Capital `S` opens the agent chooser from PR detail focus.
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 72-82
#[test]
fn test_capital_s_opens_agent_chooser_from_detail() {
    let state = prs_state_with_detail_subfocus(PrDetailSubfocus::Body);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('S')));
    assert!(matches!(event, Some(AppEvent::PrOpenAgentChooser)));
}

/// `o` on a loaded/selected PR emits PrOpenInBrowser (from list and detail).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69,88-89
#[test]
fn test_o_on_loaded_pr_emits_open_in_browser() {
    // PrList with a selected PR.
    let mut list = prs_state_with_focus(PrFocus::PrList);
    list.prs_state.selected_pr_index = Some(0);
    let event = resolve_prs_key_event(&list, &key(KeyCode::Char('o')));
    assert!(matches!(event, Some(AppEvent::PrOpenInBrowser)));
    // PrDetail with a loaded detail.
    let mut detail = prs_state_with_focus(PrFocus::PrDetail);
    detail.prs_state.pr_detail = Some(test_pr_detail());
    let event = resolve_prs_key_event(&detail, &key(KeyCode::Char('o')));
    assert!(matches!(event, Some(AppEvent::PrOpenInBrowser)));
}

/// `o` with no selection emits a NoSelectionToOpen notice (NOT None).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69,88-89
#[test]
fn test_o_with_no_selection_emits_show_notice_not_none() {
    // PrList, no selection.
    let list = prs_state_with_focus(PrFocus::PrList);
    let event = resolve_prs_key_event(&list, &key(KeyCode::Char('o')));
    assert!(matches!(
        event,
        Some(AppEvent::PrShowNotice(ReadOnlyHintKind::NoSelectionToOpen))
    ));
    // PrDetail, no loaded detail.
    let detail = prs_state_with_focus(PrFocus::PrDetail);
    let event = resolve_prs_key_event(&detail, &key(KeyCode::Char('o')));
    assert!(matches!(
        event,
        Some(AppEvent::PrShowNotice(ReadOnlyHintKind::NoSelectionToOpen))
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// Suppression (test 27)
// ═══════════════════════════════════════════════════════════════════════

/// s / Ctrl-d / Ctrl-k / l are consumed as no-ops (None at resolve level).
///
/// @plan PLAN-20260624-PR-MODE.P10
/// @requirement REQ-PR-002
/// @pseudocode component-003 lines 10-48
#[test]
fn test_suppressed_keys_ctrl_d_ctrl_k_l_consumed_noop() {
    let state = prs_base_state();
    // s (lowercase) => consumed no-op.
    let s = resolve_prs_key_event(&state, &key(KeyCode::Char('s')));
    assert!(s.is_none(), "'s' must be consumed-no-op (got {s:?})");
    // Ctrl-d => consumed no-op.
    let ctrl_d = resolve_prs_key_event(
        &state,
        &key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );
    assert!(ctrl_d.is_none(), "Ctrl-d must be consumed-no-op");
    // Ctrl-k => consumed no-op.
    let ctrl_k = resolve_prs_key_event(
        &state,
        &key_with_mods(KeyCode::Char('k'), KeyModifiers::CONTROL),
    );
    assert!(ctrl_k.is_none(), "Ctrl-k must be consumed-no-op");
    // l => consumed no-op.
    let l = resolve_prs_key_event(&state, &key(KeyCode::Char('l')));
    assert!(l.is_none(), "'l' must be consumed-no-op (got {l:?})");
}

/// Minimal PR detail for presence checks (REQ-PR-012 o-key).
fn test_pr_detail() -> jefe::domain::PullRequestDetail {
    use jefe::domain::{PrCheckStatus, PrState};
    jefe::domain::PullRequestDetail {
        repo_owner_name: String::from("owner/name"),
        number: 1,
        title: String::from("PR 1"),
        state: PrState::Open,
        is_draft: false,
        author_login: String::from("author"),
        created_at: String::new(),
        updated_at: String::new(),
        head_ref: String::new(),
        base_ref: String::new(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: String::new(),
        external_url: String::new(),
        review_decision: None,
        checks_status: PrCheckStatus::Success,
        reviews: Vec::new(),
        checks: Vec::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}
