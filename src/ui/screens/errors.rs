//! Errors mode screen — two-column layout: repos sidebar + errors workspace.
//!
//! Mirrors the Actions/PRs/Issues screen patterns: a fixed sidebar + a list
//! pane (top split) + a detail pane (bottom split). The error log is purely
//! local (no remote fetching), so all data is eager-loaded from
//! [`crate::state::ErrorsState`].

use iocraft::prelude::*;

use crate::selection::SelectablePane;
use crate::state::{AppState, ErrorsFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::detail_pane::{DetailHeaderColor, DetailHeaderRow, DetailPaneProps};
use super::super::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
};
use super::super::components::{
    KeybindBar, Sidebar, StatusBar, detail_pane_element, selectable_list_element,
};

/// Props for the errors mode screen.
#[derive(Default, Props)]
pub struct ErrorsScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Errors mode screen layout — two-column: repos sidebar + errors workspace.
#[component]
pub fn ErrorsScreen(props: &ErrorsScreenProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));
    let selection = state.and_then(|s| s.selection);

    // ── Sidebar data ────────────────────────────────────────────────────────
    let selected_repo_idx = state
        .and_then(AppState::selected_repository_visible_index)
        .unwrap_or(0);
    let visible_repo_indices = state.map_or_else(Vec::new, AppState::visible_repository_indices);
    let repositories: Vec<_> = state.map_or_else(Vec::new, |s| {
        visible_repo_indices
            .iter()
            .filter_map(|idx| s.repositories.get(*idx).cloned())
            .collect()
    });
    let agent_counts: Vec<usize> = state.map_or_else(Vec::new, |s| {
        visible_repo_indices
            .iter()
            .filter_map(|idx| {
                s.repositories
                    .get(*idx)
                    .map(|repo| s.visible_agent_count_for_repository(&repo.id))
            })
            .collect()
    });

    // ── Status bar data ─────────────────────────────────────────────────────
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);

    // ── Errors data ─────────────────────────────────────────────────────────
    let errors_focus = state.map_or(ErrorsFocus::ErrorList, |s| s.errors_state.focus);
    let errors: Vec<_> = state.map_or_else(Vec::new, |s| s.errors_state.errors.clone());
    let selected_error_idx = state.and_then(|s| s.errors_state.selected_index);
    let detail_scroll_offset = state.map_or(0, |s| s.errors_state.detail_scroll_offset);
    let selected_error = selected_error_idx.and_then(|idx| errors.get(idx));

    // ── Layout geometry ─────────────────────────────────────────────────────
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (render_cols, render_rows) = crate::layout::effective_render_size(term_cols, term_rows);
    // Errors mode has no error banner and no filter band.
    let (list_pane_rows, _) =
        crate::layout::actions_pane_rows(usize::from(render_rows), false, false);
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let sidebar_width = u32::from(crate::layout::prs_main_columns(render_cols).sidebar_width);
    let list_content_width = crate::layout::pr_list_content_width(render_cols);
    let detail_viewport_rows =
        crate::layout::prs_detail_viewport_rows(usize::from(render_rows), false, false);
    let detail_content_width = usize::from(crate::layout::prs_detail_content_width(render_cols));

    let sidebar_focused = errors_focus == ErrorsFocus::RepoList;
    let list_focused = errors_focus == ErrorsFocus::ErrorList;
    let detail_focused = errors_focus == ErrorsFocus::ErrorDetail;

    // ── Error list rows ─────────────────────────────────────────────────────
    let list_rows: Vec<SelectableRow> = errors
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let prefix = format!("[{}] ", entry.seq);
            let remaining = (list_content_width as usize).saturating_sub(prefix.chars().count());
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
            let source_label = match entry.source {
                crate::domain::ErrorSource::Issues => "Issues",
                crate::domain::ErrorSource::PullRequests => "PRs",
                crate::domain::ErrorSource::Actions => "Actions",
                crate::domain::ErrorSource::Persistence => "Persistence",
                crate::domain::ErrorSource::Agent => "Agent",
                crate::domain::ErrorSource::Startup => "Startup",
                crate::domain::ErrorSource::Other => "Other",
            };
            let meta = format!("{source_label} · {}", entry.timestamp);
            SelectableRow {
                source_index: idx,
                spans: vec![SelectableSpan {
                    text: title,
                    color: SpanColor::Themed,
                }],
                meta_line: Some(meta),
                is_selected: selected_error_idx == Some(idx),
            }
        })
        .collect();

    let empty_message = if errors.is_empty() {
        Some("No errors recorded.".to_string())
    } else {
        None
    };

    // ── Error detail ────────────────────────────────────────────────────────
    let (header_rows, detail_content) = if let Some(entry) = selected_error {
        let hdr = vec![
            DetailHeaderRow {
                content: format!("[{}] {}", entry.seq, entry.title),
                color: DetailHeaderColor::Bright,
                line: 0,
            },
            DetailHeaderRow {
                content: format!("Source: {:?}  ·  {}", entry.source, entry.timestamp),
                color: DetailHeaderColor::Dim,
                line: 1,
            },
            DetailHeaderRow {
                content: crate::ui::components::SEPARATOR_LINE.to_string(),
                color: DetailHeaderColor::Dim,
                line: 2,
            },
        ];
        (hdr, entry.detail.clone())
    } else {
        (vec![], String::new())
    };

    let header_line_offset = header_rows.len();
    let detail_props = DetailPaneProps {
        header_rows,
        content: detail_content,
        content_cursor: None,
        scroll_offset: detail_scroll_offset,
        viewport_rows: detail_viewport_rows,
        content_line_offset: header_line_offset,
        max_line_width: detail_content_width,
        focused: detail_focused,
        pane: SelectablePane::ErrorDetail,
        colors: colors.clone(),
        selection,
        composer: None,
        composer_rows: 0,
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            // ── Status bar ──────────────────────────────────────────────────
            StatusBar(
                repo_count: repo_count,
                running_count: running_count,
                agent_count: agent_count,
                theme_name: props.theme_name.clone(),
                version: crate::VERSION.to_owned(),
                warning_message: state.and_then(|s| s.warning_message.clone()),
                last_error: state.and_then(|s| {
                    s.errors_state.last_error().map(|e| e.title.clone())
                }),
                colors: colors.clone(),
                selection: selection,
            )

            // ── Main body: sidebar + errors workspace ───────────────────────
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0_f32,
                width: 100pct,
            ) {
                // Repos sidebar (fixed width, full height)
                Box(width: sidebar_width, height: 100pct) {
                    Sidebar(
                        repositories: repositories,
                        agent_counts: agent_counts,
                        selected: selected_repo_idx,
                        focused: sidebar_focused,
                        grabbed: None,
                        pane_rows: render_rows.saturating_sub(crate::layout::OUTER_BARS_HEIGHT),
                        content_width: crate::list_viewport::bordered_padded_content_width(crate::layout::PRS_SIDEBAR_WIDTH),
                        colors: colors.clone(),
                        selection: selection,
                    )
                }

                // Errors workspace (flex-grow)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    height: 100pct,
                ) {
                    // Error list (top split)
                    Box(height: list_pane_rows, width: 100pct) {
                        #(vec![selectable_list_element(SelectableListProps {
                            title: "Errors".to_string(),
                            rows: list_rows,
                            focused: list_focused,
                            empty_message,
                            colors: colors.clone(),
                            selection,
                            pane: SelectablePane::ErrorList,
                            border: ListBorder::DoubleOnFocus,
                            content_padding: false,
                            selection_style: SelectionStyle::BoldSelected,
                            content_width: list_content_width as usize,
                        })])
                    }

                    // Error detail (bottom split)
                    Box(flex_grow: 1.0_f32, width: 100pct) {
                        #(vec![detail_pane_element(detail_props)])
                    }
                }
            }

            // ── Keybind bar ─────────────────────────────────────────────────
            KeybindBar(
                screen_mode: state.map_or(ScreenMode::DashboardErrors, |s| s.screen_mode),
                terminal_focused: false,
                actions_focus: None,
                colors: colors.clone(),
            )
        }
    }
}
