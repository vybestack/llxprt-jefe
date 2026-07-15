use std::path::PathBuf;

use jefe::domain::{
    Agent, AgentId, AgentKind, LaunchSignature, LlxprtNpmPackageSelector, RemoteRepositorySettings,
    Repository, RepositoryId, SandboxEngine,
};
use jefe::selection::agent_form_content_lines;
use jefe::state::{AgentFormFocus, AppEvent, AppState, ModalState};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const NIGHTLY_SELECTOR: &str = "0.10.0-nightly.260712.21cb698b6";

fn selector(value: &str) -> LlxprtNpmPackageSelector {
    LlxprtNpmPackageSelector::normalize(value)
        .unwrap_or_else(|| panic!("selector fixture must be nonblank"))
}

fn selector_value(value: Option<&LlxprtNpmPackageSelector>) -> Option<&str> {
    value.map(LlxprtNpmPackageSelector::as_str)
}

fn remote_repository() -> Repository {
    let mut repository = Repository::new(
        RepositoryId("repo-269-behavior".to_owned()),
        "Issue 269".to_owned(),
        "issue-269".to_owned(),
        PathBuf::from("/remote/work"),
    );
    repository.remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "builder".to_owned(),
        host: "example.test".to_owned(),
        ..RemoteRepositorySettings::default()
    };
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

fn agent_for(repository: &Repository, id: &str, version: Option<&str>) -> Agent {
    let mut agent = Agent::new(
        AgentId(id.to_owned()),
        repository.id.clone(),
        id.to_owned(),
        repository.base_dir.join(id),
    );
    agent.llxprt_version = version.and_then(LlxprtNpmPackageSelector::normalize);
    agent
}

fn set_new_agent_fields(state: &mut AppState, name: &str, version: &str) {
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    name.clone_into(&mut fields.name);
    version.clone_into(&mut fields.llxprt_version);
}

fn set_edit_agent_version(state: &mut AppState, version: &str) {
    let ModalState::EditAgent { fields, .. } = &mut state.modal else {
        panic!("edit-agent modal should be open");
    };
    version.clone_into(&mut fields.llxprt_version);
}

fn set_edit_repository_default(state: &mut AppState, version: &str) {
    let ModalState::EditRepository { fields, .. } = &mut state.modal else {
        panic!("edit-repository modal should be open");
    };
    version.clone_into(&mut fields.default_llxprt_version);
}

#[test]
fn new_agent_submit_form_trims_persists_and_edit_agent_reopens_with_nonblank_llxprt_selector() {
    let repository = remote_repository();
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository).apply(AppEvent::OpenNewAgent(repository_id));
    set_new_agent_fields(&mut state, "nightly agent", "  nightly  ");

    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.agents.len(), 1);
    assert_eq!(
        selector_value(state.agents[0].llxprt_version.as_ref()),
        Some("nightly")
    );

    let created_agent_id = state.agents[0].id.clone();
    state = state.apply(AppEvent::OpenEditAgent(created_agent_id));
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("persisted new agent should reopen in Edit Agent");
    };
    assert_eq!(fields.llxprt_version, "nightly");
}

#[test]
fn new_agent_submit_form_clears_whitespace_only_llxprt_selector_to_none() {
    let mut repository = remote_repository();
    repository.default_llxprt_version = Some(selector("old-selector"));
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository).apply(AppEvent::OpenNewAgent(repository_id));
    set_new_agent_fields(&mut state, "direct agent", " \t\n ");

    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].llxprt_version.is_none());
}

#[test]
fn edit_agent_submit_form_trims_updates_and_clears_llxprt_selector() {
    let repository = remote_repository();
    let agent = agent_for(&repository, "agent-edit", Some("stable"));
    let agent_id = agent.id.clone();
    let mut state = state_with_repository(repository);
    state.agents.push(agent);

    state = state.apply(AppEvent::OpenEditAgent(agent_id.clone()));
    set_edit_agent_version(&mut state, "  nightly  ");
    state = state.apply(AppEvent::SubmitForm);
    assert_eq!(
        selector_value(state.agents[0].llxprt_version.as_ref()),
        Some("nightly")
    );

    state = state.apply(AppEvent::OpenEditAgent(agent_id));
    set_edit_agent_version(&mut state, "  \t ");
    state = state.apply(AppEvent::SubmitForm);
    assert!(state.agents[0].llxprt_version.is_none());
}

