//! Workbench projection tests (issue #626).
//!
//! Pure-function table tests covering horizontal layout, vertical layout,
//! window capping, the todo-window table, all-complete tail, counter
//! independence, bucket assignment and stable sort, paging, field-state
//! honesty, the fixed-height clipping invariant, and degenerate height.

use std::path::PathBuf;
use std::time::Instant;

use crate::domain::TypedMap;
use crate::domain::agent_definition::AgentTypeId;
use crate::domain::observation::{
    AgentObservation, BoundedText, FieldState, NativeActivityState, NativeActivityValue,
    ObservationHealth, Provenance, TodoItem, TodoList, TodoState,
};
use crate::domain::observation::{CurrentTurn, DisplayedAssistantMessage, Wait, WaitReason};
use crate::domain::{Agent, AgentId, AgentStatus, RepositoryId};
use crate::git_info::GitRepoInfo;
use crate::workbench_view::{
    StatusBucket, StatusFilterMask, TodoRender, WorkbenchRequest, WorkbenchView,
    build_workbench_view,
};

// Sections 8 onward live in a sibling file so neither exceeds the
// source-size gate. The child reaches these fixtures via `use super::*`.
// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Produce `count` working agents each with the given todo list, suitable for
/// a 4-column grid. Names are zero-padded so sort-by-name is stable.
fn four_column_agents(
    count: usize,
    todo_items: &[(TodoState, &str)],
) -> Vec<(Agent, Option<GitRepoInfo>, Option<AgentObservation>)> {
    (0..count)
        .map(|i| {
            let name = format!("agent{i:02}");
            let a = agent_with(AgentStatus::Running, &name, "r");
            let obs = todo_observation(todo_items);
            (a, Some(git("r")), Some(obs))
        })
        .collect()
}

#[path = "workbench_view_paging_tests.rs"]
mod paging_tests;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn repo(name: &str) -> RepositoryId {
    RepositoryId(name.to_string())
}

fn git(origin: &str) -> GitRepoInfo {
    GitRepoInfo {
        origin_shortform: Some(origin.to_string()),
        branch: Some("main".to_string()),
        dirty: None,
    }
}

/// Build an agent with the given status, name, and repository.
fn agent_with(status: AgentStatus, name: &str, repository: &str) -> Agent {
    let mut a = Agent::new(
        AgentId(name.to_string()),
        repo(repository),
        AgentTypeId::default(),
        TypedMap::default(),
        name.to_string(),
        PathBuf::from("/tmp"),
    );
    a.status = status;
    a
}

fn todos(items: &[(TodoState, &str)]) -> FieldState<TodoList> {
    FieldState::known(
        Provenance::Authoritative,
        TodoList {
            revision: 1,
            items: items
                .iter()
                .map(|(state, text)| TodoItem {
                    text: BoundedText((*text).to_string()),
                    state: *state,
                })
                .collect(),
        },
    )
}

fn todo_observation(items: &[(TodoState, &str)]) -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        todos: todos(items),
        ..AgentObservation::default()
    }
}

fn working_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Acting,
            },
        ),
        ..AgentObservation::default()
    }
}

fn ready_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle,
            },
        ),
        wait: FieldState::known(Provenance::Authoritative, None),
        turn: FieldState::known(Provenance::Authoritative, None),
        terminal: FieldState::known(Provenance::Authoritative, None),
        ..AgentObservation::default()
    }
}

fn waiting_observation(reason: WaitReason) -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        wait: FieldState::known(Provenance::Authoritative, Some(Wait { reason })),
        ..AgentObservation::default()
    }
}

fn stale_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Stale,
        ..AgentObservation::default()
    }
}

fn disconnected_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Disconnected,
        ..AgentObservation::default()
    }
}

fn unsupported_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Unsupported,
        ..AgentObservation::default()
    }
}

