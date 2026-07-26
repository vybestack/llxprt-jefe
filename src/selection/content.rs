//! Pane content providers for mouse selection.
//!
//! Each [`crate::selection::SelectablePane`] renders some text; to copy a
//! selection we need that text as a flat `Vec<String>` of content lines. This
//! module owns the pure mapping from [`crate::state::AppState`] data to those
//! lines, reusing the existing pure projection builders so the copyable text
//! matches what the user sees:
//! - [`crate::issue_detail_content::build_detail_content`] /
//!   [`crate::pr_detail_content::build_pr_detail_content`] for detail panes,
//! - [`crate::ui::components::pr_list::pr_list_visible_rows`] /
//!   [`crate::ui::components::issue_list::issue_list_visible_rows`] for list
//!   panes (these are pure functions even though they live next to iocraft
//!   components; importing them keeps a single source of truth for the rendered
//!   row text so selection coordinates map to the exact characters on screen).
//!
//! All functions are pure and `#[must_use]`. The terminal snapshot is passed in
//! explicitly (it lives on the runtime, not AppState) so the module stays
//! iocraft-free and side-effect-free.

use crate::domain::AgentStatus;
use crate::runtime::TerminalSnapshot;
use crate::selection::SelectablePane;
use crate::state::{AppState, DashboardGrabPane, ScreenMode};
use crate::ui::components::issue_list::{
    IssueListLayout, IssueListWindow, issue_list_props, issue_list_status_message,
};
use crate::ui::components::pr_list::{
    PrListLayout, PrListWindow, pr_list_props, pr_list_status_message,
};
use crate::ui::components::selectable_list::{ProjectedContentLine, projected_content_lines};
use crate::ui::components::{SidebarProps, sidebar_list_props, terminal_empty_message};
use crate::ui::modals::help_content_lines;

use super::projection_context::PaneContentContext;
use crate::selection::form_content;
use crate::selection::overlay_content;

/// The copyable text content of a pane, as a flat list of lines.
///
/// Built by [`pane_content_lines`] from the active app state. Line order and
/// contents mirror what the renderer paints so a selection coordinate maps to
/// the same characters the user sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneContent {
    /// The pane these lines belong to.
    pub pane: SelectablePane,
    /// Content lines (no trailing newlines per line).
    pub lines: Vec<String>,
}

impl PaneContent {
    /// Construct content for `pane` from an iterator of line strings.
    #[must_use]
    pub fn new<I: IntoIterator<Item = String>>(pane: SelectablePane, lines: I) -> Self {
        Self {
            pane,
            lines: lines.into_iter().collect(),
        }
    }

    /// Empty content for `pane` (no copyable lines).
    #[must_use]
    pub fn empty(pane: SelectablePane) -> Self {
        Self {
            pane,
            lines: Vec::new(),
        }
    }
}

/// Build the copyable content lines for `pane` from the app state.
///
/// Returns [`PaneContent::empty`] when the pane has no content (e.g. no
/// selected issue/PR, no agent). The terminal snapshot is passed separately
/// because it lives on the runtime, not AppState. `term_cols`/`term_rows` are
/// the live terminal size, used to compute the same pane widths/heights the
/// screens render with so list-row truncation matches.
///
/// # Panics
///
/// Never; all indexing is bounds-checked.
#[must_use]
pub fn pane_content_lines(
    pane: SelectablePane,
    state: &AppState,
    snapshot: Option<&TerminalSnapshot>,
    history_lines: &[String],
    term_cols: u16,
    term_rows: u16,
) -> PaneContent {
    pane_content_lines_with_context(
        pane,
        state,
        &PaneContentContext {
            terminal_snapshot: snapshot,
            history_lines,
            term_cols,
            term_rows,
            dashboard_git_info: None,
        },
    )
}

