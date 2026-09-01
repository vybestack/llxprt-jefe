//! RED tests: the definition-generated New Agent form must project through
//! the shared overlay Form control, mirroring the legacy thin renderer's
//! sections, support rows, field rows, caret, create enablement, and typed
//! field edits.

use crate::domain::TypedValue;
use crate::domain::agent_definition::{AgentDefinition, AgentTypeId, Availability, Operation};
use crate::host_controls::{ControlAction, ControlIntent, PanelHitTarget};
use crate::overlay_controls::overlay_intent;
use crate::overlay_controls_generated_form::project_generated_agent_form;
use crate::runtime::provider::protocol::PanelEvent;
use crate::state::generated_agent_form::{
    GeneratedAgentForm, GeneratedAgentFormFocus, GeneratedAgentFormIntent,
};
use crate::state::{AppState, ModalState, PaneFocus};

const WIDTH: usize = 100;

fn llxprt_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"))
}

fn compatible_llxprt() -> Availability {
    Availability::InstalledCompatible {
        identity: "0.10.0".to_owned(),
        generation: 1,
    }
}

fn generated_state() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let definition = llxprt_definition();
    let form = GeneratedAgentForm::from_definition(&definition, &compatible_llxprt())
        .unwrap_or_else(|error| panic!("LLxprt definition must produce a form: {error}"));
    state.modal = ModalState::GeneratedAgent {
        type_id: Box::new(
            AgentTypeId::parse("core.llxprt")
                .unwrap_or_else(|error| panic!("core.llxprt must parse: {error}")),
        ),
        form: Box::new(form),
        return_focus: PaneFocus::Agents,
        return_agent_type_index: 0,
    };
    state
}

fn generated_form(state: &mut AppState) -> &mut GeneratedAgentForm {
    let ModalState::GeneratedAgent { form, .. } = &mut state.modal else {
        panic!("fixture must hold a generated agent form")
    };
    form
}

/// Advance focus until it rests on a generated field row.
fn focus_first_field(state: &mut AppState) {
    let form = generated_form(state);
    for _ in 0..16 {
        if matches!(form.focus(), GeneratedAgentFormFocus::Field(_)) {
            return;
        }
        form.apply(GeneratedAgentFormIntent::Next);
    }
    panic!("focus must reach a generated field within one cycle");
}

#[test]
fn generated_form_projects_sections_support_and_fields() {
    let state = generated_state();
    let projection = project_generated_agent_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("generated form must project"));
    assert_eq!(projection.title, "New Agent");
    let rows = projection.text_rows().collect::<Vec<_>>();
    for expected in [
        "LLxprt",
        "Operations",
        "Normal: Supported",
        "> Resume: Supported",
        "Fresh Issue: Supported",
        "Fresh PR: Supported",
        "Targets",
        "Local: Supported",
        "Remote: Supported",
        "Fields",
    ] {
        assert!(
            rows.iter().any(|row| row.contains(expected)),
            "expected a row containing {expected:?}, rows={rows:?}"
        );
    }
    assert!(
        rows.iter()
            .any(|row| row.starts_with("Profile:") && row.contains('[')),
        "the profile field rides its bracketed value, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.starts_with("Yolo: [")),
        "the yolo boolean rides bracketed checkbox text, rows={rows:?}"
    );
}

#[test]
fn generated_form_marks_the_focused_field_with_a_caret() {
    let mut state = generated_state();
    focus_first_field(&mut state);
    let projection = project_generated_agent_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("generated form must project"));
    assert!(
        projection.focus_target.is_some(),
        "a field in focus carries the focus target"
    );
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains('▏') && row.contains('>')),
        "the focused field row carries the caret and focus marker, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn generated_form_edit_field_yields_a_typed_change() {
    let state = generated_state();
    let projection = project_generated_agent_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("generated form must project"));
    let field_id = projection
        .rows
        .iter()
        .find(|row| row.text.starts_with("Profile:"))
        .and_then(|row| row.target.clone())
        .and_then(|target| match target {
            PanelHitTarget::Field(id) => Some(id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the profile row carries a field hit target"));
    let intent = overlay_intent(
        &projection,
        ControlAction::EditField {
            field_id: field_id.clone(),
            value: TypedValue::String("dev2".to_owned()),
        },
    );
    assert_eq!(
        intent,
        ControlIntent::Event(PanelEvent::FieldChanged {
            field_id,
            value: TypedValue::String("dev2".to_owned()),
        })
    );
}

#[test]
fn generated_form_reflects_create_enablement() {
    let mut state = generated_state();
    let expected = if generated_form(&mut state).create_enabled() {
        "[Create enabled]"
    } else {
        "[Create disabled]"
    };
    let projection = project_generated_agent_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("generated form must project"));
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains(expected) && row.contains("[Create")),
        "the create row mirrors enablement ({expected}), rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
    assert!(
        projection.text_rows().any(|row| row.contains("[Back]")),
        "the back affordance rides the projection, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn generated_form_marks_the_focused_operation_row_and_lists_targets() {
    let mut state = generated_state();
    let form = generated_form(&mut state);
    let selected = form.selected_operation();
    let projection = project_generated_agent_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("generated form must project"));
    let operation_label = match selected {
        Operation::Normal => "Normal",
        Operation::Resume => "Resume",
        Operation::FreshIssue => "Fresh Issue",
        Operation::FreshPullRequest => "Fresh PR",
    };
    let rows = projection.text_rows().collect::<Vec<_>>();
    assert!(
        rows.iter()
            .any(|row| row.starts_with(&format!("> {operation_label}"))),
        "the focused operation is marked, rows={rows:?}"
    );
    for label in ["Local: Supported", "Remote: Supported"] {
        assert!(
            rows.iter().any(|row| row.contains(label)),
            "the {label} target row rides the projection, rows={rows:?}"
        );
    }
}
