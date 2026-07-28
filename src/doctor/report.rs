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
    /// Returns an error only if the version string is empty. The commit string
    /// may be empty in development builds without `JEFE_GIT_COMMIT`; the
    /// renderer surfaces that as `unavailable` rather than fabricating a value,
    /// satisfying AC-04's "unavailable commit is explicitly reported" contract.
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
    // AC-04: an unavailable commit is explicitly reported as unavailable,
    // never blank or fabricated, so a report attached to an issue is truthful.
    let commit = report.commit();
    let commit_label = if commit.trim().is_empty() {
        "unavailable"
    } else {
        commit
    };
    let _ = writeln!(out, "Git commit   : {commit_label}");
    let _ = writeln!(out, "Platform     : {}", report.platform());
    let _ = writeln!(out, "Architecture : {}", report.arch());
}

/// Write one section per finding kind present in the report, redacting each
/// detail before output. Only kinds that have at least one finding produce a
/// section, so every section header is followed by at least one finding line.
///
/// Issue #447: the findings are grouped into canonical-kind order in a single
/// pass, and each group is emitted directly, so neither the section ordering
/// nor the per-kind emission re-scans the full findings slice.
fn write_findings(out: &mut String, report: &DoctorReport) {
    for group in group_findings_by_kind(report.findings()) {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", group.kind.label());
        for finding in group.findings {
            let detail = redact_value(finding.detail());
            let _ = writeln!(out, "  [{}] {}", finding.status().marker(), detail);
        }
    }
}

/// The canonical, fixed ordering of all reported finding kinds.
///
/// This is the single source of truth for section order. Adding a kind here
/// is the only change required to extend the report's section set.
const fn canonical_kinds() -> [FindingKind; 10] {
    [
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
    ]
}

/// The stable ordering of section kinds present in the report's findings.
///
/// Only kinds that have at least one finding appear, in canonical order. Kinds
/// with no finding are omitted, so every emitted section is non-empty.
///
/// Issue #447: this performs a single pass over `findings` (via
/// [`group_findings_by_kind`]) rather than an O(kinds × findings) scan, then
/// projects the grouped kinds. It is retained as a `#[cfg(test)]` contract
/// surface so the ordering behavior stays pinned independently of the renderer.
#[cfg(test)]
fn ordered_section_kinds(findings: &[DiagnosticFinding]) -> Vec<FindingKind> {
    group_findings_by_kind(findings)
        .into_iter()
        .map(|group| group.kind)
        .collect()
}

/// A contiguous run of findings sharing one kind, in canonical order.
struct KindGroup<'a> {
    kind: FindingKind,
    findings: Vec<&'a DiagnosticFinding>,
}

