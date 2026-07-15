//! Full-screen layouts that compose components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

mod actions;
mod dashboard;
mod errors;
mod issues;
mod new_agent;
mod new_repository;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
mod pull_requests;
mod split;
mod theme_picker;
mod workflow_dispatch;

pub use actions::{ActionsScreen, ActionsScreenProps};
pub use dashboard::{Dashboard, DashboardProps};
pub use errors::{ErrorsScreen, ErrorsScreenProps};
pub use issues::{IssuesScreen, IssuesScreenProps};
pub use new_agent::{NewAgentForm, NewAgentFormProps};
pub use new_repository::{NewRepositoryForm, NewRepositoryFormProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-001
/// @requirement REQ-PR-NFR-003
pub use pull_requests::{PullRequestsScreen, PullRequestsScreenProps};
pub use split::{SplitScreen, SplitScreenProps};
pub use theme_picker::{ThemePickerScreen, ThemePickerScreenProps};
pub use workflow_dispatch::{WorkflowDispatchForm, WorkflowDispatchFormProps};
