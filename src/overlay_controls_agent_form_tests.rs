//! RED tests: the agent form (new/edit) must project through the shared
//! overlay Form control, mirroring the legacy renderer's field order,
//! visibility, caret, error, and submit affordance.

use crate::domain::{AgentId, Id, InternalId, RepositoryId, TypedValue};
use crate::host_controls::{ControlAction, ControlIntent, PanelHitTarget};
use crate::overlay_controls::overlay_intent;
use crate::overlay_controls_agent_form::project_agent_form;
use crate::runtime::provider::protocol::PanelEvent;
use crate::state::{AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState};

const WIDTH: usize = 80;

fn agent_state(type_id: &str, focus: AgentFormFocus) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let fields = AgentFormFields {
        agent_type_id: type_id.to_owned(),
        name: "jefe".to_owned(),
        description: "dev agent".to_owned(),
        work_dir: "/tmp/jefe".to_owned(),
        profile: "dev".to_owned(),
        mode: "--yolo".to_owned(),
        ..AgentFormFields::default()
    };
    state.modal = ModalState::NewAgent {
        repository_id: RepositoryId("r".to_owned()),
        fields,
        focus,
        cursor: AgentFormCursor::default(),
        work_dir_manual: false,
    };
    state
}

#[test]
fn agent_form_projects_its_fields_through_the_form_control() {
    let state = agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    assert_eq!(projection.title, "New Agent");
    let rows = projection.text_rows().collect::<Vec<_>>();
    for expected in [
        "Shortcut (1-9): none",
        "Name: jefe",
        "Description: dev agent",
        "Work Dir: /tmp/jefe",
        "Profile: dev",
        "Agent Runtime: core.llxprt",
        "Mode Flags: --yolo",
        "LLxprt Version:",
        "LLXPRT_DEBUG:",
        "Pass --continue: false",
        "Sandbox: false",
        "Sandbox Engine:",
        "Sandbox Flags:",
    ] {
        assert!(
            rows.iter().any(|row| row.contains(expected)),
            "expected a row containing {expected:?}, rows={rows:?}"
        );
    }
    for absent in ["YOLO:", "Quick resume:", "Model:", "CP Version:"] {
        assert!(
            rows.iter().all(|row| !row.contains(absent)),
            "llxprt hides {absent:?}, rows={rows:?}"
        );
    }
}

#[test]
fn agent_form_hides_llxprt_fields_for_code_puppy() {
    let state = agent_state("core.code-puppy", AgentFormFocus::Name);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    let rows = projection.text_rows().collect::<Vec<_>>();
    for expected in ["YOLO: false", "Quick resume: false"] {
        assert!(
            rows.iter().any(|row| row.contains(expected)),
            "code-puppy shows {expected:?}, rows={rows:?}"
        );
    }
    for absent in [
        "Mode Flags:",
        "LLXPRT_DEBUG:",
        "Pass --continue:",
        "Sandbox:",
        "Sandbox Engine:",
        "Sandbox Flags:",
    ] {
        assert!(
            rows.iter().all(|row| !row.contains(absent)),
            "code-puppy hides {absent:?}, rows={rows:?}"
        );
    }
}

#[test]
fn agent_form_marks_the_focused_field_with_a_caret() {
    let mut state = agent_state("core.llxprt", AgentFormFocus::Name);
    let ModalState::NewAgent { cursor, .. } = &mut state.modal else {
        panic!("fixture opens the new agent form");
    };
    cursor.name = 2;
    let name_id = Id::internal(InternalId::AgentFormName);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    assert_eq!(projection.focus_target.as_ref(), Some(&name_id));
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Name: je▏fe")),
        "the focused field carries the text caret, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn edit_agent_form_titles_and_carries_current_values() {
    let mut state = agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("fixture opens the new agent form");
    };
    fields.name = "existing".to_owned();
    let fields = fields.clone();
    state.modal = ModalState::EditAgent {
        id: AgentId("a".to_owned()),
        fields,
        focus: AgentFormFocus::LlxprtDebug,
        cursor: AgentFormCursor::default(),
    };
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    assert_eq!(projection.title, "Edit Agent");
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Name: existing")),
        "edit carries current values, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn agent_form_edit_field_yields_a_typed_change() {
    let state = agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    let name_id = Id::internal(InternalId::AgentFormName);
    let intent = overlay_intent(
        &projection,
        ControlAction::EditField {
            field_id: name_id.clone(),
            value: TypedValue::String("renamed".to_owned()),
        },
    );
    assert_eq!(
        intent,
        ControlIntent::Event(PanelEvent::FieldChanged {
            field_id: name_id,
            value: TypedValue::String("renamed".to_owned()),
        })
    );
}

#[test]
fn agent_form_surfaces_the_pending_error_message() {
    let mut state = agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    state.error_message = Some("name is required".to_owned());
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Error: name is required")),
        "the pending error rides the projection, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn agent_form_carries_the_shared_submit_affordance() {
    let state = agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    assert!(
        projection
            .rows
            .iter()
            .any(|row| row.text.starts_with("submit:")
                && row
                    .target
                    .as_ref()
                    .is_some_and(|target| matches!(target, PanelHitTarget::Submit))),
        "the shared Form submit affordance closes the projection, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}
