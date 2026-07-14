//! Error type for GitHub CLI operations.

/// Error types for GitHub CLI operations.
///
/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-013
/// @pseudocode component-002 lines 84-91
#[derive(Debug)]
pub enum GhError {
    NotAuthenticated(String),
    NotInstalled,
    ToolResolution(String),
    RateLimited,
    AccessDenied(String),
    ApiError(String),
    ParseError(String),
    NetworkError(String),
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAuthenticated(msg) => write!(f, "Not authenticated: {msg}"),
            Self::NotInstalled => write!(f, "GitHub CLI (gh) is not installed"),
            Self::ToolResolution(msg) => write!(f, "GitHub CLI resolution failed: {msg}"),
            Self::RateLimited => write!(f, "GitHub API rate limit exceeded"),
            Self::AccessDenied(msg) => write!(f, "Access denied: {msg}"),
            Self::ApiError(msg) => write!(f, "API error: {msg}"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
        }
    }
}

impl std::error::Error for GhError {}
