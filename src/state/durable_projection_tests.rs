//! Behavioral tests for the pure AppState <-> StateV2 durable projections
//! (issue #381 S9b-1).
//!
//! The forward projection builds the schema-2 durable candidate the reducer
//! stages as a `PersistState` effect; the inverse restores runtime state
//! fields from a loaded schema-2 document. Both are pure: no filesystem
//! access, no locks, deterministic output for equal input.

use std::path::PathBuf;

use crate::domain::{
    Agent, AgentId, AgentStatus, DormantRecord, Id, LastKnownRuntime, LaunchSignatureV1,
    PaneProcessIdentity, RemoteRepositorySettings, Repository, RepositoryId, RepositoryLocation,
    RuntimeBinding, UserPreferences, WorkerProcessIdentity,
};
use crate::persistence::state_v2::StateDocument;
use crate::state::durable_projection::{current_launch_signature, to_durable_state};
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
    let mut repository = Repository::new(
        RepositoryId(id.to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        name.to_owned(),
        format!("{name}-slug"),
        PathBuf::from(base_dir),
    );
    repository.github_repo = "acme/widgets".to_owned();
    repository.issue_base_prompt = "fix it".to_owned();
    repository.transient_max_concurrent = 2;
    repository
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
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        name.to_owned(),
        PathBuf::from(work_dir),
    )
}

