//! Pure projection helpers for the definition-driven generated form.
//!
//! This module owns construction metadata, capability availability, visibility,
//! display labels, cursors, and active-value projection. It is I/O-free and is
//! consumed only by [`super::generated_form`].

use crate::domain::agent_definition::{
    AgentDefinition, Availability, Field, FieldKind, FieldScope, FieldValue,
};

use super::generated_form::{
    FormFieldDisabledReason, FormFieldId, FormFieldValue, GeneratedFormField,
};

pub(super) fn append_fields(
    output: &mut Vec<GeneratedFormField>,
    scope: FieldScope,
    fields: &[Field],
    definition: &AgentDefinition,
    availability: &Availability,
) {
    for field in fields {
        let capability = capability_for(field, definition);
        output.push(GeneratedFormField {
            id: match scope {
                FieldScope::Repository => FormFieldId::repository(&field.id),
                FieldScope::Agent => FormFieldId::agent(&field.id),
            },
            label: display_label(&field.id),
            value: field
                .default
                .clone()
                .unwrap_or_else(|| empty_value(field.kind)),
            cursor: field.default.as_ref().map_or(0, value_cursor),
            visible: true,
            disabled_reason: capability
                .as_deref()
                .and_then(|capability| disabled_reason(capability, availability)),
            capability,
            definition: field.clone(),
        });
    }
}

pub(super) fn field_is_visible(fields: &[GeneratedFormField], index: usize) -> bool {
    let field = &fields[index];
    let mut parent_id = field.definition.visible_when.as_deref();
    while let Some(id) = parent_id {
        let parent = fields
            .iter()
            .find(|parent| parent.id.scope() == field.id.scope() && parent.id.as_str() == id);
        let Some(parent) = parent else {
            return false;
        };
        if !value_is_truthy(&parent.value) {
            return false;
        }
        parent_id = parent.definition.visible_when.as_deref();
    }
    true
}

pub(super) fn to_field_value(field: &GeneratedFormField) -> FormFieldValue {
    FormFieldValue {
        id: field.id.clone(),
        value: field.value.clone(),
    }
}

pub(super) fn value_cursor(value: &FieldValue) -> usize {
    match value {
        FieldValue::String(value) | FieldValue::Path(value) => value.chars().count(),
        FieldValue::Integer(value) => value.to_string().chars().count(),
        FieldValue::StringList(values) => values.len(),
        FieldValue::Boolean(_) | FieldValue::OptionalBoolean(_) => 0,
    }
}

pub(super) fn value_is_empty(value: &FieldValue) -> bool {
    match value {
        FieldValue::OptionalBoolean(None) => true,
        FieldValue::String(value) | FieldValue::Path(value) => value.trim().is_empty(),
        FieldValue::StringList(values) => values.is_empty(),
        FieldValue::Boolean(_) | FieldValue::OptionalBoolean(Some(_)) | FieldValue::Integer(_) => {
            false
        }
    }
}

fn empty_value(kind: FieldKind) -> FieldValue {
    match kind {
        FieldKind::Boolean => FieldValue::Boolean(false),
        FieldKind::OptionalBoolean => FieldValue::OptionalBoolean(None),
        FieldKind::String | FieldKind::Enum => FieldValue::String(String::new()),
        FieldKind::Integer => FieldValue::Integer(0),
        FieldKind::Path => FieldValue::Path(String::new()),
        FieldKind::StringList => FieldValue::StringList(Vec::new()),
    }
}

fn capability_for(field: &Field, definition: &AgentDefinition) -> Option<String> {
    let normalized = field.id.replace('_', "-");
    definition
        .probe
        .capabilities
        .as_ref()?
        .tokens
        .iter()
        .find(|token| token.id == field.id || token.id == normalized)
        .map(|token| token.id.clone())
}

fn disabled_reason(
    capability: &str,
    availability: &Availability,
) -> Option<FormFieldDisabledReason> {
    match availability {
        Availability::NotFound => Some(FormFieldDisabledReason::NotFound {
            capability: capability.to_string(),
        }),
        Availability::InstalledCompatible { capabilities, .. } => (!capabilities
            .iter()
            .any(|found| found == capability))
        .then(|| FormFieldDisabledReason::MissingCapability {
            capability: capability.to_string(),
        }),
        Availability::InstalledIncompatible { reason, .. } => {
            Some(FormFieldDisabledReason::InstalledIncompatible {
                capability: capability.to_string(),
                reason: reason.clone(),
            })
        }
        Availability::ProbeError { code, reason, .. } => {
            Some(FormFieldDisabledReason::ProbeError {
                capability: capability.to_string(),
                code: *code,
                reason: reason.clone(),
            })
        }
    }
}

fn value_is_truthy(value: &FieldValue) -> bool {
    match value {
        FieldValue::Boolean(value) | FieldValue::OptionalBoolean(Some(value)) => *value,
        FieldValue::OptionalBoolean(None) => false,
        FieldValue::String(value) | FieldValue::Path(value) => !value.is_empty(),
        FieldValue::Integer(value) => *value != 0,
        FieldValue::StringList(values) => !values.is_empty(),
    }
}

fn display_label(id: &str) -> String {
    let mut label = String::with_capacity(id.len());
    let mut capitalize = true;
    for character in id.chars() {
        if matches!(character, '_' | '-' | '.') {
            label.push(' ');
            capitalize = true;
        } else if capitalize {
            label.extend(character.to_uppercase());
            capitalize = false;
        } else {
            label.push(character);
        }
    }
    label
}
