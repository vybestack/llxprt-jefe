//! Modal dialogs.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

mod confirm;
mod help;
mod provider;

// In-app device-code auth remediation modal (issue #244).
mod auth;

pub use crate::action_projection::project_help_content_lines_effective as effective_help_content_lines;
pub use auth::{AUTH_MODAL_TITLE, AuthModal, AuthModalProps};
pub use confirm::{ConfirmModal, ConfirmModalProps};
#[cfg(test)]
pub use help::help_content_lines;
pub use help::{
    HELP_CHROME_ROWS, HELP_MODAL_WIDTH, HELP_TITLE, HelpModal, HelpModalProps, help_max_scroll,
    help_viewport_rows,
};
pub use provider::{ProviderModal, ProviderModalProps, provider_modal_lines};

pub(crate) use confirm::confirm_button_row;
