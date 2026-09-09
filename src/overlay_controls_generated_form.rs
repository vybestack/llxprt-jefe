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
    let mut rows = Vec::new();
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
    let mut rows = Vec::new();
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
/// focused declaration id. The rows exclude the section header; assembly adds
/// it so the viewport window can weigh it separately (issue #719).
struct LoweredFields {
    rows: Vec<HostControlRow>,
    fields: Vec<Field>,
    values: TypedMap,
    focus_target: Option<Id>,
}

fn field_rows(form: &GeneratedAgentForm, width: usize) -> LoweredFields {
    let mut rows = Vec::new();
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
            // An unset optional boolean carries no entry: the lowered field
            // kind is Boolean, so an empty String would lie about the type
            // (issue #706).
            if let Some(value) = typed_value(field.value()) {
                values.insert(declaration.id().clone(), value);
            }
            if focused {
                focus_target = Some(declaration.id().clone());
            }
        }
        rows.push(HostControlRow::new(
            truncate_with_ellipsis(&text, width),
            lowered
                .as_ref()
                .map(|declaration| PanelHitTarget::Field(declaration.id().clone())),
        ));
        fields.extend(lowered);
    }
    LoweredFields {
        rows,
        fields,
        values,
        focus_target,
    }
}

/// Rows the shared shell reserves for the create/back affordance pair.
const ACTION_ROW_COUNT: usize = 2;

/// Decoration rows of the unclipped assembly: the display name, the three
/// section headers, and the blank separator. The clipped branch must reserve
/// exactly this many non-content rows; [`windowed_rows_keeps_the_floor_at_the_decoration_count`]
/// pins the two branches together.
const DECORATION_ROWS: usize = 5;

/// Visible rows the shared shell's viewport offers on the committed frame.
///
/// The shell windows every projection through `HostOverlayLayout::form`; the
/// committed layout is the exact display basis the renderer drew, so the
/// projection reads its window from there instead of shipping rows that
/// cannot fit (issue #719). No committed frame means no window is knowable,
/// so the whole form projects (frame-less callers and unit tests).
fn committed_viewport_rows(state: &AppState) -> Option<usize> {
    let layout = state.resolved_layout.as_ref()?;
    let (cols, rows) = crate::screen_layout::committed_render_size(layout);
    Some(crate::overlay_controls::HostOverlayLayout::form(cols, rows).viewport_rows)
}

/// Assemble the projected rows, honoring the committed viewport window.
///
/// The #382-era renderer reserved the trailing create/back rows and clipped
/// the body; the shared shell adds title and footer rows inside the same box,
/// so the window is tighter and what yields first matters. Section headers
/// and the blank separator are decoration and go first, support rows
/// (operations/targets) go next, tailmost field rows after that, and the
/// display name and action rows always render (issue #719). Focus keeps
/// walking fields the window hides through state, exactly as it walked the
/// legacy renderer's clipped fields. Below the support-plus-action floor the
/// window still wins: support rows drop before the reserved action rows do,
/// so the widest pane that can show anything keeps the affordances.
fn windowed_rows(
    name: HostControlRow,
    operations: Vec<HostControlRow>,
    targets: Vec<HostControlRow>,
    fields: Vec<HostControlRow>,
    actions: Vec<HostControlRow>,
    viewport_rows: Option<usize>,
) -> Vec<HostControlRow> {
    if let Some(viewport_rows) = viewport_rows
        && DECORATION_ROWS + operations.len() + targets.len() + fields.len() + actions.len()
            > viewport_rows
    {
        let mut rows = Vec::with_capacity(viewport_rows);
        rows.push(name);
        let support_room = viewport_rows
            .saturating_sub(rows.len() + ACTION_ROW_COUNT)
            .min(operations.len() + targets.len());
        let operations_room = support_room.min(operations.len());
        rows.extend(operations.into_iter().take(operations_room));
        rows.extend(
            targets
                .into_iter()
                .take(support_room.saturating_sub(operations_room)),
        );
        let room = viewport_rows
            .saturating_sub(rows.len() + ACTION_ROW_COUNT)
            .min(fields.len());
        rows.extend(fields.into_iter().take(room));
        rows.extend(actions);
        return rows;
    }
    let mut rows = vec![name];
    rows.push(HostControlRow::plain("Operations".to_owned()));
    rows.extend(operations);
    rows.push(HostControlRow::plain("Targets".to_owned()));
    rows.extend(targets);
    rows.push(HostControlRow::plain("Fields".to_owned()));
    rows.extend(fields);
    rows.push(HostControlRow::plain(String::new()));
    rows.extend(actions);
    rows
}

