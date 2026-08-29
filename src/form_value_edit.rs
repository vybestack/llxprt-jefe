//! Pure typed editing for shared Form controls.

use crate::domain::plugin::field::{Field, FieldKind, Scalar};
use crate::domain::plugin_config::validate_field_value;
use crate::domain::{CanonicalDecimal, SecretRef, TypedValue};

/// One explicit, UI-independent edit to a typed Form value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormValueEdit {
    /// Append one character using the field kind's typed syntax.
    Character(char),
    /// Remove the final character or list element.
    Backspace,
    /// Toggle a boolean value.
    Toggle,
    /// Select the next declared enum choice, wrapping at the end.
    NextChoice,
}

/// Apply one edit and return either a complete typed value or a syntactically
/// valid intermediate draft.
///
/// Intermediate numeric and secret drafts use `TypedValue::String`; they remain
/// host-local and cannot pass submission validation until they become complete.
#[must_use]
pub fn edit_form_value(
    field: &Field,
    current: Option<&TypedValue>,
    edit: FormValueEdit,
) -> Option<TypedValue> {
    match field.kind() {
        FieldKind::Boolean => edit_boolean(current, edit),
        FieldKind::String | FieldKind::Path => edit_string(current, edit),
        FieldKind::Integer => edit_integer(current, edit),
        FieldKind::FiniteNumber => edit_finite_number(current, edit),
        FieldKind::Enum => edit_enum(field, current, edit),
        FieldKind::StringList => edit_string_list(current, edit),
        FieldKind::SecretReference => edit_secret_reference(current, edit),
    }
}

/// Whether one draft is complete and satisfies every field declaration.
#[must_use]
pub fn form_value_is_complete(field: &Field, value: &TypedValue) -> bool {
    validate_field_value(field, value).is_ok()
}

/// Whether one value has the field kind's editable syntax, even when it does
/// not yet satisfy the declaration's required/min/max/choice constraints.
#[must_use]
pub fn form_value_has_editable_syntax(field: &Field, value: &TypedValue) -> bool {
    match field.kind() {
        FieldKind::Boolean => matches!(value, TypedValue::Bool(_)),
        FieldKind::String | FieldKind::Path | FieldKind::Enum => {
            matches!(value, TypedValue::String(_))
        }
        FieldKind::Integer => {
            matches!(value, TypedValue::Integer(_))
                || matches!(value, TypedValue::String(value) if is_integer_draft(value))
        }
        FieldKind::FiniteNumber => {
            matches!(value, TypedValue::Integer(_) | TypedValue::Decimal(_))
                || matches!(value, TypedValue::String(value) if is_finite_number_draft(value))
        }
        FieldKind::StringList => matches!(
            value,
            TypedValue::List(values)
                if values
                    .iter()
                    .all(|value| matches!(value, TypedValue::String(_)))
        ),
        FieldKind::SecretReference => {
            matches!(value, TypedValue::SecretRef(_))
                || matches!(value, TypedValue::String(value) if value.is_empty())
        }
    }
}

fn edit_boolean(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    (edit == FormValueEdit::Toggle).then_some(TypedValue::Bool(!matches!(
        current,
        Some(TypedValue::Bool(true))
    )))
}

fn edit_string(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    let mut value = match current {
        Some(TypedValue::String(value)) => value.clone(),
        None => String::new(),
        _ => return None,
    };
    edit_text(&mut value, edit)?;
    Some(TypedValue::String(value))
}

fn edit_integer(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    let mut value = match current {
        Some(TypedValue::Integer(value)) => value.to_string(),
        Some(TypedValue::String(value)) if is_integer_draft(value) => value.clone(),
        None => String::new(),
        _ => return None,
    };
    edit_text(&mut value, edit)?;
    if !is_integer_draft(&value) {
        return None;
    }
    value.parse().ok().map_or_else(
        || Some(TypedValue::String(value)),
        |value| Some(TypedValue::Integer(value)),
    )
}

fn edit_finite_number(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    let mut value = match current {
        Some(TypedValue::Integer(value)) => value.to_string(),
        Some(TypedValue::Decimal(value)) => value.as_str().to_owned(),
        Some(TypedValue::String(value)) if is_finite_number_draft(value) => value.clone(),
        None => String::new(),
        _ => return None,
    };
    edit_text(&mut value, edit)?;
    if !is_finite_number_draft(&value) {
        return None;
    }
    value
        .parse::<i64>()
        .ok()
        .map(TypedValue::Integer)
        .or_else(|| {
            CanonicalDecimal::parse(&value)
                .ok()
                .map(TypedValue::Decimal)
        })
        .or(Some(TypedValue::String(value)))
}

fn is_integer_draft(value: &str) -> bool {
    value.is_empty()
        || value == "-"
        || value
            .strip_prefix('-')
            .unwrap_or(value)
            .bytes()
            .all(|byte| byte.is_ascii_digit())
}

