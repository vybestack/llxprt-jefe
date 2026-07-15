//! Pull Requests mode screen — two-column layout: repos sidebar + PR workspace.
//!
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-001
//! @requirement REQ-PR-NFR-003

use iocraft::prelude::*;

use crate::state::{AppState, PaneFocus, PrFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

use super::super::components::{
    AgentChooser, KeybindBar, MergeChooser, PrDetailProjectionInputs, PrListLayout, PrListWindow,
    PropertyEditor, Sidebar, StatusBar, detail_pane_element, filter_bar_element, pr_detail_props,
    pr_filter_props, pr_list_props, pr_list_status_message, selectable_list_element,
};

/// Props for the pull requests mode screen.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 1-12
#[derive(Default, Props)]
pub struct PullRequestsScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Pull Requests mode screen layout — two-column: repos sidebar + PR workspace.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 1-12
#[component]
pub fn PullRequestsScreen(props: &PullRequestsScreenProps) -> impl Into<AnyElement<'static>> {
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

    // ── PRs state ──────────────────────────────────────────────────────────
    let pr_focus = state.map_or(PrFocus::PrList, |s| s.prs_state.pr_focus);
    let pull_requests = state.map_or_else(Vec::new, |s| s.prs_state.pull_requests().to_vec());
    let selected_pr_idx = state.and_then(|s| s.prs_state.selected_pr_index());
    let list_loading = state.is_some_and(|s| s.prs_state.list_loading());
    let filter_controls_open = state.is_some_and(|s| s.prs_state.filter_ui.controls_open);
    let filter_field_index = state.map_or(0, |s| s.prs_state.filter_ui.field_index);
    let draft_labels_text = state.map_or_else(String::new, |s| {
        s.prs_state.filter_ui.draft_labels_text.clone()
    });
    let draft_filter = state.map_or_else(Default::default, |s| s.prs_state.draft_filter.clone());
    let has_filters = state.is_some_and(|s| {
        let f = &s.prs_state.committed_filter;
        f.state.is_some()
            || !f.author.is_empty()
            || !f.assignee.is_empty()
            || !f.reviewer.is_empty()
            || !f.labels.is_empty()
            || !f.query_text.is_empty()
            || f.is_draft.is_some()
            || f.review_decision != crate::domain::ReviewDecisionFilter::Any
            || f.checks_status != crate::domain::ChecksFilter::Any
    });

    // PR detail fields
    let pr_detail = state.and_then(|s| s.prs_state.pr_detail.clone());
    let detail_subfocus = state.map_or_else(Default::default, |s| s.prs_state.detail_subfocus);
    let inline_state = state.map_or_else(Default::default, |s| s.prs_state.inline_state.clone());
    let comments_loading = state.is_some_and(|s| s.prs_state.loading.comments);
    let detail_loading = state.is_some_and(|s| s.prs_state.loading.detail);
    let detail_scroll_offset = state.map_or(0, |s| s.prs_state.detail_scroll_offset);
    let detail_focused = pr_focus == PrFocus::PrDetail;

    // Error message
    let error_message = state.and_then(|s| s.prs_state.error.clone());

    // Compute the actual rows/columns available to PR panes so child
    // components do not have to infer from raw terminal size.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (render_cols, render_rows) = crate::layout::effective_render_size(term_cols, term_rows);
    let (list_pane_rows, detail_pane_height) = crate::layout::prs_pane_rows(
        usize::from(render_rows),
        error_message.is_some(),
        filter_controls_open,
    );
    let list_pane_rows = u16::try_from(list_pane_rows).unwrap_or(u16::MAX);
    let detail_pane_height = u16::try_from(detail_pane_height).unwrap_or(u16::MAX);
    let list_width = crate::layout::pr_list_content_width(render_cols);
    // Compute the detail content width from the SAME terminal size read the
    // screen already performs — single source of truth for wrapping so the
    // renderer and the reducer scroll clamp agree.
    let detail_content_width = crate::layout::prs_detail_content_width(render_cols) as usize;

    let sidebar_width = u32::from(crate::layout::prs_main_columns(render_cols).sidebar_width);

    // Agent chooser overlay
    let agent_chooser = state.and_then(|s| s.prs_state.agent_chooser.clone());
    let chooser_visible = agent_chooser.is_some();
    let chooser_agents = agent_chooser
        .as_ref()
        .map_or_else(Vec::new, |c| c.agents.clone());
    let chooser_selected = agent_chooser.as_ref().map_or(0, |c| c.selected_index);
    let chooser_transient_available = agent_chooser
        .as_ref()
        .is_some_and(|c| c.transient_available);

    // Merge chooser overlay (issue #92)
    let merge_chooser = state.and_then(|s| s.prs_state.merge_chooser.clone());
    let merge_visible = merge_chooser.is_some();
    let merge_selected = merge_chooser.as_ref().map_or(0, |c| c.selected_index);
    let merge_allowed = merge_chooser
        .as_ref()
        .and_then(|c| c.allowed_methods.clone());
    let merge_confirming = merge_chooser
        .as_ref()
        .is_some_and(|c| c.awaiting_confirmation);
    let merge_pr_number = pr_detail.as_ref().map_or(0, |d| d.number);

    // Property editor overlay (issue #175)
    let prop_editor = state.and_then(|s| s.prs_state.property_editor.clone());
    let prop_visible = prop_editor.is_some();
    let prop_header = prop_editor.as_ref().map_or_else(String::new, |e| {
        let kind_label = match e.kind {
            crate::state::PrPropertyKind::Labels => "Labels",
            crate::state::PrPropertyKind::Assignees => "Assignees",
            crate::state::PrPropertyKind::Milestone => "Milestone",
            crate::state::PrPropertyKind::Title => "Title",
            crate::state::PrPropertyKind::State => "State",
        };
        let num = pr_detail.as_ref().map_or(0, |d| d.number);
        format!("Edit {kind_label} - PR #{num}")
    });
    let prop_options: Vec<(String, bool)> = prop_editor.as_ref().map_or_else(Vec::new, |e| {
        e.options
            .iter()
            .map(|o| (o.label.clone(), o.selected))
            .collect()
    });
    let prop_selected = prop_editor.as_ref().map_or(0, |e| e.selected_index);
    let prop_multi = prop_editor.as_ref().is_some_and(|e| {
        matches!(
            e.kind,
            crate::state::PrPropertyKind::Labels | crate::state::PrPropertyKind::Assignees
        )
    });
    let prop_is_title = prop_editor
        .as_ref()
        .is_some_and(|e| matches!(e.kind, crate::state::PrPropertyKind::Title));
    let prop_title_text = prop_editor
        .as_ref()
        .map_or_else(String::new, |e| e.title_text.clone());
    let prop_title_cursor = prop_editor.as_ref().map_or(0, |e| e.title_cursor);
    let prop_error = prop_editor.as_ref().and_then(|e| e.error.clone());

    // Sidebar is highlighted when RepoList focus or PaneFocus::Repositories
    let sidebar_focused = pane_focus == PaneFocus::Repositories || pr_focus == PrFocus::RepoList;
    let list_focused = pr_focus == PrFocus::PrList || pr_focus == PrFocus::RepoList;

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

            // ── Main body: sidebar + PR workspace ───────────────────────────
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
                        grabbed: None,
                        pane_rows: render_rows.saturating_sub(crate::layout::OUTER_BARS_HEIGHT),
                        content_width: crate::list_viewport::bordered_padded_content_width(crate::layout::PRS_SIDEBAR_WIDTH),
                        colors: colors.clone(),
                        selection: selection,
                    )
                }

                // PR workspace (flex-grow)
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

                    // Filter band (when open)
                    #(if filter_controls_open {
                        vec![element! {
                            Box(width: 100pct) {
                                #(vec![filter_bar_element(pr_filter_props(
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

                    // PR list + detail (split view)
                    // Fixed 30/70 split: compact PR list + detail view
                    Box(height: list_pane_rows, width: 100pct) {
                        #(vec![selectable_list_element(pr_list_props(
                            &pull_requests,
                            PrListWindow {
                                selected_index: selected_pr_idx,
                                list_pane_rows,
                                available_width: Some(list_width),
                                layout: PrListLayout::Compact,
                            },
                            list_focused,
                            pr_list_status_message(
                                list_loading,
                                pull_requests.is_empty(),
                                has_filters,
                            ),
                            colors.clone(),
                            selection,
                        ))])
                    }
                    Box(flex_grow: 1.0_f32, width: 100pct) {
                        #(vec![detail_pane_element(pr_detail_props(
                            PrDetailProjectionInputs {
                                detail: pr_detail.as_ref(),
                                subfocus: detail_subfocus,
                                inline_state: &inline_state,
                                detail_loading,
                                comments_loading,
                                focused: detail_focused,
                                scroll_offset: detail_scroll_offset,
                                detail_content_width,
                                colors: colors.clone(),
                                viewport_rows: Some(detail_pane_height),
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
                                    transient_available: chooser_transient_available,
                                    selected_index: chooser_selected,
                                    colors: colors.clone(),
                                    selection: selection,
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Merge chooser overlay (issue #92)
                    #(if merge_visible {
                        vec![element! {
                            Box(
                                position: Position::Absolute,
                                top: 2,
                                left: 4,
                            ) {
                                MergeChooser(
                                    visible: true,
                                    pr_number: merge_pr_number,
                                    selected_index: merge_selected,
                                    allowed_methods: merge_allowed.clone(),
                                    awaiting_confirmation: merge_confirming,
                                    colors: colors.clone(),
                                    selection: selection,
                                )
                            }
                        }]
                    } else {
                        vec![]
                    })

                    // Property editor overlay (issue #175)
                    #(if prop_visible {
                        vec![element! {
                            Box(
                                position: Position::Absolute,
                                top: 2,
                                left: 4,
                            ) {
                                PropertyEditor(
                                    visible: true,
                                    header: prop_header.clone(),
                                    options: prop_options.clone(),
                                    selected_index: prop_selected,
                                    multi_select: prop_multi,
                                    title_text: prop_title_text.clone(),
                                    title_cursor: prop_title_cursor,
                                    is_title: prop_is_title,
                                    error: prop_error.clone(),
                                    terminal_cols: term_cols,
                                    terminal_rows: term_rows,
                                    colors: colors.clone(),
                                    selection: selection,
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
                screen_mode: state.map_or(ScreenMode::DashboardPullRequests, |s| s.screen_mode),
                terminal_focused: false,
                actions_focus: None,
                colors: colors.clone(),
            )
        }
    }
}
