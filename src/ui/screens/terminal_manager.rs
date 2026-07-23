//! Terminal Manager screen — two-column layout: repos sidebar + shell workspace.
//!
//! Mirrors the errors/actions screen pattern: a fixed sidebar + a list pane
//! (top split) + a detail pane (bottom split). The list shows every runtime
//! inventory shell with owner agent name, repository name, workdir, status,
//! and a close-only annotation for dead/non-Running owners. The detail pane
//! shows a throttled, read-only preview of the selected shell captured from
//! `<session>:jefe-shell` (never a second live viewer).

use iocraft::prelude::*;

use crate::runtime::TerminalSnapshot;
use crate::selection::SelectablePane;
use crate::state::project_managed_shell_rows;
use crate::state::{AppState, ManagedShellRow, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::detail_pane::{DetailHeaderColor, DetailHeaderRow, DetailPaneProps};
use super::super::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
};
use super::super::components::{
    KeybindBar, Sidebar, StatusBar, TerminalView, detail_pane_element, selectable_list_element,
};

/// Props for the Terminal Manager screen.
#[derive(Default, Props)]
pub struct TerminalManagerScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
    /// Live snapshot from the single attached viewer.
    pub terminal_snapshot: Option<TerminalSnapshot>,
    /// Scrollback history for the live lower-pane terminal.
    pub history_lines: Vec<String>,
    /// Inner PTY rows allocated to the lower pane.
    pub terminal_pane_rows: u16,
    /// Inner PTY columns allocated to the lower pane.
    pub terminal_pane_cols: u16,
}

/// Terminal Manager screen layout — two-column: repos sidebar + shell workspace.
#[component]
pub fn TerminalManagerScreen(props: &TerminalManagerScreenProps) -> impl Into<AnyElement<'static>> {
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

    // ── Manager data ────────────────────────────────────────────────────────
    let rows: Vec<ManagedShellRow> = state.map_or_else(Vec::new, project_managed_shell_rows);
    let selected_index = state.and_then(|s| s.terminal_manager.selected_index);
    let preview = state
        .map(|s| s.terminal_manager.preview.clone())
        .unwrap_or_default();
    let pending_focus = state.and_then(|s| s.terminal_manager.pending_focus.as_ref());
    let live_shell_active = state.is_some_and(AppState::shell_overlay_active);

    // ── Layout geometry ─────────────────────────────────────────────────────
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (render_cols, render_rows) = crate::layout::effective_render_size(term_cols, term_rows);
    // Manager has no error banner and no filter band (same as errors).
    let (list_pane_rows, _) =
        crate::layout::actions_pane_rows(usize::from(render_rows), false, false);
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let sidebar_width = u32::from(crate::layout::prs_main_columns(render_cols).sidebar_width);
    let list_content_width = crate::layout::pr_list_content_width(render_cols);
    let detail_viewport_rows =
        crate::layout::prs_detail_viewport_rows(usize::from(render_rows), false, false);
    let detail_content_width = usize::from(crate::layout::prs_detail_content_width(render_cols));

    // ── Shell list rows ─────────────────────────────────────────────────────
    let list_rows: Vec<SelectableRow> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let title = if row.close_only {
                format!("{} (close-only)", row.agent_name)
            } else {
                row.agent_name.clone()
            };
            let meta = format!(
                "{} · {} · {}{}",
                row.repository_name,
                row.work_dir,
                row.status_label,
                if row.close_only {
                    " · dead/non-running"
                } else {
                    ""
                }
            );
            SelectableRow {
                source_index: idx,
                spans: vec![SelectableSpan {
                    text: title,
                    color: SpanColor::Themed,
                }],
                meta_line: Some(meta),
                is_selected: selected_index == Some(idx),
            }
        })
        .collect();

    let empty_message = if rows.is_empty() {
        Some("No shells.".to_string())
    } else {
        None
    };

    // ── Preview detail ──────────────────────────────────────────────────────
    let selected_row = selected_index.and_then(|idx| rows.get(idx));
    let (header_rows, detail_content) =
        build_preview_content(selected_row, &preview, pending_focus);

    let header_line_offset = header_rows.len();
    let detail_props = DetailPaneProps {
        header_rows,
        content: detail_content,
        content_cursor: None,
        scroll_offset: 0,
        viewport_rows: detail_viewport_rows,
        content_line_offset: header_line_offset,
        max_line_width: detail_content_width,
        focused: false,
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
                last_error: state.and_then(AppState::last_error_title),
                colors: colors.clone(),
                selection: selection,
            )

            // ── Main body: sidebar + shell workspace ───────────────────────
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
                        focused: false,
                        grabbed: None,
                        pane_rows: render_rows.saturating_sub(crate::layout::OUTER_BARS_HEIGHT),
                        content_width: crate::list_viewport::bordered_padded_content_width(crate::layout::PRS_SIDEBAR_WIDTH),
                        colors: colors.clone(),
                        selection: selection,
                    )
                }

                // Shell workspace (flex-grow)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    height: 100pct,
                ) {
                    // Shell list (top split)
                    Box(height: list_pane_rows, width: 100pct) {
                        #(vec![selectable_list_element(SelectableListProps {
                            title: "Terminal Manager".to_string(),
                            rows: list_rows,
                            focused: true,
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

                    // Static preview or the single live viewer (bottom split)
                    Box(flex_grow: 1.0_f32, width: 100pct) {
                        #(if live_shell_active {
                            vec![element! {
                                TerminalView(
                                    snapshot: props.terminal_snapshot.clone(),
                                    focused: true,
                                    title: "Agent Shell".to_owned(),
                                    colors: colors.clone(),
                                    selection: selection,
                                    session_live: true,
                                    history_lines: props.history_lines.clone(),
                                    terminal_history_offset: state.and_then(|s| s.terminal_history_offset),
                                    override_theme: state.is_some_and(|s| s.override_agent_theme),
                                    pane_rows: props.terminal_pane_rows,
                                    pane_cols: props.terminal_pane_cols,
                                    focused_hint: Some("F12 list | F10 close shell".to_owned()),
                                )
                            }.into_any()]
                        } else {
                            vec![detail_pane_element(detail_props)]
                        })
                    }
                }
            }

            // ── Keybind bar ─────────────────────────────────────────────────
            KeybindBar(
                screen_mode: state.map_or(ScreenMode::DashboardTerminals, |s| s.screen_mode),
                terminal_focused: live_shell_active,
                actions_focus: None,
                colors: colors.clone(),
            )
        }
    }
}

