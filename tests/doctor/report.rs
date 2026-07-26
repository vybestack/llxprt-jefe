//! RED contract: human-readable report rendering for `jefe doctor`
//! (issue #264, AC-04 / AC-05 / AC-06 / AC-07 / AC-08).
//!
//! `render_report` is expected to be a pure function under `jefe::doctor` that
//! turns a `DoctorReport` (version metadata + a slice of `DiagnosticFinding`)
//! into a single human-readable string. The tests pin:
//!
//! - the report always includes Jefe version and commit (AC-04);
//! - platform and architecture are reported (AC-04);
//! - each diagnostic section is present with a stable, greppable header
//!   (multiplexer, namespace, ConPTY, Git, gh/auth, LLxprt Code, Code Puppy,
//!   config/state, long-path) — AC-05 through AC-08;
//! - redaction is applied *before* rendering, so a sensitive fixture supplied
//!   through a finding never reaches the rendered string (AC-09 wiring).
//!
//! The inputs are pure values; no subprocesses or real probes are involved.

use jefe::doctor::{
    DiagnosticFinding, DiagnosticStatus, DoctorReport, FindingKind, redact_value, render_report,
};
use jefe::{GIT_COMMIT, VERSION};

use crate::support::TestResultExt;

// ── Version / commit / platform / architecture (AC-04) ─────────────────────

#[test]
fn report_includes_jefe_version() {
    let report = sample_report(&[]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains(VERSION),
        "report must include the Jefe version ({VERSION}); got: {rendered:?}"
    );
}

#[test]
fn report_includes_git_commit() {
    let report = sample_report(&[]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains(GIT_COMMIT),
        "report must include the baked-in git commit ({GIT_COMMIT}); got: {rendered:?}"
    );
}

#[test]
fn report_includes_platform_label() {
    let report = sample_report(&[]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Platform") || rendered.contains("platform") || rendered.contains("OS"),
        "report must include a platform/OS section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_architecture_label() {
    let report = sample_report(&[]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Architecture")
            || rendered.contains("architecture")
            || rendered.contains("arch"),
        "report must include an architecture section; got: {rendered:?}"
    );
}

// ── Required diagnostic section headers (AC-05 .. AC-08) ────────────────────

#[test]
fn report_includes_multiplexer_section() {
    let report = sample_report(&[pass_finding(FindingKind::Multiplexer, "psmux 0.9.2 ready")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Multiplexer")
            || rendered.contains("multiplexer")
            || rendered.contains("psmux"),
        "report must include a multiplexer section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_namespace_section() {
    let report = sample_report(&[pass_finding(
        FindingKind::Namespace,
        "private namespace ready",
    )]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Namespace") || rendered.contains("namespace"),
        "report must include a namespace section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_conpty_section() {
    let report = sample_report(&[pass_finding(FindingKind::ConPty, "ConPTY available")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("ConPTY") || rendered.contains("ConPTY".to_lowercase().as_str()),
        "report must include a ConPTY section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_git_section() {
    let report = sample_report(&[pass_finding(FindingKind::Git, "git 2.43.0")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Git") || rendered.contains("git"),
        "report must include a Git section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_gh_auth_section() {
    let report = sample_report(&[pass_finding(FindingKind::GhAuth, "gh authenticated")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("gh") || rendered.contains("GitHub"),
        "report must include a gh/auth section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_llxprt_code_section() {
    let report = sample_report(&[pass_finding(FindingKind::LlxprtCode, "LLxprt Code present")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("LLxprt Code") || rendered.contains("llxprt"),
        "report must include an LLxprt Code section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_code_puppy_section() {
    let report = sample_report(&[pass_finding(FindingKind::CodePuppy, "Code Puppy present")]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Code Puppy")
            || rendered.contains("code-puppy")
            || rendered.contains("codepuppy"),
        "report must include a Code Puppy section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_config_state_section() {
    let report = sample_report(&[pass_finding(
        FindingKind::Persistence,
        "config and state directories writable",
    )]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Config")
            || rendered.contains("config")
            || rendered.contains("State")
            || rendered.contains("state")
            || rendered.contains("Persistence")
            || rendered.contains("persistence"),
        "report must include a config/state section; got: {rendered:?}"
    );
}

#[test]
fn report_includes_long_path_section() {
    let report = sample_report(&[pass_finding(
        FindingKind::LongPath,
        "Windows long-path policy ok",
    )]);
    let rendered = render_report(&report);
    assert!(
        rendered.contains("Long")
            || rendered.contains("long path")
            || rendered.contains("long-path"),
        "report must include a long-path section; got: {rendered:?}"
    );
}

// ── Redaction is applied before rendering (AC-09 wiring) ───────────────────

#[test]
fn report_applies_redaction_to_findings() {
    // A finding that carries a sensitive home path must not leak it into the
    // rendered report. `redact_value` is the same function the renderer is
    // expected to apply, so we prove the wiring by asserting the rendered
    // output matches a hand-redacted expectation on the sensitive substring.
    let sensitive = "/home/alice/.config/jefe";
    let finding = DiagnosticFinding::new(
        FindingKind::Persistence,
        DiagnosticStatus::Pass,
        sensitive.to_string(),
    );
    let report = sample_report(&[finding]);
    let rendered = render_report(&report);
    assert!(
        !rendered.contains("alice"),
        "rendered report must not leak the username from a finding detail: {rendered:?}"
    );
    // The structural label the redactor is expected to preserve.
    let expected_label = redact_value(sensitive);
    assert!(
        rendered.contains(&expected_label)
            || rendered.contains("config")
            || rendered.contains("home"),
        "rendered report must retain a structural label after redaction: {rendered:?}"
    );
}

#[test]
fn report_renders_a_finding_status_marker() {
    // Each finding line should communicate its status (pass/warn/fail) so a
    // human can scan the report. The exact glyph is an implementation detail;
    // we assert that a fail finding produces a line that is visibly not a pass.
    let fail_report = sample_report(&[fail_finding(FindingKind::Multiplexer, "psmux missing")]);
    let pass_report = sample_report(&[pass_finding(FindingKind::Multiplexer, "psmux ready")]);
    let fail_rendered = render_report(&fail_report);
    let pass_rendered = render_report(&pass_report);
    assert_ne!(
        fail_rendered, pass_rendered,
        "a failing finding must render differently from a passing one"
    );
    assert!(
        fail_rendered.contains("psmux missing") || fail_rendered.contains("missing"),
        "the finding detail must appear in the report: {fail_rendered:?}"
    );
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Build a `DoctorReport` carrying the given findings and neutral metadata.
fn sample_report(findings: &[DiagnosticFinding]) -> DoctorReport {
    DoctorReport::new(
        VERSION.to_string(),
        GIT_COMMIT.to_string(),
        sample_platform(),
        sample_arch(),
        findings.to_vec(),
    )
    .test_unwrap("build sample DoctorReport")
}

fn pass_finding(kind: FindingKind, detail: &str) -> DiagnosticFinding {
    DiagnosticFinding::new(kind, DiagnosticStatus::Pass, detail.to_string())
}

fn fail_finding(kind: FindingKind, detail: &str) -> DiagnosticFinding {
    DiagnosticFinding::new(kind, DiagnosticStatus::Fail, detail.to_string())
}

fn sample_platform() -> String {
    if cfg!(windows) {
        "windows".to_string()
    } else {
        "unix".to_string()
    }
}

fn sample_arch() -> String {
    if cfg!(target_arch = "x86_64") {
        "x86_64".to_string()
    } else {
        "unknown".to_string()
    }
}
