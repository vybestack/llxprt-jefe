//! Stable machine-readable compliance reports (issue 477).
//!
//! The compliance CLI emits one stable JSON document on success and
//! deterministic ordered failures on non-compliance. Reports never echo
//! producer payload content; only stable identifiers, invariant codes,
//! scenario/step identifiers, and cursor/sequence numbers.

use serde::{Deserialize, Serialize};

/// The overall outcome of a compliance run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportOutcome {
    Pass,
    Fail,
}

impl ReportOutcome {
    /// The stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// A stable, machine-readable compliance report.
///
/// This is the top-level document the CLI writes to stdout. It is versioned
/// independently from the fixture version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityReport {
    /// Report schema version (independent from JSP/1 schema).
    pub report_version: u64,
    /// The compliance artifact version under test.
    pub compliance_artifact_version: String,
    /// Which profile was run.
    pub profile: String,
    /// Overall outcome.
    pub outcome: ReportOutcome,
    /// Total checks evaluated.
    pub checks_total: usize,
    /// Checks that passed.
    pub checks_passed: usize,
    /// Checks that failed, in deterministic order.
    pub failures: Vec<StabilityFailure>,
}

/// One stable failure entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityFailure {
    /// The acceptance row or invariant identifier (e.g. "C1", "producer.monotonic_ordering").
    pub invariant: String,
    /// The scenario id, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The step index, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    /// The expected cursor/sequence as a typed value, populated from
    /// [`StepOutcome`](super::scenario::StepOutcome) or
    /// [`ReducerError::Gap`](super::reducer::ReducerError) without string
    /// parsing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_sequence: Option<u64>,
    /// The actual cursor/sequence as a typed value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sequence: Option<u64>,
    /// A safe, payload-free detail string.
    pub detail: String,
}

impl StabilityReport {
    /// Build a passing report.
    #[must_use]
    pub fn pass(profile: &str, artifact_version: &str, checks: usize) -> Self {
        Self {
            report_version: 1,
            compliance_artifact_version: artifact_version.to_string(),
            profile: profile.to_string(),
            outcome: ReportOutcome::Pass,
            checks_total: checks,
            checks_passed: checks,
            failures: Vec::new(),
        }
    }

    /// Build a failing report from a list of failures.
    ///
    /// The totals are internally consistent and deterministic: if the caller
    /// supplies more failures than the declared `checks_total`, the total is
    /// raised to the failure count so that `checks_passed + failures.len() ==
    /// checks_total` always holds and never silently saturates to zero.
    #[must_use]
    pub fn fail(
        profile: &str,
        artifact_version: &str,
        checks_total: usize,
        failures: Vec<StabilityFailure>,
    ) -> Self {
        let total = checks_total.max(failures.len());
        let checks_passed = total - failures.len();
        Self {
            report_version: 1,
            compliance_artifact_version: artifact_version.to_string(),
            profile: profile.to_string(),
            outcome: ReportOutcome::Fail,
            checks_total: total,
            checks_passed,
            failures,
        }
    }

    /// Serialize to a pretty JSON string.
    ///
    /// # Errors
    /// Only if serialization fails, which should not happen for this shape.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// The compliance artifact version string.
pub const COMPLIANCE_ARTIFACT_VERSION: &str = "jsp-v1-compliance-1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_report_serializes() {
        let report = StabilityReport::pass("schema", COMPLIANCE_ARTIFACT_VERSION, 5);
        let s = report
            .to_json_string()
            .unwrap_or_else(|e| panic!("serializes: {e}"));
        assert!(s.contains("\"outcome\": \"pass\""));
        assert!(s.contains("\"checks_passed\": 5"));
    }

    #[test]
    fn fail_report_carries_failures() {
        let report = StabilityReport::fail(
            "reducer",
            COMPLIANCE_ARTIFACT_VERSION,
            3,
            vec![StabilityFailure {
                invariant: "C2".to_string(),
                scenario: Some("S10".to_string()),
                step: Some(1),
                expected_sequence: Some(2),
                actual_sequence: Some(3),
                detail: "sequence gap".to_string(),
            }],
        );
        assert_eq!(report.outcome, ReportOutcome::Fail);
        assert_eq!(report.checks_passed, 2);
        let s = report
            .to_json_string()
            .unwrap_or_else(|e| panic!("serializes: {e}"));
        assert!(s.contains("C2"));
    }

    #[test]
    fn fail_report_totals_are_internally_consistent_when_failures_exceed_checks() {
        // When a caller supplies more failures than declared checks, the totals
        // must remain internally consistent (passed + failures == total) and
        // must never silently saturate to zero.
        let failures: Vec<StabilityFailure> = (0..5)
            .map(|index| StabilityFailure {
                invariant: format!("F{index}"),
                scenario: None,
                step: None,
                expected_sequence: None,
                actual_sequence: None,
                detail: "overflow".to_string(),
            })
            .collect();
        let failure_count = failures.len();
        let report = StabilityReport::fail("all", COMPLIANCE_ARTIFACT_VERSION, 2, failures);
        assert_eq!(report.outcome, ReportOutcome::Fail);
        assert_eq!(
            report.checks_total, failure_count,
            "total is raised to the failure count"
        );
        assert_eq!(report.checks_passed, 0);
        assert_eq!(
            report.checks_passed + report.failures.len(),
            report.checks_total,
            "passed + failures == total invariant holds"
        );
    }
}
