//! Agent form (new/edit) projection onto the shared overlay Form control.
//!
//! Mirrors the legacy bespoke renderer: field order, agent-kind visibility,
//! the focused-field caret, the pending error line, and the submit
//! affordance all ride [`OverlayControlProjection`] so the form draws and
//! takes input through the one shared overlay runtime.

use crate::domain::TypedValue;
use crate::domain::plugin::field::{Field, InternalField};
use crate::host_controls::PanelHitTarget;
use crate::overlay_controls::{OverlayControlProjection, prepend_detail_rows, project_form};
use crate::runtime::provider::protocol::TypedMap;
use crate::state::{AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState};

pub const AGENT_FORM_FOOTER: &str = "Tab/Down next | Shift+Tab/Up prev | Left/Right move cursor | Space toggles/cycles checkboxes | Enter submit | Esc cancel";

/// How a field's visibility is decided, matching the legacy renderer's three
/// gates: the shared per-field mask, LLxprt-only checkboxes, and the code
/// puppy booleans that replace them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentFieldGate {
    FieldVisible,
    Llxprt,
    NotLlxprt,
}

/// The agent form's declared fields in legacy render order: identity fields,
/// the runtime selector after Profile, then kind-specific defaults and the
/// sandbox block.
const AGENT_FORM_LAYOUT: [(InternalField, AgentFormFocus, AgentFieldGate); 17] = [
    (
        InternalField::AgentFormShortcut,
        AgentFormFocus::Shortcut,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormName,
        AgentFormFocus::Name,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormDescription,
        AgentFormFocus::Description,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormWorkDir,
        AgentFormFocus::WorkDir,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormProfile,
        AgentFormFocus::Profile,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormAgentType,
        AgentFormFocus::AgentType,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormModel,
        AgentFormFocus::CodePuppyModel,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormVersion,
        AgentFormFocus::CodePuppyVersion,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormYolo,
        AgentFormFocus::CodePuppyYolo,
        AgentFieldGate::NotLlxprt,
    ),
    (
        InternalField::AgentFormQuickResume,
        AgentFormFocus::CodePuppyQuickResume,
        AgentFieldGate::NotLlxprt,
    ),
    (
        InternalField::AgentFormMode,
        AgentFormFocus::Mode,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormLlxprtVersion,
        AgentFormFocus::LlxprtVersion,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormLlxprtDebug,
        AgentFormFocus::LlxprtDebug,
        AgentFieldGate::FieldVisible,
    ),
    (
        InternalField::AgentFormPassContinue,
        AgentFormFocus::PassContinue,
        AgentFieldGate::Llxprt,
    ),
    (
        InternalField::AgentFormSandbox,
        AgentFormFocus::Sandbox,
        AgentFieldGate::Llxprt,
    ),
    (
        InternalField::AgentFormSandboxEngine,
        AgentFormFocus::SandboxEngine,
        AgentFieldGate::Llxprt,
    ),
    (
        InternalField::AgentFormSandboxFlags,
        AgentFormFocus::SandboxFlags,
        AgentFieldGate::Llxprt,
    ),
];

fn agent_field_value(fields: &AgentFormFields, focus: AgentFormFocus) -> TypedValue {
    use AgentFormFocus as F;
    let text = match focus {
        F::Shortcut => return TypedValue::String(shortcut_display(fields.shortcut_slot)),
        F::Name => fields.name.as_str(),
        F::Description => fields.description.as_str(),
        F::WorkDir => fields.work_dir.as_str(),
        F::Profile => fields.profile.as_str(),
        F::AgentType => fields.agent_type_id.as_str(),
        F::CodePuppyModel => fields.code_puppy_model.as_str(),
        F::CodePuppyVersion => fields.code_puppy_version.as_str(),
        F::CodePuppyYolo => return TypedValue::Bool(fields.code_puppy_yolo),
        F::CodePuppyQuickResume => {
            return TypedValue::Bool(fields.code_puppy_quick_resume.enabled());
        }
        F::Mode => fields.mode.as_str(),
        F::LlxprtVersion => fields.llxprt_version.as_str(),
        F::LlxprtDebug => fields.llxprt_debug.as_str(),
        F::PassContinue => return TypedValue::Bool(fields.pass_continue),
        F::Sandbox => return TypedValue::Bool(fields.sandbox_enabled),
        F::SandboxEngine => fields.sandbox_engine.as_str(),
        F::SandboxFlags => fields.sandbox_flags.as_str(),
    };
    TypedValue::String(text.to_owned())
}

