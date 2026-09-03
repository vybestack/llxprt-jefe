//! Repository form presentation tests for the shared overlay Form control.

use crate::domain::{Id, InternalId, RepositoryId, TypedValue};
use crate::host_controls::{
    ControlAction, ControlIntent, ControlKind, HostControlRowStyle, HostControlTitleStyle,
    PanelHitTarget,
};
use crate::overlay_controls::{REPOSITORY_FORM_FOOTER, overlay_intent, project_repository_form};
use crate::runtime::provider::protocol::PanelEvent;
use crate::state::{
    AppState, ModalState, RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};
use unicode_width::UnicodeWidthStr;

const WIDTH: usize = 80;
const FULL_FORM_WIDTH: usize = 116;

fn field_id(id: InternalId) -> Id {
    Id::internal(id)
}

fn edit_repository_state(focus: RepositoryFormFocus) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.available_agent_type_ids.clear();
    state.modal = ModalState::EditRepository {
        id: RepositoryId("repo.existing".to_owned()),
        fields: RepositoryFormFields {
            name: "existing".to_owned(),
            base_dir: "/tmp/existing".to_owned(),
            default_type_id: "core.llxprt".to_owned(),
            github_repo: "vybestack/llxprt-jefe".to_owned(),
            transient_max_concurrent: "0".to_owned(),
            ..RepositoryFormFields::default()
        },
        focus,
        cursor: RepositoryFormCursor::default(),
    };
    state
}

fn row_texts(state: &AppState, width: usize) -> Vec<String> {
    project_repository_form(state, width)
        .unwrap_or_else(|| panic!("repository form must project"))
        .text_rows()
        .map(str::to_owned)
        .collect()
}

#[test]
fn edit_repository_form_restores_aligned_bracketed_rows_labels_and_spacers() {
    let state = edit_repository_state(RepositoryFormFocus::TransientMaxConcurrent);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("edit repository form must project"));

    assert_eq!(projection.kind, ControlKind::Form);
    assert_eq!(projection.title, " Edit Repository");
    let rows = projection.text_rows().collect::<Vec<_>>();
    assert_eq!(
        rows.first(),
        Some(&""),
        "title must be followed by a spacer"
    );
    for expected in [
        "  Name             [existing]",
        "  Base Dir         [/tmp/existing]",
        "  Default Profile  []",
        "  Default Agent    [core.llxprt]  (no available agents)",
        "  Default Mode     []",
        "  Default Version  []",
        "  Identity File    []",
        "  SSH Options (space-separated) []",
        "  Run As User      []",
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
    for replaced_label in [
        "Default LLxprt Version",
        "Default CP Version",
        "SSH Options  [",
    ] {
        assert!(
            rows.iter().all(|row| !row.contains(replaced_label)),
            "projection-local labels must hide {replaced_label:?}, rows={rows:?}"
        );
    }
}

#[test]
fn repository_form_projects_dynamic_default_agent_hint() {
    let mut state = edit_repository_state(RepositoryFormFocus::DefaultAgentType);
    state.available_agent_type_ids = vec![crate::domain::shipped_agent_type(3)];
    let expected_hint = crate::state::effective_types_hint(&state.available_agent_type_ids);
    let expected = format!("  Default Agent    [core.llxprt]  ({expected_hint})");

    let rows = row_texts(&state, WIDTH);

    assert!(
        rows.iter().any(|row| row == &expected),
        "default-agent hint must reflect the effective choices, rows={rows:?}"
    );
}

#[test]
fn repository_form_projects_contextual_blank_and_set_hints() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    let blank_rows = row_texts(&state, WIDTH);
    for expected in [
        "  Issues / PRs Repo []  (blank uses GitHub Repo)",
        "  Transient Dir    []  (blank uses /tmp)",
        "  Max Transient    [0]  (0 = no limit)",
    ] {
        assert!(
            blank_rows.iter().any(|row| row == expected),
            "expected exact blank-value hint row {expected:?}, rows={blank_rows:?}"
        );
    }

    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.github_issue_pr_repo = "vybestack/issues".to_owned();
    fields.transient_agent_dir = "/var/tmp/agents".to_owned();
    fields.transient_max_concurrent = "4".to_owned();
    let set_rows = row_texts(&state, WIDTH);
    for expected in [
        "  Issues / PRs Repo [vybestack/issues]  (override issue/PR tracker)",
        "  Transient Dir    [/var/tmp/agents]  (transient agent work dirs root)",
        "  Max Transient    [4]  (max concurrent transient agents)",
    ] {
        assert!(
            set_rows.iter().any(|row| row == expected),
            "expected exact set-value hint row {expected:?}, rows={set_rows:?}"
        );
    }
}

