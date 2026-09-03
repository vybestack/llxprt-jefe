//! Agent form projection tests for the shared overlay Form control.

use crate::domain::{AgentId, Id, InternalId, PlatformCapabilities, RepositoryId, TypedValue};
use crate::host_controls::{
    ControlAction, ControlIntent, HostControlRowStyle, HostControlTitleStyle, PanelHitTarget,
};
use crate::overlay_controls::overlay_intent;
use crate::overlay_controls_agent_form::{AGENT_FORM_FOOTER, project_agent_form};
use crate::runtime::provider::protocol::PanelEvent;
use crate::state::{AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState};
use unicode_width::UnicodeWidthStr;

const WIDTH: usize = 80;
const FULL_FORM_WIDTH: usize = 116;

fn agent_state(type_id: &str, focus: AgentFormFocus) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.available_agent_type_ids.clear();
    let fields = AgentFormFields {
        agent_type_id: type_id.to_owned(),
        name: "jefe".to_owned(),
        description: "dev agent".to_owned(),
        work_dir: "/tmp/jefe".to_owned(),
        profile: "dev".to_owned(),
        mode: "--yolo".to_owned(),
        pass_continue: true,
        sandbox_engine: "Podman".to_owned(),
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

fn edit_agent_state(type_id: &str, focus: AgentFormFocus) -> AppState {
    let mut state = agent_state(type_id, focus);
    let ModalState::NewAgent { fields, cursor, .. } = &state.modal else {
        panic!("fixture opens the new agent form");
    };
    state.modal = ModalState::EditAgent {
        id: AgentId("a".to_owned()),
        fields: fields.clone(),
        focus,
        cursor: cursor.clone(),
    };
    state
}

fn row_texts(state: &AppState, width: usize) -> Vec<String> {
    project_agent_form(state, width)
        .unwrap_or_else(|| panic!("agent form must project"))
        .text_rows()
        .map(str::to_owned)
        .collect()
}

#[test]
fn edit_agent_form_restores_aligned_bracketed_rows_and_spacers() {
    let state = edit_agent_state("core.llxprt", AgentFormFocus::SandboxEngine);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));

    assert_eq!(projection.title, " Edit Agent");
    let rows = projection.text_rows().collect::<Vec<_>>();
    assert_eq!(
        rows.first(),
        Some(&""),
        "title must be followed by a spacer"
    );
    for expected in [
        "  Shortcut (1-9)   [none]",
        "  Name             [jefe]",
        "  Description      [dev agent]",
        "  Work Dir         [/tmp/jefe]",
        "  Profile          [dev]",
        "  Mode Flags       [--yolo]",
        "  Version          []",
        "  LLXPRT_DEBUG     []",
        "  Sandbox Flags    []",
    ] {
        assert!(
            rows.contains(&expected),
            "expected exact restored row {expected:?}, rows={rows:?}"
        );
    }
    assert_eq!(
        rows.iter().filter(|row| row.is_empty()).count(),
        2,
        "the projection needs one spacer after the title and one before the footer"
    );
    assert!(
        rows.iter().all(|row| !row.contains("LLxprt Version")),
        "the projection-local display label is Version, rows={rows:?}"
    );
}

#[test]
fn agent_form_restores_checkbox_runtime_and_disabled_engine_hints() {
    let state = edit_agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let rows = row_texts(&state, WIDTH);

    for expected in [
        "  Agent Runtime    [core.llxprt]  (no available agents)",
        "  Pass --continue  [x]  (space toggles)",
        "  Sandbox          [ ]  (space toggles)",
        "  Sandbox Engine   [Podman]  (disabled)",
    ] {
        assert!(
            rows.iter().any(|row| row == expected),
            "expected exact hint row {expected:?}, rows={rows:?}"
        );
    }
    assert!(
        rows.iter()
            .all(|row| !row.contains(": true") && !row.contains(": false")),
        "booleans must not leak generic Form text, rows={rows:?}"
    );
}

#[test]
fn enabled_sandbox_engine_lists_the_platform_cycle_choices() {
    let mut state = edit_agent_state("core.llxprt", AgentFormFocus::SandboxEngine);
    let ModalState::EditAgent { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit agent form");
    };
    fields.sandbox_enabled = true;
    let labels = PlatformCapabilities::current()
        .supported_engines()
        .iter()
        .map(|engine| engine.label())
        .collect::<Vec<_>>()
        .join(" / ");
    let expected = format!("  Sandbox Engine   [Podman]  (space cycles: {labels})");

    let rows = row_texts(&state, FULL_FORM_WIDTH);
    assert!(
        rows.iter().any(|row| row == &expected),
        "enabled sandbox hint must describe the real cycle order, rows={rows:?}"
    );
}

#[test]
fn agent_form_hides_llxprt_fields_for_code_puppy() {
    let state = agent_state("core.code-puppy", AgentFormFocus::Name);
    let rows = row_texts(&state, WIDTH);
    for expected in [
        "  YOLO             [ ]  (space toggles)",
        "  Quick resume     [ ]  (space toggles)",
    ] {
        assert!(
            rows.iter().any(|row| row == expected),
            "code-puppy shows {expected:?}, rows={rows:?}"
        );
    }
    for absent in [
        "Mode Flags",
        "LLXPRT_DEBUG",
        "Pass --continue",
        "Sandbox ",
        "Sandbox Engine",
        "Sandbox Flags",
    ] {
        assert!(
            rows.iter().all(|row| !row.contains(absent)),
            "code-puppy hides {absent:?}, rows={rows:?}"
        );
    }
}

