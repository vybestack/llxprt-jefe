//! Exit-code classification for `jefe doctor` findings.
//!
//! Pure reduction of a slice of [`DiagnosticFinding`] into a typed
//! [`DoctorOutcome`], independent of how the findings were collected. This
//! keeps the exit contract (decision D-04) deterministic and unit-testable
//! without spawning any real subprocesses.
//!
//! # Exit contract
//!
//! - `Ok` (exit 0): no required blocker failed. Optional warnings do not
//!   change the outcome.
//! - `BlockingFailure` (exit 2): at least one required startup blocker
//!   (`Multiplexer`, `ConPty`, `Persistence`) failed.
//! - `CommandError` (exit 1): the diagnostic command itself could not
//!   complete. This dominates every other outcome because the report is not
//!   trustworthy when an internal probe failed.

use super::types::{DiagnosticFinding, DiagnosticStatus};

/// The coarse outcome of running the diagnostic command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DoctorOutcome {
    /// No required blocker failed; optional findings may warn.
    Ok,
    /// A required startup blocker failed (exit 2).
    BlockingFailure,
    /// The diagnostic command itself could not complete (exit 1).
    CommandError,
}

impl DoctorOutcome {
    /// The process exit code for this outcome.
    #[must_use]
    pub const fn exit_code(self) -> ExitCode {
        match self {
            Self::Ok => ExitCode::OK,
            Self::BlockingFailure => ExitCode::BLOCKING_FAILURE,
            Self::CommandError => ExitCode::COMMAND_ERROR,
        }
    }
}

/// A typed, bounded process exit code for `jefe doctor`.
///
/// Wrapping the raw code keeps call sites from inventing undocumented exit
/// values and makes the contract grep-friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExitCode(u8);

impl ExitCode {
    /// Successful diagnostics with no required blocker (0).
    pub const OK: Self = Self(0);
    /// A required startup blocker failed (2).
    pub const BLOCKING_FAILURE: Self = Self(2);
    /// The diagnostic command itself failed (1).
    pub const COMMAND_ERROR: Self = Self(1);

    /// Construct from a raw code. Kept `const` so callers cannot panic.
    #[must_use]
    pub const fn from_u8(value: u8) -> Self {
        Self(value)
    }

    /// The raw exit code value.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        code.as_u8().into()
    }
}

/// Classify a collection of findings into a typed outcome.
///
/// Command errors dominate; a required-blocker failure otherwise dominates;
/// otherwise the outcome is `Ok`. See the module docs for the full contract.
#[must_use]
pub fn classify_doctor(findings: &[DiagnosticFinding]) -> DoctorOutcome {
    if findings
        .iter()
        .any(|f| f.status() == DiagnosticStatus::CommandError)
    {
        return DoctorOutcome::CommandError;
    }
    if findings.iter().any(is_blocking_failure) {
        return DoctorOutcome::BlockingFailure;
    }
    DoctorOutcome::Ok
}

/// Whether a finding is a required startup blocker that failed.
fn is_blocking_failure(finding: &DiagnosticFinding) -> bool {
    finding.status() == DiagnosticStatus::Fail && finding.kind().is_required_blocker()
}

#[cfg(test)]
mod tests {
    use super::super::types::FindingKind;
    use super::*;

    fn finding(kind: FindingKind, status: DiagnosticStatus) -> DiagnosticFinding {
        DiagnosticFinding::new(kind, status, String::new())
    }

    #[test]
    fn empty_findings_are_ok() {
        assert_eq!(classify_doctor(&[]), DoctorOutcome::Ok);
    }

    #[test]
    fn warn_only_findings_are_ok() {
        let findings = [
            finding(FindingKind::Git, DiagnosticStatus::Warn),
            finding(FindingKind::GhAuth, DiagnosticStatus::Warn),
        ];
        assert_eq!(classify_doctor(&findings), DoctorOutcome::Ok);
    }

    #[test]
    fn optional_fail_does_not_block() {
        // A failed optional finding (e.g. Git missing) must not block.
        let findings = [finding(FindingKind::Git, DiagnosticStatus::Fail)];
        assert_eq!(classify_doctor(&findings), DoctorOutcome::Ok);
    }

    #[test]
    fn command_error_dominates_blocking_failure() {
        let findings = [
            finding(FindingKind::Multiplexer, DiagnosticStatus::Fail),
            finding(
                FindingKind::DiagnosticsInternal,
                DiagnosticStatus::CommandError,
            ),
        ];
        assert_eq!(classify_doctor(&findings), DoctorOutcome::CommandError);
    }

    #[test]
    fn exit_codes_match_contract() {
        assert_eq!(DoctorOutcome::Ok.exit_code(), ExitCode::from_u8(0));
        assert_eq!(
            DoctorOutcome::BlockingFailure.exit_code(),
            ExitCode::from_u8(2)
        );
        assert_eq!(
            DoctorOutcome::CommandError.exit_code(),
            ExitCode::from_u8(1)
        );
    }
}
