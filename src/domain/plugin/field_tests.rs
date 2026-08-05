//! Field declaration table (issue #389 CW-09, acceptance rows D5 and D6).

use super::*;
use crate::domain::plugin::limits::FIELD_CHOICE_LIMIT;
use crate::domain::{CanonicalDecimal, Id};

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
        kind,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
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
    candidate.default = Some(Scalar::Text("nope".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultKindMismatch);

    let mut good = draft(FieldKind::Integer);
    good.default = Some(Scalar::Integer(3));
    assert!(Field::parse(good).is_ok());
}

#[test]
fn an_integer_field_does_not_accept_a_decimal_default() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.default = Some(decimal("1.5"));
    assert_eq!(error_of(candidate), FieldError::DefaultKindMismatch);
}

#[test]
fn a_finite_number_field_accepts_integer_and_decimal_defaults() {
    for value in [Scalar::Integer(2), decimal("1.5")] {
        let mut candidate = draft(FieldKind::FiniteNumber);
        candidate.default = Some(value);
        assert!(Field::parse(candidate).is_ok());
    }
}

#[test]
fn an_enum_default_must_be_one_of_its_choices() {
    let mut candidate = draft(FieldKind::Enum);
    candidate.choices = vec![Scalar::Text("a".to_owned())];
    candidate.default = Some(Scalar::Text("b".to_owned()));
    assert_eq!(error_of(candidate), FieldError::DefaultNotAChoice);

    let mut good = draft(FieldKind::Enum);
    good.choices = vec![Scalar::Text("a".to_owned())];
    good.default = Some(Scalar::Text("a".to_owned()));
    assert!(Field::parse(good).is_ok());
}

#[test]
fn a_secret_reference_field_may_never_carry_a_default() {
    // A default would put the secret itself in the manifest.
    let mut candidate = draft(FieldKind::SecretReference);
    candidate.default = Some(Scalar::Text("ghp_realtoken".to_owned()));
    assert_eq!(error_of(candidate), FieldError::SecretDefault);
}

#[test]
fn only_numeric_kinds_may_declare_bounds() {
    for kind in FieldKind::ALL
        .into_iter()
        .filter(|kind| !matches!(kind, FieldKind::Integer | FieldKind::FiniteNumber))
    {
        let mut candidate = draft(kind);
        if kind == FieldKind::Enum {
            candidate.choices = vec![Scalar::Text("a".to_owned())];
        }
        candidate.minimum = Some(Scalar::Integer(0));
        assert_eq!(
            error_of(candidate),
            FieldError::BoundsOnNonNumeric,
            "{kind:?} must not declare bounds"
        );
    }
}

#[test]
fn inverted_bounds_are_rejected_and_equal_bounds_are_accepted() {
    let mut inverted = draft(FieldKind::Integer);
    inverted.minimum = Some(Scalar::Integer(5));
    inverted.maximum = Some(Scalar::Integer(4));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);

    let mut equal = draft(FieldKind::Integer);
    equal.minimum = Some(Scalar::Integer(4));
    equal.maximum = Some(Scalar::Integer(4));
    assert!(Field::parse(equal).is_ok());
}

#[test]
fn a_default_outside_declared_bounds_is_rejected() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.minimum = Some(Scalar::Integer(1));
    candidate.maximum = Some(Scalar::Integer(10));
    candidate.default = Some(Scalar::Integer(11));
    assert_eq!(error_of(candidate), FieldError::DefaultOutOfBounds);

    let mut at_edge = draft(FieldKind::Integer);
    at_edge.minimum = Some(Scalar::Integer(1));
    at_edge.maximum = Some(Scalar::Integer(10));
    at_edge.default = Some(Scalar::Integer(10));
    assert!(Field::parse(at_edge).is_ok());
}

#[test]
fn decimal_bounds_compare_numerically_not_lexically() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.minimum = Some(decimal("9.5"));
    candidate.maximum = Some(decimal("10.5"));
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
    inverted.minimum = Some(Scalar::Integer(9_007_199_254_740_993));
    inverted.maximum = Some(Scalar::Integer(9_007_199_254_740_992));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn a_default_just_outside_a_large_bound_is_still_rejected() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.maximum = Some(Scalar::Integer(9_007_199_254_740_992));
    candidate.default = Some(Scalar::Integer(9_007_199_254_740_993));
    assert_eq!(error_of(candidate), FieldError::DefaultOutOfBounds);
}

#[test]
fn decimal_bounds_compare_by_fraction_width() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.minimum = Some(decimal("0.1"));
    candidate.maximum = Some(decimal("0.10001"));
    assert!(
        Field::parse(candidate).is_ok(),
        "0.1 is below 0.10001 once fractions are padded to a common width"
    );

    let mut inverted = draft(FieldKind::FiniteNumber);
    inverted.minimum = Some(decimal("0.10001"));
    inverted.maximum = Some(decimal("0.1"));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn negative_bounds_order_by_value_not_magnitude() {
    let mut candidate = draft(FieldKind::FiniteNumber);
    candidate.minimum = Some(decimal("-10.5"));
    candidate.maximum = Some(decimal("-9.5"));
    assert!(
        Field::parse(candidate).is_ok(),
        "-10.5 is below -9.5 even though its magnitude is larger"
    );

    let mut inverted = draft(FieldKind::FiniteNumber);
    inverted.minimum = Some(decimal("-9.5"));
    inverted.maximum = Some(decimal("-10.5"));
    assert_eq!(error_of(inverted), FieldError::InvertedBounds);
}

#[test]
fn a_bound_may_span_zero() {
    let mut candidate = draft(FieldKind::Integer);
    candidate.minimum = Some(Scalar::Integer(-5));
    candidate.maximum = Some(Scalar::Integer(5));
    candidate.default = Some(Scalar::Integer(0));
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
