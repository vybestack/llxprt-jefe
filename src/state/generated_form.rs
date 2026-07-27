//! Definition-driven typed form projection and deterministic reducer.
//!
//! [`GeneratedFormDraft`] is built directly from one validated
//! [`AgentDefinition`] and an immutable [`Availability`] observation. It keeps
//! repository and agent fields in declaration order, evaluates sibling
//! visibility without I/O, preserves hidden values, and exposes only active
//! values for later planning. This module has no UI, runtime, or persistence
//! dependency.

use std::fmt;

use crate::domain::agent_definition::{
    AgentDefinition, AgentTypeId, Availability, DefinitionError, Field, FieldKind, FieldScope,
    FieldValue, ProbeErrorCode,
};

use super::generated_form_projection::{
    append_fields, field_is_visible, to_field_value, value_cursor, value_is_empty,
};

/// Scope-qualified identifier for one generated form field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldId {
    scope: FieldScope,
    id: String,
}

impl FormFieldId {
    /// Build a repository-scope field identifier.
    #[must_use]
    pub fn repository(id: impl Into<String>) -> Self {
        Self {
            scope: FieldScope::Repository,
            id: id.into(),
        }
    }

    /// Build an agent-scope field identifier.
    #[must_use]
    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            scope: FieldScope::Agent,
            id: id.into(),
        }
    }

    /// Field scope.
    #[must_use]
    pub const fn scope(&self) -> FieldScope {
        self.scope
    }

    /// Definition-authored field identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

/// Typed reason that a capability-backed field cannot be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldDisabledReason {
    /// No executable candidate resolved for the definition.
    NotFound {
        /// Capability represented by the field.
        capability: String,
    },
    /// The installed executable is incompatible.
    InstalledIncompatible {
        /// Capability represented by the field.
        capability: String,
        /// Exact probe-provided incompatibility reason.
        reason: String,
    },
    /// The capability probe failed.
    ProbeError {
        /// Capability represented by the field.
        capability: String,
        /// Closed probe diagnostic code.
        code: ProbeErrorCode,
        /// Exact probe-provided error reason.
        reason: String,
    },
    /// A successful probe did not find this optional authored capability.
    MissingCapability {
        /// Capability represented by the field.
        capability: String,
    },
}

/// One field projected from a definition into a typed form draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFormField {
    pub(super) id: FormFieldId,
    pub(super) label: String,
    pub(super) definition: Field,
    pub(super) value: FieldValue,
    pub(super) cursor: usize,
    pub(super) visible: bool,
    pub(super) capability: Option<String>,
    pub(super) disabled_reason: Option<FormFieldDisabledReason>,
}

impl GeneratedFormField {
    /// Scope-qualified field identifier.
    #[must_use]
    pub const fn id(&self) -> &FormFieldId {
        &self.id
    }

    /// Human-readable label derived from the stable field identifier.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Closed field kind.
    #[must_use]
    pub const fn kind(&self) -> FieldKind {
        self.definition.kind
    }

    /// Current typed draft value.
    #[must_use]
    pub const fn value(&self) -> &FieldValue {
        &self.value
    }

    /// Character/item cursor associated with this field's typed value.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether this field is required while active.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.definition.required
    }

    /// Inclusive integer minimum.
    #[must_use]
    pub const fn minimum(&self) -> Option<i64> {
        self.definition.minimum
    }

    /// Inclusive integer maximum.
    #[must_use]
    pub const fn maximum(&self) -> Option<i64> {
        self.definition.maximum
    }

    /// Declared enum choices in definition order.
    #[must_use]
    pub fn choices(&self) -> &[String] {
        &self.definition.choices
    }

    /// Whether this field currently participates in the visible form.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Whether this field participates in launch-signature projection.
    #[must_use]
    pub const fn launch_signature(&self) -> bool {
        self.definition.launch_signature
    }

    /// Authored capability represented by this field, when one exists.
    #[must_use]
    pub fn capability(&self) -> Option<&str> {
        self.capability.as_deref()
    }

    /// Typed disabled reason; disabled fields remain present and visible.
    #[must_use]
    pub const fn disabled_reason(&self) -> Option<&FormFieldDisabledReason> {
        self.disabled_reason.as_ref()
    }
}