fn running_binding(agent_ref: &Agent, repository: &Repository, session: &str) -> RuntimeBinding {
    RuntimeBinding {
        session_name: session.to_owned(),
        launch_signature: current_launch_signature(agent_ref, repository)
            .value_or_panic("durable signature"),
        attached: true,
        last_seen: Some(42),
        pane_identity: Some(PaneProcessIdentity::from_pid(4242)),
        worker_identity: None,
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
    assert_eq!(
        selection.screen_id.as_ref().map(Id::as_str),
        Some("core.dashboard")
    );
}

#[test]
fn forward_writes_the_active_screen_as_its_stable_identity() {
    let mut state = sample_state();
    state.nav =
        crate::state::navigation::NavState::rooted(crate::workbench::ScreenId::PullRequests);
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    assert_eq!(
        projected.selection.screen_id.as_ref().map(Id::as_str),
        Some("github.pull-requests"),
        "the document must carry the stable identity, not an ordinal"
    );
}

#[test]
fn the_active_screen_round_trips_through_the_durable_document() {
    for screen in crate::workbench::ScreenId::ALL {
        let mut state = sample_state();
        state.nav = crate::state::navigation::NavState::rooted(screen);
        let projected = to_durable_state(&state).value_or_panic("projection succeeds");
        let restored = crate::state::durable_restore::from_durable_state(&projected)
            .value_or_panic("restore succeeds");
        assert_eq!(restored.screen, screen, "screen {screen} must round-trip");
    }
}

#[test]
fn a_legacy_variant_name_cannot_reach_the_durable_screen_slot() {
    // The durable slot is an `Id`, which must start lowercase. The legacy
    // screen vocabulary was CamelCase, so no document can carry one there —
    // which is why the restore path treats an unrecognised value as a fallback
    // rather than as a second supported encoding. The legacy mapping itself is
    // exercised directly in the migration tests.
    for (legacy, _) in crate::workbench::LEGACY_SCREEN_VALUES {
        assert!(
            Id::parse(legacy).is_err(),
            "{legacy} unexpectedly parses as a durable id"
        );
    }
}

#[test]
fn an_unreadable_persisted_screen_value_costs_only_the_screen() {
    let mut projected = to_durable_state(&sample_state()).value_or_panic("projection succeeds");
    projected.selection.screen_id = Id::parse("core.nonesuch").ok();
    let restored = crate::state::durable_restore::from_durable_state(&projected)
        .value_or_panic("restore succeeds");
    assert_eq!(restored.screen, crate::workbench::ScreenId::default());
    assert_eq!(
        restored.repositories.len(),
        2,
        "the rest of the session must still restore"
    );
}

#[test]
fn a_document_without_a_screen_value_opens_on_the_initial_screen() {
    let mut projected = to_durable_state(&sample_state()).value_or_panic("projection succeeds");
    projected.selection.screen_id = None;
    let restored = crate::state::durable_restore::from_durable_state(&projected)
        .value_or_panic("restore succeeds");
    assert_eq!(restored.screen, crate::workbench::ScreenId::Dashboard);
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
fn forward_preserves_server_lost_as_last_known_running() {
    let mut state = sample_state();
    state.agents[0].status = AgentStatus::ServerLost;

    let projected = to_durable_state(&state).value_or_panic("projection succeeds");

    assert_eq!(
        projected.agents[0].runtime.last_known,
        LastKnownRuntime::Running
    );
    assert_eq!(
        projected.agents[0].runtime.session_id.as_deref(),
        Some("jefe-runner")
    );
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
    // Each role round-trips through its own slot. The pane leader was recorded
    // and comes back; the worker was never known and must stay unknown rather
    // than being filled in from the pane leader beside it (issue #543).
    assert_eq!(
        binding.pane_identity,
        Some(PaneProcessIdentity::from_pid(4242))
    );
    assert_eq!(
        binding.worker_identity, None,
        "an unknown worker must not be inferred from the pane leader"
    );
    let expected = current_launch_signature(&restored.agents[0], &restored.repositories[0])
        .value_or_panic("durable signature");
    assert_eq!(binding.launch_signature, expected);

    assert_eq!(restored.agents[1].status, AgentStatus::Queued);
    assert!(restored.agents[1].runtime_binding.is_none());
    assert_eq!(restored.agents[2].status, AgentStatus::Dead);
    assert!(restored.agents[2].runtime_binding.is_none());
}

#[test]
fn restored_launch_signature_matches_current_projection() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let restored = from_durable_state(&projected).value_or_panic("restore succeeds");

    for agent in &restored.agents {
        let repository = restored
            .repositories
            .iter()
            .find(|repository| repository.id == agent.repository_id)
            .value_or_panic("restored agent repository");
        let current = current_launch_signature(agent, repository)
            .value_or_panic("current launch signature projects");
        assert_eq!(agent.persisted_launch_signature.as_ref(), Some(&current));
    }
}

#[test]
fn active_projection_preserves_observed_launch_signature() {
    for status in [AgentStatus::Running, AgentStatus::ServerLost] {
        let mut state = sample_state();
        let active = &mut state.agents[0];
        active.status = status;
        let prior = active
            .runtime_binding
            .as_ref()
            .map(|binding| binding.launch_signature.clone())
            .value_or_panic("active binding");
        let observed = LaunchSignatureV1 {
            definition_hash: LaunchSignatureV1::default().definition_hash,
            ..prior
        };
        active
            .runtime_binding
            .as_mut()
            .value_or_panic("active binding")
            .launch_signature = observed.clone();

        let projected = to_durable_state(&state).value_or_panic("project state");
        assert_eq!(projected.agents[0].launch_signature, observed);
    }
}

#[test]
fn migrated_schema1_launch_signature_matches_current_projection() {
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "repositories": [{
            "id": "testrepo",
            "name": "testrepo",
            "slug": "testrepo",
            "base_dir": "/tmp"
        }],
        "agents": [{
            "id": "agent-sticky",
            "repository_id": "testrepo",
            "display_id": "#1",
            "name": "StickyAgent",
            "work_dir": "/tmp",
            "type_id": "llxprt",
            "pass_continue": true,
            "status": "running",
            "runtime_binding": {
                "session_name": "jefe-sticky",
                "launch_signature": {
                    "work_dir": "/tmp",
                    "profile": "",
                    "code_puppy_model": "",
                    "code_puppy_version": "",
                    "code_puppy_yolo": null,
                    "code_puppy_quick_resume": false,
                    "mode_flags": [],
                    "llxprt_debug": "",
                    "pass_continue": true,
                    "sandbox_enabled": false,
                    "sandbox_engine": "podman",
                    "sandbox_flags": "--cpus=2 --memory=12288m --pids-limit=256",
                    "remote": { "enabled": false },
                    "type_id": "llxprt",
                    "llxprt_version": null
                },
                "lifecycle_generation": 0
            }
        }]
    }))
    .value_or_panic("schema-1 fixture serializes");
    let migrated = crate::persistence::migration::migrate_state(&source)
        .value_or_panic("schema-1 fixture migrates");
    let restored = from_durable_state(migrated.state()).value_or_panic("migration restores");
    let current = current_launch_signature(&restored.agents[0], &restored.repositories[0])
        .value_or_panic("current launch signature projects");
    assert_eq!(
        restored.agents[0].persisted_launch_signature.as_ref(),
        Some(&current)
    );
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

