//! Centralized local agent-runtime availability enforcement.
//!
//! A single helper ([`require_local_kind_available`]) is called at every
//! boundary that could launch a **local** agent: new-agent form submit,
//! edit-agent submit, relaunch, restart, and issue/PR send. Remote launches
//! bypass the check because remote PATH resolution is authoritative — a
//! missing local binary does not mean the remote cannot run it.
//!
//! When no runtime at all is installed the modal stays usable (the user can
//! still fill in fields) but submit is rejected with a visible state error.
//!
//! **Valid** remote targets (enabled + valid `login_user` + valid `host`)
//! bypass the local availability check because remote PATH resolution is
//! authoritative. An enabled-but-incomplete remote (missing `login_user` or
//! `host`) is explicitly rejected — it never silently falls back to local.
//!
//! All checks use the [`AppState::available_agent_type_ids`] snapshot captured
//! once at startup ([`crate::app_init`]). No PATH I/O happens during input
//! handling — the helper accepts either an explicit slice or derives the list
//! under the state read-lock.

use std::path::Path;

use jefe::agent_candidate::AgentCandidateResolver;
use jefe::agent_candidate_path::PathSnapshot;
use jefe::agent_registry::AgentTypeRegistry;
use jefe::agent_status_view::AgentAvailabilityObservation;
use jefe::domain::agent_definition::Availability;
use jefe::domain::effects::AgentAvailabilityProbe;

/// Candidate-only startup publication plus deferred process probes.
pub struct StartupAgentAvailability {
    pub observations: Vec<AgentAvailabilityObservation>,
    pub probes: Vec<AgentAvailabilityProbe>,
    pub latest_generation: u64,
}

/// Resolve every immutable shipped definition without spawning a process.
///
/// Package selectors are intentionally blank in S4; selector participation is
/// owned by S12. Resolved definitions publish a pending row and a typed effect
/// request. Unresolved definitions publish final NotFound and no process effect.
pub fn observe_startup_agent_availability<F>(
    registry: &AgentTypeRegistry,
    repository_root: &Path,
    starting_generation: u64,
    is_enabled: F,
) -> Result<StartupAgentAvailability, String>
where
    F: Fn(&jefe::domain::agent_definition::AgentTypeId) -> bool,
{
    let path = PathSnapshot::current();
    observe_agent_availability_with_path(
        registry,
        repository_root,
        &path,
        starting_generation,
        is_enabled,
    )
}

fn observe_agent_availability_with_path<F>(
    registry: &AgentTypeRegistry,
    repository_root: &Path,
    path: &PathSnapshot,
    starting_generation: u64,
    is_enabled: F,
) -> Result<StartupAgentAvailability, String>
where
    F: Fn(&jefe::domain::agent_definition::AgentTypeId) -> bool,
{
    let resolver = AgentCandidateResolver::new(path, repository_root.to_path_buf());
    let mut observations = Vec::with_capacity(registry.definitions().len());
    let mut probes = Vec::with_capacity(registry.definitions().len());
    let mut latest_generation = starting_generation;
    for definition in registry.definitions() {
        latest_generation = latest_generation
            .checked_add(1)
            .ok_or_else(|| "agent availability probe generation exhausted".to_owned())?;
        let resolution = resolver.resolve(definition);
        let generation = latest_generation;
        let enabled = is_enabled(&definition.id);
        if resolution.is_resolved() {
            observations.push(AgentAvailabilityObservation::pending(
                definition,
                enabled,
                generation,
                resolution.clone(),
            ));
            probes.push(AgentAvailabilityProbe {
                definition: Box::new(definition.clone()),
                resolution,
                generation,
            });
        } else {
            observations.push(AgentAvailabilityObservation::not_found(
                definition, enabled, generation,
            ));
        }
    }
    Ok(StartupAgentAvailability {
        observations,
        probes,
        latest_generation,
    })
}

