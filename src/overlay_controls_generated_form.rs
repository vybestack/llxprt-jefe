//! Shared-shell Form projection for the definition-generated New Agent form.
//!
//! The legacy thin renderer drew `content_lines` directly; this projection
//! lowers the same definition-driven draft (sections, operation/target
//! support rows, typed fields, create/back affordances) into the one overlay
//! control runtime the workbench cutover keeps.

use crate::domain::action_registry::{ActionId, InternalActionId};
use crate::domain::agent_definition::{FieldKind, FieldScope, FieldValue, Operation, Support};
use crate::domain::plugin::field::{Field, FieldDraft, RestartScope};
use crate::domain::{Id, InternalId, TypedValue};
use crate::host_controls::{HostControlRow, PanelHitTarget};
use crate::runtime::provider::protocol::{Affordance, TypedMap};
use crate::state::generated_agent_form::{
    GeneratedAgentForm, GeneratedAgentFormFocus, GeneratedTarget,
};
use crate::state::generated_form::GeneratedFormField;
use crate::state::{AppState, ModalState};
use crate::ui::util::{text_with_caret, truncate_with_ellipsis};

use crate::overlay_controls::{OverlayControlProjection, bespoke_form_projection};

pub const GENERATED_FORM_FOOTER: &str =
    "Tab/Down next | Shift+Tab/Up prev | Enter choose | Esc/q Back";

const OPERATIONS: [Operation; 4] = [
    Operation::Normal,
    Operation::Resume,
    Operation::FreshIssue,
    Operation::FreshPullRequest,
];
const TARGETS: [GeneratedTarget; 2] = [GeneratedTarget::Local, GeneratedTarget::Remote];

fn operation_label(operation: Operation) -> &'static str {
    match operation {
        Operation::Normal => "Normal",
        Operation::Resume => "Resume",
        Operation::FreshIssue => "Fresh Issue",
        Operation::FreshPullRequest => "Fresh PR",
    }
}

fn target_label(target: GeneratedTarget) -> &'static str {
    match target {
        GeneratedTarget::Local => "Local",
        GeneratedTarget::Remote => "Remote",
    }
}

fn marker(focused: bool) -> &'static str {
    if focused { "> " } else { "" }
}

fn support_text(focused: bool, label: &str, support: &Support) -> String {
    match support {
        Support::Supported => format!("{}{label}: Supported", marker(focused)),
        Support::Unsupported { reason } => {
            format!("{}{label}: Unsupported: {reason}", marker(focused))
        }
    }
}

/// Bracketed display value mirroring the legacy content projection, with the
/// text caret applied only to the focused text field.
fn field_value_text(field: &GeneratedFormField, focused: bool) -> String {
    match field.value() {
        FieldValue::Boolean(value) => checkbox(*value),
        FieldValue::OptionalBoolean(value) => match value {
            Some(value) => checkbox(*value),
            None => "[unset]".to_owned(),
        },
        FieldValue::String(value) | FieldValue::Path(value) => {
            if focused {
                format!("[{}]", text_with_caret(value, field.cursor()))
            } else {
                format!("[{value}]")
            }
        }
        FieldValue::Integer(value) => format!("[{value}]"),
        FieldValue::StringList(values) => format!("[{}]", values.join(", ")),
    }
}

fn checkbox(value: bool) -> String {
    format!("[{}]", if value { "x" } else { " " })
}

/// Typed value for the shared control contract; unset optional booleans carry
/// no entry until they are set.
fn typed_value(value: &FieldValue) -> Option<TypedValue> {
    match value {
        FieldValue::Boolean(value) => Some(TypedValue::Bool(*value)),
        FieldValue::OptionalBoolean(value) => value.map(TypedValue::Bool),
        FieldValue::String(value) | FieldValue::Path(value) => {
            Some(TypedValue::String(value.clone()))
        }
        FieldValue::Integer(value) => Some(TypedValue::Integer(*value)),
        FieldValue::StringList(values) => Some(TypedValue::List(
            values
                .iter()
                .map(|value| TypedValue::String(value.clone()))
                .collect(),
        )),
    }
}

/// Scope-qualified field identifier so repository and agent fields cannot
/// collide inside one control body.
fn qualified_field_id(field: &GeneratedFormField) -> String {
    let scope = match field.id().scope() {
        FieldScope::Repository => "repository",
        FieldScope::Agent => "agent",
    };
    format!("{scope}.{}", field.id().as_str())
}

