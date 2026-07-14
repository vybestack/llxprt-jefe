//! Shared finite-width Actions detail body projection.
//!
//! Selection/copy consumers delegate to the same wrapped projection used by
//! rendering and reducer scroll bounds.

use crate::domain::WorkflowRunDetail;
use crate::layout::ActionsDetailGeometry;

/// Project the scrollable body into the wrapped physical rows rendered by the UI.
#[must_use]
pub fn actions_detail_body_lines<S: std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &std::collections::HashSet<u64, S>,
    focused_job_index: Option<usize>,
    geometry: ActionsDetailGeometry,
) -> Vec<String> {
    crate::actions_detail_view::project_actions_detail(
        detail,
        expanded_jobs,
        focused_job_index,
        geometry,
    )
    .rows
    .into_iter()
    .map(|row| row.text)
    .collect()
}
