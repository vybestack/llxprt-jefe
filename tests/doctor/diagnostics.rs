//! RED contract: typed diagnostic statuses and exit-code classification for
//! `jefe doctor` (issue #264, decision D-04 / AC-05 / AC-06 / AC-07).
//!
//! These tests exercise the *intended* pure domain model under
//! `jefe::doctor`: each probe yields a typed `DiagnosticStatus`, a collection
//! of findings classifies into a `DoctorOutcome`, and that outcome maps to a
//! process exit code. No real subprocesses are spawned here — the inputs are
//! pure probe results, so the classification is deterministic and fast.
//!
//! Exit contract (decision D-04):
//! - exit 0 when every required startup check passes;
//! - exit 2 when a required startup blocker fails (multiplexer missing /
//!   incompatible / untrusted, ConPTY unavailable on Windows, or a configured
//!   persistence path is not writable);
//! - exit 1 only when the diagnostic command itself cannot complete;
//! - missing Git, unauthenticated/missing `gh`, and absent agent runtimes are
//!   warnings (they disable features but never block startup).

use jefe::doctor::{
    DiagnosticFinding, DiagnosticStatus, DoctorOutcome, ExitCode, FindingKind, classify_doctor,
};

// ── classify_doctor: required startup blockers fail -> exit 2 ───────────────

#[test]
fn classify_missing_multiplexer_is_blocking_failure() {
    // A required multiplexer that cannot be resolved is a startup blocker.
    let findings = vec![finding(
        FindingKind::Multiplexer,
        DiagnosticStatus::Fail,
        "psmux not found",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(
        outcome,
        DoctorOutcome::BlockingFailure,
        "missing multiplexer must classify as a blocking failure"
    );
    assert_eq!(
        outcome.exit_code(),
        ExitCode::from_u8(2),
        "blocking failure must map to exit code 2"
    );
}

#[test]
fn classify_incompatible_multiplexer_is_blocking_failure() {
    let findings = vec![finding(
        FindingKind::Multiplexer,
        DiagnosticStatus::Fail,
        "psmux 0.1 below minimum 0.9",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::BlockingFailure);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(2));
}

#[test]
fn classify_untrusted_multiplexer_is_blocking_failure() {
    // A multiplexer resolved through an unsupported compatibility layer is a
    // startup blocker (AC-05 failure path).
    let findings = vec![finding(
        FindingKind::Multiplexer,
        DiagnosticStatus::Fail,
        "resolved via Git Bash compatibility layer",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::BlockingFailure);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(2));
}

#[test]
fn classify_conpty_unavailable_on_windows_is_blocking_failure() {
    // Only meaningful on Windows, but the classification must hold regardless
    // of host because the probe result is a pure input.
    let findings = vec![finding(
        FindingKind::ConPty,
        DiagnosticStatus::Fail,
        "ConPTY pseudo-console could not be allocated",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::BlockingFailure);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(2));
}

#[test]
fn classify_unwritable_persistence_path_is_blocking_failure() {
    let findings = vec![finding(
        FindingKind::Persistence,
        DiagnosticStatus::Fail,
        "configured config directory is not writable",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::BlockingFailure);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(2));
}

// ── classify_doctor: optional findings warn -> exit 0 ───────────────────────

#[test]
fn classify_missing_git_is_warning_not_blocking() {
    let findings = vec![finding(
        FindingKind::Git,
        DiagnosticStatus::Warn,
        "git not found on PATH",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(
        outcome,
        DoctorOutcome::Ok,
        "missing Git must not block startup"
    );
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

#[test]
fn classify_unauthenticated_gh_is_warning() {
    let findings = vec![finding(
        FindingKind::GhAuth,
        DiagnosticStatus::Warn,
        "gh present but not authenticated",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

#[test]
fn classify_missing_gh_is_warning() {
    let findings = vec![finding(
        FindingKind::GhAuth,
        DiagnosticStatus::Warn,
        "gh not found on PATH",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

#[test]
fn classify_missing_llxprt_code_runtime_is_warning() {
    let findings = vec![finding(
        FindingKind::LlxprtCode,
        DiagnosticStatus::Warn,
        "LLxprt Code runtime not detected",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

#[test]
fn classify_missing_code_puppy_runtime_is_warning() {
    let findings = vec![finding(
        FindingKind::CodePuppy,
        DiagnosticStatus::Warn,
        "Code Puppy runtime not detected",
    )];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

// ── classify_doctor: a CommandError finding dominates ───────────────────────

#[test]
fn classify_command_error_dominates_other_findings() {
    // If the diagnostic itself cannot complete (e.g. an internal probe
    // panicked), the exit must be 1 even if other findings would otherwise be
    // passing or warning.
    let findings = vec![
        finding(FindingKind::Git, DiagnosticStatus::Pass, "git ok"),
        finding(
            FindingKind::DiagnosticsInternal,
            DiagnosticStatus::CommandError,
            "probe for agent runtime raised an internal error",
        ),
    ];
    let outcome = classify_doctor(&findings);
    assert_eq!(
        outcome,
        DoctorOutcome::CommandError,
        "an internal probe failure must classify as command error"
    );
    assert_eq!(
        outcome.exit_code(),
        ExitCode::from_u8(1),
        "command error must map to exit code 1"
    );
}

#[test]
fn classify_command_error_dominates_even_with_blocking_failure() {
    // Command error is the most severe outcome: it means the report itself is
    // not trustworthy, so it must win over a blocking failure.
    let findings = vec![
        finding(
            FindingKind::Multiplexer,
            DiagnosticStatus::Fail,
            "psmux missing",
        ),
        finding(
            FindingKind::DiagnosticsInternal,
            DiagnosticStatus::CommandError,
            "probe raised an internal error",
        ),
    ];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::CommandError);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(1));
}

// ── classify_doctor: all-pass yields Ok ─────────────────────────────────────

#[test]
fn classify_all_pass_is_ok() {
    let findings = vec![
        finding(FindingKind::Multiplexer, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::ConPty, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::Persistence, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::Git, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::GhAuth, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::LlxprtCode, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::CodePuppy, DiagnosticStatus::Pass, "ok"),
    ];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

#[test]
fn classify_mixed_pass_and_warn_is_ok() {
    let findings = vec![
        finding(FindingKind::Multiplexer, DiagnosticStatus::Pass, "ok"),
        finding(FindingKind::Git, DiagnosticStatus::Warn, "git missing"),
        finding(
            FindingKind::GhAuth,
            DiagnosticStatus::Warn,
            "gh unauthenticated",
        ),
    ];
    let outcome = classify_doctor(&findings);
    assert_eq!(outcome, DoctorOutcome::Ok);
    assert_eq!(outcome.exit_code(), ExitCode::from_u8(0));
}

/// Construct a `DiagnosticFinding` with the given kind/status/detail and an
/// empty redacted-evidence string. Keeping the helper local avoids coupling
/// these classification tests to any specific evidence shape.
fn finding(kind: FindingKind, status: DiagnosticStatus, detail: &str) -> DiagnosticFinding {
    DiagnosticFinding::new(kind, status, detail.to_string())
}
