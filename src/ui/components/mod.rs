//! Reusable UI components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-010

mod agent_chooser;
mod agent_list;
mod filter_controls;
mod issue_detail;
mod issue_list;
mod keybind_bar;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
mod pr_detail;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
mod pr_filter_controls;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
mod pr_list;
mod preview;
mod scrollable_text;
mod sidebar;
mod status_bar;
mod terminal_view;

pub use agent_chooser::{AgentChooser, AgentChooserProps};
pub use agent_list::{AgentList, AgentListProps};
pub use filter_controls::{FilterControls, FilterControlsProps};
pub use issue_detail::{IssueDetailView, IssueDetailViewProps};
pub use issue_list::{IssueList, IssueListLayout, IssueListProps};
pub use keybind_bar::{KeybindBar, KeybindBarProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub use pr_detail::{PrDetailView, PrDetailViewProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
pub use pr_filter_controls::{PrFilterControls, PrFilterControlsProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub use pr_list::{PrList, PrListLayout, PrListProps};
pub use preview::{Preview, PreviewProps};
pub use scrollable_text::{ScrollableText, ScrollableTextProps};
pub use sidebar::{Sidebar, SidebarProps};
pub use status_bar::{StatusBar, StatusBarProps};
pub use terminal_view::{TerminalView, TerminalViewProps};
