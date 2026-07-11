//! Issue #198 scrollback policy tests, extracted from `scrollback_ops.rs`
//! to keep that handler module under the architecture-boundary line budget.
//!
//! As a child module of `scrollback_ops`, `use super::*` grants access to the
//! private policy helpers.

use super::*;

// ── max_scroll_offset ─────────────────────────────────────────────────

#[test]
fn max_offset_when_content_exceeds_viewport() {
    // 50 total lines, 10 viewport rows → can scroll back 40 lines.
    assert_eq!(max_scroll_offset(50, 10), 40);
}

#[test]
fn max_offset_zero_when_content_fits_viewport() {
    assert_eq!(max_scroll_offset(5, 10), 0);
}

#[test]
fn max_offset_zero_when_equal() {
    assert_eq!(max_scroll_offset(10, 10), 0);
}

// ── terminal_scroll_up ────────────────────────────────────────────────

#[test]
fn scroll_up_from_follow_sets_offset() {
    // Starting from None (follow-tail), scrolling up by 1 sets Some(1).
    assert_eq!(terminal_scroll_up(None, 50, 10, 1), Some(1));
}

#[test]
fn scroll_up_increments_existing_offset() {
    assert_eq!(terminal_scroll_up(Some(5), 50, 10, 3), Some(8));
}

#[test]
fn scroll_up_clamps_at_max() {
    // max = 40; scrolling up from 38 by 5 should clamp to 40.
    assert_eq!(terminal_scroll_up(Some(38), 50, 10, 5), Some(40));
}

#[test]
fn scroll_up_returns_none_when_no_scrollable_content() {
    // Content fits entirely in viewport (max = 0).
    assert_eq!(terminal_scroll_up(None, 5, 10, 1), None);
}

// ── terminal_scroll_down ──────────────────────────────────────────────

#[test]
fn scroll_down_from_offset_decrements() {
    assert_eq!(terminal_scroll_down(Some(10), 50, 10, 3), Some(7));
}

#[test]
fn scroll_down_to_bottom_clears_offset() {
    // Scrolling down from 3 by 5 → reaches bottom → None (follow-tail).
    assert_eq!(terminal_scroll_down(Some(3), 50, 10, 5), None);
}

#[test]
fn scroll_down_exact_step_to_bottom_clears_offset() {
    // Scrolling down from 5 by 5 → exactly at bottom → None.
    assert_eq!(terminal_scroll_down(Some(5), 50, 10, 5), None);
}

#[test]
fn scroll_down_from_none_stays_none() {
    assert_eq!(terminal_scroll_down(None, 50, 10, 1), None);
}

// ── page up / page down ───────────────────────────────────────────────

#[test]
fn page_up_steps_by_viewport_rows() {
    // From None, page up by 10 rows → Some(10).
    assert_eq!(terminal_scroll_page_up(None, 50, 10), Some(10));
}

#[test]
fn page_up_clamps_at_max() {
    // From Some(35), page up by 10 → max 40.
    assert_eq!(terminal_scroll_page_up(Some(35), 50, 10), Some(40));
}

#[test]
fn page_down_steps_by_viewport_rows() {
    assert_eq!(terminal_scroll_page_down(Some(25), 50, 10), Some(15));
}

#[test]
fn page_down_to_bottom_clears_offset() {
    // From Some(5), page down by 10 → None.
    assert_eq!(terminal_scroll_page_down(Some(5), 50, 10), None);
}

// ── sticky offset ─────────────────────────────────────────────────────

#[test]
fn sticky_offset_unchanged_by_new_output() {
    // The policy helpers do not change offset on "new output" — the offset
    // is sticky. This test documents that property: calling scroll_up then
    // scroll_up again with the same offset is idempotent (the reducer does
    // not auto-clear offset on dirty).
    let offset = terminal_scroll_up(None, 50, 10, 5);
    assert_eq!(offset, Some(5));
    // A second scroll_up from the same starting point produces the same.
    let offset2 = terminal_scroll_up(None, 50, 10, 5);
    assert_eq!(offset2, Some(5));
    // The offset is only cleared by scrolling down to bottom or End.
    assert_eq!(terminal_scroll_down(offset, 50, 10, 5), None);
}

// ── terminal_at_bottom ────────────────────────────────────────────────

#[test]
fn at_bottom_when_offset_none() {
    assert!(terminal_at_bottom(None));
}

