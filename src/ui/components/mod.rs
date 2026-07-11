//! Reusable UI components.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-010

/// Horizontal divider rendered between header/body sections of overlays and
/// detail panes. Shared so the visual width stays consistent across the merge
/// chooser, agent chooser, delete-confirm overlay, and detail panes.
pub(crate) const SEPARATOR_LINE: &str = "─────────────────────────────────────────";

mod agent_chooser;
mod agent_list;
/// Generic bordered, header + scrollable + optional-composer detail pane.
/// Domain layers (`issue_detail`, `pr_detail`) project into [`DetailPaneProps`]
/// and delegate rendering through [`detail_pane_element`]. The shared header-row
/// helpers (`header_highlight`, `header_row`) live here so both detail panes
/// share one source of truth.
pub(crate) mod detail_pane;
/// Generic bordered filter bar with labeled `[value]` fields and action hints.
/// Domain layers (`filter_controls` for Issues, `pr_filter_controls` for PRs)
/// project into [`FilterBarProps`] and delegate rendering through
/// [`filter_bar_element`]. The active-field inverted-color logic lives here
/// because it needs iocraft `Color`/`ResolvedColors`; the projections stay
/// iocraft-free.
pub(crate) mod filter_bar;
/// Issue filter bar projection. The pure field projection
/// (`issue_filter_fields`) and props builder (`issue_filter_props`) feed the
/// generic [`filter_bar::FilterBar`] via `filter_bar_element`.
///
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-008
mod filter_controls;
/// @requirement issue #182
mod issue_delete_confirm;
/// Issue detail pane projection. The pure header projection
/// (`issue_detail_header_view`) is reused by the selection content provider so
/// copied text matches the rendered rows; rendering is delegated to the generic
/// [`super::detail_pane::DetailPane`] via `issue_detail_props` +
/// `detail_pane_element`.
pub(crate) mod issue_detail;
/// Issue list pane projection + component (projection is reused by the
/// selection content provider so copied text matches the rendered rows).
pub(crate) mod issue_list;
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-012
pub(crate) mod keybind_bar;
/// @requirement REQ-PR-009
mod merge_chooser;
/// PR detail pane projection. The pure header projection
/// (`pr_detail_header_view`) is reused by the selection content provider;
/// rendering is delegated to the generic [`super::detail_pane::DetailPane`] via
/// `pr_detail_props` + `detail_pane_element`.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub(crate) mod pr_detail;
/// PR filter bar projection. The pure field projection
/// (`pr_filter_field_views`) and props builder (`pr_filter_props`) feed the
/// generic [`filter_bar::FilterBar`] via `filter_bar_element`.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
pub(crate) mod pr_filter_controls;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub(crate) mod pr_list;
mod preview;
mod scrollable_text;
mod selectable_line;
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
pub use agent_list::{AgentListSelection, agent_list_props};
pub use detail_pane::{
    DetailComposerProps, DetailHeaderColor, DetailHeaderRow, DetailPane, DetailPaneProps,
    composer_from_inline_state, detail_pane_element, header_highlight, header_row,
};
pub use filter_bar::{FilterBar, FilterBarProps, FilterFieldView, filter_bar_element};
pub use filter_controls::{issue_filter_action_hints, issue_filter_fields, issue_filter_props};
pub use issue_delete_confirm::{IssueDeleteConfirmOverlay, IssueDeleteConfirmProps};
pub use issue_detail::{IssueDetailProjectionInputs, issue_detail_props};
pub use issue_list::{
    IssueListLayout, IssueListWindow, issue_list_props, issue_list_status_message,
};
pub use keybind_bar::{KeybindBar, KeybindBarProps};
/// @requirement REQ-PR-009
pub use merge_chooser::{MergeChooser, MergeChooserProps};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub use pr_detail::{PrDetailProjectionInputs, pr_detail_props};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
pub use pr_filter_controls::{pr_filter_action_hints, pr_filter_field_views, pr_filter_props};
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
pub use pr_list::{PrListLayout, PrListWindow, pr_list_props, pr_list_status_message};
pub use preview::{Preview, PreviewProps};
pub use scrollable_text::{ScrollableText, ScrollableTextProps};
pub use selectable_line::selectable_line;
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

#[cfg(test)]
#[path = "detail_pane_render_tests.rs"]
mod detail_pane_render_tests;

#[cfg(test)]
#[path = "filter_bar_render_tests.rs"]
mod filter_bar_render_tests;
