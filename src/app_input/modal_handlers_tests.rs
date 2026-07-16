//! Tests for the confirm-modal key-handler predicate `confirm_focus_is_cancel`
//! (issue #228). This is the GATE that makes Enter-on-Cancel dismiss without
//! side effects.
//!
//! Extracted from `modal_handlers.rs` to keep that handler module under the
//! architecture per-file line limit.

use super::modal_handlers::{confirm_focus_is_cancel, focus_terminal_state};
use jefe::domain::{
    AgentId, AgentKind, LaunchSignature, RemoteRepositorySettings, RepositoryId, SandboxEngine,
};
use jefe::github::SendPayload;
use jefe::runtime::PreflightIssue;
use jefe::state::{AppState, ConfirmFocus, ModalState, PaneFocus};

fn sample_signature() -> LaunchSignature {
    LaunchSignature {
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
        remote: RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
        llxprt_version: None,
    }
}

/// Build all six confirm-modal variants parameterized by `focus`, so that
/// adding a new variant only requires updating one place (issue #228).
fn sample_confirm_modals(focus: ConfirmFocus) -> Vec<ModalState> {
    vec![
        ModalState::ConfirmDeleteAgent {
            id: AgentId("a1".into()),
            delete_work_dir: false,
            confirm_focus: focus,
        },
        ModalState::ConfirmDeleteRepository {
            id: RepositoryId("r1".into()),
            confirm_focus: focus,
        },
        ModalState::ConfirmKillAgent {
            id: AgentId("a1".into()),
            confirm_focus: focus,
        },
        ModalState::PreflightPrompt {
            agent_id: AgentId("a1".into()),
            signature: sample_signature(),
            issue: PreflightIssue::SshAgentNoIdentities,
            remaining_issues: Vec::new(),
            issue_self_assignment: None,
            confirm_focus: focus,
        },
        ModalState::ConfirmIssueDirtyCopy {
            agent_id: AgentId("a1".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            confirm_focus: focus,
        },
        ModalState::ConfirmIssueOriginMismatch {
            agent_id: AgentId("a1".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            actual: String::from("other/repo"),
            expected: String::from("acme/widgets"),
            confirm_focus: focus,
        },
    ]
}

/// All six confirm variants focused on Cancel must be recognized as such.
#[test]
fn confirm_focus_is_cancel_returns_true_for_cancel_focused_confirm() {
    let modals = sample_confirm_modals(ConfirmFocus::Cancel);
    for modal in &modals {
        assert!(
            confirm_focus_is_cancel(modal),
            "expected Cancel focus for {modal:?}"
        );
    }
}

/// All six confirm variants focused on Confirm must NOT be recognized as
/// Cancel.
#[test]
fn confirm_focus_is_cancel_returns_false_for_confirm_focused() {
    let modals = sample_confirm_modals(ConfirmFocus::Confirm);
    for modal in &modals {
        assert!(
            !confirm_focus_is_cancel(modal),
            "expected NOT Cancel focus for {modal:?}"
        );
    }
}

/// Non-confirm modals must return false.
#[test]
fn confirm_focus_is_cancel_returns_false_for_non_confirm_modal() {
    assert!(!confirm_focus_is_cancel(&ModalState::None));
    assert!(!confirm_focus_is_cancel(&ModalState::Help));
}

/// Proves the function reads the actual field, not a hardcoded default.
#[test]
fn confirm_focus_is_cancel_uses_correct_field_not_default() {
    let modal = ModalState::ConfirmDeleteAgent {
        id: AgentId("a1".into()),
        delete_work_dir: false,
        confirm_focus: ConfirmFocus::Confirm,
    };
    assert!(
        !confirm_focus_is_cancel(&modal),
        "must read the actual confirm_focus field, not a hardcoded Cancel default"
    );
}

#[test]
fn successful_new_agent_submit_focuses_terminal_pane_and_sets_focused() {
    let mut state = AppState {
        pane_focus: PaneFocus::Repositories,
        terminal_focused: false,
        ..AppState::default()
    };

    focus_terminal_state(&mut state);

    assert_eq!(state.pane_focus, PaneFocus::Terminal);
    assert!(state.terminal_focused);
}
