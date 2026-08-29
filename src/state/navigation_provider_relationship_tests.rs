use std::sync::Arc;

use super::AppState;
use super::provider_panels::{AcceptSnapshot, DeclareInput, EventDeclaration, EventKind};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{
    BodyKind, ListBody, ListItem, PanelBody, PanelEvent, PanelSnapshot, StructuredDiffBody,
    StructuredDiffFile, StructuredDiffPath, TreeBody, TreeNode,
};
use crate::workbench::compose::ScreenComposition;
use crate::workbench::relationship_fixtures as fixtures;
use crate::workbench::{
    PackagePanelBinding, PanelId, PortRef, ResourceSchemaRegistry, ScreenRegistry,
};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid fixture id: {error}"))
}

const PACKAGE_SCREEN: &str = r#"screen_schema = 2
id = "github.core.review"
title = "Provider review"
route = "provider-review"
initial_focus = "list"
focus_order = ["list"]

[[resources]]
type_id = "github.pull-request"
schema_version = 1
semantic_key = "semantic-key"

[[resources.fields]]
id = "semantic-key"
label = "Semantic key"
type = "string"
required = true
__REQUIRED_PAYLOAD__
[[panels]]
id = "list"
type = "github.core.list"
focusable = true
required = true

[[panels.ports]]
id = "selection"
direction = "output"
owner = "github.core.review"
type_id = "github.pull-request@1"
required = false
retained = false

[[panels]]
id = "detail-a"
type = "github.core.detail"
focusable = false
required = false

[[panels.ports]]
id = "subject"
direction = "input"
owner = "github.core.review"
type_id = "github.pull-request@1"
required = false
retained = false
__DETAIL_B__
[layout]
type = "split"
axis = "horizontal"

[[layout.children]]
min = 10
collapsible = false
size = { weight = 1 }
node = { type = "leaf", panel = "list" }

[[layout.children]]
min = 10
collapsible = true
collapse_priority = 0
size = { weight = 1 }
node = { type = "leaf", panel = "detail-a" }
__DETAIL_B_LAYOUT__
[[relationships]]
kind = "scope"
source = "list.selection"
target = "detail-a.subject"
__DETAIL_B_RELATIONSHIP__
"#;

const REQUIRED_PAYLOAD: &str = r#"
[[resources.fields]]
id = "required-payload"
label = "Required payload"
type = "string"
required = true
"#;
const DETAIL_B: &str = r#"
[[panels]]
id = "detail-b"
type = "github.core.detail"
focusable = false
required = false

[[panels.ports]]
id = "subject"
direction = "input"
owner = "github.core.review"
type_id = "github.pull-request@1"
required = false
retained = false
"#;
const DETAIL_B_LAYOUT: &str = r#"
[[layout.children]]
min = 10
collapsible = true
collapse_priority = 1
size = { weight = 1 }
node = { type = "leaf", panel = "detail-b" }
"#;
const DETAIL_B_RELATIONSHIP: &str = r#"
[[relationships]]
kind = "scope"
source = "list.selection"
target = "detail-b.subject"
"#;

fn lowered_package_screen(
    fan_out: bool,
    extra_required_field: bool,
) -> crate::workbench::LoweredScreen {
    let source = PACKAGE_SCREEN
        .replace(
            "__REQUIRED_PAYLOAD__",
            if extra_required_field {
                REQUIRED_PAYLOAD
            } else {
                ""
            },
        )
        .replace("__DETAIL_B__", if fan_out { DETAIL_B } else { "" })
        .replace(
            "__DETAIL_B_LAYOUT__",
            if fan_out { DETAIL_B_LAYOUT } else { "" },
        )
        .replace(
            "__DETAIL_B_RELATIONSHIP__",
            if fan_out { DETAIL_B_RELATIONSHIP } else { "" },
        );
    let file = crate::workbench::parse_screen_file(&source)
        .unwrap_or_else(|error| unreachable!("valid package screen syntax: {error}"));
    crate::workbench::lower_package_screen(
        &file,
        "github.core.review",
        &["github.core.list", "github.core.detail"],
        std::path::Path::new("github-core-review.screen.toml"),
    )
    .unwrap_or_else(|error| unreachable!("valid package screen lowering: {error}"))
}

