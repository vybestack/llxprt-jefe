//! Modal dialogs.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

mod confirm;
mod help;

// In-app device-code auth remediation modal (issue #244).
mod auth;

pub use auth::{AUTH_MODAL_TITLE, AuthModal, AuthModalProps};
pub use confirm::{ConfirmModal, ConfirmModalProps};
pub use help::{
    HELP_CHROME_ROWS, HELP_MODAL_WIDTH, HELP_TITLE, HelpModal, HelpModalProps, help_content_lines,
    help_viewport_rows,
};

pub(crate) use confirm::confirm_button_row;
