use crate::published_workbench::PublishedWorkbench;
use crate::workbench::ActivationValues;

use super::provider_panels::PanelLifecycle;
use super::settings_registry_provider_tests::active_package_provider_fixture;

fn package_panel_snapshot(
    panel: crate::workbench::PanelInstanceId,
    generation: u64,
    title: &str,
) -> crate::runtime::provider::protocol::PanelSnapshot {
    use crate::runtime::provider::protocol::{BodyKind, ListBody, PanelBody, PanelSnapshot};

    PanelSnapshot {
        model_schema: super::provider_panels::MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation,
        revision: 1,
        kind: BodyKind::List,
        title: title.to_owned(),
        description: None,
        loading: false,
        action_affordances: Vec::new(),
        body: PanelBody::List(ListBody {
            items: Vec::new(),
            selected_id: None,
            next_page_token: None,
        }),
    }
}

fn accept_package_panel_snapshot(
    state: &mut super::AppState,
    owner: &crate::domain::Id,
    snapshot: &crate::runtime::provider::protocol::PanelSnapshot,
) -> Result<super::provider_panels::AcceptOutcome, super::provider_panels::PanelError> {
    use crate::runtime::provider::protocol::INITIAL_PROCESS_GENERATION;

    state
        .provider_panels_for_panel_mut(crate::workbench::PanelInstanceId::from_u64(
            snapshot.panel_instance_id,
        ))
        .unwrap_or_else(|| panic!("exact panel owner must remain routable"))
        .accept_snapshot(super::provider_panels::AcceptSnapshot {
            owner,
            received_process_generation: INITIAL_PROCESS_GENERATION,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot,
        })
}

fn project_package_panel(state: &super::AppState) -> crate::provider_panel_view::PanelProjection {
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| panic!("current package descriptor must remain published"));
    let layout = crate::screen_layout::resolve_screen(state, 120, 40)
        .unwrap_or_else(|| panic!("current package layout must resolve"));
    let mut view = crate::provider_panel_view::project_current_screen(state, descriptor, &layout)
        .unwrap_or_else(|error| panic!("current package projection: {error}"));
    assert_eq!(view.panels.len(), 1);
    view.panels.remove(0)
}

struct ProviderHealthFixture {
    state: super::AppState,
    stack: crate::domain::input_context::ContextStack,
    chord: crate::domain::keymap::Chord,
    action: crate::domain::action_registry::ActionId,
    owner: crate::domain::Id,
    published: std::sync::Arc<PublishedWorkbench>,
    registry_address: *const crate::domain::action_registry::ActionRegistrySnapshot,
    actions: Vec<crate::domain::action_registry::Action>,
    descriptor_id: crate::workbench::ScreenIdentity,
    panel_type: crate::workbench::PanelTypeId,
    first_screen: crate::workbench::ScreenInstanceId,
    first_panel: crate::workbench::PanelInstanceId,
    first_snapshot: crate::runtime::provider::protocol::PanelSnapshot,
    second_panel: crate::workbench::PanelInstanceId,
    second_snapshot: crate::runtime::provider::protocol::PanelSnapshot,
}

fn provider_health_fixture() -> ProviderHealthFixture {
    use std::sync::Arc;

    use crate::domain::Id;
    use crate::provider_panel_view::PanelRender;

    let (mut state, stack, chord, action) = active_package_provider_fixture();
    let owner = Id::parse("vendor.demo").unwrap_or_else(|error| panic!("provider owner: {error}"));
    let published = Arc::clone(state.published_workbench());
    let registry_address = std::ptr::from_ref(state.action_registry());
    let actions = state.action_registry().actions().to_vec();
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| panic!("package descriptor must be published"));
    let (descriptor_id, panel_type) = (descriptor.id, descriptor.panels[0].panel_type);
    let first_screen = state.nav.current().id;
    let first_panel = state
        .provider_panels()
        .panels_for_screen(first_screen.get())[0];
    let first_snapshot = package_panel_snapshot(first_panel, 1, "first");
    accept_package_panel_snapshot(&mut state, &owner, &first_snapshot)
        .unwrap_or_else(|error| panic!("first snapshot: {error}"));
    assert_eq!(project_package_panel(&state).render, PanelRender::Control);
    let route = state.nav.current().activation.route;
    state.enter_provider_route(route, ActivationValues::default());
    let second_screen = state.nav.current().id;
    let second_panel = state
        .provider_panels()
        .panels_for_screen(second_screen.get())[0];
    assert_ne!(first_screen, second_screen);
    assert_ne!(first_panel, second_panel);
    let first_lifecycle = state
        .provider_panels_for_panel_mut(first_panel)
        .and_then(|panels| panels.lifecycle(first_panel));
    assert_eq!(first_lifecycle, Some(PanelLifecycle::Suspended));
    let second_snapshot = package_panel_snapshot(second_panel, 1, "second");
    accept_package_panel_snapshot(&mut state, &owner, &second_snapshot)
        .unwrap_or_else(|error| panic!("second snapshot: {error}"));
    ProviderHealthFixture {
        state,
        stack,
        chord,
        action,
        owner,
        published,
        registry_address,
        actions,
        descriptor_id,
        panel_type,
        first_screen,
        first_panel,
        first_snapshot,
        second_panel,
        second_snapshot,
    }
}

