use std::sync::Arc;

use super::provider_requests::ActionPolicy;
use super::relationship_runtime_tests::apply_provider;
use super::{AppState, RelationshipCommand, RelationshipCommandError};
use crate::domain::plugin::HostTriple;
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::{Id, TypedMap, TypedPortValue, TypedValue};
use crate::messages::ProviderMessage;
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::scan;
use crate::persistence::settings_document::PublishedSettings;
use crate::runtime::provider::Containment;
use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};
use crate::workbench::{
    ActivationValues, ISSUES_LIST_PANEL, PanelId, PortId, PortRef, PortValue, RouteId, ScreenId,
    SourceIntent,
};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("fixture id {value}: {error}"))
}

fn selection_port() -> PortRef {
    PortRef {
        panel: PanelId::parse(ISSUES_LIST_PANEL)
            .unwrap_or_else(|error| panic!("selection panel: {error}")),
        port: PortId::parse("selection").unwrap_or_else(|error| panic!("selection port: {error}")),
    }
}

fn issue_value(
    type_id: &str,
    schema_version: u64,
    semantic_key: &str,
    fields: impl IntoIterator<Item = (Id, TypedValue)>,
) -> PortValue {
    PortValue::Typed(TypedPortValue {
        type_id: id(type_id),
        schema_version,
        semantic_key: semantic_key.to_owned(),
        value: fields.into_iter().collect(),
    })
}

fn issues_state() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    state
}

fn publish(
    state: &mut AppState,
    owner: Id,
    value: PortValue,
) -> Result<(), RelationshipCommandError> {
    let current = state.nav.current();
    let port = selection_port();
    let panel_instance_id = current
        .relationships()
        .and_then(|relationships| relationships.panel_instance_id(&port.panel))
        .unwrap_or_else(|| panic!("Issues selection panel must have a runtime identity"));
    state
        .apply_relationship_command(RelationshipCommand {
            open_screen_id: current.id,
            panel_instance_id,
            generation: current.generation,
            owner_id: owner,
            intent: SourceIntent::Publish { port, value },
        })
        .map(|_| ())
}

const REQUIRED_RESOURCE_SCREEN: &str = r#"screen_schema = 2
id = "local.required"
title = "Required resource"
route = "local.required"
initial_focus = "main"
focus_order = ["main"]

[[resources]]
type_id = "local.required.item"
schema_version = 1
semantic_key = "semantic-key"

[[resources.fields]]
id = "semantic-key"
label = "Semantic key"
type = "string"
required = true

[[panels]]
id = "main"
type = "issue-list"
focusable = true
required = true

[[panels.ports]]
id = "selection"
direction = "output"
owner = "local.required"
type_id = "local.required.item@1"
required = true
retained = false

[layout]
type = "leaf"
panel = "main"
"#;

fn state_with_missing_required_resource() -> AppState {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let definitions = root.path().join("definitions");
    std::fs::create_dir_all(&definitions)
        .unwrap_or_else(|error| panic!("definitions directory: {error}"));
    std::fs::write(
        definitions.join("required.screen.toml"),
        REQUIRED_RESOURCE_SCREEN,
    )
    .unwrap_or_else(|error| panic!("required screen definition: {error}"));
    let file = |name: &str| ResolvedFile {
        path: root.path().join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    let paths = ResolvedPaths {
        settings: file("settings.toml"),
        state: file("state.json"),
        definitions,
        plugins: root.path().join("plugins"),
        themes: root.path().join("themes"),
    };
    let mut settings = PublishedSettings::default();
    settings.workbench.enabled_screens = vec![id("local.required")];
    let inventory = scan(&[]);
    let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: &settings,
        host: HostTriple::current(),
        containment: Containment {
            home: root.path().join("home"),
            tmpdir: root.path().join("tmp"),
            working_dir: root.path().join("work"),
            locale: "C".to_owned(),
            host_api: crate::VERSION.to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("required-resource workbench: {error}"));
    let mut state = AppState::new(Arc::new(candidate));
    state.enter_provider_route(
        RouteId::from_static("local.required"),
        ActivationValues::empty(),
    );
    state
}

#[test]
fn missing_required_resource_refuses_invocation_without_request_mutation() {
    let state = state_with_missing_required_resource();
    let requests_before = format!("{:?}", state.provider_requests);
    let transition = apply_provider(
        state,
        ProviderMessage::Invoke {
            owner: id("host"),
            action_id: id("provider.run"),
            arguments: TypedMap::new(),
            policy: ActionPolicy::new(ActionConfirmation::None, vec![ActionOutcome::Notice], false),
        },
    );

    assert!(transition.effects.is_empty());
    assert_eq!(
        format!("{:?}", transition.next_state.provider_requests),
        requests_before
    );
    assert!(
        transition.next_state.error_message.as_deref().is_some_and(
            |message| message.contains("required resource port main.selection is absent")
        )
    );
}

#[test]
fn invalid_resource_context_matrix_refuses_before_provider_invocation() {
    let semantic_key = "vybestack/llxprt-jefe#42";
    let publication_cases = [
        issue_value("github.issue", 1, semantic_key, []),
        issue_value(
            "github.issue",
            1,
            semantic_key,
            [
                (
                    id("semantic-key"),
                    TypedValue::String(semantic_key.to_owned()),
                ),
                (id("extra"), TypedValue::String("not declared".to_owned())),
            ],
        ),
        issue_value(
            "github.pull-request",
            1,
            semantic_key,
            [(
                id("semantic-key"),
                TypedValue::String(semantic_key.to_owned()),
            )],
        ),
        issue_value(
            "github.issue",
            2,
            semantic_key,
            [(
                id("semantic-key"),
                TypedValue::String(semantic_key.to_owned()),
            )],
        ),
        issue_value(
            "github.issue",
            1,
            semantic_key,
            [(
                id("semantic-key"),
                TypedValue::String("different#42".to_owned()),
            )],
        ),
    ];

    for (index, value) in publication_cases.into_iter().enumerate() {
        let mut state = issues_state();
        let requests_before = format!("{:?}", state.provider_requests);
        assert!(
            publish(&mut state, id("github.issues"), value).is_err(),
            "invalid publication case {index}"
        );
        assert_eq!(
            format!("{:?}", state.provider_requests),
            requests_before,
            "invalid publication case {index}"
        );
    }
}

#[test]
fn wrong_resource_owner_is_rejected_before_it_can_enter_invocation_context() {
    let semantic_key = "vybestack/llxprt-jefe#42";
    let mut state = issues_state();
    let requests_before = format!("{:?}", state.provider_requests);
    let value = issue_value(
        "github.issue",
        1,
        semantic_key,
        [(
            id("semantic-key"),
            TypedValue::String(semantic_key.to_owned()),
        )],
    );

    assert_eq!(
        publish(&mut state, id("github.pull-requests"), value),
        Err(RelationshipCommandError::WrongOwner)
    );
    assert_eq!(format!("{:?}", state.provider_requests), requests_before);
}
