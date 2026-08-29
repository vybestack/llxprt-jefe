//! Declared generic-confirmation overlay tests (issue #705 S6).
//!
//! The seven generic confirmation payloads carry no focus of their own: the
//! live decision focus is owned by the screen instance's declared
//! Confirmation overlay, opened only when the active instance declares it.

use super::screen_overlays::{ActiveOverlay, ConfirmationRequest};
use super::{AppEvent, AppState, ConfirmFocus, ModalState};
use crate::github::SendPayload;
use crate::state::transition::TransitionExt;
use crate::test_support::Must;
use crate::workbench::{
    ActivationValues, LayoutNode, PanelDescriptor, PanelId, PanelTypeId, PluginScreenId, RouteId,
    ScreenDescriptor, ScreenIdentity, ScreenRegistry, builtin_screens, intern,
};

use super::navigation::{
    Activation, NavIntent, NavMessage, NavOutcome, NavState, reduce_navigation,
};

const PACKAGE_SCREEN: &str = "vendor.pkg.review";
const PACKAGE_ROUTE: &str = "pkg-review";

fn package_descriptor() -> ScreenDescriptor {
    let id = intern(PACKAGE_SCREEN).must("interning a test identifier must succeed");
    let route = RouteId::parse(intern(PACKAGE_ROUTE).must("intern must succeed"))
        .must("the route identifier satisfies the grammar");
    let panel = PanelId::parse(intern("list").must("intern must succeed"))
        .must("the panel identifier satisfies the grammar");
    let panel_type = PanelTypeId::parse(intern("vendor.pkg.list").must("intern must succeed"))
        .must("the panel-type identifier satisfies the grammar");
    ScreenDescriptor {
        id: ScreenIdentity::Package(
            PluginScreenId::parse(id).must("the plugin screen identifier satisfies the grammar"),
        ),
        title: "Pkg Review".to_owned(),
        route,
        panels: vec![PanelDescriptor {
            id: panel,
            panel_type,
            host_capability: None,
            config: crate::domain::TypedMap::new(),
            focusable: true,
            required: true,
            ports: Vec::new(),
        }],
        initial_focus: panel,
        focus_order: vec![panel],
        layout: LayoutNode::Leaf { panel },
        relationships: Vec::new(),
        activation: Vec::new(),
        overlays: Vec::new(),
        host_capabilities: Vec::new(),
        bindings: Vec::new(),
    }
}

fn registry_with_package() -> ScreenRegistry {
    let mut screens = builtin_screens()
        .must("the compiled screen table is well formed")
        .screens()
        .to_vec();
    screens.push(package_descriptor());
    ScreenRegistry::new(screens).must("the composed registry is well formed")
}

fn package_request(state: &NavState) -> Activation {
    Activation::from_source(
        RouteId::parse(intern(PACKAGE_ROUTE).must("intern must succeed"))
            .must("the route identifier satisfies the grammar"),
        ActivationValues::empty(),
        state.current(),
    )
}

fn launch_signature() -> crate::domain::AgentLaunchRequest {
    crate::domain::AgentLaunchRequest {
        type_id: crate::domain::shipped_agent_type(3),
        values: crate::domain::TypedMap::new(),
        work_dir: std::path::PathBuf::from("/tmp"),
        remote: crate::domain::RemoteRepositorySettings::default(),
        operation: crate::domain::agent_definition::Operation::Normal,
    }
}

fn send_payload() -> SendPayload {
    SendPayload::default()
}