/// One active typed field value for later launch planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldValue {
    pub(super) id: FormFieldId,
    pub(super) value: FieldValue,
}

impl FormFieldValue {
    /// Scope-qualified field identifier.
    #[must_use]
    pub const fn id(&self) -> &FormFieldId {
        &self.id
    }

    /// Typed field value.
    #[must_use]
    pub const fn value(&self) -> &FieldValue {
        &self.value
    }
}

/// Deterministic intent accepted by the generated form reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormIntent {
    /// Replace one field with a kind-compatible typed draft value.
    SetValue {
        /// Field to edit.
        field: FormFieldId,
        /// New typed value.
        value: FieldValue,
    },
    /// Toggle a Boolean or cycle an OptionalBoolean.
    Toggle {
        /// Field to toggle.
        field: FormFieldId,
    },
    /// Focus one currently visible field.
    Focus(FormFieldId),
    /// Focus the next visible field, wrapping in declaration order.
    FocusNext,
    /// Focus the previous visible field, wrapping in declaration order.
    FocusPrevious,
}

/// Typed generated-form reducer failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormEditError {
    /// The scope-qualified field does not exist in this definition.
    UnknownField {
        /// Unknown identifier.
        field: FormFieldId,
    },
    /// The supplied value does not match the declared kind.
    KindMismatch {
        /// Field being edited.
        field: FormFieldId,
        /// Expected field kind.
        expected: FieldKind,
        /// Supplied value.
        actual: FieldValue,
    },
    /// A disabled capability-backed field was edited.
    DisabledField {
        /// Field being edited.
        field: FormFieldId,
        /// Typed availability reason.
        reason: FormFieldDisabledReason,
    },
    /// The requested field is currently hidden.
    HiddenField {
        /// Hidden field identifier.
        field: FormFieldId,
    },
    /// Toggle was requested for a non-boolean field.
    NotToggleable {
        /// Field being toggled.
        field: FormFieldId,
        /// Declared field kind.
        kind: FieldKind,
    },
}

/// Typed validation problem for one active generated field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormValidationProblem {
    /// A required field is empty.
    Required,
    /// Integer is below its inclusive minimum.
    BelowMinimum {
        /// Inclusive minimum.
        minimum: i64,
        /// Current draft value.
        actual: i64,
    },
    /// Integer is above its inclusive maximum.
    AboveMaximum {
        /// Inclusive maximum.
        maximum: i64,
        /// Current draft value.
        actual: i64,
    },
    /// Enum draft does not match any declared choice.
    InvalidChoice {
        /// Current draft value.
        value: String,
    },
}

/// One typed validation issue associated with a known field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormValidationIssue {
    field: FormFieldId,
    problem: FormValidationProblem,
}

impl FormValidationIssue {
    /// Known field that failed validation.
    #[must_use]
    pub const fn field(&self) -> &FormFieldId {
        &self.field
    }

    /// Typed validation problem.
    #[must_use]
    pub const fn problem(&self) -> &FormValidationProblem {
        &self.problem
    }
}

/// Failure to generate a form from a definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedFormBuildError {
    /// Definition failed its closed-schema validation.
    InvalidDefinition(DefinitionError),
}

impl fmt::Display for GeneratedFormBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition(error) => write!(formatter, "cannot generate form: {error}"),
        }
    }
}

impl std::error::Error for GeneratedFormBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidDefinition(error) => Some(error),
        }
    }
}

/// One definition-driven typed form draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFormDraft {
    type_id: AgentTypeId,
    display_name: String,
    fields: Vec<GeneratedFormField>,
    focused: Option<FormFieldId>,
}

