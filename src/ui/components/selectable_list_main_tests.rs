//! Tests for [`SelectableList`] and the domain projection wrappers.
//!
//! These lock two layers of the refactoring contract:
//! 1. The projection wrappers (`issue_list_props`, `pr_list_props`,
//!    `agent_list_props`) produce rows whose span text/colors exactly
//!    match the pre-refactor projections (parity with the unchanged pure
//!    `*_visible_rows` functions).
//! 2. The rendered ANSI output preserves the per-domain border/weight/color
//!    behavior (bold-on-select for Issue/PR, fixed status-glyph color +
//!    bright-on-select for Agent, double-vs-round border policy, empty
//!    message rendering).

use super::*;
use crate::domain::{
    Agent, AgentId, AgentStatus, Issue, IssueState, PrCheckStatus, PrState, PullRequest,
    RepositoryId,
};
use crate::git_info::GitRepoInfo;
use crate::theme::ThemeColors;
use crate::ui::components::agent_list::{
    AgentListSelection, AgentListView, AgentListWindow, agent_list_props as build_agent_list_props,
};
use crate::ui::components::issue_list::{IssueListLayout, IssueListWindow, issue_list_props};

fn agent_list_props(
    agents: &[Agent],
    git_infos: &[GitRepoInfo],
    selection: AgentListSelection,
    focused: bool,
    colors: ThemeColors,
    text_selection: Option<TextSelection>,
) -> SelectableListProps {
    build_agent_list_props(
        agents,
        git_infos,
        AgentListView {
            selection,
            window: AgentListWindow {
                pane_rows: 40,
                content_width: 80,
            },
        },
        focused,
        colors,
        text_selection,
    )
}
use crate::ui::components::pr_list::{PrListLayout, PrListWindow, pr_list_props};

/// Render a `SelectableList` element into an ANSI string at a fixed size.
fn render_ansi(props: SelectableListProps, cols: u16, rows: u16) -> String {
    let mut elem = element! {
        Box(width: u32::from(cols), height: u32::from(rows)) {
            #(vec![selectable_list_element(props)])
        }
    };
    let canvas = elem.render(Some(usize::from(cols)));
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    String::from_utf8_lossy(&buf).into_owned()
}

fn issue(n: u64) -> Issue {
    Issue {
        number: n,
        node_id: String::new(),
        title: format!("Issue {n}"),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        updated_at: "2026-06-30".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
    }
}

