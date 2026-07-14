//! Actions mode screen — two-column layout: repos sidebar + actions workspace.
//!
//! This screen mirrors the PRs screen ([`super::pull_requests`]) exactly: the
//! same shared layout constants, the same `selectable_list_element` for the
//! runs list, and the same `detail_pane_element` for the run detail. The
//! sidebar uses the fixed shared width, and the list/detail panes use the
//! shared bordered components so content never escapes its box.

use iocraft::prelude::*;

use crate::state::{ActionsFocus, AppState, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{
    ActionsDetailProjectionInputs, ActionsListLayout, ActionsListWindow, KeybindBar, Sidebar,
    StatusBar, actions_detail_props, actions_filter_props, actions_list_props,
    actions_list_status_message, detail_pane_element, filter_bar_element, selectable_list_element,
};

/// Props for the actions mode screen.
#[derive(Default, Props)]
pub struct ActionsScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Actions mode screen layout — two-column: repos sidebar + actions workspace.
///
/// The element tree matches [`crate::ui::screens::PullRequestsScreen`] so the
/// sidebar width, pane proportions, and border behavior are identical across
/// all workspace screens.
#[component]
pub fn ActionsScreen(props: &ActionsScreenProps) -> impl Into<AnyElement<'static>> {
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

    // ── Actions data ────────────────────────────────────────────────────────
    let actions_focus = state.map_or(ActionsFocus::RunList, |s| s.actions_state.focus);
    let runs = state.map_or_else(Vec::new, |s| s.actions_state.runs().to_vec());
    let selected_run_idx = state.and_then(|s| s.actions_state.selected_run_index());
    let detail = state.and_then(|s| s.actions_state.run_detail.clone());
    let error_message = state.and_then(|s| s.actions_state.error.clone());
    let filter_open = state.is_some_and(|s| s.actions_state.ui.filter_ui_open);
    let filter_field_index = state.map_or(0, |s| s.actions_state.ui.filter_field_index);
    let draft_filter =
        state.map_or_else(Default::default, |s| s.actions_state.draft_filter.clone());
    let loading = state.is_some_and(|s| s.actions_state.list_loading());
    let detail_scroll_offset = state.map_or(0, |s| s.actions_state.detail_scroll_offset);
    let expanded_jobs = state.map_or_else(std::collections::HashSet::new, |s| {
        s.actions_state.expanded_jobs.clone()
    });

    let has_filters = state.is_some_and(|s| {
        let f = &s.actions_state.committed_filter;
        !f.workflow.is_empty()
            || !f.status.is_empty()
            || !f.search.is_empty()
            || f.pr_number.is_some()
    });

    // Compute the rows/columns available to panes using the SAME shared helpers
    // the PRs screen uses — single source of truth for geometry.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (list_pane_rows, detail_pane_height) =
        crate::layout::prs_pane_rows(usize::from(term_rows), error_message.is_some(), filter_open);
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let detail_pane_height = u16::try_from(detail_pane_height).unwrap_or(u16::MAX);
    let list_width = crate::layout::pr_list_content_width(term_cols);
    let detail_content_width = crate::layout::prs_detail_content_width(term_cols) as usize;
    let sidebar_width = u32::from(crate::layout::prs_main_columns(term_cols).sidebar_width);

    // In Actions mode the sidebar focus is driven solely by ActionsFocus
    // (RepoList), not PaneFocus — otherwise the state=None default
    // (PaneFocus::Repositories + ActionsFocus::RunList) would highlight both
    // the sidebar and the run list on the first frame.
    let sidebar_focused = actions_focus == ActionsFocus::RepoList;
    let list_focused = actions_focus == ActionsFocus::RunList;
    let detail_focused = actions_focus == ActionsFocus::Detail;

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
                colors: colors.clone(),
                selection: selection,
            )

            // ── Main body: sidebar + actions workspace ──────────────────────
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
                        colors: colors.clone(),
                        selection: selection,
                    )
                }

                // Actions workspace (flex-grow)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    height: 100pct,
                ) {
                    // Error banner (when present)
                    #(if let Some(line) = crate::layout::pr_error_banner_line(error_message.as_deref()) {
                        vec![element! {
                            Box(height: 1u32, width: 100pct, padding_left: 1u32) {
                                Text(
                                    content: line,
                                    color: rc.bright,
                                    weight: Weight::Bold,
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Filter band (when open) — same generic FilterBar
                    // component the Issues/PRs screens use.
                    #(if filter_open {
                        vec![element! {
                            Box(width: 100pct) {
                                #(vec![filter_bar_element(actions_filter_props(
                                    &draft_filter,
                                    filter_field_index,
                                    true,
                                    colors.clone(),
                                ))])
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Runs list (top split)
                    Box(height: list_pane_rows, width: 100pct) {
                        #(vec![selectable_list_element(actions_list_props(
                            &runs,
                            ActionsListWindow {
                                selected_index: selected_run_idx,
                                list_pane_rows,
                                available_width: Some(list_width),
                                layout: ActionsListLayout::Compact,
                            },
                            list_focused,
                            actions_list_status_message(loading, runs.is_empty(), has_filters),
                            colors.clone(),
                            selection,
                        ))])
                    }

                    // Run detail (bottom split)
                    Box(flex_grow: 1.0_f32, width: 100pct) {
                        #(vec![detail_pane_element(actions_detail_props(
                            ActionsDetailProjectionInputs {
                                detail: detail.as_ref(),
                                scroll_offset: detail_scroll_offset,
                                viewport_rows: Some(detail_pane_height),
                                focused: detail_focused,
                                content_width: detail_content_width,
                                colors: colors.clone(),
                                selection,
                                expanded_jobs: &expanded_jobs,
                            },
                        ))])
                    }
                }
            }

            // ── Keybind bar ─────────────────────────────────────────────────
            KeybindBar(
                screen_mode: state.map_or(ScreenMode::DashboardActions, |s| s.screen_mode),
                terminal_focused: false,
                colors: colors.clone(),
            )
        }
    }
}
