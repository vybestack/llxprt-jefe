//! Filter controls component.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-008

use iocraft::prelude::*;

use crate::domain::{IssueFilter, IssueFilterState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the filter controls pane.
#[derive(Default, Props)]
pub struct FilterControlsProps {
    /// Current draft filter values.
    pub draft_filter: IssueFilter,
    /// Whether the controls are visible.
    pub visible: bool,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Index of the currently focused filter field.
    pub active_field_index: usize,
    /// Raw labels text for display during editing.
    pub draft_labels_text: String,
}

/// Filter controls — compact horizontal band showing current filter values and action hints.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-008
#[component]
pub fn FilterControls(props: &FilterControlsProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let idx = props.active_field_index;

    let state_label = match props.draft_filter.state {
        Some(IssueFilterState::Open) | None => "open",
        Some(IssueFilterState::Closed) => "closed",
        Some(IssueFilterState::All) => "all",
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

    let labels_val = if props.draft_labels_text.is_empty() {
        "any".to_string()
    } else {
        props.draft_labels_text.clone()
    };

    let search_val = if props.draft_filter.query_text.is_empty() {
        "any".to_string()
    } else {
        props.draft_filter.query_text.clone()
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
            // Filter values row
            Box(height: 1u32) {
                Text(content: "Filter: ", color: rc.dim)
                Text(content: "state:", color: label_color(idx == 0))
                Box(background_color: val_bg(idx == 0)) {
                    Text(content: format!("[{state_label}]"), color: val_color(idx == 0))
                }
                Text(content: "  author:", color: label_color(idx == 1))
                Box(background_color: val_bg(idx == 1)) {
                    Text(content: format!("[{author_val}]"), color: val_color(idx == 1))
                }
                Text(content: "  assignee:", color: label_color(idx == 2))
                Box(background_color: val_bg(idx == 2)) {
                    Text(content: format!("[{assignee_val}]"), color: val_color(idx == 2))
                }
                Text(content: "  labels:", color: label_color(idx == 3))
                Box(background_color: val_bg(idx == 3)) {
                    Text(content: format!("[{labels_val}]"), color: val_color(idx == 3))
                }
                Text(content: "  search:", color: label_color(idx == 4))
                Box(background_color: val_bg(idx == 4)) {
                    Text(content: format!("[{search_val}]"), color: val_color(idx == 4))
                }
            }
            // Actions hint row
            Box(height: 1u32) {
                Text(content: "Tab next  ", color: rc.dim)
                Text(content: "Enter apply  ", color: rc.dim)
                Text(content: "Ctrl-c clear  ", color: rc.dim)
                Text(content: "Esc cancel", color: rc.dim)
            }
        }
    }
}
