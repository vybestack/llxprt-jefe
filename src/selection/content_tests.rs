//! Tests for selectable pane content projections.

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
        default_code_puppy_version: String::new(),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        transient_agent_dir: std::path::PathBuf::new(),
        default_code_puppy_yolo: None,
        transient_max_concurrent: 0,
        default_llxprt_version: None,
        agent_ids: vec![AgentId("a1".to_string()), AgentId("a2".to_string())],
    });
    // Select the first repo so the rendered "> " prefix appears.
    state.selected_repository_index = Some(0);
    let content = pane_content_lines(SelectablePane::Sidebar, &state, None, &[], 120, 40);
    // Selected repo gets "> " prefix; matches the Sidebar renderer.
    assert_eq!(content.lines, vec!["> repo-one (0)".to_string()]);
}

fn repository_form_selection_projection_matches_runtime_focus_order() {
    use crate::state::{
        ModalState, RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
    };

    let fields = RepositoryFormFields {
        default_profile: "profile".to_owned(),
        default_agent_kind: "LLxprt".to_owned(),
        default_llxprt_version: "0.9.0".to_owned(),
        github_repo: "owner/repo".to_owned(),
        ..RepositoryFormFields::default()
    };
    let state = AppState {
        modal: ModalState::NewRepository {
            fields,
            focus: RepositoryFormFocus::DefaultLlxprtVersion,
            cursor: RepositoryFormCursor {
                default_llxprt_version: 5,
                ..RepositoryFormCursor::default()
            },
        },
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    let lines = crate::selection::repository_form_content_lines(&state)
        .unwrap_or_else(|| panic!("expected repository form projection"));
    let positions = [
        "Default Profile",
        "Default Agent",
        "Default Version",
        "GitHub Repo",
    ]
    .map(|label| {
        lines
            .iter()
            .position(|line| line.contains(label))
            .unwrap_or_else(|| panic!("missing {label}"))
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(lines[positions[2]].contains("0.9.0▏"));
    assert!(!lines.iter().any(|line| line.contains("Default Model")));
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
        head_sha: String::new(),
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
    // The process-identity label (pid + commit) must be present so mouse-copy
    // captures it (issue #223).
    assert!(
        content.lines[0].contains("pid:"),
        "keybind bar copy must include the pid marker: {}",
        content.lines[0]
    );
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
        head_sha: "sha123".to_string(),
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
    let content = pane_content_lines(SelectablePane::RepositoryForm, &state, None, &[], 120, 40);
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
fn repository_form_selection_projection_uses_runtime_focus_order() {
    repository_form_selection_projection_matches_runtime_focus_order();
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
            crate::domain::AgentChooserEntry::simple("a1", "alpha"),
            crate::domain::AgentChooserEntry::simple("a2", "beta"),
        ],
        transient_available: false,
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
        head_sha: String::new(),
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
