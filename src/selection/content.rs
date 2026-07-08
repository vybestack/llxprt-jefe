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
use crate::issue_detail_content::build_detail_content;
use crate::pr_detail_content::build_pr_detail_content;
use crate::runtime::TerminalSnapshot;
use crate::selection::SelectablePane;
use crate::state::AppState;
use crate::ui::components::issue_list::{IssueListLayout, issue_list_visible_rows};
use crate::ui::components::pr_list::pr_list_visible_rows;

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
    term_cols: u16,
    term_rows: u16,
) -> PaneContent {
    match pane {
        SelectablePane::IssueDetail => issue_detail_lines(state),
        SelectablePane::PrDetail => pr_detail_lines(state),
        SelectablePane::IssueList => issue_list_lines(state, term_cols, term_rows),
        SelectablePane::PrList => pr_list_lines(state, term_cols, term_rows),
        SelectablePane::Sidebar => sidebar_lines(state),
        SelectablePane::AgentList => agent_list_lines(state),
        SelectablePane::Preview => preview_lines(state),
        SelectablePane::TerminalView => terminal_lines(snapshot),
        SelectablePane::HelpModal => help_lines(),
        SelectablePane::StatusBar => status_bar_lines(state),
        SelectablePane::KeybindBar => keybind_bar_lines(state),
    }
}

fn issue_detail_lines(state: &AppState) -> PaneContent {
    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        return PaneContent::empty(SelectablePane::IssueDetail);
    };
    let content = build_detail_content(
        detail,
        state.issues_state.detail_subfocus,
        &state.issues_state.inline_state,
        state.issues_state.loading.comments,
    );
    PaneContent::new(
        SelectablePane::IssueDetail,
        content.text.lines().map(String::from),
    )
}

fn pr_detail_lines(state: &AppState) -> PaneContent {
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        return PaneContent::empty(SelectablePane::PrDetail);
    };
    let content = build_pr_detail_content(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    );
    PaneContent::new(
        SelectablePane::PrDetail,
        content.text.lines().map(String::from),
    )
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
    let available_width = crate::layout::issue_list_content_width(term_cols);
    let rows = issue_list_visible_rows(
        &state.issues_state.issues,
        state.issues_state.selected_issue_index,
        list_pane_rows,
        IssueListLayout::Compact,
        Some(available_width),
    );
    PaneContent::new(
        SelectablePane::IssueList,
        rows.into_iter().map(|r| r.title_line),
    )
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
    let available_width = crate::layout::pr_list_content_width(term_cols);
    let rows = pr_list_visible_rows(
        &state.prs_state.pull_requests,
        state.prs_state.selected_pr_index,
        list_pane_rows,
        Some(available_width),
    );
    PaneContent::new(
        SelectablePane::PrList,
        rows.into_iter().map(|r| r.title_line),
    )
}

/// Sidebar lines that match the rendered repo list, including the `> `/`  `
/// selection prefix and `(count)` suffix.
fn sidebar_lines(state: &AppState) -> PaneContent {
    let visible_indices = state.visible_repository_indices();
    let selected_visible = state.selected_repository_visible_index();
    let lines: Vec<String> = visible_indices
        .iter()
        .enumerate()
        .filter_map(|(vis_i, &repo_i)| {
            let repo = state.repositories.get(repo_i)?;
            let count = state.visible_agent_count_for_repository(&repo.id);
            let prefix = if selected_visible == Some(vis_i) {
                "> "
            } else {
                "  "
            };
            Some(format!("{prefix}{} ({count})", repo.name))
        })
        .collect();
    PaneContent::new(SelectablePane::Sidebar, lines)
}

/// Agent list lines that match the rendered agent list, including the
/// status icon, `> `/`  ` prefix, optional `[slot]`, and name.
fn agent_list_lines(state: &AppState) -> PaneContent {
    let Some(repo) = state.selected_repository() else {
        return PaneContent::empty(SelectablePane::AgentList);
    };
    let agents = state.visible_agents_for_repository(&repo.id);
    let selected_local = state.selected_agent_local_index().unwrap_or(0);
    let lines: Vec<String> = agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let icon = status_icon(agent.status);
            let prefix = if i == selected_local { "> " } else { "  " };
            let shortcut = agent
                .shortcut_slot
                .map_or_else(String::new, |slot| format!("[{slot}] "));
            format!("{prefix}{icon} {shortcut}{}", agent.name)
        })
        .collect();
    PaneContent::new(SelectablePane::AgentList, lines)
}