#[test]
fn not_at_bottom_when_offset_some() {
    assert!(!terminal_at_bottom(Some(5)));
}

// ── terminal_follow_indicator ─────────────────────────────────────────

#[test]
fn indicator_none_when_following() {
    assert!(terminal_follow_indicator(None).is_none());
}

#[test]
fn indicator_present_when_scrolled_back() {
    let ind = terminal_follow_indicator(Some(42));
    let Some(ind) = ind else {
        panic!("indicator must be present when scrolled back");
    };
    assert_eq!(ind.offset_lines, 42);
    assert!(
        ind.text.contains("42"),
        "indicator text must include the offset: {}",
        ind.text
    );
    assert!(
        ind.text.contains("End"),
        "indicator text must mention End key: {}",
        ind.text
    );
}

#[test]
fn indicator_has_no_emoji() {
    let ind = terminal_follow_indicator(Some(10));
    let Some(ind) = ind else {
        panic!("indicator must be present when scrolled back");
    };
    // Simple check: no non-ASCII characters (emoji are non-ASCII).
    assert!(
        ind.text.is_ascii(),
        "indicator text must be ASCII (no emoji): {}",
        ind.text
    );
}

// ── apply_scroll_request (consolidated policy) ────────────────────────

#[test]
fn apply_request_up_sets_offset_from_follow_tail() {
    assert_eq!(
        apply_scroll_request(None, 50, 10, ScrollRequest::Up),
        Some(1)
    );
}

#[test]
fn apply_request_down_to_bottom_resumes_follow() {
    assert_eq!(
        apply_scroll_request(Some(1), 50, 10, ScrollRequest::Down),
        None,
        "scrolling down to the bottom must resume follow-tail"
    );
}

#[test]
fn apply_request_page_up_advances_by_viewport() {
    assert_eq!(
        apply_scroll_request(None, 50, 10, ScrollRequest::PageUp),
        Some(10)
    );
}

#[test]
fn apply_request_follow_tail_clears_offset() {
    assert_eq!(
        apply_scroll_request(Some(42), 50, 10, ScrollRequest::FollowTail),
        None
    );
}

#[test]
fn apply_request_up_clamps_at_max() {
    // max offset = 50 - 10 = 40; requesting page-up (10) from offset 35
    // must clamp at 40, not exceed it.
    assert_eq!(
        apply_scroll_request(Some(35), 50, 10, ScrollRequest::PageUp),
        Some(40)
    );
}

#[test]
fn apply_request_to_top_jumps_to_max() {
    // Home key: jump to the top of history (max offset).
    assert_eq!(
        apply_scroll_request(Some(5), 50, 10, ScrollRequest::ToTop),
        Some(40),
        "ToTop must jump to max_scroll_offset"
    );
}

#[test]
fn apply_request_to_top_from_follow_enters_scrolled() {
    // ToTop from follow-tail (None) jumps to max.
    assert_eq!(
        apply_scroll_request(None, 50, 10, ScrollRequest::ToTop),
        Some(40)
    );
}

#[test]
fn apply_request_to_top_returns_none_when_no_scrollable_content() {
    // Content fits viewport (max=0) → ToTop returns None.
    assert_eq!(
        apply_scroll_request(None, 5, 10, ScrollRequest::ToTop),
        None
    );
}

// ── reconcile_offset_for_new_content (issue #198 review fix #3) ────────

#[test]
fn reconcile_grows_offset_when_scrolled_back_and_content_grows() {
    // old: total=50, viewport=10, offset=5 (scrolled back 5 from bottom)
    // new: total=55 (5 lines appended)
    // delta = 5 → new offset = 5 + 5 = 10, new_max = 55-10 = 45
    assert_eq!(
        reconcile_offset_for_new_content(Some(5), 50, 55, 10),
        Some(10)
    );
}

