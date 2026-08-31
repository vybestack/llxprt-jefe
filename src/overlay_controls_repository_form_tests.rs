//! Production-path tests for the repository form projected through the
//! shared overlay Form control (issue #706).
//!
//! The legacy bespoke modal renderer is replaced by the same sealed
//! HostControl Form factory the search and confirmation overlays already
//! use: field rows with typed hit targets, focus carried as data, and
//! EditField intents answered with typed PanelEvents.

use crate::domain::{Id, InternalId, TypedValue};
use crate::host_controls::{ControlAction, ControlIntent, ControlKind};
use crate::overlay_controls::{overlay_intent, project_repository_form};
use crate::runtime::provider::protocol::PanelEvent;
use crate::state::{
    AppState, ModalState, RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};

const WIDTH: usize = 80;

fn name_field_id() -> Id {
    Id::internal(InternalId::RepositoryFormName)
}

fn form_state(type_id: &str, focus: RepositoryFormFocus) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.modal = ModalState::NewRepository {
        fields: RepositoryFormFields {
            name: "jefe".to_owned(),
            base_dir: "/tmp/jefe".to_owned(),
            default_profile: "dev".to_owned(),
            default_type_id: type_id.to_owned(),
            github_repo: "vybestack/llxprt-jefe".to_owned(),
            remote_enabled: true,
            login_user: "acoliver".to_owned(),
            host: "example.com".to_owned(),
            ssh_port: "22".to_owned(),
            transient_agent_dir: String::new(),
            transient_max_concurrent: "0".to_owned(),
            ..RepositoryFormFields::default()
        },
        focus,
        cursor: RepositoryFormCursor::default(),
    };
    state
}

#[test]
fn repository_form_projects_its_fields_through_the_form_control() {
    let state = form_state("core.code-puppy", RepositoryFormFocus::IdentityFile);

    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    assert_eq!(projection.kind, ControlKind::Form);
    assert_eq!(projection.title, "New Repository");
    let rows: Vec<&str> = projection.text_rows().collect();
    for expected in [
        "Name: jefe",
        "Base Dir: /tmp/jefe",
        "Default Profile: dev",
        "Default Model:",
        "Default Agent: core.code-puppy",
        "GitHub Repo: vybestack/llxprt-jefe",
        "Remote Repository: true",
        "Login User: acoliver",
        "Host / IP: example.com",
        "SSH Port: 22",
    ] {
        assert!(
            rows.iter().any(|row| row.contains(expected)),
            "expected a row containing {expected:?}, rows={rows:?}"
        );
    }
    assert!(
        projection
            .rows
            .iter()
            .any(|row| row.text.starts_with("Name:")
                && row.target.as_ref().is_some_and(|target| {
                    matches!(
                        target,
                        crate::host_controls::PanelHitTarget::Field(id) if *id == name_field_id()
                    )
                })),
        "the name row must carry its typed field hit target"
    );
}

#[test]
fn repository_form_hides_type_gated_fields_like_the_legacy_renderer() {
    let untyped = form_state("", RepositoryFormFocus::Name);
    let projection = project_repository_form(&untyped, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));
    let rows: Vec<&str> = projection.text_rows().collect();
    for hidden in [
        "Default Model:",
        "Default YOLO:",
        "Default CP Version:",
        "Default Mode:",
        "Default LLxprt Version:",
    ] {
        assert!(
            !rows.iter().any(|row| row.contains(hidden)),
            "{hidden} stays hidden without a resolvable type, rows={rows:?}"
        );
    }

    let typed = form_state("core.code-puppy", RepositoryFormFocus::Name);
    let projection = project_repository_form(&typed, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));
    let rows: Vec<&str> = projection.text_rows().collect();
    assert!(
        rows.iter().any(|row| row.contains("Default YOLO: ")),
        "the code-puppy definition declares the yolo field, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("Default CP Version:")),
        "visibility follows the definition helper exactly, so the agent-field \
         version selector keeps the CP row visible for code-puppy, rows={rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("Default Mode:")),
        "code-puppy declares no profile fields, rows={rows:?}"
    );
}

#[test]
fn repository_form_marks_the_focused_field_with_a_caret() {
    let mut state = form_state("core.code-puppy", RepositoryFormFocus::Name);
    let ModalState::NewRepository { cursor, .. } = &mut state.modal else {
        panic!("fixture lost its form");
    };
    cursor.name = 2;

    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    assert_eq!(projection.focus_target.as_ref(), Some(&name_field_id()));
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Name: je▏fe")),
        "the focused field carries the text caret, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn edit_repository_form_titles_and_carries_current_values() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.modal = ModalState::EditRepository {
        id: crate::domain::RepositoryId::default(),
        fields: RepositoryFormFields {
            name: "existing".to_owned(),
            github_repo: "vybestack/llxprt-jefe".to_owned(),
            ..RepositoryFormFields::default()
        },
        focus: RepositoryFormFocus::GitHubRepo,
        cursor: RepositoryFormCursor::default(),
    };

    let projection =
        project_repository_form(&state, WIDTH).unwrap_or_else(|| panic!("edit form must project"));

    assert_eq!(projection.title, "Edit Repository");
    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Name: existing")),
        "edit form carries current values"
    );
}

#[test]
fn repository_form_edit_field_yields_a_typed_change() {
    let state = form_state("core.code-puppy", RepositoryFormFocus::Name);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    match overlay_intent(
        &projection,
        ControlAction::EditField {
            field_id: name_field_id(),
            value: TypedValue::String("renamed".to_owned()),
        },
    ) {
        ControlIntent::Event(PanelEvent::FieldChanged { field_id, value }) => {
            assert_eq!(field_id, name_field_id());
            assert_eq!(value, TypedValue::String("renamed".to_owned()));
        }
        other => panic!("expected a typed field change, got {other:?}"),
    }
}

#[test]
fn repository_form_surfaces_the_pending_error_message() {
    let mut state = form_state("core.code-puppy", RepositoryFormFocus::Name);
    state.error_message = Some("name is required".to_owned());

    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    assert!(
        projection
            .text_rows()
            .any(|row| row.contains("Error: name is required")),
        "the pending error rides the projection like the legacy renderer, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}

#[test]
fn repository_form_carries_the_shared_submit_affordance() {
    let state = form_state("core.code-puppy", RepositoryFormFocus::Name);
    let projection = project_repository_form(&state, WIDTH)
        .unwrap_or_else(|| panic!("repository form must project"));

    assert!(
        projection
            .rows
            .iter()
            .any(|row| row.text.starts_with("submit:")
                && row.target.as_ref().is_some_and(|target| matches!(
                    target,
                    crate::host_controls::PanelHitTarget::Submit
                ))),
        "the shared Form submit affordance closes the projection, rows={:?}",
        projection.text_rows().collect::<Vec<_>>()
    );
}
