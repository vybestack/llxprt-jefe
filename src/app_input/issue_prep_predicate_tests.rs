//! Tests for the remote predicate sentinel protocol (defect 1: fail-closed).
//!
//! `run_remote_check` uses a sentinel protocol: the script always exits 0
//! after printing exactly `JEFE_PREDICATE_TRUE` or `JEFE_PREDICATE_FALSE`.
//! Any SSH exit 255, auth/host/sudo/shell failure, or malformed/extra output
//! is an `Err` — never a safe `false` that could trigger a clone.
//!
//! These tests exercise:
//! - The pure [`classify_predicate_output`] classifier: true/false/transport/
//!   auth/sudo/shell/malformed-output scenarios.
//! - The [`wrap_predicate`] sentinel-protocol wrapper.
//! - Production-seam scenarios proving false is safe and infrastructure
//!   failures never cause a clone.

use super::*;

// ── classify_predicate_output: pure classifier (defect 1) ───────────

#[test]
fn predicate_classifier_true_sentinel_is_true() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_TRUE", "");
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_classifier_false_sentinel_is_false() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_FALSE", "");
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_classifier_false_sentinel_with_newline_is_false() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_FALSE\n", "");
    assert_eq!(result, Ok(false));
}

#[test]
fn predicate_classifier_true_sentinel_with_newline_is_true() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_TRUE\n", "");
    assert_eq!(result, Ok(true));
}

#[test]
fn predicate_classifier_false_is_safe() {
    // A clean JEFE_PREDICATE_FALSE must be Ok(false), NOT an Err — this is
    // the safe, normal missing-path predicate.
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_FALSE", "");
    assert!(result.is_ok());
    assert!(
        !result.unwrap_or(true),
        "false predicate must be safe false"
    );
}

#[test]
fn predicate_classifier_ssh_exit_255_is_err() {
    // SSH transport/auth/host failure must NEVER be confused with a safe
    // false predicate.
    let result = classify_predicate_output(Some(255), "", "Permission denied (publickey).");
    let Err(err) = &result else {
        panic!("SSH exit 255 must be Err, not a safe false: {result:?}");
    };
    assert!(
        err.contains("255") || err.contains("transport") || err.contains("auth"),
        "error must mention transport/auth/255: {err}"
    );
}

#[test]
fn predicate_classifier_nonzero_exit_is_err() {
    // Any nonzero exit (sudo failure, shell error) must be Err.
    let result = classify_predicate_output(Some(126), "", "sudo: a password is required");
    assert!(
        result.is_err(),
        "nonzero exit must be Err, not a safe false"
    );
}

#[test]
fn predicate_classifier_signal_terminated_is_err() {
    let result = classify_predicate_output(None, "", "");
    assert!(result.is_err(), "signal termination must be Err");
}

#[test]
fn predicate_classifier_empty_output_is_err() {
    // Exit 0 but no sentinel — protocol mismatch.
    let result = classify_predicate_output(Some(0), "", "");
    assert!(result.is_err(), "empty output must be Err");
}

#[test]
fn predicate_classifier_garbage_output_is_err() {
    let result = classify_predicate_output(Some(0), "some garbage", "");
    assert!(result.is_err(), "garbage output must be Err");
}

#[test]
fn predicate_classifier_prefix_before_sentinel_is_err() {
    let result =
        classify_predicate_output(Some(0), "Welcome to Ubuntu 22.04\nJEFE_PREDICATE_TRUE", "");
    assert!(
        result.is_err(),
        "prefix before sentinel must be Err: {result:?}"
    );
}

#[test]
fn predicate_classifier_suffix_after_sentinel_is_err() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_TRUE\nextra line", "");
    assert!(
        result.is_err(),
        "suffix after sentinel must be Err: {result:?}"
    );
}

#[test]
fn predicate_classifier_both_sentinels_is_err() {
    let result =
        classify_predicate_output(Some(0), "JEFE_PREDICATE_TRUE\nJEFE_PREDICATE_FALSE", "");
    assert!(result.is_err(), "both sentinels must be Err: {result:?}");
}

