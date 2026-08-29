//! Confirm-dialog button focus tests (issue #228).
//!
//! Reducer-level tests proving:
//! - Focus defaults to Cancel when a confirm modal opens.
//! - `ConfirmCycleFocus` toggles Cancel ↔ Confirm.
//! - `ConfirmCycleFocus` is a no-op for non-confirm modals.
//! - `ToggleDeleteWorkDir` preserves the focus value.
//! - The `ConfirmFocus` enum default is Cancel, pinned by
//!   `confirm_focus_default_is_cancel`; every production modal-opening site
//!   routes through `open_confirmation_payload`, which focuses the safe
//!   Cancel choice on the instance's declared Confirmation overlay.

use super::{AppEvent, AppState, ConfirmFocus, ModalState};
use super::screen_overlays::ConfirmationRequest;
use crate::domain::{AgentId, AgentLaunchRequest, RepositoryId};
use crate::github::SendPayload;
use crate::state::transition::TransitionExt;

fn sample_signature() -> AgentLaunchRequest {
    AgentLaunchRequest {
        type_id: crate::domain::shipped_agent_type(3),
        values: crate::domain::TypedMap::new(),
        work_dir: std::path::PathBuf::from("/tmp"),
        remote: crate::domain::RemoteRepositorySettings::default(),
        operation: crate::domain::agent_definition::Operation::Normal,
    }
}

#[test]
fn confirm_focus_defaults_to_cancel_on_open_delete_agent() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())))
        .committed_pure();

    assert!(matches!(
        state.nav.current().overlays().generic_confirmation(),
        Some(ConfirmationRequest::DeleteAgent { .. })
    ));
    assert_eq!(
        state.current_confirm_focus(),
        Some(ConfirmFocus::Cancel),
        "destructive confirm must default to Cancel"
    );
}

#[test]
fn confirm_focus_defaults_to_cancel_on_open_delete_repository() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteRepository(RepositoryId("r1".into())))
        .committed_pure();

    assert!(matches!(
        state.nav.current().overlays().generic_confirmation(),
        Some(ConfirmationRequest::DeleteRepository { .. })
    ));
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
}

#[test]
fn confirm_cycle_focus_toggles_cancel_to_confirm() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())))
        .committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));
}

#[test]
fn confirm_cycle_focus_toggles_confirm_to_cancel() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())))
        .committed_pure()
        .apply(AppEvent::ConfirmCycleFocus)
        .committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
}

#[test]
fn confirm_cycle_focus_noop_on_non_confirm_modal() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenHelp)
        .committed_pure();

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(
        state.active_overlay_kind(),
        Some(crate::workbench::OverlayKind::Help),
        "ConfirmCycleFocus must not change Help"
    );

    let state2 = AppState::test_fixture()
        .apply(AppEvent::ConfirmCycleFocus)
        .committed_pure();
    assert_eq!(state2.modal, ModalState::None);
}

#[test]
fn toggle_delete_work_dir_preserves_confirm_focus() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())))
        .committed_pure()
        .apply(AppEvent::ConfirmCycleFocus)
        .committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));

    let state = state.apply(AppEvent::ToggleDeleteWorkDir).committed_pure();

    match state.nav.current().overlays().generic_confirmation() {
        Some(ConfirmationRequest::DeleteAgent {
            delete_work_dir, ..
        }) => {
            assert!(*delete_work_dir, "toggle should flip to true");
            assert_eq!(
                state.current_confirm_focus(),
                Some(ConfirmFocus::Confirm),
                "toggle must preserve the focused choice"
            );
        }
        ref other => panic!("expected ConfirmDeleteAgent, got {other:?}"),
    }
}

/// The ConfirmFocus default MUST be Cancel so that any confirm modal
/// opened via Default::default() (or any opening site that relies on the
/// enum default) lands on the safe, non-destructive button (issue #228).
/// This is the structural guarantee behind "destructive confirms default to
/// Cancel" — every production opening site also focuses Cancel explicitly,
/// but this test pins the enum-level safety net.
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
    let mut state = AppState::test_fixture();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::IssueDirtyCopy {
            agent_id: AgentId("a1".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
        })
    );

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));
}

#[test]
fn close_modal_dismisses_confirm_without_side_effect() {
    let state = AppState::test_fixture()
        .apply(AppEvent::OpenDeleteAgent(AgentId("a1".into())))
        .committed_pure()
        .apply(AppEvent::CloseModal)
        .committed_pure();
    assert_eq!(state.modal, ModalState::None);
}

/// Every generic confirmation request must be recognized by the exact-instance
/// focus machinery. The focus is driven through the public reducer API used by
/// the binary.
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
        ModalState::NewAgent {
            repository_id: RepositoryId("r".into()),
            fields: crate::state::AgentFormFields::default(),
            focus: crate::state::AgentFormFocus::default(),
            cursor: crate::state::AgentFormCursor::default(),
            work_dir_manual: false,
        },
    ];
    for modal in non_confirms {
        let mut state = AppState::test_fixture();
        state.modal = modal.clone();
        assert_eq!(
            state.current_confirm_focus(),
            None,
            "non-confirm variant must return None: {modal:?}"
        );
    }
}

/// Build one sample of every generic confirmation request.
fn all_confirm_modal_samples() -> Vec<ConfirmationRequest> {
    use crate::runtime::PreflightIssue;
    vec![
        ConfirmationRequest::DeleteAgent {
            id: AgentId("a".into()),
            delete_work_dir: false,
        },
        ConfirmationRequest::DeleteRepository {
            id: RepositoryId("r".into()),
        },
        ConfirmationRequest::KillAgent {
            id: AgentId("a".into()),
        },
        ConfirmationRequest::ServerLostRecovery {
            agent_ids: vec![AgentId("a".into())],
        },
        ConfirmationRequest::Preflight {
            agent_id: AgentId("a".into()),
            signature: sample_signature(),
            issue: PreflightIssue::SshAgentNoIdentities,
            remaining_issues: Vec::new(),
            issue_self_assignment: None,
        },
        ConfirmationRequest::IssueDirtyCopy {
            agent_id: AgentId("a".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
        },
        ConfirmationRequest::IssueOriginMismatch {
            agent_id: AgentId("a".into()),
            work_dir: std::path::PathBuf::from("/tmp"),
            signature: sample_signature(),
            payload: SendPayload::default(),
            actual: String::new(),
            expected: String::new(),
        },
    ]
}

/// Assert that a single confirm variant is recognized by the focus machinery
/// and that cycling focus via the public reducer toggles Cancel ↔ Confirm.
fn assert_confirm_recognized_and_cycles(request: ConfirmationRequest) {
    let mut state = AppState::test_fixture();
    assert!(
        state.open_confirmation_payload(request.clone()),
        "confirmation must open the declared overlay: {request:?}"
    );
    assert!(
        state.current_confirm_focus().is_some(),
        "confirmation must be recognized by current_confirm_focus: {request:?}"
    );

    let toggled = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(
        toggled.current_confirm_focus(),
        Some(ConfirmFocus::Confirm),
        "ConfirmCycleFocus must toggle to Confirm for: {request:?}"
    );

    let restored = toggled.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(
        restored.current_confirm_focus(),
        Some(ConfirmFocus::Cancel),
        "ConfirmCycleFocus must toggle back to Cancel for: {request:?}"
    );
}