fn affordance_rows(form: &GeneratedAgentForm, create_enabled: bool) -> Vec<HostControlRow> {
    let create_focused = form.focus() == &GeneratedAgentFormFocus::Create;
    let back_focused = form.focus() == &GeneratedAgentFormFocus::Back;
    vec![
        HostControlRow::targeted(
            format!(
                "{}[Create {}]",
                marker(create_focused),
                if create_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
            PanelHitTarget::Submit,
        ),
        HostControlRow::plain(format!("{}[Back]", marker(back_focused))),
    ]
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
    let name = HostControlRow::plain(truncate_with_ellipsis(form.draft().display_name(), width));
    let rows = windowed_rows(
        name,
        operation_rows(form),
        target_rows(form),
        lowered.rows,
        affordance_rows(form, create_enabled),
        committed_viewport_rows(state),
    );
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

#[cfg(test)]
mod values_tests {
    use super::*;

    /// Issue #706: an unset optional boolean must carry no `values` entry.
    /// The lowered field kind is Boolean, so the old fallback inserted an
    /// empty String that lied about the field's type.
    #[test]
    fn unset_optional_boolean_carries_no_values_entry() {
        let definition = crate::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.code-puppy")
            .unwrap_or_else(|| panic!("the code-puppy definition must be shipped"));
        let availability = crate::domain::agent_definition::Availability::InstalledCompatible {
            identity: "0.10.0".to_owned(),
            generation: 1,
        };
        let form = GeneratedAgentForm::from_definition(&definition, &availability)
            .unwrap_or_else(|error| panic!("the fixture definition must build a form: {error}"));

        let unset: Vec<_> = form
            .draft()
            .fields()
            .iter()
            .filter(|field| matches!(field.value(), FieldValue::OptionalBoolean(None)))
            .collect();
        assert!(
            !unset.is_empty(),
            "the fixture must include an unset optional boolean"
        );

        let lowered = field_rows(&form, 100);
        for field in unset {
            let Some(declaration) = lowered_field(field) else {
                continue;
            };
            assert!(
                !lowered.values.contains_key(declaration.id()),
                "unset optional boolean {:?} must carry no values entry",
                declaration.id()
            );
        }
    }
}

#[cfg(test)]
mod windowed_rows_tests {
    use super::*;

    fn row(text: &str) -> HostControlRow {
        HostControlRow::plain(text.to_owned())
    }

    fn parts(
        operations: usize,
        targets: usize,
        fields: usize,
    ) -> (
        HostControlRow,
        Vec<HostControlRow>,
        Vec<HostControlRow>,
        Vec<HostControlRow>,
        Vec<HostControlRow>,
    ) {
        let name = row("name");
        let operations = (0..operations).map(|i| row(&format!("op{i}"))).collect();
        let targets = (0..targets).map(|i| row(&format!("target{i}"))).collect();
        let fields = (0..fields).map(|i| row(&format!("field{i}"))).collect();
        let actions = vec![row("[Create disabled]"), row("[Back]")];
        (name, operations, targets, fields, actions)
    }

    /// The unclipped assembly must reserve exactly `DECORATION_ROWS` rows for
    /// decoration, or the clipped branch reserves the wrong floor.
    #[test]
    fn windowed_rows_keeps_the_floor_at_the_decoration_count() {
        let (name, operations, targets, fields, actions) = parts(3, 2, 4);
        let rows = windowed_rows(name, operations, targets, fields, actions, None);
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        let content = 3 + 2 + 4 + ACTION_ROW_COUNT;
        assert_eq!(
            rows.len(),
            DECORATION_ROWS + content,
            "unclipped rows must equal decoration plus content: {texts:?}"
        );
        let expected = [vec![
            "name",
            "Operations",
            "op0",
            "op1",
            "op2",
            "Targets",
            "target0",
            "target1",
            "Fields",
            "field0",
            "field1",
            "field2",
            "field3",
            "",
            "[Create disabled]",
            "[Back]",
        ]]
        .concat();
        assert_eq!(texts, expected, "the decoration rows pin the floor count");
    }

    /// Below the support-plus-action floor the window still wins: support
    /// rows drop before the reserved action rows do (issue #719).
    #[test]
    fn windowed_rows_below_the_floor_drops_support_before_actions() {
        let (name, operations, targets, fields, actions) = parts(4, 2, 5);
        let rows = windowed_rows(name, operations, targets, fields, actions, Some(10));
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert!(
            rows.len() <= 10,
            "the window must never be exceeded: {texts:?}"
        );
        assert_eq!(texts.first(), Some(&"name"));
        assert!(
            texts.iter().any(|text| text.contains("[Create")),
            "the create affordance survives below the floor: {texts:?}"
        );
        assert_eq!(
            texts.last(),
            Some(&"[Back]"),
            "the back affordance closes the window: {texts:?}"
        );
        assert_eq!(
            texts
                .iter()
                .filter(|text| text.starts_with("field"))
                .count(),
            1,
            "room left after support goes to fields: {texts:?}"
        );
    }
}
