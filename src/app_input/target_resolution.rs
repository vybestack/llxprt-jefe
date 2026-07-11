//! One shared validated target-resolution contract for the app-input layer.
//!
//! This module wraps the low-level predicates in [`jefe::domain::target`] with
//! the app-input-layer [`WorkTarget`] enum, providing a single resolution
//! function used by availability checks, issue prep, and PR prep.
//!
//! A repository's `RemoteRepositorySettings` can be in one of three states:
//!
//! 1. **Disabled** (`enabled == false`): the target is [`WorkTarget::Local`].
//! 2. **Valid Remote** (`enabled == true` AND nonempty `login_user` AND nonempty
//!    `host`): the target is [`WorkTarget::Remote`] with the settings.
//! 3. **Invalid Remote** (`enabled == true` but `login_user` or `host` is
//!    empty): this is an **error**. Previously the code silently fell back to
//!    `Local`, which meant a user who configured remote incorrectly would
//!    have a *local* worktree created, cleaned, or launched on — without any
//!    indication. This module makes that a hard error at every boundary.
//!
//! Every layer that needs to know "local or remote?" goes through
//! [`resolve_target`] or [`validate_remote_settings`]:
//!
//! - **Availability checks** (`availability.rs`): a local launch requires the
//!   runtime kind to be locally installed; a remote launch always passes.
//!   An invalid remote config must reject here, not silently pass as local.
//! - **Issue/PR sends** (`issues_send.rs`, `prs_orchestration.rs`): resolve the
//!   target before any prep/prompt side effect; an invalid remote blocks.
//! - **Form submission** (`form_ops.rs`): the repository form must visibly
//!   reject an enabled-but-incomplete remote config.

use jefe::domain::RemoteRepositorySettings;
use jefe::domain::target::{invalid_remote_message, is_valid_remote};

/// Where prep/launch operations execute.
///
/// Re-exported from `issue_prep` so all call sites share one enum.
pub(super) use super::issue_prep::WorkTarget;

/// Resolve a validated target from remote settings.
///
/// - `enabled == false` → [`WorkTarget::Local`].
/// - `enabled == true` with nonempty `login_user` and `host` →
///   [`WorkTarget::Remote`].
/// - `enabled == true` with empty `login_user` or `host` → `Err` with a
///   clear user-facing message.
///
/// This is the single source of truth for target resolution in the
/// app-input layer. It is used by availability checks, issue prep, and PR
/// prep.
pub fn resolve_target(remote: &RemoteRepositorySettings) -> Result<WorkTarget, String> {
    if !remote.enabled {
        return Ok(WorkTarget::Local);
    }
    if !is_valid_remote(remote) {
        return Err(invalid_remote_message());
    }
    Ok(WorkTarget::Remote(remote.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(enabled: bool, user: &str, host: &str) -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled,
            login_user: user.to_owned(),
            host: host.to_owned(),
            run_as_user: String::new(),
            setup_env_default: false,
        }
    }

    // ── resolve_target ────────────────────────────────────────────────

    #[test]
    fn disabled_resolves_to_local() {
        let settings = remote(false, "", "");
        assert_eq!(resolve_target(&settings), Ok(WorkTarget::Local));
    }

    #[test]
    fn enabled_with_user_and_host_resolves_to_remote() {
        let settings = remote(true, "ubuntu", "build.example.com");
        assert_eq!(
            resolve_target(&settings),
            Ok(WorkTarget::Remote(settings.clone()))
        );
    }

    #[test]
    fn enabled_with_empty_user_is_error_not_local() {
        let settings = remote(true, "", "build.example.com");
        let result = resolve_target(&settings);
        assert!(
            result.is_err(),
            "invalid remote must NOT silently resolve to Local"
        );
        let Err(msg) = result else {
            panic!("expected error");
        };
        assert!(msg.contains("login_user"));
    }

    #[test]
    fn enabled_with_empty_host_is_error_not_local() {
        let settings = remote(true, "ubuntu", "");
        let result = resolve_target(&settings);
        assert!(result.is_err());
        let Err(msg) = result else {
            panic!("expected error");
        };
        assert!(msg.contains("host"));
    }

    #[test]
    fn enabled_with_whitespace_user_is_error() {
        let settings = remote(true, "   ", "build.example.com");
        assert!(resolve_target(&settings).is_err());
    }

    #[test]
    fn enabled_with_whitespace_host_is_error() {
        let settings = remote(true, "ubuntu", "  ");
        assert!(resolve_target(&settings).is_err());
    }

    #[test]
    fn resolve_target_error_message_mentions_fields() {
        let settings = remote(true, "", "");
        let Err(msg) = resolve_target(&settings) else {
            panic!("expected error");
        };
        assert!(msg.contains("login_user"), "message: {msg}");
        assert!(msg.contains("host"), "message: {msg}");
        assert!(msg.contains("empty"), "message: {msg}");
    }
}
