//! Pane content providers for overlay panes: choosers and confirm modal
//! (issue #178).
//!
//! These build the flat list of copyable lines that match what the
//! `AgentChooser`, `MergeChooser`, and `ConfirmModal` iocraft components
//! render, so mouse selection coordinates map to the exact characters on
//! screen.
//!
//! All functions are pure and `#[must_use]`. They read only from
//! [`crate::state::AppState`] (no iocraft, no side effects).

use crate::selection::PaneContent;
use crate::selection::SelectablePane;
use crate::state::AppState;
use crate::ui::components::property_editor_view::build_property_editor_view;

/// Separator line rendered inside the chooser overlays, matching the
/// components' `"─────…"` row.
const SEPARATOR_LINE: &str = "─────────────────────────────────────────";

/// Agent chooser overlay lines: header + separator + agent entries + hints.
#[must_use]
pub fn agent_chooser_lines(state: &AppState) -> PaneContent {
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .or(state.prs_state.agent_chooser.as_ref());
    let Some(chooser) = chooser else {
        return PaneContent::empty(SelectablePane::AgentChooser);
    };
    let mut lines = vec!["Send to Agent".to_string(), SEPARATOR_LINE.to_string()];
    if chooser.agents.is_empty() && !chooser.transient_available {
        lines.push("No agents available. Create an agent in Agents Mode.".to_string());
        lines.push(String::new());
    } else {
        for (i, entry) in chooser.agents.iter().enumerate() {
            let marker = if i == chooser.selected_index {
                "(x)"
            } else {
                "( )"
            };
            let label = crate::domain::agent_chooser_label(entry);
            lines.push(format!("{marker} {label}"));
        }
        if chooser.transient_available {
            let transient_idx = chooser.agents.len();
            let marker = if transient_idx == chooser.selected_index {
                "(x)"
            } else {
                "( )"
            };
            lines.push(format!("{marker} Transient Agent"));
        }
    }
    lines.push(SEPARATOR_LINE.to_string());
    lines.push("Enter send  Esc cancel".to_string());
    PaneContent::new(SelectablePane::AgentChooser, lines)
}

/// Merge chooser overlay lines: header + separator + methods + hints.
#[must_use]
pub fn merge_chooser_lines(state: &AppState) -> PaneContent {
    let Some(chooser) = state.prs_state.merge_chooser.as_ref() else {
        return PaneContent::empty(SelectablePane::MergeChooser);
    };
    let pr_number = state.prs_state.pr_detail.as_ref().map_or(0, |d| d.number);
    let mut lines = vec![
        format!("Merge Pull Request #{pr_number}"),
        SEPARATOR_LINE.to_string(),
    ];
    for (i, method) in crate::domain::MERGE_METHODS.iter().enumerate() {
        let selected = i == chooser.selected_index;
        let allowed = chooser.allowed_methods.as_deref();
        let enabled = match allowed {
            None => true,
            Some(methods) => methods.contains(method),
        };
        let label = if enabled {
            let marker = if selected { "(x)" } else { "( )" };
            format!("{marker} {}", method.label())
        } else {
            format!("    {} (not enabled)", method.label())
        };
        lines.push(label);
    }
    lines.push(SEPARATOR_LINE.to_string());
    if chooser.awaiting_confirmation {
        lines.push("Press Enter to confirm merge, Esc to cancel".to_string());
    } else {
        lines.push("Up/Down select  Enter confirm  Esc cancel".to_string());
    }
    PaneContent::new(SelectablePane::MergeChooser, lines)
}

