//! The panel types a screen definition may name (issue #385).
//!
//! Panel types are an immutable, compiled-in registry rather than free text.
//! A panel type selects which renderer draws a rectangle, so a definition that
//! could invent one would be asking for code that does not exist; a definition
//! that names one that does exist is composing panels the program already ships.
//!
//! `pty-terminal` is deliberately absent. A PTY panel runs a real process
//! attached to a real terminal, and a screen definition is a file a user can be
//! handed. Composing existing views into a new screen is a layout decision;
//! obtaining a shell is not, so the two are separated at the registry rather
//! than behind a flag.

use super::ids::{IdError, PanelTypeId, check_identifier};
use super::screens::PTY_PANEL_TYPE;

/// Every panel type a screen definition may name, in registry order.
pub const DEFINABLE_PANEL_TYPES: [&str; 16] = [
    "action-detail",
    "action-list",
    "agent-list",
    "agent-preview",
    "error-detail",
    "error-list",
    "filter-band",
    "issue-detail",
    "issue-list",
    "notice-band",
    "pr-detail",
    "pr-list",
    "repository-list",
    "search-input",
    "shell-list",
    "shell-preview",
];

/// Why a declared panel type was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelTypeError {
    /// No compiled renderer answers to this name.
    Unknown {
        /// The declared name.
        declared: String,
    },
    /// The name is a real panel type that definitions may not request.
    Forbidden {
        /// The declared name.
        declared: String,
    },
    /// The name is not a well-formed identifier.
    Malformed {
        /// The declared name.
        declared: String,
        /// The violated rule.
        reason: IdError,
    },
}

impl std::fmt::Display for PanelTypeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown { declared } => write!(
                formatter,
                "panel type {declared:?} has no compiled renderer (available: {})",
                DEFINABLE_PANEL_TYPES.join(", ")
            ),
            Self::Forbidden { declared } => write!(
                formatter,
                "panel type {declared:?} may not be requested by a screen definition"
            ),
            Self::Malformed { declared, reason } => {
                write!(formatter, "panel type {declared:?}: {reason}")
            }
        }
    }
}

impl std::error::Error for PanelTypeError {}

/// Resolve a declared panel type against the immutable registry.
///
/// The registry entry is found before anything is interned, so the identifier
/// that comes back is the compiled `'static` literal and an unknown or
/// forbidden name never consumes a slot in the interning table.
///
/// # Errors
///
/// Returns why the name was refused: malformed, unknown, or forbidden.
pub fn resolve_panel_type(declared: &str) -> Result<PanelTypeId, PanelTypeError> {
    check_identifier(declared).map_err(|reason| PanelTypeError::Malformed {
        declared: declared.to_owned(),
        reason,
    })?;
    if declared == PTY_PANEL_TYPE {
        return Err(PanelTypeError::Forbidden {
            declared: declared.to_owned(),
        });
    }
    let compiled = DEFINABLE_PANEL_TYPES
        .iter()
        .copied()
        .find(|candidate| *candidate == declared)
        .ok_or_else(|| PanelTypeError::Unknown {
            declared: declared.to_owned(),
        })?;
    PanelTypeId::parse(compiled).map_err(|reason| PanelTypeError::Malformed {
        declared: declared.to_owned(),
        reason,
    })
}
