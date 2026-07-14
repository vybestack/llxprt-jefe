//! Actions run-list pane projection for the generic [`SelectableList`] component.
//!
//! This module mirrors [`crate::ui::components::pr_list`]: the pure projection
//! ([`crate::actions_view::project_runs_list`]) lives in the view layer, and
//! [`actions_list_props`] maps the projected runs into [`SelectableRow`]s for
//! the shared [`SelectableList`] component. This replaces the hand-rolled
//! borders that caused workflow runs to "escape their box" — the shared
//! component owns border rendering, scroll windowing, width clamping, and
//! selection highlighting.

use crate::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus};
use crate::list_viewport::{ListGeometry, PaneRows, RowsPerItem};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;
use crate::ui::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
};

/// Actions run-list density variant.
///
/// In `Full` mode each run is a two-line row (title + meta). In `Compact`
/// mode each run is a single-line row (title only). Mirrors
/// [`crate::ui::components::pr_list::PrListLayout`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActionsListLayout {
    /// Show title and metadata for each run (2-line row).
    #[default]
    Full,
    /// Show only the title row for each run (1-line row).
    Compact,
}

impl ActionsListLayout {
    fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }
}

/// Windowing/geometry inputs for [`actions_list_props`].
///
/// Bundles the parameters the run-list projection needs to compute the visible
/// window. Mirrors [`crate::ui::components::pr_list::PrListWindow`].
#[derive(Clone, Copy, Debug)]
pub struct ActionsListWindow {
    /// Currently selected run index.
    pub selected_index: Option<usize>,
    /// Run-list pane height in rows (includes the title + border chrome).
    pub list_pane_rows: u16,
    /// Available content width (in terminal columns) for title truncation.
    pub available_width: Option<u16>,
    /// List density variant (compact rows omit the meta line).
    pub layout: ActionsListLayout,
}

/// Build the ASCII status glyph for a run.
///
/// Returns a short bracketed tag (`[OK]`, `[X]`, `[~]`, `[.]`, etc.). Uses
/// ASCII glyphs only — no pictographic emoji — for terminal-width stability.
fn run_status_glyph(
    status: WorkflowRunStatus,
    conclusion: Option<WorkflowRunConclusion>,
) -> &'static str {
    match status {
        WorkflowRunStatus::Completed => match conclusion {
            Some(WorkflowRunConclusion::Success) => "[OK]",
            Some(WorkflowRunConclusion::Failure) => "[X]",
            Some(WorkflowRunConclusion::Cancelled) => "[/]",
            _ => "[?]",
        },
        WorkflowRunStatus::InProgress => "[~]",
        WorkflowRunStatus::Queued
        | WorkflowRunStatus::Requested
        | WorkflowRunStatus::Waiting
        | WorkflowRunStatus::Pending => "[.]",
        WorkflowRunStatus::Unknown => "[?]",
    }
}

/// Map already-windowed [`ProjectedRun`](crate::actions_view::ProjectedRun)s
/// into [`SelectableRow`]s for the generic [`SelectableList`]. Each run becomes
/// a two-line row: a title line (selection prefix + status glyph + run name)
/// and a meta line (#number, workflow, branch, event, updated). In compact
/// mode the meta line is an empty string so the row renders single-line.
fn to_selectable_rows(
    view: &crate::actions_view::ActionsRunListView,
    compact: bool,
    available_width: Option<u16>,
) -> Vec<SelectableRow> {
    // Prefix ("> "/"  ") + space + glyph (4 chars) + space = 7 chars of chrome
    // before the run name. Reserve that much before truncating the title.
    const TITLE_CHROME: usize = 7;
    view.visible_runs
        .iter()
        .map(|run| {
            let prefix = if run.is_selected { "> " } else { "  " };
            let glyph = run_status_glyph(run.status, run.conclusion);
            let name = if let Some(width) = available_width {
                let budget = (usize::from(width)).saturating_sub(TITLE_CHROME).max(1);
                crate::ui::util::truncate_with_ellipsis(&run.name, budget)
            } else {
                run.name.clone()
            };
            let title_line = format!("{prefix}{glyph} {name}");
            let meta = if compact {
                String::new()
            } else {
                format!(
                    "     #{} wf:{} branch:{} event:{} updated:{}",
                    run.run_number, run.workflow_name, run.head_branch, run.event, run.updated_at
                )
            };
            SelectableRow {
                source_index: run.source_index,
                spans: vec![SelectableSpan {
                    text: title_line,
                    color: SpanColor::Themed,
                }],
                meta_line: Some(meta),
                is_selected: run.is_selected,
            }
        })
        .collect()
}

