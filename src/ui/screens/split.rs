//! Split screen — multi-agent status workbench (issue #626).
//!
//! Renders the card grid produced by [`crate::workbench_view::build_workbench_view`]
//! alongside a left rail that keeps the repository list and gains a STATUS
//! block showing the four buckets with live counts. `StatusBar` and
//! `KeybindBar` are retained from the previous repository-management layout.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003

use iocraft::prelude::*;

use crate::domain::AgentId;
use crate::git_info::GitRepoInfo;
use crate::state::{AppState, ScreenId};
use crate::theme::{ResolvedColors, ThemeColors};
use crate::workbench_view::{
    StatusBucket, StatusFilterMask, WorkbenchCard as WorkbenchCardModel, WorkbenchRequest,
    WorkbenchView, build_workbench_view,
};

use super::super::components::{KeybindBar, Sidebar, StatusBar, WorkbenchCard};

/// Props for the split screen.
#[derive(Default, Props)]
pub struct SplitScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// Split screen — multi-agent workbench.
///
/// Layout:
/// ```text
/// +----------------------------------------------------------+
/// | StatusBar                                                |
/// +----------------------------------------------------------+
/// | Sidebar (repos)  |  Workbench card grid                  |
/// | STATUS block     |  (or empty_reason message)             |
/// +----------------------------------------------------------+
/// | KeybindBar                                               |
/// +----------------------------------------------------------+
/// ```
#[component]
pub fn SplitScreen(props: &SplitScreenProps) -> impl Into<AnyElement<'static>> {
    let state = props.state.as_ref();
    let selection = state.and_then(|s| s.selection);

    let visible_repo_indices = state.map_or_else(Vec::new, AppState::visible_repository_indices);
    let repo_count = visible_repo_indices.len();
    let running_count = state.map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());
    let agent_count = state.map_or(0, AppState::visible_agent_count);
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
    let selected_repo_idx = state
        .and_then(AppState::selected_repository_visible_index)
        .unwrap_or(0);
    let search_query = state
        .and_then(|s| {
            if let crate::state::ModalState::Search { query } = &s.modal {
                Some(query.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (render_cols, render_rows) = crate::layout::effective_render_size(term_cols, term_rows);
    let split_layout = crate::layout::split_layout_for_render_size(render_cols, render_rows);
    let grabbed = state.and_then(|s| s.split_grab_index);

    // Build the workbench view from the current state.
    let view = build_workbench_view_from_state(state, render_cols, render_rows);
    // Which card Enter would attach to. Drawn with a double border so the
    // target is never ambiguous.
    let selected_agent_id = state.and_then(|s| s.selected_agent().map(|agent| agent.id.clone()));
    let status_filter = state.map_or(StatusFilterMask::all_on(), |s| {
        s.workbench.status_filter.mask()
    });

    let cursor_bucket = state.map_or(
        StatusBucket::NeedsYou,
        AppState::workbench_filter_cursor_bucket,
    );

    let sidebar_width = u32::from(crate::layout::LEFT_COL_WIDTH);
    let card_area_width = render_cols.saturating_sub(crate::layout::LEFT_COL_WIDTH);
    let card_area_width_u32 = u32::from(card_area_width);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            // Status bar
            StatusBar(
                repo_count: repo_count,
                running_count: running_count,
                agent_count: agent_count,
                theme_name: props.theme_name.clone(),
                version: crate::VERSION.to_owned(),
                kennel_mode: state.is_some_and(crate::state::AppState::is_kennel_mode),
                warning_message: state.and_then(|s| s.warning_message.clone()),
                error_count: state.map_or(0, |s| s.errors_state.count()),
                colors: colors.clone(),
                selection: selection,
            )

            // Main content — sidebar + card grid
            Box(
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0_f32,
                width: 100pct,
                background_color: rc.bg,
            ) {
                // Left rail: repositories + STATUS block
                Box(
                    flex_direction: FlexDirection::Column,
                    width: sidebar_width,
                    height: 100pct,
                    background_color: rc.bg,
                ) {
                    // Search/filter bar
                    Box(height: 1u32, width: sidebar_width, background_color: rc.bg, padding_left: 1u32) {
                        Text(content: format!("Filter: {}_", search_query), color: rc.fg)
                    }

                    Box(
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0_f32,
                        width: sidebar_width,
                    ) {
                        Sidebar(
                            repositories: repositories,
                            agent_counts: agent_counts,
                            selected: selected_repo_idx,
                            focused: true,
                            grabbed: grabbed,
                            pane_rows: split_layout.sidebar_rows,
                            // Derive the rail's content width from the rail
                            // itself. `split_layout.sidebar_content_cols` is
                            // computed from nearly the full render width, which
                            // was right when this screen was a full-width
                            // repository list but would now over-report by the
                            // whole card area and mis-elide repository names.
                            content_width: crate::list_viewport::bordered_padded_content_width(
                                crate::layout::LEFT_COL_WIDTH,
                            ),
                            colors: colors.clone(),
                            selection: selection,
                        )
                    }

                    // STATUS block — four buckets with checkboxes and live counts
                    #(status_block_elements(&view, status_filter, cursor_bucket, &rc))
                }

                // Card grid / empty state
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    width: card_area_width_u32,
                    height: 100pct,
                    background_color: rc.bg,
                    padding: 0u32,
                ) {
                    #(card_grid_elements(&view, &colors, selected_agent_id.as_ref()))
                }
            }

            // Keybind bar
            KeybindBar(
                hints: state
                    .unwrap_or_else(|| panic!("screen render requires AppState"))
                    .footer_hints(crate::action_projection::FooterProjectionInput {
                        screen: ScreenId::Repositories,
                        terminal_focused: false,
                        shell_overlay_active: false,
                        shell_resume_available: false,
                        actions_focus: None,
                        mode_override: None,
                    }),
                identity_label: crate::process_identity_label(std::process::id(), crate::GIT_COMMIT),
                colors: colors,
            )
        }
    }
}

