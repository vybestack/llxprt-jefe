//! Workbench projection tests, part two (issue #626).
//!
//! Paging arithmetic and clamping, field-state honesty, the fixed-height
//! clipping invariant, degenerate height, and the remaining projection
//! behaviours. Fixtures are shared with the parent test module.

use super::*;

// ---------------------------------------------------------------------------
// 8. Paging arithmetic, clamping, and page-index clamping on filter narrowing
// ---------------------------------------------------------------------------

#[test]
fn paging_shows_only_current_page() {
    let agents = four_column_agents(12, &[(TodoState::Pending, "x")]);
    let view = project_page(agents, 0, 200, 24);
    assert_eq!(view.layout.page, 0);
    assert!(view.layout.page_count >= 2, "12 agents should page");
    let page0_count = view.cards.len();
    assert!(page0_count < 12, "page 0 must not show all 12");
}

#[test]
fn paging_clamps_at_last_page() {
    let agents = four_column_agents(12, &[(TodoState::Pending, "x")]);
    // Request page 999 — must clamp to the last valid page.
    let view = project_page(agents, 999, 200, 24);
    // Landing on *a* valid page is not enough: clamping to page 0 would also
    // satisfy that and would silently throw the user back to the start.
    assert_eq!(
        view.layout.page,
        view.layout.page_count - 1,
        "an over-large page must clamp to the last page"
    );
    assert!(!view.cards.is_empty(), "clamped page must not be empty");
}

/// Page 0 is already the minimum, so this pins that the first page renders
/// rather than that any clamping happens. The over-large case above covers
/// clamping itself; a below-range case cannot exist because the page index is
/// unsigned.
#[test]
fn paging_first_page_renders() {
    let agents = four_column_agents(12, &[(TodoState::Pending, "x")]);
    let view = project_page(agents, 0, 200, 24);
    assert_eq!(view.layout.page, 0);
    assert!(!view.cards.is_empty());
}

#[test]
fn page_index_clamping_when_filter_narrows() {
    // 12 agents; go to a later page, then narrow the filter so fewer agents
    // remain. The page must clamp rather than render empty.
    let mut agents = vec![];
    for i in 0..6 {
        let a = agent_with(AgentStatus::Running, &format!("w{i}"), "r");
        agents.push((
            a,
            Some(git("r")),
            Some(waiting_observation(WaitReason::Permission)),
        ));
    }
    for i in 0..6 {
        let a = agent_with(AgentStatus::Running, &format!("k{i}"), "r");
        agents.push((a, Some(git("r")), Some(working_observation())));
    }
    // On page 1 with all-on, then filter to only Working (6 agents).
    let view = build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: StatusFilterMask::only(StatusBucket::Working),
        repository_filter: None,
        terminal_width: 200,
        terminal_height: 24,
        page: 5, // request a page beyond what 6 agents produce
    });
    assert!(
        view.layout.page < view.layout.page_count,
        "narrowed filter must clamp page to a valid range"
    );
    assert!(
        !view.cards.is_empty(),
        "must still render cards after clamping"
    );
}

// ---------------------------------------------------------------------------
// 9. Field-state honesty: unknown / unsupported / empty distinguishable
// ---------------------------------------------------------------------------

#[test]
fn field_state_unknown_unsupported_empty_are_distinguishable() {
    let agent = agent_with(AgentStatus::Running, "a", "r");

    // Unknown: supported field, no value.
    let unknown_obs = AgentObservation {
        health: ObservationHealth::Live,
        todos: FieldState::unknown(Provenance::Authoritative),
        ..AgentObservation::default()
    };
    // Unsupported: field not supported by producer.
    let unsupported_obs = AgentObservation {
        health: ObservationHealth::Live,
        todos: FieldState::Unsupported,
        ..AgentObservation::default()
    };
    // Empty: known list with zero items.
    let empty_obs = AgentObservation {
        health: ObservationHealth::Live,
        todos: todos(&[]),
        ..AgentObservation::default()
    };

    let view = project(
        vec![
            (agent.clone(), Some(git("r")), Some(unsupported_obs)),
            (agent.clone(), Some(git("r")), Some(unknown_obs.clone())),
            (agent.clone(), Some(git("r")), Some(empty_obs)),
        ],
        200,
        52,
    );
    let renders: Vec<&TodoRender> = view.cards.iter().map(|c| &c.todos).collect();
    assert_eq!(renders.len(), 3);
    // They must be three distinct variants.
    assert!(renders.iter().any(|r| matches!(r, TodoRender::Unsupported)));
    assert!(renders.iter().any(|r| matches!(r, TodoRender::Unknown)));
    assert!(
        renders
            .iter()
            .any(|r| { matches!(r, TodoRender::Known(w) if w.total == 0) })
    );
    // Sanity: the unknown variant is not the same as empty-known.
    assert_ne!(renders[0], renders[1]);
    assert_ne!(renders[1], renders[2]);
    assert_ne!(renders[0], renders[2]);

    // Also: no observation at all => Unsupported (telemetry unsupported, X1).
    let view_no_obs = project(vec![(agent, Some(git("r")), None)], 200, 52);
    assert!(matches!(
        view_no_obs.cards[0].todos,
        TodoRender::Unsupported
    ));
}

