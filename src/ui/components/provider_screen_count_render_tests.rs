//! Rendered-geometry contracts for dashboard counts (issue #745 follow-up).
//!
//! #752 restored the `(N)` form and proved it at the projection layer, where
//! `ListItem::label` carries the count whatever the pane width is. That layer
//! cannot fail on truncation, so it could not catch what dogfood found: at the
//! shipped 22-column rail the count is the part of the row that falls off.
//!
//! These tests render the real `ProviderScreen` to an iocraft canvas and read
//! the painted cells, so the count has to survive the resolver, the shared
//! list control's width budget and the final paint.

use crate::domain::observation::AgentObservation;
use crate::domain::{AgentStatus, AgentTypeId, Repository, RepositoryId, TypedMap};
use crate::state::AppState;
use crate::test_support::{host_panel_agent, ready_observation, waiting_observation};
use iocraft::prelude::*;
use std::path::PathBuf;

/// The operator's terminal when #745 was reopened (`stty -a`: 83 rows, 313
/// columns). The rail is fixed-width, so these assertions hold at any size;
/// this one is used because it is the size the report came from.
const DOGFOOD_COLUMNS: u16 = 313;
/// Enough rows for the split to resolve every pane; the report's 83 paint the
/// same rail with more blank sidebar rows.
const DOGFOOD_ROWS: u16 = 30;
/// Cells of the painted frame the fixed left rail occupies, borders included
/// (`SIDEBAR_COLUMNS`), so a failure message shows the rail and nothing of the
/// card grid beside it.
const RAIL_CELLS: usize = 22;

fn repositories_state() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.nav = crate::state::navigation::NavState::rooted_definition(
        crate::workbench::REPOSITORIES_IDENTITY,
        crate::workbench::RouteId::from_static("repositories"),
        crate::workbench::PanelId::from_static("repositories"),
    );
    state
}

fn repository(id: &str, name: &str) -> Repository {
    Repository::new(
        RepositoryId(format!("repo-{id}")),
        AgentTypeId::default(),
        TypedMap::default(),
        name.to_owned(),
        format!("repo-{id}"),
        PathBuf::from("/tmp"),
    )
}

fn seed(state: &mut AppState, name: &str, repository_id: &str, observed: AgentObservation) {
    let agent = host_panel_agent(name, repository_id, AgentStatus::Running);
    state.observations.insert(agent.id.clone(), observed);
    state.agents.push(agent);
}

/// Paint the split view and return the canvas as plain text, one string per
/// terminal row.
fn painted_rows(state: &AppState) -> Vec<String> {
    let mut state = state.clone();
    state.resolved_layout =
        crate::screen_layout::resolve_screen(&state, DOGFOOD_COLUMNS, DOGFOOD_ROWS);
    assert!(
        state.resolved_layout.is_some(),
        "the split view must resolve at {DOGFOOD_COLUMNS}x{DOGFOOD_ROWS}"
    );
    let mut element = element! {
        Box(width: u32::from(DOGFOOD_COLUMNS), height: u32::from(DOGFOOD_ROWS)) {
            super::ProviderScreen(
                state: Some(state.clone()),
                colors: crate::theme::ThemeColors::default(),
                theme_name: "default".to_owned(),
            )
        }
    };
    element
        .render(Some(usize::from(DOGFOOD_COLUMNS)))
        .to_string()
        .lines()
        .map(|line| line.chars().take(RAIL_CELLS).collect())
        .collect()
}

/// A bordered pane of the rail. The sidebar sits above the STATUS block and
/// paints operator-chosen repository names, so a bucket word like `Ready` can
/// legitimately appear in both panes; every assertion names the one it means.
#[derive(Clone, Copy, Debug)]
enum RailPane {
    Repositories,
    Status,
}

impl RailPane {
    /// Text carried by the pane's top border row, which holds its title.
    const fn title(self) -> &'static str {
        match self {
            // Not the screen's own header row (`Repositories - 0.0.32`): the
            // focus caret is painted into the pane border only.
            Self::Repositories => "▶ Repositories",
            Self::Status => "STATUS",
        }
    }

    /// The character the pane's bottom border row begins with.
    const fn closing(self) -> char {
        match self {
            Self::Repositories => '╚',
            Self::Status => '╰',
        }
    }
}

/// The painted rows strictly inside `pane`'s borders.
fn pane_rows(rows: &[String], pane: RailPane) -> &[String] {
    let top = rows
        .iter()
        .position(|row| row.contains(pane.title()))
        .unwrap_or_else(|| {
            panic!(
                "the rail paints no {pane:?} pane; rail:\n{}",
                rows.join("\n")
            )
        });
    let interior = &rows[top.saturating_add(1)..];
    let height = interior
        .iter()
        .position(|row| row.starts_with(pane.closing()))
        .unwrap_or_else(|| {
            panic!(
                "the {pane:?} pane has no closing border; rail:\n{}",
                rows.join("\n")
            )
        });
    &interior[..height]
}

