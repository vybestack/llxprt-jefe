//! Full-screen layouts that compose components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

mod actions;
mod errors;
mod generated_agent;
mod issues;
mod new_agent;
mod new_repository;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
mod pull_requests;
/// The Settings screen (issue #387).
mod settings;
mod split;
mod terminal_manager;
mod workflow_dispatch;

pub use actions::{ActionsScreen, ActionsScreenProps};
pub use errors::{ErrorsScreen, ErrorsScreenProps};
pub use generated_agent::{GeneratedAgentForm, GeneratedAgentFormProps};
pub use issues::{IssuesScreen, IssuesScreenProps};
pub use new_agent::{NewAgentForm, NewAgentFormProps};
pub use new_repository::{NewRepositoryForm, NewRepositoryFormProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
/// @requirement REQ-PR-NFR-003
pub use pull_requests::{PullRequestsScreen, PullRequestsScreenProps};
pub use settings::{SettingsScreen, SettingsScreenProps};
pub use split::{SplitScreen, SplitScreenProps};
pub use terminal_manager::{TerminalManagerScreen, TerminalManagerScreenProps};
pub use workflow_dispatch::{WorkflowDispatchForm, WorkflowDispatchFormProps};