fn provider_workbench(
    fan_out: bool,
    extra_required_field: bool,
) -> Arc<crate::published_workbench::PublishedWorkbench> {
    let lowered = lowered_package_screen(fan_out, extra_required_field);
    let screen = lowered.descriptor;
    let resources = ResourceSchemaRegistry::publish(lowered.resources)
        .unwrap_or_else(|error| unreachable!("valid package resource registry: {error}"));
    let registry = ScreenRegistry::with_panel_bindings(
        vec![screen.clone()],
        vec![PackagePanelBinding {
            screen: screen.id,
            panel: fixtures::panel_id("list"),
            owner: id("github.core"),
            panel_type: id("github.core.list"),
            model_kinds: Vec::new(),
            event_schema: Vec::new(),
            action_authority: Vec::new(),
        }],
    )
    .unwrap_or_else(|error| unreachable!("valid fixture screen registry: {error}"));
    let workbench = crate::test_support::published_workbench();
    let mut workbench = Arc::try_unwrap(workbench)
        .unwrap_or_else(|_| unreachable!("fixture workbench has one owner"));
    workbench.replace_test_declarations(
        ScreenComposition {
            registry,
            resource_schemas: Vec::new(),
            warnings: Vec::new(),
        },
        resources,
    );
    Arc::new(workbench)
}

fn declare_provider_panel(
    state: &mut AppState,
    body: PanelBody,
    kind: BodyKind,
) -> crate::workbench::PanelInstanceId {
    let owner = id("github.core");
    let panel_id = PanelId::parse("list")
        .unwrap_or_else(|error| unreachable!("valid fixture panel id: {error}"));
    let panel_type = id("github.core.list");
    let allowed_events = [EventDeclaration {
        kind: EventKind::Selected,
        arguments: Vec::new(),
    }];
    let screen_instance_id = state.nav.current().id.get();
    let declared = state
        .provider_panels_mut()
        .declare(DeclareInput {
            owner: &owner,
            panel_id: &panel_id,
            screen_instance_id,
            panel_type: &panel_type,
            activation: &TypedMap::new(),
            allowed_model_kinds: &[kind],
            allowed_events: &allowed_events,
            action_authority: &[],
            process_generation: 1,
        })
        .unwrap_or_else(|error| unreachable!("declare fixture panel: {error}"));
    state
        .provider_panels_mut()
        .activate(declared.instance)
        .unwrap_or_else(|error| unreachable!("activate fixture panel: {error}"));
    let snapshot = PanelSnapshot {
        model_schema: 1,
        panel_instance_id: declared.instance.as_u64(),
        generation: 1,
        revision: 1,
        kind,
        title: "Selectable".to_owned(),
        description: None,
        loading: false,
        action_affordances: Vec::new(),
        body,
    };
    state
        .provider_panels_mut()
        .accept_snapshot(AcceptSnapshot {
            owner: &owner,
            received_process_generation: 1,
            payload_byte_count: 256,
            elapsed_ms: 0,
            snapshot: &snapshot,
        })
        .unwrap_or_else(|error| unreachable!("accept fixture snapshot: {error}"));
    declared.instance
}

fn state_with_provider_screen(
    body: PanelBody,
    kind: BodyKind,
    fan_out: bool,
    extra_required_field: bool,
) -> (AppState, crate::workbench::PanelInstanceId) {
    let mut state = AppState::new(provider_workbench(fan_out, extra_required_field));
    let instance = declare_provider_panel(&mut state, body, kind);
    (state, instance)
}

