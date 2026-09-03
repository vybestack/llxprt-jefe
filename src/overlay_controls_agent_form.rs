//! Agent form (new/edit) projection onto the shared overlay Form control.
//!
//! The retained projection builds the old operator-facing row text while the
//! typed form body, field targets, caret, visibility, and submit affordance
//! continue to ride [`OverlayControlProjection`].

use crate::domain::action_registry::{ActionId, InternalActionId};
use crate::domain::plugin::field::{Field, InternalField};
use crate::domain::{InternalId, PlatformCapabilities, TypedValue};
use crate::host_controls::{
    HostControlRow, HostControlRowStyle, HostControlTitleStyle, PanelHitTarget,
};
use crate::list_viewport::fit_text_to_width;
use crate::overlay_controls::{OverlayControlProjection, bespoke_form_projection};
use crate::runtime::provider::protocol::{Affordance, Id, TypedMap};
use crate::state::{AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState};
use unicode_width::UnicodeWidthStr;

pub const AGENT_FORM_FOOTER: &str = "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles/cycles checkboxes  Enter submit  Esc";

const LABEL_WIDTH: usize = 16;

/// How a field's visibility is decided, matching the old renderer's three
/// gates: the shared per-field mask, LLxprt-only checkboxes, and the Code Puppy
/// booleans that replace them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentFieldGate {
    FieldVisible,
    Llxprt,
    NotLlxprt,
}

/// The agent form's declared fields in display order: identity fields, the
/// runtime selector after Profile, then kind-specific defaults and sandbox.
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
/// and the two cycle-only rows, which never show a caret.
fn agent_field_text<'a>(
    fields: &'a AgentFormFields,
    cursor: &AgentFormCursor,
    focus: AgentFormFocus,
) -> Option<(&'a str, usize)> {
    use AgentFormFocus as F;
    match focus {
        F::Name => Some((&fields.name, cursor.name)),
        F::Description => Some((&fields.description, cursor.description)),
        F::WorkDir => Some((&fields.work_dir, cursor.work_dir)),
        F::Profile => Some((&fields.profile, cursor.profile)),
        F::CodePuppyModel => Some((&fields.code_puppy_model, cursor.code_puppy_model)),
        F::CodePuppyVersion => Some((&fields.code_puppy_version, cursor.code_puppy_version)),
        F::Mode => Some((&fields.mode, cursor.mode)),
        F::LlxprtVersion => Some((&fields.llxprt_version, cursor.llxprt_version)),
        F::LlxprtDebug => Some((&fields.llxprt_debug, cursor.llxprt_debug)),
        F::SandboxFlags => Some((&fields.sandbox_flags, cursor.sandbox_flags)),
        F::Shortcut
        | F::AgentType
        | F::CodePuppyYolo
        | F::CodePuppyQuickResume
        | F::PassContinue
        | F::Sandbox
        | F::SandboxEngine => None,
    }
}

fn shortcut_display(slot: Option<u8>) -> String {
    slot.map_or_else(|| "none".to_owned(), |slot| slot.to_string())
}

fn display_label(field: InternalField, declared_label: &str) -> &str {
    match field {
        InternalField::AgentFormVersion | InternalField::AgentFormLlxprtVersion => "Version",
        _ => declared_label,
    }
}

fn checkbox(value: bool) -> &'static str {
    if value { "x" } else { " " }
}

fn display_value(
    fields: &AgentFormFields,
    cursor: &AgentFormCursor,
    slot: AgentFormFocus,
    focused: bool,
) -> String {
    use AgentFormFocus as F;
    match slot {
        F::Shortcut => shortcut_display(fields.shortcut_slot),
        F::CodePuppyYolo => checkbox(fields.code_puppy_yolo).to_owned(),
        F::CodePuppyQuickResume => checkbox(fields.code_puppy_quick_resume.enabled()).to_owned(),
        F::PassContinue => checkbox(fields.pass_continue).to_owned(),
        F::Sandbox => checkbox(fields.sandbox_enabled).to_owned(),
        F::AgentType => fields.agent_type_id.clone(),
        F::SandboxEngine => fields.sandbox_engine.clone(),
        _ => {
            let Some((text, offset)) = agent_field_text(fields, cursor, slot) else {
                return String::new();
            };
            if focused {
                crate::ui::util::text_with_caret(text, offset)
            } else {
                text.to_owned()
            }
        }
    }
}

fn engine_hint(fields: &AgentFormFields) -> String {
    if !fields.sandbox_enabled {
        return "disabled".to_owned();
    }
    let labels = PlatformCapabilities::current()
        .supported_engines()
        .iter()
        .map(|engine| engine.label())
        .collect::<Vec<_>>()
        .join(" / ");
    format!("space cycles: {labels}")
}

