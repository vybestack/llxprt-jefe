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
    AgentChooser, IssueDeleteConfirmOverlay, IssueDetailProjectionInputs, IssueListLayout,
    IssueListWindow, KeybindBar, Sidebar, StatusBar, detail_pane_element, filter_bar_element,
    issue_detail_props, issue_filter_props, issue_list_props, issue_list_status_message,
    selectable_list_element,
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
    let pane_focus = state.map_or(PaneFocus::Repositories, |s| s.pane_focus);

    // ── Status bar data ─────────────────────────────────────────────────────
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);

    // ── Issues state ────────────────────────────────────────────────────────
    let issue_focus = state.map_or(IssueFocus::IssueList, |s| s.issues_state.issue_focus);
    let issues = state.map_or_else(Vec::new, |s| s.issues_state.issues().to_vec());
    let selected_issue_idx = state.and_then(|s| s.issues_state.selected_issue_index());
    let list_loading = state.is_some_and(|s| s.issues_state.list_loading());
    let filter_controls_open = state.is_some_and(|s| s.issues_state.filter_ui.controls_open);
    let filter_field_index = state.map_or(0, |s| s.issues_state.filter_ui.field_index);
    let draft_labels_text = state.map_or_else(String::new, |s| {
        s.issues_state.filter_ui.draft_labels_text.clone()
    });
    let draft_filter = state.map_or_else(Default::default, |s| s.issues_state.draft_filter.clone());
    let has_filters = state.is_some_and(|s| {
        s.issues_state
            .committed_filter
            .has_active_non_default_filters()
    });

    // Issue detail fields
    let issue_detail = state.and_then(|s| s.issues_state.issue_detail.clone());
    let detail_subfocus = state.map_or_else(Default::default, |s| s.issues_state.detail_subfocus);
    let inline_state = state.map_or_else(Default::default, |s| s.issues_state.inline_state.clone());
    let comments_loading = state.is_some_and(|s| s.issues_state.loading.comments);
    let detail_scroll_offset = state.map_or(0, |s| s.issues_state.detail_scroll_offset);
    let detail_focused = issue_focus == IssueFocus::IssueDetail;

    // Error message
    let error_message = state.and_then(|s| s.issues_state.error.clone());
    // Draft notice (e.g. "No agents available") — used as the banner
    // fallback when no error is present (issue #265).
    let draft_notice = state.and_then(|s| s.issues_state.draft_notice.clone());

    // Single banner projection: error takes precedence over draft_notice.
    // This same value drives both the visible banner and the pane row sizing
    // so they can never disagree (issue #265).
    let banner_text =
        crate::layout::issues_banner_text(error_message.as_deref(), draft_notice.as_deref());
    let banner_is_error = error_message.is_some();
    let banner_content = banner_text.map(|b| {
        if banner_is_error {
            format!("Error: {b}")
        } else {
            b.to_string()
        }
    });
    let banner_color = if banner_is_error { rc.bright } else { rc.dim };
    let banner_weight = if banner_is_error {
        Weight::Bold
    } else {
        Weight::Normal
    };

    // Compute the actual rows/columns available to issue panes so child
    // components do not have to infer from raw terminal size.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (list_pane_rows, detail_pane_height) = crate::layout::issues_pane_rows(
        usize::from(term_rows),
        banner_text.is_some(),
        filter_controls_open,
    );
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let detail_pane_height = u16::try_from(detail_pane_height).unwrap_or(u16::MAX);
    let list_width = crate::layout::issue_list_content_width(term_cols);

    // Single source of truth for the fixed sidebar width: the layout constant
    // is u16 but the iocraft width field expects u32.
    let sidebar_width = u32::from(crate::layout::ISSUES_SIDEBAR_WIDTH);

    // Agent chooser overlay
    let agent_chooser = state.and_then(|s| s.issues_state.agent_chooser.clone());
    let chooser_visible = agent_chooser.is_some();
    let chooser_agents = agent_chooser
        .as_ref()
        .map_or_else(Vec::new, |c| c.agents.clone());
    let chooser_selected = agent_chooser.as_ref().map_or(0, |c| c.selected_index);

    // Delete confirm overlay (issue #182)
    let delete_confirm = state.and_then(|s| s.issues_state.delete_confirm.clone());
    let delete_visible = delete_confirm.is_some();
    let delete_issue_number = delete_confirm.as_ref().map_or(0, |c| c.issue_number);
    let delete_awaiting = delete_confirm
        .as_ref()
        .is_some_and(|c| c.awaiting_confirmation);

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
                kennel_mode: state.is_some_and(crate::state::AppState::is_kennel_mode),
                warning_message: state.and_then(|s| s.warning_message.clone()),
                colors: colors.clone(),
                selection: selection,
            )

            // ── Main body: sidebar + issues workspace ───────────────────────
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0_f32,
                width: 100pct,
            ) {
                // Repos sidebar (fixed 22u, full height)
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

                // Issues workspace (flex-grow)
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    height: 100pct,
                ) {
                    // Banner (error with precedence over draft_notice — issue #265)
                    #(if let Some(content) = banner_content {
                        vec![element! {
                            Box(height: 1u32, width: 100pct, padding_left: 1u32) {
                                Text(
                                    content: content,
                                    color: banner_color,
                                    weight: banner_weight,
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
                                #(vec![filter_bar_element(issue_filter_props(
                                    &draft_filter,
                                    &draft_labels_text,
                                    filter_field_index,
                                    true,
                                    colors.clone(),
                                ))])
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Issue list + detail (split view)
                    // Fixed 30/70 split: compact issue list + detail view
                    Box(height: list_pane_rows, width: 100pct) {
                        #(vec![selectable_list_element(issue_list_props(
                            &issues,
                            IssueListWindow {
                                selected_index: selected_issue_idx,
                                list_pane_rows,
                                layout: IssueListLayout::Compact,
                                available_width: Some(list_width),
                            },
                            list_focused,
                            issue_list_status_message(list_loading, issues.is_empty(), has_filters),
                            colors.clone(),
                            selection,
                        ))])
                    }
                    Box(flex_grow: 1.0_f32, width: 100pct) {
                        #(vec![detail_pane_element(issue_detail_props(
                            IssueDetailProjectionInputs {
                                issue_detail: issue_detail.as_ref(),
                                detail_subfocus,
                                inline_state: &inline_state,
                                comments_loading,
                                focused: detail_focused,
                                scroll_offset: detail_scroll_offset,
                                colors: colors.clone(),
                                available_height: Some(detail_pane_height),
                                available_width: Some(crate::layout::issues_detail_content_width(term_cols)),
                                selection,
                            },
                        ))])
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
                                    selection: selection,
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Delete confirm overlay (issue #182)
                    #(if delete_visible {
                        vec![element! {
                            Box(
                                position: Position::Absolute,
                                top: 2,
                                left: 4,
                            ) {
                                IssueDeleteConfirmOverlay(
                                    visible: true,
                                    issue_number: delete_issue_number,
                                    awaiting_confirmation: delete_awaiting,
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
