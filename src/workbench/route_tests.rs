//! Route declaration and activation validation contracts (issue #386, CW-06).

use std::path::PathBuf;

use crate::domain::Id;

use super::activation::{ActivationField, ActivationKind};
use super::ids::{MAX_ACTIVATION_FIELDS, RouteId, ScreenId, ScreenIdentity};
use super::route::{
    ActivationError, ActivationValue, ActivationValues, MAX_ACTIVATION_BYTES, NavCode,
    RouteDeclaration, route_declaration,
};
use super::{ScreenRegistry, screen_registry};

fn id(name: &str) -> Id {
    Id::parse(name).unwrap_or_else(|_| unreachable!("test field name is a valid identifier"))
}

fn field(name: &str, kind: ActivationKind) -> ActivationField {
    ActivationField {
        name: id(name),
        kind,
    }
}

fn review_route() -> RouteId {
    RouteId::parse("review").unwrap_or_else(|_| unreachable!("test route id is valid"))
}

fn unknown_route() -> RouteId {
    RouteId::parse("nonesuch").unwrap_or_else(|_| unreachable!("test route id is valid"))
}

fn registry() -> &'static ScreenRegistry {
    screen_registry().unwrap_or_else(|_| unreachable!("the compiled registry must be well formed"))
}

fn declaration(schema: Vec<ActivationField>) -> RouteDeclaration {
    RouteDeclaration {
        id: review_route(),
        activation_schema: schema,
        target_screen: ScreenIdentity::Compiled(ScreenId::PullRequests),
    }
}

fn values(entries: Vec<(&str, ActivationValue)>) -> ActivationValues {
    let entries: Vec<(Id, ActivationValue)> = entries
        .into_iter()
        .map(|(name, value)| (id(name), value))
        .collect();
    ActivationValues::new(entries)
        .unwrap_or_else(|_| unreachable!("test activation values are within their bounds"))
}

fn refusal(result: Result<(), ActivationError>) -> ActivationError {
    match result {
        Ok(()) => panic!("the activation must be refused"),
        Err(error) => error,
    }
}

#[test]
fn every_compiled_screen_is_reachable_through_its_declared_route() {
    let registry = registry();
    for screen in ScreenId::ALL {
        let Some(descriptor) = registry.get(screen) else {
            panic!("every compiled screen has a descriptor");
        };
        let Ok(resolved) = route_declaration(registry, descriptor.route) else {
            panic!("a compiled screen's own route must resolve");
        };
        assert_eq!(resolved.target_screen, ScreenIdentity::Compiled(screen));
        assert_eq!(resolved.id, descriptor.route);
        assert_eq!(resolved.activation_schema, descriptor.activation);
    }
}

#[test]
fn an_unknown_route_is_refused_without_a_declaration() {
    match route_declaration(registry(), unknown_route()) {
        Ok(_) => panic!("a route no descriptor declares must not resolve"),
        Err(error) => assert!(matches!(error, ActivationError::UnknownRoute { .. })),
    }
}

#[test]
fn every_activation_failure_reports_the_navigation_code() {
    let refusals = [
        ActivationError::UnknownRoute {
            route: unknown_route(),
        },
        ActivationError::UnknownField { field: id("extra") },
        ActivationError::MissingField {
            field: id("number"),
        },
        ActivationError::WrongKind {
            field: id("number"),
            expected: "integer",
            actual: "string",
        },
        ActivationError::NotPermitted { field: id("tab") },
        ActivationError::TooManyFields { count: 33 },
        ActivationError::TooLarge { bytes: 262_145 },
    ];
    for refusal in refusals {
        assert_eq!(refusal.code(), NavCode::E001);
        let rendered = refusal.to_string();
        assert!(
            rendered.starts_with("NAV-E001: "),
            "refusal must render its code: {rendered}"
        );
    }
}

#[test]
fn a_compiled_screen_accepts_an_empty_activation() {
    let registry = registry();
    for screen in ScreenId::ALL {
        let Some(descriptor) = registry.get(screen) else {
            panic!("every compiled screen has a descriptor");
        };
        let Ok(declared) = route_declaration(registry, descriptor.route) else {
            panic!("a compiled screen's own route must resolve");
        };
        assert_eq!(
            declared.validate(&ActivationValues::empty()),
            Ok(()),
            "compiled screen {screen} declares no activation fields"
        );
    }
}

#[test]
fn every_declared_kind_validates_against_a_matching_value() {
    let declared = declaration(vec![
        field("flag", ActivationKind::Boolean),
        field("maybe", ActivationKind::OptionalBoolean),
        field("title", ActivationKind::Text),
        field("number", ActivationKind::Integer),
        field(
            "tab",
            ActivationKind::Enumerated {
                permitted: vec!["files".to_owned(), "conversation".to_owned()],
            },
        ),
        field("work-dir", ActivationKind::Path),
        field("labels", ActivationKind::TextList),
    ]);
    let supplied = values(vec![
        ("flag", ActivationValue::Boolean(true)),
        ("maybe", ActivationValue::OptionalBoolean(None)),
        ("title", ActivationValue::Text("Pull Request 42".to_owned())),
        ("number", ActivationValue::Integer(42)),
        ("tab", ActivationValue::Enumerated("files".to_owned())),
        (
            "work-dir",
            ActivationValue::Path(PathBuf::from("/tmp/work")),
        ),
        (
            "labels",
            ActivationValue::TextList(vec!["bug".to_owned(), "ui".to_owned()]),
        ),
    ]);
    assert_eq!(declared.validate(&supplied), Ok(()));
}

