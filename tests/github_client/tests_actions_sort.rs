//! Behavioral coverage for Actions workflow run sorting (issue #473).

use jefe::domain::{
    ActionsSortBy, ActionsSortConfig, SortOrder, WorkflowRun, WorkflowRunConclusion,
    WorkflowRunStatus,
};
use jefe::github::compare_workflow_runs;

fn run(id: u64, run_number: u32, created_at: &str, updated_at: &str) -> WorkflowRun {
    WorkflowRun {
        id,
        name: format!("Run {run_number}"),
        head_branch: "main".to_string(),
        head_sha: format!("sha{id}"),
        run_number,
        event: "push".to_string(),
        status: WorkflowRunStatus::Completed,
        conclusion: Some(WorkflowRunConclusion::Success),
        workflow_name: "CI".to_string(),
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

fn config(by: ActionsSortBy, order: SortOrder) -> ActionsSortConfig {
    ActionsSortConfig { by, order }
}

/// Helper: sort a vec of runs by the given config, return their run_numbers.
fn sort_and_extract_numbers(runs: &mut [WorkflowRun], cfg: ActionsSortConfig) -> Vec<u32> {
    runs.sort_by(|a, b| compare_workflow_runs(a, b, cfg));
    runs.iter().map(|r| r.run_number).collect()
}

// ── Number sort ─────────────────────────────────────────────────────────────

#[test]
fn number_desc_sorts_highest_first() {
    let mut runs = vec![
        run(1, 10, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(2, 30, "2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z"),
        run(3, 20, "2026-01-03T00:00:00Z", "2026-01-03T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Number, SortOrder::Desc)),
        vec![30, 20, 10]
    );
}

#[test]
fn number_asc_sorts_lowest_first() {
    let mut runs = vec![
        run(1, 10, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(2, 30, "2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z"),
        run(3, 20, "2026-01-03T00:00:00Z", "2026-01-03T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Number, SortOrder::Asc)),
        vec![10, 20, 30]
    );
}

// ── Created sort ────────────────────────────────────────────────────────────

#[test]
fn created_desc_sorts_newest_first() {
    let mut runs = vec![
        run(1, 1, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(2, 2, "2026-03-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        run(3, 3, "2026-02-01T00:00:00Z", "2026-02-01T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Created, SortOrder::Desc)),
        vec![2, 3, 1]
    );
}

#[test]
fn created_asc_sorts_oldest_first() {
    let mut runs = vec![
        run(1, 1, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(2, 2, "2026-03-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        run(3, 3, "2026-02-01T00:00:00Z", "2026-02-01T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Created, SortOrder::Asc)),
        vec![1, 3, 2]
    );
}

// ── Updated sort ────────────────────────────────────────────────────────────

#[test]
fn updated_desc_sorts_newest_first() {
    let mut runs = vec![
        run(1, 1, "2026-01-01T00:00:00Z", "2026-01-05T00:00:00Z"),
        run(2, 2, "2026-01-01T00:00:00Z", "2026-01-03T00:00:00Z"),
        run(3, 3, "2026-01-01T00:00:00Z", "2026-01-07T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Updated, SortOrder::Desc)),
        vec![3, 1, 2]
    );
}

#[test]
fn updated_asc_sorts_oldest_first() {
    let mut runs = vec![
        run(1, 1, "2026-01-01T00:00:00Z", "2026-01-05T00:00:00Z"),
        run(2, 2, "2026-01-01T00:00:00Z", "2026-01-03T00:00:00Z"),
        run(3, 3, "2026-01-01T00:00:00Z", "2026-01-07T00:00:00Z"),
    ];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Updated, SortOrder::Asc)),
        vec![2, 1, 3]
    );
}

// ── Tie-breaking ────────────────────────────────────────────────────────────

#[test]
fn created_desc_ties_break_by_id_desc() {
    // Same created_at; id descending is the tie-breaker (matches pre-#473 behavior).
    let mut runs = [
        run(100, 30, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(200, 10, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(300, 20, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ];
    runs.sort_by(|a, b| {
        compare_workflow_runs(a, b, config(ActionsSortBy::Created, SortOrder::Desc))
    });
    assert_eq!(
        runs.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![300, 200, 100]
    );
}

#[test]
fn created_asc_ties_break_by_id_desc_too() {
    // Same created_at; tie-break is always id descending regardless of sort direction.
    let mut runs = [
        run(100, 30, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(200, 10, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
        run(300, 20, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
    ];
    runs.sort_by(|a, b| {
        compare_workflow_runs(a, b, config(ActionsSortBy::Created, SortOrder::Asc))
    });
    assert_eq!(
        runs.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![300, 200, 100]
    );
}

// ── Default config ──────────────────────────────────────────────────────────

#[test]
fn default_sort_config_is_created_desc() {
    let cfg = ActionsSortConfig::default_sort();
    assert_eq!(cfg.by, ActionsSortBy::Created);
    assert_eq!(cfg.order, SortOrder::Desc);
}

#[test]
fn default_actions_sort_by_is_created() {
    assert_eq!(ActionsSortBy::default(), ActionsSortBy::Created);
}

// ── Cycle methods ───────────────────────────────────────────────────────────

#[test]
fn cycle_next_wraps_number_created_updated() {
    assert_eq!(ActionsSortBy::Number.cycle_next(), ActionsSortBy::Created);
    assert_eq!(ActionsSortBy::Created.cycle_next(), ActionsSortBy::Updated);
    assert_eq!(ActionsSortBy::Updated.cycle_next(), ActionsSortBy::Number);
}

#[test]
fn cycle_prev_wraps_updated_created_number() {
    assert_eq!(ActionsSortBy::Updated.cycle_prev(), ActionsSortBy::Created);
    assert_eq!(ActionsSortBy::Created.cycle_prev(), ActionsSortBy::Number);
    assert_eq!(ActionsSortBy::Number.cycle_prev(), ActionsSortBy::Updated);
}

#[test]
fn sort_by_labels_are_human_readable() {
    assert_eq!(ActionsSortBy::Number.label(), "number");
    assert_eq!(ActionsSortBy::Created.label(), "created");
    assert_eq!(ActionsSortBy::Updated.label(), "updated");
}

// ── Two-element sort ────────────────────────────────────────────────────────

#[test]
fn single_run_stays_in_place() {
    let mut runs = vec![run(1, 42, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z")];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Created, SortOrder::Desc)),
        vec![42]
    );
}

#[test]
fn empty_list_stays_empty() {
    let mut runs: Vec<WorkflowRun> = vec![];
    assert_eq!(
        sort_and_extract_numbers(&mut runs, config(ActionsSortBy::Number, SortOrder::Asc)),
        Vec::<u32>::new()
    );
}
