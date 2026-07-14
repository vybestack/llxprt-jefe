//! Pure GitHub Actions list and detail viewport projections.
//!
//! This module is iocraft-free and side-effect-free: it maps the actions state
//! (runs, selected index, details, scrolling offset) plus viewport heights
//! into windowed display projections. This makes the scrolling math and layout
//! logic fully unit-testable without a terminal.

use crate::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunStatus};
use crate::list_viewport::{ContentRows, ListViewport, RowsPerItem};
use std::collections::HashSet;

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

/// Count legacy projected lines: one compatibility header plus the detail body.
#[must_use]
pub fn detail_line_count<S: ::std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &HashSet<u64, S>,
) -> usize {
    detail_body_line_count(detail, expanded_jobs) + 1
}

/// Count scrollable Actions detail body lines without allocating projections.
#[must_use]
pub fn detail_body_line_count<S: ::std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &HashSet<u64, S>,
) -> usize {
    1 + detail
        .jobs
        .iter()
        .map(|job| 1 + usize::from(expanded_jobs.contains(&job.id)) * job.steps.len())
        .sum::<usize>()
}

/// Project the complete, unwindowed scrollable Actions detail body.
///
/// Scrolling and wrapping are renderer/input concerns. Keeping this projection
/// unwindowed ensures rendering, copy, and reverse coordinate mapping consume
/// the same logical lines exactly once.
#[must_use]
pub fn detail_body_lines<S: ::std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &HashSet<u64, S>,
) -> Vec<DetailLine> {
    let mut lines = vec![DetailLine::SectionTitle {
        title: "Jobs:".to_string(),
    }];
    for job in &detail.jobs {
        let is_expanded = expanded_jobs.contains(&job.id);
        lines.push(DetailLine::JobRow {
            job_id: job.id,
            name: job.name.clone(),
            status: job.status,
            conclusion: job.conclusion,
            expanded: is_expanded,
        });
        if is_expanded {
            lines.extend(job.steps.iter().map(|step| DetailLine::StepRow {
                number: step.number,
                name: step.name.clone(),
                status: step.status,
                conclusion: step.conclusion,
            }));
        }
    }
    lines
}

/// A structured line in the projected run details view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailLine {
    Header {
        workflow_name: String,
        event: String,
        head_branch: String,
        head_sha: String,
        created_at: String,
        updated_at: String,
    },
    SectionTitle {
        title: String,
    },
    JobRow {
        job_id: u64,
        name: String,
        status: crate::domain::WorkflowRunStatus,
        conclusion: Option<crate::domain::WorkflowRunConclusion>,
        expanded: bool,
    },
    StepRow {
        number: u32,
        name: String,
        status: crate::domain::WorkflowRunStatus,
        conclusion: Option<crate::domain::WorkflowRunConclusion>,
    },
}

/// The projected window of run details (jobs/steps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsDetailView {
    pub visible_lines: Vec<DetailLine>,
    pub total_lines_count: usize,
}

