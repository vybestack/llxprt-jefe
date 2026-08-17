#[path = "common/app_state.rs"]
mod common_app_state;

use std::path::PathBuf;

use jefe::domain::canonical_values::{required_id, typed_field};
use jefe::domain::{
    Agent, AgentId, AgentTypeId, LlxprtNpmPackageSelector, RemoteRepositorySettings, Repository,
    RepositoryId, TypedMap, TypedValue,
};
use jefe::selection::agent_form_content_lines;
use jefe::state::transition::TransitionExt;
use jefe::state::{AgentFormFocus, AppEvent, AppState, ModalState};

const NIGHTLY_SELECTOR: &str = "0.10.0-nightly.260712.21cb698b6";

fn set_selector(values: &mut TypedMap, value: Option<&str>) {
    let id = required_id("version-selector")
        .unwrap_or_else(|error| panic!("valid selector field id: {error}"));
    if let Some(value) = value.and_then(LlxprtNpmPackageSelector::normalize) {
        values.insert(id, TypedValue::String(value.as_str().to_owned()));
    } else {
        values.remove(&id);
    }
}

fn typed_selector(values: &TypedMap) -> Option<&str> {
    match typed_field(values, "version_selector") {
        Some(TypedValue::String(value)) => Some(value),
        None => None,
        other => panic!("expected typed version selector, got {other:?}"),
    }
}

fn remote_repository() -> Repository {
    let mut repository = Repository::new(
        RepositoryId("repo-269-behavior".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
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
    {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository];
        state.selected_repository_index = Some(0);
        state.available_agent_type_ids = vec![
            jefe::domain::shipped_agent_type(3),
            jefe::domain::shipped_agent_type(1),
        ];
        state
    }
}

fn agent_for(repository: &Repository, id: &str, version: Option<&str>) -> Agent {
    let mut agent = Agent::new(
        AgentId(id.to_owned()),
        repository.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        id.to_owned(),
        repository.base_dir.join(id),
    );
    set_selector(&mut agent.values, version);
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
    let mut state = state_with_repository(repository)
        .apply(AppEvent::OpenNewAgent(repository_id))
        .committed_pure();
    set_new_agent_fields(&mut state, "nightly agent", "  nightly  ");

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.agents.len(), 1);
    assert_eq!(typed_selector(&state.agents[0].values), Some("nightly"));

    let created_agent_id = state.agents[0].id.clone();
    state = state
        .apply(AppEvent::OpenEditAgent(created_agent_id))
        .committed_pure();
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("persisted new agent should reopen in Edit Agent");
    };
    assert_eq!(fields.llxprt_version, "nightly");
}

#[test]
fn new_agent_submit_form_clears_whitespace_only_llxprt_selector_to_none() {
    let mut repository = remote_repository();
    set_selector(&mut repository.default_values, Some("old-selector"));
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository)
        .apply(AppEvent::OpenNewAgent(repository_id))
        .committed_pure();
    set_new_agent_fields(&mut state, "direct agent", " \t\n ");

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.agents.len(), 1);
    assert_eq!(typed_selector(&state.agents[0].values), Some(""));
}

#[test]
fn edit_agent_submit_form_trims_updates_and_clears_llxprt_selector() {
    let repository = remote_repository();
    let agent = agent_for(&repository, "agent-edit", Some("stable"));
    let agent_id = agent.id.clone();
    let mut state = state_with_repository(repository);
    state.agents.push(agent);

    state = state
        .apply(AppEvent::OpenEditAgent(agent_id.clone()))
        .committed_pure();
    set_edit_agent_version(&mut state, "  nightly  ");
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(typed_selector(&state.agents[0].values), Some("nightly"));

    state = state
        .apply(AppEvent::OpenEditAgent(agent_id))
        .committed_pure();
    set_edit_agent_version(&mut state, "  \t ");
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(typed_selector(&state.agents[0].values), Some(""));
}

#[test]
fn edit_agent_runtime_switch_to_code_puppy_and_back_retains_hidden_llxprt_selector_draft() {
    let repository = remote_repository();
    let agent = agent_for(&repository, "agent-runtime-switch", Some("stable"));
    let agent_id = agent.id.clone();
    let mut state = state_with_repository(repository);
    state.agents.push(agent);
    state = state
        .apply(AppEvent::OpenEditAgent(agent_id))
        .committed_pure();

    let ModalState::EditAgent { fields, focus, .. } = &mut state.modal else {
        panic!("edit-agent modal should be open");
    };
    fields.llxprt_version = NIGHTLY_SELECTOR.to_owned();
    *focus = AgentFormFocus::AgentType;

    state = state.apply(AppEvent::FormToggleCheckbox).committed_pure();
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("edit-agent modal should remain open after runtime switch");
    };
    assert_eq!(fields.agent_type_id, "core.code-puppy");
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);

    state = state.apply(AppEvent::FormToggleCheckbox).committed_pure();
    let ModalState::EditAgent { fields, .. } = &state.modal else {
        panic!("edit-agent modal should remain open after switching back");
    };
    assert_eq!(fields.agent_type_id, "core.claude-code");
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);
}