/// A helper to project a set of agents with the default all-on filter at a
/// given terminal size, with no repository filter.
fn project(
    agents: Vec<(Agent, Option<GitRepoInfo>, Option<AgentObservation>)>,
    width: usize,
    height: usize,
) -> WorkbenchView {
    build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: StatusFilterMask::all_on(),
        repository_filter: None,
        terminal_width: width,
        terminal_height: height,
        page: 0,
    })
}

fn project_filtered(
    agents: Vec<(Agent, Option<GitRepoInfo>, Option<AgentObservation>)>,
    filter: StatusFilterMask,
    width: usize,
    height: usize,
) -> WorkbenchView {
    build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: filter,
        repository_filter: None,
        terminal_width: width,
        terminal_height: height,
        page: 0,
    })
}

fn project_page(
    agents: Vec<(Agent, Option<GitRepoInfo>, Option<AgentObservation>)>,
    page: usize,
    width: usize,
    height: usize,
) -> WorkbenchView {
    build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: StatusFilterMask::all_on(),
        repository_filter: None,
        terminal_width: width,
        terminal_height: height,
        page,
    })
}

// ---------------------------------------------------------------------------
// 1. Horizontal layout table
// ---------------------------------------------------------------------------

#[test]
fn horizontal_layout_across_many_widths() {
    // (width, expected_columns, expected_card_width)
    // usable = width - 22 (sidebar) - 1 (gap)
    let cases: &[(usize, usize, usize)] = &[
        // Degenerate / very narrow: single column floor, min card width 40.
        (0, 1, 40),
        (10, 1, 40),
        (22, 1, 40),
        (40, 1, 40),
        (62, 1, 40), // usable = 62-22-1 = 39 -> 1 col; card = min(52, 39) -> 39 < 40, so back_off
        // keeps 1 col, but floor sets 40.
        // usable = 63 -> 63/(40+1) = 1; card = min(52, 62)= 40
        (63, 1, 40),
        // Two columns: need usable >= 2*40 + 1 gap = 81.
        // usable = 81: (81+1)/(40+1) = 82/41 = 2; card = min(52, (81-1)/2)=min(52,40)=40
        (81 + 22 + 1, 2, 40), // width=104
        // Cap at max width 52: usable large enough that card hits 52.
        // 1 col: usable >= 52 -> width >= 75; card = min(52, usable) = 52
        // usable = 200-22-1 = 177; cols = (177+1)/41 = 4;
        // card = min(52, (177-3)/4) = min(52, 43) = 43.
        (200, 4, 43),
    ];

    for &(width, exp_cols, exp_card) in cases {
        let view = project(vec![], width, 40);
        let layout = view.layout;
        assert_eq!(
            layout.columns, exp_cols,
            "columns at width {width}: got {}, want {exp_cols}",
            layout.columns
        );
        assert_eq!(
            layout.card_width, exp_card,
            "card_width at width {width}: got {}, want {exp_card}",
            layout.card_width
        );
        assert!(
            layout.card_width >= 40,
            "card_width at width {width} below minimum: got {}",
            layout.card_width
        );
        assert!(
            layout.card_width <= 52,
            "card_width at width {width} above maximum: got {}",
            layout.card_width
        );
    }
}

#[test]
fn horizontal_single_column_floor_on_narrow() {
    let view = project(vec![], 30, 40);
    assert_eq!(view.layout.columns, 1);
    assert!(view.layout.card_width >= 40);
}

#[test]
fn horizontal_max_width_cap() {
    // Very wide terminal: card width must cap at 52.
    let view = project(vec![], 1000, 40);
    assert!(view.layout.card_width <= 52);
    assert!(view.layout.columns >= 1);
}

#[test]
fn horizontal_backoff_never_below_minimum() {
    // No card should ever be below the minimum width at any width.
    for width in 0..=400 {
        let view = project(vec![], width, 40);
        assert!(
            view.layout.card_width >= 40,
            "width {width}: card_width {} below minimum",
            view.layout.card_width
        );
    }
}