fn selection_bodies() -> Vec<(PanelBody, BodyKind, Id, &'static str)> {
    let selected = id("selected");
    vec![
        (
            PanelBody::List(ListBody {
                items: vec![ListItem {
                    id: selected.clone(),
                    label: "Selected".to_owned(),
                    description: None,
                    status: None,
                    actions: Vec::new(),
                }],
                selected_id: None,
                next_page_token: None,
            }),
            BodyKind::List,
            selected.clone(),
            "selected",
        ),
        (
            PanelBody::Tree(TreeBody {
                schema_version: 1,
                nodes: vec![TreeNode {
                    id: selected.clone(),
                    parent_id: None,
                    label: "Selected".to_owned(),
                    semantic_key: id("tree-key"),
                    depth: 0,
                    expandable: false,
                    expanded: false,
                }],
                selected_id: None,
            }),
            BodyKind::Tree,
            selected.clone(),
            "tree-key",
        ),
        (
            PanelBody::StructuredDiff(StructuredDiffBody {
                schema_version: 1,
                files: vec![StructuredDiffFile {
                    id: selected.clone(),
                    path: StructuredDiffPath::Added("selected.rs".to_owned()),
                    old_mode: None,
                    new_mode: None,
                    binary: true,
                    hunks: Vec::new(),
                }],
                selected_file_id: None,
            }),
            BodyKind::StructuredDiff,
            selected,
            "selected",
        ),
    ]
}

fn target_value(state: &AppState, panel: &'static str) -> crate::workbench::PortValue {
    let target = PortRef {
        panel: PanelId::parse(panel)
            .unwrap_or_else(|error| unreachable!("valid target panel: {error}")),
        port: fixtures::port_id("subject"),
    };
    let runtime = state
        .nav
        .current()
        .relationships()
        .unwrap_or_else(|| unreachable!("fixture has relationship runtime"));
    let key = runtime
        .port_key(&target)
        .unwrap_or_else(|| unreachable!("fixture target has runtime key"));
    state.nav.current().relationship_state().value(&key)
}

#[test]
fn generic_list_tree_and_structured_diff_selection_publish_declared_typed_values() {
    for (body, kind, selected, expected_key) in selection_bodies() {
        let (mut state, instance) = state_with_provider_screen(body, kind, false, false);
        let committed = state.submit_provider_panel_semantic_event(
            instance,
            &fixtures::panel_id("list"),
            PanelEvent::Selected { id: selected },
        );
        assert!(committed, "{:?}", state.error_message);
        let crate::workbench::PortValue::Typed(value) = target_value(&state, "detail-a") else {
            panic!("selection must publish a typed target value");
        };
        assert_eq!(value.semantic_key, expected_key);
        assert_eq!(
            value.value.get(&id("semantic-key")),
            Some(&TypedValue::String(expected_key.to_owned()))
        );
    }
}

#[test]
fn generic_selection_fans_out_in_declared_order_in_one_commit() {
    let (body, kind, selected, expected_key) = selection_bodies().remove(0);
    let (mut state, instance) = state_with_provider_screen(body, kind, true, false);
    let source_panel = fixtures::panel_id("list");
    let event = PanelEvent::Selected {
        id: selected.clone(),
    };

    let mut ordering_probe = state.clone();
    let mut commands = ordering_probe
        .selection_relationship_commands(instance, &source_panel, &event)
        .unwrap_or_else(|error| panic!("project package selection: {error}"));
    assert_eq!(commands.len(), 1);
    let updates = ordering_probe
        .apply_relationship_command(commands.remove(0))
        .unwrap_or_else(|error| panic!("apply projected relationship command: {error}"));
    assert_eq!(
        updates.iter().map(|update| update.port).collect::<Vec<_>>(),
        vec![
            PortRef {
                panel: source_panel,
                port: fixtures::port_id("selection"),
            },
            PortRef {
                panel: fixtures::panel_id("detail-a"),
                port: fixtures::port_id("subject"),
            },
            PortRef {
                panel: fixtures::panel_id("detail-b"),
                port: fixtures::port_id("subject"),
            },
        ]
    );

    assert!(state.submit_provider_panel_semantic_event(instance, &source_panel, event));

    for panel in ["detail-a", "detail-b"] {
        let crate::workbench::PortValue::Typed(value) = target_value(&state, panel) else {
            panic!("fan-out target must receive a typed value");
        };
        assert_eq!(value.semantic_key, expected_key);
    }
}

