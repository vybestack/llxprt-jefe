//! Full-screen layouts that compose components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

mod actions;
mod errors;
mod issues;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
mod pull_requests;
/// The Settings screen (issue #387).
mod settings;
mod workflow_dispatch;

pub use actions::{ActionsScreen, ActionsScreenProps};
pub use errors::{ErrorsScreen, ErrorsScreenProps};
pub use issues::{IssuesScreen, IssuesScreenProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
/// @requirement REQ-PR-NFR-003
pub use pull_requests::{PullRequestsScreen, PullRequestsScreenProps};
pub use settings::{SettingsScreen, SettingsScreenProps};
pub use workflow_dispatch::{WorkflowDispatchForm, WorkflowDispatchFormProps};