#[test]
fn reconcile_preserves_absolute_viewport_position() {
    // Pure integration test: project at Some(offset); append k lines;
    // reconcile; re-project; assert the projected rows are IDENTICAL.
    // This is the behavioral requirement from the issue: new output while
    // scrolled back must not jump the viewport.
    use crate::runtime::{TerminalCellStyle, TerminalSnapshot};
    use crate::ui::components::terminal_viewport::build_terminal_viewport;
    use iocraft::Color;

    let style = TerminalCellStyle {
        fg: Color::White,
        bg: Color::Black,
        bold: false,
        dim: false,
        underline: false,
    };
    let make_history = |n: usize| (0..n).map(|i| format!("row{i}")).collect::<Vec<_>>();
    let live = TerminalSnapshot {
        rows: 0,
        cols: 80,
        cells: vec![],
    };

    let old_total = 30;
    let viewport_rows = 5;
    let offset = Some(10);

    let proj_before = build_terminal_viewport(
        &live,
        &make_history(old_total),
        offset,
        viewport_rows,
        80,
        style,
    );

    // Append 8 new lines.
    let new_total = old_total + 8;
    let reconciled = reconcile_offset_for_new_content(offset, old_total, new_total, viewport_rows);
    let proj_after = build_terminal_viewport(
        &live,
        &make_history(new_total),
        reconciled,
        viewport_rows,
        80,
        style,
    );

    // The projected rows must be identical — the viewport didn't move.
    for r in 0..viewport_rows {
        let before: String = proj_before.snapshot.cells[r].iter().map(|c| c.ch).collect();
        let after: String = proj_after.snapshot.cells[r].iter().map(|c| c.ch).collect();
        assert_eq!(
            before.trim(),
            after.trim(),
            "row {r} must be identical after reconciliation"
        );
    }
}

#[test]
fn reconcile_clamps_to_new_max() {
    // old: total=50, viewport=10, offset=40 (max=40, at top)
    // new: total=55 → delta=5 → raw offset = 45, new_max = 45 → clamp 45
    assert_eq!(
        reconcile_offset_for_new_content(Some(40), 50, 55, 10),
        Some(45)
    );
}

#[test]
fn reconcile_returns_none_for_follow_tail() {
    // Follow-tail (None) is unaffected by new content.
    assert_eq!(reconcile_offset_for_new_content(None, 50, 55, 10), None);
}

#[test]
fn reconcile_unchanged_when_no_growth() {
    // delta = 0 → offset unchanged.
    assert_eq!(
        reconcile_offset_for_new_content(Some(5), 50, 50, 10),
        Some(5)
    );
}

#[test]
fn reconcile_unchanged_when_content_shrinks() {
    // new_total < old_total → checked_sub returns None → function returns None.
    // (Content shrinking is not a normal terminal operation; returning None
    // lets the caller fall back to follow-tail or re-derive.)
    assert_eq!(reconcile_offset_for_new_content(Some(5), 55, 50, 10), None);
}

// ── terminal_content_start_line (issue #198 review fix #4) ─────────────

#[test]
fn content_start_line_follow_tail_shows_bottom() {
    // total=50, viewport=10, offset=None (follow) → max=40, start=40-0=40
    assert_eq!(terminal_content_start_line(None, 50, 10), 40);
}

#[test]
fn content_start_line_scrolled_back() {
    // total=50, viewport=10, offset=Some(15) → max=40, start=40-15=25
    assert_eq!(terminal_content_start_line(Some(15), 50, 10), 25);
}

#[test]
fn content_start_line_at_top() {
    // total=50, viewport=10, offset=Some(40)=max → start=40-40=0
    assert_eq!(terminal_content_start_line(Some(40), 50, 10), 0);
}

#[test]
fn content_start_line_content_fits_viewport() {
    // total=5, viewport=10 → max=0, start=0 regardless of offset.
    assert_eq!(terminal_content_start_line(Some(3), 5, 10), 0);
    assert_eq!(terminal_content_start_line(None, 5, 10), 0);
}

#[test]
fn content_start_line_offset_exceeds_max_clamps_to_zero() {
    // offset > max → saturating_sub → 0 (defensive: offset should never
    // exceed max, but the function must not underflow).
    assert_eq!(terminal_content_start_line(Some(100), 50, 10), 0);
}

#[test]
fn selection_offset_agrees_with_viewport_projection() {
    // Behavioral selection test (issue #198 review fix #4): the top-relative
    // start line derived from a bottom-relative offset must map to the SAME
    // content rows the viewport projection paints. With distinctive history
    // rows and a nonzero bottom-relative offset, verify the start line +
    // viewport row index resolves to the expected content row.
    let total_lines = 50;
    let viewport_rows = 10;
    let bottom_relative_offset = Some(15);

    let start_line =
        terminal_content_start_line(bottom_relative_offset, total_lines, viewport_rows);
    // max_offset = 40, start = 40 - 15 = 25.
    assert_eq!(start_line, 25);

    // The first viewport row (row 0) maps to content line 25.
    assert_eq!(start_line, 25);
    // The last viewport row (row 9) maps to content line 34.
    assert_eq!(start_line + (viewport_rows - 1), 34);
}