/// Loading/empty status line for the Actions run list. Returns `None` when
/// runs are shown (non-empty and not loading).
#[must_use]
pub fn actions_list_status_message(
    loading: bool,
    is_empty: bool,
    has_filters: bool,
) -> Option<&'static str> {
    if loading {
        Some("Loading workflow runs...")
    } else if is_empty {
        if has_filters {
            Some("No workflow runs match filters")
        } else {
            Some("No workflow runs found")
        }
    } else {
        None
    }
}

/// Build [`SelectableListProps`] for the Actions run-list pane.
///
/// Calls [`crate::actions_view::project_runs_list`] and maps each projected run
/// into a [`SelectableRow`]. The pane chrome (top border + title + bottom
/// border = 3 rows) is subtracted to get the inner viewport height. In Full
/// mode each run occupies 2 terminal rows (title + meta), so the run-budget is
/// half the row-budget; in Compact mode each run is 1 row. This ensures
/// scrolling reaches every run regardless of row height — no run is permanently
/// hidden below the fold.
#[must_use]
pub fn actions_list_props(
    runs: &[WorkflowRun],
    window: ActionsListWindow,
    focused: bool,
    empty_message: Option<&str>,
    colors: ThemeColors,
    selection: Option<TextSelection>,
) -> SelectableListProps {
    let rows_per_item = RowsPerItem::new(if window.layout.is_compact() { 1 } else { 2 });
    let geometry = ListGeometry::bordered(rows_per_item);
    let run_budget = geometry
        .item_capacity(PaneRows::new(usize::from(window.list_pane_rows)))
        .get();
    let list_view = crate::actions_view::project_runs_list(runs, window.selected_index, run_budget);
    SelectableListProps {
        title: "Workflow Runs".to_string(),
        rows: to_selectable_rows(
            &list_view,
            window.layout.is_compact(),
            window.available_width,
        ),
        focused,
        empty_message: empty_message.map(String::from),
        colors,
        selection,
        pane: SelectablePane::ActionsList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: window
            .available_width
            .map_or_else(|| usize::from(u16::MAX), usize::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{WorkflowRun, WorkflowRunConclusion, WorkflowRunStatus};
    use crate::theme::ThemeColors;

    fn make_run(
        id: u64,
        status: WorkflowRunStatus,
        conclusion: Option<WorkflowRunConclusion>,
    ) -> WorkflowRun {
        WorkflowRun {
            id,
            name: format!("Run {id}"),
            head_branch: "main".to_string(),
            head_sha: format!("sha{id}"),
            run_number: u32::try_from(id).unwrap_or_default(),
            event: "push".to_string(),
            status,
            conclusion,
            workflow_name: "CI".to_string(),
            created_at: "time".to_string(),
            updated_at: "time".to_string(),
        }
    }

    #[test]
    fn actions_list_props_empty_message_when_loading() {
        let msg = actions_list_status_message(true, false, false);
        assert_eq!(msg, Some("Loading workflow runs..."));
    }

    #[test]
    fn actions_list_props_empty_message_no_filters() {
        let msg = actions_list_status_message(false, true, false);
        assert_eq!(msg, Some("No workflow runs found"));
    }

    #[test]
    fn actions_list_props_empty_message_with_filters() {
        let msg = actions_list_status_message(false, true, true);
        assert_eq!(msg, Some("No workflow runs match filters"));
    }

    #[test]
    fn actions_list_props_empty_message_none_when_runs_exist() {
        let msg = actions_list_status_message(false, false, false);
        assert!(msg.is_none());
    }

    #[test]
    fn actions_list_props_projects_rows_with_status_glyph() {
        let runs = vec![
            make_run(
                1,
                WorkflowRunStatus::Completed,
                Some(WorkflowRunConclusion::Success),
            ),
            make_run(
                2,
                WorkflowRunStatus::Completed,
                Some(WorkflowRunConclusion::Failure),
            ),
        ];
        let window = ActionsListWindow {
            selected_index: Some(0),
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Full,
        };
        let props = actions_list_props(&runs, window, true, None, ThemeColors::default(), None);
        assert_eq!(props.rows.len(), 2);
        // First run is selected → prefix "> "
        assert!(props.rows[0].spans[0].text.starts_with("> "));
        assert!(props.rows[0].spans[0].text.contains("[OK]"));
        // Second run is not selected → prefix "  "
        assert!(props.rows[1].spans[0].text.starts_with("  "));
        assert!(props.rows[1].spans[0].text.contains("[X]"));
        // Meta line present (non-empty in Full mode)
        assert!(props.rows[0].meta_line.is_some());
        assert!(
            props.rows[0]
                .meta_line
                .as_deref()
                .unwrap_or("")
                .contains("#1")
        );
    }

    #[test]
    fn actions_list_props_pane_is_actions_list() {
        let runs: Vec<WorkflowRun> = Vec::new();
        let window = ActionsListWindow {
            selected_index: None,
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Full,
        };
        let props = actions_list_props(
            &runs,
            window,
            true,
            Some("empty"),
            ThemeColors::default(),
            None,
        );
        assert_eq!(props.pane, SelectablePane::ActionsList);
    }

    #[test]
    fn actions_list_props_title_is_workflow_runs() {
        let runs: Vec<WorkflowRun> = Vec::new();
        let window = ActionsListWindow {
            selected_index: None,
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Full,
        };
        let props = actions_list_props(&runs, window, true, None, ThemeColors::default(), None);
        assert_eq!(props.title, "Workflow Runs");
    }

    // ---- BUG 1: scrolling must reach every run (2-line rows) ----

    /// In Full mode each run is 2 terminal rows. With a pane that fits 10
    /// terminal rows (8 content after chrome), only 4 runs are visible at a
    /// time. Selecting the last run must scroll it into view — no run should
    /// be permanently hidden below the fold.
    #[test]
    fn full_mode_last_run_reachable_when_selected() {
        let runs: Vec<WorkflowRun> = (0..10)
            .map(|i| {
                make_run(
                    i,
                    WorkflowRunStatus::Completed,
                    Some(WorkflowRunConclusion::Success),
                )
            })
            .collect();
        // 10 terminal rows → 8 content rows → 4 runs per page (2-line rows).
        let window = ActionsListWindow {
            selected_index: Some(9), // last run
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Full,
        };
        let props = actions_list_props(&runs, window, true, None, ThemeColors::default(), None);
        // The last run (id 9) must be among the visible rows.
        assert!(
            props.rows.iter().any(|r| r.spans[0].text.contains("Run 9")),
            "last run must be visible when selected, rows: {:?}",
            props
                .rows
                .iter()
                .map(|r| &r.spans[0].text)
                .collect::<Vec<_>>()
        );
    }

    /// In Compact mode each run is 1 terminal row. With a pane that fits 10
    /// terminal rows (8 content), 8 runs are visible. Selecting the last run
    /// (index 9) must scroll it into view.
    #[test]
    fn compact_mode_last_run_reachable_when_selected() {
        let runs: Vec<WorkflowRun> = (0..10)
            .map(|i| {
                make_run(
                    i,
                    WorkflowRunStatus::Completed,
                    Some(WorkflowRunConclusion::Success),
                )
            })
            .collect();
        let window = ActionsListWindow {
            selected_index: Some(9),
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Compact,
        };
        let props = actions_list_props(&runs, window, true, None, ThemeColors::default(), None);
        assert!(
            props.rows.iter().any(|r| r.spans[0].text.contains("Run 9")),
            "last run must be visible when selected (compact)"
        );
        // Compact mode: meta_line is empty string (single-line row).
        assert!(
            props
                .rows
                .iter()
                .all(|r| r.meta_line.as_deref() == Some("")),
            "compact mode must produce empty meta_line for all rows"
        );
    }

    /// In Full mode, meta_line is non-empty (2-line row).
    #[test]
    fn full_mode_meta_line_is_non_empty() {
        let runs = vec![make_run(
            0,
            WorkflowRunStatus::Completed,
            Some(WorkflowRunConclusion::Success),
        )];
        let window = ActionsListWindow {
            selected_index: Some(0),
            list_pane_rows: 10,
            available_width: Some(60),
            layout: ActionsListLayout::Full,
        };
        let props = actions_list_props(&runs, window, true, None, ThemeColors::default(), None);
        assert!(
            props.rows[0]
                .meta_line
                .as_deref()
                .is_some_and(|m| !m.is_empty()),
            "full mode must produce non-empty meta_line"
        );
    }
}
