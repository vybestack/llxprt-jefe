//! Actions run-detail pane projection.
//!
//! Mirrors [`crate::ui::components::pr_detail`]: this module builds a
//! [`DetailPaneProps`] from the workflow run detail and delegates rendering to
//! the generic [`DetailPane`] via [`detail_pane_element`]. The heavy projection
//! (windowing, line building) lives in [`crate::actions_view`]; this module
//! handles the header-rows + content-string + viewport-math glue.

use crate::actions_detail_view::project_actions_detail;
use crate::domain::WorkflowRunDetail;
use crate::layout::ActionsDetailGeometry;
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;

use super::detail_pane::{DetailHeaderColor, DetailHeaderRow, DetailPaneProps};

/// Number of fixed metadata header rows for the Actions detail pane.
///
/// The header renders:
/// 1. `Workflow: <name>`,
/// 2. `Triggered by: <event> on branch <branch>`,
/// 3. `Commit SHA: <sha>`,
/// 4. `Created: <created> | Updated: <updated>`,
/// 5. a horizontal rule separator.
const ACTIONS_DETAIL_HEADER_ROWS: usize = 5;

/// Inputs the Actions screen passes to [`actions_detail_props`], bundled to
/// stay under the clippy::too_many-arguments threshold.
pub struct ActionsDetailProjectionInputs<'a> {
    /// Full run detail (run metadata + jobs/steps), or `None` if nothing is
    /// selected / detail is loading.
    pub detail: Option<&'a WorkflowRunDetail>,
    /// Scroll offset for the content viewport.
    pub scroll_offset: usize,
    /// Exact detail display geometry supplied by the shared layout helper.
    pub geometry: ActionsDetailGeometry,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any.
    pub selection: Option<TextSelection>,
    /// Focused job index within the loaded detail.
    pub focused_job_index: Option<usize>,
    /// Set of expanded job ids (collapsed = not in set).
    pub expanded_jobs: &'a std::collections::HashSet<u64>,
}

/// Build the five fixed header rows for a loaded run detail.
fn build_header_rows(detail: &WorkflowRunDetail) -> Vec<DetailHeaderRow> {
    let run = &detail.run;
    vec![
        DetailHeaderRow {
            content: format!("Workflow: {}", run.workflow_name),
            color: DetailHeaderColor::Fg,
            line: 0,
        },
        DetailHeaderRow {
            content: format!("Triggered by: {} on branch {}", run.event, run.head_branch),
            color: DetailHeaderColor::Dim,
            line: 1,
        },
        DetailHeaderRow {
            content: format!("Commit SHA: {}", run.head_sha),
            color: DetailHeaderColor::Dim,
            line: 2,
        },
        DetailHeaderRow {
            content: format!("Created: {} | Updated: {}", run.created_at, run.updated_at),
            color: DetailHeaderColor::Dim,
            line: 3,
        },
        DetailHeaderRow {
            content: "─────────────────────────────────────────".to_string(),
            color: DetailHeaderColor::Dim,
            line: 4,
        },
    ]
}

/// Pure projection of the Actions detail pane into a [`DetailPaneProps`].
///
/// The shared Actions projection supplies already-wrapped display rows, so the
/// generic renderer consumes the same row coordinates used by reducer bounds.
#[must_use]
pub fn actions_detail_props(inputs: ActionsDetailProjectionInputs<'_>) -> DetailPaneProps {
    let (header_rows, content) = if let Some(detail) = inputs.detail {
        let projection = project_actions_detail(
            detail,
            inputs.expanded_jobs,
            inputs.focused_job_index,
            inputs.geometry,
        );
        (build_header_rows(detail), projection.content())
    } else {
        (
            build_placeholder_header_rows(),
            "Select a workflow run to view details.".to_string(),
        )
    };

    DetailPaneProps {
        header_rows,
        content,
        content_cursor: None,
        scroll_offset: inputs.scroll_offset,
        viewport_rows: inputs.geometry.viewport_rows,
        content_line_offset: ACTIONS_DETAIL_HEADER_ROWS,
        max_line_width: inputs.geometry.content_width,
        focused: inputs.focused,
        pane: SelectablePane::ActionsDetail,
        colors: inputs.colors,
        selection: inputs.selection,
        composer: None,
        composer_rows: 0,
    }
}

