//! Issues mode screen — two-column layout: repos sidebar + issues workspace.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-001
//! @requirement REQ-ISS-NFR-001

use iocraft::prelude::*;

use crate::state::{AppState, IssueFocus, PaneFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{
    AgentChooser, FilterControls, IssueDetailView, IssueList, KeybindBar, Sidebar, StatusBar,
};

/// Props for the issues mode screen.
#[derive(Default, Props)]
pub struct IssuesScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Issues mode screen layout — two-column: repos sidebar + issues workspace.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-001
#[component]
pub fn IssuesScreen(props: &IssuesScreenProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));

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
    let pane_focus = state.map_or(PaneFocus::Repositories, |s| s.pane_focus);

    // ── Status bar data ─────────────────────────────────────────────────────
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);

    // ── Issues state ────────────────────────────────────────────────────────
    let issue_focus = state.map_or(IssueFocus::IssueList, |s| s.issues_state.issue_focus);
    let issues = state.map_or_else(Vec::new, |s| s.issues_state.issues.clone());
    let selected_issue_idx = state.and_then(|s| s.issues_state.selected_issue_index);
    let list_loading = state.is_some_and(|s| s.issues_state.list_loading);
    let filter_controls_open = state.is_some_and(|s| s.issues_state.filter_controls_open);
    let filter_field_index = state.map_or(0, |s| s.issues_state.filter_field_index);
    let draft_labels_text =
        state.map_or_else(String::new, |s| s.issues_state.draft_labels_text.clone());
    let draft_filter = state.map_or_else(Default::default, |s| s.issues_state.draft_filter.clone());
    let has_filters = state.is_some_and(|s| {
        let f = &s.issues_state.committed_filter;
        f.state.is_some()
            || !f.author.is_empty()
            || !f.assignee.is_empty()
            || !f.labels.is_empty()
            || !f.query_text.is_empty()
    });

    // Issue detail fields
    let issue_detail = state.and_then(|s| s.issues_state.issue_detail.clone());
    let detail_subfocus = state.map_or_else(Default::default, |s| s.issues_state.detail_subfocus);
    let inline_state = state.map_or_else(Default::default, |s| s.issues_state.inline_state.clone());
    let comments_loading = state.is_some_and(|s| s.issues_state.comments_loading);
    let issue_list_scroll_offset = state.map_or(0, |s| s.issues_state.issue_list_scroll_offset());
    let detail_scroll_offset = state.map_or(0, |s| s.issues_state.detail_scroll_offset);
    let detail_focused = issue_focus == IssueFocus::IssueDetail;

    // Error message
    let error_message = state.and_then(|s| s.issues_state.error.clone());

    // Agent chooser overlay
    let agent_chooser = state.and_then(|s| s.issues_state.agent_chooser.clone());
    let chooser_visible = agent_chooser.is_some();
    let chooser_agents = agent_chooser
        .as_ref()
        .map_or_else(Vec::new, |c| c.agents.clone());
    let chooser_selected = agent_chooser.as_ref().map_or(0, |c| c.selected_index);

    // Sidebar is highlighted when RepoList focus or PaneFocus::Repositories
    let sidebar_focused =
        pane_focus == PaneFocus::Repositories || issue_focus == IssueFocus::RepoList;
    let list_focused = issue_focus == IssueFocus::IssueList || issue_focus == IssueFocus::RepoList;

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
            )

            // ── Main body: sidebar + issues workspace ───────────────────────
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                width: 100pct,
            ) {
                // Repos sidebar (fixed 22u, full height)
                Box(width: 22u32, height: 100pct) {
                    Sidebar(
                        repositories: repositories,
                        agent_counts: agent_counts,
                        selected: selected_repo_idx,
                        focused: sidebar_focused,
                        colors: colors.clone(),
                    )
                }

                // Issues workspace (flex-grow)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    height: 100pct,
                ) {
                    // Error banner (when present)
                    #(if let Some(ref err) = error_message {
                        vec![element! {
                            Box(height: 1u32, width: 100pct, padding_left: 1u32) {
                                Text(
                                    content: format!("Error: {}", err),
                                    color: rc.bright,
                                    weight: Weight::Bold,
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Filter band (when open)
                    #(if filter_controls_open {
                        vec![element! {
                            Box(width: 100pct) {
                                FilterControls(
                                    draft_filter: draft_filter.clone(),
                                    visible: true,
                                    colors: colors.clone(),
                                    active_field_index: filter_field_index,
                                    draft_labels_text: draft_labels_text.clone(),
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Issue list + detail (split view)
                    // Fixed 30/70 split: compact issue list + detail view
                    Box(height: 30pct, width: 100pct) {
                        IssueList(
                            issues: issues.clone(),
                            selected_index: selected_issue_idx,
                            focused: list_focused,
                            loading: list_loading,
                            has_filters: has_filters,
                            compact: true,
                            scroll_offset: issue_list_scroll_offset,
                            colors: colors.clone(),
                        )
                    }
                    Box(flex_grow: 1.0, width: 100pct) {
                        IssueDetailView(
                            issue_detail: issue_detail.clone(),
                            detail_subfocus: detail_subfocus,
                            inline_state: inline_state.clone(),
                            comments_loading: comments_loading,
                            focused: detail_focused,
                            scroll_offset: detail_scroll_offset,
                            colors: colors.clone(),
                        )
                    }

                    // Agent chooser overlay (anchored inside workspace)
                    #(if chooser_visible {
                        vec![element! {
                            Box(
                                position: Position::Absolute,
                                top: 2,
                                left: 4,
                            ) {
                                AgentChooser(
                                    visible: true,
                                    agents: chooser_agents.clone(),
                                    selected_index: chooser_selected,
                                    colors: colors.clone(),
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })
                }
            }

            // ── Keybind bar ─────────────────────────────────────────────────
            KeybindBar(
                screen_mode: state.map_or(ScreenMode::DashboardIssues, |s| s.screen_mode),
                terminal_focused: false,
                colors: colors.clone(),
            )
        }
    }
}
