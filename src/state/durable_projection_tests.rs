//! Behavioral tests for the pure AppState <-> StateV2 durable projections
//! (issue #381 S9b-1).
//!
//! The forward projection builds the schema-2 durable candidate the reducer
//! stages as a `PersistState` effect; the inverse restores runtime state
//! fields from a loaded schema-2 document. Both are pure: no filesystem
//! access, no locks, deterministic output for equal input.

use std::path::PathBuf;

use crate::domain::{
    Agent, AgentId, AgentKind, AgentStatus, DormantRecord, Id, LastKnownRuntime, LaunchSignature,
    RemoteRepositorySettings, Repository, RepositoryId, RepositoryLocation, RuntimeBinding,
    UserPreferences,
};
use crate::persistence::state_v2::StateDocument;
use crate::state::durable_projection::to_durable_state;
use crate::state::durable_restore::from_durable_state;
use crate::state::{AppState, PaneFocus};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: value was absent"),
        }
    }
}

fn local_repository(id: &str, name: &str, base_dir: &str) -> Repository {
    Repository {
        id: RepositoryId(id.to_owned()),
        name: name.to_owned(),
        slug: format!("{name}-slug"),
        base_dir: PathBuf::from(base_dir),
        default_profile: "default".to_owned(),
        default_code_puppy_model: String::new(),
        default_code_puppy_version: String::new(),
        github_repo: "acme/widgets".to_owned(),
        github_issue_pr_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: "fix it".to_owned(),
        default_agent_kind: AgentKind::Llxprt,
        transient_agent_dir: PathBuf::new(),
        default_code_puppy_yolo: None,
        default_llxprt_mode_flags: Vec::new(),
        transient_max_concurrent: 2,
        default_llxprt_version: None,
        agent_ids: Vec::new(),
    }
}

fn remote_repository(id: &str) -> Repository {
    let mut repository = local_repository(id, "remote-repo", "/srv/checkout");
    repository.remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "deploy".to_owned(),
        host: "Build.Example.COM".to_owned(),
        port: Some(2222),
        identity_file: PathBuf::from("/home/deploy/.ssh/id_ed25519"),
        options: vec!["-o".to_owned(), "BatchMode=yes".to_owned()],
        run_as_user: "worker".to_owned(),
        setup_env_default: true,
    };
    repository
}

fn agent(id: &str, repository_id: &str, name: &str, work_dir: &str) -> Agent {
    Agent::new(
        AgentId(id.to_owned()),
        RepositoryId(repository_id.to_owned()),
        name.to_owned(),
        PathBuf::from(work_dir),
    )
}

fn running_binding(agent_ref: &Agent, repository: &Repository, session: &str) -> RuntimeBinding {
    RuntimeBinding {
        session_name: session.to_owned(),
        launch_signature: LaunchSignature::for_agent(agent_ref, repository),
        attached: true,
        last_seen: Some(42),
        pid: Some(4242),
        process_identity: None,
        worker_identities: Vec::new(),
        lifecycle_generation: 7,
    }
}

fn sample_state() -> AppState {
    let repo_a = local_repository("repo-a1", "alpha", "/work/alpha");
    let repo_b = remote_repository("repo-b2");
    let mut running = agent("agent-a1", "repo-a1", "runner", "/work/alpha/wt1");
    running.status = AgentStatus::Running;
    running.runtime_binding = Some(running_binding(&running, &repo_a, "jefe-runner"));
    let mut dead = agent("agent-b1", "repo-b2", "stopped", "/srv/checkout/wt");
    dead.status = AgentStatus::Dead;
    let queued = agent("agent-a2", "repo-a1", "waiting", "/work/alpha/wt2");

    let mut state = AppState {
        repositories: vec![repo_a, repo_b],
        agents: vec![running, queued, dead],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        hide_idle_repositories: true,
        pane_focus: PaneFocus::Agents,
        terminal_focused: false,
        durable_revision: 9,
        ..AppState::default()
    };
    state.last_selected_agent_by_repo = vec![
        (
            RepositoryId("repo-a1".to_owned()),
            AgentId("agent-a2".to_owned()),
        ),
        (
            RepositoryId("repo-b2".to_owned()),
            AgentId("agent-b1".to_owned()),
        ),
    ];
    state.user_preferences = UserPreferences::default();
    state.rebuild_repository_agent_ids();
    state
}

