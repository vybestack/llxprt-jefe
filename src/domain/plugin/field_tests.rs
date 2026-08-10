//! Field declaration table (issue #389 CW-09, acceptance rows D5 and D6).

use super::*;
use crate::domain::plugin::limits::FIELD_CHOICE_LIMIT;
use crate::domain::{CanonicalDecimal, Id, SecretRef, TypedValue};
use crate::test_support::Must;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn decimal(value: &str) -> Scalar {
    Scalar::Decimal(
        CanonicalDecimal::parse(value)
            .unwrap_or_else(|error| panic!("{value} must parse: {error}")),
    )
}

/// A minimal field of `kind` with everything optional left unset.
fn draft(kind: FieldKind) -> FieldDraft {
    FieldDraft {
        id: id("setting"),
        label: "Setting".to_owned(),
        description: None,
        kind,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    }
}

fn error_of(draft: FieldDraft) -> FieldError {
    Field::parse(draft)
        .err()
        .unwrap_or_else(|| panic!("the draft must be rejected"))
}

#[test]
fn field_kinds_use_lower_kebab_case_wire_names() {
    assert_eq!(FieldKind::Boolean.as_wire(), "boolean");
    assert_eq!(FieldKind::FiniteNumber.as_wire(), "finite-number");
    assert_eq!(FieldKind::StringList.as_wire(), "string-list");
    assert_eq!(FieldKind::SecretReference.as_wire(), "secret-reference");
    for kind in FieldKind::ALL {
        assert_eq!(FieldKind::from_wire(kind.as_wire()), Some(kind));
    }
}

#[test]
fn restart_scopes_use_lower_kebab_case_wire_names() {
    for (scope, wire) in [
        (RestartScope::None, "none"),
        (RestartScope::Provider, "provider"),
        (RestartScope::Host, "host"),
    ] {
        assert_eq!(scope.as_wire(), wire);
        assert_eq!(RestartScope::from_wire(wire), Some(scope));
    }
}

#[test]
fn wrong_case_and_snake_case_spellings_are_not_wire_names() {
    for text in [
        "Boolean",
        "finite_number",
        "finiteNumber",
        "string_list",
        "SECRET-REFERENCE",
        "",
    ] {
        assert_eq!(FieldKind::from_wire(text), None, "{text:?} is not a kind");
    }
}

#[test]
fn a_minimal_field_of_every_scalar_kind_parses() {
    for kind in FieldKind::ALL {
        let mut candidate = draft(kind);
        if kind == FieldKind::Enum {
            candidate.choices = vec![Scalar::Text("a".to_owned())];
        }
        let field =
            Field::parse(candidate).unwrap_or_else(|error| panic!("{kind:?} must parse: {error}"));
        assert_eq!(field.kind(), kind);
        assert_eq!(field.id().as_str(), "setting");
        assert_eq!(field.restart(), RestartScope::None);
    }
}

#[test]
fn an_enum_field_must_declare_at_least_one_choice() {
    assert_eq!(
        error_of(draft(FieldKind::Enum)),
        FieldError::EnumWithoutChoices
    );
}

#[test]
fn only_an_enum_field_may_declare_choices() {
    for kind in FieldKind::ALL.into_iter().filter(|k| *k != FieldKind::Enum) {
        let mut candidate = draft(kind);
        candidate.choices = vec![Scalar::Text("a".to_owned())];
        assert_eq!(
            error_of(candidate),
            FieldError::ChoicesOnNonEnum,
            "{kind:?} must not declare choices"
        );
    }
}

#[test]
fn choices_accept_the_limit_and_reject_one_more() {
    let mut at_limit = draft(FieldKind::Enum);
    at_limit.choices = (0..FIELD_CHOICE_LIMIT)
        .map(|index| Scalar::Text(format!("c{index}")))
        .collect();
    assert!(Field::parse(at_limit).is_ok());

    let mut over_limit = draft(FieldKind::Enum);
    over_limit.choices = (0..=FIELD_CHOICE_LIMIT)
        .map(|index| Scalar::Text(format!("c{index}")))
        .collect();
    assert_eq!(
        error_of(over_limit),
        FieldError::TooManyChoices {
            len: FIELD_CHOICE_LIMIT + 1
        }
    );
}

