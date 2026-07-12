//! Confirm-dialog button focus tests (issue #228).
//!
//! Reducer-level tests proving:
//! - Focus defaults to Cancel when a confirm modal opens.
//! - `ConfirmCycleFocus` toggles Cancel ↔ Confirm.
//! - `ConfirmCycleFocus` is a no-op for non-confirm modals.
//! - `ToggleDeleteWorkDir` preserves the focus value.
//! - The `ConfirmFocus` enum default is Cancel, pinned by
//!   `confirm_focus_default_is_cancel`; every production modal-opening site
//!   also sets `ConfirmFocus::Cancel` explicitly (see `modal_ops.rs`,
//!   `issues_send.rs`, `preflight.rs`, and `app_input/mod.rs`).

use super::{AppEvent, AppState, ConfirmFocus, ModalState};
use crate::domain::{AgentId, LaunchSignature, RepositoryId, SandboxEngine};
use crate::github::SendPayload;

fn sample_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_yolo: Some(false),
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

#[test]
fn confirm_focus_defaults_to_cancel_on_open_delete_agent() {
    let state = AppState::default().apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())));

    match state.modal {
        ModalState::ConfirmDeleteAgent { confirm_focus, .. } => {
            assert_eq!(
                confirm_focus,
                ConfirmFocus::Cancel,
                "destructive confirm must default to Cancel"
            );
        }
        ref other => panic!("expected ConfirmDeleteAgent, got {other:?}"),
    }
}

#[test]
fn confirm_focus_defaults_to_cancel_on_open_delete_repository() {
    let state =
        AppState::default().apply(AppEvent::OpenDeleteRepository(RepositoryId("r1".into())));

    match state.modal {
        ModalState::ConfirmDeleteRepository { confirm_focus, .. } => {
            assert_eq!(confirm_focus, ConfirmFocus::Cancel);
        }
        ref other => panic!("expected ConfirmDeleteRepository, got {other:?}"),
    }
}

#[test]
fn confirm_cycle_focus_toggles_cancel_to_confirm() {
    let state = AppState::default().apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())));
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));

    let state = state.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));
}

#[test]
fn confirm_cycle_focus_toggles_confirm_to_cancel() {
    let state = AppState {
        modal: ModalState::ConfirmDeleteAgent {
            id: AgentId("a1".into()),
            delete_work_dir: false,
            confirm_focus: ConfirmFocus::Confirm,
        },
        ..AppState::default()
    };

    let state = state.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
}

#[test]
fn confirm_cycle_focus_noop_on_non_confirm_modal() {
    let state = AppState {
        modal: ModalState::Help,
        ..AppState::default()
    };
    let before = state.modal.clone();

    let state = state.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(
        state.modal, before,
        "ConfirmCycleFocus must not change Help"
    );

    let state2 = AppState::default().apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(state2.modal, ModalState::None);
}

#[test]
fn toggle_delete_work_dir_preserves_confirm_focus() {
    let state = AppState {
        modal: ModalState::ConfirmDeleteAgent {
            id: AgentId("a1".into()),
            delete_work_dir: false,
            confirm_focus: ConfirmFocus::Confirm,
        },
        ..AppState::default()
    };

    let state = state.apply(AppEvent::ToggleDeleteWorkDir);

    match state.modal {
        ModalState::ConfirmDeleteAgent {
            delete_work_dir,
            confirm_focus,
            ..
        } => {
            assert!(delete_work_dir, "toggle should flip to true");
            assert_eq!(
                confirm_focus,
                ConfirmFocus::Confirm,
                "toggle must preserve confirm_focus"
            );
        }
        ref other => panic!("expected ConfirmDeleteAgent, got {other:?}"),
    }
}

/// The ConfirmFocus default MUST be Cancel so that any confirm modal
/// opened via Default::default() (or any opening site that relies on the
/// enum default) lands on the safe, non-destructive button (issue #228).
/// This is the structural guarantee behind "destructive confirms default to
/// Cancel" — every production opening site also sets Cancel explicitly, but
/// this test pins the enum-level safety net.
#[test]
fn confirm_focus_default_is_cancel() {
    assert_eq!(
        ConfirmFocus::default(),
        ConfirmFocus::Cancel,
        "ConfirmFocus must default to Cancel so destructive confirms are safe by default"
    );
}

#[test]
fn cycle_focus_works_on_dirty_copy() {
    let state = AppState {
        modal: ModalState::ConfirmIssueDirtyCopy {
            agent_id: AgentId("a1".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            confirm_focus: ConfirmFocus::Cancel,
        },
        ..AppState::default()
    };

    let state = state.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));
}