#[test]
fn agent_form_marks_the_focused_field_with_a_caret_inside_brackets() {
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
            .any(|row| row == "  Name             [je▏fe]"),
        "the focused field carries the text caret inside brackets, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
    assert!(projection.rows.iter().any(|row| {
        row.target.as_ref() == Some(&PanelHitTarget::Field(name_id.clone()))
            && row.text == "  Name             [je▏fe]"
            && row.style == HostControlRowStyle::Bright
    }));
    assert_eq!(projection.title_style, HostControlTitleStyle::Plain);
    assert_eq!(projection.title, " New Agent");
}

#[test]
fn edit_agent_focused_row_is_bright() {
    let state = edit_agent_state("core.llxprt", AgentFormFocus::Name);
    let name_id = Id::internal(InternalId::AgentFormName);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("edit agent form must project"));

    assert!(projection.rows.iter().any(|row| {
        row.target.as_ref() == Some(&PanelHitTarget::Field(name_id.clone()))
            && row.style == HostControlRowStyle::Bright
    }));
    assert_eq!(projection.title_style, HostControlTitleStyle::Plain);
    assert_eq!(projection.title, " Edit Agent");
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
fn agent_form_places_the_pending_error_after_fields_and_spacer() {
    let mut state = edit_agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    state.error_message = Some("name is required".to_owned());
    let rows = row_texts(&state, WIDTH);
    let error_index = rows
        .iter()
        .position(|row| row == "  Error: name is required")
        .unwrap_or_else(|| panic!("the pending error must ride the projection, rows={rows:?}"));

    assert!(error_index > 0, "error must follow the field rows");
    assert_eq!(rows[error_index - 1], "", "error must follow the spacer");
    assert_eq!(
        error_index,
        rows.len() - 1,
        "error must be the final projected row immediately above the shell footer"
    );
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("edit agent form must project"));
    assert!(
        projection.rows.iter().any(|row| {
            row.text == "  Error: name is required" && row.style == HostControlRowStyle::Bright
        }),
        "the pending validation error must render bright"
    );
}

#[test]
fn agent_form_hides_submit_text_but_retains_submit_contract() {
    let state = edit_agent_state("core.llxprt", AgentFormFocus::LlxprtDebug);
    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));

    assert!(
        projection
            .text_rows()
            .all(|row| !row.contains("submit:") && !row.contains("overlay-submit")),
        "internal submit text must not be visible, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
    assert!(
        projection.rows.iter().any(|row| {
            row.text.is_empty() && row.target.as_ref() == Some(&PanelHitTarget::Submit)
        }),
        "the blank pre-footer spacer retains the shared submit hit target"
    );
    assert!(matches!(
        overlay_intent(&projection, ControlAction::Activate),
        ControlIntent::Event(PanelEvent::Submit { .. })
    ));
}

#[test]
fn agent_form_fits_long_values_inside_one_bracketed_field_row() {
    let mut state = edit_agent_state("core.llxprt", AgentFormFocus::Name);
    let ModalState::EditAgent { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit agent form");
    };
    fields.name = "界".repeat(200);
    let name_id = Id::internal(InternalId::AgentFormName);

    let projection =
        project_agent_form(&state, WIDTH).unwrap_or_else(|| panic!("agent form must project"));
    let matching = projection
        .rows
        .iter()
        .filter(|row| row.target.as_ref() == Some(&PanelHitTarget::Field(name_id.clone())))
        .collect::<Vec<_>>();

    assert_eq!(
        matching.len(),
        1,
        "one field must project to exactly one row"
    );
    let row = &matching[0].text;
    assert!(row.starts_with("  Name             ["), "row={row:?}");
    assert!(
        row.ends_with(']'),
        "the closing bracket must survive fitting: {row:?}"
    );
    assert!(
        row.contains('…'),
        "the over-width value must be truncated: {row:?}"
    );
    assert!(
        UnicodeWidthStr::width(row.as_str()) <= WIDTH,
        "row exceeds the overlay width: {row:?}"
    );
}

#[test]
fn agent_form_footer_is_one_row_and_fits_a_120_column_overlay() {
    assert!(
        AGENT_FORM_FOOTER.starts_with("  "),
        "footer must carry the two-space row indent: {AGENT_FORM_FOOTER:?}"
    );
    assert!(
        AGENT_FORM_FOOTER.ends_with(
            "Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles/cycles checkboxes  Enter submit  Esc"
        ),
        "footer wording through the ending must be unchanged: {AGENT_FORM_FOOTER:?}"
    );
    assert!(!AGENT_FORM_FOOTER.contains('\n'));
    assert!(
        UnicodeWidthStr::width(AGENT_FORM_FOOTER) <= FULL_FORM_WIDTH,
        "footer must fit the 116-cell content width: {AGENT_FORM_FOOTER:?}"
    );
}