/// Build the workbench view model from the application state.
fn build_workbench_view_from_state(
    state: Option<&AppState>,
    render_cols: u16,
    render_rows: u16,
) -> WorkbenchView {
    let Some(state) = state else {
        return build_workbench_view(&WorkbenchRequest {
            agents: Vec::new(),
            status_filter: StatusFilterMask::all_on(),
            repository_filter: None,
            terminal_width: usize::from(render_cols),
            terminal_height: usize::from(render_rows),
            page: 0,
        });
    };

    let agents: Vec<_> = state
        .agents
        .iter()
        .map(|agent| {
            let repo = state.repository_by_id(&agent.repository_id);
            let git_info = repo.map(|r| GitRepoInfo::from_configured_origin(&r.github_repo));
            let observation = state.observations.get(&agent.id);
            (agent.clone(), git_info, observation.cloned())
        })
        .collect();

    let repository_filter = state.split_filter.as_ref().map(|id| id.0.clone());

    build_workbench_view(&WorkbenchRequest {
        agents,
        status_filter: state.workbench.status_filter.mask(),
        repository_filter,
        terminal_width: usize::from(render_cols),
        terminal_height: usize::from(render_rows),
        page: state.workbench.page,
    })
}

/// Build the STATUS block elements for the left rail.
fn status_block_elements(
    view: &WorkbenchView,
    filter: StatusFilterMask,
    cursor_bucket: StatusBucket,
    rc: &ResolvedColors,
) -> Vec<AnyElement<'static>> {
    let buckets = [
        (StatusBucket::NeedsYou, "Needs you"),
        (StatusBucket::Working, "Working"),
        (StatusBucket::Ready, "Ready"),
        (StatusBucket::Stale, "Stale"),
    ];

    let mut elements = Vec::with_capacity(buckets.len() + 1);

    // Header
    elements.push(
        element! {
            Box(height: 1u32, padding_left: 1u32, background_color: rc.bg) {
                Text(content: "STATUS", color: rc.bright, weight: Weight::Bold)
            }
        }
        .into_any(),
    );

    for (bucket, label) in buckets {
        let checked = filter.allows(bucket);
        let mark = if checked { "[x]" } else { "[ ]" };
        let count = view.bucket_counts[bucket.as_index()];
        let cursor = if bucket == cursor_bucket { ">" } else { " " };
        let line = format!("{cursor}{mark} {label} ({count})");
        elements.push(
            element! {
                Box(height: 1u32, padding_left: 1u32, background_color: rc.bg) {
                    Text(content: line, color: rc.fg)
                }
            }
            .into_any(),
        );
    }

    elements
}

/// Build the card grid elements (or the empty-state message).
fn card_grid_elements(
    view: &WorkbenchView,
    colors: &ThemeColors,
    selected: Option<&AgentId>,
) -> Vec<AnyElement<'static>> {
    if let Some(reason) = &view.empty_reason {
        return vec![empty_state_element(reason)];
    }

    let columns = view.layout.columns.max(1);
    let todo_window = view.layout.todo_window;
    let mut elements = render_card_rows(view, columns, todo_window, colors, selected);

    if view.layout.page_count > 1 {
        elements.push(page_position_element(view));
    }

    elements
}

/// Render a single empty-state message element.
fn empty_state_element(reason: &str) -> AnyElement<'static> {
    element! {
        Box(height: 1u32, padding_left: 1u32) {
            Text(content: reason.to_string(), color: Color::DarkGrey)
        }
    }
    .into_any()
}

/// Render the card rows (row-major, `columns` per row).
fn render_card_rows(
    view: &WorkbenchView,
    columns: usize,
    todo_window: usize,
    colors: &ThemeColors,
    selected: Option<&AgentId>,
) -> Vec<AnyElement<'static>> {
    view.cards
        .chunks(columns)
        .map(|chunk| render_card_row(chunk, view.layout.card_width, todo_window, colors, selected))
        .collect()
}

/// Render one row of cards.
fn render_card_row(
    chunk: &[WorkbenchCardModel],
    card_width: usize,
    todo_window: usize,
    colors: &ThemeColors,
    selected: Option<&AgentId>,
) -> AnyElement<'static> {
    let row_children: Vec<AnyElement<'static>> = chunk
        .iter()
        .map(|card| {
            let is_selected = selected.is_some_and(|id| *id == card.agent_id);
            element! {
                WorkbenchCard(
                    card: Some(card.clone()),
                    card_width: card_width,
                    todo_window: todo_window,
                    selected: is_selected,
                    colors: colors.clone(),
                )
            }
            .into_any()
        })
        .collect();

    element! {
        Box(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            background_color: colors_bg(colors),
        ) {
            #(row_children)
        }
    }
    .into_any()
}

/// Render the "Page X of Y" line when paging.
fn page_position_element(view: &WorkbenchView) -> AnyElement<'static> {
    let line = format!(
        "Page {} of {}",
        view.layout.page + 1,
        view.layout.page_count
    );
    element! {
        Box(height: 1u32, padding_left: 1u32) {
            Text(content: line, color: Color::DarkGrey)
        }
    }
    .into_any()
}
/// Extract the background color from theme colors for inline use.
fn colors_bg(colors: &ThemeColors) -> Color {
    ResolvedColors::from_theme(Some(colors)).bg
}