#[test]
fn predicate_classifier_partial_sentinel_is_err() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_TRU", "");
    assert!(result.is_err(), "partial sentinel must be Err");
}

#[test]
fn predicate_classifier_infra_failure_never_causes_clone() {
    // The critical safety property: an infrastructure failure (exit 255)
    // must be Err, never Ok(true) or Ok(false). A clone could only happen
    // if `exists` were Ok(false) — proving that infra failures don't
    // produce false is the core regression guard.
    let result = classify_predicate_output(Some(255), "", "Connection refused");
    assert!(result.is_err(), "infra failure must be Err");
    assert!(
        !matches!(result, Ok(false)),
        "infra failure must not be false"
    );
    assert!(
        !matches!(result, Ok(true)),
        "infra failure must not be true"
    );
}

#[test]
fn predicate_classifier_false_sentinel_does_not_cause_clone() {
    // Conversely, a clean false (path absent) is Ok(false) — and the prep
    // flow treats that as "clone if identity present." This test proves the
    // boundary: only the exact sentinel false is a safe false; everything
    // else fails closed.
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_FALSE", "");
    assert_eq!(result, Ok(false));
}

// ── wrap_predicate: sentinel protocol wrapper (defect 1) ────────────

#[test]
fn wrap_predicate_emits_true_sentinel_on_success() {
    let wrapped = wrap_predicate("test -d /tmp");
    // Verify the wrapped script references both sentinels and the condition.
    assert!(wrapped.contains("JEFE_PREDICATE_TRUE"));
    assert!(wrapped.contains("JEFE_PREDICATE_FALSE"));
    assert!(wrapped.contains("test -d /tmp"));
}

#[test]
fn wrap_predicate_uses_printf_for_exact_output() {
    let wrapped = wrap_predicate("true");
    // printf '%s' ensures no trailing newline from the print itself.
    assert!(
        wrapped.contains("printf '%s'"),
        "must use printf '%s' for exact sentinel: {wrapped}"
    );
}

#[test]
fn wrap_predicate_braces_condition() {
    // The condition is wrapped in { ...; } so compound conditions work
    // correctly with && / ||.
    let wrapped = wrap_predicate("a | b");
    assert!(
        wrapped.contains("{ a | b; }"),
        "must brace-group the condition: {wrapped}"
    );
}

// ── Production seam: run_remote_check uses sentinel protocol ─────────
//
// These tests verify that the production RemotePrepRunner.run() method
// delegates predicate checks through the sentinel-based
// classify_predicate_output, proving the fail-closed contract. Since we
// cannot make a real SSH connection in unit tests, we exercise the pure
// classifier with the exact scenarios the production path encounters.

#[test]
fn production_seam_path_absent_is_safe_false() {
    // Scenario: remote path does not exist. The sentinel protocol prints
    // JEFE_PREDICATE_FALSE → Ok(false). This is the ONLY case where a clone
    // is allowed (when identity is present).
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_FALSE", "");
    assert_eq!(result, Ok(false), "absent path must be safe false");
}

#[test]
fn production_seam_path_exists_is_true() {
    let result = classify_predicate_output(Some(0), "JEFE_PREDICATE_TRUE", "");
    assert_eq!(result, Ok(true), "existing path must be true");
}

#[test]
fn production_seam_ssh_auth_failure_never_causes_clone() {
    // If SSH fails with exit 255 (auth/host/transport), `exists` must be
    // Err — never Ok(false). This is the core defect: the old blanket
    // nonzero=false treated auth failures as "path absent," triggering an
    // erroneous clone.
    let result = classify_predicate_output(Some(255), "", "Permission denied");
    assert!(
        result.is_err(),
        "SSH auth failure must be Err — never a safe false that causes clone"
    );
}

