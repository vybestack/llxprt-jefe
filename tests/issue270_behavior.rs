use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentKind, LaunchSignature, Repository, RepositoryId};
use jefe::selection::{agent_form_content_lines, repository_form_content_lines};
use jefe::services::{CreateAgentParams, create_agent, prospective_agent_launch};
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

trait ResultTestExt<T> {
    fn value_or_panic(self, message: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultTestExt<T> for Result<T, E> {
    fn value_or_panic(self, message: &str) -> T {
        self.unwrap_or_else(|error| panic!("{message}: {error:?}"))
    }
}

fn repository(kind: AgentKind) -> Repository {
    let mut repository = Repository::new(
        RepositoryId("repo-270".to_owned()),
        "Issue 270".to_owned(),
        "issue-270".to_owned(),
        PathBuf::from("/tmp/issue-270"),
    );
    repository.default_agent_kind = kind;
    repository
}

fn state_with_repository(repository: Repository) -> AppState {
    AppState {
        repositories: vec![repository],
        selected_repository_index: Some(0),
        installed_agent_kinds: vec![AgentKind::Llxprt, AgentKind::CodePuppy],
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
        agent_kind: "code_puppy",
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
    let puppy = agent_form_visibility(AgentKind::CodePuppy);
    let llxprt = agent_form_visibility(AgentKind::Llxprt);

    assert!(is_field_visible(AgentFormFocus::CodePuppyVersion, puppy));
    assert!(!is_field_visible(AgentFormFocus::CodePuppyVersion, llxprt));
    assert_eq!(
        next_visible_focus(AgentFormFocus::CodePuppyModel, puppy),
        AgentFormFocus::CodePuppyVersion
    );
    assert_eq!(
        prev_visible_focus(AgentFormFocus::CodePuppyVersion, puppy),
        AgentFormFocus::CodePuppyModel
    );

    let mut state = state_with_repository(repository(AgentKind::CodePuppy))
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-270".to_owned())));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    fields.code_puppy_version = "0.0.361-rc1".to_owned();
    fields.agent_kind = AgentKind::Llxprt.label().to_owned();
    assert!(
        !agent_form_content_lines(&state)
            .value_or_panic("agent form content")
            .iter()
            .any(|line| line.contains("0.0.361-rc1"))
    );

    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should remain open");
    };
    fields.agent_kind = AgentKind::CodePuppy.label().to_owned();
    assert!(
        agent_form_content_lines(&state)
            .value_or_panic("agent form content")
            .iter()
            .any(|line| line.contains("Version") && line.contains("0.0.361-rc1"))
    );
}

#[test]
fn repository_default_version_is_code_puppy_only_focusable_and_draft_is_retained() {
    assert!(is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyVersion,
        AgentKind::CodePuppy
    ));
    assert!(!is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyVersion,
        AgentKind::Llxprt
    ));
    assert_eq!(
        next_visible_repository_focus(RepositoryFormFocus::DefaultAgentKind, AgentKind::CodePuppy),
        RepositoryFormFocus::DefaultCodePuppyVersion
    );

    let mut state = state_with_repository(repository(AgentKind::CodePuppy)).apply(
        AppEvent::OpenEditRepository(RepositoryId("repo-270".to_owned())),
    );
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("edit-repository modal should be open");
    };
    fields.default_code_puppy_version = "0.0.361".to_owned();
    fields.default_agent_kind = AgentKind::Llxprt.label().to_owned();
    assert!(
        !repository_form_content_lines(&state)
            .value_or_panic("repository form content")
            .iter()
            .any(|line| line.contains("0.0.361"))
    );

    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("edit-repository modal should remain open");
    };
    fields.default_agent_kind = AgentKind::CodePuppy.label().to_owned();
    fields.default_code_puppy_version = "  0.0.361  ".to_owned();
    assert!(
        repository_form_content_lines(&state)
            .value_or_panic("repository form content")
            .iter()
            .any(|line| line.contains("Default Version") && line.contains("0.0.361"))
    );
    state = state.apply(AppEvent::SubmitForm);
    assert_eq!(state.repositories[0].default_code_puppy_version, "0.0.361");
}