#[test]
fn unsupported_todos_not_rendered_as_empty() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let obs = unsupported_observation();
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    assert!(
        !matches!(&view.cards[0].todos, TodoRender::Known(w) if w.total == 0),
        "unsupported must not render as an empty Known list"
    );
}

// ---------------------------------------------------------------------------
// 10. Fixed-height invariant and clipping
// ---------------------------------------------------------------------------

#[test]
fn fixed_height_invariant_every_card_same_line_count() {
    // Vary name lengths, list lengths, and completion patterns.
    let names = [
        "a",
        "medium-name",
        "a-very-long-agent-name-that-exceeds-budget",
    ];
    let list_patterns: &[&[(TodoState, &str)]] = &[
        &[],
        &[(TodoState::Pending, "x")],
        &[
            (TodoState::Completed, "done"),
            (TodoState::InProgress, "active"),
            (TodoState::Pending, "next"),
        ],
        &[
            (TodoState::Completed, "d"),
            (TodoState::Completed, "d"),
            (TodoState::Completed, "d"),
            (TodoState::Completed, "d"),
        ],
    ];
    for &width in &[80, 120, 200] {
        for &height in &[20, 40, 60] {
            let mut agents = vec![];
            for name in names {
                for pattern in list_patterns {
                    let a = agent_with(AgentStatus::Running, name, "repo");
                    let obs = todo_observation(pattern);
                    agents.push((a, Some(git("repo")), Some(obs)));
                }
            }
            let view = project(agents, width, height);
            // Every Known todo window has exactly todo_window visible lines.
            let w = view.layout.todo_window;
            for card in &view.cards {
                if let TodoRender::Known(window) = &card.todos {
                    assert_eq!(
                        window.visible.len(),
                        w,
                        "card {} todos must have exactly {w} visible lines",
                        card.agent_id.0
                    );
                }
            }
        }
    }
}

#[test]
fn clipping_no_line_exceeds_card_width() {
    let long_name = "x".repeat(200);
    let long_todo = "y".repeat(200);
    let long_msg = DisplayedAssistantMessage {
        content: BoundedText("z".repeat(200)),
        committed_ms: 0,
    };
    let mut obs = todo_observation(&[(TodoState::InProgress, &long_todo)]);
    obs.last_message = FieldState::known(Provenance::Authoritative, long_msg);
    let agent = agent_with(AgentStatus::Running, &long_name, "repo");
    for &width in &[80, 120, 200] {
        let view = project(
            vec![(agent.clone(), Some(git("repo")), Some(obs.clone()))],
            width,
            40,
        );
        let card_width = view.layout.card_width;
        for card in &view.cards {
            assert_line_within(&card.header.status_label, card_width, "status_label");
            assert_line_within(&card.header.repo_name.text, card_width, "repo_name");
            assert_line_within(&card.need, card_width, "need");
            if let Some(msg) = &card.last_message {
                assert_line_within(msg, card_width, "last_message");
            }
            if let TodoRender::Known(window) = &card.todos {
                for line in &window.visible {
                    assert_line_within(&line.text, card_width, "todo line");
                }
            }
        }
    }
}