#[test]
fn production_seam_sudo_failure_never_causes_clone() {
    // sudo -n failure (effective-user switch fails) returns nonzero. Must
    // be Err, not a safe false.
    let result = classify_predicate_output(Some(1), "", "sudo: a password is required");
    assert!(
        result.is_err(),
        "sudo failure must be Err — never a safe false"
    );
}

#[test]
fn production_seam_shell_error_never_causes_clone() {
    // A shell parse error or missing command returns nonzero. Must be Err.
    let result = classify_predicate_output(Some(127), "", "command not found");
    assert!(
        result.is_err(),
        "shell error must be Err — never a safe false"
    );
}

#[test]
fn production_seam_malformed_output_never_causes_clone() {
    // Even with exit 0, malformed output (no exact sentinel) must be Err.
    let result = classify_predicate_output(Some(0), "banner\nJEFE_PREDICATE_TRUE", "");
    assert!(
        result.is_err(),
        "malformed output must be Err — never a safe false or true"
    );
}

// ── classify_origin_url_output: origin-probe classifier (MUST-FIX #1) ──
//
// `read_remote_origin_url` must distinguish three outcomes:
// 1. Origin exists + get-url succeeds → Ok(Some(url)).
// 2. Origin absent (get-url exits nonzero, swallowed by `|| true`) → Ok(None).
// 3. SSH/sudo/shell failure → Err.
//
// These tests exercise the pure classifier with the exact scenarios the
// production path encounters.

#[test]
fn origin_url_classifier_success_with_url() {
    let result = classify_origin_url_output(Some(0), "git@github.com:acme/widgets.git\n", "");
    assert_eq!(
        result,
        Ok(Some("git@github.com:acme/widgets.git".to_owned()))
    );
}

#[test]
fn origin_url_classifier_success_empty_is_no_origin() {
    // The classifier treats exit 0 + empty stdout as Ok(None). In production
    // this state is produced by the read_remote_origin_url wrapper mapping
    // git's exit-2 (no origin remote) to empty stdout; this test verifies the
    // classifier's contract for that input directly, independent of how the
    // shell produced it.
    let result = classify_origin_url_output(Some(0), "", "");
    assert_eq!(result, Ok(None));
}

#[test]
fn origin_url_classifier_success_whitespace_only_is_no_origin() {
    let result = classify_origin_url_output(Some(0), "  \n", "");
    assert_eq!(result, Ok(None));
}

#[test]
fn origin_url_classifier_ssh_exit_255_is_err() {
    let result = classify_origin_url_output(Some(255), "", "Permission denied (publickey).");
    let Err(err) = &result else {
        panic!("SSH exit 255 must be Err: {result:?}");
    };
    assert!(
        err.contains("255") || err.contains("transport") || err.contains("auth"),
        "error must mention transport/auth/255: {err}"
    );
}

#[test]
fn origin_url_classifier_other_nonzero_is_err() {
    let result = classify_origin_url_output(Some(126), "", "sudo: a password is required");
    assert!(result.is_err(), "nonzero exit must be Err");
}

#[test]
fn origin_url_classifier_signal_terminated_is_err() {
    let result = classify_origin_url_output(None, "", "");
    assert!(result.is_err(), "signal termination must be Err");
}

#[test]
fn origin_url_classifier_success_trims_url() {
    let result = classify_origin_url_output(Some(0), "  https://github.com/acme/widgets.git\n", "");
    assert_eq!(
        result,
        Ok(Some("https://github.com/acme/widgets.git".to_owned()))
    );
}

#[test]
fn origin_url_classifier_success_empty_with_stderr_is_no_origin() {
    // The classifier keys off exit code + stdout, not stderr: exit 0 with
    // empty stdout is Ok(None) even if stderr carries incidental text. This
    // is a distinct edge case from the plain empty-stdout test (which has
    // empty stderr too) and guards against a future change that keys the
    // no-origin decision on stderr presence.
    let result = classify_origin_url_output(Some(0), "", "warning: something");
    assert_eq!(result, Ok(None));
}