/// Project the workflow run details into a scroll-windowed view.
///
/// Jobs not in `expanded_jobs` render as a single JobRow; expanded jobs also
/// render their StepRows. The JobRow carries an `expanded` flag so the
/// renderer can show the `\u{25B8}`/`\u{25BE}` indicator.
#[must_use]
pub fn project_detail_view<S: ::std::hash::BuildHasher>(
    detail: &WorkflowRunDetail,
    detail_scroll_offset: usize,
    detail_viewport_height: usize,
    expanded_jobs: &HashSet<u64, S>,
) -> ActionsDetailView {
    let mut lines = vec![DetailLine::Header {
        workflow_name: detail.run.workflow_name.clone(),
        event: detail.run.event.clone(),
        head_branch: detail.run.head_branch.clone(),
        head_sha: detail.run.head_sha.clone(),
        created_at: detail.run.created_at.clone(),
        updated_at: detail.run.updated_at.clone(),
    }];
    lines.extend(detail_body_lines(detail, expanded_jobs));
    let total_lines_count = lines.len();
    let start = detail_scroll_offset.min(total_lines_count.saturating_sub(1));
    let visible_lines = if lines.is_empty() {
        Vec::new()
    } else {
        lines
            .into_iter()
            .skip(start)
            .take(detail_viewport_height)
            .collect()
    };

    ActionsDetailView {
        visible_lines,
        total_lines_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        WorkflowRun, WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunJob, WorkflowRunStatus,
        WorkflowRunStep,
    };

    fn make_run(id: u64) -> WorkflowRun {
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
    fn test_project_runs_list_scrolling() {
        let runs: Vec<WorkflowRun> = (0..10).map(make_run).collect();

        // Trailing-edge follow keeps the selected run at the bottom edge.
        let projection = project_runs_list(&runs, Some(5), 3);
        assert_eq!(projection.first_visible_run_index, 3);
        assert_eq!(projection.visible_runs.len(), 3);
        assert_eq!(projection.visible_runs[0].id, 3);
        assert_eq!(projection.visible_runs[1].id, 4);
        assert_eq!(projection.visible_runs[2].id, 5);
        assert!(projection.visible_runs[2].is_selected);
    }

    #[test]
    fn test_project_runs_list_empty_no_panic() {
        let projection = project_runs_list(&[], None, 5);
        assert!(projection.visible_runs.is_empty());
        assert_eq!(projection.first_visible_run_index, 0);
        assert_eq!(projection.total_runs_count, 0);
    }

    #[test]
    fn test_project_runs_list_zero_height_viewport() {
        let runs: Vec<WorkflowRun> = (0..5).map(make_run).collect();
        let projection = project_runs_list(&runs, Some(2), 0);
        // Zero-height viewport: no visible runs but no panic.
        assert!(projection.visible_runs.is_empty());
        assert_eq!(projection.total_runs_count, 5);
    }

    #[test]
    fn test_project_runs_list_stale_selected_eq_len_no_panic() {
        let runs: Vec<WorkflowRun> = vec![make_run(0), make_run(1), make_run(2)];
        // selected_run_index == runs.len() (3 == 3) — stale, must not panic.
        let projection = project_runs_list(&runs, Some(3), 5);
        assert_eq!(projection.total_runs_count, 3);
        // The clamped index should be 2 (len-1), not 3.
        assert!(
            projection.visible_runs.iter().all(|r| r.id <= 2),
            "clamped selection must not exceed last index"
        );
    }

    #[test]
    fn test_project_runs_list_stale_selected_greatly_exceeds_len_no_panic() {
        let runs: Vec<WorkflowRun> = vec![make_run(0), make_run(1)];
        // selected_run_index >> runs.len() — extremely stale, must not panic.
        let projection = project_runs_list(&runs, Some(999), 3);
        assert_eq!(projection.total_runs_count, 2);
        assert!(
            projection.visible_runs.iter().all(|r| r.id <= 1),
            "greatly-exceeding selection must clamp to valid range"
        );
    }

    #[test]
    fn test_project_runs_list_final_partial_page() {
        let runs: Vec<WorkflowRun> = (0..7).map(make_run).collect();
        // Select the last run (index 6) with a 5-row viewport. The window
        // start is clamped to len-viewport = 7-5 = 2 so a full 5-row page fits
        // (runs 2..7) rather than sliding to show only a partial tail.
        let projection = project_runs_list(&runs, Some(6), 5);
        assert_eq!(
            projection.first_visible_run_index, 2,
            "window start must leave room for a full viewport"
        );
        assert_eq!(
            projection.visible_runs.len(),
            5,
            "a full page must fit when the list is longer than the viewport"
        );
        assert_eq!(projection.total_runs_count, 7);
        // The selected (last) run should be visible.
        assert!(
            projection.visible_runs.last().is_some_and(|r| r.id == 6),
            "final page must include the last run"
        );
    }

    #[test]
    fn test_project_runs_list_short_list_shows_all_from_top() {
        // When the list is shorter than the viewport, the window start
        // saturates to 0 and everything is shown from the top.
        let runs: Vec<WorkflowRun> = vec![make_run(0), make_run(1)];
        let projection = project_runs_list(&runs, Some(1), 5);
        assert_eq!(projection.first_visible_run_index, 0);
        assert_eq!(projection.visible_runs.len(), 2);
    }

    #[test]
    fn test_project_detail_view_scrolling_and_clamp() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![WorkflowRunJob {
                id: 0,
                name: "build".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                steps: vec![
                    WorkflowRunStep {
                        name: "checkout".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Success),
                        number: 1,
                    },
                    WorkflowRunStep {
                        name: "compile".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Success),
                        number: 2,
                    },
                ],
            }],
        };
        // All jobs expanded for backward-compat with the original test.
        let expanded: HashSet<u64> = HashSet::from([0u64]);
        // 2 (header + section title) + 1 job + 2 steps = 5 lines.
        assert_eq!(detail_line_count(&detail, &expanded), 5);

        // Normal windowed projection from offset 0 with viewport 3 -> first 3.
        let view = project_detail_view(&detail, 0, 3, &expanded);
        assert_eq!(view.total_lines_count, 5);
        assert_eq!(view.visible_lines.len(), 3);

        // Offset 1 with viewport 3 -> lines 1..4 (3 lines).
        let view = project_detail_view(&detail, 1, 3, &expanded);
        assert_eq!(view.visible_lines.len(), 3);

        // Offset past end clamps to last line (4), showing 1 line.
        let view = project_detail_view(&detail, 999, 3, &expanded);
        assert_eq!(view.visible_lines.len(), 1);
    }

    #[test]
    fn test_project_detail_view_zero_viewport() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: Vec::new(),
        };
        let expanded = HashSet::new();
        // Zero-height viewport: no visible lines but no panic.
        let view = project_detail_view(&detail, 0, 0, &expanded);
        assert_eq!(view.total_lines_count, 2);
        assert!(view.visible_lines.is_empty());
    }

    #[test]
    fn test_project_detail_view_line_order() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![WorkflowRunJob {
                id: 0,
                name: "build".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                steps: vec![WorkflowRunStep {
                    name: "checkout".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    number: 1,
                }],
            }],
        };
        let expanded: HashSet<u64> = HashSet::from([0u64]);
        // Header -> SectionTitle -> JobRow -> StepRow
        let view = project_detail_view(&detail, 0, 10, &expanded);
        assert_eq!(view.visible_lines.len(), 4);
        assert!(matches!(view.visible_lines[0], DetailLine::Header { .. }));
        assert!(matches!(
            view.visible_lines[1],
            DetailLine::SectionTitle { .. }
        ));
        assert!(matches!(view.visible_lines[2], DetailLine::JobRow { .. }));
        assert!(matches!(view.visible_lines[3], DetailLine::StepRow { .. }));
    }

    #[test]
    fn test_detail_line_count_zero_jobs() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: Vec::new(),
        };
        let expanded = HashSet::new();
        // 1 header + 1 section title + 0 jobs = 2
        assert_eq!(detail_line_count(&detail, &expanded), 2);
    }

    #[test]
    fn test_detail_line_count_one_job_zero_steps() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![WorkflowRunJob {
                id: 0,
                name: "build".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: None,
                steps: Vec::new(),
            }],
        };
        let expanded = HashSet::new();
        // 2 + 1 job + 0 steps = 3
        assert_eq!(detail_line_count(&detail, &expanded), 3);
    }

    #[test]
    fn test_detail_line_count_multiple_jobs_and_steps() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![
                WorkflowRunJob {
                    id: 0,
                    name: "build".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![
                        WorkflowRunStep {
                            name: "checkout".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 1,
                        },
                        WorkflowRunStep {
                            name: "compile".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 2,
                        },
                    ],
                },
                WorkflowRunJob {
                    id: 1,
                    name: "test".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![WorkflowRunStep {
                        name: "unit-tests".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Success),
                        number: 1,
                    }],
                },
            ],
        };
        let expanded: HashSet<u64> = [0u64, 1u64].into_iter().collect();
        // 2 (header+title) + 1 job + 2 steps + 1 job + 1 step = 7
        assert_eq!(detail_line_count(&detail, &expanded), 7);
    }

    // ---- BUG 5: expand/collapse ----

    #[test]
    fn detail_line_count_all_collapsed() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![
                WorkflowRunJob {
                    id: 10,
                    name: "build".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![
                        WorkflowRunStep {
                            name: "checkout".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 1,
                        },
                        WorkflowRunStep {
                            name: "compile".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 2,
                        },
                    ],
                },
                WorkflowRunJob {
                    id: 20,
                    name: "test".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![WorkflowRunStep {
                        name: "unit-tests".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Success),
                        number: 1,
                    }],
                },
            ],
        };
        let expanded = HashSet::new();
        // 2 (header+title) + 2 jobs (collapsed, no steps) = 4
        assert_eq!(
            detail_line_count(&detail, &expanded),
            2 + 2,
            "all collapsed: 2 + num_jobs"
        );
    }

    #[test]
    fn detail_line_count_one_job_expanded() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![
                WorkflowRunJob {
                    id: 10,
                    name: "build".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![
                        WorkflowRunStep {
                            name: "checkout".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 1,
                        },
                        WorkflowRunStep {
                            name: "compile".to_string(),
                            status: WorkflowRunStatus::Completed,
                            conclusion: Some(WorkflowRunConclusion::Success),
                            number: 2,
                        },
                    ],
                },
                WorkflowRunJob {
                    id: 20,
                    name: "test".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    steps: vec![WorkflowRunStep {
                        name: "unit-tests".to_string(),
                        status: WorkflowRunStatus::Completed,
                        conclusion: Some(WorkflowRunConclusion::Success),
                        number: 1,
                    }],
                },
            ],
        };
        let expanded: HashSet<u64> = HashSet::from([10u64]);
        // 2 + 2 jobs + 2 steps in expanded job 10 = 6
        assert_eq!(
            detail_line_count(&detail, &expanded),
            2 + 2 + 2,
            "one expanded: 2 + num_jobs + steps_in_expanded_job"
        );
    }

    #[test]
    fn project_detail_view_collapsed_omits_steps() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![WorkflowRunJob {
                id: 10,
                name: "build".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                steps: vec![WorkflowRunStep {
                    name: "checkout".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    number: 1,
                }],
            }],
        };
        let expanded = HashSet::new();
        let view = project_detail_view(&detail, 0, 10, &expanded);
        // Header + SectionTitle + 1 JobRow (no steps) = 3 lines.
        assert_eq!(view.visible_lines.len(), 3);
        // The JobRow must exist but no StepRow.
        assert!(matches!(view.visible_lines[2], DetailLine::JobRow { .. }));
        assert!(
            !view
                .visible_lines
                .iter()
                .any(|l| matches!(l, DetailLine::StepRow { .. })),
            "collapsed job must not emit StepRows"
        );
        // The JobRow must carry expanded: false.
        assert!(matches!(
            view.visible_lines[2],
            DetailLine::JobRow {
                expanded: false,
                ..
            }
        ));
    }

    #[test]
    fn project_detail_view_expanded_includes_steps() {
        let detail = WorkflowRunDetail {
            run: make_run(0),
            jobs: vec![WorkflowRunJob {
                id: 10,
                name: "build".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                steps: vec![WorkflowRunStep {
                    name: "checkout".to_string(),
                    status: WorkflowRunStatus::Completed,
                    conclusion: Some(WorkflowRunConclusion::Success),
                    number: 1,
                }],
            }],
        };
        let expanded: HashSet<u64> = HashSet::from([10u64]);
        let view = project_detail_view(&detail, 0, 10, &expanded);
        // Header + SectionTitle + 1 JobRow + 1 StepRow = 4 lines.
        assert_eq!(view.visible_lines.len(), 4);
        assert!(matches!(
            view.visible_lines[2],
            DetailLine::JobRow { expanded: true, .. }
        ));
        assert!(matches!(view.visible_lines[3], DetailLine::StepRow { .. }));
    }
}