/// Close-reason chooser overlay lines: header + separator + reasons + optional
/// duplicate search + hints (issue #188).
#[must_use]
pub fn close_reason_chooser_lines(state: &AppState) -> PaneContent {
    let Some(chooser) = state.issues_state.close_reason_chooser.as_ref() else {
        return PaneContent::empty(SelectablePane::CloseReasonChooser);
    };
    let lines = crate::ui::components::close_reason_chooser_lines(
        chooser.issue_number,
        chooser.selected_index,
        chooser.awaiting_confirmation,
        chooser.duplicate_search.as_ref().map(|s| s.query.as_str()),
        &chooser
            .duplicate_search
            .as_ref()
            .map(|s| s.candidates.clone())
            .unwrap_or_default(),
        chooser
            .duplicate_search
            .as_ref()
            .map_or(0, |s| s.selected_index),
    );
    PaneContent::new(SelectablePane::CloseReasonChooser, lines)
}

/// Issue delete-confirm overlay lines (issue #182).
#[must_use]
pub fn issue_delete_confirm_lines(state: &AppState) -> PaneContent {
    let Some(confirm) = state.issues_state.delete_confirm.as_ref() else {
        return PaneContent::empty(SelectablePane::IssueDeleteConfirm);
    };
    PaneContent::new(
        SelectablePane::IssueDeleteConfirm,
        vec![
            crate::ui::components::delete_confirm_header(confirm.issue_number),
            "This action cannot be undone.".to_string(),
            crate::ui::components::SEPARATOR_LINE.to_string(),
            crate::ui::components::delete_confirm_hint(confirm.awaiting_confirmation).to_string(),
        ],
    )
}

/// Confirm modal lines: title + blank + message + optional checkbox + buttons + blank.
///
/// The ConfirmModal renders inside a 50x10 bordered box with padding 1 (6
/// inner rows). The title box is height 2 (1 text + 1 blank), the message
/// uses flex_grow, an optional checkbox is height 1, and the buttons box is
/// height 2 (1 text + 1 blank). This projection mirrors those exact rows so
/// selection coordinates map to what the user sees.
///
/// Known limitation: the message is projected as a single line. If the
/// rendered message exceeds the 48-column inner width, iocraft wraps it to
/// additional rows that this projection does not account for. Selection of
/// the wrapped portion will be slightly misaligned.
#[must_use]
pub fn confirm_modal_lines(state: &AppState) -> PaneContent {
    // Reuse the single source of truth from the orchestration layer so the
    // projected text can never drift from the rendered modal.
    let Some(data) = crate::ui::orchestration::derive_confirm_modal_data(state, &state.modal)
    else {
        return PaneContent::empty(SelectablePane::ConfirmModal);
    };
    let mut lines = vec![data.title, String::new()];
    lines.push(data.message);
    if data.show_delete_work_dir {
        let mark = if data.delete_work_dir { "x" } else { " " };
        lines.push(format!("[{mark}] Delete work directory"));
    } else {
        lines.push(String::new());
    }
    lines.push(crate::ui::modals::confirm_button_row(data.confirm_focus));
    lines.push(String::new());
    PaneContent::new(SelectablePane::ConfirmModal, lines)
}

/// Property editor overlay lines: header + separator + options/title +
/// separator + hints (issue #175).
///
/// Mirrors the `PropertyEditor` component's rendered rows so mouse selection
/// coordinates map to the exact characters on screen.
#[must_use]
pub fn property_editor_lines(state: &AppState) -> PaneContent {
    if let Some(editor) = state.issues_state.property_editor.as_ref() {
        let number = state
            .issues_state
            .issue_detail
            .as_ref()
            .map_or(0, |d| d.number);
        return build_issue_editor_lines(editor, number);
    }
    if let Some(editor) = state.prs_state.property_editor.as_ref() {
        let number = state.prs_state.pr_detail.as_ref().map_or(0, |d| d.number);
        return build_pr_editor_lines(editor, number);
    }
    PaneContent::empty(SelectablePane::PropertyEditor)
}