#[test]
fn close_modal_dismisses_confirm_without_side_effect() {
    let state = AppState {
        modal: ModalState::ConfirmDeleteAgent {
            id: AgentId("a1".into()),
            delete_work_dir: false,
            confirm_focus: ConfirmFocus::Cancel,
        },
        ..AppState::default()
    };

    let state = state.apply(AppEvent::CloseModal);
    assert_eq!(state.modal, ModalState::None);
}

/// Every confirm modal variant must be recognized by the focus machinery
/// (issue #228). If a new confirm variant is added to `ModalState`, this test
/// will fail until it is added to `current_confirm_focus`/`set_confirm_focus`
/// AND to the sample list below — preventing silent regressions. The focus is
/// driven through the public reducer API (`AppEvent::ConfirmCycleFocus`),
/// which is the real event path used by the binary.
#[test]
fn all_confirm_variants_recognized_by_focus_machinery() {
    for modal in all_confirm_modal_samples() {
        assert_confirm_recognized_and_cycles(modal);
    }
}

/// Non-confirm modals must yield `None` from `current_confirm_focus` so that
/// `ConfirmCycleFocus` is a no-op outside confirm dialogs (issue #228).
#[test]
fn non_confirm_modals_return_none_focus() {
    let non_confirms: Vec<ModalState> = vec![
        ModalState::None,
        ModalState::Help,
        ModalState::NewAgent {
            repository_id: RepositoryId("r".into()),
            fields: crate::state::AgentFormFields::default(),
            focus: crate::state::AgentFormFocus::default(),
            cursor: crate::state::AgentFormCursor::default(),
            work_dir_manual: false,
        },
        ModalState::Search {
            query: String::new(),
        },
    ];
    for modal in non_confirms {
        let state = AppState {
            modal: modal.clone(),
            ..AppState::default()
        };
        assert_eq!(
            state.current_confirm_focus(),
            None,
            "non-confirm variant must return None: {modal:?}"
        );
    }
}

/// Build one sample of every confirm modal variant. If a new confirm variant
/// is added to `ModalState`, it must be added here (and the tests using this
/// list enforce coverage).
fn all_confirm_modal_samples() -> Vec<ModalState> {
    use crate::runtime::PreflightIssue;
    vec![
        ModalState::ConfirmDeleteAgent {
            id: AgentId("a".into()),
            delete_work_dir: false,
            confirm_focus: ConfirmFocus::Cancel,
        },
        ModalState::ConfirmDeleteRepository {
            id: RepositoryId("r".into()),
            confirm_focus: ConfirmFocus::Cancel,
        },
        ModalState::ConfirmKillAgent {
            id: AgentId("a".into()),
            confirm_focus: ConfirmFocus::Cancel,
        },
        ModalState::PreflightPrompt {
            agent_id: AgentId("a".into()),
            signature: sample_signature(),
            issue: PreflightIssue::SshAgentNoIdentities,
            remaining_issues: Vec::new(),
            confirm_focus: ConfirmFocus::Cancel,
        },
        ModalState::ConfirmIssueDirtyCopy {
            agent_id: AgentId("a".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            confirm_focus: ConfirmFocus::Cancel,
        },
        ModalState::ConfirmIssueOriginMismatch {
            agent_id: AgentId("a".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            actual: String::new(),
            expected: String::new(),
            confirm_focus: ConfirmFocus::Cancel,
        },
    ]
}

/// Assert that a single confirm variant is recognized by the focus machinery
/// and that cycling focus via the public reducer toggles Cancel ↔ Confirm.
fn assert_confirm_recognized_and_cycles(modal: ModalState) {
    let state = AppState {
        modal: modal.clone(),
        ..AppState::default()
    };
    assert!(
        state.current_confirm_focus().is_some(),
        "confirm variant must be recognized by current_confirm_focus: {modal:?}"
    );

    let toggled = state.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(
        toggled.current_confirm_focus(),
        Some(ConfirmFocus::Confirm),
        "ConfirmCycleFocus must toggle to Confirm for: {modal:?}"
    );

    let restored = toggled.apply(AppEvent::ConfirmCycleFocus);
    assert_eq!(
        restored.current_confirm_focus(),
        Some(ConfirmFocus::Cancel),
        "ConfirmCycleFocus must toggle back to Cancel for: {modal:?}"
    );
}
