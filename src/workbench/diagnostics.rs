//! Screen composition diagnostics (issue #385).
//!
//! Configuration diagnostics have their own closed, serialized vocabulary in
//! [`crate::persistence::diagnostic`], and widening it would change a persisted
//! contract for a failure that is not about configuration files at all. Screen
//! composition therefore has its own small code family, and a composition
//! failure reports both: the `SCR` code says the screen registry was refused,
//! and the accompanying `CFG` diagnostic says which rule the offending file
//! broke.
//!
//! Diagnostics carry paths, spans, and rule names — never a value read out of a
//! definition file. A definition cannot declare a secret, but it can declare a
//! title or a config string, and none of those belong in a log line.

use crate::domain::ByteSpan;
use crate::persistence::diagnostic::{DiagnosticPath, Severity};

/// Closed screen-composition diagnostic code set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScrCode {
    /// The candidate screen registry was refused; prior authority is retained.
    E301,
}

impl ScrCode {
    /// The stable operator-facing code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E301 => "SCR-E301",
        }
    }
}

impl std::fmt::Display for ScrCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One redacted screen-composition diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDiagnostic {
    /// Stable code.
    pub code: ScrCode,
    /// How badly it went.
    pub severity: Severity,
    /// The definition file the failure is attributable to.
    pub path: DiagnosticPath,
    /// Byte range within that file, when one can be attributed.
    pub span: Option<ByteSpan>,
    /// What the operator should do.
    pub correction: String,
    /// The violated rule, with no value from the file.
    pub redacted_detail: String,
}

impl ScreenDiagnostic {
    /// Build one composition-refusal diagnostic.
    #[must_use]
    pub fn refused(
        path: DiagnosticPath,
        span: Option<ByteSpan>,
        redacted_detail: impl Into<String>,
    ) -> Self {
        Self {
            code: ScrCode::E301,
            severity: Severity::Error,
            path,
            span,
            correction: "correct or disable the named screen definition, then restart".to_owned(),
            redacted_detail: redacted_detail.into(),
        }
    }
}

impl std::fmt::Display for ScreenDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} {}: {}",
            self.code,
            self.path.as_str(),
            self.redacted_detail
        )
    }
}

impl std::error::Error for ScreenDiagnostic {}
