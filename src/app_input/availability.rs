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
}

/// Resolve every immutable shipped definition without spawning a process.
///
/// Package selectors are intentionally blank in S4; selector participation is
/// owned by S12. Resolved definitions publish a pending row and a typed effect
/// request. Unresolved definitions publish final NotFound and no process effect.
pub fn observe_startup_agent_availability<F>(
    registry: &AgentTypeRegistry,
    repository_root: &Path,
    is_enabled: F,
) -> StartupAgentAvailability
where
    F: Fn(&jefe::domain::agent_definition::AgentTypeId) -> bool,
{
    let path = PathSnapshot::current();
    observe_agent_availability_with_path(registry, repository_root, &path, is_enabled)
}

fn observe_agent_availability_with_path<F>(
    registry: &AgentTypeRegistry,
    repository_root: &Path,
    path: &PathSnapshot,
    is_enabled: F,
) -> StartupAgentAvailability
where
    F: Fn(&jefe::domain::agent_definition::AgentTypeId) -> bool,
{
    let resolver = AgentCandidateResolver::new(path, repository_root.to_path_buf());
    let mut observations = Vec::with_capacity(registry.definitions().len());
    let mut probes = Vec::with_capacity(registry.definitions().len());
    for (index, definition) in registry.definitions().iter().enumerate() {
        let resolution = resolver.resolve(definition);
        let generation = u64::try_from(index).map_or(u64::MAX, |value| value.saturating_add(1));
        let enabled = is_enabled(&definition.id);
        if resolution.is_resolved() {
            observations.push(AgentAvailabilityObservation::pending(
                definition, enabled, generation,
            ));
            probes.push(AgentAvailabilityProbe {
                definition: Box::new(definition.clone()),
                resolution,
                generation,
            });
        } else {
            observations.push(AgentAvailabilityObservation::new(
                definition,
                enabled,
                Availability::NotFound,
            ));
        }
    }
    StartupAgentAvailability {
        observations,
        probes,
    }
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

pub(super) fn require_launch_available(
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

pub(super) fn launch_available_or_error(
    app_state: &mut AppStateHandle,
    request: &AgentLaunchRequest,
) -> bool {
    let available = jefe::agent_detection::available_agent_type_ids();
    match require_launch_available(request, available) {
        Ok(()) => true,
        Err(message) => {
            app_state.write().error_message = Some(message);
            false
        }
    }
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

        let startup = observe_agent_availability_with_path(&registry, temp.path(), &path, |_| true);

        assert_eq!(startup.observations.len(), 1);
        assert_eq!(startup.probes.len(), 1);
        assert_eq!(startup.observations[0].pending_generation(), Some(1));
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
