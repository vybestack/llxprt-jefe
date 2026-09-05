//! Repository-sidebar host-panel projection tests (issue #745).
//!
//! The sidebar's agent count is a count, not a status word, and the corpus
//! spells it `LLxprt Jefe (0)`. It rides the shared list control, whose
//! `status` suffix is `" [{value}]"`, so the projection hands the number over
//! as a typed `count` that the control renders `(N)` and keeps out of the
//! width budget it elides the name against.
//!
//! These tests own the *composition-root* row form. The retained pre-cutover
//! component (`src/ui/components/sidebar.rs`) is still live — the actions,
//! issues, pull-requests and errors screens mount it, and
//! `selection::content` projects it for text selection — but #715 repointed
//! the composition root off it, so its tests cannot pin what that screen
//! renders. Both paths spell the row the same way, and each is pinned where
//! it is rendered.

use crate::host_controls::project_control_body;
use crate::host_panel_models::{HostPanelModel, project_host_panel};
use crate::runtime::provider::protocol::{ListItem, PanelBody};
use crate::state::AppState;
use crate::test_support::{host_panel_agent, host_panel_repository};
use crate::workbench::HostPanelModelSource;

/// Two repositories: the first empty, the second holding two agents.
fn state_with_two_repositories() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![host_panel_repository("one"), host_panel_repository("two")];
    state.agents = vec![
        host_panel_agent("alpha", "repo-two", crate::domain::AgentStatus::Running),
        host_panel_agent("beta", "repo-two", crate::domain::AgentStatus::Running),
    ];
    state.selected_repository_index = Some(0);
    state
}

/// The content width the resolver hands the sidebar on the shipped split: the
/// 22-column rail less its chrome and list padding
/// (`src/workbench/screens.rs`). Rows are asserted there rather than at a
/// comfortable width, because that is where #752's folded count was lost.
const SIDEBAR_PANE_WIDTH: usize = 18;

fn repository_items(model: &HostPanelModel) -> &Vec<ListItem> {
    let PanelBody::List(body) = &model.body else {
        panic!(
            "the repository sidebar must project a list body, got {:?}",
            model.body.kind()
        );
    };
    &body.items
}

fn projected_rows(model: &HostPanelModel, width: usize) -> Vec<String> {
    project_control_body(
        &model.body,
        &model.action_affordances,
        model.selected_id.as_ref(),
        None,
        width,
    )
    .into_iter()
    .map(|row| row.text)
    .collect()
}

/// Issue #745 A1: the count is a typed count the shared control renders `(N)`
/// and protects, so the shared `" [{value}]"` status suffix never reaches a
/// sidebar row and the number never rides inside the truncatable name.
#[test]
fn repository_rows_carry_the_parenthesized_agent_count() {
    let state = state_with_two_repositories();

    let model = project_host_panel(&state, HostPanelModelSource::RepositoryList);

    assert_eq!(model.title, "Repositories");
    let items = repository_items(&model);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.label.as_str(), item.count))
            .collect::<Vec<_>>(),
        [("Repo one", Some(0)), ("Repo two", Some(2))],
        "the label is the repository name; the count is carried beside it"
    );
    assert_eq!(
        projected_rows(&model, SIDEBAR_PANE_WIDTH),
        [">> Repo one (0)", "   Repo two (2)"],
        "and the rows the pane paints spell `(N)`, as the corpus pins it"
    );
    assert!(
        items.iter().all(|item| item.status.is_none()),
        "a count is not a status word, so the shared `[value]` suffix stays clear: {items:?}"
    );
    assert!(
        items.iter().all(|item| item.description.is_none()),
        "one row per repository: no description second rows"
    );
}

/// Issue #745 A2: through the shared control the selection marker still leads
/// the row and the parenthesized count still terminates it.
#[test]
fn repository_row_renders_the_parenthesized_count_through_the_shared_control() {
    let state = state_with_two_repositories();

    let model = project_host_panel(&state, HostPanelModelSource::RepositoryList);

    assert_eq!(
        projected_rows(&model, 40),
        [">> Repo one (0)", "   Repo two (2)"],
        "the rendered sidebar rows are the corpus form"
    );
}

/// Issue #745 A6: #723's invariant holds. A row too long for the pane is
/// truncated in place; it never becomes a second row that shifts every later
/// repository down. The follow-up adds what #752 could not keep: the name is
/// the span that gives way, and the count is still there afterwards.
#[test]
fn an_overlong_repository_row_truncates_the_name_and_keeps_the_count() {
    let mut state = state_with_two_repositories();
    state.repositories[0].name = "a".repeat(40);

    let model = project_host_panel(&state, HostPanelModelSource::RepositoryList);
    let rows = projected_rows(&model, 16);

    assert_eq!(rows.len(), 2, "one row per repository: {rows:?}");
    let first = rows.first().map(String::as_str).unwrap_or_default();
    assert!(first.starts_with(">> "), "the marker survives: {first:?}");
    assert!(
        first.chars().count() <= 16,
        "the row fits the pane width: {first:?}"
    );
    assert!(
        first.contains('\u{2026}'),
        "the overlong name is visibly truncated: {first:?}"
    );
    assert!(
        first.ends_with(" (0)"),
        "the count outlives the name it belongs to: {first:?}"
    );
}