/// Build the preview header + body content from the selected row and the last
/// captured preview. Pure projection so the reducer stays deterministic.
fn build_preview_content(
    selected_row: Option<&ManagedShellRow>,
    preview: &crate::state::ShellPreview,
    pending_focus: Option<&crate::state::PendingShellFocus>,
) -> (Vec<DetailHeaderRow>, String) {
    let Some(row) = selected_row else {
        return (vec![], String::new());
    };
    let mut header_rows = vec![
        DetailHeaderRow {
            content: format!("Agent: {}", row.agent_name),
            color: DetailHeaderColor::Bright,
            line: 0,
        },
        DetailHeaderRow {
            content: format!(
                "Repo: {} · Workdir: {} · Status: {}{}",
                row.repository_name,
                row.work_dir,
                row.status_label,
                if row.close_only { " (close-only)" } else { "" }
            ),
            color: DetailHeaderColor::Dim,
            line: 1,
        },
        DetailHeaderRow {
            content: crate::ui::components::SEPARATOR_LINE.to_string(),
            color: DetailHeaderColor::Dim,
            line: 2,
        },
    ];
    if let Some(pending) = pending_focus {
        header_rows.push(DetailHeaderRow {
            content: format!("Focusing {}…", pending.agent_id.0),
            color: DetailHeaderColor::Dim,
            line: 3,
        });
    }
    let body = if preview.failed {
        "(preview unavailable)".to_string()
    } else if preview.lines.is_empty() {
        if row.close_only {
            "(owner not running — close-only)".to_string()
        } else {
            "(capturing preview…)".to_string()
        }
    } else {
        preview.lines.join("\n")
    };
    (header_rows, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
    use crate::state::ShellPreview;
    use std::path::PathBuf;

    fn make_agent(id: &str, name: &str, repo_id: &str, status: AgentStatus) -> Agent {
        let mut agent = Agent::new(
            AgentId(id.into()),
            RepositoryId(repo_id.into()),
            name.into(),
            PathBuf::from(format!("/tmp/{id}")),
        );
        agent.status = status;
        agent
    }

    #[test]
    fn build_preview_content_dead_shows_close_only_placeholder() {
        let mut state = AppState::default();
        state.repositories.push(Repository::new(
            RepositoryId("r".into()),
            "Repo".into(),
            "repo".into(),
            PathBuf::from("/tmp"),
        ));
        let mut agent = make_agent("a", "Alpha", "r", AgentStatus::Dead);
        agent.status = AgentStatus::Dead;
        state.agents.push(agent);
        state.record_shell_window(AgentId("a".into()));
        let rows = project_managed_shell_rows(&state);
        let Some(row) = rows.first() else {
            panic!("row present");
        };
        let (header, body) = build_preview_content(Some(row), &ShellPreview::default(), None);
        assert!(header.len() >= 3);
        assert_eq!(body, "(owner not running — close-only)");
    }
}
