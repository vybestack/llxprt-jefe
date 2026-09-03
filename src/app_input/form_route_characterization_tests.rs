//! Production key-route characterization for retained Edit Agent and Edit Repository behavior.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};
use jefe::domain::action_registry::{HandlerKey, Resolution};
use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId, TypedMap};
use jefe::list_viewport::PageItemCount;
use jefe::state::{AgentFormFocus, AppEvent, AppState, ModalState, PaneFocus};

use super::action_handlers::{BoundaryAction, HandlerExecution, execution_for};
use super::{new_agent_submit, raw_key_mutations};

#[derive(Debug, PartialEq, Eq)]
enum ProductionRoute {
    RawKey,
    RegistryEvent(HandlerKey),
    RegistryBoundary(HandlerKey),
}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeAttachment {
    Bound,
    Unbound,
}

#[derive(Debug, PartialEq, Eq)]
struct AgentRuntimeObservation {
    status: AgentStatus,
    attachment: RuntimeAttachment,
}

#[derive(Debug, PartialEq, Eq)]
struct AgentRuntimeSnapshot(HashMap<AgentId, AgentRuntimeObservation>);

impl AgentRuntimeSnapshot {
    fn capture(state: &AppState) -> Self {
        Self(
            state
                .agents
                .iter()
                .map(|agent| {
                    let attachment = if agent.runtime_binding.is_some() {
                        RuntimeAttachment::Bound
                    } else {
                        RuntimeAttachment::Unbound
                    };
                    (
                        agent.id.clone(),
                        AgentRuntimeObservation {
                            status: agent.status,
                            attachment,
                        },
                    )
                })
                .collect(),
        )
    }
}

struct RouteHarness {
    state: AppState,
    package_probe_calls: Cell<usize>,
}

impl RouteHarness {
    fn new(state: AppState) -> Self {
        Self {
            state,
            package_probe_calls: Cell::new(0),
        }
    }

    fn assert_route(&mut self, code: KeyCode, expected: ProductionRoute) {
        assert_eq!(
            route_key(&mut self.state, &key(code), &self.package_probe_calls),
            expected
        );
    }

    fn type_text(&mut self, text: &str) {
        type_text(&mut self.state, text, &self.package_probe_calls);
    }

    fn assert_no_package_probes(&self, message: &str) {
        assert_eq!(self.package_probe_calls.get(), 0, "{message}");
    }
}

struct AgentEditFixture {
    routes: RouteHarness,
    target_id: AgentId,
    other_id: AgentId,
    runtime_before: AgentRuntimeSnapshot,
}

impl AgentEditFixture {
    fn new() -> Self {
        let repository_id = RepositoryId("repo.route".to_owned());
        let target_id = AgentId("agent.target".to_owned());
        let other_id = AgentId("agent.other".to_owned());
        let mut route_repository = repository(
            &repository_id.0,
            "Route repository",
            "/tmp/issue727-route-repository",
        );
        route_repository.agent_ids = vec![other_id.clone(), target_id.clone()];
        let mut state = crate::test_app_state();
        state.repositories = vec![route_repository];
        state.agents = vec![
            agent(
                &other_id.0,
                &repository_id,
                "other",
                "/tmp/issue727-route-repository/other",
            ),
            agent(
                &target_id.0,
                &repository_id,
                "target",
                "/tmp/issue727-route-repository/target",
            ),
        ];
        state.selected_repository_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        select_agent(&mut state, &target_id);
        let runtime_before = AgentRuntimeSnapshot::capture(&state);
        Self {
            routes: RouteHarness::new(state),
            target_id,
            other_id,
            runtime_before,
        }
    }

    fn open_target(&mut self, expected_name: &str) {
        select_agent(&mut self.routes.state, &self.target_id);
        self.routes.assert_route(
            KeyCode::Enter,
            ProductionRoute::RegistryEvent(HandlerKey::ActivateDashboardSelection),
        );
        assert_edit_agent_draft(&self.routes.state, &self.target_id, expected_name);
    }

    fn focus_name_field(&mut self) {
        self.routes.assert_route(
            KeyCode::Tab,
            ProductionRoute::RegistryEvent(HandlerKey::FormNextField),
        );
        assert!(matches!(
            self.routes.state.modal,
            ModalState::EditAgent {
                focus: AgentFormFocus::Name,
                ..
            }
        ));
    }

    fn type_corrected_suffix(&mut self) {
        self.routes.type_text("X");
        self.routes
            .assert_route(KeyCode::Backspace, ProductionRoute::RawKey);
        self.routes.type_text("-edited");
    }