#[test]
fn repository_form_restores_checkbox_marks_and_toggle_hints() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.remote_enabled = false;
    fields.setup_env_default = true;
    let rows = row_texts(&state, WIDTH);

    for expected in [
        "  Remote Repository [ ]  (space toggles)",
        "  Setup Env Default [x]  (space toggles)",
    ] {
        assert!(
            rows.iter().any(|row| row == expected),
            "expected checkbox row {expected:?}, rows={rows:?}"
        );
    }
    assert!(
        rows.iter()
            .all(|row| !row.contains(": true") && !row.contains(": false")),
        "booleans must not leak generic Form text, rows={rows:?}"
    );
}

#[test]
fn repository_form_marks_the_focused_field_with_a_caret_inside_brackets() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    let ModalState::EditRepository { cursor, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    cursor.name = 2;
    let name_id = field_id(InternalId::RepositoryFormName);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    assert_eq!(projection.focus_target.as_ref(), Some(&name_id));
    assert!(projection.rows.iter().any(|row| {
        row.target.as_ref() == Some(&PanelHitTarget::Field(name_id.clone()))
            && row.text == "  Name             [ex▏isting]"
            && row.style == HostControlRowStyle::Bright
    }));
    assert_eq!(projection.title_style, HostControlTitleStyle::Plain);
    assert_eq!(projection.title, " Edit Repository");
}

#[test]
fn repository_remote_rows_follow_legacy_disabled_precedence() {
    let disabled = edit_repository_state(RepositoryFormFocus::Host);
    let disabled_projection = project_repository_form(&disabled, WIDTH)
        .unwrap_or_else(|| panic!("disabled repository form must project"));
    let remote_field_ids = [
        InternalId::RepositoryFormLoginUser,
        InternalId::RepositoryFormHost,
        InternalId::RepositoryFormSshPort,
        InternalId::RepositoryFormIdentityFile,
        InternalId::RepositoryFormSshOptions,
        InternalId::RepositoryFormRunAsUser,
    ]
    .map(field_id);

    for id in &remote_field_ids {
        let row = disabled_projection
            .rows
            .iter()
            .find(|row| row.target.as_ref() == Some(&PanelHitTarget::Field(id.clone())))
            .unwrap_or_else(|| panic!("remote field {id:?} must project"));
        assert_eq!(
            row.style,
            HostControlRowStyle::Dim,
            "disabled wins over focus for remote field {id:?}"
        );
    }

    let mut enabled = edit_repository_state(RepositoryFormFocus::Name);
    let ModalState::EditRepository { fields, .. } = &mut enabled.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.remote_enabled = true;
    let enabled_projection = project_repository_form(&enabled, WIDTH)
        .unwrap_or_else(|| panic!("enabled repository form must project"));
    for id in &remote_field_ids {
        let row = enabled_projection
            .rows
            .iter()
            .find(|row| row.target.as_ref() == Some(&PanelHitTarget::Field(id.clone())))
            .unwrap_or_else(|| panic!("remote field {id:?} must project"));
        assert_eq!(row.style, HostControlRowStyle::Normal);
    }
}

#[test]
fn repository_form_hides_type_gated_fields_like_the_legacy_renderer() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.default_type_id.clear();
    let untyped_rows = row_texts(&state, WIDTH);
    assert!(
        untyped_rows
            .iter()
            .any(|row| row == "  Default Profile  []"),
        "the always-present default profile row must remain visible, rows={untyped_rows:?}"
    );
    for hidden in [
        "Default Model",
        "Default YOLO",
        "Default Version",
        "Default Mode",
    ] {
        assert!(
            untyped_rows.iter().all(|row| !row.contains(hidden)),
            "{hidden} stays hidden without a resolvable type, rows={untyped_rows:?}"
        );
    }

    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.default_type_id = "core.code-puppy".to_owned();
    let code_puppy_rows = row_texts(&state, WIDTH);
    for visible in [
        "Default Model",
        "Default Agent",
        "Default YOLO",
        "Default Version",
    ] {
        assert!(
            code_puppy_rows.iter().any(|row| row.contains(visible)),
            "code-puppy must show {visible:?}, rows={code_puppy_rows:?}"
        );
    }
    assert!(
        code_puppy_rows
            .iter()
            .all(|row| !row.starts_with("  Default Mode     [")),
        "code-puppy declares no profile fields, rows={code_puppy_rows:?}"
    );
}