#[test]
fn edit_agent_runtime_switch_to_code_puppy_and_back_retains_hidden_llxprt_selector_draft() {
    let repository = remote_repository();
    let agent = agent_for(&repository, "agent-runtime-switch", Some("stable"));
    let agent_id = agent.id.clone();
    let mut state = state_with_repository(repository);
    state.agents.push(agent);
    state = state.apply(AppEvent::OpenEditAgent(agent_id));

    let ModalState::EditAgent { fields, focus, .. } = &mut state.modal else {
        panic!("edit-agent modal should be open");
    };
    fields.llxprt_version = NIGHTLY_SELECTOR.to_owned();
    *focus = AgentFormFocus::AgentKind;

    state = state.apply(AppEvent::FormToggleCheckbox);
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("edit-agent modal should remain open after runtime switch");
    };
    assert_eq!(fields.agent_kind, "code_puppy");
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);

    state = state.apply(AppEvent::FormToggleCheckbox);
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("edit-agent modal should remain open after switching back");
    };
    assert_eq!(fields.agent_kind, "LLxprt");
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);
}

#[test]
fn edit_repository_submit_form_trims_updates_and_clears_default_without_changing_existing_agents() {
    let mut repository = remote_repository();
    repository.default_llxprt_version = Some(selector("repository-old"));
    let repository_id = repository.id.clone();
    let agent = agent_for(&repository, "existing-agent", Some("agent-pinned"));
    let mut state = state_with_repository(repository);
    state.agents.push(agent);

    state = state.apply(AppEvent::OpenEditRepository(repository_id.clone()));
    set_edit_repository_default(&mut state, "  nightly  ");
    state = state.apply(AppEvent::SubmitForm);
    assert_eq!(
        selector_value(state.repositories[0].default_llxprt_version.as_ref()),
        Some("nightly")
    );
    assert_eq!(
        selector_value(state.agents[0].llxprt_version.as_ref()),
        Some("agent-pinned")
    );

    state = state.apply(AppEvent::OpenEditRepository(repository_id));
    set_edit_repository_default(&mut state, " \n\t ");
    state = state.apply(AppEvent::SubmitForm);
    assert!(state.repositories[0].default_llxprt_version.is_none());
    assert_eq!(
        selector_value(state.agents[0].llxprt_version.as_ref()),
        Some("agent-pinned")
    );
}

#[test]
fn new_agent_created_after_repository_edit_copies_updated_default_llxprt_selector() {
    let mut repository = remote_repository();
    repository.default_llxprt_version = Some(selector("stable"));
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository);

    state = state.apply(AppEvent::OpenEditRepository(repository_id.clone()));
    set_edit_repository_default(&mut state, &format!("  {NIGHTLY_SELECTOR}  "));
    state = state.apply(AppEvent::SubmitForm);
    state = state.apply(AppEvent::OpenNewAgent(repository_id));

    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);
    fields.name = "later agent".to_owned();
    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.agents.len(), 1);
    assert_eq!(
        selector_value(state.agents[0].llxprt_version.as_ref()),
        Some(NIGHTLY_SELECTOR)
    );
}

fn with_selector_field(mut base: Value, field: &str, value: Option<Value>) -> Value {
    let Value::Object(fields) = &mut base else {
        panic!("serde fixture should be a JSON object");
    };
    if let Some(value) = value {
        fields.insert(field.to_owned(), value);
    } else {
        fields.remove(field);
    }
    base
}

fn assert_selector_serde_compatibility<T>(base: Value, field: &str)
where
    T: DeserializeOwned + Serialize,
{
    for (case, input) in [
        ("missing", None),
        ("null", Some(Value::Null)),
        ("blank", Some(Value::String(" \t\n ".to_owned()))),
    ] {
        let decoded: T = serde_json::from_value(with_selector_field(base.clone(), field, input))
            .unwrap_or_else(|error| panic!("{case} selector should deserialize: {error}"));
        let encoded = serde_json::to_value(decoded)
            .unwrap_or_else(|error| panic!("{case} selector should serialize: {error}"));
        assert_eq!(
            encoded.get(field),
            Some(&Value::Null),
            "{case} selector should normalize to null"
        );
    }

    let decoded: T = serde_json::from_value(with_selector_field(
        base,
        field,
        Some(Value::String(format!("  {NIGHTLY_SELECTOR}  "))),
    ))
    .unwrap_or_else(|error| panic!("nonblank selector should deserialize: {error}"));
    let encoded = serde_json::to_value(&decoded)
        .unwrap_or_else(|error| panic!("normalized selector should serialize: {error}"));
    assert_eq!(
        encoded.get(field),
        Some(&Value::String(NIGHTLY_SELECTOR.to_owned()))
    );

    let round_trip: T = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("nightly selector should round trip: {error}"));
    let round_trip_value = serde_json::to_value(round_trip)
        .unwrap_or_else(|error| panic!("round-tripped selector should serialize: {error}"));
    assert_eq!(
        round_trip_value.get(field),
        Some(&Value::String(NIGHTLY_SELECTOR.to_owned()))
    );
}

