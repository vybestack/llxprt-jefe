//! Behavioral tests for restart-preflight ordering (issue #269).
//!
//! `dispatch_restart_agent` runs `relaunch_preflight_passed` BEFORE the
//! destructive `kill_runtime_agent`. These tests prove the preflight
//! predicates that gate the kill:
//!
//! 1. An invalid version selector (embedded NUL) is rejected by the FIRST
//!    preflight check — `validate_version_selector` — so the kill is never
//!    reached.
//! 2. A remote npm/availability failure (missing npm for versioned LLxprt)
//!    is rejected by the availability check — again before the kill.
//!
//! Since `AppStateHandle` (`HookState<AppState>`) cannot be constructed in
//! unit tests, these tests exercise the SAME pure predicates that
//! `relaunch_preflight_passed` evaluates in sequence. The predicates are
//! evaluated in declaration order inside `relaunch_preflight_passed`:
//! selector validation → availability → sandbox preflight. An `Err`/`false`
//! from any earlier predicate short-circuits the function to `return false`
//! before `kill_runtime_agent` is reached.

use super::availability::require_local_kind_or_npm_available;
use jefe::domain::{AgentKind, RemoteRepositorySettings, validate_version_selector};

/// An invalid version selector (embedded NUL) must be rejected by
/// `validate_version_selector` — the FIRST preflight check in
/// `relaunch_preflight_passed`. Since `relaunch_preflight_passed` returns
/// `false` on the first failing check, the kill in `dispatch_restart_agent`
/// is never reached for an invalid selector.
#[test]
fn restart_preflight_rejects_invalid_selector_before_kill() {
    let invalid = "0.9.0\x00; rm -rf /";
    let result = validate_version_selector(invalid);
    assert!(
        result.is_err(),
        "NUL selector must be rejected so restart kill is skipped"
    );
}

/// A valid selector (exact version, semver, dist-tag, blank) must pass
/// `validate_version_selector` — proving the selector check does NOT
/// over-reject valid restarts.
#[test]
fn restart_preflight_accepts_valid_selectors() {
    for valid in ["", "  ", "0.9.0", "0.10.0-nightly", "latest", "next"] {
        assert!(
            validate_version_selector(valid).is_ok(),
            "valid selector '{valid}' must pass preflight"
        );
    }
}

/// A versioned LLxprt launch with npm NOT present must be rejected by
/// `require_local_kind_or_npm_available` — the SECOND preflight check. Since
/// `relaunch_preflight_passed` returns `false` on the first failing check,
/// the kill in `dispatch_restart_agent` is never reached for an unavailable
/// target.
#[test]
fn restart_preflight_rejects_missing_npm_for_versioned_llxprt_before_kill() {
    let remote = RemoteRepositorySettings::default();
    let result = require_local_kind_or_npm_available(
        AgentKind::Llxprt,
        "0.9.0",
        &remote,
        &[],
        false, // npm NOT present
    );
    assert!(
        result.is_err(),
        "missing npm for versioned LLxprt must be rejected so restart kill is skipped"
    );
}

/// A versioned LLxprt launch with npm present must pass the availability
/// check — proving it does NOT over-reject valid restarts (assuming selector
/// and sandbox preflight also pass).
#[test]
fn restart_preflight_accepts_versioned_llxprt_with_npm_present() {
    let remote = RemoteRepositorySettings::default();
    let result = require_local_kind_or_npm_available(
        AgentKind::Llxprt,
        "0.9.0",
        &remote,
        &[],
        true, // npm present
    );
    assert!(
        result.is_ok(),
        "versioned LLxprt with npm present must pass availability preflight"
    );
}

/// An invalid remote identity must be rejected by
/// `require_local_kind_or_npm_available` — proving the availability check
/// catches remote misconfiguration BEFORE the kill.
#[test]
fn restart_preflight_rejects_invalid_remote_identity_before_kill() {
    let remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "-oProxyCommand".to_owned(),
        host: "build.example.com".to_owned(),
        ..Default::default()
    };
    let result =
        require_local_kind_or_npm_available(AgentKind::Llxprt, "0.9.0", &remote, &[], false);
    assert!(
        result.is_err(),
        "invalid remote identity must be rejected so restart kill is skipped"
    );
}