fn status_icon(status: AgentStatus) -> char {
    match status {
        AgentStatus::Running => '*',
        AgentStatus::Completed => '+',
        AgentStatus::Dead => 'x',
        AgentStatus::Errored => '!',
        AgentStatus::Waiting => '?',
        AgentStatus::Paused => '-',
        AgentStatus::Queued => 'o',
    }
}

fn preview_lines(state: &AppState) -> PaneContent {
    let Some(agent) = state.selected_agent() else {
        return PaneContent::new(
            SelectablePane::Preview,
            vec!["No agent selected".to_string()],
        );
    };
    let lines = vec![
        format!("Name: {}", agent.name),
        format!("Status: {:?}", agent.status),
        format!("Dir: {}", agent.work_dir.display()),
        "Todo:".to_string(),
        "  (no tasks)".to_string(),
    ];
    PaneContent::new(SelectablePane::Preview, lines)
}

fn terminal_lines(snapshot: Option<&TerminalSnapshot>) -> PaneContent {
    let Some(snap) = snapshot else {
        return PaneContent::new(
            SelectablePane::TerminalView,
            vec!["No terminal attached".to_string()],
        );
    };
    let lines: Vec<String> = (0..snap.rows)
        .map(|row| {
            snap.cells.get(row).map_or_else(String::new, |cells| {
                cells.iter().take(snap.cols).map(|c| c.ch).collect()
            })
        })
        .collect();
    PaneContent::new(SelectablePane::TerminalView, lines)
}

fn help_lines() -> PaneContent {
    PaneContent::new(SelectablePane::HelpModal, Vec::<String>::new())
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
    let left = format!("LLxprt Jefe - {}", crate::VERSION);
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
            underline: false,
        };
        let cells = vec![
            vec![
                TerminalCell { ch: 'h', style },
                TerminalCell { ch: 'i', style },
            ],
            vec![TerminalCell { ch: '!', style }],
        ];
        let snap = TerminalSnapshot {
            rows: 2,
            cols: 2,
            cells,
        };
        let content = pane_content_lines(
            SelectablePane::TerminalView,
            &AppState::default(),
            Some(&snap),
            120,
            40,
        );
        assert_eq!(content.lines, vec!["hi".to_string(), "!".to_string()]);
    }

    #[test]
    fn terminal_lines_none_snapshot_shows_placeholder() {
        let content = pane_content_lines(
            SelectablePane::TerminalView,
            &AppState::default(),
            None,
            120,
            40,
        );
        assert_eq!(content.lines, vec!["No terminal attached".to_string()]);
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
            github_repo: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            agent_ids: vec![AgentId("a1".to_string()), AgentId("a2".to_string())],
        });
        // Select the first repo so the rendered "> " prefix appears.
        state.selected_repository_index = Some(0);
        let content = pane_content_lines(SelectablePane::Sidebar, &state, None, 120, 40);
        // Selected repo gets "> " prefix; matches the Sidebar renderer.
        assert_eq!(content.lines, vec!["> repo-one (0)".to_string()]);
    }

    #[test]
    fn pr_list_lines_match_rendered_projection_with_prefix() {
        use crate::domain::{PrCheckStatus, PrState, PullRequest};
        let mut state = AppState::default();
        state.prs_state.pull_requests.push(PullRequest {
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
        });
        state.prs_state.selected_pr_index = Some(0);
        let content = pane_content_lines(SelectablePane::PrList, &state, None, 120, 40);
        // Compact mode: one line per PR, with the "> " selected prefix and #N.
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].starts_with("> #7 "));
    }

    #[test]
    fn issue_list_lines_match_rendered_projection_with_prefix() {
        use crate::domain::{Issue, IssueState};
        let mut state = AppState::default();
        state.issues_state.issues.push(Issue {
            number: 3,
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
        state.issues_state.selected_issue_index = Some(0);
        let content = pane_content_lines(SelectablePane::IssueList, &state, None, 120, 40);
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].starts_with("> #3 "));
    }

    #[test]
    fn status_bar_lines_match_rendered_left_and_center() {
        let content = pane_content_lines(
            SelectablePane::StatusBar,
            &AppState::default(),
            None,
            120,
            40,
        );
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].contains("LLxprt Jefe -"));
        assert!(content.lines[0].contains("repos |"));
    }

    #[test]
    fn keybind_bar_lines_match_rendered_hints() {
        let mut state = AppState::default();
        state.screen_mode = crate::state::ScreenMode::Dashboard;
        let content = pane_content_lines(SelectablePane::KeybindBar, &state, None, 120, 40);
        assert_eq!(content.lines.len(), 1);
        assert!(content.lines[0].contains("navigate"));
    }
}
