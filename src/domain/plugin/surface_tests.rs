//! Panel, route, screen and configuration declaration table
//! (issue #389 CW-09, acceptance rows D4 and D5).

use super::*;
use crate::domain::Id;
use crate::domain::plugin::limits::{
    CONFIG_FIELD_LIMIT, PANEL_PORT_LIMIT, ROUTE_ACTIVATION_FIELD_LIMIT, SCREEN_ID_LIMIT,
};
use crate::domain::plugin::{Field, FieldDraft, FieldKind, RelativePath, RestartScope};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn field(name: &str) -> Field {
    Field::parse(FieldDraft {
        id: id(name),
        label: name.to_owned(),
        description: None,
        kind: FieldKind::String,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("{name} must parse: {error}"))
}

fn port(name: &str) -> Port {
    Port::new(id(name))
}

fn panel_draft() -> PanelDraft {
    PanelDraft {
        id: id("vendor.pkg.list"),
        model_kinds: vec![ModelKind::List],
        event_schema: Vec::new(),
        handler: id("render"),
        ports: Vec::new(),
    }
}

#[test]
fn model_and_event_kinds_use_lower_kebab_case_wire_names() {
    assert_eq!(ModelKind::List.as_wire(), "list");
    assert_eq!(ModelKind::Error.as_wire(), "error");
    assert_eq!(ModelKind::ALL.len(), 7);
    for kind in ModelKind::ALL {
        assert_eq!(ModelKind::from_wire(kind.as_wire()), Some(kind));
    }

    assert_eq!(EventKind::FieldChanged.as_wire(), "field-changed");
    assert_eq!(EventKind::PageRequested.as_wire(), "page-requested");
    assert_eq!(EventKind::LinkSelected.as_wire(), "link-selected");
    assert_eq!(EventKind::ALL.len(), 9);
    for kind in EventKind::ALL {
        assert_eq!(EventKind::from_wire(kind.as_wire()), Some(kind));
    }
}

#[test]
fn a_panel_must_declare_at_least_one_model_kind() {
    let mut candidate = panel_draft();
    candidate.model_kinds = Vec::new();
    assert_eq!(
        Panel::parse(candidate).err(),
        Some(PanelError::NoModelKinds)
    );
}

#[test]
fn a_panel_may_declare_every_model_and_event_kind_once() {
    let mut candidate = panel_draft();
    candidate.model_kinds = ModelKind::ALL.to_vec();
    candidate.event_schema = EventKind::ALL
        .into_iter()
        .map(|kind| EventSchemaEntry::new(kind, Vec::new()))
        .collect();
    assert!(Panel::parse(candidate).is_ok());
}

#[test]
fn a_panel_rejects_a_repeated_model_or_event_kind() {
    let mut models = panel_draft();
    models.model_kinds = vec![ModelKind::List, ModelKind::List];
    assert_eq!(
        Panel::parse(models).err(),
        Some(PanelError::DuplicateModelKind {
            kind: "list".to_owned()
        })
    );

    let mut events = panel_draft();
    events.event_schema = vec![
        EventSchemaEntry::new(EventKind::Submit, Vec::new()),
        EventSchemaEntry::new(EventKind::Submit, Vec::new()),
    ];
    assert_eq!(
        Panel::parse(events).err(),
        Some(PanelError::DuplicateEventKind {
            kind: "submit".to_owned()
        })
    );
}

#[test]
fn an_event_schema_entry_carries_typed_arguments() {
    let arg = field("action.target");
    let entry = EventSchemaEntry::new(EventKind::Action, vec![arg]);
    let mut candidate = panel_draft();
    candidate.event_schema = vec![entry];
    let panel = Panel::parse(candidate).unwrap_or_else(|error| panic!("must parse: {error}"));
    let schema = panel.event_schema();
    assert_eq!(schema.len(), 1);
    assert_eq!(schema[0].kind(), EventKind::Action);
    assert_eq!(schema[0].arguments().len(), 1);
}

#[test]
fn panel_ports_accept_their_limit_and_reject_one_more() {
    let mut at_limit = panel_draft();
    at_limit.ports = (0..PANEL_PORT_LIMIT)
        .map(|index| port(&format!("p{index}")))
        .collect();
    assert!(Panel::parse(at_limit).is_ok());

    let mut over_limit = panel_draft();
    over_limit.ports = (0..=PANEL_PORT_LIMIT)
        .map(|index| port(&format!("p{index}")))
        .collect();
    assert_eq!(
        Panel::parse(over_limit).err(),
        Some(PanelError::TooManyPorts {
            len: PANEL_PORT_LIMIT + 1
        })
    );
}

#[test]
fn a_duplicate_port_id_is_rejected() {
    let mut candidate = panel_draft();
    candidate.ports = vec![port("same"), port("same")];
    assert_eq!(
        Panel::parse(candidate).err(),
        Some(PanelError::DuplicatePort {
            id: "same".to_owned()
        })
    );
}

#[test]
fn route_activation_fields_accept_their_limit_and_reject_one_more() {
    let at_limit = Route::parse(RouteDraft {
        id: id("vendor.pkg.open"),
        activation_fields: (0..ROUTE_ACTIVATION_FIELD_LIMIT)
            .map(|index| field(&format!("f{index}")))
            .collect(),
        target_screen: id("vendor.pkg.screen"),
    });
    assert!(at_limit.is_ok());

    let over_limit = Route::parse(RouteDraft {
        id: id("vendor.pkg.open"),
        activation_fields: (0..=ROUTE_ACTIVATION_FIELD_LIMIT)
            .map(|index| field(&format!("f{index}")))
            .collect(),
        target_screen: id("vendor.pkg.screen"),
    });
    assert_eq!(
        over_limit.err(),
        Some(RouteError::TooManyActivationFields {
            len: ROUTE_ACTIVATION_FIELD_LIMIT + 1
        })
    );
}

#[test]
fn a_route_rejects_duplicate_activation_field_ids() {
    let route = Route::parse(RouteDraft {
        id: id("vendor.pkg.open"),
        activation_fields: vec![field("same"), field("same")],
        target_screen: id("vendor.pkg.screen"),
    });
    assert_eq!(
        route.err(),
        Some(RouteError::DuplicateActivationField {
            id: "same".to_owned()
        })
    );
}

#[test]
fn a_screen_contribution_binds_between_one_and_its_limit_of_ids() {
    let path = RelativePath::parse("screens/main.json")
        .unwrap_or_else(|error| panic!("must parse: {error}"));

    assert_eq!(
        ScreenContribution::parse(path.clone(), Vec::new()).err(),
        Some(ScreenContributionError::NoScreenIds)
    );

    let at_limit: Vec<Id> = (0..SCREEN_ID_LIMIT)
        .map(|index| id(&format!("vendor.pkg.s{index}")))
        .collect();
    assert!(ScreenContribution::parse(path.clone(), at_limit).is_ok());

    let over_limit: Vec<Id> = (0..=SCREEN_ID_LIMIT)
        .map(|index| id(&format!("vendor.pkg.s{index}")))
        .collect();
    assert_eq!(
        ScreenContribution::parse(path, over_limit).err(),
        Some(ScreenContributionError::TooManyScreenIds {
            len: SCREEN_ID_LIMIT + 1
        })
    );
}

#[test]
fn a_screen_contribution_rejects_a_repeated_screen_id() {
    let path = RelativePath::parse("screens/main.json")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(
        ScreenContribution::parse(path, vec![id("vendor.pkg.a"), id("vendor.pkg.a")]).err(),
        Some(ScreenContributionError::DuplicateScreenId {
            id: "vendor.pkg.a".to_owned()
        })
    );
}

#[test]
fn a_config_schema_version_must_be_at_least_one() {
    assert_eq!(
        ConfigSchema::parse(0, Vec::new()).err(),
        Some(ConfigSchemaError::VersionTooLow { version: 0 })
    );
    assert!(ConfigSchema::parse(1, Vec::new()).is_ok());
}

#[test]
fn a_config_schema_version_is_a_positive_u64() {
    // A value beyond u32::MAX must be accepted end-to-end.
    let large: u64 = u64::from(u32::MAX) + 1;
    let schema = ConfigSchema::parse(large, Vec::new())
        .unwrap_or_else(|error| panic!("u64 schema version must parse: {error}"));
    assert_eq!(schema.schema_version(), large);
}

#[test]
fn config_fields_accept_their_limit_and_reject_one_more() {
    let at_limit: Vec<Field> = (0..CONFIG_FIELD_LIMIT)
        .map(|index| field(&format!("f{index}")))
        .collect();
    assert!(ConfigSchema::parse(1, at_limit).is_ok());

    let over_limit: Vec<Field> = (0..=CONFIG_FIELD_LIMIT)
        .map(|index| field(&format!("f{index}")))
        .collect();
    assert_eq!(
        ConfigSchema::parse(1, over_limit).err(),
        Some(ConfigSchemaError::TooManyFields {
            len: CONFIG_FIELD_LIMIT + 1
        })
    );
}

#[test]
fn a_config_schema_rejects_a_duplicate_field_id() {
    assert_eq!(
        ConfigSchema::parse(1, vec![field("same"), field("same")]).err(),
        Some(ConfigSchemaError::DuplicateField {
            id: "same".to_owned()
        })
    );
}

#[test]
fn a_config_schema_rejects_a_visibility_reference_it_cannot_resolve() {
    let mut gated = FieldDraft {
        id: id("child"),
        label: "Child".to_owned(),
        description: None,
        kind: FieldKind::Boolean,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: Some(id("absent")),
        restart: RestartScope::None,
    };
    let child = Field::parse(gated.clone()).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(
        ConfigSchema::parse(1, vec![child]).err(),
        Some(ConfigSchemaError::UnresolvedVisibility {
            field: "child".to_owned(),
            references: "absent".to_owned()
        })
    );

    gated.visible_when = Some(id("parent"));
    let child = Field::parse(gated).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert!(ConfigSchema::parse(1, vec![field("parent"), child]).is_ok());
}

#[test]
fn a_config_schema_rejects_a_visibility_cycle() {
    let make = |name: &str, gate: &str| {
        Field::parse(FieldDraft {
            id: id(name),
            label: name.to_owned(),
            description: None,
            kind: FieldKind::Boolean,
            required: false,
            default: None,
            min: None,
            max: None,
            choices: Vec::new(),
            unique: false,
            visible_when: Some(id(gate)),
            restart: RestartScope::None,
        })
        .unwrap_or_else(|error| panic!("{name} must parse: {error}"))
    };
    // a -> b -> c -> a
    let error = ConfigSchema::parse(1, vec![make("a", "b"), make("b", "c"), make("c", "a")])
        .err()
        .unwrap_or_else(|| panic!("a cycle must be rejected"));
    assert!(
        matches!(error, ConfigSchemaError::VisibilityCycle { .. }),
        "expected a cycle diagnostic, got {error}"
    );
}