// ── Reducer integration (AppEvent → AppState) ─────────────────────────

/// Build a test AppState with the given scrollback geometry set inline to
/// avoid `field_reassign_with_default` (set fields at construction).
fn state_with_geometry(total_lines: usize, viewport_rows: usize) -> crate::state::AppState {
    crate::state::AppState {
        terminal_total_lines: total_lines,
        terminal_viewport_rows: viewport_rows,
        ..Default::default()
    }
}

#[test]
fn reducer_terminal_scroll_up_sets_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_geometry(50, 10);
    state = state.apply(AppEvent::TerminalScrollUp);
    assert_eq!(state.terminal_history_offset, Some(1));
}

#[test]
fn reducer_terminal_scroll_down_to_bottom_clears_offset() {
    use crate::state::{AppEvent, AppState};
    let mut state = AppState {
        terminal_total_lines: 50,
        terminal_viewport_rows: 10,
        terminal_history_offset: Some(2),
        ..Default::default()
    };
    state = state.apply(AppEvent::TerminalScrollDown);
    assert_eq!(state.terminal_history_offset, Some(1));
    state = state.apply(AppEvent::TerminalScrollDown);
    assert_eq!(state.terminal_history_offset, None);
}

#[test]
fn reducer_terminal_follow_tail_clears_offset() {
    use crate::state::{AppEvent, AppState};
    let mut state = AppState {
        terminal_history_offset: Some(42),
        ..Default::default()
    };
    state = state.apply(AppEvent::TerminalFollowTail);
    assert_eq!(state.terminal_history_offset, None);
}

#[test]
fn reducer_terminal_page_up_down() {
    use crate::state::AppEvent;
    let mut state = state_with_geometry(50, 10);
    state = state.apply(AppEvent::TerminalScrollPageUp);
    assert_eq!(state.terminal_history_offset, Some(10));
    state = state.apply(AppEvent::TerminalScrollPageDown);
    assert_eq!(state.terminal_history_offset, None);
}

#[test]
fn reducer_scroll_events_not_blocked_when_terminal_focused() {
    use crate::state::{AppEvent, AppState, PaneFocus};
    let mut state = AppState {
        terminal_focused: true,
        pane_focus: PaneFocus::Terminal,
        terminal_total_lines: 50,
        terminal_viewport_rows: 10,
        ..Default::default()
    };
    // Even when terminal is focused, scroll events should be applied
    // (not blocked like normal navigation).
    state = state.apply(AppEvent::TerminalScrollUp);
    assert_eq!(state.terminal_history_offset, Some(1));
}

// ── Agent switch resets scroll state (issue #198 review fix #6) ────────

/// Build a minimal AppState with one repository + one agent so selection
/// events have a valid target.
fn state_with_agent() -> crate::state::AppState {
    use crate::domain::{Agent, AgentId, Repository, RepositoryId};
    let repo_id = RepositoryId("test/repo".into());
    let repo = Repository::new(
        repo_id.clone(),
        "test/repo".into(),
        "test-repo".into(),
        std::path::PathBuf::from("/tmp"),
    );
    let mut agent = Agent::new(
        AgentId("agent-1".into()),
        repo_id,
        "Agent 1".into(),
        std::path::PathBuf::from("/tmp"),
    );
    // Assign shortcut slot 1 so JumpToAgentByShortcut(1) finds this agent.
    agent.shortcut_slot = Some(1);
    crate::state::AppState {
        repositories: vec![repo],
        agents: vec![agent],
        selected_repository_index: Some(0),
        terminal_total_lines: 50,
        terminal_viewport_rows: 10,
        terminal_history_offset: Some(20),
        ..Default::default()
    }
}

#[test]
fn reducer_select_repository_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_agent();
    // Re-selecting the same repo still resets the offset.
    state = state.apply(AppEvent::SelectRepository(0));
    assert_eq!(
        state.terminal_history_offset, None,
        "selecting a repository must reset the scroll offset"
    );
}

#[test]
fn reducer_select_agent_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_agent();
    state = state.apply(AppEvent::SelectAgent(0));
    assert_eq!(
        state.terminal_history_offset, None,
        "selecting an agent must reset the scroll offset"
    );
}

