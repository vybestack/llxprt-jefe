use std::path::PathBuf;

use jefe::domain::canonical_values::{required_id, typed_field};
use jefe::domain::{Agent, AgentId, AgentTypeId, Repository, RepositoryId, TypedMap, TypedValue};
use jefe::selection::{agent_form_content_lines, repository_form_content_lines};
use jefe::services::{CreateAgentParams, create_agent, prospective_agent_launch};
use jefe::state::transition::TransitionExt;
use jefe::state::{
    AgentFormFocus, AppEvent, AppState, ModalState, RepositoryFormFocus, agent_form_visibility,
    is_field_visible, is_repository_field_visible, next_visible_focus,
    next_visible_repository_focus, prev_visible_focus,
};

trait OptionTestExt<T> {
    fn value_or_panic(self, message: &str) -> T;
}

impl<T> OptionTestExt<T> for Option<T> {
    fn value_or_panic(self, message: &str) -> T {
        self.unwrap_or_else(|| panic!("{message}"))
    }
}

fn repository(kind: AgentTypeId) -> Repository {
    Repository::new(
        RepositoryId("repo-270".to_owned()),
        kind,
        TypedMap::new(),
        "Issue 270".to_owned(),
        "issue-270".to_owned(),
        PathBuf::from("/tmp/issue-270"),
    )
}

fn set_string(values: &mut TypedMap, field: &str, value: &str) {
    let normalized = field.replace('_', "-");
    let id = required_id(&normalized)
        .unwrap_or_else(|error| panic!("valid typed field {field}: {error}"));
    values.insert(id, TypedValue::String(value.to_owned()));
}

fn string_value<'a>(values: &'a TypedMap, field: &str) -> Option<&'a str> {
    match typed_field(values, field) {
        Some(TypedValue::String(value)) => Some(value),
        None => None,
        other => panic!("expected string field {field}, got {other:?}"),
    }
}

fn state_with_repository(repository: Repository) -> AppState {
    AppState {
        repositories: vec![repository],
        selected_repository_index: Some(0),
        available_agent_type_ids: vec![
            jefe::domain::shipped_agent_type(3),
            jefe::domain::shipped_agent_type(1),
        ],
        ..AppState::default()
    }
}

fn create_params<'a>(repository: &'a Repository, version: &'a str) -> CreateAgentParams<'a> {
    CreateAgentParams {
        repository,
        name: "Puppy",
        description: "",
        work_dir: "/tmp/issue-270/puppy",
        profile: "",
        code_puppy_model: "",
        code_puppy_version: version,
        code_puppy_yolo: false,
        code_puppy_quick_resume: jefe::domain::QuickResume::default(),
        agent_type_id: "core.code-puppy",
        llxprt_version: "",
        mode: "",
        llxprt_debug: "",
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman",
        sandbox_flags: "",
        shortcut_slot: None,
        next_display_index: 1,
    }
}

#[test]
fn code_puppy_agent_version_is_visible_focusable_and_hidden_draft_survives_switching() {
    let puppy_id = jefe::domain::shipped_agent_type(1);
    let llxprt_id = jefe::domain::shipped_agent_type(3);
    let puppy = agent_form_visibility(Some(&puppy_id));
    let llxprt = agent_form_visibility(Some(&llxprt_id));

    assert!(is_field_visible(AgentFormFocus::CodePuppyVersion, &puppy));
    assert!(!is_field_visible(AgentFormFocus::CodePuppyVersion, &llxprt));
    assert_eq!(
        next_visible_focus(AgentFormFocus::CodePuppyModel, &puppy),
        AgentFormFocus::CodePuppyVersion
    );
    assert_eq!(
        prev_visible_focus(AgentFormFocus::CodePuppyVersion, &puppy),
        AgentFormFocus::CodePuppyModel
    );

    let mut state = state_with_repository(repository(jefe::domain::shipped_agent_type(1)))
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-270".to_owned())))
        .committed_pure();
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    fields.code_puppy_version = "0.0.361-rc1".to_owned();
    fields.agent_type_id = "core.llxprt".to_owned();
    assert!(
        !agent_form_content_lines(&state)
            .value_or_panic("agent form content")
            .iter()
            .any(|line| line.contains("0.0.361-rc1"))
    );

    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should remain open");
    };
    fields.agent_type_id = "core.code-puppy".to_owned();
    assert!(
        agent_form_content_lines(&state)
            .value_or_panic("agent form content")
            .iter()
            .any(|line| line.contains("Version") && line.contains("0.0.361-rc1"))
    );
}