/// Build copyable pane content from explicit, already-resolved runtime inputs.
#[must_use]
pub fn pane_content_lines_with_context(
    pane: SelectablePane,
    state: &AppState,
    context: &PaneContentContext<'_>,
) -> PaneContent {
    let (render_cols, render_rows) =
        crate::layout::effective_render_size(context.term_cols, context.term_rows);
    match pane {
        SelectablePane::IssueDetail => issue_detail_lines(state),
        SelectablePane::PrDetail => pr_detail_lines(state),
        SelectablePane::IssueList => issue_list_lines(state, render_cols, render_rows),
        SelectablePane::PrList => pr_list_lines(state, render_cols, render_rows),
        SelectablePane::ActionsList => {
            super::actions_content::actions_list_lines(state, render_cols, render_rows)
        }
        SelectablePane::ActionsDetail => {
            super::actions_content::actions_detail_lines(state, render_cols, render_rows)
        }
        SelectablePane::ErrorList => super::errors_content::error_list_lines(state, render_cols),
        SelectablePane::ErrorDetail => super::errors_content::error_detail_lines(state),
        SelectablePane::Sidebar => sidebar_lines(state, render_cols, render_rows),
        SelectablePane::AgentList => super::dashboard_content::agent_list_lines(
            state,
            render_cols,
            render_rows,
            context.dashboard_git_info,
        ),
        SelectablePane::Preview => {
            super::dashboard_content::preview_lines(state, context.dashboard_git_info)
        }
        SelectablePane::TerminalView => {
            terminal_lines(context.terminal_snapshot, state, context.history_lines)
        }
        SelectablePane::HelpModal => help_lines(),
        SelectablePane::StatusBar => status_bar_lines(state),
        SelectablePane::KeybindBar => keybind_bar_lines(state),
        SelectablePane::AgentForm => agent_form_lines(state),
        SelectablePane::RepositoryForm => repository_form_lines(state),
        SelectablePane::AgentChooser => overlay_content::agent_chooser_lines(state),
        SelectablePane::MergeChooser => overlay_content::merge_chooser_lines(state),
        SelectablePane::PropertyEditor => overlay_content::property_editor_lines(state),
        SelectablePane::CloseReasonChooser => overlay_content::close_reason_chooser_lines(state),
        SelectablePane::IssueDeleteConfirm => overlay_content::issue_delete_confirm_lines(state),
        SelectablePane::ConfirmModal => overlay_content::confirm_modal_lines(state),
    }
}

fn issue_detail_lines(state: &AppState) -> PaneContent {
    let props = crate::ui::components::issue_detail::issue_detail_props(
        crate::ui::components::issue_detail::IssueDetailProjectionInputs {
            issue_detail: state.issues_state.issue_detail.as_ref(),
            detail_subfocus: state.issues_state.detail_subfocus,
            inline_state: &state.issues_state.inline_state,
            comments_loading: state.issues_state.loading.comments,
            focused: false,
            scroll_offset: state.issues_state.detail_scroll_offset,
            colors: crate::theme::ThemeColors::default(),
            available_height: None,
            available_width: None,
            selection: None,
        },
    );
    detail_props_content(SelectablePane::IssueDetail, props)
}

fn pr_detail_lines(state: &AppState) -> PaneContent {
    let props = crate::ui::components::pr_detail::pr_detail_props(
        crate::ui::components::pr_detail::PrDetailProjectionInputs {
            detail: state.prs_state.pr_detail.as_ref(),
            subfocus: state.prs_state.detail_subfocus,
            scroll_offset: state.prs_state.detail_scroll_offset,
            viewport_rows: None,
            detail_loading: state.prs_state.loading.detail,
            comments_loading: state.prs_state.loading.comments,
            focused: false,
            inline_state: &state.prs_state.inline_state,
            detail_content_width: 80,
            colors: crate::theme::ThemeColors::default(),
            selection: None,
        },
    );
    detail_props_content(SelectablePane::PrDetail, props)
}

fn detail_props_content(
    pane: SelectablePane,
    props: crate::ui::components::detail_pane::DetailPaneProps,
) -> PaneContent {
    let mut lines = props
        .header_rows
        .into_iter()
        .map(|row| row.content)
        .collect::<Vec<_>>();
    lines.extend(props.content.lines().map(String::from));
    PaneContent::new(pane, lines)
}

/// Issue list lines that match the rendered Compact-mode projection exactly
/// (prefix + `#number` + truncated title, one line per issue).
fn issue_list_lines(state: &AppState, term_cols: u16, term_rows: u16) -> PaneContent {
    // Use the shared banner projection so the selection window matches the
    // rendered pane sizing — a notice-only banner reserves the same row as
    // an error banner (issue #265 second review).
    let banner_visible = crate::layout::issues_banner_visible(
        state.issues_state.error.as_deref(),
        state.issues_state.draft_notice.as_deref(),
    );
    let (list_pane_rows, _) = crate::layout::issues_pane_rows(
        usize::from(term_rows),
        banner_visible,
        state.issues_state.filter_ui.controls_open,
    );
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let issues = state.issues_state.issues();
    let props = issue_list_props(
        issues,
        IssueListWindow {
            selected_index: state.issues_state.selected_issue_index(),
            list_pane_rows,
            layout: IssueListLayout::Compact,
            available_width: Some(crate::layout::issue_list_content_width(term_cols)),
        },
        false,
        issue_list_status_message(
            state.issues_state.list_loading(),
            issues.is_empty(),
            state
                .issues_state
                .committed_filter
                .has_active_non_default_filters(),
        ),
        crate::theme::ThemeColors::default(),
        None,
    );
    projected_list_content(SelectablePane::IssueList, projected_content_lines(&props))
}