fn every_generic_confirmation_payload() -> Vec<ConfirmationRequest> {
    let agent_id = crate::domain::AgentId("a1".into());
    vec![
        ConfirmationRequest::DeleteRepository {
            id: crate::domain::RepositoryId("r1".into()),
        },
        ConfirmationRequest::DeleteAgent {
            id: agent_id.clone(),
            delete_work_dir: false,
        },
        ConfirmationRequest::KillAgent {
            id: agent_id.clone(),
        },
        ConfirmationRequest::ServerLostRecovery {
            agent_ids: vec![agent_id.clone()],
        },
        ConfirmationRequest::Preflight {
            agent_id: agent_id.clone(),
            signature: launch_signature(),
            issue: crate::runtime::PreflightIssue::SshAgentNoIdentities,
            remaining_issues: Vec::new(),
            issue_self_assignment: None,
        },
        ConfirmationRequest::IssueDirtyCopy {
            agent_id: agent_id.clone(),
            work_dir: std::path::PathBuf::from("/tmp/repo"),
            signature: launch_signature(),
            payload: send_payload(),
        },
        ConfirmationRequest::IssueOriginMismatch {
            agent_id,
            work_dir: std::path::PathBuf::from("/tmp/repo"),
            signature: launch_signature(),
            payload: send_payload(),
            actual: "git@github.com:acme/wrong.git".into(),
            expected: "git@github.com:acme/right.git".into(),
        },
    ]
}

#[test]
fn every_generic_confirmation_opens_the_declared_instance_overlay_on_cancel() {
    for payload in every_generic_confirmation_payload() {
        let mut state = AppState::test_fixture();
        assert!(
            state.open_confirmation_payload(payload.clone()),
            "a declared instance must accept the confirmation"
        );
        assert_eq!(
            state.nav.current().overlays().active(),
            Some(&ActiveOverlay::GenericConfirmation {
                request: Box::new(payload),
                focus: ConfirmFocus::Cancel,
            }),
            "opening must focus the safe Cancel choice"
        );
        assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
        assert!(state.blocking_overlay_owns_mouse());
    }
}

#[test]
fn back_restores_the_exact_owner_of_a_suspended_generic_confirmation() {
    let mut state = AppState::test_fixture();
    let owner = state.nav.current().id;
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("agent-1".to_owned()),
        })
    );
    state.enter_screen(crate::workbench::ScreenId::Issues);
    assert_ne!(state.nav.current().id, owner);

    state = state.apply(AppEvent::Back).committed_pure();

    assert_eq!(state.nav.current().id, owner);
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
}

#[test]
fn every_generic_confirmation_terminal_close_clears_payload_and_exact_overlay() {
    for payload in every_generic_confirmation_payload() {
        let mut state = AppState::test_fixture();
        assert!(state.open_confirmation_payload(payload));

        assert!(state.close_generic_confirmation());
        assert!(matches!(state.modal, ModalState::None));
        assert_eq!(state.nav.current().overlays().active(), None);
    }
}

#[test]
fn expected_payload_close_rejects_a_valid_replacement_without_mutation() {
    let original = ConfirmationRequest::DeleteRepository {
        id: crate::domain::RepositoryId("r1".into()),
    };
    let replacement = ConfirmationRequest::DeleteRepository {
        id: crate::domain::RepositoryId("r2".into()),
    };
    let mut state = AppState::test_fixture();
    assert!(state.open_confirmation_payload(original.clone()));
    let before_overlay = state.nav.current().overlays().active().cloned();

    assert!(!state.close_expected_generic_confirmation(&replacement));
    assert_eq!(
        state.nav.current().overlays().active(),
        before_overlay.as_ref()
    );
    assert!(state.close_expected_generic_confirmation(&original));
    assert!(matches!(state.modal, ModalState::None));
    assert_eq!(state.nav.current().overlays().active(), None);
}

#[test]
fn generic_form_submission_cannot_split_confirmation_payload_from_its_overlay() {
    let payload = ConfirmationRequest::KillAgent {
        id: crate::domain::AgentId("a1".into()),
    };
    let mut state = AppState::test_fixture();
    assert!(state.open_confirmation_payload(payload.clone()));
    let overlay = state.nav.current().overlays().active().cloned();

    let state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert!(matches!(state.modal, ModalState::None));
    assert_eq!(
        state.nav.current().overlays().generic_confirmation(),
        Some(&payload)
    );
    assert_eq!(state.nav.current().overlays().active(), overlay.as_ref());
}

