//! Shared finite-width Actions detail body projection.

use crate::domain::WorkflowRunDetail;

/// Project the scrollable body into one finite-width physical row per logical item.
#[must_use]
pub fn actions_detail_body_lines<S: std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &std::collections::HashSet<u64, S>,
    content_width: usize,
) -> Vec<String> {
    crate::actions_view::detail_body_lines(detail, expanded_jobs)
        .iter()
        .map(crate::ui::components::actions_detail_line_text)
        .map(|line| crate::list_viewport::fit_text_to_width(&line, content_width))
        .collect()
}

/// Newline-joined body text consumed by the generic detail renderer.
#[must_use]
pub fn actions_detail_body_text<S: std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &std::collections::HashSet<u64, S>,
    content_width: usize,
) -> String {
    actions_detail_body_lines(detail, expanded_jobs, content_width).join("\n")
}
