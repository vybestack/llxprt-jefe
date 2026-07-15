//! Pure Actions job-detail geometry and wrapped display-row projection.
//!
//! This module is the single iocraft-free source of truth for job focus
//! normalization, status text, wrapping, scroll bounds, and focus reveal.

use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::domain::{WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunStatus};
use crate::layout::ActionsDetailGeometry;
use crate::text_wrap::wrap_text;

/// One wrapped row rendered in the Actions detail document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsDetailRow {
    /// Plain text rendered for this display row.
    pub text: String,
    /// Job owning this row when it is part of a job heading.
    pub job_index: Option<usize>,
}

/// Complete wrapped Actions detail projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsDetailProjection {
    /// Display rows in document order.
    pub rows: Vec<ActionsDetailRow>,
    /// Normalized job focus used by both renderer and reducer transitions.
    pub focused_job_index: Option<usize>,
}

impl ActionsDetailProjection {
    /// Return the first display row occupied by the focused job heading.
    #[must_use]
    pub fn focused_job_row(&self) -> Option<usize> {
        let focused = self.focused_job_index?;
        self.rows
            .iter()
            .position(|row| row.job_index == Some(focused))
    }

    /// Return the greatest valid display-row scroll offset.
    #[must_use]
    pub fn max_scroll_offset(&self, viewport_rows: usize) -> usize {
        self.rows.len().saturating_sub(viewport_rows)
    }

    /// Clamp an offset and reveal the focused job within the display viewport.
    ///
    /// A zero-height viewport retains a bounded document-row anchor so a later
    /// resize can reveal the same focused job without losing its position.
    #[must_use]
    pub fn reveal_focused_job(&self, offset: usize, viewport_rows: usize) -> usize {
        let max = self.max_scroll_offset(viewport_rows);
        let current = offset.min(max);
        let Some(row) = self.focused_job_row() else {
            return current;
        };
        if viewport_rows == 0 || row < current {
            return row.min(max);
        }
        if row >= current.saturating_add(viewport_rows) {
            return row.saturating_add(1).saturating_sub(viewport_rows).min(max);
        }
        current
    }