fn assert_line_within(text: &str, width: usize, label: &str) {
    let display_width = unicode_width::UnicodeWidthStr::width(text);
    assert!(
        display_width <= width,
        "{label} display width {display_width} exceeds card width {width}: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// 11. Degenerate height: too short for one card still yields one row
// ---------------------------------------------------------------------------

#[test]
fn degenerate_height_yields_one_row_not_malformed() {
    let agents = four_column_agents(4, &[(TodoState::Pending, "x")]);
    // Height too small for even one full card.
    let view = project(agents, 200, 8);
    assert!(
        view.layout.rows_visible >= 1,
        "degenerate height must still yield at least one row"
    );
    assert!(!view.cards.is_empty(), "must render at least one card");
    // No card line may exceed the card width.
    let card_width = view.layout.card_width;
    for card in &view.cards {
        if let TodoRender::Known(window) = &card.todos {
            assert_eq!(
                window.visible.len(),
                view.layout.todo_window,
                "degenerate card must still have the correct window size"
            );
            for line in &window.visible {
                assert_line_within(&line.text, card_width, "degenerate todo line");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Extra: empty states, shortcut slot, need line surfacing
// ---------------------------------------------------------------------------

#[test]
fn all_filters_off_yields_explicit_empty_state() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project_filtered(
        vec![(agent, Some(git("r")), Some(working_observation()))],
        StatusFilterMask::default(),
        200,
        40,
    );
    assert!(view.cards.is_empty());
    assert!(
        view.empty_reason
            .as_ref()
            .is_some_and(|r| r.contains("filters")),
        "all-off must yield an explicit empty reason, got {:?}",
        view.empty_reason
    );
}

#[test]
fn no_agents_yields_empty_state() {
    let view = project(vec![], 200, 40);
    assert!(view.cards.is_empty());
    assert!(view.empty_reason.is_some());
}

#[test]
fn need_line_surfaces_wait_reason() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let obs = waiting_observation(WaitReason::Permission);
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    assert!(
        view.cards[0].need.contains("permission"),
        "need line must surface the wait reason, got {:?}",
        view.cards[0].need
    );
}

#[test]
fn shortcut_slot_present_only_when_assigned() {
    let mut agent = agent_with(AgentStatus::Running, "a", "r");
    agent.shortcut_slot = Some(3);
    let view = project(
        vec![(agent, Some(git("r")), Some(working_observation()))],
        200,
        52,
    );
    assert_eq!(view.cards[0].header.shortcut_slot.as_deref(), Some("3"));
}

#[test]
fn shortcut_slot_absent_when_unassigned() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(
        vec![(agent, Some(git("r")), Some(working_observation()))],
        200,
        52,
    );
    assert!(view.cards[0].header.shortcut_slot.is_none());
}

#[test]
fn repository_filter_isolates_agents() {
    let a = agent_with(AgentStatus::Running, "a", "repo-a");
    let b = agent_with(AgentStatus::Running, "b", "repo-b");
    let agents = vec![
        (a, Some(git("repo-a")), Some(working_observation())),
        (b, Some(git("repo-b")), Some(working_observation())),
    ];
    let view = build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: StatusFilterMask::all_on(),
        repository_filter: Some("repo-a".to_string()),
        terminal_width: 200,
        terminal_height: 52,
        page: 0,
    });
    assert_eq!(view.cards.len(), 1);
    assert_eq!(view.cards[0].agent_id.0, "a");
    // Counts stay unfiltered.
    assert_eq!(view.bucket_counts[StatusBucket::Working.as_index()], 2);
}

#[test]
fn stale_never_folds_into_ready() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(
        vec![(agent, Some(git("r")), Some(stale_observation()))],
        200,
        52,
    );
    assert_eq!(view.cards[0].bucket, StatusBucket::Stale);
    assert_ne!(view.cards[0].bucket, StatusBucket::Ready);
}

#[test]
fn disconnected_lands_in_stale_bucket() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(
        vec![(agent, Some(git("r")), Some(disconnected_observation()))],
        200,
        52,
    );
    assert_eq!(view.cards[0].bucket, StatusBucket::Stale);
}

#[test]
fn no_observation_lands_in_stale_bucket() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(vec![(agent, Some(git("r")), None)], 200, 52);
    assert_eq!(view.cards[0].bucket, StatusBucket::Stale);
}