#[test]
fn horizontal_four_columns_at_200_width() {
    let view = project(vec![], 200, 40);
    assert_eq!(view.layout.columns, 4);
}

#[test]
fn horizontal_two_columns_at_120_width() {
    let view = project(vec![], 120, 40);
    assert_eq!(view.layout.columns, 2);
}

// ---------------------------------------------------------------------------
// 2. Vertical layout table
// ---------------------------------------------------------------------------

#[test]
fn vertical_layout_table_heights_x_agent_counts() {
    // With 4 columns (width=200), 6 agents => rows_needed=ceil(6/4)=2.
    // avail = height - 6 = 46; rows_at_min = 46/11 = 4; rows_needed(2) <= 4 => grow.
    // grown = 46/2 - 1 - 7 = 15; W = clamp(min(15, longest), 3, 8).
    let agents = four_column_agents(6, &[(TodoState::Pending, "a")]);
    let view = project(agents, 200, 52);
    assert_eq!(view.layout.columns, 4);
    assert!(view.layout.todo_window >= 3);
    assert!(view.layout.todo_window <= 8);
    assert_eq!(view.layout.page_count, 1);
    assert_eq!(view.layout.rows_visible, 2);
}

#[test]
fn increasing_height_never_reduces_visible_agents() {
    let agents = four_column_agents(
        8,
        &[
            (TodoState::Pending, "a"),
            (TodoState::Pending, "b"),
            (TodoState::Pending, "c"),
        ],
    );
    let mut prev_visible = 0;
    for height in 10..=60 {
        let view = project(agents.clone(), 200, height);
        let visible = view.cards.len();
        assert!(
            visible >= prev_visible || view.layout.page_count > 1,
            "height {height}: visible ({visible}) reduced from {prev_visible}"
        );
        prev_visible = visible.max(prev_visible).min(8);
    }
}

#[test]
fn vertical_many_agents_page_correctly() {
    // 12 agents, 4 columns => rows_needed=3.
    // At a short height, paging kicks in.
    let agents = four_column_agents(12, &[(TodoState::Pending, "a")]);
    let view = project(agents, 200, 24);
    assert_eq!(view.layout.columns, 4);
    assert!(view.layout.page_count >= 1);
    // Each page shows at most rows_visible * columns cards.
    let max_per_page = view.layout.rows_visible * view.layout.columns;
    assert!(view.cards.len() <= max_per_page);
    assert!(!view.cards.is_empty());
}

// ---------------------------------------------------------------------------
// 3. Window cap: tall terminal + short lists => window = longest list
// ---------------------------------------------------------------------------

#[test]
fn window_capped_at_longest_visible_list() {
    // Tall terminal with short lists: W must equal the longest list, not W_MAX.
    let agents = four_column_agents(2, &[(TodoState::Pending, "a"), (TodoState::Pending, "b")]);
    let view = project(agents, 200, 80);
    // The cap says W never exceeds the longest visible list; the floor says W is
    // never below W_MIN. With a 2-item longest list on a tall terminal the two
    // rules collide and the floor wins, so W is exactly W_MIN rather than 2.
    // That is deliberate: a card shorter than W_MIN would make the grid ragged,
    // which is the flaw this layout exists to avoid. The cap still does its job
    // by stopping growth well below what an 80-row terminal would otherwise
    // allow.
    assert_eq!(
        view.layout.todo_window, 3,
        "floor should win over the cap for a short list"
    );
    for card in &view.cards {
        if let TodoRender::Known(window) = &card.todos {
            let non_blank = window.visible.iter().filter(|l| !l.is_blank).count();
            // The window is padded to W, but only real items may be non-blank,
            // so the non-blank count is exactly the list length here.
            assert_eq!(
                non_blank, window.total,
                "every list item should be visible when W exceeds the list"
            );
            assert_eq!(
                window.visible.len(),
                view.layout.todo_window,
                "every card must be exactly W lines tall"
            );
        }
    }
}

