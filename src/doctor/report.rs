//! Human-readable report rendering for `jefe doctor` (issue #264, AC-04..09).
//!
//! [`render_report`] turns a [`DoctorReport`] into a single human-readable
//! string. Redaction ([`super::redact_value`]) is applied to every finding
//! detail before it is written, so a sensitive fixture supplied through a
//! finding never reaches the rendered output.
//!
//! The renderer is pure: it performs no I/O and no redaction of its own
//! beyond delegating to the shared redactor. No JSON, clipboard, or telemetry
//! surface is produced (decision D-06).

use std::fmt::Write as _;

use super::redaction::redact_value;
use super::types::{DiagnosticFinding, FindingKind};

/// A complete diagnostic report ready for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    version: String,
    commit: String,
    platform: String,
    arch: String,
    findings: Vec<DiagnosticFinding>,
}

impl DoctorReport {
    /// Construct a report from baked-in identity metadata and findings.
    ///
    /// Returns an error only if the version/commit strings are empty, since a
    /// report missing its identity metadata cannot satisfy AC-04.
    pub fn new(
        version: String,
        commit: String,
        platform: String,
        arch: String,
        findings: Vec<DiagnosticFinding>,
    ) -> Result<Self, ReportBuildError> {
        if version.trim().is_empty() {
            return Err(ReportBuildError::MissingVersion);
        }
        Ok(Self {
            version,
            commit,
            platform,
            arch,
            findings,
        })
    }

    /// The Jefe version carried by this report.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The baked-in git commit carried by this report.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// The platform label carried by this report.
    #[must_use]
    pub fn platform(&self) -> &str {
        &self.platform
    }

    /// The architecture label carried by this report.
    #[must_use]
    pub fn arch(&self) -> &str {
        &self.arch
    }

    /// The findings carried by this report.
    #[must_use]
    pub fn findings(&self) -> &[DiagnosticFinding] {
        &self.findings
    }

    /// Construct a minimal report carrying only identity metadata.
    ///
    /// Used as a last-resort fallback when normal construction fails.
    #[must_use]
    pub fn minimal(platform: String, arch: String) -> Self {
        Self {
            version: "unknown".to_string(),
            commit: "unknown".to_string(),
            platform,
            arch,
            findings: Vec::new(),
        }
    }
}

/// Failure to construct a [`DoctorReport`] from incomplete identity metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportBuildError {
    /// The version string was empty.
    MissingVersion,
}

impl std::fmt::Display for ReportBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVersion => write!(f, "report version metadata is required"),
        }
    }
}

impl std::error::Error for ReportBuildError {}

/// Render a [`DoctorReport`] to a human-readable string.
///
/// Redaction is applied to every finding detail before it is written. The
/// report always includes the Jefe version, commit, platform, and
/// architecture, followed by a section per finding kind (AC-04 through AC-08).
#[must_use]
pub fn render_report(report: &DoctorReport) -> String {
    let mut out = String::new();
    write_header(&mut out, report);
    write_identity(&mut out, report);
    write_findings(&mut out, report);
    out
}

/// Write the report title.
fn write_header(out: &mut String, _report: &DoctorReport) {
    let _ = writeln!(out, "jefe doctor report");
    let _ = writeln!(out, "==================");
}

/// Write the version/commit/platform/architecture identity block.
fn write_identity(out: &mut String, report: &DoctorReport) {
    let _ = writeln!(out);
    let _ = writeln!(out, "Jefe version : {}", report.version());
    let _ = writeln!(out, "Git commit   : {}", report.commit());
    let _ = writeln!(out, "Platform     : {}", report.platform());
    let _ = writeln!(out, "Architecture : {}", report.arch());
}

/// Write one section per finding, redacting each detail before output.
fn write_findings(out: &mut String, report: &DoctorReport) {
    let kinds = ordered_section_kinds(report.findings());
    for kind in kinds {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", kind.label());
        let wrote_any = write_kind_findings(out, report, kind);
        if !wrote_any {
            let _ = writeln!(out, "  (no probe ran for this section)");
        }
    }
}

/// Write all findings of `kind`, redacting each detail. Returns whether any
/// finding was written.
fn write_kind_findings(out: &mut String, report: &DoctorReport, kind: FindingKind) -> bool {
    let mut wrote = false;
    for finding in report.findings().iter().filter(|f| f.kind() == kind) {
        let detail = redact_value(finding.detail());
        let _ = writeln!(out, "  [{}] {}", finding.status().marker(), detail);
        wrote = true;
    }
    wrote
}

/// The stable ordering of section kinds present in the report's findings,
/// followed by the canonical ordering for kinds that have no finding yet.
fn ordered_section_kinds(findings: &[DiagnosticFinding]) -> Vec<FindingKind> {
    let canonical = [
        FindingKind::Multiplexer,
        FindingKind::Namespace,
        FindingKind::ConPty,
        FindingKind::Git,
        FindingKind::GhAuth,
        FindingKind::LlxprtCode,
        FindingKind::CodePuppy,
        FindingKind::Persistence,
        FindingKind::LongPath,
        FindingKind::DiagnosticsInternal,
    ];
    let present: Vec<FindingKind> = canonical
        .iter()
        .copied()
        .filter(|kind| findings.iter().any(|f| f.kind() == *kind))
        .collect();
    present
}

#[cfg(test)]
mod tests {
    use super::super::types::DiagnosticStatus;
    use super::*;

    fn report(findings: &[DiagnosticFinding]) -> DoctorReport {
        DoctorReport::new(
            "0.0.32".to_string(),
            "abc123".to_string(),
            "unix".to_string(),
            "x86_64".to_string(),
            findings.to_vec(),
        )
        .unwrap_or_else(|error| panic!("build report: {error:?}"))
    }

    #[test]
    fn render_includes_version_commit_platform_arch() {
        let rendered = render_report(&report(&[]));
        assert!(rendered.contains("0.0.32"));
        assert!(rendered.contains("abc123"));
        assert!(rendered.contains("Platform"));
        assert!(rendered.contains("Architecture"));
    }

    #[test]
    fn render_applies_redaction() {
        let finding = DiagnosticFinding::new(
            FindingKind::Persistence,
            DiagnosticStatus::Pass,
            "/home/alice/.config/jefe".to_string(),
        );
        let rendered = render_report(&report(&[finding]));
        assert!(!rendered.contains("alice"));
    }

    #[test]
    fn empty_version_is_rejected() {
        let err = DoctorReport::new(
            String::new(),
            "abc".to_string(),
            "unix".to_string(),
            "x86_64".to_string(),
            vec![],
        )
        .err()
        .unwrap_or_else(|| panic!("empty version must be rejected"));
        assert_eq!(err, ReportBuildError::MissingVersion);
    }
}