/// PR list lines that match the rendered Compact-mode projection exactly
/// (prefix + `#number` + truncated title, one line per PR).
fn pr_list_lines(state: &AppState, term_cols: u16, term_rows: u16) -> PaneContent {
    let (list_pane_rows, _) = crate::layout::prs_pane_rows(
        usize::from(term_rows),
        state.prs_state.error.is_some(),
        state.prs_state.filter_ui.controls_open,
    );
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let pull_requests = state.prs_state.pull_requests();
    let filter = &state.prs_state.committed_filter;
    let has_filters = filter.state.is_some()
        || !filter.author.is_empty()
        || !filter.assignee.is_empty()
        || !filter.reviewer.is_empty()
        || !filter.labels.is_empty()
        || !filter.query_text.is_empty()
        || filter.is_draft.is_some()
        || filter.review_decision != crate::domain::ReviewDecisionFilter::Any
        || filter.checks_status != crate::domain::ChecksFilter::Any;
    let props = pr_list_props(
        pull_requests,
        PrListWindow {
            selected_index: state.prs_state.selected_pr_index(),
            list_pane_rows,
            available_width: Some(crate::layout::pr_list_content_width(term_cols)),
            layout: PrListLayout::Compact,
        },
        false,
        pr_list_status_message(
            state.prs_state.list_loading(),
            pull_requests.is_empty(),
            has_filters,
        ),
        crate::theme::ThemeColors::default(),
        None,
    );
    projected_list_content(SelectablePane::PrList, projected_content_lines(&props))
}

fn projected_list_content(pane: SelectablePane, lines: Vec<ProjectedContentLine>) -> PaneContent {
    PaneContent::new(pane, lines.into_iter().map(|line| line.text))
}

fn sidebar_lines(state: &AppState, render_cols: u16, render_rows: u16) -> PaneContent {
    let visible_indices = state.visible_repository_indices();
    let repositories: Vec<_> = visible_indices
        .iter()
        .filter_map(|index| state.repositories.get(*index).cloned())
        .collect();
    let counts = repositories
        .iter()
        .map(|repo| state.visible_agent_count_for_repository(&repo.id))
        .collect();
    let (pane_rows, content_width) = if state.screen_mode == crate::state::ScreenMode::Split {
        let layout = crate::layout::split_layout_for_render_size(render_cols, render_rows);
        (layout.sidebar_rows, layout.sidebar_content_cols)
    } else {
        (
            render_rows.saturating_sub(crate::layout::OUTER_BARS_HEIGHT),
            crate::list_viewport::bordered_padded_content_width(crate::layout::LEFT_COL_WIDTH),
        )
    };
    let props = sidebar_list_props(&SidebarProps {
        repositories,
        agent_counts: counts,
        selected: state.selected_repository_visible_index().unwrap_or(0),
        grabbed: if state.screen_mode == crate::state::ScreenMode::Split {
            state.split_grab_index
        } else {
            state.dashboard_grab.as_ref().and_then(|grab| match grab {
                DashboardGrabPane::Repository { visible_index } => Some(*visible_index),
                DashboardGrabPane::Agent { .. } => None,
            })
        },
        pane_rows,
        content_width,
        ..SidebarProps::default()
    });
    projected_list_content(SelectablePane::Sidebar, projected_content_lines(&props))
}