/// The one row of `pane` that carries `needle`; panics unless exactly one does.
///
/// `needle` is a leading fragment of the row's label rather than the whole of
/// it, because the label is exactly the span a narrow pane is allowed to
/// elide. Matching on the full label would make a passing count assertion
/// impossible to distinguish from a missing row. A fragment is not unique
/// across the rail — a repository named `Ready to ship` carries the bucket
/// word `Ready` — so the search is scoped to one pane, and a second match
/// inside that pane fails the test instead of silently binding to the first.
fn painted_row(rows: &[String], pane: RailPane, needle: &str) -> String {
    let matched: Vec<&String> = pane_rows(rows, pane)
        .iter()
        .filter(|row| row.contains(needle))
        .collect();
    match matched.as_slice() {
        [row] => (*row).clone(),
        _ => panic!(
            "exactly one {pane:?} row must carry {needle:?}, matched {}; rail:\n{}",
            matched.len(),
            rows.join("\n")
        ),
    }
}

fn assert_count_painted(rows: &[String], pane: RailPane, label: &str, count: usize) {
    let row = painted_row(rows, pane, label);
    assert!(
        row.contains(&format!("({count})")),
        "the painted {pane:?} row for {label:?} must keep its ({count}) count: {row:?}"
    );
}

/// B1, B2: the bucket the shipped rail cannot fit once its count reaches two
/// digits. `[x] Needs you (12)` plus the three-cell marker is 21 cells in a
/// 20-cell content rectangle, so before the fix the paint reads
/// `>> [x] Needs you (1…`.
#[test]
fn status_counts_survive_the_shipped_status_pane_width() {
    let mut state = repositories_state();
    state.repositories = vec![repository("one", "Repo one")];
    for index in 0..12 {
        seed(
            &mut state,
            &format!("waiting-{index}"),
            "repo-one",
            waiting_observation(),
        );
    }
    for index in 0..10 {
        seed(
            &mut state,
            &format!("ready-{index}"),
            "repo-one",
            ready_observation(),
        );
    }

    let rows = painted_rows(&state);

    assert_count_painted(&rows, RailPane::Status, "Needs", 12);
    assert_count_painted(&rows, RailPane::Status, "Working", 0);
    assert_count_painted(&rows, RailPane::Status, "Ready", 10);
    assert_count_painted(&rows, RailPane::Status, "Stale", 0);

    let needs_you = painted_row(&rows, RailPane::Status, "Needs");
    assert!(
        needs_you.contains('…'),
        "the bucket label, not the count, is what the 20-cell pane elides: {needs_you:?}"
    );
}

/// B3: a zero count is still a count. An empty workspace must not paint a
/// bucket row with the number missing.
#[test]
fn status_counts_survive_when_every_bucket_is_empty() {
    let mut state = repositories_state();
    state.repositories = vec![repository("one", "Repo one")];

    let rows = painted_rows(&state);

    for label in ["Needs you", "Working", "Ready", "Stale"] {
        assert_count_painted(&rows, RailPane::Status, label, 0);
    }
    // At a single-digit count every bucket label fits whole; the pane has
    // exactly zero slack left, which is how the corpus passed on this row.
}

/// B4: the sidebar's 18-cell content rectangle is narrower still, so a
/// real-length repository name pushes the count off the row entirely. The name
/// is what may be elided; the count is not.
#[test]
fn repository_counts_survive_a_name_that_overflows_the_sidebar() {
    let mut state = repositories_state();
    state.repositories = vec![repository("long", "llxprt-code-rs workspace")];
    for index in 0..3 {
        seed(
            &mut state,
            &format!("agent-{index}"),
            "repo-long",
            ready_observation(),
        );
    }

    let rows = painted_rows(&state);
    let row = painted_row(&rows, RailPane::Repositories, "llxprt");

    assert!(
        row.contains("(3)"),
        "the sidebar row must keep its (3) count: {row:?}"
    );
    assert!(
        row.contains('…'),
        "the overlong name, not the count, is the elided part: {row:?}"
    );
}

/// The rail carries two panes, and a repository is named by the operator, so a
/// sidebar row may contain a bucket word. Here `Ready to ship` sits above
/// `[x] Ready`, and the two counts differ, so a lookup that took the first
/// matching row anywhere in the rail would read the repository's `(4)` while
/// claiming to assert the bucket's `(1)`.
#[test]
fn a_repository_named_after_a_bucket_does_not_answer_for_the_bucket() {
    let mut state = repositories_state();
    state.repositories = vec![
        repository("one", "Repo one"),
        repository("ready", "Ready to ship"),
    ];
    seed(&mut state, "ready-agent", "repo-ready", ready_observation());
    for index in 0..3 {
        seed(
            &mut state,
            &format!("waiting-{index}"),
            "repo-ready",
            waiting_observation(),
        );
    }

    let rows = painted_rows(&state);

    let bucket = painted_row(&rows, RailPane::Status, "Ready");
    let repository = painted_row(&rows, RailPane::Repositories, "Ready");
    assert_ne!(
        bucket, repository,
        "the two panes must not resolve to the same painted row"
    );
    assert_count_painted(&rows, RailPane::Status, "Ready", 1);
    assert_count_painted(&rows, RailPane::Status, "Needs", 3);
    assert_count_painted(&rows, RailPane::Repositories, "Ready to", 4);
}
