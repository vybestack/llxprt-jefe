//! Pure GitHub Actions run-list viewport projection.
//!
//! This iocraft-free module maps loaded runs and list geometry into a stable
//! selection-following window. Job-detail projection lives in
//! [`crate::actions_detail_view`].

use crate::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus};

/// A single run in the projected runs list view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRun {
    pub id: u64,
    pub name: String,
    pub head_branch: String,
    pub head_sha: String,
    pub run_number: u32,
    pub event: String,
    pub workflow_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: WorkflowRunStatus,
    pub conclusion: Option<WorkflowRunConclusion>,
    pub is_selected: bool,
}

/// The projected window of workflow runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsRunListView {
    pub visible_runs: Vec<ProjectedRun>,
    pub first_visible_run_index: usize,
    pub total_runs_count: usize,
}

/// Project the full list of runs into a scroll-windowed view.
#[must_use]
pub fn project_runs_list(
    runs: &[WorkflowRun],
    selected_run_index: Option<usize>,
    list_viewport_height: usize,
) -> ActionsRunListView {
    if runs.is_empty() {
        return ActionsRunListView {
            visible_runs: Vec::new(),
            first_visible_run_index: 0,
            total_runs_count: 0,
        };
    }
    let selected_idx = selected_run_index.unwrap_or(0).min(runs.len() - 1);
    let max_first_visible = runs.len().saturating_sub(list_viewport_height);
    let first_visible_run = selected_idx
        .saturating_sub(list_viewport_height / 2)
        .min(max_first_visible);
    let end = (first_visible_run + list_viewport_height).min(runs.len());
    let visible_runs = runs[first_visible_run..end]
        .iter()
        .enumerate()
        .map(|(offset, run)| projected_run(run, first_visible_run + offset, selected_run_index))
        .collect();
    ActionsRunListView {
        visible_runs,
        first_visible_run_index: first_visible_run,
        total_runs_count: runs.len(),
    }
}

fn projected_run(run: &WorkflowRun, index: usize, selected: Option<usize>) -> ProjectedRun {
    ProjectedRun {
        id: run.id,
        name: run.name.clone(),
        head_branch: run.head_branch.clone(),
        head_sha: run.head_sha.clone(),
        run_number: run.run_number,
        event: run.event.clone(),
        workflow_name: run.workflow_name.clone(),
        created_at: run.created_at.clone(),
        updated_at: run.updated_at.clone(),
        status: run.status,
        conclusion: run.conclusion,
        is_selected: selected == Some(index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let runs: Vec<_> = (0..10).map(run).collect();
        let view = project_runs_list(&runs, Some(5), 3);

        assert_eq!(view.first_visible_run_index, 4);
        assert_eq!(view.visible_runs.len(), 3);
        assert!(view.visible_runs[1].is_selected);
    }

    #[test]
    fn final_page_stays_full() {
        let runs: Vec<_> = (0..7).map(run).collect();
        let view = project_runs_list(&runs, Some(6), 5);

        assert_eq!(view.first_visible_run_index, 2);
        assert_eq!(view.visible_runs.len(), 5);
        assert!(view.visible_runs.last().is_some_and(|run| run.id == 6));
    }

    #[test]
    fn empty_and_zero_height_are_total() {
        assert!(project_runs_list(&[], None, 5).visible_runs.is_empty());
        let runs: Vec<_> = (0..3).map(run).collect();
        assert!(project_runs_list(&runs, Some(2), 0).visible_runs.is_empty());
    }

    #[test]
    fn short_list_stays_visible_from_the_top() {
        let runs: Vec<_> = (0..2).map(run).collect();
        let view = project_runs_list(&runs, Some(1), 5);

        assert_eq!(view.first_visible_run_index, 0);
        assert_eq!(view.visible_runs.len(), 2);
        assert!(view.visible_runs[1].is_selected);
    }

    #[test]
    fn stale_selection_clamps_window_without_marking_invalid_row() {
        let runs: Vec<_> = (0..3).map(run).collect();
        let view = project_runs_list(&runs, Some(99), 2);

        assert_eq!(view.first_visible_run_index, 1);
        assert!(view.visible_runs.iter().all(|run| !run.is_selected));
    }
}