    fn submit_after_reorder(&mut self) {
        self.routes.state.agents.swap(0, 1);
        self.routes.assert_route(
            KeyCode::Enter,
            ProductionRoute::RegistryBoundary(HandlerKey::FormSubmit),
        );
        assert!(matches!(self.routes.state.modal, ModalState::None));
        assert_eq!(
            agent_name(&self.routes.state, &self.target_id),
            Some("target-edited")
        );
        assert_eq!(
            agent_name(&self.routes.state, &self.other_id),
            Some("other"),
            "the reordered non-target agent must remain unchanged"
        );
        self.assert_no_agent_side_effects();
    }

    fn type_then_cancel(&mut self, text: &str) {
        self.routes.type_text(text);
        self.routes.assert_route(
            KeyCode::Esc,
            ProductionRoute::RegistryEvent(HandlerKey::FormCancel),
        );
        assert!(matches!(self.routes.state.modal, ModalState::None));
    }

    fn assert_no_agent_side_effects(&self) {
        self.routes
            .assert_no_package_probes("Edit Agent must not launch or probe");
        assert!(
            !self.routes.state.terminal_focused,
            "Edit Agent must not focus the terminal"
        );
        assert_eq!(self.routes.state.pane_focus, PaneFocus::Agents);
        assert_eq!(
            AgentRuntimeSnapshot::capture(&self.routes.state),
            self.runtime_before,
            "Edit Agent must not change runtime status or attachment"
        );
    }
}

struct RepositoryEditFixture {
    routes: RouteHarness,
    target_id: RepositoryId,
    other_id: RepositoryId,
}

impl RepositoryEditFixture {
    fn new() -> Self {
        let target_id = RepositoryId("repo.target".to_owned());
        let other_id = RepositoryId("repo.other".to_owned());
        let mut state = crate::test_app_state();
        state.repositories = vec![
            repository(&other_id.0, "other", "/tmp/issue727-other"),
            repository(&target_id.0, "target", "/tmp/issue727-target"),
        ];
        state.pane_focus = PaneFocus::Repositories;
        select_repository(&mut state, &target_id);
        Self {
            routes: RouteHarness::new(state),
            target_id,
            other_id,
        }
    }

    fn open_target(&mut self, expected_name: &str) {
        select_repository(&mut self.routes.state, &self.target_id);
        self.routes.assert_route(
            KeyCode::Enter,
            ProductionRoute::RegistryEvent(HandlerKey::ActivateDashboardSelection),
        );
        assert_edit_repository_draft(&self.routes.state, &self.target_id, expected_name);
    }

    fn type_corrected_name_and_traverse(&mut self) {
        self.routes.type_text("-editd");
        self.routes
            .assert_route(KeyCode::Backspace, ProductionRoute::RawKey);
        self.routes.type_text("ed");
        self.routes.assert_route(
            KeyCode::Tab,
            ProductionRoute::RegistryEvent(HandlerKey::FormNextField),
        );
    }

    fn submit_after_reorder(&mut self) {
        self.routes.state.repositories.swap(0, 1);
        self.routes.assert_route(
            KeyCode::Enter,
            ProductionRoute::RegistryBoundary(HandlerKey::FormSubmit),
        );
        assert!(matches!(self.routes.state.modal, ModalState::None));
        assert_eq!(
            repository_name(&self.routes.state, &self.target_id),
            Some("target-edited")
        );
        assert_eq!(
            repository_name(&self.routes.state, &self.other_id),
            Some("other"),
            "the reordered non-target repository must remain unchanged"
        );
        self.routes
            .assert_no_package_probes("repository edits never probe packages");
    }

