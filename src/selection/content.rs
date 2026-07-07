//! Pane content providers for mouse selection.
//!
//! Each [`crate::selection::SelectablePane`] renders some text; to copy a
//! selection we need that text as a flat `Vec<String>` of content lines. This
//! module owns the pure mapping from [`crate::state::AppState`] data to those
//! lines, reusing the existing pure projection builders
//! ([`crate::issue_detail_content::build_detail_content`],
//! [`crate::pr_detail_content::build_pr_detail_content`]) so the copyable text
//! matches what the user sees.
//!
//! All functions are pure and `#[must_use]`. The terminal snapshot is passed in
//! explicitly (it lives on the runtime, not AppState) so the module stays
//! iocraft-free and side-effect-free.

use crate::domain::{Agent, AgentStatus, Issue, PullRequest};
use crate::issue_detail_content::build_detail_content;
use crate::pr_detail_content::build_pr_detail_content;
use crate::runtime::TerminalSnapshot;
use crate::selection::SelectablePane;
use crate::state::AppState;

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
/// because it lives on the runtime, not AppState.
///
/// # Panics
///
/// Never; all indexing is bounds-checked.
#[must_use]
pub fn pane_content_lines(
    pane: SelectablePane,
    state: &AppState,
    snapshot: Option<&TerminalSnapshot>,
) -> PaneContent {
    match pane {
        SelectablePane::IssueDetail => issue_detail_lines(state),
        SelectablePane::PrDetail => pr_detail_lines(state),
        SelectablePane::IssueList => issue_list_lines(state),
        SelectablePane::PrList => pr_list_lines(state),
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

fn issue_list_lines(state: &AppState) -> PaneContent {
    let lines: Vec<String> = state
        .issues_state
        .issues
        .iter()
        .flat_map(issue_to_lines)
        .collect();
    PaneContent::new(SelectablePane::IssueList, lines)
}

fn pr_list_lines(state: &AppState) -> PaneContent {
    let lines: Vec<String> = state
        .prs_state
        .pull_requests
        .iter()
        .flat_map(pr_to_lines)
        .collect();
    PaneContent::new(SelectablePane::PrList, lines)
}

fn issue_to_lines(issue: &Issue) -> Vec<String> {
    vec![
        format!("#{} {}", issue.number, issue.title),
        format!(
            "  @{} updated:{} comments:{}",
            issue.author_login, issue.updated_at, issue.comment_count
        ),
    ]
}

fn pr_to_lines(pr: &PullRequest) -> Vec<String> {
    vec![format!("#{} {}", pr.number, pr.title)]
}

fn sidebar_lines(state: &AppState) -> PaneContent {
    let lines: Vec<String> = state
        .repositories
        .iter()
        .map(|repo| format!("{} ({})", repo.name, repo.agent_ids.len()))
        .collect();
    PaneContent::new(SelectablePane::Sidebar, lines)
}

fn agent_list_lines(state: &AppState) -> PaneContent {
    let Some(repo) = state.selected_repository() else {
        return PaneContent::empty(SelectablePane::AgentList);
    };
    let agents = state.visible_agents_for_repository(&repo.id);
    let lines: Vec<String> = agents.iter().map(agent_to_line).collect();
    PaneContent::new(SelectablePane::AgentList, lines)
}

fn agent_to_line(agent: &Agent) -> String {
    let icon = status_icon(agent.status);
    let shortcut = agent
        .shortcut_slot
        .map_or_else(String::new, |slot| format!("[{slot}] "));
    format!("{icon} {}{}", shortcut, agent.name)
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

fn status_bar_lines(state: &AppState) -> PaneContent {
    let repo_count = state.repositories.len();
    let running = state.agents.iter().filter(|a| a.is_running()).count();
    let line = format!(
        "jefe {} | repos: {} | running: {}",
        crate::VERSION,
        repo_count,
        running
    );
    PaneContent::new(SelectablePane::StatusBar, vec![line])
}

fn keybind_bar_lines(state: &AppState) -> PaneContent {
    // The keybind bar content varies by screen mode; expose the mode label so
    // at least the visible hint text is copyable.
    let label = match state.screen_mode {
        crate::state::ScreenMode::Dashboard => "Dashboard",
        crate::state::ScreenMode::Split => "Split",
        crate::state::ScreenMode::DashboardIssues => "Issues",
        crate::state::ScreenMode::DashboardPullRequests => "Pull Requests",
    };
    PaneContent::new(SelectablePane::KeybindBar, vec![label.to_string()])
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
        );
        assert_eq!(content.lines, vec!["hi".to_string(), "!".to_string()]);
    }

    #[test]
    fn terminal_lines_none_snapshot_shows_placeholder() {
        let content = pane_content_lines(SelectablePane::TerminalView, &AppState::default(), None);
        assert_eq!(content.lines, vec!["No terminal attached".to_string()]);
    }

    #[test]
    fn sidebar_lines_count_repos() {
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
        let content = pane_content_lines(SelectablePane::Sidebar, &state, None);
        assert_eq!(content.lines, vec!["repo-one (2)".to_string()]);
    }
}