fn transient_agent(repository_id: &str) -> Agent {
    let mut transient = agent("transient-1f", repository_id, "scratch", "/tmp/scratch");
    transient.origin = crate::domain::AgentOrigin::Transient;
    transient
}

#[test]
fn forward_projects_schema2_with_preserved_valid_ids() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");

    assert_eq!(projected.state_schema, 2);
    assert_eq!(projected.revision, 9);
    assert_eq!(projected.repositories.len(), 2);
    assert_eq!(projected.agents.len(), 3);
    assert_eq!(projected.repositories[0].id.as_str(), "repo-a1");
    assert_eq!(projected.agents[0].id.as_str(), "agent-a1");
    assert_eq!(projected.agents[0].repository_id.as_str(), "repo-a1");
}

#[test]
fn forward_output_parses_as_strict_schema2_document() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let mut encoded = serde_json::to_vec_pretty(&projected).value_or_panic("serialize candidate");
    encoded.push(b'\n');

    let document = StateDocument::parse(&encoded).value_or_panic("strict schema-2 parse");
    assert_eq!(document.state(), &projected);
}

#[test]
fn forward_excludes_transient_agents_and_their_references() {
    let mut state = sample_state();
    state.agents.push(transient_agent("repo-a1"));
    state.last_selected_agent_by_repo = vec![(
        RepositoryId("repo-a1".to_owned()),
        AgentId("transient-1f".to_owned()),
    )];
    state.selected_agent_index = Some(3);
    state.rebuild_repository_agent_ids();

    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    assert_eq!(projected.agents.len(), 3);
    assert!(
        projected
            .agents
            .iter()
            .all(|agent| agent.id.as_str() != "transient-1f")
    );
    assert!(projected.last_selected_agent_by_repo.is_empty());
    assert_eq!(projected.selection.agent_id, None);
}

#[test]
fn forward_maps_selection_indices_to_ids() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");

    let selection = &projected.selection;
    assert_eq!(
        selection.repository_id.as_ref().map(Id::as_str),
        Some("repo-a1")
    );
    assert_eq!(
        selection.agent_id.as_ref().map(Id::as_str),
        Some("agent-a2")
    );
    assert_eq!(selection.screen_id, None);
}

#[test]
fn forward_maps_runtime_status_to_last_known() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");

    let running = &projected.agents[0].runtime;
    assert_eq!(running.last_known, LastKnownRuntime::Running);
    assert_eq!(running.session_id.as_deref(), Some("jefe-runner"));
    assert_eq!(running.invocation_generation, 7);

    let queued = &projected.agents[1].runtime;
    assert_eq!(queued.last_known, LastKnownRuntime::Unknown);
    assert_eq!(queued.session_id, None);

    let dead = &projected.agents[2].runtime;
    assert_eq!(dead.last_known, LastKnownRuntime::Stopped);
}

