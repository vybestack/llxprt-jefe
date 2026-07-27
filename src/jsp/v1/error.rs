//! Stable JSP/1 error code taxonomy (issue #476, J1 slice, decision 14).
//!
//! Six stable error codes cover the closed parser surface. Diagnostics carry
//! a stable code and a safe, payload-free detail string. The parser never
//! echoes producer input values in diagnostics.

use std::fmt;

/// Stable error codes for JSP/1.
///
/// These strings are part of the wire contract and must not change without a
/// version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JspCode {
    /// `JSP-E001` — closed JSON syntax/shape: malformed JSON, unknown field,
    /// duplicate field, wrong type, trailing data.
    EClosedShape,
    /// `JSP-E002` — inclusive bound exceeded (size, length, count).
    EBound,
    /// `JSP-E003` — unsupported version or kind.
    EUnsupportedVersion,
    /// `JSP-E004` — identity/binding invariant violation.
    EIdentity,
    /// `JSP-E005` — field-state violation (illegal state algebra).
    EFieldState,
    /// `JSP-E006` — snapshot semantic invariant violation.
    ESemantic,
}

impl JspCode {
    /// The stable wire string for this code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EClosedShape => "JSP-E001",
            Self::EBound => "JSP-E002",
            Self::EUnsupportedVersion => "JSP-E003",
            Self::EIdentity => "JSP-E004",
            Self::EFieldState => "JSP-E005",
            Self::ESemantic => "JSP-E006",
        }
    }
}

impl fmt::Display for JspCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A coded JSP/1 parse error with a safe, payload-free detail string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspError {
    code: JspCode,
    detail: String,
}

impl JspError {
    /// Construct an error from a code and a safe detail string.
    #[must_use]
    pub fn new(code: JspCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// `JSP-E001` closed-shape error.
    #[must_use]
    pub fn closed_shape(detail: impl Into<String>) -> Self {
        Self::new(JspCode::EClosedShape, detail)
    }

    /// `JSP-E002` bound error.
    #[must_use]
    pub fn bound(detail: impl Into<String>) -> Self {
        Self::new(JspCode::EBound, detail)
    }

    /// `JSP-E003` unsupported-version/kind error.
    #[must_use]
    pub fn unsupported_version(detail: impl Into<String>) -> Self {
        Self::new(JspCode::EUnsupportedVersion, detail)
    }

    /// `JSP-E004` identity error.
    #[must_use]
    pub fn identity(detail: impl Into<String>) -> Self {
        Self::new(JspCode::EIdentity, detail)
    }

    /// `JSP-E005` field-state error.
    #[must_use]
    pub fn field_state(detail: impl Into<String>) -> Self {
        Self::new(JspCode::EFieldState, detail)
    }

    /// `JSP-E006` semantic error.
    #[must_use]
    pub fn semantic(detail: impl Into<String>) -> Self {
        Self::new(JspCode::ESemantic, detail)
    }

    /// The stable error code.
    #[must_use]
    pub const fn code(&self) -> JspCode {
        self.code
    }

    /// The safe, payload-free detail string.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for JspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for JspError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_stable_strings() {
        assert_eq!(JspCode::EClosedShape.as_str(), "JSP-E001");
        assert_eq!(JspCode::EBound.as_str(), "JSP-E002");
        assert_eq!(JspCode::EUnsupportedVersion.as_str(), "JSP-E003");
        assert_eq!(JspCode::EIdentity.as_str(), "JSP-E004");
        assert_eq!(JspCode::EFieldState.as_str(), "JSP-E005");
        assert_eq!(JspCode::ESemantic.as_str(), "JSP-E006");
    }

    #[test]
    fn constructors_map_to_correct_codes() {
        assert_eq!(JspError::closed_shape("x").code(), JspCode::EClosedShape);
        assert_eq!(JspError::bound("x").code(), JspCode::EBound);
        assert_eq!(
            JspError::unsupported_version("x").code(),
            JspCode::EUnsupportedVersion
        );
        assert_eq!(JspError::identity("x").code(), JspCode::EIdentity);
        assert_eq!(JspError::field_state("x").code(), JspCode::EFieldState);
        assert_eq!(JspError::semantic("x").code(), JspCode::ESemantic);
    }

    #[test]
    fn display_includes_code_and_detail() {
        let error = JspError::bound("snapshot.agent_id: exceeds maximum length");
        assert_eq!(
            error.to_string(),
            "JSP-E002: snapshot.agent_id: exceeds maximum length"
        );
    }
}
