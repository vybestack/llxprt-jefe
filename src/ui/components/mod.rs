//! Reusable UI components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-010

mod agent_chooser;
mod agent_list;
mod filter_controls;
/// Issue detail pane projection + component. The projection
/// (`issue_detail_header_view`, `header_highlight`, `header_row`) is reused by
/// the PR detail component and the selection content provider so copied text
/// matches the rendered rows.
pub(crate) mod issue_detail;
/// Issue list pane projection + component (projection is reused by the
/// selection content provider so copied text matches the rendered rows).
pub(crate) mod issue_list;
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-012
pub(crate) mod keybind_bar;
/// @requirement REQ-PR-009
mod merge_chooser;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub(crate) mod pr_detail;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
pub(crate) mod pr_filter_controls;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub(crate) mod pr_list;
mod preview;
mod scrollable_text;
/// Generic bordered, scrollable, selectable list used by the Issue, PR, and
/// Agent list panes. Domain layers project into [`SelectableRow`]s; this
/// component owns the iocraft rendering once.
pub(crate) mod selectable_list;
mod sidebar;
mod status_bar;
mod terminal_view;
/// Fixed-size multiline text-box component with an inline caret.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
mod text_box;

pub use agent_chooser::{AgentChooser, AgentChooserProps};
pub use agent_list::agent_list_props;
pub use filter_controls::{FilterControls, FilterControlsProps};
pub use issue_detail::{IssueDetailView, IssueDetailViewProps};
pub use issue_list::{
    IssueListLayout, IssueListWindow, issue_list_props, issue_list_status_message,
};
pub use keybind_bar::{KeybindBar, KeybindBarProps};
/// @requirement REQ-PR-009
pub use merge_chooser::{MergeChooser, MergeChooserProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub use pr_detail::{PrDetailView, PrDetailViewProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
pub use pr_filter_controls::{PrFilterControls, PrFilterControlsProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub use pr_list::{PrListLayout, PrListWindow, pr_list_props, pr_list_status_message};
pub use preview::{Preview, PreviewProps};
pub use scrollable_text::{ScrollableText, ScrollableTextProps};
pub use selectable_list::{
    ListBorder, SelectableList, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle,
    SpanColor, SpanRole, selectable_list_element,
};
pub use sidebar::{Sidebar, SidebarProps};
pub use status_bar::{StatusBar, StatusBarProps};
pub use terminal_view::{TerminalView, TerminalViewProps, terminal_empty_message};
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
pub use text_box::{TextBox, TextBoxProps};

#[cfg(test)]
#[path = "pr_render_tests.rs"]
mod pr_render_tests;

/// @plan PLAN-20260624-PR-MODE.P14
#[cfg(test)]
#[path = "issue_detail_render_tests.rs"]
mod issue_detail_render_tests;

/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 1-12
#[cfg(test)]
#[path = "pr_detail_render_tests.rs"]
mod pr_detail_render_tests;

#[cfg(test)]
#[path = "pr_render_screen_tests.rs"]
mod pr_render_screen_tests;