use jefe::domain::agent_definition::AgentTypeId;
use jefe::domain::canonical_values::typed_field;
use jefe::domain::{AgentLaunchRequest, RemoteRepositorySettings, TypedValue};

use super::AppStateHandle;

pub(super) fn require_local_kind_available(
    type_id: &AgentTypeId,
    remote: &RemoteRepositorySettings,
    available: &[AgentTypeId],
) -> Result<(), String> {
    if jefe::domain::target::is_valid_remote(remote) {
        return Ok(());
    }
    if remote.enabled {
        return Err(jefe::domain::target::invalid_remote_message());
    }
    require_local_kind_available_for_target(type_id, available)
}

pub(super) fn launch_available_or_error(
    app_state: &mut AppStateHandle,
    request: &AgentLaunchRequest,
) -> bool {
    let result = {
        let state = app_state.read();
        if request.remote.enabled {
            require_local_kind_available(&request.type_id, &request.remote, &[])
        } else if has_package_selector(request) {
            Ok(())
        } else {
            state
                .agent_type_availability
                .iter()
                .find(|observation| observation.type_id() == &request.type_id)
                .ok_or_else(|| format!("no state-owned availability for {}", request.type_id))
                .and_then(launch_availability_result)
        }
    };
    match result {
        Ok(()) => true,
        Err(message) => {
            app_state.write().error_message = Some(message);
            false
        }
    }
}

pub(super) fn launch_state_evidence(
    app_state: &AppStateHandle,
    request: &AgentLaunchRequest,
) -> Result<jefe::runtime::launch_compose::LaunchStateEvidence, jefe::runtime::RuntimeError> {
    let state = app_state.read();
    let observation = state
        .agent_type_availability
        .iter()
        .find(|observation| observation.type_id() == &request.type_id)
        .ok_or_else(|| {
            jefe::runtime::RuntimeError::SpawnFailed(format!(
                "no state-owned availability evidence for {}",
                request.type_id
            ))
        })?;
    Ok(
        jefe::runtime::launch_compose::LaunchStateEvidence::from_observation(
            observation,
            state.pending_effects.screen_generation,
            state.pending_effects.activation_generation,
        ),
    )
}

/// Reject an unlaunchable request before any prep side effect, without probing.
///
/// The authoritative probe belongs to `prepare_launch` at spawn time; running
/// it again here made one send execute the agent two or three times and
/// multiplied the exposure to a transient probe timeout (issue #553).
pub(super) fn validate_launch_or_error(
    app_state: &mut AppStateHandle,
    request: &AgentLaunchRequest,
) -> bool {
    let validated = launch_state_evidence(app_state, request)
        .and_then(|_| jefe::runtime::launch_compose::validate_launch(request));
    match validated {
        Ok(()) => true,
        Err(error) => {
            app_state.write().error_message = Some(error.to_string());
            false
        }
    }
}

fn launch_availability_result(observation: &AgentAvailabilityObservation) -> Result<(), String> {
    match observation.availability() {
        Availability::InstalledCompatible { .. } => Ok(()),
        Availability::InstalledIncompatible { reason, .. }
        | Availability::ProbeError { reason, .. } => Err(reason.clone()),
        Availability::NotFound
            if observation.pending_generation().is_some()
                && observation
                    .candidate_resolution()
                    .is_some_and(jefe::agent_candidate::CandidateResolution::is_resolved) =>
        {
            Ok(())
        }
        Availability::NotFound => Err(format!(
            "{} is not installed on the local PATH",
            observation.display_name()
        )),
    }
}

fn has_package_selector(request: &AgentLaunchRequest) -> bool {
    matches!(
        typed_field(&request.values, "version_selector"),
        Some(TypedValue::String(value)) if !value.trim().is_empty()
    )
}