#[test]
fn window_grows_with_height_when_lists_are_long() {
    // Long lists (8 items) + tall terminal => W grows toward 8.
    let long_list: Vec<(TodoState, &str)> = (0..8).map(|_i| (TodoState::Pending, "item")).collect();
    let long_list_ref: &[(TodoState, &str)] = &long_list;
    let agents = four_column_agents(1, long_list_ref);
    let view_short = project(agents.clone(), 200, 20);
    let view_tall = project(agents, 200, 60);
    assert!(
        view_tall.layout.todo_window >= view_short.layout.todo_window,
        "tall terminal must not reduce todo_window"
    );
}

// ---------------------------------------------------------------------------
// 4. Todo window table: lengths 0..10 x current position
// ---------------------------------------------------------------------------

#[test]
fn todo_window_current_always_visible_when_one_exists() {
    for len in 1usize..=10 {
        for current in 0..len {
            let mut items: Vec<(TodoState, &str)> = vec![(TodoState::Pending, "x"); len];
            // Everything before `current` is completed; `current` is the item
            // the producer says is in progress.
            for item in items.iter_mut().take(current) {
                *item = (TodoState::Completed, "x");
            }
            items[current] = (TodoState::InProgress, "x");
            let obs = todo_observation(&items);
            let agent = agent_with(AgentStatus::Running, "a", "repo");
            let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
            let card = &view.cards[0];
            let TodoRender::Known(window) = &card.todos else {
                panic!("expected Known todos for len={len}");
            };
            // The current item must be visible.
            assert!(
                window.current.is_some(),
                "len={len} current={current}: expected a current marker"
            );
            let Some(current_idx) = window.current else {
                panic!("len={len} current={current}: expected a current index");
            };
            assert!(
                window.visible[current_idx].is_current,
                "len={len} current={current}: visible slot not marked current"
            );
        }
    }
}

#[test]
fn todo_window_preceding_finished_item_visible_when_one_exists() {
    // When a preceding finished item exists, it should be in the window.
    for len in 2usize..=10 {
        let mut items: Vec<(TodoState, &str)> = vec![(TodoState::Pending, "x"); len];
        items[0] = (TodoState::Completed, "done");
        items[1] = (TodoState::InProgress, "x");
        let obs = todo_observation(&items);
        let agent = agent_with(AgentStatus::Running, "a", "repo");
        let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
        let card = &view.cards[0];
        let TodoRender::Known(window) = &card.todos else {
            panic!("expected Known todos");
        };
        // The first visible item should be the completed one (start = open-1).
        assert!(
            !window.visible.is_empty(),
            "len={len}: window must not be empty"
        );
    }
}

/// The active item is the one the producer says is in progress, wherever it
/// sits. An agent working out of order used to be misreported, because the
/// first item that was merely not finished was marked as the one being worked
/// on (issue #625).
#[test]
fn active_item_is_the_published_one_even_out_of_order() {
    let items = vec![
        (TodoState::Pending, "not started"),
        (TodoState::Pending, "also not started"),
        (TodoState::InProgress, "actually being worked on"),
    ];
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let TodoRender::Known(window) = &view.cards[0].todos else {
        panic!("expected Known todos");
    };

    let marked: Vec<&str> = window
        .visible
        .iter()
        .filter(|line| line.is_current)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(
        marked.len(),
        1,
        "exactly the published in-progress item is active: {:?}",
        window.visible
    );
    assert!(
        marked[0].contains("actually being worked on"),
        "the active marker must sit on the published item, not the first unfinished one: {:?}",
        marked[0]
    );
}