/// Lower one generated field onto a closed control-body declaration.
/// Definition-sourced data shapes this lowering, so unparseable declarations
/// degrade to an untargeted row instead of failing the whole projection.
fn lowered_field(field: &GeneratedFormField) -> Option<Field> {
    let kind = match field.kind() {
        FieldKind::Boolean | FieldKind::OptionalBoolean => {
            crate::domain::plugin::field::FieldKind::Boolean
        }
        FieldKind::String | FieldKind::Enum | FieldKind::Path => {
            crate::domain::plugin::field::FieldKind::String
        }
        FieldKind::Integer => crate::domain::plugin::field::FieldKind::Integer,
        FieldKind::StringList => crate::domain::plugin::field::FieldKind::StringList,
    };
    Field::parse(FieldDraft {
        id: Id::parse(&qualified_field_id(field)).ok()?,
        label: field.label().to_owned(),
        description: None,
        kind,
        required: field.required(),
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .ok()
}

fn operation_rows(form: &GeneratedAgentForm) -> Vec<HostControlRow> {
    let mut rows = vec![HostControlRow::plain("Operations".to_owned())];
    for operation in OPERATIONS {
        let focused = form.focus() == &GeneratedAgentFormFocus::Operation(operation);
        rows.push(HostControlRow::plain(support_text(
            focused,
            operation_label(operation),
            form.operation_support(operation),
        )));
    }
    rows
}

fn target_rows(form: &GeneratedAgentForm) -> Vec<HostControlRow> {
    let mut rows = vec![HostControlRow::plain("Targets".to_owned())];
    for target in TARGETS {
        let focused = form.focus() == &GeneratedAgentFormFocus::Target(target);
        rows.push(HostControlRow::plain(support_text(
            focused,
            target_label(target),
            form.target_support(target),
        )));
    }
    rows
}

/// Visible field rows plus their lowered declarations, typed values, and the
/// focused declaration id.
struct LoweredFields {
    rows: Vec<HostControlRow>,
    fields: Vec<Field>,
    values: TypedMap,
    focus_target: Option<Id>,
}

fn field_rows(form: &GeneratedAgentForm, width: usize) -> LoweredFields {
    let mut rows = vec![HostControlRow::plain("Fields".to_owned())];
    let mut fields = Vec::new();
    let mut values = TypedMap::new();
    let mut focus_target = None;
    for field in form.draft().fields().iter().filter(|field| field.visible()) {
        let focused =
            matches!(form.focus(), GeneratedAgentFormFocus::Field(id) if id == field.id());
        let text = format!(
            "{}{}: {}",
            marker(focused),
            field.label(),
            field_value_text(field, focused)
        );
        let lowered = lowered_field(field);
        if let Some(declaration) = &lowered {
            values.insert(
                declaration.id().clone(),
                typed_value(field.value()).unwrap_or(TypedValue::String(String::new())),
            );
            if focused {
                focus_target = Some(declaration.id().clone());
            }
        }
        rows.push(HostControlRow {
            text: truncate_with_ellipsis(&text, width),
            target: lowered
                .as_ref()
                .map(|declaration| PanelHitTarget::Field(declaration.id().clone())),
        });
        fields.extend(lowered);
    }
    LoweredFields {
        rows,
        fields,
        values,
        focus_target,
    }
}

/// Project the open definition-generated New Agent form as a shared-shell
/// form control, mirroring the legacy thin renderer's content lines.
#[must_use]
pub fn project_generated_agent_form(
    state: &AppState,
    width: usize,
) -> Option<OverlayControlProjection> {
    let ModalState::GeneratedAgent { form, .. } = &state.modal else {
        return None;
    };
    let lowered = field_rows(form, width);
    let create_enabled = form.create_enabled();
    let mut rows = vec![HostControlRow::plain(truncate_with_ellipsis(
        form.draft().display_name(),
        width,
    ))];
    rows.extend(operation_rows(form));
    rows.extend(target_rows(form));
    rows.extend(lowered.rows);
    rows.push(HostControlRow::plain(String::new()));
    let create_focused = form.focus() == &GeneratedAgentFormFocus::Create;
    rows.push(HostControlRow {
        text: format!(
            "{}[Create {}]",
            marker(create_focused),
            if create_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ),
        target: Some(PanelHitTarget::Submit),
    });
    let back_focused = form.focus() == &GeneratedAgentFormFocus::Back;
    rows.push(HostControlRow::plain(format!(
        "{}[Back]",
        marker(back_focused)
    )));
    let affordances = vec![Affordance {
        id: Id::internal(InternalId::OverlaySubmit),
        label: "Create".to_owned(),
        action_id: ActionId::internal(InternalActionId::OverlaySubmit),
        arguments: None,
        enabled: create_enabled,
        unavailable_reason: None,
    }];
    Some(bespoke_form_projection(
        "New Agent",
        rows,
        lowered.fields,
        lowered.values,
        affordances,
        lowered.focus_target,
    ))
}