#[test]
fn forward_rewrites_invalid_ids_deterministically_with_remapped_references() {
    let mut state = sample_state();
    state.repositories[0].id = RepositoryId("Bad Repo!".to_owned());
    for agent in &mut state.agents {
        if agent.repository_id.0 == "repo-a1" {
            agent.repository_id = RepositoryId("Bad Repo!".to_owned());
        }
    }
    state.agents[0].id = AgentId(String::new());
    state.last_selected_agent_by_repo = vec![(
        RepositoryId("Bad Repo!".to_owned()),
        AgentId("agent-a2".to_owned()),
    )];
    state.rebuild_repository_agent_ids();

    let first = to_durable_state(&state).value_or_panic("projection succeeds");
    let second = to_durable_state(&state).value_or_panic("projection is deterministic");
    assert_eq!(first, second);

    let rewritten_repo = first.repositories[0].id.clone();
    assert!(rewritten_repo.as_str().starts_with("repo."));
    assert_eq!(first.agents[0].repository_id, rewritten_repo);
    assert!(first.agents[0].id.as_str().starts_with("agent."));
    assert_eq!(
        first.last_selected_agent_by_repo.keys().next(),
        Some(&rewritten_repo)
    );

    let mut encoded = serde_json::to_vec_pretty(&first).value_or_panic("serialize candidate");
    encoded.push(b'\n');
    StateDocument::parse(&encoded).value_or_panic("rewritten ids remain strict schema-2");
}

#[test]
fn forward_preserves_remote_location_and_dormant_records() {
    let mut state = sample_state();
    state.dormant_records = vec![DormantRecord {
        kind: "schema1.root.legacy-flag".to_owned(),
        stable_id: None,
        raw_schema: 1,
        reason: "schema-1 owner or field is unavailable in schema 2".to_owned(),
        raw_value: serde_json::json!({"enabled": true}),
    }];

    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    assert_eq!(projected.dormant_records, state.dormant_records);
    match &projected.repositories[1].location {
        RepositoryLocation::Remote(remote) => {
            assert!(remote.remote_target.contains("build.example.com"));
            assert!(remote.remote_target.contains("/srv/checkout"));
        }
        RepositoryLocation::Local(local) => {
            panic!("expected remote location, found local {}", local.local_path)
        }
    }
}

#[test]
fn inverse_restores_runtime_fields_from_projection() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    assert_eq!(restored.revision, 9);
    assert_eq!(restored.repositories.len(), 2);
    assert_eq!(restored.agents.len(), 3);
    assert_eq!(restored.selected_repository_index, Some(0));
    assert_eq!(restored.selected_agent_index, Some(1));
    assert!(restored.hide_idle_repositories);
    assert_eq!(restored.pane_focus, PaneFocus::Agents);
    assert!(!restored.terminal_focused);
    assert_eq!(restored.last_selected_agent_by_repo.len(), 2);
    assert_eq!(restored.repositories[0].id.0, "repo-a1");
    assert_eq!(restored.repositories[0].name, "alpha");
    assert_eq!(restored.repositories[0].slug, "alpha-slug");
    assert_eq!(
        restored.repositories[0].base_dir,
        PathBuf::from("/work/alpha")
    );
}

#[test]
fn inverse_restores_remote_settings_including_base_dir() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    let remote_repo = &restored.repositories[1];
    assert!(remote_repo.remote.enabled);
    assert_eq!(remote_repo.remote.login_user, "deploy");
    assert_eq!(remote_repo.remote.host, "Build.Example.COM");
    assert_eq!(remote_repo.remote.port, Some(2222));
    assert_eq!(remote_repo.remote.run_as_user, "worker");
    assert!(remote_repo.remote.setup_env_default);
    assert_eq!(remote_repo.base_dir, PathBuf::from("/srv/checkout"));
    assert_eq!(
        remote_repo.remote.identity_file,
        PathBuf::from("/home/deploy/.ssh/id_ed25519")
    );
    assert_eq!(
        remote_repo.remote.options,
        vec!["-o".to_owned(), "BatchMode=yes".to_owned()]
    );
}

#[test]
fn inverse_synthesizes_status_and_binding_from_last_known() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    let running = &restored.agents[0];
    assert_eq!(running.status, AgentStatus::Running);
    let binding = running
        .runtime_binding
        .as_ref()
        .value_or_panic("running agent restores a binding");
    assert_eq!(binding.session_name, "jefe-runner");
    assert_eq!(binding.lifecycle_generation, 7);
    assert_eq!(binding.pid, None);
    let expected = LaunchSignature::for_agent(&restored.agents[0], &restored.repositories[0]);
    assert_eq!(binding.launch_signature, expected);

    assert_eq!(restored.agents[1].status, AgentStatus::Queued);
    assert!(restored.agents[1].runtime_binding.is_none());
    assert_eq!(restored.agents[2].status, AgentStatus::Dead);
    assert!(restored.agents[2].runtime_binding.is_none());
}

