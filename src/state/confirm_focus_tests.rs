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
