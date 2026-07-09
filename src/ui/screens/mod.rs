//! Full-screen layouts that compose components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

mod dashboard;
mod issues;
mod new_agent;
mod new_repository;
mod split;
mod theme_picker;

pub use dashboard::{Dashboard, DashboardProps};
pub use issues::{IssuesScreen, IssuesScreenProps};
pub use new_agent::{NewAgentForm, NewAgentFormProps};
pub use new_repository::{NewRepositoryForm, NewRepositoryFormProps};
pub use split::{SplitScreen, SplitScreenProps};
pub use theme_picker::{ThemePickerScreen, ThemePickerScreenProps};