#[test]
fn duplicate_choices_are_rejected() {
    let mut candidate = draft(FieldKind::Enum);
    candidate.choices = vec![Scalar::Text("a".to_owned()), Scalar::Text("a".to_owned())];
    assert_eq!(error_of(candidate), FieldError::DuplicateChoice);
}

#[test]
fn a_default_must_match_the_declared_kind() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.default = Some(TypedValue::String("nope".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultKindMismatch);

    let mut good = draft(FieldKind::Integer);
    good.default = Some(TypedValue::Integer(3));
    assert!(Field::parse(good).is_ok());
}

#[test]
fn an_integer_field_does_not_accept_a_decimal_default() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.default = Some(TypedValue::Decimal(
        CanonicalDecimal::parse("1.5").must("decimal"),
    ));
    assert_eq!(error_of(candidate), FieldError::DefaultKindMismatch);
}

#[test]
fn a_finite_number_field_accepts_integer_and_decimal_defaults() {
    for value in [
        TypedValue::Integer(2),
        TypedValue::Decimal(CanonicalDecimal::parse("1.5").must("decimal")),
    ] {
        let mut candidate = draft(FieldKind::FiniteNumber);
        candidate.default = Some(value);
        assert!(Field::parse(candidate).is_ok());
    }
}

#[test]
fn an_enum_default_must_be_one_of_its_choices() {
    let mut candidate = draft(FieldKind::Enum);
    candidate.choices = vec![Scalar::Text("a".to_owned())];
    candidate.default = Some(TypedValue::String("b".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultNotAChoice);

    let mut good = draft(FieldKind::Enum);
    good.choices = vec![Scalar::Text("a".to_owned())];
    good.default = Some(TypedValue::String("a".to_owned()));
    assert!(Field::parse(good).is_ok());
}

#[test]
fn a_secret_reference_field_accepts_only_a_secret_ref_default() {
    // A string literal is wrong-typed and must be rejected.
    let mut candidate = draft(FieldKind::SecretReference);
    candidate.default = Some(TypedValue::String("ghp_realtoken".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultKindMismatch);

    // A SecretRef default names the environment variable; it is not the secret.
    let mut good = draft(FieldKind::SecretReference);
    good.default = Some(TypedValue::SecretRef(SecretRef {
        env: crate::domain::plugin::SecretReference::parse("API_TOKEN")
            .unwrap_or_else(|error| panic!("must parse: {error}")),
    }));
    assert!(Field::parse(good).is_ok());
}

#[test]
fn only_boundable_kinds_may_declare_bounds() {
    for kind in FieldKind::ALL.into_iter().filter(|kind| {
        !matches!(
            kind,
            FieldKind::Integer
                | FieldKind::FiniteNumber
                | FieldKind::String
                | FieldKind::StringList
        )
    }) {
        let mut candidate = draft(kind);
        if kind == FieldKind::Enum {
            candidate.choices = vec![Scalar::Text("a".to_owned())];
        }
        candidate.min = Some(Scalar::Integer(0));
        assert_eq!(
            error_of(candidate),
            FieldError::BoundsOnUnsupportedKind,
            "{kind:?} must not declare bounds"
        );
    }
}

#[test]
fn inverted_bounds_are_rejected_and_equal_bounds_are_accepted() {
    let mut inverted = draft(FieldKind::Integer);
    inverted.min = Some(Scalar::Integer(5));
    inverted.max = Some(Scalar::Integer(4));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);

    let mut equal = draft(FieldKind::Integer);
    equal.min = Some(Scalar::Integer(4));
    equal.max = Some(Scalar::Integer(4));
    assert!(Field::parse(equal).is_ok());
}

#[test]
fn inverted_string_and_list_length_bounds_are_rejected() {
    for kind in [FieldKind::String, FieldKind::StringList] {
        let mut candidate = draft(kind);
        candidate.min = Some(Scalar::Integer(5));
        candidate.max = Some(Scalar::Integer(4));

        assert_eq!(
            error_of(candidate),
            FieldError::InvertedBounds,
            "{kind:?} length bounds must preserve min <= max"
        );
    }
}

#[test]
fn a_default_outside_declared_bounds_is_rejected() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.min = Some(Scalar::Integer(1));
    candidate.max = Some(Scalar::Integer(10));
    candidate.default = Some(TypedValue::Integer(11));
    assert_eq!(error_of(candidate), FieldError::DefaultOutOfBounds);

    let mut at_edge = draft(FieldKind::Integer);
    at_edge.min = Some(Scalar::Integer(1));
    at_edge.max = Some(Scalar::Integer(10));
    at_edge.default = Some(TypedValue::Integer(10));
    assert!(Field::parse(at_edge).is_ok());
}