#[test]
fn repository_projection_order_agrees_with_reducer_around_default_agent_and_yolo() {
    use RepositoryFormFocus as F;

    assert_eq!(F::DefaultCodePuppyModel.next(), F::DefaultAgentType);
    assert_eq!(F::DefaultAgentType.next(), F::DefaultCodePuppyYolo);
    assert_eq!(F::DefaultCodePuppyYolo.next(), F::DefaultCodePuppyVersion);

    let mut state = edit_repository_state(F::Name);
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.default_type_id = "core.code-puppy".to_owned();
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));
    let ordered_ids = [
        InternalId::RepositoryFormDefaultModel,
        InternalId::RepositoryFormDefaultAgentType,
        InternalId::RepositoryFormDefaultYolo,
        InternalId::RepositoryFormDefaultVersion,
    ]
    .map(field_id);
    let projected = projection
        .rows
        .iter()
        .filter_map(|row| match row.target.as_ref() {
            Some(PanelHitTarget::Field(id)) if ordered_ids.contains(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(projected, ordered_ids);
    assert_eq!(
        projected
            .iter()
            .filter(|id| **id == field_id(InternalId::RepositoryFormDefaultYolo))
            .count(),
        1,
        "Default YOLO must project exactly once"
    );
}

#[test]
fn repository_form_edit_field_yields_a_typed_change() {
    let state = edit_repository_state(RepositoryFormFocus::Name);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));
    let name_id = field_id(InternalId::RepositoryFormName);

    assert_eq!(
        overlay_intent(
            &projection,
            ControlAction::EditField {
                field_id: name_id.clone(),
                value: TypedValue::String("renamed".to_owned()),
            },
        ),
        ControlIntent::Event(PanelEvent::FieldChanged {
            field_id: name_id,
            value: TypedValue::String("renamed".to_owned()),
        })
    );
}

#[test]
fn repository_form_places_the_pending_error_after_fields_and_spacer() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    state.error_message = Some("GitHub repository must be owner/repo".to_owned());
    let rows = row_texts(&state, WIDTH);
    let error_index = rows
        .iter()
        .position(|row| row == "  Error: GitHub repository must be owner/repo")
        .unwrap_or_else(|| panic!("the pending error must ride the projection, rows={rows:?}"));

    assert!(error_index > 0, "error must follow the field rows");
    assert_eq!(rows[error_index - 1], "", "error must follow the spacer");
    assert_eq!(
        error_index,
        rows.len() - 1,
        "error must be the final projected row immediately above the shell footer"
    );
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("edit repository form must project"));
    assert!(
        projection.rows.iter().any(|row| {
            row.text == "  Error: GitHub repository must be owner/repo"
                && row.style == HostControlRowStyle::Bright
        }),
        "the pending validation error must render bright"
    );
}

#[test]
fn repository_form_hides_submit_text_but_retains_submit_contract() {
    let state = edit_repository_state(RepositoryFormFocus::Name);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

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
fn repository_form_fits_long_values_inside_one_bracketed_field_row() {
    let mut state = edit_repository_state(RepositoryFormFocus::Name);
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("fixture opens the edit repository form");
    };
    fields.name = "界".repeat(200);
    let name_id = field_id(InternalId::RepositoryFormName);

    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));
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
fn repository_form_footer_is_one_row_and_fits_a_120_column_overlay() {
    assert!(
        REPOSITORY_FORM_FOOTER.starts_with("  "),
        "footer must carry the two-space row indent: {REPOSITORY_FORM_FOOTER:?}"
    );
    assert!(
        REPOSITORY_FORM_FOOTER.ends_with(
            "Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles remote options  Enter submit  Esc cancel"
        ),
        "footer wording through the ending must be unchanged: {REPOSITORY_FORM_FOOTER:?}"
    );
    assert!(!REPOSITORY_FORM_FOOTER.contains('\n'));
    assert!(
        UnicodeWidthStr::width(REPOSITORY_FORM_FOOTER) <= FULL_FORM_WIDTH,
        "footer must fit the 116-cell content width: {REPOSITORY_FORM_FOOTER:?}"
    );
}
