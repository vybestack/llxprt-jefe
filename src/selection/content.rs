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
use crate::state::{AppState, DashboardGrabPane};
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
        SelectablePane::CloseReasonChooser => overlay_content::close_reason_chooser_lines(state),
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
    let (list_pane_rows, _) = crate::layout::issues_pane_rows(
        usize::from(term_rows),
        state.issues_state.error.is_some(),
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
/// mode, reusing the pure [`keybind_hints_for`] projection.
fn keybind_bar_lines(state: &AppState) -> PaneContent {
    let hints = crate::ui::components::keybind_bar::keybind_hints_for(state.screen_mode, false);
    PaneContent::new(SelectablePane::KeybindBar, vec![hints.to_string()])
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
mod tests {
    use super::*;

    #[test]
    fn pane_content_empty_has_no_lines() {
        let c = PaneContent::empty(SelectablePane::Sidebar);
        assert!(c.lines.is_empty());
        assert!(matches!(c.pane, SelectablePane::Sidebar));
    }

    #[test]
    fn pane_content_new_collects_lines() {
        let c = PaneContent::new(
            SelectablePane::IssueList,
            vec!["a".to_string(), "b".to_string()],
        );
        assert_eq!(c.lines, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn terminal_lines_from_snapshot() {
        use crate::runtime::{TerminalCell, TerminalCellStyle};
        use iocraft::Color;
        let style = TerminalCellStyle {
            fg: Color::White,
            bg: Color::Black,
            bold: false,
            dim: false,
            underline: false,
        };
        let cells = vec![
            vec![
                TerminalCell {
                    ch: 'h',
                    style,
                    wide_spacer: false,
                },
                TerminalCell {
                    ch: 'i',
                    style,
                    wide_spacer: false,
                },
            ],
            // Second line has a width-2 glyph '中' + its trailing spacer, then '!'.
            // The spacer cell must be filtered out so the line reads "中!" (issue #197).
            vec![
                TerminalCell {
                    ch: '中',
                    style,
                    wide_spacer: false,
                },
                TerminalCell {
                    ch: ' ',
                    style,
                    wide_spacer: true,
                },
                TerminalCell {
                    ch: '!',
                    style,
                    wide_spacer: false,
                },
            ],
        ];
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 3,
            cells,
            wraps: Vec::new(),
        };
        let content = pane_content_lines(
            SelectablePane::TerminalView,
            &AppState::default(),
            Some(&snap),
            &[],
            120,
            40,
        );
        assert_eq!(content.lines, vec!["hi".to_string(), "中!".to_string()]);
    }

    #[test]
    fn terminal_lines_none_snapshot_shows_placeholder() {
        let content = pane_content_lines(
            SelectablePane::TerminalView,
            &AppState::default(),
            None,
            &[],
            120,
            40,
        );
        assert_eq!(content.lines, vec!["No terminal attached".to_string()]);
    }

    /// A Running selected agent with no snapshot yet shows the reassuring
    /// "session live" hint rather than the misleading "No terminal attached"
    /// (issue #160).
    #[test]
    fn terminal_lines_none_snapshot_running_agent_shows_session_live() {
        use crate::domain::{Agent, AgentId, Repository, RepositoryId};
        let repo_id = RepositoryId("r1".to_string());
        let mut state = AppState::default();
        state.repositories.push(Repository::new(
            repo_id.clone(),
            "repo".to_string(),
            "repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        let agent_id = AgentId("a1".to_string());
        let mut agent = Agent::new(
            agent_id,
            repo_id,
            "agent".to_string(),
            std::path::PathBuf::from("/tmp/agent"),
        );
        agent.status = AgentStatus::Running;
        state.agents.push(agent);
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);

        let content = pane_content_lines(SelectablePane::TerminalView, &state, None, &[], 120, 40);
        assert_eq!(
            content.lines,
            vec!["Session live - press t to focus terminal".to_string()]
        );
    }

    #[test]
    fn sidebar_lines_include_selection_prefix() {
        use crate::domain::{AgentId, Repository, RepositoryId};
        let mut state = AppState::default();
        state.repositories.push(Repository {
            id: RepositoryId("r1".to_string()),
            name: "repo-one".to_string(),
            slug: "repo-one".to_string(),
            base_dir: std::path::PathBuf::new(),
            default_profile: String::new(),
            default_code_puppy_model: String::new(),
            github_repo: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            default_agent_kind: crate::domain::AgentKind::Llxprt,
            agent_ids: vec![AgentId("a1".to_string()), AgentId("a2".to_string())],
        });
        // Select the first repo so the rendered "> " prefix appears.
        state.selected_repository_index = Some(0);
        let content = pane_content_lines(SelectablePane::Sidebar, &state, None, &[], 120, 40);
        // Selected repo gets "> " prefix; matches the Sidebar renderer.
        assert_eq!(content.lines, vec!["> repo-one (0)".to_string()]);
    }

    #[test]
    fn pr_list_lines_match_rendered_projection_with_prefix() {
        use crate::domain::{PrCheckStatus, PrState, PullRequest};
        let mut state = AppState::default();
        state.prs_state.list.replace_items(vec![PullRequest {
            number: 7,
            title: "A title".to_string(),
            state: PrState::Open,
            author_login: "octocat".to_string(),
            updated_at: String::new(),
            head_ref: String::new(),
            base_ref: String::new(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        }]);
        state.prs_state.list.set_selected_index(Some(0));
        let content = pane_content_lines(SelectablePane::PrList, &state, None, &[], 120, 40);
        // Compact mode: one line per PR, with the "> " selected prefix and #N.
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].starts_with("> #7 "));
    }

    #[test]
    fn issue_list_lines_match_rendered_projection_with_prefix() {
        use crate::domain::{Issue, IssueState};
        let mut state = AppState::default();
        state.issues_state.list.items_mut().push(Issue {
            number: 3,
            node_id: String::new(),
            title: "Bug".to_string(),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            updated_at: String::new(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
        });
        state.issues_state.list.set_selected_index(Some(0));
        let content = pane_content_lines(SelectablePane::IssueList, &state, None, &[], 120, 40);
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].starts_with("> #3 "));
    }

    #[test]
    fn status_bar_lines_match_rendered_left_and_center() {
        let content = pane_content_lines(
            SelectablePane::StatusBar,
            &AppState::default(),
            None,
            &[],
            120,
            40,
        );
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].contains("LLxprt Jefe -"));
        assert!(content.lines[0].contains("repos |"));
    }

    #[test]
    fn status_bar_lines_show_kennel_mode_for_selected_code_puppy_agent() {
        let repo_id = crate::domain::RepositoryId("kennel-repo".to_owned());
        let mut state = AppState::default();
        state.repositories.push(crate::domain::Repository::new(
            repo_id.clone(),
            "Kennel Repo".to_owned(),
            "kennel".to_owned(),
            std::path::PathBuf::from("/tmp/kennel"),
        ));
        let mut agent = crate::domain::Agent::new(
            crate::domain::AgentId("puppy".to_owned()),
            repo_id,
            "Puppy".to_owned(),
            std::path::PathBuf::from("/tmp/kennel/puppy"),
        );
        agent.agent_kind = crate::domain::AgentKind::CodePuppy;
        state.agents.push(agent);
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);

        let content = pane_content_lines(SelectablePane::StatusBar, &state, None, &[], 120, 40);
        assert!(content.lines[0].contains("LLxprt Jefe (Kennel mode) -"));
    }
    #[test]
    fn keybind_bar_lines_match_rendered_hints() {
        let mut state = AppState::default();
        state.screen_mode = crate::state::ScreenMode::Dashboard;
        let content = pane_content_lines(SelectablePane::KeybindBar, &state, None, &[], 120, 40);
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].contains("navigate"));
    }

    #[test]
    fn issue_detail_lines_start_with_header_rows() {
        use crate::domain::{IssueDetail, IssueState};
        let mut state = AppState::default();
        state.issues_state.issue_detail = Some(IssueDetail {
            repo_owner_name: "o/r".to_string(),
            number: 42,
            node_id: String::new(),
            title: "My Issue".to_string(),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-02-01".to_string(),
            labels: vec!["bug".to_string()],
            assignees: vec!["alice".to_string()],
            milestone: Some("v1".to_string()),
            body: "Body text".to_string(),
            external_url: "https://example.com/42".to_string(),
            comments: crate::domain::PaginatedList::from_loaded(
                crate::domain::CommentDetailIdentity {
                    scope_repo_id: crate::domain::RepositoryId::default(),
                    number: 42,
                },
                Vec::new(),
                crate::domain::PageToken::from_cursor(None, false),
            ),
        });
        let content = pane_content_lines(SelectablePane::IssueDetail, &state, None, &[], 120, 40);
        // Line 0: title, Line 1: state/author, Line 2: labels/assignees/milestone,
        // Line 3: url, Line 4: separator, then scrollable content lines.
        assert!(content.lines.len() > 5);
        assert_eq!(content.lines[0], "#42 My Issue");
        assert!(content.lines[1].contains("OPEN"));
        assert!(content.lines[1].contains("@octocat"));
        assert!(content.lines[2].contains("labels: bug"));
        assert!(content.lines[2].contains("assignees: alice"));
        assert!(content.lines[2].contains("milestone: v1"));
        assert_eq!(content.lines[3], "https://example.com/42");
        assert!(content.lines[4].starts_with('─'));
    }

    #[test]
    fn pr_detail_lines_start_with_header_rows() {
        use crate::domain::{PrCheckStatus, PrState, PullRequestDetail};
        let mut state = AppState::default();
        state.prs_state.pr_detail = Some(PullRequestDetail {
            repo_owner_name: "o/r".to_string(),
            number: 7,
            title: "My PR".to_string(),
            state: PrState::Open,
            is_draft: false,
            author_login: "octocat".to_string(),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-02-01".to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            labels: vec!["enhancement".to_string()],
            assignees: vec!["bob".to_string()],
            milestone: None,
            body: "PR body".to_string(),
            external_url: "https://example.com/pull/7".to_string(),
            review_decision: None,
            checks_status: PrCheckStatus::None,
            reviews: Vec::new(),
            checks: Vec::new(),
            comments: crate::domain::PaginatedList::from_loaded(
                crate::domain::CommentDetailIdentity {
                    scope_repo_id: crate::domain::RepositoryId::default(),
                    number: 7,
                },
                Vec::new(),
                crate::domain::PageToken::from_cursor(None, false),
            ),
            mergeable: None,
            merge_state_status: None,
        });
        let content = pane_content_lines(SelectablePane::PrDetail, &state, None, &[], 120, 40);
        // Header rows first, then scrollable content.
        assert!(content.lines.len() > 5);
        assert_eq!(content.lines[0], "#7 My PR");
        assert!(content.lines[1].contains("OPEN"));
        assert!(content.lines[1].contains("octocat"));
        assert!(content.lines[2].contains("feature --> main"));
        assert!(content.lines[2].contains("labels: enhancement"));
        assert!(content.lines[2].contains("assignees: bob"));
        assert_eq!(content.lines[3], "https://example.com/pull/7");
        assert!(content.lines[4].starts_with('─'));
    }

    // ── Issue #178: select-to-copy for forms, choosers, confirm, and help ──

    #[test]
    fn help_modal_lines_match_help_content_projection() {
        let content = pane_content_lines(
            SelectablePane::HelpModal,
            &AppState::default(),
            None,
            &[],
            120,
            40,
        );
        // help_lines() must project the actual help content (issue #178: it
        // was returning an empty Vec).
        assert!(
            !content.lines.is_empty(),
            "help modal must have copyable content"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("Navigation")),
            "help modal content must include the Navigation section"
        );
    }

    #[test]
    fn agent_form_lines_include_title_and_fields() {
        use crate::domain::RepositoryId;
        use crate::state::{AgentFormFields, ModalState};
        let mut state = AppState::default();
        state.modal = ModalState::NewAgent {
            repository_id: RepositoryId("r1".to_string()),
            fields: AgentFormFields {
                name: "my-agent".to_string(),
                ..Default::default()
            },
            focus: crate::state::AgentFormFocus::Name,
            cursor: crate::state::AgentFormCursor::default(),
            work_dir_manual: false,
        };
        let content = pane_content_lines(SelectablePane::AgentForm, &state, None, &[], 120, 40);
        assert!(
            content.lines.iter().any(|l| l.contains("New Agent")),
            "agent form must include the title"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("my-agent")),
            "agent form must include the agent name field value"
        );
    }

    #[test]
    fn agent_form_lines_empty_when_no_modal() {
        let state = AppState::default();
        let content = pane_content_lines(SelectablePane::AgentForm, &state, None, &[], 120, 40);
        assert!(
            content.lines.is_empty(),
            "agent form with no modal should have no content"
        );
    }

    #[test]
    fn repository_form_lines_include_title_and_fields() {
        use crate::state::{ModalState, RepositoryFormFields};
        let mut state = AppState::default();
        state.modal = ModalState::NewRepository {
            fields: RepositoryFormFields {
                name: "my-repo".to_string(),
                ..Default::default()
            },
            focus: crate::state::RepositoryFormFocus::Name,
            cursor: crate::state::RepositoryFormCursor::default(),
        };
        let content =
            pane_content_lines(SelectablePane::RepositoryForm, &state, None, &[], 120, 40);
        assert!(
            content.lines.iter().any(|l| l.contains("New Repository")),
            "repository form must include the title"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("my-repo")),
            "repository form must include the repo name field value"
        );
    }

    #[test]
    fn agent_chooser_lines_include_header_and_agent_names() {
        use crate::domain::{Agent, AgentId, Repository, RepositoryId};
        let mut state = AppState::default();
        // Add a repository and two agents so the chooser has entries.
        let repo_id = RepositoryId("r1".to_string());
        state.repositories.push(Repository::new(
            repo_id.clone(),
            "repo".to_string(),
            "repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        state.agents.push(Agent::new(
            AgentId("a1".to_string()),
            repo_id.clone(),
            "alpha".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));
        state.agents.push(Agent::new(
            AgentId("a2".to_string()),
            repo_id,
            "beta".to_string(),
            std::path::PathBuf::from("/tmp/a2"),
        ));
        state.selected_repository_index = Some(0);
        // Open the agent chooser from issues state.
        state.issues_state.agent_chooser = Some(crate::state::AgentChooserState {
            selected_index: 0,
            agents: vec![
                (AgentId("a1".to_string()), "alpha".to_string()),
                (AgentId("a2".to_string()), "beta".to_string()),
            ],
        });
        let content = pane_content_lines(SelectablePane::AgentChooser, &state, None, &[], 120, 40);
        assert!(
            content.lines.iter().any(|l| l.contains("Send to Agent")),
            "agent chooser must include header"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("alpha")),
            "agent chooser must list agent names"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("beta")),
            "agent chooser must list agent names"
        );
    }

    #[test]
    fn merge_chooser_lines_include_header_and_methods() {
        use crate::domain::{PrCheckStatus, PrState, PullRequestDetail};
        let mut state = AppState::default();
        state.prs_state.merge_chooser = Some(crate::state::PrMergeChooserState {
            selected_index: 0,
            allowed_methods: None,
            awaiting_confirmation: false,
        });
        // Merge chooser needs a PR number for the header.
        state.prs_state.pr_detail = Some(PullRequestDetail {
            repo_owner_name: "o/r".to_string(),
            number: 42,
            title: "T".to_string(),
            state: PrState::Open,
            is_draft: false,
            author_login: "x".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            head_ref: String::new(),
            base_ref: String::new(),
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            body: String::new(),
            external_url: String::new(),
            review_decision: None,
            checks_status: PrCheckStatus::None,
            reviews: Vec::new(),
            checks: Vec::new(),
            comments: crate::domain::PaginatedList::from_loaded(
                crate::domain::CommentDetailIdentity {
                    scope_repo_id: crate::domain::RepositoryId::default(),
                    number: 42,
                },
                Vec::new(),
                crate::domain::PageToken::from_cursor(None, false),
            ),
            mergeable: None,
            merge_state_status: None,
        });
        let content = pane_content_lines(SelectablePane::MergeChooser, &state, None, &[], 120, 40);
        assert!(
            content
                .lines
                .iter()
                .any(|l| l.contains("Merge Pull Request #42")),
            "merge chooser must include PR number header"
        );
        assert!(
            content
                .lines
                .iter()
                .any(|l| l.contains("Create a merge commit")),
            "merge chooser must list merge methods"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("Squash and merge")),
            "merge chooser must list merge methods"
        );
    }

    #[test]
    fn confirm_modal_lines_include_title_and_message() {
        use crate::domain::{Agent, AgentId, Repository, RepositoryId};
        use crate::state::ModalState;
        let mut state = AppState::default();
        let repo_id = RepositoryId("r1".to_string());
        state.repositories.push(Repository::new(
            repo_id.clone(),
            "repo".to_string(),
            "repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        let agent_id = AgentId("a1".to_string());
        state.agents.push(Agent::new(
            agent_id.clone(),
            repo_id,
            "my-agent".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));
        state.modal = ModalState::ConfirmDeleteAgent {
            id: agent_id,
            delete_work_dir: false,
            confirm_focus: crate::state::ConfirmFocus::Cancel,
        };
        let content = pane_content_lines(SelectablePane::ConfirmModal, &state, None, &[], 120, 40);
        assert!(
            content.lines.iter().any(|l| l.contains("Delete Agent")),
            "confirm modal must include the title"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("my-agent")),
            "confirm modal must include the message with the agent name"
        );
    }
}
