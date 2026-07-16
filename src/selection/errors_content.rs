//! Errors pane content projections used by mouse selection and copy (issue #292).

use crate::selection::SelectablePane;
use crate::state::AppState;

use super::content::PaneContent;

/// Build the error list pane content lines (one per stored error).
#[must_use]
pub fn error_list_lines(state: &AppState, render_cols: u16) -> PaneContent {
    let content_width = crate::layout::pr_list_content_width(render_cols) as usize;
    let lines: Vec<String> = state
        .errors_state
        .errors
        .iter()
        .map(|entry| {
            let prefix = format!("[{}] ", entry.seq);
            let remaining = content_width.saturating_sub(prefix.chars().count());
            let title = if entry.title.chars().count() > remaining {
                let truncated: String = entry
                    .title
                    .chars()
                    .take(remaining.saturating_sub(1))
                    .collect();
                format!("{prefix}{truncated}…")
            } else {
                format!("{prefix}{}", entry.title)
            };
            crate::list_viewport::fit_text_to_width(&title, content_width)
        })
        .collect();
    if lines.is_empty() {
        return PaneContent::new(
            SelectablePane::ErrorList,
            vec!["No errors recorded.".to_string()],
        );
    }
    PaneContent::new(SelectablePane::ErrorList, lines)
}

/// Build the error detail pane content lines (header + detail body).
#[must_use]
pub fn error_detail_lines(state: &AppState) -> PaneContent {
    let Some(entry) = state.errors_state.selected_error() else {
        return PaneContent::new(
            SelectablePane::ErrorDetail,
            vec!["Select an error to view details.".to_string()],
        );
    };
    let source_label = match entry.source {
        crate::domain::ErrorSource::Issues => "Issues",
        crate::domain::ErrorSource::PullRequests => "PRs",
        crate::domain::ErrorSource::Actions => "Actions",
        crate::domain::ErrorSource::Persistence => "Persistence",
        crate::domain::ErrorSource::Agent => "Agent",
        crate::domain::ErrorSource::Startup => "Startup",
        crate::domain::ErrorSource::Other => "Other",
    };
    let mut lines = vec![
        format!("[{}] {}", entry.seq, entry.title),
        format!("Source: {source_label}  ·  {}", entry.timestamp),
        crate::ui::components::SEPARATOR_LINE.to_string(),
    ];
    lines.extend(entry.detail.lines().map(String::from));
    PaneContent::new(SelectablePane::ErrorDetail, lines)
}