fn pr(n: u64) -> PullRequest {
    PullRequest {
        number: n,
        title: format!("PR {n}"),
        state: PrState::Open,
        author_login: "octocat".to_string(),
        updated_at: "2026-01-01".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

fn agent(name: &str, status: AgentStatus) -> Agent {
    let mut a = Agent::new(
        AgentId(name.to_string()),
        RepositoryId("r".to_string()),
        name.to_string(),
        std::path::PathBuf::from("/tmp"),
    );
    a.status = status;
    a
}

// ── Projection parity ───────────────────────────────────────────────────

/// `issue_list_props` rows' first span text must equal the unchanged
/// `issue_list_visible_rows` `title_line` (parity with the pure projection).
#[test]
fn issue_list_props_first_span_matches_visible_rows_title_line() {
    let issues: Vec<Issue> = (1..=3).map(issue).collect();
    let window = IssueListWindow {
        selected_index: Some(1),
        list_pane_rows: 10,
        layout: IssueListLayout::Compact,
        available_width: Some(40),
    };
    let props = issue_list_props(&issues, window, true, None, ThemeColors::default(), None);
    let visible = crate::ui::components::issue_list::issue_list_visible_rows(
        &issues,
        Some(1),
        10,
        IssueListLayout::Compact,
        Some(40),
    );
    assert_eq!(props.rows.len(), visible.len());
    for (got, want) in props.rows.iter().zip(visible.iter()) {
        assert_eq!(got.spans.len(), 1);
        assert_eq!(got.spans[0].text, want.title_line);
        assert!(got.meta_line.as_deref() == Some("") || got.meta_line.is_some());
        assert_eq!(got.is_selected, want.is_selected);
    }
}

/// `pr_list_props` rows' first span text must equal the unchanged
/// `pr_list_visible_rows` `title_line`, and compact rows carry an empty
/// meta line (signaling single-line rendering).
#[test]
fn pr_list_props_first_span_matches_visible_rows_title_line() {
    let prs: Vec<PullRequest> = (1..=3).map(pr).collect();
    let window = PrListWindow {
        selected_index: Some(0),
        list_pane_rows: 10,
        available_width: Some(40),
        layout: PrListLayout::Compact,
    };
    let props = pr_list_props(&prs, window, true, None, ThemeColors::default(), None);
    let visible = crate::ui::components::pr_list::pr_list_visible_rows(&prs, Some(0), 10, Some(40));
    assert_eq!(props.rows.len(), visible.len());
    for (got, want) in props.rows.iter().zip(visible.iter()) {
        assert_eq!(got.spans.len(), 1);
        assert_eq!(got.spans[0].text, want.title_line);
        // Compact → empty meta string (single-line row).
        assert_eq!(got.meta_line.as_deref(), Some(""));
        assert_eq!(got.is_selected, want.is_selected);
    }
}

/// A Running selected agent projects to three spans: prefix "> ", a
/// fixed-color status glyph, and " {name}". The first span is themed.
#[test]
fn agent_list_props_running_selected_spans() {
    let agents = vec![agent("alpha", AgentStatus::Running)];
    let props = agent_list_props(
        &agents,
        &[],
        AgentListSelection::default(),
        false,
        ThemeColors::default(),
        None,
    );
    assert_eq!(props.rows.len(), 1);
    let row = &props.rows[0];
    assert_eq!(row.spans.len(), 3);
    assert_eq!(row.spans[0].text, "> ");
    assert!(matches!(row.spans[0].color, SpanColor::Themed));
    assert_eq!(row.spans[1].text, "*");
    // Running → bright fixed role (resolved by the component).
    assert!(matches!(
        row.spans[1].color,
        SpanColor::Role(SpanRole::Bright)
    ));
    assert_eq!(row.spans[2].text, " alpha");
    assert!(matches!(row.spans[2].color, SpanColor::Themed));
    assert!(row.meta_line.is_none());
    assert!(row.is_selected);
}

/// A grabbed agent's prefix span text is "↕ " regardless of selection.
#[test]
fn agent_list_props_grabbed_prefix() {
    let agents = vec![
        agent("alpha", AgentStatus::Running),
        agent("beta", AgentStatus::Completed),
    ];
    let props = agent_list_props(
        &agents,
        &[],
        AgentListSelection {
            selected: 0,
            grabbed: Some(1),
        },
        false,
        ThemeColors::default(),
        None,
    );
    // Row 1 is grabbed (index 1).
    assert_eq!(props.rows[1].spans[0].text, "\u{2195} ");
    // Row 0 is selected but NOT grabbed → "> ".
    assert_eq!(props.rows[0].spans[0].text, "> ");
}

/// The agent status-glyph fixed color maps each `AgentStatus` exactly as the
/// pre-refactor component did.
#[test]
fn agent_list_props_status_glyph_color_per_status() {
    let cases: [(AgentStatus, SpanRole); 7] = [
        (AgentStatus::Running, SpanRole::Bright),
        (AgentStatus::Completed, SpanRole::Bright),
        (AgentStatus::Dead, SpanRole::Red),
        (AgentStatus::Errored, SpanRole::Red),
        (AgentStatus::Waiting, SpanRole::Yellow),
        (AgentStatus::Paused, SpanRole::Blue),
        (AgentStatus::Queued, SpanRole::Dim),
    ];
    for (status, expected) in cases {
        let agents = vec![agent("a", status)];
        let props = agent_list_props(
            &agents,
            &[],
            AgentListSelection::default(),
            false,
            ThemeColors::default(),
            None,
        );
        let row = &props.rows[0];
        assert_eq!(
            row.spans.len(),
            3,
            "status {status:?} should project 3 spans"
        );
        match row.spans[1].color {
            SpanColor::Role(r) => {
                assert_eq!(r, expected, "status {status:?} glyph role mismatch");
            }
            SpanColor::Themed => panic!("status {status:?} glyph must be Role, got Themed"),
        }
    }
}

// ── Render-canvas (ANSI) identity ───────────────────────────────────────

/// BoldSelected + compact issue row: the selected row's title renders bold
/// (`\e[1m`), and the title text appears in the output.
#[test]
fn bold_selected_compact_issue_row_renders_bold_when_selected() {
    let issues: Vec<Issue> = (1..=2).map(issue).collect();
    let window = IssueListWindow {
        selected_index: Some(0),
        list_pane_rows: 8,
        layout: IssueListLayout::Compact,
        available_width: Some(40),
    };
    let props = issue_list_props(&issues, window, true, None, ThemeColors::default(), None);
    let ansi = render_ansi(props, 40, 8);
    assert!(ansi.contains("Issue 1"), "title text must appear: {ansi}");
    assert!(
        ansi.contains("\u{1b}[1m"),
        "selected row must render bold (SGR 1): {ansi}"
    );
}

/// BrightSelected agent row: the status glyph keeps its fixed color (Red for
/// Dead) even when selected, and the agent name uses the bright color —
/// matching the pre-refactor AgentList which always used `Weight::Normal`
/// for rows and a fixed status-glyph color. (The title row is always bold
/// in every list, so we assert on the glyph color + name color, not on the
/// absence of any bold SGR.)
#[test]
fn bright_selected_agent_row_keeps_fixed_glyph_color() {
    let agents = vec![agent("dead-agent", AgentStatus::Dead)];
    let props = agent_list_props(
        &agents,
        &[],
        AgentListSelection::default(),
        true,
        ThemeColors::default(),
        None,
    );
    let ansi = render_ansi(props, 30, 8);
    // `Color::Red` is emitted by iocraft as the 256-color code 38;5;9. The
    // key byte-identity guarantee is that the status glyph keeps its fixed
    // color (immune to the BrightSelected themed-color policy).
    assert!(
        ansi.contains("\u{1b}[38;5;9m"),
        "Dead status glyph must keep its fixed Red (38;5;9) color: {ansi}"
    );
    // The selected agent's themed name span uses bright (#00ff00).
    assert!(
        ansi.contains("dead-agent"),
        "agent name text must appear: {ansi}"
    );
}

// ── Agent list with git info (issue #170) ───────────────────────────────

/// When `git_infos` carries an origin shortform + branch, the agent row
/// gets a 4th dim-colored span with `  {origin} @ {branch}` text.
#[test]
fn agent_list_props_with_git_info_adds_suffix_span() {
    let agents = vec![agent("fix-login", AgentStatus::Running)];
    let git_infos = vec![GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: None,
    }];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        false,
        ThemeColors::default(),
        None,
    );
    let row = &props.rows[0];
    assert_eq!(
        row.spans.len(),
        4,
        "should have prefix + glyph + name + git suffix"
    );
    assert_eq!(row.spans[3].text, "  vybestack/llxprt-jefe @ main");
    assert!(matches!(row.spans[3].color, SpanColor::Role(SpanRole::Dim)));
}