/// Group `findings` into canonical-kind order in a single pass.
///
/// Each finding is visited exactly once and appended to its kind's bucket. The
/// returned groups are ordered by [`canonical_kinds`] and contain only kinds
/// with at least one finding, preserving input order within each group. This
/// is the shared grouping that drives both section ordering and per-section
/// emission, eliminating the previous O(kinds × findings) scans.
fn group_findings_by_kind(findings: &[DiagnosticFinding]) -> Vec<KindGroup<'_>> {
    let canonical = canonical_kinds();
    // Index each canonical kind once so the single pass can bucket in O(1).
    let mut index = std::collections::HashMap::with_capacity(canonical.len());
    for (pos, kind) in canonical.iter().enumerate() {
        index.insert(kind, pos);
    }
    let mut buckets: Vec<Vec<&DiagnosticFinding>> =
        (0..canonical.len()).map(|_| Vec::new()).collect();
    for finding in findings {
        if let Some(&pos) = index.get(&finding.kind()) {
            buckets[pos].push(finding);
        }
    }
    buckets
        .into_iter()
        .zip(canonical.iter())
        .filter(|(bucket, _)| !bucket.is_empty())
        .map(|(bucket, kind)| KindGroup {
            kind: *kind,
            findings: bucket,
        })
        .collect()
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

    fn pass_finding(kind: FindingKind, detail: &str) -> DiagnosticFinding {
        DiagnosticFinding::new(kind, DiagnosticStatus::Pass, detail.to_string())
    }

    // Issue #447: `ordered_section_kinds` must produce the canonical ordering
    // from a single pass over the findings slice, deduplicating repeated kinds
    // and omitting absent ones, regardless of input order.
    #[test]
    fn ordered_section_kinds_returns_canonical_order_for_all_kinds() {
        let findings: Vec<DiagnosticFinding> = [
            pass_finding(FindingKind::DiagnosticsInternal, "diag"),
            pass_finding(FindingKind::LongPath, "longpath"),
            pass_finding(FindingKind::Persistence, "persist"),
            pass_finding(FindingKind::CodePuppy, "cp"),
            pass_finding(FindingKind::LlxprtCode, "lc"),
            pass_finding(FindingKind::GhAuth, "gh"),
            pass_finding(FindingKind::Git, "git"),
            pass_finding(FindingKind::ConPty, "conpty"),
            pass_finding(FindingKind::Namespace, "ns"),
            pass_finding(FindingKind::Multiplexer, "mux"),
        ]
        .to_vec();
        assert_eq!(
            ordered_section_kinds(&findings),
            vec![
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
            ]
        );
    }

    #[test]
    fn ordered_section_kinds_deduplicates_repeated_kinds() {
        let findings = vec![
            pass_finding(FindingKind::Git, "a"),
            pass_finding(FindingKind::Git, "b"),
            pass_finding(FindingKind::Multiplexer, "m"),
            pass_finding(FindingKind::Git, "c"),
        ];
        assert_eq!(
            ordered_section_kinds(&findings),
            vec![FindingKind::Multiplexer, FindingKind::Git]
        );
    }

    #[test]
    fn ordered_section_kinds_omits_absent_kinds() {
        let findings = vec![pass_finding(FindingKind::Persistence, "p")];
        assert_eq!(
            ordered_section_kinds(&findings),
            vec![FindingKind::Persistence]
        );
    }

    // Regression guards for issue #447: the refactor must not change observable
    // output. Pin canonical section ordering, per-kind grouping, status markers,
    // and redaction for a representative multi-kind/multi-status input. The
    // assertions are split across two tests so each stays readable and well
    // under the function-line budget.
    fn regression_findings() -> [DiagnosticFinding; 4] {
        [
            DiagnosticFinding::new(
                FindingKind::Git,
                DiagnosticStatus::Pass,
                "git 2.43.0".to_string(),
            ),
            DiagnosticFinding::new(
                FindingKind::Multiplexer,
                DiagnosticStatus::Fail,
                "psmux missing".to_string(),
            ),
            DiagnosticFinding::new(
                FindingKind::Git,
                DiagnosticStatus::Warn,
                "old git".to_string(),
            ),
            DiagnosticFinding::new(
                FindingKind::Persistence,
                DiagnosticStatus::CommandError,
                "/home/user/.config/jefe".to_string(),
            ),
        ]
    }

    #[test]
    fn render_report_preserves_canonical_section_order_and_grouping() {
        let rendered = render_report(&report(&regression_findings()));
        let Some(mux) = rendered.find("Multiplexer") else {
            panic!("Multiplexer header missing: {rendered:?}");
        };
        let Some(git) = rendered.find("\nGit\n") else {
            panic!("Git header missing: {rendered:?}");
        };
        let Some(persist) = rendered.find("Config / state persistence") else {
            panic!("Persistence header missing: {rendered:?}");
        };
        assert!(mux < git, "Multiplexer must precede Git: {rendered:?}");
        assert!(git < persist, "Git must precede Persistence: {rendered:?}");
        assert_eq!(
            rendered.matches("\nGit\n").count(),
            1,
            "single Git header for two Git findings: {rendered:?}"
        );
        let Some(first_git_line) = rendered.find("[+] git 2.43.0") else {
            panic!("pass marker + detail missing: {rendered:?}");
        };
        let Some(second_git_line) = rendered.find("[~] old git") else {
            panic!("warn marker + detail missing: {rendered:?}");
        };
        assert!(
            first_git_line < second_git_line,
            "Git findings keep input order"
        );
    }

    #[test]
    fn render_report_preserves_status_markers_and_redaction() {
        let rendered = render_report(&report(&regression_findings()));
        assert!(
            rendered.contains("[x] psmux missing"),
            "fail marker: {rendered:?}"
        );
        assert!(
            rendered.contains("[!] /home/[redacted-user]/.config/jefe"),
            "command-error marker + redacted path: {rendered:?}"
        );
        // Redaction removes the bare username. The marker `[redacted-user]`
        // legitimately contains the substring "user", so assert the original
        // `/home/user/` path is gone rather than the bare substring.
        assert!(
            !rendered.contains("/home/user/"),
            "raw home path must be redacted: {rendered:?}"
        );
    }
}
