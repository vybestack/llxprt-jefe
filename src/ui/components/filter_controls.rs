//! Issue filter bar projection (pure, iocraft-free).
//!
//! Extracts the Issue filter field views, action hints, and full
//! [`FilterBarProps`] for the generic [`super::filter_bar::FilterBar`] to
//! render. This module owns NO iocraft types (`Color`, `Props`,
//! `AnyElement`) — it is a pure projection per the pure-views pattern
//! (see `dev-docs/standards/architecture.md`). The screen calls
//! `filter_bar_element(issue_filter_props(...))`.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-008

use crate::domain::{FILTER_CHOICE_ANY, IssueFilter, IssueFilterState};
use crate::theme::ThemeColors;

use super::filter_bar::{FilterBarProps, FilterFieldView};

/// Row-1 prefix text before the first field (matches the pre-refactor
/// `FilterControls` component exactly).
const ROW_PREFIX: &str = "Filter: ";

/// Row-2+ continuation prefix: 8 spaces (matches the pre-refactor Issues
/// `FilterControls` component exactly — `"        "`).
const CONTINUATION_PREFIX: &str = "        ";

/// Number of fields per row (matches the pre-refactor two-row layout).
const FIELDS_PER_ROW: usize = 4;

// Compile-time invariant: the continuation prefix must align with the row
// prefix so row-2+ fields line up under row-1 fields. Both are 8 chars.
const _: () = assert!(ROW_PREFIX.len() == CONTINUATION_PREFIX.len());

/// Render `value` if non-empty, otherwise the "any" sentinel (used for the
/// text fields: author, assignee, labels, type, milestone, module, search).
fn display_any(value: &str) -> String {
    if value.is_empty() {
        FILTER_CHOICE_ANY.to_string()
    } else {
        value.to_string()
    }
}

/// The display value for the state filter field (without brackets), matching
/// the pre-refactor `state_label` match.
fn state_label(state: Option<IssueFilterState>) -> &'static str {
    match state {
        Some(IssueFilterState::Open) | None => "open",
        Some(IssueFilterState::Closed) => "closed",
        Some(IssueFilterState::All) => "all",
    }
}

/// Pure projection of the eight Issue filter fields (state, author, assignee,
/// labels, type, milestone, module, search) with display values + active
/// highlighting.
///
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-008
#[must_use]
pub fn issue_filter_fields(
    filter: &IssueFilter,
    draft_labels_text: &str,
    active_index: usize,
) -> Vec<FilterFieldView> {
    vec![
        FilterFieldView {
            label: "state".to_string(),
            value: state_label(filter.state).to_string(),
            active: active_index == 0,
        },
        FilterFieldView {
            label: "author".to_string(),
            value: display_any(&filter.author),
            active: active_index == 1,
        },
        FilterFieldView {
            label: "assignee".to_string(),
            value: display_any(&filter.assignee),
            active: active_index == 2,
        },
        FilterFieldView {
            label: "labels".to_string(),
            value: display_any(draft_labels_text),
            active: active_index == 3,
        },
        FilterFieldView {
            label: "type".to_string(),
            value: display_any(&filter.issue_type),
            active: active_index == 4,
        },
        FilterFieldView {
            label: "milestone".to_string(),
            value: display_any(&filter.milestone),
            active: active_index == 5,
        },
        FilterFieldView {
            label: "module".to_string(),
            value: display_any(&filter.module),
            active: active_index == 6,
        },
        FilterFieldView {
            label: "search".to_string(),
            value: display_any(&filter.query_text),
            active: active_index == 7,
        },
    ]
}

/// Action-hint segments for the Issues filter bar (matches the pre-refactor
/// `FilterControls` action-hints row exactly).
///
/// Returns a `&'static [&'static str]` slice to avoid per-render heap
/// allocation — the hints are compile-time constants.
///
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-008
#[must_use]
pub fn issue_filter_action_hints() -> &'static [&'static str] {
    shared_filter_action_hints()
}

/// Shared key hints for the common filter-control workflow.
#[must_use]
pub fn shared_filter_action_hints() -> &'static [&'static str] {
    &[
        "Tab next  ",
        "←/→ cycle  ",
        "Enter apply  ",
        "Delete field  ",
        "Ctrl-L clear all  ",
        "Esc cancel",
    ]
}

/// Build the full [`FilterBarProps`] for the Issues filter bar.
///
/// The screen calls `filter_bar_element(issue_filter_props(...))` to render
/// the generic component. This projection owns the field computation, the
/// row-prefix text, the continuation-prefix alignment, and the action hints.
///
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-008
#[must_use]
pub fn issue_filter_props(
    filter: &IssueFilter,
    draft_labels_text: &str,
    active_index: usize,
    visible: bool,
    colors: ThemeColors,
) -> FilterBarProps {
    FilterBarProps {
        fields: issue_filter_fields(filter, draft_labels_text, active_index),
        visible,
        row_prefix: ROW_PREFIX,
        continuation_prefix: CONTINUATION_PREFIX,
        fields_per_row: FIELDS_PER_ROW,
        action_hints: issue_filter_action_hints(),
        colors,
    }
}

/// Number of fields per row for the Actions filter bar (three fields:
/// workflow, status, pr — all fit on one row).
const ACTIONS_FIELDS_PER_ROW: usize = 3;

/// Pure projection of the three Actions filter fields (workflow, status, pr)
/// with display values + active highlighting, for the generic [`FilterBar`].
///
/// @plan PLAN-20260711-ACTIONS-MODE
#[must_use]
pub fn actions_filter_fields(
    filter: &crate::domain::ActionsFilter,
    active_index: usize,
) -> Vec<FilterFieldView> {
    vec![
        FilterFieldView {
            label: "workflow".to_string(),
            value: display_any(&filter.workflow),
            active: active_index == 0,
        },
        FilterFieldView {
            label: "status".to_string(),
            value: display_any(&filter.status),
            active: active_index == 1,
        },
        FilterFieldView {
            label: "pr".to_string(),
            value: filter
                .pr_number
                .map_or_else(|| display_any(""), |n| format!("#{n}")),
            active: active_index == 2,
        },
    ]
}

/// Action-hint segments for the Actions filter bar (matches the Actions key
/// scheme: Tab next field, Up/Down cycle value, Enter apply, Esc cancel).
#[must_use]
pub fn actions_filter_action_hints() -> &'static [&'static str] {
    shared_filter_action_hints()
}

/// Build the full [`FilterBarProps`] for the Actions filter bar.
///
/// The Actions screen calls `filter_bar_element(actions_filter_props(...))`
/// to render the same generic component the Issues screen uses.
///
/// @plan PLAN-20260711-ACTIONS-MODE
#[must_use]
pub fn actions_filter_props(
    filter: &crate::domain::ActionsFilter,
    active_index: usize,
    visible: bool,
    colors: ThemeColors,
) -> FilterBarProps {
    FilterBarProps {
        fields: actions_filter_fields(filter, active_index),
        visible,
        row_prefix: ROW_PREFIX,
        continuation_prefix: CONTINUATION_PREFIX,
        fields_per_row: ACTIONS_FIELDS_PER_ROW,
        action_hints: actions_filter_action_hints(),
        colors,
    }
}