impl GeneratedFormDraft {
    /// Generate a typed form from a validated definition and availability.
    ///
    /// Repository fields precede agent fields, with declaration order retained
    /// inside each scope. Defaults are copied exactly; absent defaults receive
    /// only their kind's empty draft value.
    pub fn from_definition(
        definition: &AgentDefinition,
        availability: &Availability,
    ) -> Result<Self, GeneratedFormBuildError> {
        definition
            .validate()
            .map_err(GeneratedFormBuildError::InvalidDefinition)?;
        let mut fields =
            Vec::with_capacity(definition.repository_fields.len() + definition.agent_fields.len());
        append_fields(
            &mut fields,
            FieldScope::Repository,
            &definition.repository_fields,
            definition,
            availability,
        );
        append_fields(
            &mut fields,
            FieldScope::Agent,
            &definition.agent_fields,
            definition,
            availability,
        );
        let mut draft = Self {
            type_id: definition.id.clone(),
            display_name: definition.display_name.clone(),
            fields,
            focused: None,
        };
        draft.refresh_projection();
        Ok(draft)
    }

    /// Stable definition id.
    #[must_use]
    pub const fn type_id(&self) -> &AgentTypeId {
        &self.type_id
    }

    /// Definition display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// All generated fields, including hidden and disabled fields.
    #[must_use]
    pub fn fields(&self) -> &[GeneratedFormField] {
        &self.fields
    }

    /// Find one known field. Unknown IDs return `None`; they are never defaulted.
    #[must_use]
    pub fn field(&self, id: &FormFieldId) -> Option<&GeneratedFormField> {
        self.fields.iter().find(|field| field.id == *id)
    }

    /// Current focus, if any visible field exists.
    #[must_use]
    pub const fn focused(&self) -> Option<&FormFieldId> {
        self.focused.as_ref()
    }

    /// Visible field identifiers in deterministic declaration order.
    #[must_use]
    pub fn visible_field_ids(&self) -> Vec<FormFieldId> {
        self.fields
            .iter()
            .filter(|field| field.visible)
            .map(|field| field.id.clone())
            .collect()
    }

    /// Apply one deterministic, I/O-free state transition.
    pub fn reduce(mut self, intent: FormIntent) -> Result<Self, FormEditError> {
        match intent {
            FormIntent::SetValue { field, value } => self.set_value(&field, value)?,
            FormIntent::Toggle { field } => self.toggle(&field)?,
            FormIntent::Focus(field) => self.set_focus(&field)?,
            FormIntent::FocusNext => self.move_focus(true),
            FormIntent::FocusPrevious => self.move_focus(false),
        }
        self.refresh_projection();
        Ok(self)
    }

    /// Validate every visible, enabled field in declaration order.
    #[must_use]
    pub fn validation_issues(&self) -> Vec<FormValidationIssue> {
        let mut issues = Vec::new();
        for field in self
            .fields
            .iter()
            .filter(|field| field.visible && field.disabled_reason.is_none())
        {
            validate_field(field, &mut issues);
        }
        issues
    }

    /// Project visible, enabled values in declaration order for later planning.
    #[must_use]
    pub fn active_values(&self) -> Vec<FormFieldValue> {
        self.fields
            .iter()
            .filter(|field| field.visible && field.disabled_reason.is_none())
            .map(to_field_value)
            .collect()
    }

    /// Project active launch-signature values in declaration order.
    #[must_use]
    pub fn launch_signature_values(&self) -> Vec<FormFieldValue> {
        self.fields
            .iter()
            .filter(|field| {
                field.visible
                    && field.disabled_reason.is_none()
                    && field.definition.launch_signature
            })
            .map(to_field_value)
            .collect()
    }

    fn set_value(&mut self, id: &FormFieldId, value: FieldValue) -> Result<(), FormEditError> {
        let field = self.field_mut(id)?;
        reject_disabled(field)?;
        if !value.matches_kind(field.definition.kind) {
            return Err(FormEditError::KindMismatch {
                field: id.clone(),
                expected: field.definition.kind,
                actual: value,
            });
        }
        field.cursor = value_cursor(&value);
        field.value = value;
        Ok(())
    }