#[test]
fn repository_default_version_is_code_puppy_only_focusable_and_draft_is_retained() {
    let puppy = jefe::domain::shipped_agent_type(1);
    let llxprt = jefe::domain::shipped_agent_type(3);
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyVersion,
        Some(&puppy)
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyVersion,
        Some(&llxprt)
    ));
    assert_eq!(
        next_visible_repository_focus(RepositoryFormFocus::DefaultAgentType, &puppy),
        RepositoryFormFocus::DefaultCodePuppyYolo
    );
    assert_eq!(
        next_visible_repository_focus(RepositoryFormFocus::DefaultCodePuppyYolo, &puppy),
        RepositoryFormFocus::DefaultCodePuppyVersion
    );

    let mut state = state_with_repository(repository(jefe::domain::shipped_agent_type(1)))
        .apply(AppEvent::OpenEditRepository(RepositoryId(
            "repo-270".to_owned(),
        )))
        .committed_pure();
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("edit-repository modal should be open");
    };
    fields.default_code_puppy_version = "0.0.361".to_owned();
    fields.default_type_id = "core.llxprt".to_owned();
    assert!(
        !repository_form_content_lines(&state)
            .value_or_panic("repository form content")
            .iter()
            .any(|line| line.contains("0.0.361"))
    );

    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("edit-repository modal should remain open");
    };
    fields.default_type_id = "core.code-puppy".to_owned();
    fields.default_code_puppy_version = "  0.0.361  ".to_owned();
    assert!(
        repository_form_content_lines(&state)
            .value_or_panic("repository form content")
            .iter()
            .any(|line| line.contains("Default Version") && line.contains("0.0.361"))
    );
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        string_value(&state.repositories[0].default_values, "version_selector"),
        Some("0.0.361")
    );
}

#[test]
fn create_and_edit_mappings_trim_code_puppy_versions() {
    let repository = repository(jefe::domain::shipped_agent_type(1));
    let agent = create_agent(create_params(&repository, "  0.0.361-rc1  "))
        .value_or_panic("valid Code Puppy agent");
    assert_eq!(
        string_value(&agent.values, "version_selector"),
        Some("0.0.361-rc1")
    );

    let mut state = state_with_repository(repository);
    state.agents.push(agent);
    let agent_id = state.agents[0].id.clone();
    state = state
        .apply(AppEvent::OpenEditAgent(agent_id))
        .committed_pure();
    let ModalState::EditAgent { fields, .. } = &mut state.modal else {
        panic!("edit-agent modal should be open");
    };
    fields.code_puppy_version = "  nightly  ".to_owned();
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        string_value(&state.agents[0].values, "version_selector"),
        Some("nightly")
    );
}

#[test]
fn repository_default_copies_once_into_new_persistent_and_transient_code_puppy_agents() {
    let mut repository = repository(jefe::domain::shipped_agent_type(1));
    set_string(
        &mut repository.default_values,
        "version_selector",
        "0.0.361",
    );

    let persistent = create_agent(create_params(&repository, ""))
        .value_or_panic("valid persistent Code Puppy agent");
    let transient = Agent::new_transient(
        AgentId("transient-270".to_owned()),
        repository.id.clone(),
        repository.effective_transient_dir().join("transient-270"),
        &repository,
    );
    assert_eq!(
        string_value(&persistent.values, "version_selector"),
        Some("")
    );
    assert_eq!(
        string_value(&transient.values, "version_selector"),
        Some("0.0.361")
    );

    set_string(&mut repository.default_values, "version_selector", "later");
    assert_eq!(
        string_value(&persistent.values, "version_selector"),
        Some("")
    );
    assert_eq!(
        string_value(&transient.values, "version_selector"),
        Some("0.0.361")
    );
}

#[test]
fn llxprt_agents_do_not_copy_code_puppy_repository_default() {
    let mut repository = repository(jefe::domain::shipped_agent_type(3));
    set_string(
        &mut repository.default_values,
        "version_selector",
        "0.0.361",
    );
    let persistent = create_agent(CreateAgentParams {
        agent_type_id: "core.llxprt",
        ..create_params(&repository, "")
    })
    .value_or_panic("valid persistent LLxprt agent");
    let agent = Agent::new_transient(
        AgentId("transient-llxprt-270".to_owned()),
        repository.id.clone(),
        repository
            .effective_transient_dir()
            .join("transient-llxprt-270"),
        &repository,
    );
    assert_eq!(
        string_value(&persistent.values, "version_selector"),
        Some("")
    );
    assert_eq!(
        string_value(&agent.values, "version_selector"),
        Some("0.0.361")
    );
}

#[test]
fn prospective_launch_carries_trimmed_pin_as_a_typed_value() {
    let repository = repository(jefe::domain::shipped_agent_type(1));
    let request = prospective_agent_launch(&create_params(&repository, "  0.0.361-rc1  "))
        .value_or_panic("valid prospective launch");
    assert_eq!(request.type_id, jefe::domain::shipped_agent_type(1));
    assert_eq!(
        string_value(&request.values, "version_selector"),
        Some("0.0.361-rc1")
    );
}