#[test]
fn decimal_bounds_compare_numerically_not_lexically() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.min = Some(decimal("9.5"));
    candidate.max = Some(decimal("10.5"));
    assert!(
        Field::parse(candidate).is_ok(),
        "9.5 is below 10.5 numerically even though it sorts after lexically"
    );
}

#[test]
fn large_integer_bounds_do_not_collapse_onto_each_other() {
    // These two values are one apart and both beyond f64's 52-bit mantissa, so
    // a float comparison would call them equal and accept the inverted pair.
    let mut inverted = draft(FieldKind::Integer);
    inverted.min = Some(Scalar::Integer(9_007_199_254_740_993));
    inverted.max = Some(Scalar::Integer(9_007_199_254_740_992));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn a_default_just_outside_a_large_bound_is_still_rejected() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.max = Some(Scalar::Integer(9_007_199_254_740_992));
    candidate.default = Some(TypedValue::Integer(9_007_199_254_740_993));
    assert_eq!(error_of(candidate), FieldError::DefaultOutOfBounds);
}

#[test]
fn decimal_bounds_compare_by_fraction_width() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.min = Some(decimal("0.1"));
    candidate.max = Some(decimal("0.10001"));
    assert!(
        Field::parse(candidate).is_ok(),
        "0.1 is below 0.10001 once fractions are padded to a common width"
    );

    let mut inverted = draft(FieldKind::FiniteNumber);
    inverted.min = Some(decimal("0.10001"));
    inverted.max = Some(decimal("0.1"));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn negative_bounds_order_by_value_not_magnitude() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.min = Some(decimal("-10.5"));
    candidate.max = Some(decimal("-9.5"));
    assert!(
        Field::parse(candidate).is_ok(),
        "-10.5 is below -9.5 even though its magnitude is larger"
    );

    let mut inverted = draft(FieldKind::FiniteNumber);
    inverted.min = Some(decimal("-9.5"));
    inverted.max = Some(decimal("-10.5"));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn a_bound_may_span_zero() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.min = Some(Scalar::Integer(-5));
    candidate.max = Some(Scalar::Integer(5));
    candidate.default = Some(TypedValue::Integer(0));
    assert!(Field::parse(candidate).is_ok());
}

#[test]
fn a_field_may_not_gate_its_own_visibility() {
    let mut candidate = draft(FieldKind::Boolean);
    candidate.visible_when = Some(id("setting"));
    assert_eq!(error_of(candidate), FieldError::SelfVisibility);
}

#[test]
fn a_field_records_the_sibling_that_gates_it() {
    let mut candidate = draft(FieldKind::Boolean);
    candidate.visible_when = Some(id("other"));
    let field = Field::parse(candidate).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(
        field.visible_when().map(Id::as_str),
        Some("other"),
        "the reference is recorded here and resolved by manifest validation"
    );
}

#[test]
fn a_field_exposes_its_label_and_description() {
    let mut candidate = draft(FieldKind::String);
    candidate.label = "Display Name".to_owned();
    candidate.description = Some("Shown in the header".to_owned());
    let field = Field::parse(candidate).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(field.label(), "Display Name");
    assert_eq!(field.description(), Some("Shown in the header"));
}

#[test]
fn a_field_may_not_have_a_blank_label() {
    let mut candidate = draft(FieldKind::String);
    candidate.label = "  ".to_owned();
    assert_eq!(error_of(candidate), FieldError::BlankLabel);
}