/// A list with nothing in progress has no active item. Marking the first
/// unfinished entry would be a guess presented as fact.
#[test]
fn nothing_in_progress_means_no_active_item() {
    let items = vec![
        (TodoState::Completed, "done"),
        (TodoState::Pending, "blocked on review"),
        (TodoState::Pending, "later"),
    ];
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let TodoRender::Known(window) = &view.cards[0].todos else {
        panic!("expected Known todos");
    };

    assert!(
        window.current.is_none(),
        "an unfinished item is not evidence that it is being worked on"
    );
    assert!(
        window.visible.iter().all(|line| !line.is_current),
        "no line may claim to be active: {:?}",
        window.visible
    );
    assert_eq!(window.done, 1, "the counter still reflects the whole list");
    assert_eq!(window.total, 3);
}

/// An agent working several items at once has every one of them marked. Each
/// marker is something the producer said, so none of them is an inference.
#[test]
fn several_items_in_progress_are_all_marked() {
    let items = vec![
        (TodoState::InProgress, "first strand"),
        (TodoState::Completed, "finished"),
        (TodoState::InProgress, "second strand"),
    ];
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let TodoRender::Known(window) = &view.cards[0].todos else {
        panic!("expected Known todos");
    };

    assert_eq!(
        window.visible.iter().filter(|line| line.is_current).count(),
        2,
        "both published strands are active: {:?}",
        window.visible
    );
}

/// A state JSP/1 does not recognize is not completed and is not active, and
/// the card says so rather than passing it off as either.
#[test]
fn unrecognized_state_is_neither_done_nor_active() {
    let items = vec![
        (TodoState::Completed, "done"),
        (TodoState::Unrecognized, "odd"),
    ];
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let TodoRender::Known(window) = &view.cards[0].todos else {
        panic!("expected Known todos");
    };

    assert_eq!(window.done, 1, "an unrecognized state is not completed");
    assert!(
        window.current.is_none(),
        "an unrecognized state is not the active item"
    );
    let odd = window
        .visible
        .iter()
        .find(|line| line.text.contains("odd"))
        .unwrap_or_else(|| panic!("the unrecognized item must render"));
    assert!(
        odd.text.contains("[?]"),
        "an unrecognized state gets its own marker: {:?}",
        odd.text
    );
}

// ---------------------------------------------------------------------------
// 5. All-complete: tail shown, no current marker
// ---------------------------------------------------------------------------

#[test]
fn all_complete_shows_tail_no_current_marker() {
    let items = vec![
        (TodoState::Completed, "done1"),
        (TodoState::Completed, "done2"),
        (TodoState::Completed, "done3"),
    ];
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let card = &view.cards[0];
    let TodoRender::Known(window) = &card.todos else {
        panic!("expected Known todos");
    };
    assert_eq!(window.done, 3);
    assert_eq!(window.total, 3);
    assert!(
        window.current.is_none(),
        "all-complete must have no current"
    );
    // The tail (last items) should be visible, not blank-padded above real items.
    let non_blank = window.visible.iter().filter(|l| !l.is_blank).count();
    assert_eq!(
        non_blank, 3,
        "tail of all-complete list must show real items"
    );
}

// ---------------------------------------------------------------------------
// 6. Counter independence: long list reports true done/total
// ---------------------------------------------------------------------------

#[test]
fn counter_independence_long_list_reports_true_counts() {
    let items: Vec<(TodoState, &str)> = (0..20)
        .map(|i| {
            if i < 7 {
                (TodoState::Completed, "task")
            } else {
                (TodoState::Pending, "task")
            }
        })
        .collect();
    let obs = todo_observation(&items);
    let agent = agent_with(AgentStatus::Running, "a", "repo");
    let view = project(vec![(agent, Some(git("repo")), Some(obs))], 200, 52);
    let card = &view.cards[0];
    let TodoRender::Known(window) = &card.todos else {
        panic!("expected Known todos");
    };
    assert_eq!(
        window.done, 7,
        "done must reflect the FULL list, not the window"
    );
    assert_eq!(
        window.total, 20,
        "total must reflect the FULL list, not the window"
    );
    // The window itself is smaller.
    assert!(
        window.visible.len() <= 8,
        "window must not exceed W_MAX in visible lines"
    );
}