#[test]
fn invalid_projection_refuses_provider_selection_and_relationships_atomically() {
    let (body, kind, selected, _) = selection_bodies().remove(0);
    let (mut state, instance) = state_with_provider_screen(body, kind, true, true);

    assert!(!state.submit_provider_panel_semantic_event(
        instance,
        &fixtures::panel_id("list"),
        PanelEvent::Selected { id: selected },
    ));

    assert_eq!(
        target_value(&state, "detail-a"),
        crate::workbench::PortValue::Absent
    );
    assert_eq!(
        target_value(&state, "detail-b"),
        crate::workbench::PortValue::Absent
    );
    assert_eq!(
        state
            .provider_panels()
            .host_local(instance)
            .and_then(|local| local.selected_id.as_ref()),
        None
    );
}

#[test]
fn suspended_provider_selection_cannot_publish_and_resume_restores_exact_instance_authority() {
    let (body, kind, selected, expected_key) = selection_bodies().remove(1);
    let restored_body = body.clone();
    let (mut state, instance) = state_with_provider_screen(body, kind, false, false);
    state
        .provider_panels_mut()
        .suspend(instance)
        .unwrap_or_else(|error| unreachable!("suspend fixture panel: {error}"));

    assert!(!state.submit_provider_panel_semantic_event(
        instance,
        &fixtures::panel_id("list"),
        PanelEvent::Selected {
            id: selected.clone(),
        },
    ));
    assert_eq!(
        target_value(&state, "detail-a"),
        crate::workbench::PortValue::Absent
    );

    state
        .provider_panels_mut()
        .resume(instance)
        .unwrap_or_else(|error| unreachable!("resume fixture panel: {error}"));
    let restored = PanelSnapshot {
        model_schema: 1,
        panel_instance_id: instance.as_u64(),
        generation: 2,
        revision: 1,
        kind,
        title: "Restored".to_owned(),
        description: None,
        loading: false,
        action_affordances: Vec::new(),
        body: restored_body,
    };
    state
        .provider_panels_mut()
        .accept_snapshot(AcceptSnapshot {
            owner: &id("github.core"),
            received_process_generation: 1,
            payload_byte_count: 256,
            elapsed_ms: 0,
            snapshot: &restored,
        })
        .unwrap_or_else(|error| unreachable!("accept restored snapshot: {error}"));

    assert!(state.submit_provider_panel_semantic_event(
        instance,
        &fixtures::panel_id("list"),
        PanelEvent::Selected { id: selected },
    ));
    let crate::workbench::PortValue::Typed(value) = target_value(&state, "detail-a") else {
        panic!("restored selection must publish a typed target value");
    };
    assert_eq!(value.semantic_key, expected_key);
}

#[test]
fn stale_provider_generation_cannot_mutate_selection_or_relationship_targets() {
    let (body, kind, selected, _) = selection_bodies().remove(2);
    let (mut state, instance) = state_with_provider_screen(body, kind, true, false);
    state
        .provider_panels_mut()
        .retry(instance)
        .unwrap_or_else(|error| unreachable!("retry fixture panel: {error}"));

    assert!(!state.submit_provider_panel_semantic_event(
        instance,
        &fixtures::panel_id("list"),
        PanelEvent::Selected { id: selected },
    ));
    assert_eq!(
        target_value(&state, "detail-a"),
        crate::workbench::PortValue::Absent
    );
    assert_eq!(
        target_value(&state, "detail-b"),
        crate::workbench::PortValue::Absent
    );
    assert_eq!(
        state
            .provider_panels()
            .host_local(instance)
            .and_then(|local| local.selected_id.as_ref()),
        None
    );
}