fn is_finite_number_draft(value: &str) -> bool {
    if value.is_empty() || value == "-" {
        return true;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let Some((integer, fraction)) = unsigned.split_once('.') else {
        return unsigned.bytes().all(|byte| byte.is_ascii_digit());
    };
    !integer.is_empty()
        && integer.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
        && !fraction.contains('.')
}

fn edit_enum(
    field: &Field,
    current: Option<&TypedValue>,
    edit: FormValueEdit,
) -> Option<TypedValue> {
    if edit != FormValueEdit::NextChoice {
        return None;
    }
    let choices: Vec<&str> = field
        .choices()
        .iter()
        .filter_map(|choice| match choice {
            Scalar::Text(value) => Some(value.as_str()),
            Scalar::Bool(_) | Scalar::Integer(_) | Scalar::Decimal(_) => None,
        })
        .collect();
    let current = match current {
        Some(TypedValue::String(value)) => Some(value.as_str()),
        None => None,
        _ => return None,
    };
    let index = current
        .and_then(|value| choices.iter().position(|choice| *choice == value))
        .map_or(0, |index| (index + 1) % choices.len());
    choices
        .get(index)
        .map(|choice| TypedValue::String((*choice).to_owned()))
}

fn edit_string_list(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    let mut values = match current {
        Some(TypedValue::List(values))
            if values
                .iter()
                .all(|value| matches!(value, TypedValue::String(_))) =>
        {
            values.clone()
        }
        None => Vec::new(),
        _ => return None,
    };
    match edit {
        FormValueEdit::Character(',') => values.push(TypedValue::String(String::new())),
        FormValueEdit::Character(character) => {
            if values.is_empty() {
                values.push(TypedValue::String(String::new()));
            }
            let Some(TypedValue::String(value)) = values.last_mut() else {
                return None;
            };
            value.push(character);
        }
        FormValueEdit::Backspace => {
            let Some(TypedValue::String(value)) = values.last_mut() else {
                return None;
            };
            if value.pop().is_none() {
                values.pop();
            }
        }
        FormValueEdit::Toggle | FormValueEdit::NextChoice => return None,
    }
    Some(TypedValue::List(values))
}

fn edit_secret_reference(current: Option<&TypedValue>, edit: FormValueEdit) -> Option<TypedValue> {
    let mut value = match current {
        Some(TypedValue::SecretRef(value)) => value.env.env().to_owned(),
        Some(TypedValue::String(value)) if value.is_empty() => value.clone(),
        None => String::new(),
        _ => return None,
    };
    edit_text(&mut value, edit)?;
    if value.is_empty() {
        return Some(TypedValue::String(value));
    }
    crate::domain::plugin::SecretReference::parse(&value)
        .ok()
        .map(|env| TypedValue::SecretRef(SecretRef { env }))
}

fn edit_text(value: &mut String, edit: FormValueEdit) -> Option<()> {
    match edit {
        FormValueEdit::Character(character) => value.push(character),
        FormValueEdit::Backspace => {
            value.pop()?;
        }
        FormValueEdit::Toggle | FormValueEdit::NextChoice => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{FormValueEdit, edit_form_value};
    use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
    use crate::domain::{CanonicalDecimal, Id, SecretRef, TypedValue};

    fn field(kind: FieldKind, choices: Vec<Scalar>, min: Option<Scalar>) -> Field {
        let Ok(id) = Id::parse(kind.as_wire()) else {
            panic!("test field ID must be valid");
        };
        let Ok(field) = Field::parse(FieldDraft {
            id,
            label: kind.as_wire().to_owned(),
            description: None,
            kind,
            required: true,
            default: None,
            min,
            max: None,
            choices,
            unique: false,
            visible_when: None,
            restart: RestartScope::None,
        }) else {
            panic!("test field must be valid");
        };
        field
    }

    fn edited(field: &Field, current: Option<&TypedValue>, edit: FormValueEdit) -> TypedValue {
        let Some(value) = edit_form_value(field, current, edit) else {
            panic!("test edit must produce a value");
        };
        value
    }

    fn decimal(value: &str) -> CanonicalDecimal {
        let Ok(value) = CanonicalDecimal::parse(value) else {
            panic!("test decimal must be canonical");
        };
        value
    }

    fn secret_reference(value: &str) -> crate::domain::plugin::SecretReference {
        let Ok(value) = crate::domain::plugin::SecretReference::parse(value) else {
            panic!("test secret reference must be valid");
        };
        value
    }

    #[test]
    fn explicit_edits_construct_boolean_string_integer_and_number_values() {
        let boolean = field(FieldKind::Boolean, Vec::new(), None);
        assert_eq!(
            edit_form_value(&boolean, None, FormValueEdit::Toggle),
            Some(TypedValue::Bool(true))
        );
        let string = field(FieldKind::String, Vec::new(), None);
        assert_eq!(
            edit_form_value(&string, None, FormValueEdit::Character('x')),
            Some(TypedValue::String("x".to_owned()))
        );
        let integer = field(FieldKind::Integer, Vec::new(), None);
        assert_eq!(
            edit_form_value(&integer, None, FormValueEdit::Character('4')),
            Some(TypedValue::Integer(4))
        );
        let finite = field(FieldKind::FiniteNumber, Vec::new(), None);
        let current = TypedValue::Decimal(decimal("1.2"));
        assert_eq!(
            edit_form_value(&finite, Some(&current), FormValueEdit::Character('5')),
            Some(TypedValue::Decimal(decimal("1.25")))
        );
    }

    #[test]
    fn explicit_edits_construct_enum_and_path_values() {
        let enumeration = field(
            FieldKind::Enum,
            vec![
                Scalar::Text("red".to_owned()),
                Scalar::Text("blue".to_owned()),
            ],
            None,
        );
        assert_eq!(
            edit_form_value(&enumeration, None, FormValueEdit::NextChoice),
            Some(TypedValue::String("red".to_owned()))
        );
        assert_eq!(
            edit_form_value(
                &enumeration,
                Some(&TypedValue::String("red".to_owned())),
                FormValueEdit::NextChoice,
            ),
            Some(TypedValue::String("blue".to_owned()))
        );
        let path = field(FieldKind::Path, Vec::new(), None);
        assert_eq!(
            edit_form_value(&path, None, FormValueEdit::Character('/')),
            Some(TypedValue::String("/".to_owned()))
        );
    }

    #[test]
    fn explicit_edits_construct_list_and_secret_reference_values() {
        let string_list = field(FieldKind::StringList, Vec::new(), None);
        let first = edited(&string_list, None, FormValueEdit::Character('a'));
        let next = edited(&string_list, Some(&first), FormValueEdit::Character(','));
        assert_eq!(
            edit_form_value(&string_list, Some(&next), FormValueEdit::Character('b')),
            Some(TypedValue::List(vec![
                TypedValue::String("a".to_owned()),
                TypedValue::String("b".to_owned()),
            ]))
        );
        let secret = field(FieldKind::SecretReference, Vec::new(), None);
        let first = edited(&secret, None, FormValueEdit::Character('A'));
        assert_eq!(
            edit_form_value(&secret, Some(&first), FormValueEdit::Character('1')),
            Some(TypedValue::SecretRef(SecretRef {
                env: secret_reference("A1"),
            }))
        );
    }

    #[test]
    fn key_sequences_retain_only_syntactically_valid_intermediate_drafts() {
        let integer = field(FieldKind::Integer, Vec::new(), None);
        let sign = edited(&integer, None, FormValueEdit::Character('-'));
        assert_eq!(sign, TypedValue::String("-".to_owned()));
        assert_eq!(
            edit_form_value(&integer, Some(&sign), FormValueEdit::Character('4')),
            Some(TypedValue::Integer(-4))
        );
        assert_eq!(
            edit_form_value(&integer, None, FormValueEdit::Character('x')),
            None
        );

        let finite = field(FieldKind::FiniteNumber, Vec::new(), None);
        let one = edited(&finite, None, FormValueEdit::Character('1'));
        let decimal_point = edited(&finite, Some(&one), FormValueEdit::Character('.'));
        assert_eq!(decimal_point, TypedValue::String("1.".to_owned()));
        assert_eq!(
            edit_form_value(&finite, Some(&decimal_point), FormValueEdit::Character('2'),),
            Some(TypedValue::Decimal(decimal("1.2")))
        );
    }

    #[test]
    fn constraint_incomplete_values_remain_editable_but_are_not_complete() {
        let bounded = field(FieldKind::String, Vec::new(), Some(Scalar::Integer(3)));
        let first = edited(&bounded, None, FormValueEdit::Character('x'));
        assert_eq!(first, TypedValue::String("x".to_owned()));
        assert!(!super::form_value_is_complete(&bounded, &first));
        let second = edited(&bounded, Some(&first), FormValueEdit::Character('y'));
        let complete = edited(&bounded, Some(&second), FormValueEdit::Character('z'));
        assert!(super::form_value_is_complete(&bounded, &complete));
    }

    #[test]
    fn enum_list_and_secret_edits_wrap_remove_and_reject_invalid_syntax() {
        let enumeration = field(
            FieldKind::Enum,
            vec![
                Scalar::Text("red".to_owned()),
                Scalar::Text("blue".to_owned()),
            ],
            None,
        );
        assert_eq!(
            edit_form_value(
                &enumeration,
                Some(&TypedValue::String("blue".to_owned())),
                FormValueEdit::NextChoice,
            ),
            Some(TypedValue::String("red".to_owned()))
        );

        let list = field(FieldKind::StringList, Vec::new(), None);
        let item = edited(&list, None, FormValueEdit::Character('a'));
        let separator = edited(&list, Some(&item), FormValueEdit::Character(','));
        assert_eq!(
            edit_form_value(&list, Some(&separator), FormValueEdit::Backspace),
            Some(item)
        );

        let secret = field(FieldKind::SecretReference, Vec::new(), None);
        assert_eq!(
            edit_form_value(&secret, None, FormValueEdit::Character('a')),
            None
        );
    }
}