/// Issue #642 AC1/AC2/AC3: the descendant anchors the orphan reaper matches
/// against must survive a restart.
///
/// Before this fix the projection dropped `worker_identities` and the restore
/// hardcoded `Vec::new()`, so `orphan_evidence` saw an empty anchor set on
/// every startup, returned `NoOrphan`, and the reap never ran — leaking a
/// session-host and a psmux server per restart.
///
/// The round trip goes through the real serialized document so the assertion
/// covers the serde wiring, not just the in-memory structs.
#[test]
fn descendant_anchors_survive_the_durable_round_trip() {
    let mut state = sample_state();
    let anchors = vec![
        WorkerProcessIdentity::new(4310, 111),
        WorkerProcessIdentity::new(4311, 222),
    ];
    state.agents[0]
        .runtime_binding
        .as_mut()
        .value_or_panic("the running agent has a binding")
        .worker_identities
        .clone_from(&anchors);

    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    assert_eq!(
        projected.agents[0].runtime.worker_identities, anchors,
        "the durable record must carry the descendant anchors in recorded order"
    );

    let encoded = serde_json::to_string(&projected).value_or_panic("serialize candidate");
    let reparsed: crate::domain::StateV2 =
        serde_json::from_str(&encoded).value_or_panic("deserialize candidate");

    let restored = from_durable_state(&reparsed).value_or_panic("restore succeeds");
    let binding = restored.agents[0]
        .runtime_binding
        .as_ref()
        .value_or_panic("the restored running agent keeps its binding");
    assert_eq!(
        binding.worker_identities, anchors,
        "the anchors must survive the round trip so orphan reaping works after a restart"
    );
}

/// Issue #642 AC2: a document written before the anchors were persisted has no
/// `worker_identities` key at all. It must still load, and an empty anchor set
/// must stay out of the document so existing files and goldens are unchanged.
///
/// The load goes through `StateDocument::parse` rather than straight into
/// `from_durable_state`, because that is the boundary a real state.json crosses
/// on startup — the one that also rejects duplicate keys and unknown fields. A
/// keyless document has to survive the strict parser, not just serde defaults.
#[test]
fn a_document_without_descendant_anchors_restores_an_empty_set() {
    let state = sample_state();
    let projected = to_durable_state(&state).value_or_panic("projection succeeds");
    let encoded = serde_json::to_vec_pretty(&projected).value_or_panic("serialize candidate");
    let text = String::from_utf8(encoded.clone()).value_or_panic("candidate is utf-8");

    assert!(
        !text.contains("worker_identities"),
        "an empty anchor set must be omitted so pre-#642 documents stay byte-identical"
    );

    let parsed = StateDocument::parse(&encoded).unwrap_or_else(|diagnostics| {
        panic!("a pre-#642 document must still parse: {diagnostics:?}")
    });
    let restored = from_durable_state(parsed.state()).value_or_panic("restore succeeds");
    let binding = restored.agents[0]
        .runtime_binding
        .as_ref()
        .value_or_panic("the restored running agent keeps its binding");
    assert!(
        binding.worker_identities.is_empty(),
        "a document without anchors must restore an empty set, not fail"
    );
}