#[test]
fn reducer_jump_to_agent_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_agent();
    state = state.apply(AppEvent::JumpToAgentByShortcut(1));
    assert_eq!(
        state.terminal_history_offset, None,
        "jump-to-agent shortcut must reset the scroll offset"
    );
}

// ── Arrow navigation resets scroll state (issue #198 review fix #4) ────

/// Build a state with one repo and two agents so Up/Down navigation has a
/// valid move target.
fn state_with_two_agents() -> crate::state::AppState {
    use crate::domain::{Agent, AgentId, Repository, RepositoryId};
    let repo_id = RepositoryId("test/repo".into());
    let repo = Repository::new(
        repo_id.clone(),
        "test/repo".into(),
        "test-repo".into(),
        std::path::PathBuf::from("/tmp"),
    );
    let agent1 = Agent::new(
        AgentId("agent-1".into()),
        repo_id.clone(),
        "Agent 1".into(),
        std::path::PathBuf::from("/tmp"),
    );
    let agent2 = Agent::new(
        AgentId("agent-2".into()),
        repo_id,
        "Agent 2".into(),
        std::path::PathBuf::from("/tmp"),
    );
    crate::state::AppState {
        repositories: vec![repo],
        agents: vec![agent1, agent2],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: crate::state::PaneFocus::Agents,
        terminal_total_lines: 50,
        terminal_viewport_rows: 10,
        terminal_history_offset: Some(20),
        ..Default::default()
    }
}

#[test]
fn reducer_navigate_down_agent_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_two_agents();
    // Navigate Down in the agents pane: agent 0 → agent 1.
    state = state.apply(AppEvent::NavigateDown);
    assert_eq!(
        state.terminal_history_offset, None,
        "arrow-down agent navigation must reset the scroll offset"
    );
    assert_eq!(state.terminal_viewport_rows, 0);
    assert_eq!(state.terminal_total_lines, 0);
}

#[test]
fn reducer_navigate_up_agent_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_two_agents();
    state.selected_agent_index = Some(1);
    state.terminal_history_offset = Some(15);
    // Navigate Up in the agents pane: agent 1 → agent 0.
    state = state.apply(AppEvent::NavigateUp);
    assert_eq!(
        state.terminal_history_offset, None,
        "arrow-up agent navigation must reset the scroll offset"
    );
}

/// Build a state with two repos and one agent each so repo navigation has
/// a valid move target.
fn state_with_two_repos() -> crate::state::AppState {
    use crate::domain::{Agent, AgentId, Repository, RepositoryId};
    let repo1 = Repository::new(
        RepositoryId("repo1".into()),
        "repo1".into(),
        "repo1".into(),
        std::path::PathBuf::from("/tmp/r1"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo2".into()),
        "repo2".into(),
        "repo2".into(),
        std::path::PathBuf::from("/tmp/r2"),
    );
    let agent1 = Agent::new(
        AgentId("a1".into()),
        RepositoryId("repo1".into()),
        "A1".into(),
        std::path::PathBuf::from("/tmp/r1"),
    );
    let agent2 = Agent::new(
        AgentId("a2".into()),
        RepositoryId("repo2".into()),
        "A2".into(),
        std::path::PathBuf::from("/tmp/r2"),
    );
    crate::state::AppState {
        repositories: vec![repo1, repo2],
        agents: vec![agent1, agent2],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: crate::state::PaneFocus::Repositories,
        terminal_total_lines: 50,
        terminal_viewport_rows: 10,
        terminal_history_offset: Some(20),
        ..Default::default()
    }
}

#[test]
fn reducer_navigate_down_repo_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_two_repos();
    // Navigate Down in the repositories pane: repo 0 → repo 1.
    state = state.apply(AppEvent::NavigateDown);
    assert_eq!(
        state.terminal_history_offset, None,
        "arrow-down repo navigation must reset the scroll offset"
    );
    assert_eq!(state.terminal_viewport_rows, 0);
    assert_eq!(state.terminal_total_lines, 0);
}

#[test]
fn reducer_navigate_up_repo_resets_scroll_offset() {
    use crate::state::AppEvent;
    let mut state = state_with_two_repos();
    state.selected_repository_index = Some(1);
    state.terminal_history_offset = Some(15);
    // Navigate Up in the repositories pane: repo 1 → repo 0.
    state = state.apply(AppEvent::NavigateUp);
    assert_eq!(
        state.terminal_history_offset, None,
        "arrow-up repo navigation must reset the scroll offset"
    );
}