#[test]
fn create_and_edit_mappings_trim_code_puppy_versions() {
    let repository = repository(AgentKind::CodePuppy);
    let agent = create_agent(create_params(&repository, "  0.0.361-rc1  "))
        .value_or_panic("valid Code Puppy agent");
    assert_eq!(agent.code_puppy_version, "0.0.361-rc1");

    let mut state = state_with_repository(repository);
    state.agents.push(agent);
    let agent_id = state.agents[0].id.clone();
    state = state.apply(AppEvent::OpenEditAgent(agent_id));
    let ModalState::EditAgent { fields, .. } = &mut state.modal else {
        panic!("edit-agent modal should be open");
    };
    fields.code_puppy_version = "  nightly  ".to_owned();
    state = state.apply(AppEvent::SubmitForm);
    assert_eq!(state.agents[0].code_puppy_version, "nightly");
}

#[test]
fn repository_default_copies_once_into_new_persistent_and_transient_code_puppy_agents() {
    let mut repository = repository(AgentKind::CodePuppy);
    repository.default_code_puppy_version = "  0.0.361  ".to_owned();

    let persistent = create_agent(create_params(&repository, ""))
        .value_or_panic("valid persistent Code Puppy agent");
    let transient = Agent::new_transient(
        AgentId("transient-270".to_owned()),
        repository.id.clone(),
        repository.effective_transient_dir().join("transient-270"),
        &repository,
    );
    assert_eq!(persistent.code_puppy_version, "0.0.361");
    assert_eq!(transient.code_puppy_version, "0.0.361");

    repository.default_code_puppy_version = "later".to_owned();
    assert_eq!(persistent.code_puppy_version, "0.0.361");
    assert_eq!(transient.code_puppy_version, "0.0.361");
}

#[test]
fn llxprt_agents_do_not_copy_code_puppy_repository_default() {
    let mut repository = repository(AgentKind::Llxprt);
    repository.default_code_puppy_version = "0.0.361".to_owned();
    let persistent = create_agent(CreateAgentParams {
        agent_kind: "LLxprt",
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
    assert_eq!(persistent.code_puppy_version, "");
    assert_eq!(agent.code_puppy_version, "");
}

#[test]
fn prospective_launch_carries_trimmed_pin_and_legacy_signature_defaults_blank() {
    let repository = repository(AgentKind::CodePuppy);
    let signature = prospective_agent_launch(&create_params(&repository, "  0.0.361-rc1  "))
        .value_or_panic("valid prospective launch");
    assert_eq!(signature.code_puppy_version, "0.0.361-rc1");

    let value = serde_json::to_value(&signature).value_or_panic("serialize launch signature");
    let mut object = value
        .as_object()
        .cloned()
        .value_or_panic("signature object");
    object.remove("code_puppy_version");
    let legacy: LaunchSignature =
        serde_json::from_value(object.into()).value_or_panic("legacy signature should deserialize");
    assert_eq!(legacy.code_puppy_version, "");
}

#[test]
fn legacy_missing_code_puppy_version_fields_deserialize_blank() {
    let repository_value = serde_json::to_value(repository(AgentKind::CodePuppy))
        .value_or_panic("serialize repository");
    let mut repository_object = repository_value
        .as_object()
        .cloned()
        .value_or_panic("repository object");
    repository_object.remove("default_code_puppy_version");
    let restored_repository: Repository = serde_json::from_value(repository_object.into())
        .value_or_panic("legacy repository should deserialize");
    assert_eq!(restored_repository.default_code_puppy_version, "");

    let agent = Agent::new(
        AgentId("legacy-agent-270".to_owned()),
        restored_repository.id,
        "Legacy".to_owned(),
        PathBuf::from("/tmp/legacy-agent-270"),
    );
    let agent_value = serde_json::to_value(agent).value_or_panic("serialize agent");
    let mut agent_object = agent_value
        .as_object()
        .cloned()
        .value_or_panic("agent object");
    agent_object.remove("code_puppy_version");
    let restored_agent: Agent = serde_json::from_value(agent_object.into())
        .value_or_panic("legacy agent should deserialize");
    assert_eq!(restored_agent.code_puppy_version, "");
}
