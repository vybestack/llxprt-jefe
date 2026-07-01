//! Filter controls component.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-008

use iocraft::prelude::*;

use crate::domain::{FILTER_CHOICE_ANY, IssueFilter, IssueFilterState};
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

fn display_any(value: &str) -> String {
    if value.is_empty() {
        FILTER_CHOICE_ANY.to_string()
    } else {
        value.to_string()
    }
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

    let author_val = display_any(&props.draft_filter.author);
    let assignee_val = display_any(&props.draft_filter.assignee);
    let labels_val = display_any(&props.draft_labels_text);
    let type_val = display_any(&props.draft_filter.issue_type);
    let milestone_val = display_any(&props.draft_filter.milestone);
    let module_val = display_any(&props.draft_filter.module);
    let search_val = display_any(&props.draft_filter.query_text);

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
            // Filter values rows
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
            }
            Box(height: 1u32) {
                Text(content: "        ", color: rc.dim)
                Text(content: "type:", color: label_color(idx == 4))
                Box(background_color: val_bg(idx == 4)) {
                    Text(content: format!("[{type_val}]"), color: val_color(idx == 4))
                }
                Text(content: "  milestone:", color: label_color(idx == 5))
                Box(background_color: val_bg(idx == 5)) {
                    Text(content: format!("[{milestone_val}]"), color: val_color(idx == 5))
                }
                Text(content: "  module:", color: label_color(idx == 6))
                Box(background_color: val_bg(idx == 6)) {
                    Text(content: format!("[{module_val}]"), color: val_color(idx == 6))
                }
                Text(content: "  search:", color: label_color(idx == 7))
                Box(background_color: val_bg(idx == 7)) {
                    Text(content: format!("[{search_val}]"), color: val_color(idx == 7))
                }
            }
            // Actions hint row
            Box(height: 1u32) {
                Text(content: "Tab next  ", color: rc.dim)
                Text(content: "←/→ choices  ", color: rc.dim)
                Text(content: "Enter apply  ", color: rc.dim)
                Text(content: "Delete field  ", color: rc.dim)
                Text(content: "Ctrl-L clear all  ", color: rc.dim)
                Text(content: "Esc cancel", color: rc.dim)
            }
        }
    }
}