fn field_hint(state: &AppState, fields: &AgentFormFields, slot: AgentFormFocus) -> Option<String> {
    use AgentFormFocus as F;
    match slot {
        F::AgentType => Some(crate::state::effective_types_hint(
            &effective_kinds_for_form(state),
        )),
        F::CodePuppyYolo | F::CodePuppyQuickResume | F::PassContinue | F::Sandbox => {
            Some("space toggles".to_owned())
        }
        F::SandboxEngine => Some(engine_hint(fields)),
        _ => None,
    }
}

fn field_row(label: &str, value: &str, hint: Option<&str>, width: usize) -> String {
    let prefix = format!("  {label:<LABEL_WIDTH$} [");
    let suffix = hint.map_or_else(|| "]".to_owned(), |hint| format!("]  ({hint})"));
    let fixed_width = UnicodeWidthStr::width(prefix.as_str())
        .saturating_add(UnicodeWidthStr::width(suffix.as_str()));
    let Some(value_width) = width.checked_sub(fixed_width) else {
        return fit_text_to_width(&format!("{prefix}{value}{suffix}"), width);
    };
    format!("{prefix}{}{suffix}", fit_text_to_width(value, value_width))
}

fn effective_kinds_for_form(state: &AppState) -> Vec<crate::domain::agent_definition::AgentTypeId> {
    let is_remote = match &state.modal {
        ModalState::NewAgent { repository_id, .. } => state
            .repository_by_id(repository_id)
            .is_some_and(|repository| repository.remote.enabled),
        ModalState::EditAgent { id, .. } => state
            .repository_for_agent(id)
            .is_some_and(|repository| repository.remote.enabled),
        _ => false,
    };
    crate::state::effective_agent_type_ids(&state.available_agent_type_ids, is_remote)
}

struct AgentFormParts {
    rows: Vec<HostControlRow>,
    declared: Vec<Field>,
    values: TypedMap,
    focus_target: Option<Id>,
}

fn build_agent_form_parts(
    state: &AppState,
    fields: &AgentFormFields,
    focus: AgentFormFocus,
    cursor: &AgentFormCursor,
    width: usize,
) -> AgentFormParts {
    let type_id = crate::state::type_id_from_form_value(&fields.agent_type_id);
    let visibility = crate::state::agent_form_visibility(type_id.as_ref());
    let mut parts = AgentFormParts {
        rows: vec![HostControlRow::plain(String::new())],
        declared: Vec::new(),
        values: TypedMap::new(),
        focus_target: None,
    };

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
        let field_id = declaration.id().clone();
        let focused = slot == focus;
        let value = display_value(fields, cursor, slot, focused);
        let hint = field_hint(state, fields, slot);
        let style = if focused {
            HostControlRowStyle::Bright
        } else {
            HostControlRowStyle::Normal
        };
        parts.rows.push(
            HostControlRow::targeted(
                field_row(
                    display_label(field, declaration.label()),
                    &value,
                    hint.as_deref(),
                    width,
                ),
                PanelHitTarget::Field(field_id.clone()),
            )
            .with_style(style),
        );
        parts
            .values
            .insert(field_id.clone(), agent_field_value(fields, slot));
        if focused {
            parts.focus_target = Some(field_id);
        }
        parts.declared.push(declaration);
    }

    parts
}

fn finish_agent_form(
    title: &str,
    mut parts: AgentFormParts,
    error: Option<&str>,
    width: usize,
) -> OverlayControlProjection {
    parts.rows.push(HostControlRow::targeted(
        String::new(),
        PanelHitTarget::Submit,
    ));
    if let Some(error) = error {
        parts.rows.push(
            HostControlRow::plain(fit_text_to_width(&format!("  Error: {error}"), width))
                .with_style(HostControlRowStyle::Bright),
        );
    }

    let affordances = vec![Affordance {
        id: Id::internal(InternalId::OverlaySubmit),
        label: "Apply".to_owned(),
        action_id: ActionId::internal(InternalActionId::OverlaySubmit),
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    }];
    bespoke_form_projection(
        title,
        parts.rows,
        parts.declared,
        parts.values,
        affordances,
        parts.focus_target,
    )
    .with_title_style(HostControlTitleStyle::Plain)
}

/// Project the open agent form (new or edit) as a shared-shell form control.
///
/// The row stream owns display text only. The typed body, field IDs, visibility,
/// action affordance, and reducer-facing values remain the shared runtime's
/// input and intent contract.
#[must_use]
pub fn project_agent_form(state: &AppState, width: usize) -> Option<OverlayControlProjection> {
    let (title, fields, focus, cursor) = match &state.modal {
        ModalState::NewAgent {
            fields,
            focus,
            cursor,
            ..
        } => (" New Agent", fields, focus, cursor),
        ModalState::EditAgent {
            fields,
            focus,
            cursor,
            ..
        } => (" Edit Agent", fields, focus, cursor),
        _ => return None,
    };
    let parts = build_agent_form_parts(state, fields, *focus, cursor, width);
    Some(finish_agent_form(
        title,
        parts,
        state.error_message.as_deref(),
        width,
    ))
}