fn build_issue_editor_lines(
    editor: &crate::state::IssuePropertyEditorState,
    number: u64,
) -> PaneContent {
    use crate::state::IssuePropertyKind as K;
    let kind_label = match editor.kind {
        K::Labels => "Labels",
        K::Assignees => "Assignees",
        K::Milestone => "Milestone",
        K::Title => "Title",
        K::Type => "Type",
        K::State => "State",
    };
    let is_title = matches!(editor.kind, K::Title);
    let multi = matches!(editor.kind, K::Labels | K::Assignees);
    build_editor_lines(EditorLineParams {
        kind_label,
        entity: "Issue",
        number,
        is_title,
        multi,
        title_text: &editor.title_text,
        options: &editor.options,
        selected_index: editor.selected_index,
        error: editor.error.as_ref(),
        viewport_rows: PROPERTY_EDITOR_DEFAULT_VIEWPORT,
    })
}

fn build_pr_editor_lines(editor: &crate::state::PrPropertyEditorState, number: u64) -> PaneContent {
    use crate::state::PrPropertyKind as K;
    let kind_label = match editor.kind {
        K::Labels => "Labels",
        K::Assignees => "Assignees",
        K::Milestone => "Milestone",
        K::Title => "Title",
        K::State => "State",
    };
    let is_title = matches!(editor.kind, K::Title);
    let multi = matches!(editor.kind, K::Labels | K::Assignees);
    build_editor_lines(EditorLineParams {
        kind_label,
        entity: "PR",
        number,
        is_title,
        multi,
        title_text: &editor.title_text,
        options: &editor.options,
        selected_index: editor.selected_index,
        error: editor.error.as_ref(),
        viewport_rows: PROPERTY_EDITOR_DEFAULT_VIEWPORT,
    })
}

/// Aggregated parameters for [`build_editor_lines`] to stay under clippy's
/// `too_many_arguments` threshold.
struct EditorLineParams<'a> {
    kind_label: &'a str,
    entity: &'a str,
    number: u64,
    is_title: bool,
    multi: bool,
    title_text: &'a str,
    options: &'a [crate::state::PropertyOption],
    selected_index: usize,
    error: Option<&'a String>,
    /// Maximum option rows to render; older rows scroll out of view as the
    /// cursor moves (issue #175 F4).
    viewport_rows: usize,
}

/// Default viewport height for the property-editor option list (issue #175 F4).
/// Keeps the overlay usable on small terminals when a repo has many labels or
/// assignees. The cursor-following window is computed by the pure projection
/// in `property_editor_view`.
const PROPERTY_EDITOR_DEFAULT_VIEWPORT: usize = 12;