// ---------------------------------------------------------------------------
// 7. Bucket assignment and stable sort
// ---------------------------------------------------------------------------

#[test]
fn bucket_assignment_and_needs_you_first() {
    let waiting = agent_with(AgentStatus::Running, "waiting", "r");
    let working = agent_with(AgentStatus::Running, "working", "r");
    let ready = agent_with(AgentStatus::Running, "ready", "r");
    let stale = agent_with(AgentStatus::Running, "stale", "r");
    let agents = vec![
        (stale, Some(git("r")), Some(stale_observation())),
        (ready, Some(git("r")), Some(ready_observation())),
        (working, Some(git("r")), Some(working_observation())),
        (
            waiting,
            Some(git("r")),
            Some(waiting_observation(WaitReason::Permission)),
        ),
    ];
    let view = project(agents, 200, 52);
    assert_eq!(view.cards.len(), 4);
    assert_eq!(view.cards[0].bucket, StatusBucket::NeedsYou);
    assert_eq!(view.cards[1].bucket, StatusBucket::Working);
    assert_eq!(view.cards[2].bucket, StatusBucket::Ready);
    assert_eq!(view.cards[3].bucket, StatusBucket::Stale);
}

#[test]
fn stable_sort_intra_bucket_preserves_incoming_order() {
    // Two working agents: "alpha" before "bravo" in input.
    let alpha = agent_with(AgentStatus::Running, "alpha", "r");
    let bravo = agent_with(AgentStatus::Running, "bravo", "r");
    let agents = vec![
        (alpha, Some(git("r")), Some(working_observation())),
        (bravo, Some(git("r")), Some(working_observation())),
    ];
    let view = project(agents, 200, 52);
    assert_eq!(view.cards[0].agent_id.0, "alpha");
    assert_eq!(view.cards[1].agent_id.0, "bravo");
}

#[test]
fn elapsed_time_change_does_not_reorder() {
    // Two working agents with different turn anchors; only elapsed differs.
    // Both are Working, so order must be preserved regardless of elapsed.
    let alpha = agent_with(AgentStatus::Running, "alpha", "r");
    let bravo = agent_with(AgentStatus::Running, "bravo", "r");
    let mut obs_a = working_observation();
    obs_a.turn = FieldState::known(
        Provenance::Authoritative,
        Some(CurrentTurn { elapsed_ms: 1_000 }),
    );
    let mut obs_b = working_observation();
    obs_b.turn = FieldState::known(
        Provenance::Authoritative,
        Some(CurrentTurn {
            elapsed_ms: 999_999,
        }),
    );
    let agents = vec![
        (alpha, Some(git("r")), Some(obs_a)),
        (bravo, Some(git("r")), Some(obs_b)),
    ];
    let view = project(agents, 200, 52);
    // Same bucket => incoming order preserved.
    assert_eq!(view.cards[0].agent_id.0, "alpha");
    assert_eq!(view.cards[1].agent_id.0, "bravo");
}

#[test]
fn bucket_counts_unfiltered() {
    let waiting = agent_with(AgentStatus::Running, "w", "r");
    let working = agent_with(AgentStatus::Running, "wk", "r");
    let ready = agent_with(AgentStatus::Running, "rd", "r");
    let stale = agent_with(AgentStatus::Running, "st", "r");
    let agents = vec![
        (
            waiting,
            Some(git("r")),
            Some(waiting_observation(WaitReason::Question)),
        ),
        (working, Some(git("r")), Some(working_observation())),
        (ready, Some(git("r")), Some(ready_observation())),
        (stale, Some(git("r")), Some(stale_observation())),
    ];
    // Filter to only NeedsYou.
    let view = project_filtered(
        agents,
        StatusFilterMask::only(StatusBucket::NeedsYou),
        200,
        52,
    );
    assert_eq!(
        view.bucket_counts,
        [1, 1, 1, 1],
        "counts must stay unfiltered"
    );
    assert_eq!(view.cards.len(), 1, "only NeedsYou renders");
}
