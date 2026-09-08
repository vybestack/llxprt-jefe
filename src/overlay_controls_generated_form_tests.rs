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

/// The shell's drawn window for a projection at the committed frame size.
///
/// `HostControlOverlay` draws `rows.skip(viewport).take(viewport_rows)`; this
/// reproduces that window so the test asserts exactly what renders.
fn shell_window(
    state: &AppState,
    cols: u16,
    rows: u16,
) -> (
    crate::overlay_controls::OverlayControlProjection,
    Vec<String>,
) {
    let layout = crate::overlay_controls::HostOverlayLayout::form(cols, rows);
    let projection = project_generated_agent_form(state, layout.content_width)
        .unwrap_or_else(|| panic!("generated form must project"));
    let visible = projection
        .text_rows()
        .skip(projection.viewport)
        .take(layout.viewport_rows)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (projection, visible)
}

/// Issue #719: at a 54x16 terminal the shared shell's viewport cannot hold
/// the whole form, and the create/back affordance rows were clipped off
/// below `Fields`. The #382-era contract reserves those action rows; the
/// projection must never ship them outside the drawn window.
#[test]
fn generated_form_keeps_affordance_rows_inside_the_small_viewport_window() {
    let mut state = generated_state();
    let resolved = crate::screen_layout::resolve_screen(&state, 54, 16)
        .unwrap_or_else(|| panic!("a 54x16 frame must resolve for the fixture screen"));
    let (cols, rows) = crate::screen_layout::committed_render_size(&resolved);
    state.resolved_layout = Some(resolved);

    let (projection, visible) = shell_window(&state, cols, rows);
    assert!(
        projection.rows.len() <= usize::from(rows).saturating_sub(6),
        "the projection must fit the committed viewport, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
    assert!(
        visible.iter().any(|row| row.contains("[Create ")),
        "the create affordance must stay visible at 54x16, visible={visible:?}"
    );
    assert!(
        visible.iter().any(|row| row.contains("[Back]")),
        "the back affordance must stay visible at 54x16, visible={visible:?}"
    );
    assert!(
        visible
            .iter()
            .any(|row| row.contains("Claude Code") || row.contains("LLxprt")),
        "the display name must stay visible at 54x16, visible={visible:?}"
    );
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
