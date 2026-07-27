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
fn report_reports_empty_commit_as_unavailable_not_blank() {
    // AC-04: an unavailable commit must be explicitly reported as
    // unavailable, not fabricated or rendered as a blank identity line that
    // hides the missing metadata from a user attaching the report to an issue.
    let report = DoctorReport::new(
        VERSION.to_string(),
        String::new(),
        sample_platform(),
        sample_arch(),
        vec![],
    )
    .test_unwrap("build report with empty commit");
    let rendered = render_report(&report);
    assert!(
        rendered.contains("unavailable"),
        "an empty commit must be rendered as 'unavailable', not blank: {rendered:?}"
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
fn report_uses_canonical_section_headers() {
    let findings = [
        pass_finding(FindingKind::Multiplexer, "fixture-a"),
        pass_finding(FindingKind::Namespace, "fixture-b"),
        pass_finding(FindingKind::ConPty, "fixture-c"),
        pass_finding(FindingKind::Git, "fixture-d"),
        pass_finding(FindingKind::GhAuth, "fixture-e"),
        pass_finding(FindingKind::LlxprtCode, "fixture-f"),
        pass_finding(FindingKind::CodePuppy, "fixture-g"),
        pass_finding(FindingKind::Persistence, "fixture-h"),
        pass_finding(FindingKind::LongPath, "fixture-i"),
    ];
    let rendered = render_report(&sample_report(&findings));
    for header in [
        "
Multiplexer
",
        "
Namespace
",
        "
ConPTY
",
        "
Git
",
        "
gh / GitHub auth
",
        "
LLxprt Code
",
        "
Code Puppy
",
        "
Config / state persistence
",
        "
Long-path support
",
    ] {
        assert!(
            rendered.contains(header),
            "report must include canonical header {header:?}; got: {rendered:?}"
        );
    }
}

#[test]
fn report_renders_sections_in_canonical_order_regardless_of_input() {
    // Findings supplied out of canonical order must still render under their
    // canonical section header sequence so a shuffled collector cannot reorder
    // the report. Git and Multiplexer are deliberately reversed relative to the
    // canonical header order.
    let findings = [
        pass_finding(FindingKind::Git, "fixture-git"),
        pass_finding(FindingKind::Multiplexer, "fixture-mux"),
    ];
    let rendered = render_report(&sample_report(&findings));
    let mux_pos = rendered.find("Multiplexer");
    let git_pos = rendered.find(
        "
Git
",
    );
    let (Some(mux_pos), Some(git_pos)) = (mux_pos, git_pos) else {
        panic!("both canonical headers must be present: {rendered:?}");
    };
    assert!(
        mux_pos < git_pos,
        "Multiplexer section must precede Git regardless of input order: {rendered:?}"
    );
}

#[test]
fn report_groups_multiple_findings_of_one_kind_under_one_header() {
    // Two findings of the same kind must both render under a single section
    // header so the renderer neither drops duplicates nor emits a header per
    // finding.
    let findings = [
        pass_finding(FindingKind::Git, "fixture-git-a"),
        pass_finding(FindingKind::Git, "fixture-git-b"),
    ];
    let rendered = render_report(&sample_report(&findings));
    assert_eq!(
        rendered
            .matches(
                "
Git
"
            )
            .count(),
        1,
        "a single Git header must render for multiple Git findings: {rendered:?}"
    );
    assert!(
        rendered.contains("fixture-git-a") && rendered.contains("fixture-git-b"),
        "both Git findings must render: {rendered:?}"
    );
}

#[test]
fn minimal_report_is_renderable_with_unknown_identity() {
    // The last-resort fallback constructor must produce a valid report whose
    // identity is explicitly unknown and whose body renders without findings.
    let report = DoctorReport::minimal(sample_platform(), sample_arch());
    let rendered = render_report(&report);
    assert!(
        rendered.contains("unknown"),
        "minimal report must surface unknown version/commit: {rendered:?}"
    );
    assert!(
        !rendered.contains("fixture"),
        "minimal report must not carry stray findings: {rendered:?}"
    );
}

// ── Redaction is applied before rendering (AC-09 wiring) ───────────────────

#[test]
fn report_applies_redaction_to_findings() {
    // A finding that carries a sensitive home path must not leak it into the
    // rendered report, and the renderer must emit the exact redacted label
    // (not merely a substring that already appears in the raw input). Pinning
    // the concrete redacted form catches a no-op redactor that the previous
    // tautological fallback (`contains("config") || contains("home")`) could
    // not detect, since both substrings are present in the raw fixture.
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
    let expected_label = redact_value(sensitive);
    assert!(
        rendered.contains(&expected_label),
        "rendered report must contain the exact redacted label {expected_label:?}: {rendered:?}"
    );
}

#[test]
fn report_renders_a_finding_status_marker() {
    // Each finding line communicates its status through the concrete marker
    // glyph defined by `DiagnosticStatus::marker()` (`+`/`~`/`x`/`!`). Assert
    // on those exact glyphs rather than only that fail/pass reports differ,
    // which would pass even if no marker were rendered (the differing detail
    // text alone would satisfy `assert_ne`).
    let fail_finding = fail_finding(FindingKind::Multiplexer, "psmux missing");
    let pass_finding = pass_finding(FindingKind::Multiplexer, "psmux ready");
    let fail_rendered = render_report(&sample_report(&[fail_finding]));
    let pass_rendered = render_report(&sample_report(&[pass_finding]));
    assert!(
        fail_rendered.contains("[x] psmux missing"),
        "a failing finding must render the 'x' status marker before its detail: {fail_rendered:?}"
    );
    assert!(
        pass_rendered.contains("[+] psmux ready"),
        "a passing finding must render the '+' status marker before its detail: {pass_rendered:?}"
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
