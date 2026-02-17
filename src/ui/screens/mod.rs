//! Full-screen layouts that compose components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

mod dashboard;
mod new_agent;
mod new_repository;
mod split;

pub use dashboard::{Dashboard, DashboardProps};
pub use new_agent::{NewAgentForm, NewAgentFormProps};
pub use new_repository::{NewRepositoryForm, NewRepositoryFormProps};
pub use split::{SplitScreen, SplitScreenProps};
