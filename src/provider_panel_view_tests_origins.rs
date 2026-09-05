use crate::domain::action_registry::ActionId;
use crate::domain::{Id, TypedMap};
use crate::host_controls::{ControlAction, ControlIntent, ControlKind, control_intent_body};
use crate::provider_panel_view::{
    PanelHitTarget, PanelRender, PanelStatus, project_provider_screen,
};
use crate::runtime::provider::protocol::{
    Affordance, BodyKind, DetailBody, EmptyBody, ErrorBody, FormBody, ListBody, ListItem,
    PanelBody, PanelSnapshot, ProgressBody, StatusBody, StatusRow, StatusRowState,
    StructuredDiffBody, StructuredDiffFile, StructuredDiffPath, TreeBody, TreeNode,
};
use crate::state::provider_panels::{
    AcceptSnapshot, DeclareInput, MODEL_SCHEMA, ProviderPanelState,
};
use crate::workbench::{
    CustomScreenId, DASHBOARD_IDENTITY, LayoutNode, PackagePanelBinding, PanelDescriptor, PanelId,
    PanelState, PanelTypeId, PluginScreenId, Rect, RouteId, ScreenDescriptor, ScreenIdentity,
    ScreenInstanceId, ScreenRegistry, control_origin_composition, control_origin_definition,
    resolve_layout, try_control_origin_composition,
    try_control_origin_composition_with_definitions,
};

const PANEL_ID: PanelId = PanelId::from_static("control");
const SCREEN_INSTANCE: u64 = 1;

fn parsed_id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("id {value}: {error}"))
}

fn action_id() -> ActionId {
    ActionId::parse("vendor.fixture.act").unwrap_or_else(|error| panic!("action fixture: {error}"))
}

fn action_affordance() -> Affordance {
    Affordance {
        id: parsed_id("act"),
        label: "Act".to_owned(),
        action_id: action_id(),
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    }
}

fn selectable_bodies(first: &Id, second: &Id) -> Vec<PanelBody> {
    let ids = || [first.clone(), second.clone()];
    vec![
        PanelBody::List(ListBody {
            items: ids()
                .into_iter()
                .map(|id| ListItem {
                    label: id.as_str().to_owned(),
                    id,
                    description: None,
                    status: None,
                    count: None,
                    actions: Vec::new(),
                })
                .collect(),
            selected_id: Some(first.clone()),
            next_page_token: None,
        }),
        PanelBody::Tree(TreeBody {
            schema_version: 1,
            nodes: ids()
                .into_iter()
                .map(|id| TreeNode {
                    label: id.as_str().to_owned(),
                    semantic_key: id.clone(),
                    id,
                    parent_id: None,
                    depth: 0,
                    expandable: false,
                    expanded: false,
                })
                .collect(),
            selected_id: Some(first.clone()),
        }),
        PanelBody::StructuredDiff(StructuredDiffBody {
            schema_version: 1,
            files: ids()
                .into_iter()
                .map(|id| StructuredDiffFile {
                    path: StructuredDiffPath::Modified(format!("{}.rs", id.as_str())),
                    id,
                    old_mode: None,
                    new_mode: None,
                    binary: true,
                    hunks: Vec::new(),
                })
                .collect(),
            selected_file_id: Some(first.clone()),
        }),
    ]
}

fn bodies() -> Vec<PanelBody> {
    let selectable = selectable_bodies(&parsed_id("first"), &parsed_id("second"));
    vec![
        selectable[0].clone(),
        selectable[1].clone(),
        PanelBody::Detail(DetailBody {
            document: "detail".to_owned(),
            metadata: Vec::new(),
            actions: vec![parsed_id("act")],
        }),
        selectable[2].clone(),
        PanelBody::Form(FormBody {
            fields: Vec::new(),
            values: TypedMap::new(),
            field_errors: Vec::new(),
            submit_action: action_id(),
        }),
        PanelBody::Status(StatusBody {
            rows: vec![StatusRow {
                label: "State".to_owned(),
                value: "Ready".to_owned(),
                state: StatusRowState::Normal,
            }],
        }),
        PanelBody::Progress(ProgressBody {
            message: "Working".to_owned(),
            completed: Some(1),
            total: Some(2),
            cancellable: true,
        }),
        PanelBody::Empty(EmptyBody {
            message: "Empty".to_owned(),
            action: Some(parsed_id("act")),
        }),
        PanelBody::Error(ErrorBody {
            code: "fixture".to_owned(),
            message: "Failed".to_owned(),
            retryable: true,
            retry_action: Some(parsed_id("act")),
        }),
    ]
}

fn action_for(kind: BodyKind) -> ControlAction {
    match kind {
        BodyKind::List | BodyKind::Tree | BodyKind::StructuredDiff => ControlAction::Next,
        BodyKind::Form => ControlAction::Submit,
        BodyKind::Progress => ControlAction::Cancel,
        BodyKind::Error => ControlAction::Retry,
        BodyKind::Detail | BodyKind::Status | BodyKind::Empty => {
            ControlAction::Action(parsed_id("act"))
        }
    }
}

