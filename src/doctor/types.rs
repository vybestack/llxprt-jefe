//! Typed diagnostic domain model for `jefe doctor`.
//!
//! These types are the pure vocabulary shared by classification, rendering,
//! and the runtime collector. They carry no side effects so they can be
//! exercised deterministically in unit tests.

/// The diagnostic area a finding reports.
///
/// Variants group findings into the report sections required by the issue
/// acceptance matrix (AC-04 through AC-08). They also drive classification:
/// `Multiplexer`, `ConPty`, and `Persistence` are required startup blockers,
/// while the remaining kinds are optional feature prerequisites that warn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingKind {
    /// Local multiplexer (tmux on Unix, psmux on Windows) readiness.
    Multiplexer,
    /// Private multiplexer isolation (socket / namespace).
    Namespace,
    /// Windows ConPTY pseudo-console availability.
    ConPty,
    /// Git command-line client presence and version.
    Git,
    /// GitHub CLI (`gh`) presence and authentication.
    GhAuth,
    /// LLxprt Code agent runtime presence.
    LlxprtCode,
    /// Code Puppy agent runtime presence.
    CodePuppy,
    /// Config/state directory writability.
    Persistence,
    /// Windows long-path policy limitations.
    LongPath,
    /// The diagnostic command itself failed to complete.
    DiagnosticsInternal,
}

impl FindingKind {
    /// The human-readable section label used in the rendered report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Multiplexer => "Multiplexer",
            Self::Namespace => "Namespace",
            Self::ConPty => "ConPTY",
            Self::Git => "Git",
            Self::GhAuth => "gh / GitHub auth",
            Self::LlxprtCode => "LLxprt Code",
            Self::CodePuppy => "Code Puppy",
            Self::Persistence => "Config / state persistence",
            Self::LongPath => "Long-path support",
            Self::DiagnosticsInternal => "Diagnostics internal",
        }
    }

    /// Whether a failed finding of this kind blocks Jefe startup.
    ///
    /// Required blockers are the multiplexer, ConPTY (platform-dependent but
    /// classified purely from the probe result), and the configured
    /// persistence path. Optional findings never block startup (D-04).
    #[must_use]
    pub const fn is_required_blocker(self) -> bool {
        matches!(self, Self::Multiplexer | Self::ConPty | Self::Persistence)
    }
}

/// The status outcome of a single diagnostic probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticStatus {
    /// The probe succeeded.
    Pass,
    /// The probe succeeded with a non-blocking caveat (optional prerequisite).
    Warn,
    /// The probe failed and the finding blocks startup (required prerequisite).
    Fail,
    /// The diagnostic command itself could not complete this probe.
    CommandError,
}

impl DiagnosticStatus {
    /// The single-character marker rendered beside a finding line.
    #[must_use]
    pub const fn marker(self) -> char {
        match self {
            Self::Pass => '+',
            Self::Warn => '~',
            Self::Fail => 'x',
            Self::CommandError => '!',
        }
    }
}

/// A single diagnostic finding produced by a probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFinding {
    kind: FindingKind,
    status: DiagnosticStatus,
    detail: String,
}

impl DiagnosticFinding {
    /// Construct a finding from its kind, status, and evidence detail.
    #[must_use]
    pub fn new(kind: FindingKind, status: DiagnosticStatus, detail: String) -> Self {
        Self {
            kind,
            status,
            detail,
        }
    }

    /// The diagnostic area this finding reports.
    #[must_use]
    pub const fn kind(&self) -> FindingKind {
        self.kind
    }

    /// The status outcome of this finding.
    #[must_use]
    pub const fn status(&self) -> DiagnosticStatus {
        self.status
    }

    /// The (already-redacted at render time) evidence detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_blocker_set_matches_decision_d04() {
        assert!(FindingKind::Multiplexer.is_required_blocker());
        assert!(FindingKind::ConPty.is_required_blocker());
        assert!(FindingKind::Persistence.is_required_blocker());
        assert!(!FindingKind::Git.is_required_blocker());
        assert!(!FindingKind::GhAuth.is_required_blocker());
        assert!(!FindingKind::LlxprtCode.is_required_blocker());
        assert!(!FindingKind::CodePuppy.is_required_blocker());
        assert!(!FindingKind::LongPath.is_required_blocker());
    }

    #[test]
    fn status_markers_are_distinct() {
        let markers = [
            DiagnosticStatus::Pass.marker(),
            DiagnosticStatus::Warn.marker(),
            DiagnosticStatus::Fail.marker(),
            DiagnosticStatus::CommandError.marker(),
        ];
        let unique: std::collections::HashSet<char> = markers.iter().copied().collect();
        assert_eq!(unique.len(), markers.len(), "markers must be distinct");
    }
}