    /// Join projected display rows into the pre-wrapped renderer document.
    #[must_use]
    pub fn content(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Normalize a possibly stale job index against the loaded detail.
#[must_use]
pub fn normalize_job_focus(
    detail: &WorkflowRunDetail,
    focused_job_index: Option<usize>,
) -> Option<usize> {
    let last = detail.jobs.len().checked_sub(1)?;
    Some(focused_job_index.unwrap_or(0).min(last))
}

/// Project a run detail into the exact wrapped rows consumed by the renderer.
#[must_use]
pub fn project_actions_detail<S: BuildHasher>(
    detail: &WorkflowRunDetail,
    expanded_jobs: &HashSet<u64, S>,
    focused_job_index: Option<usize>,
    geometry: ActionsDetailGeometry,
) -> ActionsDetailProjection {
    let focused_job_index = normalize_job_focus(detail, focused_job_index);
    let mut rows = Vec::new();
    push_wrapped_row(&mut rows, String::new(), None, geometry.content_width);
    push_wrapped_row(&mut rows, "Jobs:".to_string(), None, geometry.content_width);
    for (job_index, job) in detail.jobs.iter().enumerate() {
        let expanded = expanded_jobs.contains(&job.id);
        let marker = if focused_job_index == Some(job_index) {
            ">"
        } else {
            " "
        };
        let indicator = if expanded { "\u{25BE}" } else { "\u{25B8}" };
        let glyph = status_glyph(job.status, job.conclusion);
        push_wrapped_row(
            &mut rows,
            format!("{marker} {indicator} {glyph} {}", job.name),
            Some(job_index),
            geometry.content_width,
        );
        if expanded {
            for step in &job.steps {
                let glyph = status_glyph(step.status, step.conclusion);
                push_wrapped_row(
                    &mut rows,
                    format!("  {glyph} [{}] {}", step.number, step.name),
                    None,
                    geometry.content_width,
                );
            }
        }
    }
    ActionsDetailProjection {
        rows,
        focused_job_index,
    }
}

fn push_wrapped_row(
    rows: &mut Vec<ActionsDetailRow>,
    text: String,
    job_index: Option<usize>,
    width: usize,
) {
    rows.extend(
        wrap_text(&text, width)
            .into_iter()
            .map(|row| ActionsDetailRow {
                text: row.text,
                job_index,
            }),
    );
}

fn status_glyph(
    status: WorkflowRunStatus,
    conclusion: Option<WorkflowRunConclusion>,
) -> &'static str {
    match status {
        WorkflowRunStatus::Completed => match conclusion {
            Some(WorkflowRunConclusion::Success) => "\u{2713}",
            Some(
                WorkflowRunConclusion::Failure
                | WorkflowRunConclusion::TimedOut
                | WorkflowRunConclusion::ActionRequired
                | WorkflowRunConclusion::StartupFailure,
            ) => "\u{2717}",
            Some(
                WorkflowRunConclusion::Cancelled
                | WorkflowRunConclusion::Skipped
                | WorkflowRunConclusion::Stale
                | WorkflowRunConclusion::Neutral,
            ) => "\u{2298}",
            Some(WorkflowRunConclusion::Unknown) | None => "?",
        },
        WorkflowRunStatus::InProgress => "~",
        WorkflowRunStatus::Queued
        | WorkflowRunStatus::Requested
        | WorkflowRunStatus::Waiting
        | WorkflowRunStatus::Pending => ".",
        WorkflowRunStatus::Unknown => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{WorkflowRun, WorkflowRunJob, WorkflowRunStep};

    fn run() -> WorkflowRun {
        WorkflowRun {
            id: 1,
            name: "run".to_string(),
            head_branch: "main".to_string(),
            head_sha: "abc".to_string(),
            run_number: 1,
            event: "push".to_string(),
            status: WorkflowRunStatus::Completed,
            conclusion: Some(WorkflowRunConclusion::Success),
            workflow_name: "CI".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn job(id: u64, name: &str, step_name: &str) -> WorkflowRunJob {
        WorkflowRunJob {
            id,
            name: name.to_string(),
            status: WorkflowRunStatus::Completed,
            conclusion: Some(if id == 2 {
                WorkflowRunConclusion::Failure
            } else {
                WorkflowRunConclusion::Success
            }),
            steps: vec![WorkflowRunStep {
                name: step_name.to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(if id == 2 {
                    WorkflowRunConclusion::Failure
                } else {
                    WorkflowRunConclusion::Success
                }),
                number: 1,
            }],
        }
    }

    fn detail() -> WorkflowRunDetail {
        WorkflowRunDetail {
            run: run(),
            jobs: vec![
                job(
                    1,
                    "a very long build job heading",
                    "a very long checkout step name",
                ),
                job(2, "test", "failing tests"),
                job(3, "final deployment job", "publish"),
            ],
        }
    }

    fn geometry(width: usize, rows: usize) -> ActionsDetailGeometry {
        ActionsDetailGeometry {
            viewport_rows: rows,
            content_width: width,
        }
    }

    #[test]
    fn narrow_projection_wraps_job_and_step_and_preserves_status_glyphs() {
        let detail = detail();
        let expanded = HashSet::from([1, 2]);
        let view = project_actions_detail(&detail, &expanded, Some(1), geometry(10, 4));

        assert!(
            view.rows.len() > 10,
            "long job and step text must add display rows"
        );
        assert_eq!(view.focused_job_index, Some(1));
        assert!(view.content().contains('\u{2713}'));
        assert!(view.content().contains('\u{2717}'));
        assert!(view.rows.iter().all(|row| row.text.chars().count() <= 10));
    }

    #[test]
    fn final_job_reveal_uses_wrapped_predecessor_rows_and_valid_bounds() {
        let detail = detail();
        let expanded = HashSet::from([1, 2]);
        let view = project_actions_detail(&detail, &expanded, Some(2), geometry(9, 3));
        let offset = view.reveal_focused_job(0, 3);
        let focused = view.focused_job_row().unwrap_or_default();

        assert!(focused >= offset && focused < offset + 3);
        assert!(offset <= view.max_scroll_offset(3));
        assert!(
            focused > 6,
            "wrapped long job and step rows must shift the final job"
        );
    }

    #[test]
    fn zero_width_and_zero_viewport_keep_projection_and_bounds_total() {
        let detail = detail();
        let view = project_actions_detail(&detail, &HashSet::new(), Some(99), geometry(0, 0));

        assert_eq!(view.focused_job_index, Some(2));
        assert!(view.rows.iter().all(|row| row.text.is_empty()));
        assert!(view.reveal_focused_job(usize::MAX, 0) <= view.max_scroll_offset(0));
    }

    #[test]
    fn empty_jobs_normalize_focus_to_none() {
        let detail = WorkflowRunDetail {
            run: run(),
            jobs: Vec::new(),
        };
        let view = project_actions_detail(&detail, &HashSet::new(), Some(9), geometry(5, 2));

        assert_eq!(view.focused_job_index, None);
        assert_eq!(view.focused_job_row(), None);
        assert_eq!(view.rows.len(), 2);
    }
}