pub(super) fn require_local_kind_available_for_target(
    type_id: &AgentTypeId,
    available: &[AgentTypeId],
) -> Result<(), String> {
    if available.contains(type_id) {
        return Ok(());
    }
    let label = jefe::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == *type_id)
        .map(|definition| definition.display_name)
        .ok_or_else(|| format!("unknown active agent type {type_id}"))?;
    Err(format!(
        "{label} is not installed on the local PATH. Install it or use a remote repository."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{AgentTypeId, RemoteRepositorySettings};

    /// A pinned package-runner selector does not require a global snapshot of
    /// that runner being installed locally (issue #382 availability contract).
    fn require_launch_available(
        request: &AgentLaunchRequest,
        available: &[AgentTypeId],
    ) -> Result<(), String> {
        if !request.remote.enabled
            && matches!(
                typed_field(&request.values, "version_selector"),
                Some(TypedValue::String(value)) if !value.trim().is_empty()
            )
            && jefe::domain::agent_definition::AgentDefinition::shipped()
                .into_iter()
                .find(|definition| definition.id == request.type_id)
                .is_some_and(|definition| {
                    definition
                        .candidates
                        .iter()
                        .any(|candidate| candidate.kind.is_package_runner())
                })
        {
            return Ok(());
        }
        require_local_kind_available(&request.type_id, &request.remote, available)
    }

    fn request(type_id: AgentTypeId, selector: &str) -> AgentLaunchRequest {
        let mut values = jefe::domain::TypedMap::new();
        if !selector.is_empty() {
            jefe::domain::canonical_values::insert_json(
                &mut values,
                "version_selector",
                serde_json::Value::String(selector.to_owned()),
            )
            .unwrap_or_else(|error| panic!("valid selector fixture: {error}"));
        }
        AgentLaunchRequest {
            type_id,
            values,
            work_dir: "/tmp/work".into(),
            remote: RemoteRepositorySettings::default(),
            operation: jefe::domain::agent_definition::Operation::Normal,
        }
    }

    fn valid_remote() -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "build.example.com".to_owned(),
            ..Default::default()
        }
    }

    #[cfg(windows)]
    #[test]
    fn pending_resolved_candidate_can_continue_to_authoritative_launch_probe() {
        let definition = jefe::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.llxprt")
            .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
        let temp = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must be created: {error}"));
        let wrapper = temp.path().join("llxprt.cmd");
        std::fs::write(&wrapper, b"@echo off\r\nexit /b 0\r\n")
            .unwrap_or_else(|error| panic!("wrapper fixture must be written: {error}"));
        let path = PathSnapshot::for_platform(
            jefe::runtime::AgentExecutablePlatform::current(),
            vec![temp.path().to_path_buf()],
            std::env::var_os("PATHEXT"),
        );
        let resolution =
            AgentCandidateResolver::new(&path, temp.path().to_path_buf()).resolve(&definition);
        assert!(resolution.is_resolved(), "wrapper fixture must resolve");
        let observation = AgentAvailabilityObservation::pending(&definition, true, 1, resolution);

        assert!(
            launch_availability_result(&observation).is_ok(),
            "a pending observation is checking a resolved startup candidate; launch preparation owns the authoritative probe"
        );
    }

    #[test]
    fn final_not_found_candidate_remains_rejected() {
        let definition = jefe::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.llxprt")
            .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
        let observation = AgentAvailabilityObservation::not_found(&definition, true, 1);

        match launch_availability_result(&observation) {
            Ok(()) => panic!("final NotFound must remain fail-closed"),
            Err(error) => assert!(error.contains("local PATH")),
        }
    }

    #[test]
    fn malformed_pending_not_found_evidence_remains_rejected() {
        let definition = jefe::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.llxprt")
            .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
        let observation = AgentAvailabilityObservation::pending(
            &definition,
            true,
            1,
            jefe::agent_candidate::CandidateResolution::NotFound(Vec::new()),
        );

        assert!(launch_availability_result(&observation).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn startup_publication_does_not_execute_a_hanging_probe() {
        use std::os::unix::fs::PermissionsExt;

        use jefe::runtime::AgentExecutablePlatform;

        let temp = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must be created: {error}"));
        let executable = temp.path().join("hanging-agent");
        std::fs::write(&executable, b"#!/bin/sh\nwhile :; do :; done\n")
            .unwrap_or_else(|error| panic!("probe fixture must be written: {error}"));
        let mut permissions = std::fs::metadata(&executable)
            .unwrap_or_else(|error| panic!("probe fixture metadata must exist: {error}"))
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions)
            .unwrap_or_else(|error| panic!("probe fixture must be executable: {error}"));

        let mut definition = jefe::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.codex")
            .unwrap_or_else(|| panic!("Codex definition must be shipped"));
        definition.candidates = vec![jefe::domain::agent_definition::ExecutableCandidate {
            kind: jefe::domain::agent_definition::CandidateKind::PathName {
                name: "hanging-agent".to_owned(),
            },
            value: "hanging-agent".into(),
        }];
        definition
            .agent_fields
            .retain(|field| field.id != "version_selector");
        let registry = AgentTypeRegistry::publish(vec![definition])
            .unwrap_or_else(|error| panic!("fixture registry must publish: {error}"));
        let path = PathSnapshot::for_platform(
            AgentExecutablePlatform::Unix,
            vec![temp.path().to_path_buf()],
            None,
        );

        let startup =
            observe_agent_availability_with_path(&registry, temp.path(), &path, 0, |_| true)
                .unwrap_or_else(|error| panic!("startup observation must succeed: {error}"));

        assert_eq!(startup.observations.len(), 1);
        assert_eq!(startup.probes.len(), 1);
        assert_eq!(startup.observations[0].pending_generation(), Some(1));
        assert!(startup.observations[0].candidate_resolution().is_some());
    }

    #[test]
    fn pinned_code_puppy_does_not_require_global_code_puppy_snapshot() {
        assert!(
            require_launch_available(
                &request(jefe::domain::shipped_agent_type(1), "0.0.361"),
                &[jefe::domain::shipped_agent_type(3)],
            )
            .is_ok()
        );
        assert!(
            require_launch_available(
                &request(jefe::domain::shipped_agent_type(1), ""),
                &[jefe::domain::shipped_agent_type(3)],
            )
            .is_err()
        );
    }

    #[test]
    fn valid_remote_always_passes() {
        let remote = valid_remote();
        let available = &[jefe::domain::shipped_agent_type(3)];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available)
                .is_ok()
        );
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(3), &remote, available)
                .is_ok()
        );
    }

    #[test]
    fn local_kind_in_snapshot_passes() {
        let remote = RemoteRepositorySettings::default();
        let available = &[jefe::domain::shipped_agent_type(1)];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available)
                .is_ok()
        );
    }

    #[test]
    fn local_kind_missing_returns_error_with_label() {
        let remote = RemoteRepositorySettings::default();
        let available = &[jefe::domain::shipped_agent_type(3)];
        let result =
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available);
        let Err(msg) = result else {
            panic!("CodePuppy should not be available in this snapshot");
        };
        assert!(msg.contains("Code Puppy"));
        assert!(msg.contains("PATH"));
    }

    #[test]
    fn empty_snapshot_rejects_all_local_kinds() {
        let remote = RemoteRepositorySettings::default();
        let available = &[][..];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available)
                .is_err()
        );
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(3), &remote, available)
                .is_err()
        );
    }

    #[test]
    fn incomplete_enabled_remote_is_rejected_not_silent_local() {
        // enabled=true but login_user/host empty must NOT silently pass as
        // local — it must return an error.
        let remote = RemoteRepositorySettings {
            enabled: true,
            ..Default::default()
        };
        let available = &[jefe::domain::shipped_agent_type(3)];
        let result =
            require_local_kind_available(&jefe::domain::shipped_agent_type(3), &remote, available);
        assert!(
            result.is_err(),
            "incomplete enabled remote must NOT silently become local"
        );
        let Err(msg) = result else {
            return;
        };
        assert!(msg.contains("login_user"));
        assert!(msg.contains("host"));
    }

    #[test]
    fn incomplete_enabled_remote_rejected_even_when_kind_installed() {
        // Even if the kind is locally available, an incomplete enabled
        // remote is rejected — the user asked for remote and got neither
        // valid remote nor a clear local.
        let remote = RemoteRepositorySettings {
            enabled: true,
            ..Default::default()
        };
        let available = &[
            jefe::domain::shipped_agent_type(1),
            jefe::domain::shipped_agent_type(3),
        ];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(3), &remote, available)
                .is_err()
        );
    }

    // ── Form submit-path tests (defect 1) ────────────────────────────
    //
    // validate_form_kind_available in modal_handlers.rs must construct
    // RemoteRepositorySettings from ALL entered repository fields
    // (enabled, login_user, host, run_as_user, setup_env_default), not
    // defaults. These tests exercise the same predicate with settings built
    // from form fields to prove the submit-path contract.

    /// A complete enabled remote (all fields populated from the form) passes
    /// target validation **independent of local PATH** — even when the kind
    /// is NOT in the local installed snapshot. This is the core defect-1 fix:
    /// the old code built `RemoteRepositorySettings { enabled, ..Default }`
    /// so login_user/host were always empty and a complete remote config was
    /// misclassified as an incomplete remote (error) instead of a valid
    /// remote (pass).
    #[test]
    fn complete_enabled_remote_passes_independent_of_local_path() {
        let remote = RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "build.example.com".to_owned(),
            run_as_user: "acoliver".to_owned(),
            setup_env_default: true,
            ..RemoteRepositorySettings::default()
        };
        // CodePuppy is NOT installed locally.
        let available = &[jefe::domain::shipped_agent_type(3)];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available)
                .is_ok(),
            "complete enabled remote must pass even when kind is not locally installed"
        );
    }

    /// A complete enabled remote with only the required fields (login_user +
    /// host) passes; run_as_user and setup_env_default are optional.
    #[test]
    fn complete_enabled_remote_minimal_fields_passes() {
        let remote = RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "build.example.com".to_owned(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        };
        let available = &[][..];
        assert!(
            require_local_kind_available(&jefe::domain::shipped_agent_type(3), &remote, available)
                .is_ok(),
            "complete enabled remote with empty optional fields must pass"
        );
    }

    /// An incomplete enabled remote (login_user set but host empty) fails
    /// regardless of whether the kind is locally installed — this is the
    /// regression guard for the old bug where defaults masked incompleteness.
    #[test]
    fn incomplete_enabled_remote_with_empty_host_fails() {
        let remote = RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: String::new(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        };
        let available = &[
            jefe::domain::shipped_agent_type(1),
            jefe::domain::shipped_agent_type(3),
        ];
        let result =
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available);
        assert!(result.is_err(), "incomplete remote must fail");
    }

    /// An incomplete enabled remote (host set but login_user empty) fails.
    #[test]
    fn incomplete_enabled_remote_with_empty_login_user_fails() {
        let remote = RemoteRepositorySettings {
            enabled: true,
            login_user: String::new(),
            host: "build.example.com".to_owned(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        };
        let available = &[jefe::domain::shipped_agent_type(1)];
        let result =
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available);
        assert!(result.is_err(), "incomplete remote must fail");
    }

    /// A disabled remote with the kind not installed fails — this proves the
    /// "not remote" path still enforces local availability.
    #[test]
    fn disabled_remote_with_uninstalled_kind_fails() {
        let remote = RemoteRepositorySettings {
            enabled: false,
            login_user: "ubuntu".to_owned(),
            host: "build.example.com".to_owned(),
            run_as_user: String::new(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        };
        let available = &[jefe::domain::shipped_agent_type(3)];
        let result =
            require_local_kind_available(&jefe::domain::shipped_agent_type(1), &remote, available);
        assert!(
            result.is_err(),
            "disabled remote + uninstalled kind must fail"
        );
    }
}