    fn type_then_cancel(&mut self, text: &str) {
        self.routes.type_text(text);
        self.routes.assert_route(
            KeyCode::Esc,
            ProductionRoute::RegistryEvent(HandlerKey::FormCancel),
        );
        assert!(matches!(self.routes.state.modal, ModalState::None));
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn commit(state: &mut AppState, event: AppEvent) {
    jefe::state::transition::commit_pure_site(state, event.into());
}

fn route_key(
    state: &mut AppState,
    event: &KeyEvent,
    package_probe_calls: &Cell<usize>,
) -> ProductionRoute {
    if let Some(raw_event) = raw_key_mutations::resolve(state, event) {
        commit(state, raw_event);
        return ProductionRoute::RawKey;
    }

    let resolved = crate::app_shell_key_routing::resolve_compiled_registry_key(state, event);
    let Resolution::Dispatch { handler, .. } = resolved.resolution else {
        panic!("production registry must dispatch {event:?}, got {resolved:?}");
    };
    match execution_for(handler, resolved.chord, state, PageItemCount::new(1)) {
        HandlerExecution::Event(app_event) => {
            commit(state, app_event);
            ProductionRoute::RegistryEvent(handler)
        }
        HandlerExecution::Boundary(BoundaryAction::FormSubmit) => {
            let plan = new_agent_submit::new_agent_package_probe_plan(state);
            let probe_result = new_agent_submit::execute_new_agent_package_probe(&plan, |_| {
                package_probe_calls.set(package_probe_calls.get() + 1);
                Ok::<(), &'static str>(())
            });
            assert!(
                new_agent_submit::apply_form_submit_after_package_probe(state, probe_result),
                "the edit-form submit boundary must accept its probe-free plan"
            );
            ProductionRoute::RegistryBoundary(handler)
        }
        other => panic!("unexpected production execution for {event:?}: {other:?}"),
    }
}

fn type_text(state: &mut AppState, text: &str, package_probe_calls: &Cell<usize>) {
    for character in text.chars() {
        assert_eq!(
            route_key(state, &key(KeyCode::Char(character)), package_probe_calls),
            ProductionRoute::RawKey,
            "typed form text must use the production raw-key fallback"
        );
    }
}

fn repository(id: &str, name: &str, base_dir: &str) -> Repository {
    Repository::new(
        RepositoryId(id.to_owned()),
        jefe::domain::shipped_agent_type(3),
        TypedMap::new(),
        name.to_owned(),
        id.to_owned(),
        PathBuf::from(base_dir),
    )
}

fn agent(id: &str, repository_id: &RepositoryId, name: &str, work_dir: &str) -> Agent {
    Agent::new(
        AgentId(id.to_owned()),
        repository_id.clone(),
        jefe::domain::shipped_agent_type(3),
        TypedMap::new(),
        name.to_owned(),
        PathBuf::from(work_dir),
    )
}

fn select_agent(state: &mut AppState, id: &AgentId) {
    state.selected_agent_index = state.agents.iter().position(|agent| agent.id == *id);
    assert!(
        state.selected_agent_index.is_some(),
        "agent fixture must exist"
    );
}

fn select_repository(state: &mut AppState, id: &RepositoryId) {
    state.selected_repository_index = state
        .repositories
        .iter()
        .position(|repository| repository.id == *id);
    assert!(
        state.selected_repository_index.is_some(),
        "repository fixture must exist"
    );
}

fn agent_name<'a>(state: &'a AppState, id: &AgentId) -> Option<&'a str> {
    state
        .agents
        .iter()
        .find(|agent| agent.id == *id)
        .map(|agent| agent.name.as_str())
}

fn repository_name<'a>(state: &'a AppState, id: &RepositoryId) -> Option<&'a str> {
    state
        .repositories
        .iter()
        .find(|repository| repository.id == *id)
        .map(|repository| repository.name.as_str())
}

fn assert_edit_agent_draft(state: &AppState, id: &AgentId, expected_name: &str) {
    let ModalState::EditAgent {
        id: open_id,
        fields,
        ..
    } = &state.modal
    else {
        panic!("Edit Agent must remain open, got {:?}", state.modal);
    };
    assert_eq!(open_id, id, "the form must retain the stable AgentId");
    assert_eq!(fields.name, expected_name);
}

fn assert_edit_repository_draft(state: &AppState, id: &RepositoryId, expected_name: &str) {
    let ModalState::EditRepository {
        id: open_id,
        fields,
        ..
    } = &state.modal
    else {
        panic!("Edit Repository must remain open, got {:?}", state.modal);
    };
    assert_eq!(open_id, id, "the form must retain the stable RepositoryId");
    assert_eq!(fields.name, expected_name);
}

#[test]
fn edit_agent_raw_key_route_updates_by_id_and_cancel_discards_the_reopened_draft() {
    let mut fixture = AgentEditFixture::new();
    fixture.open_target("target");
    fixture.focus_name_field();
    fixture.type_corrected_suffix();
    fixture.submit_after_reorder();

    fixture.open_target("target-edited");
    fixture.focus_name_field();
    fixture.type_then_cancel("-discard");
    fixture.open_target("target-edited");
}

#[test]
fn edit_repository_raw_key_route_updates_by_id_and_cancel_discards_the_reopened_draft() {
    let mut fixture = RepositoryEditFixture::new();
    fixture.open_target("target");
    fixture.type_corrected_name_and_traverse();
    fixture.submit_after_reorder();

    fixture.open_target("target-edited");
    fixture.type_then_cancel("-discard");
    fixture.open_target("target-edited");
}
