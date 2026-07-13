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

/// Separator line rendered inside the chooser overlays, matching the
/// components' `"─────…"` row.
const SEPARATOR_LINE: &str = "─────────────────────────────────────────";

/// Agent chooser overlay lines: header + separator + agent entries + hints.
#[must_use]
pub fn agent_chooser_lines(state: &AppState) -> PaneContent {
    // The chooser can be active in issues or PR mode; check both.
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .or(state.prs_state.agent_chooser.as_ref());
    let Some(chooser) = chooser else {
        return PaneContent::empty(SelectablePane::AgentChooser);
    };
    let mut lines = vec!["Send to Agent".to_string(), SEPARATOR_LINE.to_string()];
    if chooser.agents.is_empty() {
        // The empty-state box has height 2 (text + blank), matching the
        // renderer's Box(height: 2u32).
        lines.push("No agents available. Create an agent in Agents Mode.".to_string());
        lines.push(String::new());
    } else {
        for (i, (_id, name)) in chooser.agents.iter().enumerate() {
            let marker = if i == chooser.selected_index {
                "(x)"
            } else {
                "( )"
            };
            lines.push(format!("{marker} {name}"));
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
        assert_eq!(content.lines[0], "Dirty Working Copy");
        assert!(content.lines[2].contains("uncommitted changes"));
    }

    #[test]
    fn confirm_modal_origin_mismatch_renders_actual_expected() {
        use crate::domain::{LaunchSignature, SandboxEngine};
        use crate::github::SendPayload;
        let signature = LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            code_puppy_model: String::new(),
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
        let state = AppState {
            issues_state: crate::state::IssuesState {
                agent_chooser: Some(crate::state::AgentChooserState {
                    selected_index: 0,
                    agents: vec![],
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let content = agent_chooser_lines(&state);
        // Empty state box has height 2 (text + blank), so the "No agents"
        // message is followed by a blank line before the separator.
        let Some(no_agents_idx) = content
            .lines
            .iter()
            .position(|l| l.contains("No agents available"))
        else {
            panic!("must have no-agents message");
        };
        let next_idx = no_agents_idx + 1;
        assert!(
            next_idx < content.lines.len() && content.lines[next_idx].is_empty(),
            "empty-state blank row must follow the no-agents message"
        );
    }

    #[test]
    fn agent_chooser_with_agents_exact_lines() {
        let state = AppState {
            screen_mode: crate::state::ScreenMode::DashboardIssues,
            issues_state: crate::state::IssuesState {
                agent_chooser: Some(crate::state::AgentChooserState {
                    selected_index: 0,
                    agents: vec![
                        (AgentId("a1".to_string()), "alpha".to_string()),
                        (AgentId("a2".to_string()), "beta".to_string()),
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
        assert!(content.lines[2].contains("(x) alpha"));
        assert!(content.lines[3].contains("( ) beta"));
        // Two agents → lines 2-3 are entries, line 4 is separator, line 5+ hints.
        assert!(content.lines[4].starts_with('─'));
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

    /// Build one sample of every confirm modal variant for overlay rendering
    /// tests (mirrors `all_confirm_modal_samples` in `confirm_focus_tests.rs`).
    fn overlay_confirm_modal_samples() -> Vec<crate::state::ModalState> {
        use crate::domain::{LaunchSignature, SandboxEngine};
        use crate::github::SendPayload;
        use crate::runtime::PreflightIssue;
        use crate::state::{ConfirmFocus, ModalState};

        fn sig() -> LaunchSignature {
            LaunchSignature {
                work_dir: std::path::PathBuf::from("/tmp"),
                profile: String::new(),
                code_puppy_model: String::new(),
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
            }
        }

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
                signature: sig(),
                issue: PreflightIssue::SshAgentNoIdentities,
                remaining_issues: Vec::new(),
                issue_self_assignment: None,
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmIssueDirtyCopy {
                agent_id: AgentId("a".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature: sig(),
                payload: SendPayload::default(),
                confirm_focus: ConfirmFocus::Cancel,
            },
            ModalState::ConfirmIssueOriginMismatch {
                agent_id: AgentId("a".to_string()),
                work_dir: std::path::PathBuf::from("/tmp"),
                signature: sig(),
                payload: SendPayload::default(),
                actual: String::new(),
                expected: String::new(),
                confirm_focus: ConfirmFocus::Cancel,
            },
        ]
    }
}