/// Raw text and caret offset of an editable text field; `None` for booleans
/// and for the two cycle-only rows (shortcut slot, runtime selector), which
/// never showed a caret in the legacy renderer.
fn agent_field_text(
    fields: &AgentFormFields,
    cursor: &AgentFormCursor,
    focus: AgentFormFocus,
) -> Option<(String, usize)> {
    use AgentFormFocus as F;
    let (text, offset) = match focus {
        F::Name => (&fields.name, cursor.name),
        F::Description => (&fields.description, cursor.description),
        F::WorkDir => (&fields.work_dir, cursor.work_dir),
        F::Profile => (&fields.profile, cursor.profile),
        F::CodePuppyModel => (&fields.code_puppy_model, cursor.code_puppy_model),
        F::CodePuppyVersion => (&fields.code_puppy_version, cursor.code_puppy_version),
        F::Mode => (&fields.mode, cursor.mode),
        F::LlxprtVersion => (&fields.llxprt_version, cursor.llxprt_version),
        F::LlxprtDebug => (&fields.llxprt_debug, cursor.llxprt_debug),
        F::SandboxFlags => (&fields.sandbox_flags, cursor.sandbox_flags),
        F::Shortcut
        | F::AgentType
        | F::CodePuppyYolo
        | F::CodePuppyQuickResume
        | F::PassContinue
        | F::Sandbox
        | F::SandboxEngine => return None,
    };
    Some((text.clone(), offset))
}

fn shortcut_display(slot: Option<u8>) -> String {
    slot.map_or_else(|| "none".to_owned(), |slot| slot.to_string())
}

/// Project the open agent form (new or edit) as a shared-shell form control.
///
/// Visibility mirrors the legacy renderer exactly: the per-field mask for
/// text fields and runtime defaults, `shows_llxprt_fields` polarity for the
/// checkbox block. Per-field dynamic hints (runtime cycles, sandbox engines)
/// do not ride this projection; the footer documents the cycling keys.
#[must_use]
pub fn project_agent_form(state: &AppState, width: usize) -> Option<OverlayControlProjection> {
    let (title, fields, focus, cursor) = match &state.modal {
        ModalState::NewAgent {
            fields,
            focus,
            cursor,
            ..
        } => ("New Agent", fields, focus, cursor),
        ModalState::EditAgent {
            fields,
            focus,
            cursor,
            ..
        } => ("Edit Agent", fields, focus, cursor),
        _ => return None,
    };
    let type_id = crate::state::type_id_from_form_value(&fields.agent_type_id);
    let visibility = crate::state::agent_form_visibility(type_id.as_ref());
    let mut declared = Vec::new();
    let mut values = TypedMap::new();
    let mut focused = None;
    for (field, slot, gate) in AGENT_FORM_LAYOUT {
        let visible = match gate {
            AgentFieldGate::FieldVisible => crate::state::is_field_visible(slot, &visibility),
            AgentFieldGate::Llxprt => visibility.shows_llxprt_fields(),
            AgentFieldGate::NotLlxprt => !visibility.shows_llxprt_fields(),
        };
        if !visible {
            continue;
        }
        let declaration = Field::internal(field);
        values.insert(declaration.id().clone(), agent_field_value(fields, slot));
        if slot == *focus {
            focused = Some((
                declaration.id().clone(),
                declaration.label().to_owned(),
                agent_field_text(fields, cursor, slot),
            ));
        }
        declared.push(declaration);
    }
    let mut projection = project_form(title, declared, values, 0, width);
    if let Some((id, label, Some((text, offset)))) = &focused {
        let caret = crate::ui::util::text_with_caret(text, *offset);
        for row in &mut projection.rows {
            if row.target.as_ref() == Some(&PanelHitTarget::Field(id.clone())) {
                row.text = format!("{label}: {caret}");
            }
        }
    }
    if let Some(error) = state.error_message.as_deref() {
        let error_line = format!("Error: {error}");
        prepend_detail_rows(&mut projection.rows, &error_line, width);
    }
    projection.focus_target = focused.map(|(id, _, _)| id);
    Some(projection)
}
