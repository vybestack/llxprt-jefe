//! Actions run-detail pane projection.
//!
//! Mirrors [`crate::ui::components::pr_detail`]: this module builds a
//! [`DetailPaneProps`] from the workflow run detail and delegates rendering to
//! the generic [`DetailPane`] via [`detail_pane_element`]. The heavy projection
//! (windowing, line building) lives in [`crate::actions_view`]; this module
//! handles the header-rows + content-string + viewport-math glue.

use crate::actions_view::{DetailLine, project_detail_view};
use crate::domain::WorkflowRunDetail;
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
    /// Detail pane height in rows, supplied by the screen.
    pub viewport_rows: Option<u16>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Content width (terminal cols) for text wrapping.
    pub content_width: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any.
    pub selection: Option<TextSelection>,
    /// Set of expanded job ids (collapsed = not in set).
    pub expanded_jobs: &'a std::collections::HashSet<u64>,
}

/// Map a job/step status+conclusion to a leading glyph.
///
/// Uses non-pictographic, width-stable symbols (NOT emoji):
/// - `\u{2713}` (check mark) success
/// - `\u{2717}` (ballot X) failure
/// - `\u{2298}` (circled division) cancelled / skipped
/// - `~` in-progress
/// - `.` queued / pending
/// - `?` unknown
fn status_glyph(
    status: crate::domain::WorkflowRunStatus,
    conclusion: Option<crate::domain::WorkflowRunConclusion>,
) -> &'static str {
    match status {
        crate::domain::WorkflowRunStatus::Completed => match conclusion {
            Some(crate::domain::WorkflowRunConclusion::Success) => "\u{2713}",
            Some(crate::domain::WorkflowRunConclusion::Failure) => "\u{2717}",
            Some(
                crate::domain::WorkflowRunConclusion::Cancelled
                | crate::domain::WorkflowRunConclusion::Skipped,
            ) => "\u{2298}",
            _ => "?",
        },
        crate::domain::WorkflowRunStatus::InProgress => "~",
        crate::domain::WorkflowRunStatus::Queued
        | crate::domain::WorkflowRunStatus::Requested
        | crate::domain::WorkflowRunStatus::Waiting
        | crate::domain::WorkflowRunStatus::Pending => ".",
        crate::domain::WorkflowRunStatus::Unknown => "?",
    }
}

/// Render a [`DetailLine`] to its plain-text representation for the content
/// string. Each line becomes one row in the scrollable viewport. Job and step
/// lines use a leading status glyph instead of trailing text.
fn line_text(line: &DetailLine) -> String {
    match line {
        DetailLine::Header {
            workflow_name,
            event,
            head_branch,
            head_sha,
            created_at,
            updated_at,
        } => {
            let _ = (
                workflow_name,
                event,
                head_branch,
                head_sha,
                created_at,
                updated_at,
            );
            String::new()
        }
        DetailLine::SectionTitle { title } => title.clone(),
        DetailLine::JobRow {
            name,
            status,
            conclusion,
            expanded,
            ..
        } => {
            let glyph = status_glyph(*status, *conclusion);
            let indicator = if *expanded { "\u{25BE}" } else { "\u{25B8}" };
            format!("{indicator} {glyph} {name}")
        }
        DetailLine::StepRow {
            number,
            name,
            status,
            conclusion,
        } => {
            let glyph = status_glyph(*status, *conclusion);
            format!("  {glyph} [{number}] {name}")
        }
    }
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

/// Compute the scrollable viewport rows from the supplied pane height.
///
/// Mirrors the PR detail viewport math: subtract header rows + 2 (border +
/// separator). Falls back to a default terminal height when none is supplied.
fn detail_viewport_rows(available_height: Option<u16>) -> usize {
    const DEFAULT_TERM_ROWS: usize = 40;
    if let Some(height) = available_height {
        (usize::from(height)).saturating_sub(ACTIONS_DETAIL_HEADER_ROWS + 2)
    } else {
        crate::layout::prs_detail_viewport_rows(DEFAULT_TERM_ROWS, false, false)
    }
}

/// Pure projection of the Actions detail pane into a [`DetailPaneProps`].
///
/// Builds header rows from the run metadata, flattens jobs/steps into a plain
/// content string (newline-joined), and computes the scroll viewport rows.
/// When no detail is loaded, a placeholder is shown.
#[must_use]
pub fn actions_detail_props(inputs: ActionsDetailProjectionInputs<'_>) -> DetailPaneProps {
    let scroll_rows = detail_viewport_rows(inputs.viewport_rows);

    let (header_rows, content) = if let Some(detail) = inputs.detail {
        let rows = build_header_rows(detail);
        let view = project_detail_view(
            detail,
            inputs.scroll_offset,
            scroll_rows,
            inputs.expanded_jobs,
        );
        let text = view
            .visible_lines
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n");
        (rows, text)
    } else {
        let rows = build_placeholder_header_rows();
        (rows, "Select a workflow run to view details.".to_string())
    };

    DetailPaneProps {
        header_rows,
        content,
        content_cursor: None,
        scroll_offset: inputs.scroll_offset,
        viewport_rows: scroll_rows,
        content_line_offset: ACTIONS_DETAIL_HEADER_ROWS,
        max_line_width: inputs.content_width,
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
            viewport_rows: Some(20),
            focused: true,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
            expanded_jobs: &expanded_all(),
        };
        let props = actions_detail_props(inputs);
        assert!(
            props.content.contains("\u{2717} build"),
            "failed job must start with failure glyph, got: {}",
            props.content
        );
    }

    #[test]
    fn status_glyph_maps_all_conclusions() {
        use crate::domain::WorkflowRunStatus as S;
        assert_eq!(
            status_glyph(S::Completed, Some(WorkflowRunConclusion::Success)),
            "\u{2713}"
        );
        assert_eq!(
            status_glyph(S::Completed, Some(WorkflowRunConclusion::Failure)),
            "\u{2717}"
        );
        assert_eq!(
            status_glyph(S::Completed, Some(WorkflowRunConclusion::Cancelled)),
            "\u{2298}"
        );
        assert_eq!(
            status_glyph(S::Completed, Some(WorkflowRunConclusion::Skipped)),
            "\u{2298}"
        );
        assert_eq!(status_glyph(S::InProgress, None), "~");
        assert_eq!(status_glyph(S::Queued, None), ".");
        assert_eq!(status_glyph(S::Pending, None), ".");
        assert_eq!(status_glyph(S::Unknown, None), "?");
    }

    // ---- BUG 5: expand/collapse indicator ----

    #[test]
    fn collapsed_job_renders_triangle_right_indicator() {
        let detail = make_detail();
        let inputs = ActionsDetailProjectionInputs {
            detail: Some(&detail),
            scroll_offset: 0,
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
            viewport_rows: Some(20),
            focused: false,
            content_width: 80,
            colors: ThemeColors::default(),
            selection: None,
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
}