    fn toggle(&mut self, id: &FormFieldId) -> Result<(), FormEditError> {
        let field = self.field_mut(id)?;
        reject_disabled(field)?;
        field.value = match field.value {
            FieldValue::Boolean(value) => FieldValue::Boolean(!value),
            FieldValue::OptionalBoolean(value) => FieldValue::OptionalBoolean(match value {
                None => Some(true),
                Some(true) => Some(false),
                Some(false) => None,
            }),
            _ => {
                return Err(FormEditError::NotToggleable {
                    field: id.clone(),
                    kind: field.definition.kind,
                });
            }
        };
        Ok(())
    }

    fn set_focus(&mut self, id: &FormFieldId) -> Result<(), FormEditError> {
        let field = self
            .field(id)
            .ok_or_else(|| FormEditError::UnknownField { field: id.clone() })?;
        if !field.visible {
            return Err(FormEditError::HiddenField { field: id.clone() });
        }
        self.focused = Some(id.clone());
        Ok(())
    }

    fn move_focus(&mut self, forward: bool) {
        let visible = self.visible_field_ids();
        if visible.is_empty() {
            self.focused = None;
            return;
        }
        let current = self
            .focused
            .as_ref()
            .and_then(|focused| visible.iter().position(|id| id == focused));
        let index = match (current, forward) {
            (Some(index), true) => (index + 1) % visible.len(),
            (Some(0) | None, false) => visible.len() - 1,
            (Some(index), false) => index - 1,
            (None, true) => 0,
        };
        self.focused = visible.get(index).cloned();
    }

    fn field_mut(&mut self, id: &FormFieldId) -> Result<&mut GeneratedFormField, FormEditError> {
        self.fields
            .iter_mut()
            .find(|field| field.id == *id)
            .ok_or_else(|| FormEditError::UnknownField { field: id.clone() })
    }

    fn refresh_projection(&mut self) {
        for index in 0..self.fields.len() {
            self.fields[index].visible = field_is_visible(&self.fields, index);
        }
        if !self.focused.as_ref().is_some_and(|focused| {
            self.fields
                .iter()
                .any(|field| field.visible && field.id == *focused)
        }) {
            self.focused = self
                .fields
                .iter()
                .find(|field| field.visible)
                .map(|field| field.id.clone());
        }
    }
}

fn validate_field(field: &GeneratedFormField, issues: &mut Vec<FormValidationIssue>) {
    if field.definition.required && value_is_empty(&field.value) {
        push_issue(field, FormValidationProblem::Required, issues);
    }
    if let FieldValue::Integer(actual) = field.value {
        if let Some(minimum) = field.definition.minimum
            && actual < minimum
        {
            push_issue(
                field,
                FormValidationProblem::BelowMinimum { minimum, actual },
                issues,
            );
        }
        if let Some(maximum) = field.definition.maximum
            && actual > maximum
        {
            push_issue(
                field,
                FormValidationProblem::AboveMaximum { maximum, actual },
                issues,
            );
        }
    }
    if field.definition.kind == FieldKind::Enum
        && let FieldValue::String(value) = &field.value
        && !value.is_empty()
        && !field
            .definition
            .choices
            .iter()
            .any(|choice| choice == value)
    {
        push_issue(
            field,
            FormValidationProblem::InvalidChoice {
                value: value.clone(),
            },
            issues,
        );
    }
}

fn push_issue(
    field: &GeneratedFormField,
    problem: FormValidationProblem,
    issues: &mut Vec<FormValidationIssue>,
) {
    issues.push(FormValidationIssue {
        field: field.id.clone(),
        problem,
    });
}

fn reject_disabled(field: &GeneratedFormField) -> Result<(), FormEditError> {
    if let Some(reason) = &field.disabled_reason {
        return Err(FormEditError::DisabledField {
            field: field.id.clone(),
            reason: reason.clone(),
        });
    }
    Ok(())
}
