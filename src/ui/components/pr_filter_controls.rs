//! PR filter controls component.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-008

use iocraft::prelude::*;

use crate::domain::{ChecksFilter, PrFilter, PrFilterState, ReviewDecisionFilter};
use crate::theme::{ResolvedColors, ThemeColors};

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
    let idx = props.active_field_index;

    let state_label = match props.draft_filter.state {
        None | Some(PrFilterState::Open) => "open",
        Some(PrFilterState::Closed) => "closed",
        Some(PrFilterState::Merged) => "merged",
        Some(PrFilterState::All) => "all",
    };

    let draft_label = match props.draft_filter.is_draft {
        None => "any",
        Some(true) => "drafts-only",
        Some(false) => "ready-only",
    };

    let review_label = match props.draft_filter.review_decision {
        ReviewDecisionFilter::Any => "any",
        ReviewDecisionFilter::Approved => "approved",
        ReviewDecisionFilter::ChangesRequested => "changes-requested",
        ReviewDecisionFilter::ReviewRequired => "review-required",
        ReviewDecisionFilter::None => "none",
    };

    let checks_label = match props.draft_filter.checks_status {
        ChecksFilter::Any => "any",
        ChecksFilter::Success => "success",
        ChecksFilter::Failing => "failing",
        ChecksFilter::Pending => "pending",
    };

    let author_val = if props.draft_filter.author.is_empty() {
        "any".to_string()
    } else {
        props.draft_filter.author.clone()
    };

    let assignee_val = if props.draft_filter.assignee.is_empty() {
        "any".to_string()
    } else {
        props.draft_filter.assignee.clone()
    };

    let reviewer_val = if props.draft_filter.reviewer.is_empty() {
        "any".to_string()
    } else {
        props.draft_filter.reviewer.clone()
    };

    let labels_val = if props.draft_labels_text.is_empty() {
        "any".to_string()
    } else {
        props.draft_labels_text.clone()
    };

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
                Text(content: "state:", color: label_color(idx == 0))
                Box(background_color: val_bg(idx == 0)) {
                    Text(content: format!("[{state_label}]"), color: val_color(idx == 0))
                }
                Text(content: "  draft:", color: label_color(idx == 1))
                Box(background_color: val_bg(idx == 1)) {
                    Text(content: format!("[{draft_label}]"), color: val_color(idx == 1))
                }
                Text(content: "  review:", color: label_color(idx == 2))
                Box(background_color: val_bg(idx == 2)) {
                    Text(content: format!("[{review_label}]"), color: val_color(idx == 2))
                }
                Text(content: "  checks:", color: label_color(idx == 3))
                Box(background_color: val_bg(idx == 3)) {
                    Text(content: format!("[{checks_label}]"), color: val_color(idx == 3))
                }
            }
            // Filter values row 2: author, assignee, reviewer, labels
            Box(height: 1u32) {
                Text(content: "       ", color: rc.dim)
                Text(content: "author:", color: label_color(idx == 4))
                Box(background_color: val_bg(idx == 4)) {
                    Text(content: format!("[{author_val}]"), color: val_color(idx == 4))
                }
                Text(content: "  assignee:", color: label_color(idx == 5))
                Box(background_color: val_bg(idx == 5)) {
                    Text(content: format!("[{assignee_val}]"), color: val_color(idx == 5))
                }
                Text(content: "  reviewer:", color: label_color(idx == 6))
                Box(background_color: val_bg(idx == 6)) {
                    Text(content: format!("[{reviewer_val}]"), color: val_color(idx == 6))
                }
                Text(content: "  labels:", color: label_color(idx == 7))
                Box(background_color: val_bg(idx == 7)) {
                    Text(content: format!("[{labels_val}]"), color: val_color(idx == 7))
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