#[test]
fn an_undeclared_field_is_refused() {
    let declared = declaration(vec![field("number", ActivationKind::Integer)]);
    let supplied = values(vec![
        ("number", ActivationValue::Integer(42)),
        ("extra", ActivationValue::Boolean(true)),
    ]);
    assert_eq!(
        declared.validate(&supplied),
        Err(ActivationError::UnknownField { field: id("extra") })
    );
}

#[test]
fn a_missing_declared_field_is_refused() {
    let declared = declaration(vec![
        field("number", ActivationKind::Integer),
        field("flag", ActivationKind::Boolean),
    ]);
    let supplied = values(vec![("number", ActivationValue::Integer(42))]);
    assert_eq!(
        declared.validate(&supplied),
        Err(ActivationError::MissingField { field: id("flag") })
    );
}

#[test]
fn a_value_of_the_wrong_kind_is_refused() {
    let declared = declaration(vec![field("number", ActivationKind::Integer)]);
    let supplied = values(vec![(
        "number",
        ActivationValue::Text("forty-two".to_owned()),
    )]);
    assert_eq!(
        declared.validate(&supplied),
        Err(ActivationError::WrongKind {
            field: id("number"),
            expected: "integer",
            actual: "string",
        })
    );
}

#[test]
fn an_optional_boolean_declaration_does_not_accept_a_plain_boolean() {
    // The two kinds are distinct in the schema, so a screen that declared an
    // absent-capable field never silently receives one that cannot be absent.
    let declared = declaration(vec![field("maybe", ActivationKind::OptionalBoolean)]);
    let supplied = values(vec![("maybe", ActivationValue::Boolean(true))]);
    assert!(matches!(
        declared.validate(&supplied),
        Err(ActivationError::WrongKind { .. })
    ));
}

#[test]
fn an_enumerated_value_outside_the_permitted_set_is_refused() {
    let declared = declaration(vec![field(
        "tab",
        ActivationKind::Enumerated {
            permitted: vec!["files".to_owned(), "conversation".to_owned()],
        },
    )]);
    let supplied = values(vec![(
        "tab",
        ActivationValue::Enumerated("diff".to_owned()),
    )]);
    assert_eq!(
        declared.validate(&supplied),
        Err(ActivationError::NotPermitted { field: id("tab") })
    );
}

#[test]
fn more_fields_than_the_declared_bound_are_refused_before_validation() {
    let entries: Vec<(Id, ActivationValue)> = (0..=MAX_ACTIVATION_FIELDS)
        .map(|index| {
            (
                id(&format!("field-{index}")),
                ActivationValue::Integer(i64::try_from(index).unwrap_or_default()),
            )
        })
        .collect();
    let count = entries.len();
    assert_eq!(
        ActivationValues::new(entries),
        Err(ActivationError::TooManyFields { count })
    );
}

#[test]
fn exactly_the_declared_field_bound_is_accepted() {
    let entries: Vec<(Id, ActivationValue)> = (0..MAX_ACTIVATION_FIELDS)
        .map(|index| {
            (
                id(&format!("field-{index}")),
                ActivationValue::Integer(i64::try_from(index).unwrap_or_default()),
            )
        })
        .collect();
    let Ok(built) = ActivationValues::new(entries) else {
        panic!("32 fields is within the bound");
    };
    assert_eq!(built.len(), MAX_ACTIVATION_FIELDS);
}

#[test]
fn an_activation_larger_than_the_serialized_bound_is_refused() {
    let oversized = "x".repeat(MAX_ACTIVATION_BYTES + 1);
    match ActivationValues::new(vec![(id("title"), ActivationValue::Text(oversized))]) {
        Ok(_) => panic!("an oversized activation must be refused"),
        Err(error) => assert!(matches!(error, ActivationError::TooLarge { .. })),
    }
}

#[test]
fn a_refusal_never_carries_the_offending_value() {
    let declared = declaration(vec![field("title", ActivationKind::Integer)]);
    let supplied = values(vec![(
        "title",
        ActivationValue::Text("s3cret-material".to_owned()),
    )]);
    let rendered = refusal(declared.validate(&supplied)).to_string();
    assert!(
        !rendered.contains("s3cret-material"),
        "a diagnostic must not repeat an activation value: {rendered}"
    );
    assert!(rendered.contains("title"), "the field name is safe to name");
}

#[test]
fn duplicate_field_names_collapse_to_one_entry() {
    // The container is keyed by field name, so a caller cannot smuggle two
    // values for one declared field past validation.
    let Ok(built) = ActivationValues::new(vec![
        (id("number"), ActivationValue::Integer(1)),
        (id("number"), ActivationValue::Integer(2)),
    ]) else {
        panic!("duplicate keys are within the field bound");
    };
    assert_eq!(built.len(), 1);
    assert_eq!(
        built.get(&id("number")),
        Some(&ActivationValue::Integer(2)),
        "the last value for a field wins"
    );
}
