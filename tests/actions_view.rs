use jefe::actions_view::project_runs_list;
use jefe::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus};

fn run(id: u64) -> WorkflowRun {
    WorkflowRun {
        id,
        name: format!("run {id}"),
        head_branch: "main".to_string(),
        head_sha: format!("sha{id}"),
        run_number: u32::try_from(id).unwrap_or_default(),
        event: "push".to_string(),
        status: WorkflowRunStatus::Completed,
        conclusion: Some(WorkflowRunConclusion::Success),
        workflow_name: "CI".to_string(),
        created_at: "time".to_string(),
        updated_at: "time".to_string(),
    }
}

#[test]
fn selection_is_centered_when_possible() {
    let runs = (0..10).map(run).collect::<Vec<_>>();
    let projection = project_runs_list(&runs, Some(5), 3);
    assert_eq!(projection.first_visible_run_index, 3);
    assert_eq!(projection.visible_runs.len(), 3);
    assert_eq!(projection.visible_runs[0].id, 3);
    assert_eq!(projection.visible_runs[1].id, 4);
    assert_eq!(projection.visible_runs[2].id, 5);
    assert!(projection.visible_runs[2].is_selected);
}

#[test]
fn final_page_stays_full() {
    let runs = (0..7).map(run).collect::<Vec<_>>();
    let view = project_runs_list(&runs, Some(6), 5);
    assert_eq!(view.first_visible_run_index, 2);
    assert_eq!(view.visible_runs.len(), 5);
    assert!(view.visible_runs.last().is_some_and(|run| run.id == 6));
}

#[test]
fn empty_and_zero_height_are_total() {
    assert!(project_runs_list(&[], None, 5).visible_runs.is_empty());
    let runs = (0..3).map(run).collect::<Vec<_>>();
    assert!(project_runs_list(&runs, Some(2), 0).visible_runs.is_empty());
}

#[test]
fn short_list_stays_visible_from_the_top() {
    let runs = (0..2).map(run).collect::<Vec<_>>();
    let view = project_runs_list(&runs, Some(1), 5);
    assert_eq!(view.first_visible_run_index, 0);
    assert_eq!(view.visible_runs.len(), 2);
    assert!(view.visible_runs[1].is_selected);
}

#[test]
fn stale_selection_clamps_window_without_marking_invalid_row() {
    let runs = (0..3).map(run).collect::<Vec<_>>();
    let view = project_runs_list(&runs, Some(99), 2);
    assert_eq!(view.first_visible_run_index, 1);
    assert!(view.visible_runs.iter().all(|run| !run.is_selected));
}