/// Build empty placeholder header rows (for the "no run selected" state).
fn build_placeholder_header_rows() -> Vec<DetailHeaderRow> {
    (0..ACTIONS_DETAIL_HEADER_ROWS)
        .map(|i| DetailHeaderRow {
            content: String::new(),
            color: DetailHeaderColor::Dim,
            line: i,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        WorkflowRun, WorkflowRunConclusion, WorkflowRunDetail, WorkflowRunJob, WorkflowRunStatus,
        WorkflowRunStep,
    };
    use crate::theme::ThemeColors;

    fn make_detail() -> WorkflowRunDetail {
        WorkflowRunDetail {
            run: WorkflowRun {
                id: 1,
                name: "Run 1".to_string(),
                head_branch: "main".to_string(),
                head_sha: "abc123".to_string(),
                run_number: 1,
                event: "push".to_string(),
                status: WorkflowRunStatus::Completed,
                conclusion: Some(WorkflowRunConclusion::Success),
                workflow_name: "CI".to_string(),
                created_at: "2026-01-01".to_string(),
                updated_at: "2026-01-02".to_string(),
            },
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
        }
    }

    fn expanded_all() -> std::collections::HashSet<u64> {
        std::iter::once(0u64).collect()
    }

    fn expanded_none() -> std::collections::HashSet<u64> {
        std::collections::HashSet::new()
    }

    #[test]
    fn actions_detail_props_with_detail_builds_header_and_content() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: true,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert_eq!(props.header_rows.len(), 5);
        assert!(props.header_rows[0].content.contains("CI"));
        assert!(props.content.contains("Jobs:"));
        assert!(props.content.contains("build"));
        assert!(props.content.contains("checkout"));
        assert_eq!(props.pane, SelectablePane::ActionsDetail);
        assert!(props.focused);
    }

    #[test]
    fn actions_detail_props_no_detail_shows_placeholder() {
        let inputs = ActionsDetailProjectionInputs {
            detail: None,
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_none(),
        };
        let props = actions_detail_props(inputs);
        assert_eq!(props.header_rows.len(), 5);
        assert_eq!(props.content, "Select a workflow run to view details.");
    }

    #[test]
    fn actions_detail_props_content_line_offset_matches_header_count() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert_eq!(props.content_line_offset, ACTIONS_DETAIL_HEADER_ROWS);
    }

    // ---- BUG 4: Leading status glyphs on job/step lines ----

    #[test]
    fn job_row_renders_leading_success_glyph() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("\u{2713} build"),
            "job row must start with success glyph + name, got: {}",
            props.content
        );
        assert!(
            !props.content.contains("(success)"),
            "trailing status text must be removed from job rows"
        );
    }

    #[test]
    fn step_row_renders_leading_success_glyph() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("  \u{2713} [1] checkout"),
            "step row must start with indent + success glyph + [num] + name, got: {}",
            props.content
        );
        assert!(
            !props.content.contains("(success)"),
            "trailing status text must be removed from step rows"
        );
    }

    #[test]
    fn job_row_renders_failure_glyph() {
        let mut detail = make_detail();
        detail.jobs[0].conclusion = Some(WorkflowRunConclusion::Failure);
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("\u{2717} build"),
            "failed job must start with failure glyph, got: {}",
            props.content
        );
    }

    // ---- BUG 5: expand/collapse indicator ----

    #[test]
    fn collapsed_job_renders_triangle_right_indicator() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_none(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("\u{25B8}"),
            "collapsed job must show right-pointing triangle, got: {}",
            props.content
        );
        assert!(
            !props.content.contains("checkout"),
            "collapsed job must not show steps"
        );
    }

    #[test]
    fn expanded_job_renders_triangle_down_indicator() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: false,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("\u{25BE}"),
            "expanded job must show down-pointing triangle, got: {}",
            props.content
        );
        assert!(
            props.content.contains("checkout"),
            "expanded job must show steps"
        );
    }

    #[test]
    fn focused_job_renders_stable_marker_and_steps_keep_status_glyphs() {
        let mut detail = make_detail();
        detail.jobs[0].steps[0].conclusion = Some(WorkflowRunConclusion::Failure);
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            geometry: ActionsDetailGeometry {
                viewport_rows: 13,
                content_width: 80,
            },
            focused: true,
            colors: ThemeColors::default(),
            selection: None,
            focused_job_index: Some(0),
            expanded_jobs: &expanded_all(),
        };

        let props = actions_detail_props(inputs);

        assert!(props.content.contains("> \u{25BE} \u{2713} build"));
        assert!(props.content.contains("  \u{2717} [1] checkout"));
    }
}
