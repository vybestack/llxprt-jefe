//! Modal dialogs.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

// In-app device-code auth remediation modal (issue #244).
mod auth;

pub use crate::action_projection::project_help_content_lines_effective as effective_help_content_lines;
pub use auth::{AUTH_MODAL_TITLE, AuthModal, AuthModalProps};