#[test]
fn edit_repository_submit_form_trims_updates_and_clears_default_without_changing_existing_agents() {
    let mut repository = remote_repository();
    set_selector(&mut repository.default_values, Some("repository-old"));
    let repository_id = repository.id.clone();
    let agent = agent_for(&repository, "existing-agent", Some("agent-pinned"));
    let mut state = state_with_repository(repository);
    state.agents.push(agent);

    state = state
        .apply(AppEvent::OpenEditRepository(repository_id.clone()))
        .committed_pure();
    set_edit_repository_default(&mut state, "  nightly  ");
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        typed_selector(&state.repositories[0].default_values),
        Some("nightly")
    );
    assert_eq!(
        typed_selector(&state.agents[0].values),
        Some("agent-pinned")
    );

    state = state
        .apply(AppEvent::OpenEditRepository(repository_id))
        .committed_pure();
    set_edit_repository_default(&mut state, " \n\t ");
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    assert_eq!(
        typed_selector(&state.repositories[0].default_values),
        Some("")
    );
    assert_eq!(
        typed_selector(&state.agents[0].values),
        Some("agent-pinned")
    );
}

#[test]
fn new_agent_created_after_repository_edit_copies_updated_default_llxprt_selector() {
    let mut repository = remote_repository();
    set_selector(&mut repository.default_values, Some("stable"));
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository);

    state = state
        .apply(AppEvent::OpenEditRepository(repository_id.clone()))
        .committed_pure();
    set_edit_repository_default(&mut state, &format!("  {NIGHTLY_SELECTOR}  "));
    state = state.apply(AppEvent::SubmitForm).committed_pure();
    state = state
        .apply(AppEvent::OpenNewAgent(repository_id))
        .committed_pure();

    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    assert_eq!(fields.llxprt_version, NIGHTLY_SELECTOR);
    fields.name = "later agent".to_owned();
    state = state.apply(AppEvent::SubmitForm).committed_pure();

    assert_eq!(state.agents.len(), 1);
    assert_eq!(
        typed_selector(&state.agents[0].values),
        Some(NIGHTLY_SELECTOR)
    );
}

fn projection_state(type_id: AgentTypeId) -> AppState {
    let repository = remote_repository();
    let repository_id = repository.id.clone();
    let mut state = state_with_repository(repository)
        .apply(AppEvent::OpenNewAgent(repository_id))
        .committed_pure();
    let ModalState::NewAgent { fields, focus, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    "Projection Agent".clone_into(&mut fields.name);
    "projection description".clone_into(&mut fields.description);
    "/remote/work/projection-agent".clone_into(&mut fields.work_dir);
    "reviewer".clone_into(&mut fields.profile);
    type_id.to_string().clone_into(&mut fields.agent_type_id);
    "sonnet".clone_into(&mut fields.code_puppy_model);
    "puppy-nightly".clone_into(&mut fields.code_puppy_version);
    fields.code_puppy_yolo = true;
    fields.code_puppy_quick_resume = true.into();
    "--yolo".clone_into(&mut fields.mode);
    NIGHTLY_SELECTOR.clone_into(&mut fields.llxprt_version);
    "trace".clone_into(&mut fields.llxprt_debug);
    *focus = AgentFormFocus::Shortcut;
    state
}

#[test]
fn code_puppy_selection_content_projection_shows_puppy_version_without_llxprt_version() {
    let state = projection_state(jefe::domain::shipped_agent_type(1));
    let lines = agent_form_content_lines(&state)
        .unwrap_or_else(|| panic!("new-agent selection content should project"));

    assert_eq!(
        &lines[6..11],
        [
            "  Agent Runtime    [core.code-puppy]  (space cycles: LLxprt / Code Puppy / Claude Code / Codex CLI)",
            "  Model            [sonnet]",
            "  Version          [puppy-nightly]",
            "  YOLO             [x]  (space toggles)",
            "  Quick resume     [x]  (space toggles)",
        ]
    );
    assert!(
        lines.iter().all(|line| !line.contains(NIGHTLY_SELECTOR)),
        "Code Puppy projection must omit the dormant LLxprt Version value"
    );
}

#[test]
fn llxprt_selection_content_projection_retains_version_and_omits_code_puppy_rows() {
    let state = projection_state(jefe::domain::shipped_agent_type(3));
    let lines = agent_form_content_lines(&state)
        .unwrap_or_else(|| panic!("new-agent selection content should project"));

    assert_eq!(
        &lines[7..11],
        [
            "  Agent Runtime    [core.llxprt]  (space cycles: LLxprt / Code Puppy / Claude Code / Codex CLI)",
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
