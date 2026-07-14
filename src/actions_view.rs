//! Pure GitHub Actions run-list viewport projection.
//!
//! This iocraft-free module maps loaded runs and list geometry into a stable
//! selection-following window. Job-detail projection lives in
//! [`crate::actions_detail_view`].

use crate::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus};
use crate::list_viewport::{ContentRows, ListViewport, RowsPerItem};

/// A single run in the projected runs list view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRun {
    /// Absolute workflow-run index represented by this visible row.
    pub source_index: usize,
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
    let viewport = ListViewport::uniform(
        runs.len(),
        selected_run_index,
        ContentRows::new(list_viewport_height),
        RowsPerItem::new(1),
    );
    let first_visible_run = viewport.first_visible_item();
    let visible_slice = &runs[viewport.visible_range()];

    let visible_runs = visible_slice
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let actual_idx = first_visible_run + i;
            ProjectedRun {
                source_index: actual_idx,
                id: r.id,
                name: r.name.clone(),
                head_branch: r.head_branch.clone(),
                head_sha: r.head_sha.clone(),
                run_number: r.run_number,
                event: r.event.clone(),
                workflow_name: r.workflow_name.clone(),
                created_at: r.created_at.clone(),
                updated_at: r.updated_at.clone(),
                status: r.status,
                conclusion: r.conclusion,
                is_selected: selected_run_index == Some(actual_idx),
            }
        })
        .collect();
    ActionsRunListView {
        visible_runs,
        first_visible_run_index: first_visible_run,
        total_runs_count: runs.len(),
    }
}
