//! Repository-form behavior tests for transient launch defaults (issue #317).

#[path = "common/app_state.rs"]
mod common_app_state;

use jefe::domain::canonical_values::{required_id, typed_field};
use jefe::domain::{Repository, RepositoryId, TypedMap, TypedValue};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, ModalState, RepositoryFormFocus, is_repository_field_visible};

fn set_yolo(values: &mut TypedMap, value: bool) {
    let id = required_id("yolo").unwrap_or_else(|error| panic!("valid yolo field id: {error}"));
    values.insert(id, TypedValue::Bool(value));
}

fn set_mode(values: &mut TypedMap, value: &str) {
    let id = required_id("mode-flags")
        .unwrap_or_else(|error| panic!("valid mode-flags field id: {error}"));
    values.insert(id, TypedValue::String(value.to_owned()));
}

fn yolo_value(values: &TypedMap) -> Option<bool> {
    match typed_field(values, "yolo") {
        Some(TypedValue::Bool(value)) => Some(*value),
        None => None,
        other => panic!("expected typed yolo field, got {other:?}"),
    }
}

#[test]
fn new_repository_defaults_transient_yolo_for_both_runtimes() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.available_agent_type_ids = vec![jefe::domain::shipped_agent_type(3)];
        state
    }
    .apply(AppEvent::OpenNewRepository)
    .committed_pure();

    let ModalState::NewRepository { fields, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--yolo");
    assert!(fields.default_code_puppy_yolo);
}

#[test]
fn repository_form_normalizes_and_persists_llxprt_mode_flags() {
    let mut state = crate::common_app_state::app_state()
        .apply(AppEvent::OpenNewRepository)
        .committed_pure();
    let ModalState::NewRepository { fields, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    fields.name = "Repo".to_owned();
    fields.default_type_id = jefe::domain::shipped_agent_type(3).to_string();
    fields.default_llxprt_mode = "  --yolo   --fast  ".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(
        yolo_value(&state.repositories[0].default_values),
        Some(true)
    );

    let repository_id = state.repositories[0].id.clone();
    state = state
        .apply(AppEvent::OpenEditRepository(repository_id))
        .committed_pure();
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.default_llxprt_mode = "   ".to_owned();

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(
        yolo_value(&state.repositories[0].default_values),
        Some(false)
    );
}

#[test]
fn edit_repository_loads_mode_and_code_puppy_yolo_choices() {
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo-317".into(),
    );
    set_yolo(&mut repository.default_values, false);
    set_mode(&mut repository.default_values, "--fast");
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository];
        state
    }
    .apply(AppEvent::OpenEditRepository(RepositoryId(
        "repo-317".to_owned(),
    )))
    .committed_pure();

    let ModalState::EditRepository { fields, cursor, .. } = state.modal else {
        panic!("expected edit-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--fast");
    assert_eq!(cursor.default_llxprt_mode, 6);
    assert!(!fields.default_code_puppy_yolo);
}

#[test]
fn repository_mode_field_supports_character_and_backspace_editing() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.available_agent_type_ids = vec![jefe::domain::shipped_agent_type(3)];
        state
    }
    .apply(AppEvent::OpenNewRepository)
    .committed_pure();
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::DefaultLlxprtMode;

    state = state.apply(AppEvent::FormChar('x')).committed_pure();
    state = state.apply(AppEvent::FormBackspace).committed_pure();

    let ModalState::NewRepository { fields, cursor, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--yolo");
    assert_eq!(cursor.default_llxprt_mode, 6);
}

#[test]
fn repository_runtime_specific_fields_are_visible_only_for_their_runtime() {
    let llxprt = jefe::domain::shipped_agent_type(3);
    let code_puppy = jefe::domain::shipped_agent_type(1);
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultLlxprtMode,
        Some(&llxprt)
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyYolo,
        Some(&llxprt)
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultLlxprtMode,
        Some(&code_puppy)
    ));
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyYolo,
        Some(&code_puppy)
    ));
}