#[test]
fn agent_serde_selector_missing_null_blank_and_nonblank_normalize_and_round_trip_exact_nightly() {
    let repository = remote_repository();
    let agent = agent_for(&repository, "agent-serde", None);
    let base = serde_json::to_value(agent)
        .unwrap_or_else(|error| panic!("agent fixture should serialize: {error}"));

    assert_selector_serde_compatibility::<Agent>(base, "llxprt_version");
}

#[test]
fn repository_serde_selector_missing_null_blank_and_nonblank_normalize_and_round_trip_exact_nightly()
 {
    let base = serde_json::to_value(remote_repository())
        .unwrap_or_else(|error| panic!("repository fixture should serialize: {error}"));

    assert_selector_serde_compatibility::<Repository>(base, "default_llxprt_version");
}

#[test]
fn launch_signature_serde_selector_missing_null_blank_and_nonblank_normalize_and_round_trip_exact_nightly()
 {
    let signature = LaunchSignature {
        work_dir: PathBuf::from("/remote/work/serde"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: vec!["--yolo".to_owned()],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
        llxprt_version: None,
    };
    let base = serde_json::to_value(signature)
        .unwrap_or_else(|error| panic!("launch signature fixture should serialize: {error}"));

    assert_selector_serde_compatibility::<LaunchSignature>(base, "llxprt_version");
}

fn projection_state(agent_kind: AgentKind) -> AppState {
    let repository = remote_repository();
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository).apply(AppEvent::OpenNewAgent(repository_id));
    let ModalState::NewAgent { fields, focus, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    "Projection Agent".clone_into(&mut fields.name);
    "projection description".clone_into(&mut fields.description);
    "/remote/work/projection-agent".clone_into(&mut fields.work_dir);
    "reviewer".clone_into(&mut fields.profile);
    agent_kind.label().clone_into(&mut fields.agent_kind);
    "sonnet".clone_into(&mut fields.code_puppy_model);
    fields.code_puppy_yolo = true;
    fields.code_puppy_quick_resume = true.into();
    "--yolo".clone_into(&mut fields.mode);
    NIGHTLY_SELECTOR.clone_into(&mut fields.llxprt_version);
    "trace".clone_into(&mut fields.llxprt_debug);
    *focus = AgentFormFocus::Shortcut;
    state
}

#[test]
fn code_puppy_selection_content_projection_matches_ui_order_and_format_without_llxprt_version() {
    let state = projection_state(AgentKind::CodePuppy);
    let lines = agent_form_content_lines(&state)
        .unwrap_or_else(|| panic!("new-agent selection content should project"));

    assert_eq!(
        &lines[6..10],
        [
            "  Agent Runtime    [code_puppy]  (space cycles: LLxprt / code_puppy)",
            "  Model            [sonnet]",
            "  YOLO             [x]  (space toggles)",
            "  Quick resume     [x]  (space toggles)",
        ]
    );
    assert!(
        lines.iter().all(|line| !line.starts_with("  Version")),
        "Code Puppy projection must omit the LLxprt Version row"
    );
}

#[test]
fn llxprt_selection_content_projection_retains_version_and_omits_code_puppy_rows() {
    let state = projection_state(AgentKind::Llxprt);
    let lines = agent_form_content_lines(&state)
        .unwrap_or_else(|| panic!("new-agent selection content should project"));

    assert_eq!(
        &lines[7..11],
        [
            "  Agent Runtime    [LLxprt]  (space cycles: LLxprt / code_puppy)",
            "  Mode Flags       [--yolo]",
            "  Version          [0.10.0-nightly.260712.21cb698b6]",
            "  LLXPRT_DEBUG     [trace]",
        ]
    );
    assert!(lines.iter().all(|line| {
        !line.starts_with("  Model")
            && !line.starts_with("  YOLO")
            && !line.starts_with("  Quick resume")
    }));
}