#[test]
fn round_trip_is_idempotent_in_canonical_bytes() {
    let mut state = sample_state();
    state.dormant_records = vec![DormantRecord {
        kind: "schema1.agent.extra".to_owned(),
        stable_id: None,
        raw_schema: 1,
        reason: "schema-1 owner or field is unavailable in schema 2".to_owned(),
        raw_value: serde_json::json!("legacy"),
    }];
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    let mut second_state = AppState {
        repositories: restored.repositories,
        agents: restored.agents,
        selected_repository_index: restored.selected_repository_index,
        selected_agent_index: restored.selected_agent_index,
        hide_idle_repositories: restored.hide_idle_repositories,
        pane_focus: restored.pane_focus,
        terminal_focused: restored.terminal_focused,
        durable_revision: restored.revision,
        dormant_records: restored.dormant_records,
        ..AppState::default()
    };
    second_state.last_selected_agent_by_repo = restored.last_selected_agent_by_repo;
    second_state.user_preferences = restored.user_preferences;
    let reprojected = to_durable_state(&second_state).value_or_panic("second projection");

    let first_bytes = serde_json::to_vec_pretty(&projected).value_or_panic("first bytes");
    let second_bytes = serde_json::to_vec_pretty(&reprojected).value_or_panic("second bytes");
    assert_eq!(first_bytes, second_bytes);
}

#[test]
fn inverse_restores_user_preferences_round_trip() {
    let mut state = sample_state();
    let repo_id = RepositoryId("repo-a1".to_owned());
    let mut preferences = state.user_preferences.for_repo(&repo_id);
    preferences.issue_search_query = "roadmap".to_owned();
    preferences.issue_filter_field_index = 3;
    state
        .user_preferences
        .update_for_repo(&repo_id, preferences);

    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    let restored_preferences = restored.user_preferences.for_repo(&repo_id);
    assert_eq!(restored_preferences.issue_search_query, "roadmap");
    assert_eq!(restored_preferences.issue_filter_field_index, 3);
}

#[test]
fn inverse_rejects_agent_with_unknown_repository() {
    let state = sample_state();
    let mut projected = to_durable_state(&state).value_or_panic("projection succeeds");
    projected.agents[0].repository_id =
        crate::domain::Id::parse("repo.unknown").value_or_panic("valid id");

    assert!(from_durable_state(&projected).is_err());
}

/// Two runtime repositories must never collapse onto one durable identifier.
/// A repository whose runtime id is already a valid `Id` is preserved verbatim,
/// so it can equal the id minted for a different repository whose runtime id
/// was unusable. The projection must refuse rather than emit a document with
/// duplicate ids (issue #381).
#[test]
fn forward_rejects_two_repositories_that_share_one_durable_id() {
    let mut state = sample_state();
    // "Repo One" cannot parse as an Id, so it mints to this exact digest.
    let minted = "repo.165ed0b47b52ea93c2ec1b4fd8b4a5e4becd6a6dc49ca1c6b7542df112fa62bc";
    state.repositories = vec![
        local_repository("Repo One", "minted", "/srv/one"),
        local_repository(minted, "preserved", "/srv/two"),
    ];
    state.agents.clear();
    state.selected_repository_index = None;
    state.selected_agent_index = None;
    state.last_selected_agent_by_repo.clear();
    state.rebuild_repository_agent_ids();

    let projected = to_durable_state(&state);

    assert!(
        projected.is_err(),
        "a durable id shared by two repositories must be refused, got {:?}",
        projected.map(|state| state
            .repositories
            .iter()
            .map(|repository| repository.id.to_string())
            .collect::<Vec<_>>())
    );
}

