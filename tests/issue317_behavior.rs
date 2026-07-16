//! Repository-form behavior tests for transient launch defaults (issue #317).

use jefe::domain::{AgentKind, Repository, RepositoryId};
use jefe::state::{
    AppEvent, AppState, ModalState, RepositoryFormFocus, is_repository_field_visible,
};

#[test]
fn new_repository_defaults_transient_yolo_for_both_runtimes() {
    let state = AppState {
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewRepository);

    let ModalState::NewRepository { fields, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--yolo");
    assert!(fields.default_code_puppy_yolo);
}

#[test]
fn repository_form_normalizes_and_persists_llxprt_mode_flags() {
    let mut state = AppState::default().apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { fields, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    fields.name = "Repo".to_owned();
    fields.default_agent_kind = AgentKind::Llxprt.label().to_owned();
    fields.default_llxprt_mode = "  --yolo   --fast  ".to_owned();

    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(
        state.repositories[0].default_llxprt_mode_flags,
        vec!["--yolo", "--fast"]
    );

    let repository_id = state.repositories[0].id.clone();
    state = state.apply(AppEvent::OpenEditRepository(repository_id));
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("expected edit-repository modal");
    };
    fields.default_llxprt_mode = "   ".to_owned();

    state = state.apply(AppEvent::SubmitForm);

    assert!(state.repositories[0].default_llxprt_mode_flags.is_empty());
}

#[test]
fn edit_repository_loads_mode_and_code_puppy_yolo_choices() {
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo-317".into(),
    );
    repository.default_llxprt_mode_flags = vec!["--fast".to_owned()];
    repository.default_code_puppy_yolo = None;
    let state = AppState {
        repositories: vec![repository],
        ..AppState::default()
    }
    .apply(AppEvent::OpenEditRepository(RepositoryId(
        "repo-317".to_owned(),
    )));

    let ModalState::EditRepository { fields, cursor, .. } = state.modal else {
        panic!("expected edit-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--fast");
    assert_eq!(cursor.default_llxprt_mode, 6);
    assert!(!fields.default_code_puppy_yolo);
}

#[test]
fn repository_mode_field_supports_character_and_backspace_editing() {
    let mut state = AppState {
        installed_agent_kinds: vec![AgentKind::Llxprt],
        ..AppState::default()
    }
    .apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::DefaultLlxprtMode;

    state = state.apply(AppEvent::FormChar('x'));
    state = state.apply(AppEvent::FormBackspace);

    let ModalState::NewRepository { fields, cursor, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(fields.default_llxprt_mode, "--yolo");
    assert_eq!(cursor.default_llxprt_mode, 6);
}

#[test]
fn repository_runtime_specific_fields_are_visible_only_for_their_runtime() {
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultLlxprtMode,
        AgentKind::Llxprt
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyYolo,
        AgentKind::Llxprt
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultLlxprtMode,
        AgentKind::CodePuppy
    ));
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyYolo,
        AgentKind::CodePuppy
    ));
}

#[test]
fn explicit_persisted_yolo_opt_outs_survive_deserialization() {
    let repository: Repository = serde_json::from_value(serde_json::json!({
        "id": "repo-1",
        "name": "Repo",
        "slug": "repo",
        "base_dir": "/tmp/repo",
        "default_profile": "",
        "default_code_puppy_yolo": null,
        "default_llxprt_mode_flags": [],
        "agent_ids": []
    }))
    .unwrap_or_else(|error| panic!("repository should deserialize: {error}"));

    assert_eq!(repository.default_code_puppy_yolo, None);
    assert!(repository.default_llxprt_mode_flags.is_empty());
}
