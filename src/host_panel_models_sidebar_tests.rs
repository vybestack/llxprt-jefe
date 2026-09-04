//! Repository-sidebar host-panel projection tests (issue #745).
//!
//! The sidebar's agent count is a count, not a status word, and the corpus
//! spells it `LLxprt Jefe (0)`. It rides the shared list control, whose
//! `status` suffix is `" [{value}]"`, so the projection composes the count
//! into its own label instead of handing it over as a `status` value.
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

/// Issue #745 A1: the count is parenthesized and lives in the label, so the
/// shared `" [{value}]"` status suffix never reaches a sidebar row.
#[test]
fn repository_rows_carry_the_parenthesized_agent_count() {
    let state = state_with_two_repositories();

    let model = project_host_panel(&state, HostPanelModelSource::RepositoryList);

    assert_eq!(model.title, "Repositories");
    let items = repository_items(&model);
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["Repo one (0)", "Repo two (2)"],
        "the sidebar spells its agent count `(N)`, as the corpus pins it"
    );
    assert!(
        items.iter().all(|item| item.status.is_none()),
        "the count is the projection's own suffix, not a shared status value: {items:?}"
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

/// Issue #745 A6: #723's invariant survives the fold. A row too long for the
/// pane is truncated in place; it never becomes a second row that shifts every
/// later repository down.
#[test]
fn a_folded_repository_row_truncates_instead_of_wrapping() {
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
}
