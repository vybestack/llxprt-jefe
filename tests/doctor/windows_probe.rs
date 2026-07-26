//! RED contract: native Windows probe classification for `jefe doctor`
//! (issue #264, AC-06 / AC-08).
//!
//! These tests exercise the pure parsing/classification of Windows-only
//! signals (the `LongPathsEnabled` registry policy and terminal-host
//! evidence) without touching the registry or process tree.

use jefe::doctor::{
    DiagnosticStatus, FindingKind, LongPathPolicy, long_path_finding, terminal_host_evidence,
};

#[test]
fn classifies_enabled_registry_dword() {
    let raw = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\FileSystem\r\n\
               LongPathsEnabled    REG_DWORD    0x1\r\n";
    assert_eq!(LongPathPolicy::classify(raw), LongPathPolicy::Enabled);
}

#[test]
fn classifies_disabled_registry_dword() {
    let raw = "    LongPathsEnabled    REG_DWORD    0x0";
    assert_eq!(LongPathPolicy::classify(raw), LongPathPolicy::Disabled);
}

#[test]
fn classifies_missing_registry_value() {
    assert_eq!(LongPathPolicy::classify(""), LongPathPolicy::Missing);
    assert_eq!(
        LongPathPolicy::classify(
            "ERROR: The system was unable to find the specified registry key or value."
        ),
        LongPathPolicy::Missing
    );
}

#[test]
fn enabled_long_path_passes_and_disabled_warns() {
    assert_eq!(
        long_path_finding(LongPathPolicy::Enabled).status(),
        DiagnosticStatus::Pass
    );
    assert_eq!(
        long_path_finding(LongPathPolicy::Disabled).status(),
        DiagnosticStatus::Warn
    );
    assert_eq!(
        long_path_finding(LongPathPolicy::Missing).status(),
        DiagnosticStatus::Warn
    );
}

#[test]
fn long_path_finding_is_never_a_blocker() {
    let finding = long_path_finding(LongPathPolicy::Disabled);
    assert_eq!(finding.kind(), FindingKind::LongPath);
    assert!(!FindingKind::LongPath.is_required_blocker());
}

#[test]
fn long_path_detail_is_truthful_about_state() {
    assert!(
        long_path_finding(LongPathPolicy::Enabled)
            .detail()
            .contains("enabled")
    );
    assert!(
        long_path_finding(LongPathPolicy::Disabled)
            .detail()
            .contains("disabled")
    );
    assert!(
        long_path_finding(LongPathPolicy::Missing)
            .detail()
            .contains("absent")
    );
}

#[test]
fn terminal_host_evidence_includes_host_and_os() {
    let evidence = terminal_host_evidence();
    assert!(
        evidence.contains("host:"),
        "terminal host evidence must include the console host: {evidence:?}"
    );
    assert!(
        evidence.contains("os:"),
        "terminal host evidence must include the OS: {evidence:?}"
    );
}