#[test]
fn unique_is_rejected_on_every_kind_except_string_list() {
    for kind in FieldKind::ALL
        .into_iter()
        .filter(|kind| *kind != FieldKind::StringList)
    {
        let mut candidate = draft(kind);
        if kind == FieldKind::Enum {
            candidate.choices = vec![Scalar::Text("a".to_owned())];
        }
        candidate.unique = true;
        assert_eq!(
            error_of(candidate),
            FieldError::UniqueOnNonList,
            "{kind:?} must not declare unique"
        );
    }
}

#[test]
fn unique_is_accepted_on_string_list_and_defaults_false() {
    let mut candidate = draft(FieldKind::StringList);
    candidate.unique = true;
    let field = Field::parse(candidate).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert!(field.unique());

    let unset = draft(FieldKind::StringList);
    let field = Field::parse(unset).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert!(!field.unique());
}

#[test]
fn string_and_string_list_accept_integer_length_bounds() {
    for kind in [FieldKind::String, FieldKind::StringList] {
        let mut candidate = draft(kind);
        candidate.min = Some(Scalar::Integer(1));
        candidate.max = Some(Scalar::Integer(100));
        let field =
            Field::parse(candidate).unwrap_or_else(|error| panic!("{kind:?} must parse: {error}"));
        assert_eq!(field.min(), Some(&Scalar::Integer(1)));
        assert_eq!(field.max(), Some(&Scalar::Integer(100)));
    }
}

#[test]
fn string_and_string_list_bounds_must_be_integers() {
    for kind in [FieldKind::String, FieldKind::StringList] {
        let mut candidate = draft(kind);
        candidate.min = Some(decimal("1.5"));
        assert_eq!(
            error_of(candidate),
            FieldError::BoundKindMismatch,
            "{kind:?} length bound must be an integer"
        );
    }
}

#[test]
fn a_string_default_outside_length_bounds_is_rejected() {
    let mut candidate = draft(FieldKind::String);
    candidate.min = Some(Scalar::Integer(3));
    candidate.default = Some(TypedValue::String("ab".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultOutOfBounds);

    let mut inside = draft(FieldKind::String);
    inside.min = Some(Scalar::Integer(3));
    inside.default = Some(TypedValue::String("abc".to_owned()));
    assert!(Field::parse(inside).is_ok());
}

#[test]
fn a_string_list_field_accepts_a_list_default() {
    let mut candidate = draft(FieldKind::StringList);
    candidate.default = Some(TypedValue::List(vec![
        TypedValue::String("alpha".to_owned()),
        TypedValue::String("beta".to_owned()),
    ]));
    let field = Field::parse(candidate).must("a list default must parse");
    assert!(field.default().is_some());
}

#[test]
fn a_secret_reference_field_accepts_a_reference_default() {
    let mut candidate = draft(FieldKind::SecretReference);
    candidate.default = Some(TypedValue::SecretRef(SecretRef {
        env: crate::domain::plugin::SecretReference::parse("API_TOKEN")
            .unwrap_or_else(|error| panic!("must parse: {error}")),
    }));
    assert!(
        Field::parse(candidate).is_ok(),
        "a secret-reference field may declare a reference default"
    );
}

#[test]
fn a_wrong_type_default_is_rejected() {
    let mut secret = draft(FieldKind::SecretReference);
    secret.default = Some(TypedValue::String("ghp_token".to_owned()));
    assert_eq!(
        error_of(secret),
        FieldError::DefaultKindMismatch,
        "a literal string is not a valid secret-reference default"
    );
}

#[test]
fn a_string_list_default_outside_length_bounds_is_rejected() {
    let mut candidate = draft(FieldKind::StringList);
    candidate.min = Some(Scalar::Integer(3));
    candidate.default = Some(TypedValue::List(vec![
        TypedValue::String("a".to_owned()),
        TypedValue::String("b".to_owned()),
    ]));
    assert_eq!(
        error_of(candidate),
        FieldError::DefaultOutOfBounds,
        "two items is below the declared minimum of three"
    );
}