fn selected_id(body: &PanelBody) -> Option<&Id> {
    match body {
        PanelBody::List(body) => body.selected_id.as_ref(),
        PanelBody::Tree(body) => body.selected_id.as_ref(),
        PanelBody::StructuredDiff(body) => body.selected_file_id.as_ref(),
        PanelBody::Detail(_)
        | PanelBody::Form(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_)
        | PanelBody::Empty(_)
        | PanelBody::Error(_) => None,
    }
}

fn fixture_descriptor() -> ScreenDescriptor {
    ScreenDescriptor {
        id: DASHBOARD_IDENTITY,
        title: "Fixture".to_owned(),
        route: RouteId::from_static("fixture"),
        panels: vec![PanelDescriptor {
            id: PANEL_ID,
            panel_type: PanelTypeId::from_static("vendor.fixture.control"),
            host_capability: None,
            config: TypedMap::new(),
            focusable: true,
            required: true,
            ports: Vec::new(),
        }],
        initial_focus: PANEL_ID,
        focus_order: vec![PANEL_ID],
        relationships: Vec::new(),
        activation: Vec::new(),
        overlays: Vec::new(),
        host_capabilities: Vec::new(),
        bindings: Vec::new(),
        layout: LayoutNode::Leaf { panel: PANEL_ID },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderContract {
    owner: Id,
    panel_type: Id,
    model_kinds: Vec<BodyKind>,
    action_authority: Vec<ActionId>,
}

fn body_kind(kind: ControlKind) -> BodyKind {
    match kind {
        ControlKind::List => BodyKind::List,
        ControlKind::Tree => BodyKind::Tree,
        ControlKind::Detail => BodyKind::Detail,
        ControlKind::StructuredDiff => BodyKind::StructuredDiff,
        ControlKind::Form => BodyKind::Form,
        ControlKind::Status => BodyKind::Status,
        ControlKind::Progress => BodyKind::Progress,
        ControlKind::Empty => BodyKind::Empty,
        ControlKind::Error => BodyKind::Error,
    }
}

fn fixture_contract() -> ProviderContract {
    ProviderContract {
        owner: parsed_id("vendor.fixture"),
        panel_type: parsed_id("vendor.fixture.control"),
        model_kinds: ControlKind::ALL.into_iter().map(body_kind).collect(),
        action_authority: vec![action_id()],
    }
}

fn binding_contract(binding: &PackagePanelBinding) -> ProviderContract {
    ProviderContract {
        owner: binding.owner.clone(),
        panel_type: binding.panel_type.clone(),
        model_kinds: binding
            .model_kinds
            .iter()
            .copied()
            .map(ControlKind::from)
            .map(body_kind)
            .collect(),
        action_authority: binding.action_authority.clone(),
    }
}

fn exact_binding(registry: &ScreenRegistry, screen: ScreenIdentity) -> ProviderContract {
    let binding = registry
        .panel_binding(screen, &PANEL_ID)
        .unwrap_or_else(|| panic!("selected-provider binding for {screen}"));
    let contract = binding_contract(binding);
    assert_eq!(contract, fixture_contract(), "binding for {screen}");
    contract
}

fn fixture_panels(
    origin: ScreenIdentity,
    contract: &ProviderContract,
    body: &PanelBody,
    affordance: &Affordance,
) -> ProviderPanelState {
    let activation = TypedMap::new();
    let mut panels = ProviderPanelState::default();
    let declaration = panels
        .declare(DeclareInput {
            owner: &contract.owner,
            panel_id: &PANEL_ID,
            screen_instance_id: SCREEN_INSTANCE,
            panel_type: &contract.panel_type,
            activation: &activation,
            allowed_model_kinds: &contract.model_kinds,
            allowed_events: &[],
            action_authority: &contract.action_authority,
            process_generation: 1,
        })
        .unwrap_or_else(|error| panic!("declare {origin}: {error:?}"));
    let activated = panels
        .activate(declaration.instance)
        .unwrap_or_else(|error| panic!("activate {origin}: {error:?}"));
    assert_eq!(activated.effect.owner, contract.owner);
    assert_eq!(activated.effect.panel_type, contract.panel_type);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: declaration.instance.as_u64(),
        generation: 1,
        revision: 1,
        kind: body.kind(),
        title: "Fixture".to_owned(),
        description: None,
        loading: false,
        action_affordances: vec![affordance.clone()],
        body: body.clone(),
    };
    panels
        .accept_snapshot(AcceptSnapshot {
            owner: &contract.owner,
            received_process_generation: 1,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot: &snapshot,
        })
        .unwrap_or_else(|error| panic!("snapshot {origin}: {error:?}"));
    panels
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeSemantics {
    title: String,
    status: PanelStatus,
    lines: Vec<String>,
    max_scroll_offset: u32,
    render: PanelRender,
    hit_targets: Vec<Option<PanelHitTarget>>,
    intent: ControlIntent,
}

fn runtime_semantics(
    descriptor: &ScreenDescriptor,
    contract: &ProviderContract,
    body: &PanelBody,
) -> RuntimeSemantics {
    let origin = descriptor.id;
    let affordance = action_affordance();
    let panels = fixture_panels(origin, contract, body, &affordance);
    let layout = resolve_layout(
        descriptor,
        ScreenInstanceId::preview(),
        Rect::new(0, 0, 48, 16),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| panic!("layout {origin}: {error:?}"));
    let projection =
        project_provider_screen(descriptor, SCREEN_INSTANCE, &panels, &layout, &PANEL_ID)
            .unwrap_or_else(|error| panic!("project {origin}: {error}"));
    let panel = projection
        .panels
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("panel projection for {origin}"));
    let intent = control_intent_body(
        body,
        &[affordance],
        selected_id(body),
        None,
        None,
        action_for(body.kind()),
    );
    assert!(panel.visible, "{origin} must be visible");
    assert!(panel.focused, "{origin} must be focused");
    assert_eq!(panel.status, PanelStatus::Active, "{origin}");
    assert_ne!(panel.lines, vec!["provider unavailable".to_owned()]);
    assert!(!matches!(intent, ControlIntent::None), "{origin}");
    RuntimeSemantics {
        title: panel.title,
        status: panel.status,
        lines: panel.lines,
        max_scroll_offset: panel.max_scroll_offset,
        render: panel.render,
        hit_targets: panel.hit_targets,
        intent,
    }
}

fn descriptor(registry: &ScreenRegistry, identity: ScreenIdentity) -> &ScreenDescriptor {
    registry
        .get_identity(identity)
        .unwrap_or_else(|| panic!("composed descriptor {identity}"))
}

fn package_control_origin_definition() -> String {
    control_origin_definition()
        .replace("local.control-fixture", "vendor.fixture.screen")
        .replace(
            "route = \"control-fixture\"",
            "route = \"vendor.fixture.screen\"",
        )
}

#[test]
fn local_provider_panel_without_selected_owner_refuses_composition() {
    let compiled = ScreenRegistry::new(vec![fixture_descriptor()])
        .unwrap_or_else(|error| panic!("compiled control fixture: {error}"));
    let local =
        control_origin_definition().replace("vendor.fixture.control", "vendor.missing.control");

    assert!(
        try_control_origin_composition_with_definitions(
            &compiled,
            &local,
            &package_control_origin_definition(),
        )
        .is_err()
    );
}

#[test]
fn package_provider_panel_without_selected_owner_refuses_composition() {
    let compiled = ScreenRegistry::new(vec![fixture_descriptor()])
        .unwrap_or_else(|error| panic!("compiled control fixture: {error}"));
    let package = package_control_origin_definition()
        .replace("vendor.fixture.control", "vendor.missing.control");

    assert!(
        try_control_origin_composition_with_definitions(
            &compiled,
            &control_origin_definition(),
            &package,
        )
        .is_err()
    );
}

#[test]
fn compiled_provider_panel_without_selected_owner_refuses_composition() {
    let mut descriptor = fixture_descriptor();
    descriptor.panels[0].panel_type = PanelTypeId::from_static("vendor.missing.control");
    let compiled = ScreenRegistry::new(vec![descriptor])
        .unwrap_or_else(|error| panic!("compiled missing-owner fixture: {error}"));

    assert!(try_control_origin_composition(&compiled).is_err());
}

#[test]
fn all_nine_controls_share_composed_builtin_local_and_package_runtime_semantics() {
    let compiled = ScreenRegistry::new(vec![fixture_descriptor()])
        .unwrap_or_else(|error| panic!("compiled control fixture: {error}"));
    let composition = control_origin_composition(&compiled);
    let local = ScreenIdentity::Custom(
        CustomScreenId::parse("local.control-fixture")
            .unwrap_or_else(|error| panic!("local origin: {error}")),
    );
    let package = ScreenIdentity::Package(PluginScreenId::from_static("vendor.fixture.screen"));
    let origins = [DASHBOARD_IDENTITY, local, package];
    let contracts = origins.map(|origin| exact_binding(&composition.registry, origin));
    let bodies = bodies();
    assert_eq!(
        bodies
            .iter()
            .map(|body| ControlKind::from(body.kind()))
            .collect::<Vec<_>>(),
        ControlKind::ALL
    );

    for body in bodies {
        let expected = runtime_semantics(
            descriptor(&composition.registry, origins[0]),
            &contracts[0],
            &body,
        );
        for (origin, contract) in origins.into_iter().zip(&contracts).skip(1) {
            assert_eq!(
                runtime_semantics(descriptor(&composition.registry, origin), contract, &body),
                expected,
                "{:?} diverged for {origin}",
                body.kind()
            );
        }
    }
}
