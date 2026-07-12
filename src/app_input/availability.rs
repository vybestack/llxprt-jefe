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
//! All checks use the [`AppState::installed_agent_kinds`] snapshot captured
//! once at startup ([`crate::app_init`]). No PATH I/O happens during input
//! handling — the helper accepts either an explicit slice or derives the list
//! under the state read-lock.

use jefe::domain::{AgentKind, RemoteRepositorySettings};

use super::AppStateHandle;

/// Reject a local launch when the agent's runtime kind is not in the supplied
/// `available` snapshot.
///
/// Pure (no state mutation, no PATH I/O) so it can be called without holding
/// any lock. Returns `Ok(())` if the launch may proceed. Returns
/// `Err(message)` with a user-facing explanation when the kind is missing
/// from the local snapshot.
///
/// Remote-enabled agents always pass — remote PATH resolution is authoritative
/// and the local PATH cannot determine what is installed on a remote host.
/// The **target remote availability probe** (`remote_probe`) is the actual
/// guard: it runs a side-effect-free `ssh -T` check for the exact binary on
/// the remote host immediately before any side effect or launch. No local
/// startup cache of remote availability is built.
pub(super) fn require_local_kind_available(
    kind: AgentKind,
    remote: &RemoteRepositorySettings,
    available: &[AgentKind],
) -> Result<(), String> {
    if jefe::domain::target::is_valid_remote(remote) {
        // A valid remote target always passes — local PATH cannot determine
        // remote installation. The remote probe guards before side effects.
        return Ok(());
    }
    if remote.enabled {
        // Remote is enabled but incomplete (missing login_user or host).
        // This must NOT silently become local — reject with a clear error.
        return Err(jefe::domain::target::invalid_remote_message());
    }
    if available.contains(&kind) {
        return Ok(());
    }
    Err(format!(
        "{} is not installed on the local PATH. Install it or use a remote repository.",
        kind.label()
    ))
}

/// Reject a local launch when the agent's runtime kind is not in the supplied
/// `available` snapshot.
///
/// This variant is used by the centralized pre-side-effect availability
/// validation ([`crate::app_input::remote_probe::require_runtime_available`])
/// after the target has already been resolved to `Local`. It checks only
/// local availability — no remote/incomplete-remote logic, since the target
/// is already known to be local.
///
/// Pure (no state mutation, no PATH I/O).
pub(super) fn require_local_kind_available_for_target(
    kind: AgentKind,
    available: &[AgentKind],
) -> Result<(), String> {
    if available.contains(&kind) {
        return Ok(());
    }
    Err(format!(
        "{} is not installed on the local PATH. Install it or use a remote repository.",
        kind.label()
    ))
}

/// Pre-submit guard for new-agent and edit-agent forms.
///
/// Reads the `installed_agent_kinds` snapshot from `app_state` under a short
/// read-lock, then calls [`require_local_kind_available`]. If the resolved
/// kind is not locally available **and** the repository is not remote-enabled,
/// this writes a visible error into `app_state` and returns `false` so the
/// caller skips the launch. Remote agents always pass.
pub(super) fn local_kind_available_or_error(
    app_state: &mut AppStateHandle,
    kind: AgentKind,
    remote: &RemoteRepositorySettings,
) -> bool {
    let available = {
        let state = app_state.read();
        state.installed_agent_kinds.clone()
    };
    match require_local_kind_available(kind, remote, &available) {
        Ok(()) => true,
        Err(message) => {
            let mut state = app_state.write();
            state.error_message = Some(message);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{AgentKind, RemoteRepositorySettings};

    fn valid_remote() -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "build.example.com".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn valid_remote_always_passes() {
        let remote = valid_remote();
        let available = &[AgentKind::Llxprt];
        assert!(require_local_kind_available(AgentKind::CodePuppy, &remote, available).is_ok());
        assert!(require_local_kind_available(AgentKind::Llxprt, &remote, available).is_ok());
    }

    #[test]
    fn local_kind_in_snapshot_passes() {
        let remote = RemoteRepositorySettings::default();
        let available = &[AgentKind::CodePuppy];
        assert!(require_local_kind_available(AgentKind::CodePuppy, &remote, available).is_ok());
    }

    #[test]
    fn local_kind_missing_returns_error_with_label() {
        let remote = RemoteRepositorySettings::default();
        let available = &[AgentKind::Llxprt];
        let result = require_local_kind_available(AgentKind::CodePuppy, &remote, available);
        let Err(msg) = result else {
            panic!("CodePuppy should not be available in this snapshot");
        };
        assert!(msg.contains("code_puppy"));
        assert!(msg.contains("PATH"));
    }

    #[test]
    fn empty_snapshot_rejects_all_local_kinds() {
        let remote = RemoteRepositorySettings::default();
        let available = &[][..];
        assert!(require_local_kind_available(AgentKind::CodePuppy, &remote, available).is_err());
        assert!(require_local_kind_available(AgentKind::Llxprt, &remote, available).is_err());
    }

    #[test]
    fn incomplete_enabled_remote_is_rejected_not_silent_local() {
        // enabled=true but login_user/host empty must NOT silently pass as
        // local — it must return an error.
        let remote = RemoteRepositorySettings {
            enabled: true,
            ..Default::default()
        };
        let available = &[AgentKind::Llxprt];
        let result = require_local_kind_available(AgentKind::Llxprt, &remote, available);
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
        let available = &[AgentKind::CodePuppy, AgentKind::Llxprt];
        assert!(require_local_kind_available(AgentKind::Llxprt, &remote, available).is_err());
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
        };
        // CodePuppy is NOT installed locally.
        let available = &[AgentKind::Llxprt];
        assert!(
            require_local_kind_available(AgentKind::CodePuppy, &remote, available).is_ok(),
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
        };
        let available = &[][..];
        assert!(
            require_local_kind_available(AgentKind::Llxprt, &remote, available).is_ok(),
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
        };
        let available = &[AgentKind::CodePuppy, AgentKind::Llxprt];
        let result = require_local_kind_available(AgentKind::CodePuppy, &remote, available);
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
        };
        let available = &[AgentKind::CodePuppy];
        let result = require_local_kind_available(AgentKind::CodePuppy, &remote, available);
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
        };
        let available = &[AgentKind::Llxprt];
        let result = require_local_kind_available(AgentKind::CodePuppy, &remote, available);
        assert!(
            result.is_err(),
            "disabled remote + uninstalled kind must fail"
        );
    }
}
