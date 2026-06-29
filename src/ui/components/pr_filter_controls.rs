//! PR filter controls component.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-008

use iocraft::prelude::*;

use crate::domain::{ChecksFilter, PrFilter, PrFilterState, ReviewDecisionFilter};
use crate::theme::{ResolvedColors, ThemeColors};

/// Projected PR filter field exactly as the component renders it (label,
/// display value, and active highlight state). The `#[component]` delegates
/// to a list of these so tests assert the SAME fields the component renders
/// (REQ-PR-008).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
pub struct PrFilterFieldView {
    /// Field label ("state", "draft", ...).
    pub label: &'static str,
    /// Display value WITHOUT brackets (e.g. "open", "any", "approved").
    pub value: String,
    /// Whether this field is the active (highlighted) field.
    pub active: bool,
}

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

/// Pure projection of the eight PR filter fields exactly as the component
/// renders them (labels + display values + active highlighting). Returns
/// exactly 8 entries in field order: state, draft, review, checks, author,
/// assignee, reviewer, labels.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
pub fn pr_filter_field_views(
    filter: &PrFilter,
    draft_labels_text: &str,
    active_index: usize,
) -> Vec<PrFilterFieldView> {
    vec![
        PrFilterFieldView {
            label: "state",
            value: state_filter_value(filter.state).to_string(),
            active: active_index == 0,
        },
        PrFilterFieldView {
            label: "draft",
            value: draft_filter_value(filter.is_draft).to_string(),
            active: active_index == 1,
        },
        PrFilterFieldView {
            label: "review",
            value: review_filter_value(filter.review_decision).to_string(),
            active: active_index == 2,
        },
        PrFilterFieldView {
            label: "checks",
            value: checks_filter_value(filter.checks_status).to_string(),
            active: active_index == 3,
        },
        PrFilterFieldView {
            label: "author",
            value: text_or_any(&filter.author),
            active: active_index == 4,
        },
        PrFilterFieldView {
            label: "assignee",
            value: text_or_any(&filter.assignee),
            active: active_index == 5,
        },
        PrFilterFieldView {
            label: "reviewer",
            value: text_or_any(&filter.reviewer),
            active: active_index == 6,
        },
        PrFilterFieldView {
            label: "labels",
            value: text_or_any(draft_labels_text),
            active: active_index == 7,
        },
    ]
}

/// Props for the PR filter controls pane.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
#[derive(Default, Props)]
pub struct PrFilterControlsProps {
    /// Current draft filter values.
    pub draft_filter: PrFilter,
    /// Whether the controls are visible.
    pub visible: bool,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Index of the currently focused filter field.
    pub active_field_index: usize,
    /// Raw labels text for display during editing.
    pub draft_labels_text: String,
}

/// PR filter controls — compact band showing the eight filter fields and action hints.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 1-12
#[component]
pub fn PrFilterControls(props: &PrFilterControlsProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));

    let fields = pr_filter_field_views(
        &props.draft_filter,
        &props.draft_labels_text,
        props.active_field_index,
    );

    // Active field: inverted colors (bright bg, dark fg). Inactive: normal.
    let val_color = |active: bool| if active { rc.bg } else { rc.fg };
    let val_bg = |active: bool| if active { rc.bright } else { rc.bg };
    let label_color = |active: bool| if active { rc.bright } else { rc.dim };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            border_style: BorderStyle::Round,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            // Filter values row 1: state, draft, review, checks
            Box(height: 1u32) {
                Text(content: "Filter: ", color: rc.dim)
                Text(content: format!("{}:", fields[0].label), color: label_color(fields[0].active))
                Box(background_color: val_bg(fields[0].active)) {
                    Text(content: format!("[{}]", fields[0].value), color: val_color(fields[0].active))
                }
                Text(content: format!("  {}:", fields[1].label), color: label_color(fields[1].active))
                Box(background_color: val_bg(fields[1].active)) {
                    Text(content: format!("[{}]", fields[1].value), color: val_color(fields[1].active))
                }
                Text(content: format!("  {}:", fields[2].label), color: label_color(fields[2].active))
                Box(background_color: val_bg(fields[2].active)) {
                    Text(content: format!("[{}]", fields[2].value), color: val_color(fields[2].active))
                }
                Text(content: format!("  {}:", fields[3].label), color: label_color(fields[3].active))
                Box(background_color: val_bg(fields[3].active)) {
                    Text(content: format!("[{}]", fields[3].value), color: val_color(fields[3].active))
                }
            }
            // Filter values row 2: author, assignee, reviewer, labels
            Box(height: 1u32) {
                Text(content: "       ", color: rc.dim)
                Text(content: format!("{}:", fields[4].label), color: label_color(fields[4].active))
                Box(background_color: val_bg(fields[4].active)) {
                    Text(content: format!("[{}]", fields[4].value), color: val_color(fields[4].active))
                }
                Text(content: format!("  {}:", fields[5].label), color: label_color(fields[5].active))
                Box(background_color: val_bg(fields[5].active)) {
                    Text(content: format!("[{}]", fields[5].value), color: val_color(fields[5].active))
                }
                Text(content: format!("  {}:", fields[6].label), color: label_color(fields[6].active))
                Box(background_color: val_bg(fields[6].active)) {
                    Text(content: format!("[{}]", fields[6].value), color: val_color(fields[6].active))
                }
                Text(content: format!("  {}:", fields[7].label), color: label_color(fields[7].active))
                Box(background_color: val_bg(fields[7].active)) {
                    Text(content: format!("[{}]", fields[7].value), color: val_color(fields[7].active))
                }
            }
            // Actions hint row
            Box(height: 1u32) {
                Text(content: "Tab next  ", color: rc.dim)
                Text(content: "Space cycle  ", color: rc.dim)
                Text(content: "Enter apply  ", color: rc.dim)
                Text(content: "Ctrl-c clear  ", color: rc.dim)
                Text(content: "Esc cancel", color: rc.dim)
            }
        }
    }
}