fn health_availability(
    fixture: &ProviderHealthFixture,
    correlation_id: u64,
    availability: crate::domain::action_registry::Availability,
) -> crate::domain::action_registry::AvailabilityGeneration {
    use crate::domain::action_registry::{ActionAvailability, AvailabilityGeneration};
    use crate::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};

    AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(correlation_id),
            owner: fixture.owner.clone(),
            screen_generation: 1,
            activation_generation: correlation_id - 704,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "provider-health"),
        },
        vec![ActionAvailability::new(
            fixture.action.clone(),
            availability,
        )],
    )
}

fn assert_health_identity(fixture: &ProviderHealthFixture) {
    use std::sync::Arc;

    assert!(Arc::ptr_eq(
        fixture.state.published_workbench(),
        &fixture.published
    ));
    assert!(std::ptr::eq(
        fixture.state.action_registry(),
        fixture.registry_address
    ));
    assert_eq!(fixture.state.action_registry().actions(), fixture.actions);
    let descriptor = fixture
        .state
        .published_workbench()
        .screen_registry()
        .get_identity(fixture.state.screen())
        .unwrap_or_else(|| panic!("package descriptor must remain published"));
    assert_eq!(descriptor.id, fixture.descriptor_id);
    assert_eq!(descriptor.panels[0].panel_type, fixture.panel_type);
}

fn fail_provider_health(fixture: &mut ProviderHealthFixture) {
    use crate::domain::action_registry::{Availability, Resolution};
    use crate::provider_panel_view::{PanelRender, PanelStatus};

    fixture.state.action_availability = Some(health_availability(
        fixture,
        705,
        Availability::Unavailable {
            reason: "provider unavailable".to_owned(),
        },
    ));
    fixture.state.fail_provider_panels_for_owner(&fixture.owner);
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .lifecycle(fixture.second_panel),
        Some(PanelLifecycle::Failed)
    );
    let first_lifecycle = fixture
        .state
        .provider_panels_for_panel_mut(fixture.first_panel)
        .and_then(|panels| panels.lifecycle(fixture.first_panel));
    assert_eq!(first_lifecycle, Some(PanelLifecycle::Failed));
    assert_eq!(
        fixture.state.resolve_action(&fixture.chord, &fixture.stack),
        Resolution::Unavailable {
            action: fixture.action.clone(),
            reason: "provider unavailable".to_owned(),
        }
    );
    assert_health_identity(fixture);
    let projection = project_package_panel(&fixture.state);
    assert_eq!(projection.status, PanelStatus::Failed);
    assert_eq!(projection.render, PanelRender::Control);
}

fn recover_current_provider_panel(fixture: &mut ProviderHealthFixture) {
    use crate::domain::action_registry::Availability;
    use crate::provider_panel_view::PanelRender;
    use crate::runtime::provider::protocol::PanelEvent;
    use crate::state::provider_panels::PanelError;

    fixture.state.action_availability =
        Some(health_availability(fixture, 706, Availability::Available));
    assert!(
        fixture
            .state
            .submit_provider_panel_event(fixture.second_panel, PanelEvent::Retry)
    );
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .generation(fixture.second_panel),
        Some(2)
    );
    assert_eq!(
        accept_package_panel_snapshot(&mut fixture.state, &fixture.owner, &fixture.second_snapshot,),
        Err(PanelError::GenerationMismatch)
    );
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .accepted_revision(fixture.second_panel),
        None
    );
    let recovered = package_panel_snapshot(fixture.second_panel, 2, "second recovered");
    accept_package_panel_snapshot(&mut fixture.state, &fixture.owner, &recovered)
        .unwrap_or_else(|error| panic!("recovered second snapshot: {error}"));
    assert_eq!(
        project_package_panel(&fixture.state).render,
        PanelRender::Control
    );
}

fn restore_first_provider_panel(fixture: &mut ProviderHealthFixture) {
    use crate::domain::action_registry::{HandlerKey, Resolution};
    use crate::provider_panel_view::{PanelRender, PanelStatus};
    use crate::state::provider_panels::PanelError;

    fixture.state.leave_screen();
    assert_eq!(fixture.state.nav.current().id, fixture.first_screen);
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .lifecycle(fixture.first_panel),
        Some(PanelLifecycle::Activating)
    );
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .generation(fixture.first_panel),
        Some(2)
    );
    assert_eq!(
        accept_package_panel_snapshot(&mut fixture.state, &fixture.owner, &fixture.first_snapshot,),
        Err(PanelError::GenerationMismatch)
    );
    assert_eq!(
        fixture
            .state
            .provider_panels()
            .accepted_revision(fixture.first_panel),
        None
    );
    let recovered = package_panel_snapshot(fixture.first_panel, 2, "first recovered");
    accept_package_panel_snapshot(&mut fixture.state, &fixture.owner, &recovered)
        .unwrap_or_else(|error| panic!("recovered first snapshot: {error}"));
    assert_health_identity(fixture);
    let projection = project_package_panel(&fixture.state);
    assert_eq!(projection.status, PanelStatus::Active);
    assert_eq!(projection.render, PanelRender::Control);
    assert_eq!(
        fixture.state.resolve_action(&fixture.chord, &fixture.stack),
        Resolution::Dispatch {
            action: fixture.action.clone(),
            handler: HandlerKey::ProviderAction,
        }
    );
}

#[test]
fn persistent_provider_health_preserves_exact_owners_and_published_control_identity() {
    let mut fixture = provider_health_fixture();
    fail_provider_health(&mut fixture);
    recover_current_provider_panel(&mut fixture);
    restore_first_provider_panel(&mut fixture);
}