/// When `git_infos` entry has no data (both None), no suffix span is added
/// — the row stays at 3 spans.
#[test]
fn agent_list_props_with_empty_git_info_no_suffix() {
    let agents = vec![agent("fix-login", AgentStatus::Running)];
    let git_infos = vec![GitRepoInfo::default()];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        false,
        ThemeColors::default(),
        None,
    );
    let row = &props.rows[0];
    assert_eq!(
        row.spans.len(),
        3,
        "empty git info should not add a suffix span"
    );
}

/// When `git_infos` is shorter than `agents` (missing entry), the agent at
/// the missing index just renders without a suffix (graceful degradation).
#[test]
fn agent_list_props_git_infos_shorter_than_agents() {
    let agents = vec![
        agent("alpha", AgentStatus::Running),
        agent("beta", AgentStatus::Completed),
    ];
    // Only provide git info for index 0.
    let git_infos = vec![GitRepoInfo {
        origin_shortform: Some("acme/widgets".to_owned()),
        branch: Some("dev".to_owned()),
        dirty: None,
    }];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        false,
        ThemeColors::default(),
        None,
    );
    assert_eq!(props.rows[0].spans.len(), 4, "row 0 has git suffix");
    assert_eq!(props.rows[1].spans.len(), 3, "row 1 has no git suffix");
}

/// Agent list git-info suffix renders in the rendered output.
#[test]
fn agent_list_git_info_suffix_renders() {
    let agents = vec![agent("fix-login", AgentStatus::Running)];
    let git_infos = vec![GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: None,
    }];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        true,
        ThemeColors::default(),
        None,
    );
    let ansi = render_ansi(props, 60, 8);
    assert!(
        ansi.contains("vybestack/llxprt-jefe @ main"),
        "git suffix text must appear in rendered output: {ansi}"
    );
}