#[test]
fn todo_window_active_item_centers_correctly() {
    // List of 8; the producer says index 5 is the one being worked on.
    let items: Vec<(TodoState, &str)> = (0..8)
        .map(|i| match i {
            0..=4 => (TodoState::Completed, "x"),
            5 => (TodoState::InProgress, "x"),
            _ => (TodoState::Pending, "x"),
        })
        .collect();
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "r");
    // A 20-row terminal is tall enough for a 6-line window here; the exact
    // value is asserted below rather than assumed.
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 20);
    let card = &view.cards[0];
    let TodoRender::Known(window) = &card.todos else {
        panic!("expected Known");
    };
    let w = view.layout.todo_window;
    // The current item (global index 5) must be visible.
    assert!(window.current.is_some());
    let Some(current_visible) = window.current else {
        panic!("expected a current index");
    };
    assert!(window.visible[current_visible].is_current);
    // A 20-row terminal yields W=6 for this list, not W_MIN. Pinning the real
    // value keeps the arithmetic below honest: with 8 items and W=6 the window
    // start is min(current-1, total-W) = min(4, 2) = 2, so the current item
    // (global index 5) lands at visible index 3.
    assert_eq!(w, 6, "a 20-row terminal should yield a 6-line window");
    assert_eq!(window.visible.len(), w);
    assert_eq!(
        current_visible, 3,
        "current item should sit at visible index 3 for start=2"
    );
    // Anchoring backs up for context, so the current item is never the first
    // line while earlier items exist.
    assert!(
        current_visible >= 1,
        "a preceding item must stay visible for context"
    );
    // Exactly one line may claim to be current.
    assert_eq!(
        window.visible.iter().filter(|l| l.is_current).count(),
        1,
        "exactly one visible line may be marked current"
    );
    // The counter is computed from the whole list, not the window.
    assert_eq!(window.total, 8);
    assert_eq!(window.done, 5);
}

#[test]
fn last_message_rendered_when_present() {
    let mut obs = working_observation();
    obs.last_message = FieldState::known(
        Provenance::Authoritative,
        DisplayedAssistantMessage {
            content: BoundedText("Pushed the commit.".to_string()),
            committed_ms: 0,
        },
    );
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    assert_eq!(
        view.cards[0].last_message.as_deref(),
        Some("Pushed the commit.")
    );
}

#[test]
fn last_message_absent_when_unsupported() {
    let obs = working_observation(); // last_message defaults to Unsupported
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    assert!(view.cards[0].last_message.is_none());
}

#[test]
fn turn_elapsed_label_rendered_for_active_turn() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let mut obs = working_observation();
    obs.turn = FieldState::known(
        Provenance::Authoritative,
        Some(CurrentTurn {
            elapsed_ms: 252_000,
        }),
    );
    // Anchor the observation now, so the locally-measured component is ~0 and
    // the label is dominated by the producer's 252s anchor. Asserting a prefix
    // rather than the whole string keeps this from being flaky on the seconds
    // component while still pinning that the anchor is actually used: if the
    // producer value were ignored the label would read "0s" or "—".
    obs.turn_observed_at = Some(Instant::now());
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    let elapsed = &view.cards[0].header.elapsed;
    assert!(
        elapsed.starts_with("4m"),
        "elapsed must be driven by the 252s turn anchor, got {elapsed}"
    );
}

#[test]
fn elapsed_dash_when_no_turn() {
    let agent = agent_with(AgentStatus::Running, "a", "r");
    let obs = ready_observation(); // no active turn
    let view = project(vec![(agent, Some(git("r")), Some(obs))], 200, 52);
    assert_eq!(view.cards[0].header.elapsed, "—");
}

#[test]
fn stable_sort_across_pages() {
    // 12 agents, all working, on page 0 and page 1. Order must be stable.
    let agents = four_column_agents(12, &[(TodoState::Pending, "x")]);
    let view0 = project_page(agents.clone(), 0, 200, 24);
    let view1 = project_page(agents, 1, 200, 24);
    let names0: Vec<&str> = view0.cards.iter().map(|c| c.agent_id.0.as_str()).collect();
    let names1: Vec<&str> = view1.cards.iter().map(|c| c.agent_id.0.as_str()).collect();
    // No overlap between pages.
    for n in &names0 {
        assert!(!names1.contains(n), "agent {n} appeared on both pages");
    }
}