#[test]
fn an_undeclared_confirmation_refuses_to_open_the_payload() {
    let registry = registry_with_package();
    let mut nav = NavState::default();
    let request = package_request(&nav);
    nav = reduce_navigation(
        nav,
        &registry,
        NavMessage::Navigate(NavIntent::Push(request)),
    )
    .state;
    assert_eq!(
        nav.depth(),
        1,
        "the undeclaring package screen must be current"
    );

    let mut state = AppState::test_fixture();
    state.nav = nav;
    assert!(
        !state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("a1".into()),
        }),
        "an instance that declares no Confirmation must refuse the payload"
    );
    assert_eq!(state.nav.current().overlays().active(), None);
    assert!(matches!(state.modal, ModalState::None));
}

#[test]
fn cycling_focus_moves_only_the_instance_overlay() {
    let mut state = AppState::test_fixture();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("a1".into()),
        })
    );

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Confirm));
    assert!(matches!(
        state.nav.current().overlays().active(),
        Some(ActiveOverlay::GenericConfirmation {
            focus: ConfirmFocus::Confirm,
            ..
        })
    ));

    let state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    assert_eq!(state.current_confirm_focus(), Some(ConfirmFocus::Cancel));
}

#[test]
fn closing_a_confirmation_releases_the_instance_overlay() {
    let mut state = AppState::test_fixture();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("a1".into()),
        })
    );

    let state = state.apply(AppEvent::CloseModal).committed_pure();
    assert!(matches!(state.modal, ModalState::None));
    assert_eq!(state.nav.current().overlays().active(), None);
    assert_eq!(state.current_confirm_focus(), None);
}

fn assert_suspended_payload_inert(state: &AppState) {
    assert!(
        crate::ui::orchestration::derive_confirm_modal_data(state).is_none(),
        "a suspended payload cannot project render data on another instance"
    );
    assert!(
        crate::selection::pane_content_lines(
            crate::selection::SelectablePane::ConfirmModal,
            state,
            None,
            &[],
            120,
            40,
        )
        .lines
        .is_empty(),
        "a suspended payload cannot leak into selection content"
    );
    assert_eq!(
        crate::input::input_mode_for_state(state),
        crate::input::InputMode::Normal,
        "a suspended payload cannot intercept keyboard input"
    );
}

#[test]
fn confirmation_focus_survives_suspend_and_restore() {
    let registry = registry_with_package();
    let mut state = AppState::test_fixture();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::KillAgent {
            id: crate::domain::AgentId("a1".into()),
        })
    );
    let mut state = state.apply(AppEvent::ConfirmCycleFocus).committed_pure();
    let request = package_request(&state.nav);
    state.nav = reduce_navigation(
        state.nav,
        &registry,
        NavMessage::Navigate(NavIntent::Push(request)),
    )
    .state;
    assert!(state.nav.current().overlays().active().is_none());
    assert_eq!(state.current_confirm_focus(), None);
    assert!(
        !matches!(
            state.back_resolution(),
            super::navigation_unwind::BackResolution::Local(
                super::navigation_unwind::LocalIntent::CloseHostConfirmation
            )
        ),
        "a suspended confirmation cannot own Back on another instance"
    );
    assert_suspended_payload_inert(&state);
    state = state.apply(AppEvent::CloseModal).committed_pure();
    assert!(matches!(state.modal, ModalState::None));
    assert_suspended_payload_inert(&state);

    let restored = reduce_navigation(state.nav, &registry, NavMessage::Navigate(NavIntent::Back));
    assert!(matches!(restored.outcome, NavOutcome::Restored { .. }));
    assert!(
        matches!(
            restored.state.current().overlays().active(),
            Some(ActiveOverlay::GenericConfirmation {
                request,
                focus: ConfirmFocus::Confirm,
            }) if matches!(request.as_ref(), ConfirmationRequest::KillAgent { .. })
        ),
        "the suspended instance must restore its own request and decision focus"
    );
}
