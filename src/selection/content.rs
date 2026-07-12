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

use crate::domain::{AgentStatus, IssueDetail};
use crate::issue_detail_content::build_detail_content;
use crate::pr_detail_content::build_pr_detail_content;
use crate::runtime::TerminalSnapshot;
use crate::selection::SelectablePane;
use crate::state::AppState;
use crate::ui::components::issue_detail::issue_detail_header_view;
use crate::ui::components::issue_list::{IssueListLayout, issue_list_visible_rows};
use crate::ui::components::pr_detail::pr_detail_header_view;
use crate::ui::components::pr_list::pr_list_visible_rows;
use crate::ui::components::terminal_empty_message;
use crate::ui::modals::help_content_lines;

use crate::selection::form_content;
use crate::selection::overlay_content;

/// Separator line rendered between the fixed detail header and the scrollable
/// content, mirroring the components' `"─────…"` row.
const DETAIL_SEPARATOR_LINE: &str = "─────────────────────────────────────────";

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
    match pane {
        SelectablePane::IssueDetail => issue_detail_lines(state),
        SelectablePane::PrDetail => pr_detail_lines(state),
        SelectablePane::IssueList => issue_list_lines(state, term_cols, term_rows),
        SelectablePane::PrList => pr_list_lines(state, term_cols, term_rows),
        SelectablePane::ActionsList | SelectablePane::ActionsDetail => PaneContent::empty(pane),
        SelectablePane::Sidebar => sidebar_lines(state),
        SelectablePane::AgentList => agent_list_lines(state),
        SelectablePane::Preview => preview_lines(state),
        SelectablePane::TerminalView => terminal_lines(snapshot, state, history_lines),
        SelectablePane::HelpModal => help_lines(),
        SelectablePane::StatusBar => status_bar_lines(state),
        SelectablePane::KeybindBar => keybind_bar_lines(state),
        SelectablePane::AgentForm => agent_form_lines(state),
        SelectablePane::RepositoryForm => repository_form_lines(state),
        SelectablePane::AgentChooser => overlay_content::agent_chooser_lines(state),
        SelectablePane::MergeChooser => overlay_content::merge_chooser_lines(state),
        SelectablePane::PropertyEditor => overlay_content::property_editor_lines(state),
        SelectablePane::ConfirmModal => overlay_content::confirm_modal_lines(state),
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
    let mut lines = issue_detail_header_lines(detail);
    lines.extend(content.text.lines().map(String::from));
    PaneContent::new(SelectablePane::IssueDetail, lines)
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
    let header = pr_detail_header_view(detail);
    let mut lines = vec![
        header.title,
        header.state,
        header.branches,
        header.url,
        DETAIL_SEPARATOR_LINE.to_string(),
    ];
    lines.extend(content.text.lines().map(String::from));
    PaneContent::new(SelectablePane::PrDetail, lines)
}

/// Build the five fixed header lines the `IssueDetailView` renders above its
/// scrollable viewport, so selection coordinates map to those header rows too.
///
/// Reuses [`issue_detail_header_view`] (the same pure projection the renderer
/// uses) so the copyable header text never drifts from what the user sees.
fn issue_detail_header_lines(detail: &IssueDetail) -> Vec<String> {
    let header = issue_detail_header_view(detail);
    vec![
        header.title,
        header.state,
        header.labels,
        header.url,
        DETAIL_SEPARATOR_LINE.to_string(),
    ]
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
        let content = pane_content_lines(SelectablePane::PrList, &state, None, &[], 120, 40);
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
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
            issue_type_name: None,
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
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
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
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
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