fn build_editor_lines(p: EditorLineParams) -> PaneContent {
    let mut lines = vec![
        format!("Edit {} - {} #{}", p.kind_label, p.entity, p.number),
        SEPARATOR_LINE.to_string(),
    ];
    if p.is_title {
        lines.push(p.title_text.to_string());
    } else {
        // F4: window the options so the overlay stays within the terminal.
        let view = build_property_editor_view(p.options.len(), p.selected_index, p.viewport_rows);
        for full_index in view.iter_visible() {
            let Some(opt) = p.options.get(full_index) else {
                break;
            };
            let is_cursor = full_index == p.selected_index;
            let label_text = if p.multi {
                let marker = if opt.selected { "(x)" } else { "( )" };
                format!("{marker} {}", opt.label)
            } else {
                let marker = if is_cursor { ">" } else { " " };
                format!("{marker} {}", opt.label)
            };
            lines.push(label_text);
        }
    }
    lines.push(SEPARATOR_LINE.to_string());
    if let Some(err) = p.error {
        lines.push(err.clone());
    } else if p.is_title {
        lines.push("type title  Ctrl+Enter apply  Esc cancel".to_string());
    } else if p.multi {
        lines.push("Up/Down move  Space toggle  Enter apply  Esc cancel".to_string());
    } else {
        lines.push("Up/Down move  Enter apply  Esc cancel".to_string());
    }
    PaneContent::new(SelectablePane::PropertyEditor, lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Agent, AgentId, Repository, RepositoryId};
    use crate::state::ModalState;

    #[test]
    fn confirm_modal_delete_agent_exact_layout_with_checkbox() {
        let repo_id = RepositoryId("r1".to_string());
        let agent_id = AgentId("a1".to_string());
        let mut state = AppState {
            modal: ModalState::ConfirmDeleteAgent {
                id: agent_id.clone(),
                delete_work_dir: true,
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        state.repositories.push(Repository::new(
            repo_id.clone(),
            "repo".to_string(),
            "repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        state.agents.push(Agent::new(
            agent_id,
            repo_id,
            "my-agent".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));
        let content = confirm_modal_lines(&state);
        assert_eq!(
            content.lines,
            vec![
                "Delete Agent".to_string(),
                String::new(),
                "Delete my-agent?".to_string(),
                "[x] Delete work directory".to_string(),
                "( Cancel )  [ Confirm ]".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn confirm_modal_delete_repo_exact_layout_without_checkbox() {
        let mut state = AppState {
            modal: ModalState::ConfirmDeleteRepository {
                id: RepositoryId("r1".to_string()),
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        state.repositories.push(Repository::new(
            RepositoryId("r1".to_string()),
            "my-repo".to_string(),
            "my-repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        let content = confirm_modal_lines(&state);
        assert_eq!(
            content.lines,
            vec![
                "Delete Repository".to_string(),
                String::new(),
                "Delete my-repo and all its agents?".to_string(),
                String::new(),
                "( Cancel )  [ Confirm ]".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn confirm_modal_kill_agent_exact_layout() {
        let repo_id = RepositoryId("r1".to_string());
        let agent_id = AgentId("a1".to_string());
        let mut state = AppState {
            modal: ModalState::ConfirmKillAgent {
                id: agent_id.clone(),
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        state.repositories.push(Repository::new(
            repo_id.clone(),
            "repo".to_string(),
            "repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        state.agents.push(Agent::new(
            agent_id,
            repo_id,
            "running-agent".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        ));
        let content = confirm_modal_lines(&state);
        assert_eq!(content.lines[0], "Kill Agent");
        assert_eq!(content.lines[2], "Kill running-agent?");
    }

    /// Find the confirm-modal button row by its button markers (issue #228).
    fn confirm_button_row_line(lines: &[String]) -> usize {
        lines
            .iter()
            .position(|l| l.contains("Cancel") && l.contains("Confirm"))
            .unwrap_or_else(|| {
                panic!("confirm modal must have a Cancel/Confirm button row, got: {lines:?}")
            })
    }

    #[test]
    fn confirm_modal_focus_rendered_as_cancel_default() {
        let mut state = AppState {
            modal: ModalState::ConfirmDeleteRepository {
                id: RepositoryId("r1".to_string()),
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        state.repositories.push(Repository::new(
            RepositoryId("r1".to_string()),
            "my-repo".to_string(),
            "my-repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        let content = confirm_modal_lines(&state);
        // Default focus = Cancel → the focused button uses (…)
        assert_eq!(
            content.lines[confirm_button_row_line(&content.lines)],
            "( Cancel )  [ Confirm ]"
        );
    }

    #[test]
    fn confirm_modal_focus_rendered_as_confirm() {
        let mut state = AppState {
            modal: ModalState::ConfirmDeleteRepository {
                id: RepositoryId("r1".to_string()),
                confirm_focus: crate::state::ConfirmFocus::Confirm,
            },
            ..Default::default()
        };
        state.repositories.push(Repository::new(
            RepositoryId("r1".to_string()),
            "my-repo".to_string(),
            "my-repo".to_string(),
            std::path::PathBuf::from("/tmp/repo"),
        ));
        let content = confirm_modal_lines(&state);
        // Focus = Confirm → the focused button uses (…)
        assert_eq!(
            content.lines[confirm_button_row_line(&content.lines)],
            "[ Cancel ]  ( Confirm )"
        );
    }

    #[test]
    fn confirm_modal_preflight_prompt_has_content() {
        use crate::domain::{LaunchSignature, SandboxEngine};
        use crate::runtime::PreflightIssue;
        let signature = LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
            llxprt_version: None,
        };
        let state = AppState {
            modal: ModalState::PreflightPrompt {
                agent_id: AgentId("a1".to_string()),
                signature,
                issue: PreflightIssue::SshAgentNoIdentities,
                remaining_issues: Vec::new(),
                issue_self_assignment: None,
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        let content = confirm_modal_lines(&state);
        assert!(
            !content.lines.is_empty(),
            "preflight prompt must have content"
        );
        assert!(
            content.lines[0].contains("SSH"),
            "preflight SSH issue title must be present"
        );
    }

    #[test]
    fn confirm_modal_dirty_copy_has_content() {
        use crate::domain::{LaunchSignature, SandboxEngine};
        use crate::github::SendPayload;
        let signature = LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
            llxprt_version: None,
        };
        let payload = SendPayload {
            repository: "o/r".to_string(),
            issue_number: 42,
            ..Default::default()
        };
        let state = AppState {
            modal: ModalState::ConfirmIssueDirtyCopy {
                agent_id: AgentId("a1".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature,
                payload,
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        let content = confirm_modal_lines(&state);
        assert!(!content.lines.is_empty());
        assert_eq!(content.lines[0], "Working Copy Not Ready");
        assert!(content.lines[2].contains("default branch"));
    }

    #[test]
    fn confirm_modal_origin_mismatch_renders_actual_expected() {
        use crate::domain::{LaunchSignature, SandboxEngine};
        use crate::github::SendPayload;
        let signature = LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
            llxprt_version: None,
        };
        let payload = SendPayload {
            repository: "o/r".to_string(),
            issue_number: 42,
            ..Default::default()
        };
        let state = AppState {
            modal: ModalState::ConfirmIssueOriginMismatch {
                agent_id: AgentId("a1".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature,
                payload,
                actual: "other/repo".to_string(),
                expected: "acme/widgets".to_string(),
                confirm_focus: crate::state::ConfirmFocus::Cancel,
            },
            ..Default::default()
        };
        let content = confirm_modal_lines(&state);
        assert!(!content.lines.is_empty());
        assert_eq!(content.lines[0], "Wrong Repository");
        assert!(
            content.lines[2].contains("other/repo"),
            "modal must show actual origin: {content:?}"
        );
        assert!(
            content.lines[2].contains("acme/widgets"),
            "modal must show expected origin: {content:?}"
        );
    }

    #[test]
    fn agent_chooser_empty_has_two_line_empty_state() {
        use crate::domain::{AgentChooserEntry, AgentKind, ChooserRuntimeConfig, DirtyStatus};
        let state = AppState {
            issues_state: crate::state::IssuesState {
                agent_chooser: Some(crate::state::AgentChooserState {
                    selected_index: 0,
                    transient_available: false,
                    agents: vec![
                        AgentChooserEntry {
                            agent_id: AgentId("a1".to_string()),
                            name: "alpha".to_string(),
                            kind: AgentKind::Llxprt,
                            runtime_config: ChooserRuntimeConfig::new("ops"),
                            branch: None,
                            dirty: DirtyStatus::unknown(),
                        },
                        AgentChooserEntry {
                            agent_id: AgentId("a2".to_string()),
                            name: "beta".to_string(),
                            kind: AgentKind::CodePuppy,
                            runtime_config: ChooserRuntimeConfig::new("minimax-m3"),
                            branch: Some("feature".to_string()),
                            dirty: DirtyStatus::dirty(),
                        },
                    ],
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let content = agent_chooser_lines(&state);
        // Verify exact structure: header, separator, entries, separator, hints.
        assert_eq!(content.lines[0], "Send to Agent");
        assert!(content.lines[1].starts_with('─'));
        // Entry 0: LLxprt with a named profile, unknown dirty (no marker).
        assert_eq!(
            content.lines[2], "(x) alpha [LLxprt] profile: ops",
            "selected entry must show kind label, configured profile, no dirty marker"
        );
        // Entry 1: Code Puppy with a named model, dirty branch-adjacent marker.
        assert_eq!(
            content.lines[3], "( ) beta [Code Puppy] model: minimax-m3  @ feature *",
            "unselected entry must show kind label, configured model, branch with dirty marker"
        );
        assert!(content.lines[4].starts_with('─'));
    }

    #[test]
    fn agent_chooser_default_and_clean_render() {
        use crate::domain::{AgentChooserEntry, AgentKind, ChooserRuntimeConfig, DirtyStatus};
        let state = AppState {
            screen_mode: crate::state::ScreenMode::DashboardIssues,
            issues_state: crate::state::IssuesState {
                agent_chooser: Some(crate::state::AgentChooserState {
                    selected_index: 0,
                    transient_available: false,
                    agents: vec![
                        AgentChooserEntry {
                            agent_id: AgentId("d1".to_string()),
                            name: "delta".to_string(),
                            kind: AgentKind::Llxprt,
                            runtime_config: ChooserRuntimeConfig::default(),
                            branch: None,
                            dirty: DirtyStatus::clean(),
                        },
                        AgentChooserEntry {
                            agent_id: AgentId("d2".to_string()),
                            name: "epsilon".to_string(),
                            kind: AgentKind::CodePuppy,
                            runtime_config: ChooserRuntimeConfig::default(),
                            branch: None,
                            dirty: DirtyStatus::clean(),
                        },
                    ],
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let content = agent_chooser_lines(&state);
        // Empty config must show the explicit (default) label; clean tree
        // must NOT show the dirty marker.
        assert_eq!(
            content.lines[2], "(x) delta [LLxprt] profile: (default)",
            "empty profile must show (default) and clean tree must not show dirty marker"
        );
        assert_eq!(
            content.lines[3], "( ) epsilon [Code Puppy] model: (default)",
            "empty model must show (default) and clean tree must not show dirty marker"
        );
    }

    /// Every confirm modal variant must render — i.e.
    /// `derive_confirm_modal_data` must return `Some` for all six confirm
    /// variants (issue #228). If a new confirm variant is added to
    /// `ModalState` without a matching arm in `derive_confirm_modal_data`,
    /// this test will fail (the catch-all `_ => return None` would silently
    /// suppress rendering otherwise).
    #[test]
    fn confirm_modal_renders_all_variants() {
        use crate::ui::orchestration::derive_confirm_modal_data;

        let state = AppState::default();
        for modal in overlay_confirm_modal_samples() {
            assert!(
                derive_confirm_modal_data(&state, &modal).is_some(),
                "derive_confirm_modal_data must return Some for confirm variant: {modal:?}"
            );
        }
    }

    fn confirm_signature() -> crate::domain::LaunchSignature {
        crate::domain::LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
            llxprt_version: None,
        }
    }

    fn overlay_confirm_modal_samples() -> Vec<crate::state::ModalState> {
        use crate::github::SendPayload;
        use crate::runtime::PreflightIssue;
        use crate::state::{ConfirmFocus, ModalState};

        vec![
            ModalState::ConfirmDeleteAgent {
                id: AgentId("a".to_string()),
                delete_work_dir: false,
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmDeleteRepository {
                id: RepositoryId("r".to_string()),
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmKillAgent {
                id: AgentId("a".to_string()),
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::PreflightPrompt {
                agent_id: AgentId("a".to_string()),
                signature: confirm_signature(),
                issue: PreflightIssue::SshAgentNoIdentities,
                remaining_issues: Vec::new(),
                issue_self_assignment: None,
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmIssueDirtyCopy {
                agent_id: AgentId("a".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature: confirm_signature(),
                payload: SendPayload::default(),
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmIssueOriginMismatch {
                agent_id: AgentId("a".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature: confirm_signature(),
                payload: SendPayload::default(),
                actual: String::new(),
                expected: String::new(),
                confirm_focus: ConfirmFocus::Cancel,
            },
        ]
    }

    #[test]
    fn property_editor_lines_include_header_and_options() {
        use crate::domain::{IssueDetail, IssueState};
        use crate::state::{IssuePropertyEditorState, IssuePropertyKind, PropertyOption};
        let mut state = AppState::default();
        state.issues_state.issue_detail = Some(IssueDetail {
            repo_owner_name: "o/r".to_string(),
            number: 42,
            node_id: String::new(),
            title: "T".to_string(),
            state: IssueState::Open,
            author_login: "x".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            labels: vec!["bug".to_string()],
            assignees: Vec::new(),
            milestone: None,
            body: String::new(),
            external_url: String::new(),
            comments: crate::domain::PaginatedList::default(),
            issue_type_name: None,
            state_reason: None,
        });
        state.issues_state.property_editor = Some(IssuePropertyEditorState {
            kind: IssuePropertyKind::Labels,
            options: vec![PropertyOption {
                label: "bug".to_string(),
                selected: true,
                id: None,
            }],
            selected_index: 0,
            title_text: String::new(),
            title_cursor: 0,
            error: None,
            baseline: Vec::new(),
            loading_failed: false,
            options_loading: false,
            load_request_id: 0,
        });
        let content = property_editor_lines(&state);
        assert!(
            content
                .lines
                .iter()
                .any(|l| l.contains("Edit Labels - Issue #42")),
            "property editor must include header with kind and issue number"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("(x) bug")),
            "property editor must list selected option"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("Space toggle")),
            "property editor multi-select hint must mention Space toggle"
        );
    }

    #[test]
    fn property_editor_lines_empty_when_no_editor() {
        let state = AppState::default();
        let content = property_editor_lines(&state);
        assert!(
            content.lines.is_empty(),
            "property editor with no editor should have no content"
        );
    }

    #[test]
    fn property_editor_lines_pr_state() {
        use crate::state::{PrPropertyEditorState, PrPropertyKind, PropertyOption};
        let mut state = AppState::default();
        state.prs_state.pr_detail = Some(test_pr_detail_for_prop_editor(7));
        state.prs_state.property_editor = Some(PrPropertyEditorState {
            kind: PrPropertyKind::State,
            options: vec![
                PropertyOption {
                    label: "Open".to_string(),
                    selected: true,
                    id: None,
                },
                PropertyOption {
                    label: "Closed".to_string(),
                    selected: false,
                    id: None,
                },
            ],
            selected_index: 0,
            title_text: String::new(),
            title_cursor: 0,
            error: None,
            baseline: Vec::new(),
            loading_failed: false,
            options_loading: false,
            load_request_id: 0,
        });
        let content = property_editor_lines(&state);
        assert!(
            content
                .lines
                .iter()
                .any(|l| l.contains("Edit State - PR #7")),
            "PR property editor must include header with kind and PR number"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("> Open")),
            "single-select property editor must use > cursor marker"
        );
        assert!(
            content.lines.iter().any(|l| l.contains("Enter apply")),
            "single-select property editor hint must mention Enter apply"
        );
    }

    /// Minimal PR detail for property-editor overlay content tests.
    fn test_pr_detail_for_prop_editor(number: u64) -> crate::domain::PullRequestDetail {
        use crate::domain::{PrCheckStatus, PrState, PullRequestDetail};
        PullRequestDetail {
            repo_owner_name: "o/r".to_string(),
            number,
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
            comments: crate::domain::PaginatedList::default(),
            mergeable: None,
            merge_state_status: None,
        }
    }
}
