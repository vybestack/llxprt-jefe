//! Coded error taxonomy for the schema-1 harness (issue #380).
//!
//! Every failure carries one `HAR-E001..E007` code plus a redactable detail
//! string. Exit codes follow the issue contract: validation 2, I/O and
//! process 4, timeout 124, success 0.

/// The closed `HAR-E001..E007` diagnostic code set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarCode {
    /// Syntax, duplicate key, or unknown field.
    E001,
    /// A bounded resource exceeded its inclusive limit.
    E002,
    /// Invalid `${...}` interpolation.
    E003,
    /// Containment or filesystem race violation.
    E004,
    /// Process or PTY failure.
    E005,
    /// Assertion mismatch.
    E006,
    /// Cleanup failure.
    E007,
}

impl HarCode {
    /// The stable diagnostic label, e.g. `HAR-E001`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::E001 => "HAR-E001",
            Self::E002 => "HAR-E002",
            Self::E003 => "HAR-E003",
            Self::E004 => "HAR-E004",
            Self::E005 => "HAR-E005",
            Self::E006 => "HAR-E006",
            Self::E007 => "HAR-E007",
        }
    }
}

/// A harness failure: one code, a detail message, and a timeout marker.
///
/// The timeout marker exists because timeouts share the process code space
/// (`HAR-E005`) but must map to exit 124 rather than 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessError {
    pub code: HarCode,
    pub detail: String,
    pub timeout: bool,
}

impl HarnessError {
    #[must_use]
    pub fn new(code: HarCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            timeout: false,
        }
    }

    /// Syntax, duplicate-key, or unknown-field failure (`HAR-E001`).
    #[must_use]
    pub fn syntax(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E001, detail)
    }

    /// Inclusive-bound violation (`HAR-E002`).
    #[must_use]
    pub fn limit(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E002, detail)
    }

    /// Interpolation violation (`HAR-E003`).
    #[must_use]
    pub fn interpolation(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E003, detail)
    }

    /// Containment or race violation (`HAR-E004`).
    #[must_use]
    pub fn containment(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E004, detail)
    }

    /// Process or PTY failure (`HAR-E005`).
    #[must_use]
    pub fn process(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E005, detail)
    }

    /// Assertion mismatch (`HAR-E006`).
    #[must_use]
    pub fn assertion(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E006, detail)
    }

    /// Cleanup failure (`HAR-E007`).
    #[must_use]
    pub fn cleanup(detail: impl Into<String>) -> Self {
        Self::new(HarCode::E007, detail)
    }

    /// A wait or process bound breach that must exit 124.
    #[must_use]
    pub fn wait_timeout(detail: impl Into<String>) -> Self {
        Self {
            code: HarCode::E005,
            detail: detail.into(),
            timeout: true,
        }
    }

    /// Map this failure to the contract exit code.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        if self.timeout {
            return 124;
        }
        match self.code {
            HarCode::E001 | HarCode::E002 | HarCode::E003 => 2,
            HarCode::E004 | HarCode::E005 | HarCode::E006 | HarCode::E007 => 4,
        }
    }
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.label(), self.detail)
    }
}

impl std::error::Error for HarnessError {}
