//! PR filter bar projection (pure, iocraft-free).
//!
//! Extracts the PR filter field views, action hints, and full
//! [`FilterBarProps`] for the generic [`super::filter_bar::FilterBar`] to
//! render. This module owns NO iocraft types (`Color`, `Props`,
//! `AnyElement`) — it is a pure projection per the pure-views pattern
//! (see `dev-docs/standards/architecture.md`). The screen calls
//! `filter_bar_element(pr_filter_props(...))`.
//!
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-008

use crate::domain::{ChecksFilter, PrFilter, PrFilterState, ReviewDecisionFilter};
use crate::theme::ThemeColors;

use super::filter_bar::{FilterBarProps, FilterFieldView};

/// Row-1 prefix text before the first field (matches the pre-refactor
/// `PrFilterControls` component exactly).
const ROW_PREFIX: &str = "Filter: ";

/// Row-2+ continuation prefix: 7 spaces (matches the pre-refactor
/// `PrFilterControls` component exactly — `"       "`).
const CONTINUATION_PREFIX: &str = "       ";

/// Number of fields per row (matches the pre-refactor two-row layout).
const FIELDS_PER_ROW: usize = 4;

/// The display value for the state filter field (without brackets).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
fn state_filter_value(state: Option<PrFilterState>) -> &'static str {
    match state {
        None | Some(PrFilterState::Open) => "open",
        Some(PrFilterState::Closed) => "closed",
        Some(PrFilterState::Merged) => "merged",
        Some(PrFilterState::All) => "all",
    }
}

/// The display value for the draft filter field (without brackets).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
fn draft_filter_value(is_draft: Option<bool>) -> &'static str {
    match is_draft {
        None => "any",
        Some(true) => "drafts-only",
        Some(false) => "ready-only",
    }
}

/// The display value for the review-decision filter field (without brackets).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
fn review_filter_value(review: ReviewDecisionFilter) -> &'static str {
    match review {
        ReviewDecisionFilter::Any => "any",
        ReviewDecisionFilter::Approved => "approved",
        ReviewDecisionFilter::ChangesRequested => "changes-requested",
        ReviewDecisionFilter::ReviewRequired => "review-required",
        ReviewDecisionFilter::None => "none",
    }
}

/// The display value for the checks filter field (without brackets).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
fn checks_filter_value(checks: ChecksFilter) -> &'static str {
    match checks {
        ChecksFilter::Any => "any",
        ChecksFilter::Success => "success",
        ChecksFilter::Failing => "failing",
        ChecksFilter::Pending => "pending",
    }
}

/// Render `value` if non-empty, otherwise "any" (used for the text fields:
/// author, assignee, reviewer, labels).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
fn text_or_any(value: &str) -> String {
    if value.is_empty() {
        "any".to_string()
    } else {
        value.to_string()
    }
}

/// Pure projection of the eight PR filter fields (state, draft, review,
/// checks, author, assignee, reviewer, labels) with display values + active
/// highlighting.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_filter_field_views(
    filter: &PrFilter,
    draft_labels_text: &str,
    active_index: usize,
) -> Vec<FilterFieldView> {
    vec![
        FilterFieldView {
            label: "state".to_string(),
            value: state_filter_value(filter.state).to_string(),
            active: active_index == 0,
        },
        FilterFieldView {
            label: "draft".to_string(),
            value: draft_filter_value(filter.is_draft).to_string(),
            active: active_index == 1,
        },
        FilterFieldView {
            label: "review".to_string(),
            value: review_filter_value(filter.review_decision).to_string(),
            active: active_index == 2,
        },
        FilterFieldView {
            label: "checks".to_string(),
            value: checks_filter_value(filter.checks_status).to_string(),
            active: active_index == 3,
        },
        FilterFieldView {
            label: "author".to_string(),
            value: text_or_any(&filter.author),
            active: active_index == 4,
        },
        FilterFieldView {
            label: "assignee".to_string(),
            value: text_or_any(&filter.assignee),
            active: active_index == 5,
        },
        FilterFieldView {
            label: "reviewer".to_string(),
            value: text_or_any(&filter.reviewer),
            active: active_index == 6,
        },
        FilterFieldView {
            label: "labels".to_string(),
            value: text_or_any(draft_labels_text),
            active: active_index == 7,
        },
    ]
}

/// Action-hint segments for the PR filter bar (matches the pre-refactor
/// `PrFilterControls` action-hints row exactly).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
#[must_use]
pub fn pr_filter_action_hints() -> Vec<String> {
    vec![
        "Tab next  ".to_string(),
        "Space cycle  ".to_string(),
        "Enter apply  ".to_string(),
        "Ctrl-c clear  ".to_string(),
        "Esc cancel".to_string(),
    ]
}

/// Build the full [`FilterBarProps`] for the PR filter bar.
///
/// The screen calls `filter_bar_element(pr_filter_props(...))` to render
/// the generic component. This projection owns the field computation, the
/// row-prefix text, the continuation-prefix alignment, and the action hints.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
#[must_use]
pub fn pr_filter_props(
    filter: &PrFilter,
    draft_labels_text: &str,
    active_index: usize,
    visible: bool,
    colors: ThemeColors,
) -> FilterBarProps {
    FilterBarProps {
        fields: pr_filter_field_views(filter, draft_labels_text, active_index),
        visible,
        row_prefix: ROW_PREFIX.to_string(),
        continuation_prefix: CONTINUATION_PREFIX.to_string(),
        fields_per_row: FIELDS_PER_ROW,
        action_hints: pr_filter_action_hints(),
        colors,
    }
}