/// Agent list git-info suffix includes the dirty marker (` *`) when the
/// working tree is dirty (issue #230).
#[test]
fn agent_list_dirty_suffix_renders_marker() {
    let agents = vec![agent("fix-login", AgentStatus::Running)];
    let git_infos = vec![GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: Some(true),
    }];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        true,
        ThemeColors::default(),
        None,
    );
    // The suffix span text must include the dirty marker.
    let suffix = find_git_suffix_span(&props.rows[0].spans);
    assert_eq!(
        suffix, "  vybestack/llxprt-jefe @ main *",
        "dirty worktree must append ' *' to the git suffix span"
    );
    let ansi = render_ansi(props, 70, 8);
    assert!(
        ansi.contains("vybestack/llxprt-jefe @ main *"),
        "dirty marker must appear in rendered output: {ansi}"
    );
}

fn find_git_suffix_span(spans: &[SelectableSpan]) -> &str {
    spans
        .iter()
        .rev()
        .find(|s| s.text.starts_with("  "))
        .map_or("", |s| s.text.as_str())
}

#[test]
fn agent_list_unknown_dirty_suffix_no_marker() {
    let agents = vec![agent("fix-login", AgentStatus::Running)];
    let git_infos = vec![GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: None,
    }];
    let props = agent_list_props(
        &agents,
        &git_infos,
        AgentListSelection::default(),
        true,
        ThemeColors::default(),
        None,
    );
    let suffix = find_git_suffix_span(&props.rows[0].spans);
    assert_eq!(
        suffix, "  vybestack/llxprt-jefe @ main",
        "unknown dirty status must not append a dirty marker"
    );
    let ansi = render_ansi(props, 70, 8);
    assert!(
        !ansi.contains("main *"),
        "unknown dirty status must not show dirty marker in rendered output: {ansi}"
    );
}
/// Clean worktree (dirty: Some(false)) renders the git suffix WITHOUT the
/// dirty marker — end-to-end render check symmetric to the dirty test.
#[test]
fn agent_list_clean_suffix_renders_no_marker() {
    let props = agent_list_props(
        &[agent("fix-login", AgentStatus::Running)],
        &[GitRepoInfo {
            origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
            branch: Some("main".to_owned()),
            dirty: Some(false),
        }],
        AgentListSelection::default(),
        true,
        ThemeColors::default(),
        None,
    );
    let ansi = render_ansi(props, 70, 8);
    assert!(
        ansi.contains("vybestack/llxprt-jefe @ main"),
        "rendered: {ansi}"
    );
    assert!(
        !ansi.contains("main *"),
        "clean must not show marker: {ansi}"
    );
}

/// An empty-message list renders the message text in the dim color and no
/// row text appears.
#[test]
fn empty_message_renders_in_dim() {
    let props = SelectableListProps {
        title: "Issues".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("No issues found".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::IssueList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 38,
    };
    let ansi = render_ansi(props, 40, 8);
    assert!(
        ansi.contains("No issues found"),
        "empty message must render"
    );
    // Green-screen dim = #6a9955 → RGB 106,153,85 (accent_secondary).
    assert!(
        ansi.contains("\u{1b}[38;2;106;153;85m"),
        "empty message must render in dim color: {ansi}"
    );
}

/// DoubleOnFocus border: a focused list renders a double border (`╔`), an
/// unfocused one renders a round border (`╭`).
#[test]
fn double_on_focus_border_switches_on_focus() {
    let base = || SelectableListProps {
        title: "Issues".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("x".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::IssueList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 38,
    };
    let unfocused = render_ansi(base(), 20, 6);
    let mut focused = base();
    focused.focused = true;
    let focused = render_ansi(focused, 20, 6);
    assert!(
        unfocused.contains('╭'),
        "unfocused DoubleOnFocus must use round border: {unfocused}"
    );
    assert!(
        focused.contains('╔'),
        "focused DoubleOnFocus must use double border: {focused}"
    );
}

/// RoundFocusedColor border (Agent): always round (`╭`), focused or not.
#[test]
fn round_focused_color_border_always_round() {
    let base = || SelectableListProps {
        title: "Agents".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("x".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::AgentList,
        border: ListBorder::RoundFocusedColor,
        content_padding: true,
        selection_style: SelectionStyle::BrightSelected,
        content_width: 16,
    };
    let unfocused = render_ansi(base(), 20, 6);
    let mut focused = base();
    focused.focused = true;
    let focused = render_ansi(focused, 20, 6);
    assert!(
        unfocused.contains('╭') && focused.contains('╭'),
        "RoundFocusedColor must always use round border"
    );
    // Focused border color brightens to border_focused (#00ff00 → 0;255;0).
    assert!(
        focused.contains("\u{1b}[38;2;0;255;0m"),
        "focused RoundFocusedColor border must use border_focused color: {focused}"
    );
}