/// Restoring a remote repository must not resurrect a disabled connection.
/// The forward projection selects `RepositoryLocation::Remote` from
/// `remote.enabled`, so forcing it back to `true` on restore would make a
/// disabled remote impossible to persist (issue #381).
#[test]
fn inverse_preserves_a_disabled_remote_flag() {
    let mut state = sample_state();
    state.repositories = vec![remote_repository("repo-remote")];
    state.agents.clear();
    state.selected_repository_index = Some(0);
    state.selected_agent_index = None;
    state.last_selected_agent_by_repo.clear();
    state.rebuild_repository_agent_ids();

    // A remote-located record whose stored settings say the connection is
    // disabled: exactly what persisting a disabled remote produces.
    let mut durable = to_durable_state(&state).value_or_panic("project the remote repository");
    let values = &mut durable.repositories[0].agent_defaults.values;
    let remote_key = crate::domain::Id::parse("remote").value_or_panic("the remote key");
    let Some(crate::domain::TypedValue::Map(remote)) = values.get_mut(&remote_key) else {
        panic!("the projected repository should carry a remote value map");
    };
    let enabled_key = crate::domain::Id::parse("enabled").value_or_panic("the enabled key");
    let _ = remote.insert(enabled_key, crate::domain::TypedValue::Bool(false));

    let restored = from_durable_state(&durable).value_or_panic("restore the disabled remote");

    assert!(
        !restored.repositories[0].remote.enabled,
        "a stored disabled remote must not be silently re-enabled on restore"
    );
}

/// A non-UTF-8 path cannot be represented in JSON, so the projection must
/// refuse it instead of writing U+FFFD replacement characters that never
/// round-trip back to the original bytes.
#[test]
#[cfg(unix)]
fn forward_refuses_a_work_dir_that_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt;

    let repository = local_repository("r1", "repo", "/srv/repo");
    let invalid = std::ffi::OsStr::from_bytes(b"/srv/repo/bad\xff\xfename");
    let mut broken = agent("a1", "r1", "agent", "/srv/repo");
    broken.work_dir = PathBuf::from(invalid);

    let mut state = AppState::default();
    state.repositories.push(repository);
    state.agents.push(broken);
    state.rebuild_repository_agent_ids();

    let Err(error) = to_durable_state(&state) else {
        panic!("a non-UTF-8 work_dir must not project into a durable document");
    };
    let detail = format!("{error:?}");
    assert!(
        detail.contains("UTF-8") || detail.contains("utf-8") || detail.contains("utf8"),
        "the rejection must explain the encoding problem, got: {detail}"
    );
}

/// A remembered pairing is only durable when the agent actually belongs to the
/// repository it is remembered under. The ownership predicate in
/// `project_last_selected` is not a redundant presence check: both ids resolve
/// successfully here, and only the cross-repository ownership test rejects the
/// stale pairing.
#[test]
fn forward_drops_a_remembered_agent_owned_by_another_repository() {
    let mut state = AppState::default();
    state
        .repositories
        .push(local_repository("r1", "first", "/srv/first"));
    state
        .repositories
        .push(local_repository("r2", "second", "/srv/second"));
    state
        .agents
        .push(agent("a1", "r1", "first agent", "/srv/first"));
    state
        .agents
        .push(agent("a2", "r2", "second agent", "/srv/second"));
    state.rebuild_repository_agent_ids();
    // r1 remembers an agent that lives in r2.
    state.last_selected_agent_by_repo = vec![(
        crate::domain::RepositoryId("r1".to_owned()),
        crate::domain::AgentId("a2".to_owned()),
    )];

    let Ok(durable) = to_durable_state(&state) else {
        panic!("the state must project");
    };
    assert!(
        durable.last_selected_agent_by_repo.is_empty(),
        "a cross-repository remembered selection must not be persisted, got: {:?}",
        durable.last_selected_agent_by_repo
    );
}