fn terminal_lines(
    snapshot: Option<&TerminalSnapshot>,
    state: &AppState,
    history_lines: &[String],
) -> PaneContent {
    // Issue #198: include retained history lines above the live snapshot rows
    // so selection coordinates map to the scrolled viewport content.
    // Issue #197: filter wide-character spacers so selection text matches the
    // rendered grid (each wide cell occupies 2 columns; the trailing spacer
    // is not a real glyph).
    let live_lines: Vec<String> = snapshot.map_or_else(Vec::new, |snap| {
        (0..snap.rows)
            .map(|row| {
                snap.cells.get(row).map_or_else(String::new, |cells| {
                    cells
                        .iter()
                        .take(snap.cols)
                        .filter(|c| !c.wide_spacer)
                        .map(|c| c.ch)
                        .collect()
                })
            })
            .collect()
    });

    if history_lines.is_empty() && live_lines.is_empty() {
        // A Running selected agent has a live session even before the viewer
        // finishes attaching; mirror the TerminalView empty-state copy so a
        // healthy session is not mistaken for a lost one (issue #160).
        let session_live = state
            .selected_agent()
            .is_some_and(|agent| agent.status == AgentStatus::Running);
        return PaneContent::new(
            SelectablePane::TerminalView,
            vec![terminal_empty_message(session_live).to_string()],
        );
    }

    // Build the combined history+live vector once with a single allocation.
    let mut all_lines: Vec<String> = Vec::with_capacity(history_lines.len() + live_lines.len());
    all_lines.extend_from_slice(history_lines);
    all_lines.extend(live_lines);
    PaneContent::new(SelectablePane::TerminalView, all_lines)
}

fn help_lines() -> PaneContent {
    // Issue #178: project the actual help content instead of an empty Vec so
    // select-to-copy works inside the help modal. Reuses the single source of
    // truth (`help_content_lines`) that the renderer windows. The title row
    // and its trailing blank are included as content lines 0-1 so the (2,2)
    // content origin maps to the title text.
    let mut lines: Vec<String> = vec![crate::ui::modals::HELP_TITLE.to_string(), String::new()];
    lines.extend(help_content_lines().iter().copied().map(str::to_string));
    PaneContent::new(SelectablePane::HelpModal, lines)
}

/// Status bar line that matches the rendered format:
/// `LLxprt Jefe - {version}   {repos} repos | {running}/{total} running   {theme}`.
///
/// The theme name is not in AppState (it is a screen prop), so the right-hand
/// segment is omitted here; the left/center segments — where selection is most
/// likely — match exactly.
fn status_bar_lines(state: &AppState) -> PaneContent {
    let repo_count = state.visible_repository_indices().len();
    let running = state.agents.iter().filter(|a| a.is_running()).count();
    let agent_count = state.visible_agent_count();
    let kennel_suffix = state
        .selected_agent()
        .filter(|agent| agent.agent_kind.is_kennel())
        .map_or("", |_| " (Kennel mode)");
    let left = format!("LLxprt Jefe{kennel_suffix} - {}", crate::VERSION);
    let center = format!("{repo_count} repos | {running}/{agent_count} running");
    let line = format!("{left}   {center}");
    PaneContent::new(SelectablePane::StatusBar, vec![line])
}

/// Keybind bar line that matches the rendered hint text for the active screen
/// mode, reusing the pure [`keybind_hints_for`] projection. The right-aligned
/// process-identity label (pid + commit) is appended so mouse-selection copy
/// captures it (issue #223).
fn keybind_bar_lines(state: &AppState) -> PaneContent {
    let actions_focus =
        (state.screen_mode == ScreenMode::DashboardActions).then_some(state.actions_state.focus);
    let hints = crate::ui::components::keybind_bar::keybind_hints_for(
        state.screen_mode,
        false,
        actions_focus,
    );
    let identity = crate::process_identity_label(std::process::id(), crate::GIT_COMMIT);
    // The rendered bar uses SpaceBetween so the identity sits on the far
    // right. For the flat selection text, append it with a separator so the
    // full row is captureable in one string.
    let line = format!("{hints}   {identity}");
    PaneContent::new(SelectablePane::KeybindBar, vec![line])
}

// ── Issue #178: overlay content providers (delegated to submodules) ──────

/// Agent-definition form lines that match the rendered field layout.
fn agent_form_lines(state: &AppState) -> PaneContent {
    match form_content::agent_form_content_lines(state) {
        Some(lines) => PaneContent::new(SelectablePane::AgentForm, lines),
        None => PaneContent::empty(SelectablePane::AgentForm),
    }
}

/// Repository-definition form lines that match the rendered field layout.
fn repository_form_lines(state: &AppState) -> PaneContent {
    match form_content::repository_form_content_lines(state) {
        Some(lines) => PaneContent::new(SelectablePane::RepositoryForm, lines),
        None => PaneContent::empty(SelectablePane::RepositoryForm),
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "content_notice_tests.rs"]
mod content_notice_tests;
