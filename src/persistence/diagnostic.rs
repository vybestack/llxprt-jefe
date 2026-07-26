//! Closed configuration diagnostic vocabulary and inclusive bounds.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

pub use crate::domain::ByteSpan as Span;
use crate::domain::{CanonicalSemver, Id, ProvenanceOrigin};

/// Maximum settings or state document size in bytes.
pub const FILE_LIMIT: usize = 1_048_576;
/// Maximum recursive container depth, with the root at depth one.
pub const NESTING_LIMIT: usize = 16;
/// Maximum entries in one map.
pub const MAP_LIMIT: usize = 256;
/// Maximum elements in one array.
pub const ARRAY_LIMIT: usize = 1_024;
/// Maximum UTF-8 bytes in one string.
pub const STRING_LIMIT: usize = 262_144;
/// Maximum encoded bytes in one path.
pub const PATH_LIMIT: usize = 4_096;
/// Maximum diagnostics produced by one owning parser.
pub const DIAGNOSTIC_LIMIT: usize = 256;
/// Maximum provenance origins for one effective value.
pub const PROVENANCE_LIMIT: usize = 16;
/// Maximum effects in one transition.
pub const EFFECT_LIMIT: usize = 64;
/// Maximum completion-produced follow-up effects.
pub const FOLLOW_UP_LIMIT: usize = 64;

/// Closed configuration diagnostic code set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CfgCode {
    #[serde(rename = "CFG-E001")]
    E001,
    #[serde(rename = "CFG-E002")]
    E002,
    #[serde(rename = "CFG-E003")]
    E003,
    #[serde(rename = "CFG-W004")]
    W004,
    #[serde(rename = "CFG-E005")]
    E005,
    #[serde(rename = "CFG-E006")]
    E006,
    #[serde(rename = "CFG-E007")]
    E007,
    #[serde(rename = "CFG-E008")]
    E008,
    #[serde(rename = "CFG-E102")]
    E102,
    #[serde(rename = "CFG-E103")]
    E103,
    #[serde(rename = "CFG-E104")]
    E104,
}

impl CfgCode {
    /// Return the stable operator-facing code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E001 => "CFG-E001",
            Self::E002 => "CFG-E002",
            Self::E003 => "CFG-E003",
            Self::W004 => "CFG-W004",
            Self::E005 => "CFG-E005",
            Self::E006 => "CFG-E006",
            Self::E007 => "CFG-E007",
            Self::E008 => "CFG-E008",
            Self::E102 => "CFG-E102",
            Self::E103 => "CFG-E103",
            Self::E104 => "CFG-E104",
        }
    }
}

/// Diagnostic severity in required sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Canonical path used as a deterministic diagnostic key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiagnosticPath(String);

impl DiagnosticPath {
    /// Construct a canonical diagnostic path supplied by its owning parser.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Return the document root path.
    #[must_use]
    pub fn root() -> Self {
        Self("/".to_owned())
    }

    /// Borrow the canonical path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One redacted, deterministic configuration diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: CfgCode,
    pub severity: Severity,
    pub path: DiagnosticPath,
    pub span: Option<Span>,
    pub owner: Option<Id>,
    pub owner_version: Option<CanonicalSemver>,
    pub provenance: Vec<ProvenanceOrigin>,
    pub correction: String,
    pub redacted_detail: String,
}

impl Diagnostic {
    /// Construct a diagnostic without owner/provenance metadata.
    #[must_use]
    pub fn new(
        code: CfgCode,
        severity: Severity,
        path: DiagnosticPath,
        span: Option<Span>,
        correction: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            path,
            span,
            owner: None,
            owner_version: None,
            provenance: Vec::new(),
            correction: correction.into(),
            redacted_detail: String::new(),
        }
    }
}

impl Ord for Diagnostic {
    /// Order by report position first: severity, path, span, then code.
    ///
    /// The remaining fields break ties so that ordering stays consistent with
    /// `Eq`. Without them two diagnostics differing only in their detail would
    /// compare `Equal` while being unequal, which violates the `Ord` contract
    /// and makes sorting, deduplication and `BTree` keys unsound.
    fn cmp(&self, other: &Self) -> Ordering {
        self.severity
            .cmp(&other.severity)
            .then_with(|| self.path.cmp(&other.path))
            .then_with(|| self.span.cmp(&other.span))
            .then_with(|| self.code.as_str().cmp(other.code.as_str()))
            .then_with(|| self.owner.cmp(&other.owner))
            .then_with(|| self.correction.cmp(&other.correction))
            .then_with(|| self.redacted_detail.cmp(&other.redacted_detail))
            .then_with(|| {
                owner_version_key(self.owner_version.as_ref())
                    .cmp(&owner_version_key(other.owner_version.as_ref()))
            })
            .then_with(|| provenance_key(&self.provenance).cmp(&provenance_key(&other.provenance)))
    }
}

impl PartialOrd for Diagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Stable tiebreaker text for an optional owner version.
///
/// `CanonicalSemver` orders by SemVer precedence, under which distinct values
/// can be equal; comparing the rendered text keeps the total order consistent
/// with equality.
fn owner_version_key(version: Option<&CanonicalSemver>) -> String {
    version.map(ToString::to_string).unwrap_or_default()
}

/// Stable tiebreaker text for a provenance chain, which carries no ordering.
fn provenance_key(provenance: &[ProvenanceOrigin]) -> String {
    provenance
        .iter()
        .map(|origin| format!("{origin:?}"))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

/// Validate an inclusive bound without truncating the owning value.
pub fn validate_inclusive_limit(
    actual: usize,
    limit: usize,
    path: DiagnosticPath,
) -> Result<(), Box<Diagnostic>> {
    if actual <= limit {
        return Ok(());
    }
    let mut diagnostic = Diagnostic::new(
        CfgCode::E008,
        Severity::Error,
        path,
        None,
        format!("reduce the value to at most {limit}"),
    );
    diagnostic.redacted_detail = format!("observed {actual}; inclusive limit {limit}");
    Err(Box::new(diagnostic))
}

#[cfg(test)]
mod ord_contract_tests {
    use super::{CfgCode, Diagnostic, DiagnosticPath, Severity};

    /// `Ord` must agree with `Eq`: values that compare `Equal` have to be equal.
    /// Diagnostics differing only in their detail previously ordered as equal
    /// while comparing unequal, which makes sorting and `BTree` use unsound.
    #[test]
    fn ordering_is_consistent_with_equality() {
        let base = Diagnostic::new(
            CfgCode::E008,
            Severity::Error,
            DiagnosticPath::root(),
            None,
            "correct it",
        );
        let mut other = base.clone();
        other.redacted_detail = "a different detail".to_owned();

        assert_ne!(base, other, "the two diagnostics are not equal");
        assert_ne!(
            base.cmp(&other),
            std::cmp::Ordering::Equal,
            "unequal diagnostics must not compare as equal"
        );
    }
}
